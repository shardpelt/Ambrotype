//! Demosaicing for Fujifilm X-Trans sensors: Frank Markesteijn's algorithm.
//!
//! Ported from darktable's `src/iop/demosaicing/xtrans.c`
//! (`xtrans_markesteijn_interpolate`, GPL-3.0-or-later, Copyright (C)
//! 2010-2026 darktable developers), which adapts dcraw 9.20's
//! `xtrans_interpolate` by Dave Coffin and Frank Markesteijn. The structure
//! and constants follow darktable's code; buffers are indexed instead of
//! pointer-walked, and tiles are processed in parallel bands of rows.
//!
//! In short: green is interpolated in four directions (horizontal, vertical,
//! two diagonals), clamped to the range of the surrounding greens; red and
//! blue follow from colour differences; then, per pixel, the directions whose
//! result is smoothest in a perceptual space (YPbPr) are averaged.

use rayon::prelude::*;

/// Tile size (pixels); tiles overlap by `PAD` on each side.
const TS: usize = 256;
const TSI: isize = TS as isize;
/// Border each tile needs around the part it outputs (1 pass).
const PAD: i32 = 12;
/// Directions: horizontal, vertical and the two diagonals (1 pass).
const NDIR: usize = 4;
const PAD_G1_G3: i32 = 3;
const PAD_G_INTERP: i32 = 3;
/// Pixels at the image edge filled by plain interpolation afterwards.
const EDGE: usize = 6;

/// The 6x6 colour pattern: 0 red, 1 green, 2 blue, for `(row, col)`
/// relative to the image's top-left pixel.
pub type Pattern = [[u8; 6]; 6];

#[inline]
fn fc(row: i32, col: i32, p: &Pattern) -> usize {
    p[((row + 600) % 6) as usize][((col + 600) % 6) as usize] as usize
}

#[inline]
fn sqr(v: f32) -> f32 {
    v * v
}

/// darktable's CLAMPS: `v` limited to `lo..=hi`, `lo` winning when they cross.
#[inline]
fn clamps(v: f32, lo: f32, hi: f32) -> f32 {
    if v > lo {
        if v < hi { v } else { hi }
    } else {
        lo
    }
}

/// Demosaic one X-Trans mosaic (`width` x `height` samples, row-major) into
/// interleaved RGB.
pub fn demosaic(input: &[f32], width: usize, height: usize, pattern: &Pattern) -> Vec<f32> {
    assert_eq!(input.len(), width * height);
    let mut out = vec![0.0f32; width * height * 3];
    if width < 2 * EDGE || height < 2 * EDGE {
        border(&mut out, input, width, height, pattern, width.max(height));
        return out;
    }
    let hexes = Hexes::new(pattern);
    let band = TS - 2 * PAD as usize;
    out.par_chunks_mut(band * width * 3)
        .enumerate()
        .for_each_init(Tile::new, |tile, (k, rows)| {
            let top = (k * band) as i32 - PAD;
            let mut left = -PAD;
            while left < width as i32 - PAD {
                tile.run(input, width, height, pattern, &hexes, top, left, rows);
                left += band as i32;
            }
        });
    border(&mut out, input, width, height, pattern, EDGE);
    out
}

/// The green "hexagon" around each pixel of a 3x3 cell, as offsets within a
/// tile, plus the position of the solitary green pixel.
struct Hexes {
    all: [[[isize; 8]; 3]; 3],
    sgrow: i32,
    sgcol: i32,
}

impl Hexes {
    fn new(p: &Pattern) -> Self {
        const ORTH: [i32; 12] = [1, 0, 0, 1, -1, 0, 0, -1, 1, 0, 0, 1];
        const PATT: [[i32; 16]; 2] = [
            [0, 1, 0, -1, 2, 0, -1, 0, 1, 1, 1, -1, 0, 0, 0, 0],
            [0, 1, 0, -2, 1, 0, -2, 0, 1, 1, -2, -2, 1, -1, -1, 1],
        ];
        let mut all = [[[0isize; 8]; 3]; 3];
        let (mut sgrow, mut sgcol) = (0, 0);
        for row in 0..3i32 {
            for col in 0..3i32 {
                let g = usize::from(fc(row, col, p) == 1);
                let mut ng = 0;
                for d in (0..10).step_by(2) {
                    if fc(row + ORTH[d], col + ORTH[d + 2], p) == 1 {
                        ng = 0;
                    } else {
                        ng += 1;
                    }
                    // Four non-green neighbours: the solitary green pixel.
                    if ng == 4 {
                        sgrow = row;
                        sgcol = col;
                    }
                    if ng == g + 1 {
                        for c in 0..8 {
                            let v = ORTH[d] * PATT[g][c * 2] + ORTH[d + 1] * PATT[g][c * 2 + 1];
                            let h = ORTH[d + 2] * PATT[g][c * 2] + ORTH[d + 3] * PATT[g][c * 2 + 1];
                            all[row as usize][col as usize][c ^ ((g * 2) & d)] =
                                h as isize + v as isize * TSI;
                        }
                    }
                }
            }
        }
        Self { all, sgrow, sgcol }
    }

    #[inline]
    fn at(&self, row: i32, col: i32) -> &[isize; 8] {
        &self.all[((row + 600) % 3) as usize][((col + 600) % 3) as usize]
    }
}

/// Per-thread working buffers for one tile.
struct Tile {
    /// NDIR tiles of RGB, one per direction.
    rgb: Vec<f32>,
    /// Y, Pb, Pr planes.
    yuv: Vec<f32>,
    /// NDIR planes of derivatives.
    drv: Vec<f32>,
    gmin: Vec<f32>,
    gmax: Vec<f32>,
    homo: Vec<u8>,
    homosum: Vec<u8>,
}

/// Index of pixel (`d`, `row`, `col`) in the direction tiles.
#[inline]
fn px(d: usize, row: i32, col: i32) -> isize {
    (d as isize * TSI + row as isize) * TSI + col as isize
}

impl Tile {
    fn new() -> Self {
        Self {
            rgb: vec![0.0; NDIR * TS * TS * 3],
            yuv: vec![0.0; 3 * TS * TS],
            drv: vec![0.0; NDIR * TS * TS],
            gmin: vec![0.0; TS * TS],
            gmax: vec![0.0; TS * TS],
            homo: vec![0; NDIR * TS * TS],
            homosum: vec![0; NDIR * TS * TS],
        }
    }

    #[inline]
    fn g(&self, p: isize, c: usize) -> f32 {
        self.rgb[(p * 3) as usize + c]
    }

    #[inline]
    fn set(&mut self, p: isize, c: usize, v: f32) {
        self.rgb[(p * 3) as usize + c] = v;
    }

    /// Demosaic the tile whose top-left image position is (`top`, `left`),
    /// writing its inner part into `rows` (the output rows of this band).
    #[allow(clippy::too_many_arguments)]
    fn run(
        &mut self,
        input: &[f32],
        width: usize,
        height: usize,
        pat: &Pattern,
        hexes: &Hexes,
        top: i32,
        left: i32,
        rows: &mut [f32],
    ) {
        let (w, h) = (width as i32, height as i32);
        let (sgrow, sgcol) = (hexes.sgrow, hexes.sgcol);
        let mrow = (top + TS as i32).min(h + PAD);
        let mcol = (left + TS as i32).min(w + PAD);
        let at = |row: i32, col: i32| input[(row * w + col) as usize].max(0.0);

        // Copy the tile in; beyond the image edge, mirror (or interpolate
        // when the mirrored pixel is another colour).
        let translate = |n: i32, size: i32| if n >= size { 2 * size - n - 2 } else { n.abs() };
        for row in top..mrow {
            for col in left..mcol {
                let p = px(0, row - top, col - left);
                let f = fc(row, col, pat);
                let v = if col >= 0 && row >= 0 && col < w && row < h {
                    at(row, col)
                } else {
                    let (cy, cx) = (translate(row, h), translate(col, w));
                    if f == fc(cy, cx, pat) {
                        at(cy, cx)
                    } else {
                        let (mut sum, mut count) = (0.0, 0);
                        for y in row - 1..=row + 1 {
                            for x in col - 1..=col + 1 {
                                let (yy, xx) = (translate(y, h), translate(x, w));
                                if fc(yy, xx, pat) == f {
                                    sum += at(yy, xx);
                                    count += 1;
                                }
                            }
                        }
                        sum / count as f32
                    }
                };
                for c in 0..3 {
                    self.set(p, c, if c == f { v } else { 0.0 });
                }
            }
        }
        // Every direction starts from the same mosaic.
        let one = TS * TS * 3;
        let (first, rest) = self.rgb.split_at_mut(one);
        for chunk in rest.chunks_mut(one) {
            chunk.copy_from_slice(first);
        }

        // Green min/max around each red-blue pair, to clamp the interpolated greens.
        let mut row = top + PAD_G1_G3;
        while row < mrow - PAD_G1_G3 {
            let (mut min, mut max) = (f32::MAX, 0.0f32);
            let mut col = left + PAD_G1_G3;
            while col < mcol - PAD_G1_G3 {
                if fc(row, col, pat) == 1 {
                    min = f32::MAX;
                    max = 0.0;
                    col += 1;
                    continue;
                }
                if max == 0.0 {
                    let p = px(0, row - top, col - left);
                    let hex = hexes.at(row, col);
                    for &o in &hex[..6] {
                        let v = self.g(p + o, 1);
                        min = min.min(v);
                        max = max.max(v);
                    }
                }
                let i = ((row - top) * TS as i32 + col - left) as usize;
                self.gmin[i] = min;
                self.gmax[i] = max;
                // Vertical red-blue pairs: hop down to the second pixel, then
                // back up and right to the next pair.
                match (row - sgrow) % 3 {
                    1 => {
                        if row < mrow - 4 {
                            row += 1;
                            col -= 1;
                        }
                    }
                    2 => {
                        min = f32::MAX;
                        max = 0.0;
                        col += 2;
                        if col < mcol - 4 && row > top + 3 {
                            row -= 1;
                        }
                    }
                    _ => {}
                }
                col += 1;
            }
            row += 1;
        }

        // Green in all four directions.
        for row in top + PAD_G_INTERP..mrow - PAD_G_INTERP {
            for col in left + PAD_G_INTERP..mcol - PAD_G_INTERP {
                let f = fc(row, col, pat);
                if f == 1 {
                    continue;
                }
                let p = px(0, row - top, col - left);
                let hex = hexes.at(row, col);
                let g = |o: isize| self.g(p + o, 1);
                let s = |o: isize| self.g(p + o, f);
                let mut color = [0.0f32; 4];
                color[0] = 0.679_687_5 * (g(hex[1]) + g(hex[0]))
                    - 0.179_687_5 * (g(2 * hex[1]) + g(2 * hex[0]));
                color[1] =
                    0.871_093_75 * g(hex[3]) + g(hex[2]) * 0.13 + 0.359_375 * (s(0) - s(-hex[2]));
                for c in 0..2 {
                    color[2 + c] = 0.640_625 * g(hex[4 + c])
                        + 0.359_375 * g(-2 * hex[4 + c])
                        + 0.128_906_25 * (2.0 * s(0) - s(3 * hex[4 + c]) - s(-3 * hex[4 + c]));
                }
                let i = ((row - top) * TS as i32 + col - left) as usize;
                let (lo, hi) = (self.gmin[i], self.gmax[i]);
                let flip = usize::from((row - sgrow) % 3 == 0);
                for (c, v) in color.iter().enumerate() {
                    self.set(px(c ^ flip, row - top, col - left), 1, clamps(*v, lo, hi));
                }
            }
        }

        // Red and blue for the solitary green pixels.
        let pad_rb_g = 6;
        let tsz = (TS * TS) as isize;
        let mut row = (top - sgrow + pad_rb_g + 2) / 3 * 3 + sgrow;
        while row < mrow - pad_rb_g {
            let mut col = (left - sgcol + pad_rb_g + 2) / 3 * 3 + sgcol;
            while col < mcol - pad_rb_g {
                let mut rfx = px(0, row - top, col - left);
                let mut hc = fc(row, col + 1, pat);
                let mut diff = [0.0f32; 6];
                let mut color = [[0.0f32; 6]; 2];
                let mut i: isize = 1;
                for d in 0..6 {
                    for c in 0..2 {
                        let o = i << c;
                        let g = 2.0 * self.g(rfx, 1) - self.g(rfx + o, 1) - self.g(rfx - o, 1);
                        color[usize::from(hc != 0)][d] =
                            g + self.g(rfx + o, hc) + self.g(rfx - o, hc);
                        if d > 1 {
                            diff[d] += sqr(self.g(rfx + o, 1)
                                - self.g(rfx - o, 1)
                                - self.g(rfx + o, hc)
                                + self.g(rfx - o, hc))
                                + sqr(g);
                        }
                        hc ^= 2;
                    }
                    if d < 2 || d & 1 == 1 {
                        let d_out = d - usize::from(d > 1 && diff[d - 1] < diff[d]);
                        self.set(rfx, 0, color[0][d_out] / 2.0);
                        self.set(rfx, 2, color[1][d_out] / 2.0);
                        rfx += tsz;
                    }
                    i ^= TSI ^ 1;
                    hc ^= 2;
                }
                col += 3;
            }
            row += 3;
        }

        // Red for blue pixels and blue for red ones.
        let pad_rb_br = 6;
        for row in top + pad_rb_br..mrow - pad_rb_br {
            for col in left + pad_rb_br..mcol - pad_rb_br {
                let f = 2 - fc(row, col, pat);
                if f == 1 {
                    continue;
                }
                let mut rfx = px(0, row - top, col - left);
                let c: isize = if (row - sgrow) % 3 != 0 { TSI } else { 1 };
                let h: isize = 3 * (c ^ TSI ^ 1);
                for d in 0..4isize {
                    let g0 = self.g(rfx, 1);
                    let i = if d > 1
                        || ((d ^ c) & 1) != 0
                        || ((g0 - self.g(rfx + c, 1)).abs() + (g0 - self.g(rfx - c, 1)).abs())
                            < 2.0
                                * ((g0 - self.g(rfx + h, 1)).abs()
                                    + (g0 - self.g(rfx - h, 1)).abs())
                    {
                        c
                    } else {
                        h
                    };
                    let v = (self.g(rfx + i, f) + self.g(rfx - i, f) + 2.0 * g0
                        - self.g(rfx + i, 1)
                        - self.g(rfx - i, 1))
                        / 2.0;
                    self.set(rfx, f, v);
                    rfx += tsz;
                }
            }
        }

        // Red and blue for the 2x2 blocks of green.
        let pad_g22 = 8;
        for row in top + pad_g22..mrow - pad_g22 {
            if (row - sgrow) % 3 == 0 {
                continue;
            }
            for col in left + pad_g22..mcol - pad_g22 {
                if (col - sgcol) % 3 == 0 {
                    continue;
                }
                let mut rfx = px(0, row - top, col - left);
                let hex = hexes.at(row, col);
                for d in (0..NDIR).step_by(2) {
                    let (a, b) = (hex[d], hex[d + 1]);
                    if a + b != 0 {
                        let g =
                            3.0 * self.g(rfx, 1) - 2.0 * self.g(rfx + a, 1) - self.g(rfx + b, 1);
                        for c in [0, 2] {
                            let v = (g + 2.0 * self.g(rfx + a, c) + self.g(rfx + b, c)) / 3.0;
                            self.set(rfx, c, v);
                        }
                    } else {
                        let g = 2.0 * self.g(rfx, 1) - self.g(rfx + a, 1) - self.g(rfx + b, 1);
                        for c in [0, 2] {
                            let v = (g + self.g(rfx + a, c) + self.g(rfx + b, c)) / 2.0;
                            self.set(rfx, c, v);
                        }
                    }
                    rfx += tsz;
                }
            }
        }

        // From here on, positions are within the tile.
        let (mrow, mcol) = (mrow - top, mcol - left);
        let plane = |r: i32, c: i32| (r as usize) * TS + c as usize;

        // Derivatives of each direction's result in YPbPr.
        let dirs: [isize; 4] = [1, TSI, TSI + 1, TSI - 1];
        for d in 0..NDIR {
            let pad_yuv = 8;
            for row in pad_yuv..mrow - pad_yuv {
                for col in pad_yuv..mcol - pad_yuv {
                    let p = px(d, row, col);
                    let (r, g, b) = (self.g(p, 0), self.g(p, 1), self.g(p, 2));
                    let y = 0.2627 * r + 0.6780 * g + 0.0593 * b;
                    let i = plane(row, col);
                    self.yuv[i] = y;
                    self.yuv[TS * TS + i] = (b - y) * 0.56433;
                    self.yuv[2 * TS * TS + i] = (r - y) * 0.67815;
                }
            }
            let f = dirs[d & 3];
            let pad_drv = 9;
            for row in pad_drv..mrow - pad_drv {
                for col in pad_drv..mcol - pad_drv {
                    let i = plane(row, col) as isize;
                    let mut sum = 0.0;
                    for ch in 0..3isize {
                        let base = ch * tsz + i;
                        let y = |o: isize| self.yuv[(base + o) as usize];
                        sum += sqr(2.0 * y(0) - y(f) - y(-f));
                    }
                    self.drv[d * TS * TS + i as usize] = sum;
                }
            }
        }

        // Homogeneity: how many neighbours are about as smooth as the
        // smoothest direction.
        self.homo.fill(0);
        let pad_homo = 10;
        for row in pad_homo..mrow - pad_homo {
            for col in pad_homo..mcol - pad_homo {
                let i = plane(row, col);
                let tr = (0..NDIR)
                    .map(|d| self.drv[d * TS * TS + i])
                    .fold(f32::MAX, f32::min)
                    * 8.0;
                for d in 0..NDIR {
                    let mut n = 0u8;
                    for v in -1..=1 {
                        for hh in -1..=1 {
                            n += u8::from(self.drv[d * TS * TS + plane(row + v, col + hh)] <= tr);
                        }
                    }
                    self.homo[d * TS * TS + i] = n;
                }
            }
        }

        // 5x5 sums of the homogeneity maps.
        for d in 0..NDIR {
            let base = d * TS * TS;
            for row in PAD..mrow - PAD {
                for col in PAD..mcol - PAD {
                    let mut sum = 0u8;
                    for v in -2..=2 {
                        for hh in -2..=2 {
                            sum += self.homo[base + plane(row + v, col + hh)];
                        }
                    }
                    self.homosum[base + plane(row, col)] = sum;
                }
            }
        }

        // Average the most homogeneous directions.
        let band_top = top + PAD;
        for row in PAD..mrow - PAD {
            for col in PAD..mcol - PAD {
                // (The 3-pass variant also drops the weaker of each pair of
                // passes here; with one pass there are no pairs.)
                let i = plane(row, col);
                let hm: [u8; NDIR] = std::array::from_fn(|d| self.homosum[d * TS * TS + i]);
                let mut maxval = hm.iter().copied().max().unwrap_or(0);
                maxval -= maxval >> 3;
                let mut avg = [0.0f32; 3];
                let mut n = 0.0f32;
                for (d, _) in hm.iter().enumerate().filter(|(_, h)| **h >= maxval) {
                    let p = px(d, row, col);
                    for (c, a) in avg.iter_mut().enumerate() {
                        *a += self.g(p, c);
                    }
                    n += 1.0;
                }
                let (y, x) = (row + top - band_top, col + left);
                let o = ((y as usize) * width + x as usize) * 3;
                for (slot, a) in rows[o..o + 3].iter_mut().zip(avg) {
                    *slot = (a / n).max(0.0);
                }
            }
        }
    }
}

/// Plain interpolation for the outer `edge` pixels: each colour is the mean
/// of its samples in the 3x3 (or, if missing there, 5x5) neighbourhood.
fn border(out: &mut [f32], input: &[f32], width: usize, height: usize, pat: &Pattern, edge: usize) {
    for y in 0..height {
        let inner = y >= edge && y + edge < height;
        let mut x = 0;
        while x < width {
            if inner && x == edge && width > 2 * edge {
                x = width - edge;
                continue;
            }
            let mut rgb = [0.0f32; 3];
            for r in [1i32, 2] {
                let mut sum = [0.0f32; 3];
                let mut count = [0u32; 3];
                for yy in y as i32 - r..=y as i32 + r {
                    for xx in x as i32 - r..=x as i32 + r {
                        if yy < 0 || xx < 0 || yy >= height as i32 || xx >= width as i32 {
                            continue;
                        }
                        let f = fc(yy, xx, pat);
                        sum[f] += input[yy as usize * width + xx as usize].max(0.0);
                        count[f] += 1;
                    }
                }
                if count.iter().all(|c| *c > 0) {
                    rgb = std::array::from_fn(|c| sum[c] / count[c] as f32);
                    break;
                }
            }
            out[(y * width + x) * 3..(y * width + x) * 3 + 3].copy_from_slice(&rgb);
            x += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The usual X-Trans layout.
    const XTRANS: Pattern = [
        [1, 1, 0, 1, 1, 2],
        [1, 1, 2, 1, 1, 0],
        [2, 0, 1, 0, 2, 1],
        [1, 1, 2, 1, 1, 0],
        [1, 1, 0, 1, 1, 2],
        [0, 2, 1, 2, 0, 1],
    ];

    /// Sample a smooth RGB image through the mosaic.
    fn mosaic(w: usize, h: usize, f: impl Fn(usize, usize) -> [f32; 3]) -> Vec<f32> {
        (0..h)
            .flat_map(|y| (0..w).map(move |x| (y, x)))
            .map(|(y, x)| f(x, y)[fc(y as i32, x as i32, &XTRANS)])
            .collect()
    }

    #[test]
    fn flat_colour_comes_back_exactly() {
        let (w, h) = (300, 290);
        let input = mosaic(w, h, |_, _| [0.4, 0.3, 0.2]);
        let out = demosaic(&input, w, h, &XTRANS);
        for p in out.chunks(3) {
            for (v, want) in p.iter().zip([0.4, 0.3, 0.2]) {
                assert!((v - want).abs() < 1e-4, "{p:?}");
            }
        }
    }

    #[test]
    fn smooth_gradients_stay_close() {
        // Larger than one tile both ways, so tile seams are covered.
        let (w, h) = (530, 270);
        let f = |x: usize, y: usize| {
            let (u, v) = (x as f32 / w as f32, y as f32 / h as f32);
            [0.2 + 0.6 * u, 0.3 + 0.4 * v, 0.7 - 0.5 * u * v]
        };
        let input = mosaic(w, h, f);
        let out = demosaic(&input, w, h, &XTRANS);
        let mut worst = 0.0f32;
        for y in 0..h {
            for x in 0..w {
                let want = f(x, y);
                for c in 0..3 {
                    worst = worst.max((out[(y * w + x) * 3 + c] - want[c]).abs());
                }
            }
        }
        assert!(worst < 0.01, "worst error {worst}");
    }

    #[test]
    fn sharp_edges_beat_plain_averaging() {
        // A vertical black/white edge: the result should switch within a
        // couple of pixels and stay neutral.
        let (w, h) = (300, 64);
        let input = mosaic(w, h, |x, _| if x < 150 { [0.05; 3] } else { [0.8; 3] });
        let out = demosaic(&input, w, h, &XTRANS);
        let y = 32;
        for x in [140, 147, 153, 160] {
            let p = &out[(y * w + x) * 3..(y * w + x) * 3 + 3];
            let want = if x < 150 { 0.05 } else { 0.8 };
            for v in p {
                assert!((v - want).abs() < 0.02, "x {x}: {p:?}");
            }
        }
    }
}
