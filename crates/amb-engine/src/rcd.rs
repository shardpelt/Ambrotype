// SPDX-License-Identifier: GPL-3.0-or-later
//! Demosaicing for Bayer sensors: Ratio Corrected Demosaicing (RCD), by
//! Luis Sanz Rodríguez (version 2.3).
//!
//! Written from the algorithm as RawTherapee (`rtengine/rcd_demosaic.cc`)
//! and darktable (`src/iop/demosaicing/rcd.c`) implement it, both
//! GPL-3.0-or-later; the original is GPL-3.0
//! (<https://github.com/LuisSR/RCD-Demosaicing>). The steps, their names
//! and the 9-pixel border follow RawTherapee's code; the image is worked
//! in bands of rows in parallel instead of square tiles.
//!
//! In short: each pixel gets a vertical-or-horizontal weight from the
//! energy of a high-pass filter in each direction; green at red and blue
//! comes from its four neighbours, each corrected by the ratio of a
//! low-pass image there and here, blended by that weight; red and blue then
//! follow from colour differences, first along the diagonals (with their
//! own weight), then at the greens. It keeps edges sharp with few zippers
//! or false colours, as PPG (rawler's) doesn't.
//!
//! Samples are white-balanced first, as RawTherapee does, so the colour
//! differences compare like with like, and scaled back afterwards.

use rayon::prelude::*;

/// The 2x2 colour pattern: 0 red, 1 green, 2 blue, for `(row % 2, col % 2)`.
pub type Pattern = [[u8; 2]; 2];

/// Rows each band outputs; bands read `BORDER` more on each side.
const BAND: usize = 128;
/// How far from a band's edge results are valid (RawTherapee's `rcdBorder`).
const BORDER: usize = 9;
const EPS: f32 = 1e-5;
const EPSSQ: f32 = 1e-10;

/// Demosaic `samples` (`w × h`, one per pixel, `pattern` from the top-left)
/// to interleaved RGB. `wb` are the white-balance multipliers the samples
/// are worked at (green 1); the result is in the samples' own scale.
pub fn demosaic(samples: &[f32], w: usize, h: usize, pattern: &Pattern, wb: [f32; 3]) -> Vec<f32> {
    let mut out = vec![0.0f32; w * h * 3];
    if w < 2 * BORDER + 2 || h < 2 * BORDER + 2 {
        bilinear_rows(samples, w, h, pattern, &mut out, 0, h, |_, _| true);
        return out;
    }
    let colour = |r: usize, c: usize| pattern[r % 2][c % 2] as usize;
    out.par_chunks_mut(BAND * w * 3)
        .enumerate()
        .for_each(|(b, chunk)| {
            let y0 = b * BAND;
            let y1 = (y0 + BAND).min(h);
            let top = y0.saturating_sub(BORDER);
            let bottom = (y1 + BORDER).min(h);
            let band = band(samples, w, top, bottom, &colour, wb);
            for y in y0..y1 {
                let by = y - top;
                for x in 0..w {
                    let o = ((y - y0) * w + x) * 3;
                    for c in 0..3 {
                        chunk[o + c] = band[c][by * w + x].max(0.0) / wb[c];
                    }
                }
            }
            // Near the image's edges RCD has too few neighbours.
            let edge =
                |y: usize, x: usize| y < BORDER || y >= h - BORDER || x < BORDER || x >= w - BORDER;
            bilinear_rows(samples, w, h, pattern, chunk, y0, y1, edge);
        });
    out
}

/// RCD on rows `top..bottom` (all columns): the three channel planes, white
/// balanced. Valid where at least `BORDER` from the band's edges.
fn band(
    samples: &[f32],
    w: usize,
    top: usize,
    bottom: usize,
    colour: &(dyn Fn(usize, usize) -> usize + Sync),
    wb: [f32; 3],
) -> [Vec<f32>; 3] {
    let bh = bottom - top;
    let n = w * bh;
    let fc = |r: usize, c: usize| colour(r + top, c);
    let cfa: Vec<f32> = (0..n)
        .map(|i| {
            let (r, c) = (i / w, i % w);
            (samples[(r + top) * w + c] * wb[fc(r, c)]).max(0.0)
        })
        .collect();
    let mut rgb = [vec![0.0f32; n], vec![0.0f32; n], vec![0.0f32; n]];
    for (i, v) in cfa.iter().enumerate() {
        rgb[fc(i / w, i % w)][i] = *v;
    }
    let (w1, w2, w3, w4) = (w, 2 * w, 3 * w, 4 * w);
    let sqr = |v: f32| v * v;
    let intp = |a: f32, b: f32, c: f32| a * (b - c) + c;
    // A pixel isn't green.
    let rb = |r: usize, c: usize| fc(r, c) != 1;

    // Step 1.1: the square of the vertical and horizontal colour-difference
    // high-pass filter.
    let mut buf_v = vec![0.0f32; n];
    let mut buf_h = vec![0.0f32; n];
    for row in 3..bh - 3 {
        for col in 0..w {
            let i = row * w + col;
            buf_v[i] = sqr((cfa[i - w3] - cfa[i - w1] - cfa[i + w1] + cfa[i + w3])
                - 3.0 * (cfa[i - w2] + cfa[i + w2])
                + 6.0 * cfa[i]);
        }
    }
    for row in 0..bh {
        for col in 3..w - 3 {
            let i = row * w + col;
            buf_h[i] = sqr((cfa[i - 3] - cfa[i - 1] - cfa[i + 1] + cfa[i + 3])
                - 3.0 * (cfa[i - 2] + cfa[i + 2])
                + 6.0 * cfa[i]);
        }
    }
    // Step 1.2: the vertical and horizontal directional discrimination.
    let mut vh_dir = vec![0.5f32; n];
    for row in 4..bh - 4 {
        for col in 4..w - 4 {
            let i = row * w + col;
            let v = EPSSQ.max(buf_v[i - w1] + buf_v[i] + buf_v[i + w1]);
            let h = EPSSQ.max(buf_h[i - 1] + buf_h[i] + buf_h[i + 1]);
            vh_dir[i] = v / (v + h);
        }
    }
    drop((buf_v, buf_h));

    // Step 2: a low-pass filter of red, green and blue together, at red and
    // blue pixels.
    let mut lpf = vec![0.0f32; n];
    for row in 2..bh - 2 {
        for col in 2..w - 2 {
            if !rb(row, col) {
                continue;
            }
            let i = row * w + col;
            lpf[i] = cfa[i]
                + 0.5 * (cfa[i - w1] + cfa[i + w1] + cfa[i - 1] + cfa[i + 1])
                + 0.25 * (cfa[i - w1 - 1] + cfa[i - w1 + 1] + cfa[i + w1 - 1] + cfa[i + w1 + 1]);
        }
    }

    // A weight refined by its diagonal neighbours: theirs when they agree
    // more strongly on a direction.
    let refined = |dir: &[f32], i: usize| {
        let central = dir[i];
        let around = 0.25 * (dir[i - w1 - 1] + dir[i - w1 + 1] + dir[i + w1 - 1] + dir[i + w1 + 1]);
        if (0.5 - central).abs() < (0.5 - around).abs() {
            around
        } else {
            central
        }
    };

    // Step 3: green at red and blue pixels.
    for row in 4..bh - 4 {
        for col in 4..w - 4 {
            if !rb(row, col) {
                continue;
            }
            let i = row * w + col;
            let (c, l) = (&cfa, &lpf);
            let n_grad = EPS
                + (c[i - w1] - c[i + w1]).abs()
                + (c[i] - c[i - w2]).abs()
                + (c[i - w1] - c[i - w3]).abs()
                + (c[i - w2] - c[i - w4]).abs();
            let s_grad = EPS
                + (c[i - w1] - c[i + w1]).abs()
                + (c[i] - c[i + w2]).abs()
                + (c[i + w1] - c[i + w3]).abs()
                + (c[i + w2] - c[i + w4]).abs();
            let w_grad = EPS
                + (c[i - 1] - c[i + 1]).abs()
                + (c[i] - c[i - 2]).abs()
                + (c[i - 1] - c[i - 3]).abs()
                + (c[i - 2] - c[i - 4]).abs();
            let e_grad = EPS
                + (c[i - 1] - c[i + 1]).abs()
                + (c[i] - c[i + 2]).abs()
                + (c[i + 1] - c[i + 3]).abs()
                + (c[i + 2] - c[i + 4]).abs();
            let ratio = |j: usize| 1.0 + (l[i] - l[j]) / (EPS + l[i] + l[j]);
            let n_est = c[i - w1] * ratio(i - w2);
            let s_est = c[i + w1] * ratio(i + w2);
            let w_est = c[i - 1] * ratio(i - 2);
            let e_est = c[i + 1] * ratio(i + 2);
            let v_est = (s_grad * n_est + n_grad * s_est) / (n_grad + s_grad);
            let h_est = (w_grad * e_est + e_grad * w_est) / (e_grad + w_grad);
            rgb[1][i] = intp(refined(&vh_dir, i), h_est, v_est).max(0.0);
        }
    }
    drop(lpf);

    // Step 4.1: the diagonal (P: \, Q: /) high-pass filters.
    let mut buf_p = vec![0.0f32; n];
    let mut buf_q = vec![0.0f32; n];
    for row in 3..bh - 3 {
        for col in 3..w - 3 {
            let i = row * w + col;
            buf_p[i] = sqr(
                (cfa[i - w3 - 3] - cfa[i - w1 - 1] - cfa[i + w1 + 1] + cfa[i + w3 + 3])
                    - 3.0 * (cfa[i - w2 - 2] + cfa[i + w2 + 2])
                    + 6.0 * cfa[i],
            );
            buf_q[i] = sqr(
                (cfa[i - w3 + 3] - cfa[i - w1 + 1] - cfa[i + w1 - 1] + cfa[i + w3 - 3])
                    - 3.0 * (cfa[i - w2 + 2] + cfa[i + w2 - 2])
                    + 6.0 * cfa[i],
            );
        }
    }
    // Step 4.2: the diagonal discrimination at red and blue pixels.
    let mut pq_dir = vec![0.5f32; n];
    for row in 4..bh - 4 {
        for col in 4..w - 4 {
            if !rb(row, col) {
                continue;
            }
            let i = row * w + col;
            let p = EPSSQ.max(buf_p[i - w1 - 1] + buf_p[i] + buf_p[i + w1 + 1]);
            let q = EPSSQ.max(buf_q[i - w1 + 1] + buf_q[i] + buf_q[i + w1 - 1]);
            pq_dir[i] = p / (p + q);
        }
    }
    drop((buf_p, buf_q));

    // Step 4.3: red at blue pixels and blue at red ones.
    for row in 4..bh - 4 {
        for col in 4..w - 4 {
            if !rb(row, col) {
                continue;
            }
            let i = row * w + col;
            let c = 2 - fc(row, col);
            let disc = refined(&pq_dir, i);
            let (x, g) = (&rgb[c], &rgb[1]);
            let nw_grad = EPS
                + (x[i - w1 - 1] - x[i + w1 + 1]).abs()
                + (x[i - w1 - 1] - x[i - w3 - 3]).abs()
                + (g[i] - g[i - w2 - 2]).abs();
            let ne_grad = EPS
                + (x[i - w1 + 1] - x[i + w1 - 1]).abs()
                + (x[i - w1 + 1] - x[i - w3 + 3]).abs()
                + (g[i] - g[i - w2 + 2]).abs();
            let sw_grad = EPS
                + (x[i + w1 - 1] - x[i - w1 + 1]).abs()
                + (x[i + w1 - 1] - x[i + w3 - 3]).abs()
                + (g[i] - g[i + w2 - 2]).abs();
            let se_grad = EPS
                + (x[i + w1 + 1] - x[i - w1 - 1]).abs()
                + (x[i + w1 + 1] - x[i + w3 + 3]).abs()
                + (g[i] - g[i + w2 + 2]).abs();
            let nw = x[i - w1 - 1] - g[i - w1 - 1];
            let ne = x[i - w1 + 1] - g[i - w1 + 1];
            let sw = x[i + w1 - 1] - g[i + w1 - 1];
            let se = x[i + w1 + 1] - g[i + w1 + 1];
            let p_est = (nw_grad * se + se_grad * nw) / (nw_grad + se_grad);
            let q_est = (ne_grad * sw + sw_grad * ne) / (ne_grad + sw_grad);
            let v = (g[i] + intp(disc, q_est, p_est)).max(0.0);
            rgb[c][i] = v;
        }
    }
    drop(pq_dir);

    // Step 4.4: red and blue at green pixels.
    for row in 4..bh - 4 {
        for col in 4..w - 4 {
            if rb(row, col) {
                continue;
            }
            let i = row * w + col;
            let disc = refined(&vh_dir, i);
            for c in [0, 2] {
                let (x, g) = (&rgb[c], &rgb[1]);
                let n_grad = EPS
                    + (g[i] - g[i - w2]).abs()
                    + (x[i - w1] - x[i + w1]).abs()
                    + (x[i - w1] - x[i - w3]).abs();
                let s_grad = EPS
                    + (g[i] - g[i + w2]).abs()
                    + (x[i + w1] - x[i - w1]).abs()
                    + (x[i + w1] - x[i + w3]).abs();
                let w_grad = EPS
                    + (g[i] - g[i - 2]).abs()
                    + (x[i - 1] - x[i + 1]).abs()
                    + (x[i - 1] - x[i - 3]).abs();
                let e_grad = EPS
                    + (g[i] - g[i + 2]).abs()
                    + (x[i + 1] - x[i - 1]).abs()
                    + (x[i + 1] - x[i + 3]).abs();
                let n_est = x[i - w1] - g[i - w1];
                let s_est = x[i + w1] - g[i + w1];
                let w_est = x[i - 1] - g[i - 1];
                let e_est = x[i + 1] - g[i + 1];
                let v_est = (n_grad * s_est + s_grad * n_est) / (n_grad + s_grad);
                let h_est = (e_grad * w_est + w_grad * e_est) / (e_grad + w_grad);
                let v = (g[i] + intp(disc, h_est, v_est)).max(0.0);
                rgb[c][i] = v;
            }
        }
    }
    rgb
}

/// Plain bilinear demosaic of the pixels in rows `y0..y1` that `which`
/// picks (`out` holds those rows, interleaved, from `y0`): each missing
/// colour is the mean of the same colour among the 3x3 neighbours.
#[allow(clippy::too_many_arguments)]
fn bilinear_rows(
    samples: &[f32],
    w: usize,
    h: usize,
    pattern: &Pattern,
    out: &mut [f32],
    y0: usize,
    y1: usize,
    which: impl Fn(usize, usize) -> bool,
) {
    for y in y0..y1 {
        for x in 0..w {
            if !which(y, x) {
                continue;
            }
            let mut sum = [0.0f32; 3];
            let mut count = [0u32; 3];
            for dy in -1i64..=1 {
                for dx in -1i64..=1 {
                    let (yy, xx) = (y as i64 + dy, x as i64 + dx);
                    if yy < 0 || xx < 0 || yy >= h as i64 || xx >= w as i64 {
                        continue;
                    }
                    let (yy, xx) = (yy as usize, xx as usize);
                    let c = pattern[yy % 2][xx % 2] as usize;
                    sum[c] += samples[yy * w + xx];
                    count[c] += 1;
                }
            }
            let own = pattern[y % 2][x % 2] as usize;
            let o = ((y - y0) * w + x) * 3;
            for c in 0..3 {
                out[o + c] = if c == own {
                    samples[y * w + x]
                } else if count[c] > 0 {
                    sum[c] / count[c] as f32
                } else {
                    0.0
                };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RGGB: Pattern = [[0, 1], [1, 2]];

    /// A mosaic of `img` (interleaved RGB) through the pattern.
    fn mosaic(img: &[f32], w: usize, h: usize, p: &Pattern) -> Vec<f32> {
        (0..w * h)
            .map(|i| img[i * 3 + p[(i / w) % 2][(i % w) % 2] as usize])
            .collect()
    }

    /// Mean absolute error over the inside (away from the plain border).
    fn error(a: &[f32], b: &[f32], w: usize, h: usize) -> f32 {
        let mut sum = 0.0;
        let mut n = 0;
        for y in BORDER..h - BORDER {
            for x in BORDER..w - BORDER {
                for c in 0..3 {
                    let i = (y * w + x) * 3 + c;
                    sum += (a[i] - b[i]).abs();
                    n += 1;
                }
            }
        }
        sum / n as f32
    }

    fn image(w: usize, h: usize, f: impl Fn(usize, usize) -> [f32; 3]) -> Vec<f32> {
        (0..w * h).flat_map(|i| f(i % w, i / w)).collect()
    }

    #[test]
    fn flat_colour_comes_back() {
        let (w, h) = (64, 300);
        let img = image(w, h, |_, _| [0.4, 0.3, 0.2]);
        for p in [RGGB, [[1, 0], [2, 1]], [[2, 1], [1, 0]], [[1, 2], [0, 1]]] {
            let out = demosaic(&mosaic(&img, w, h, &p), w, h, &p, [2.0, 1.0, 1.5]);
            for (a, b) in out.iter().zip(&img) {
                assert!((a - b).abs() < 1e-4, "{a} {b}");
            }
        }
    }

    #[test]
    fn smooth_gradients_stay_close() {
        let (w, h) = (96, 280);
        let img = image(w, h, |x, y| {
            let (u, v) = (x as f32 / w as f32, y as f32 / h as f32);
            [0.1 + 0.6 * u, 0.2 + 0.5 * v, 0.6 - 0.4 * u * v]
        });
        let out = demosaic(&mosaic(&img, w, h, &RGGB), w, h, &RGGB, [1.0; 3]);
        assert!(
            error(&out, &img, w, h) < 2e-3,
            "{}",
            error(&out, &img, w, h)
        );
    }

    #[test]
    fn sharp_edges_beat_bilinear() {
        // Black and white stripes at an angle, and a coloured disc.
        let (w, h) = (128, 260);
        let img = image(w, h, |x, y| {
            let stripe = ((x as f32 * 0.9 + y as f32 * 0.35) / 5.0).floor() as i64 % 2 == 0;
            let (dx, dy) = (x as f32 - 64.0, y as f32 - 130.0);
            if dx * dx + dy * dy < 900.0 {
                [0.7, 0.2, 0.1]
            } else if stripe {
                [0.8, 0.8, 0.8]
            } else {
                [0.05, 0.05, 0.05]
            }
        });
        let cfa = mosaic(&img, w, h, &RGGB);
        let rcd = demosaic(&cfa, w, h, &RGGB, [1.0; 3]);
        let mut plain = vec![0.0; w * h * 3];
        bilinear_rows(&cfa, w, h, &RGGB, &mut plain, 0, h, |_, _| true);
        let (e_rcd, e_bil) = (error(&rcd, &img, w, h), error(&plain, &img, w, h));
        assert!(e_rcd < 0.75 * e_bil, "rcd {e_rcd} bilinear {e_bil}");
    }

    #[test]
    fn bands_join_without_seams() {
        // A diagonal gradient across several bands: rows either side of a
        // band boundary differ as little as rows within a band.
        let (w, h) = (48, 3 * BAND + 17);
        let img = image(w, h, |x, y| {
            let t = (x + y) as f32 / (w + h) as f32;
            [t, 0.5 * t + 0.2, 1.0 - t]
        });
        let out = demosaic(&mosaic(&img, w, h, &RGGB), w, h, &RGGB, [1.0; 3]);
        assert!(error(&out, &img, w, h) < 2e-3);
    }

    #[test]
    fn small_images_fall_back() {
        let img = image(10, 7, |_, _| [0.5, 0.5, 0.5]);
        let out = demosaic(&mosaic(&img, 10, 7, &RGGB), 10, 7, &RGGB, [1.0; 3]);
        assert_eq!(out.len(), 10 * 7 * 3);
        assert!(out.iter().all(|v| (v - 0.5).abs() < 1e-6));
    }
}
