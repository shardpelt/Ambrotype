// SPDX-License-Identifier: GPL-3.0-or-later
//! The sky mask: how sky-like a colour is, and the model-free sky detection.

use super::*;

/// How sky-like a colour (OkLab of the tone-encoded light) is: bright, and
/// blue, cyan or near neutral. Foliage, earth and warm colours have a
/// positive b (yellow) and dark ones a low L, so they aren't sky.
pub(super) fn sky_colour(lab: [f32; 3]) -> f32 {
    let [l, _, b] = lab.map(f64::from);
    let bright = smoothstep(0.38, 0.62, l);
    let not_yellow = 1.0 - smoothstep(0.0, 0.05, b);
    (bright * not_yellow) as f32
}

/// **Ours, not Adobe's:** the sky as smooth, bright, blue or neutral areas
/// joined to the top edge, on a grid of at most 128 cells a side. Each cell
/// scores by [`sky_colour`] and how little its brightness changes against
/// its neighbours; cells above 0.5 in the top row start a fill that spreads
/// through cells above 0.35, then grows into sky-coloured textured cells.
pub fn sky_map(lab: &[[f32; 3]], w: usize, h: usize) -> SkyMap {
    let scale = (w.max(h) as f64 / 128.0).max(1.0);
    let (sw, sh) = (
        ((w as f64 / scale).ceil() as usize).max(1),
        ((h as f64 / scale).ceil() as usize).max(1),
    );
    let mut cells = vec![[0.0f32; 3]; sw * sh];
    let mut count = vec![0u32; sw * sh];
    for y in 0..h {
        let cy = ((y as f64 / scale) as usize).min(sh - 1);
        for x in 0..w {
            let i = cy * sw + ((x as f64 / scale) as usize).min(sw - 1);
            for c in 0..3 {
                cells[i][c] += lab[y * w + x][c];
            }
            count[i] += 1;
        }
    }
    for (c, n) in cells.iter_mut().zip(&count) {
        *c = c.map(|v| v / (*n).max(1) as f32);
    }
    let score: Vec<f32> = (0..sw * sh)
        .map(|i| {
            let (x, y) = (i % sw, i / sw);
            let l = cells[i][0];
            let mut change = 0.0f32;
            for (dx, dy) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)] {
                let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                if nx >= 0 && ny >= 0 && (nx as usize) < sw && (ny as usize) < sh {
                    change = change.max((cells[ny as usize * sw + nx as usize][0] - l).abs());
                }
            }
            let smooth = 1.0 - smoothstep(0.02, 0.07, f64::from(change)) as f32;
            sky_colour(cells[i]) * smooth
        })
        .collect();
    let mut sky = vec![false; sw * sh];
    let mut stack: Vec<usize> = (0..sw).filter(|&x| score[x] > 0.5).collect();
    while let Some(i) = stack.pop() {
        if sky[i] {
            continue;
        }
        sky[i] = true;
        let (x, y) = (i % sw, i / sw);
        let mut push = |nx: usize, ny: usize| {
            let j = ny * sw + nx;
            if !sky[j] && score[j] > 0.35 {
                stack.push(j);
            }
        };
        if x > 0 {
            push(x - 1, y);
        }
        if x + 1 < sw {
            push(x + 1, y);
        }
        if y > 0 {
            push(x, y - 1);
        }
        if y + 1 < sh {
            push(x, y + 1);
        }
    }
    // Grow two cells into neighbours whose colour is sky-like however
    // textured (sky between branches): the per-pixel colour test then
    // leaves the branches out.
    for _ in 0..2 {
        let grown: Vec<bool> = (0..sw * sh)
            .map(|i| {
                if sky[i] {
                    return true;
                }
                let (x, y) = (i % sw, i / sw);
                let near = (x > 0 && sky[i - 1])
                    || (x + 1 < sw && sky[i + 1])
                    || (y > 0 && sky[i - sw])
                    || (y + 1 < sh && sky[i + sw]);
                near && sky_colour(cells[i]) > 0.3
            })
            .collect();
        sky = grown;
    }
    SkyMap {
        width: sw,
        height: sh,
        data: sky.iter().map(|&s| if s { 1.0 } else { 0.0 }).collect(),
    }
}

impl SkyMap {
    /// Bilinear value at output fractions (u, v).
    pub(super) fn at(&self, u: f64, v: f64) -> f32 {
        let x = (u * self.width as f64 - 0.5).clamp(0.0, (self.width - 1) as f64);
        let y = (v * self.height as f64 - 0.5).clamp(0.0, (self.height - 1) as f64);
        let (x0, y0) = (x.floor() as usize, y.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(self.width - 1), (y0 + 1).min(self.height - 1));
        let (fx, fy) = ((x - x0 as f64) as f32, (y - y0 as f64) as f32);
        let p = |xx: usize, yy: usize| self.data[yy * self.width + xx];
        let top = p(x0, y0) + (p(x1, y0) - p(x0, y0)) * fx;
        let bottom = p(x0, y1) + (p(x1, y1) - p(x0, y1)) * fx;
        top + (bottom - top) * fy
    }
}
