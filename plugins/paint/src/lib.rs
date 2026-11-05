// SPDX-License-Identifier: GPL-3.0-or-later
//! Paint: the photo as an oil painting, a cartoon or a poster.
//!
//! A pixel effect. Oil is a Kuwahara filter: each pixel takes the mean of
//! the calmest of the four squares it is a corner of, so flat areas become
//! brush-like patches and edges stay. Sums over squares come from integral
//! images, so a big brush costs no more than a small one. Cartoon and
//! Poster add flat bands of tone and ink lines (a difference of two blurs
//! of the brightness, dark where it dips). Edges and Neon are Visualise
//! Spots' view as a look: the edges glowing on black, white or coloured.

use amb_plugin_sdk::{Effect, Pixels};

fn luma(c: [f32; 3]) -> f32 {
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}

/// Integral images of the red, green, blue and brightness planes (f32)
/// and of brightness squared (f64, where the variance needs precision):
/// each sums the rows above y and the columns left of x. Kept small, as a
/// plugin has 64 MB.
struct Sums {
    w: usize,
    planes: [Vec<f32>; 4],
    squares: Vec<f64>,
}

impl Sums {
    fn new(px: &Pixels) -> Sums {
        let (w, h) = (px.width, px.height);
        let n = (w + 1) * (h + 1);
        let mut planes: [Vec<f32>; 4] = std::array::from_fn(|_| vec![0.0; n]);
        let mut squares = vec![0.0f64; n];
        for y in 0..h {
            let mut row = [0.0f32; 4];
            let mut row_sq = 0.0f64;
            for x in 0..w {
                let c = px.get(x, y);
                let l = luma(c);
                for (k, v) in [c[0], c[1], c[2], l].into_iter().enumerate() {
                    row[k] += v;
                    planes[k][(y + 1) * (w + 1) + x + 1] = planes[k][y * (w + 1) + x + 1] + row[k];
                }
                row_sq += f64::from(l) * f64::from(l);
                squares[(y + 1) * (w + 1) + x + 1] = squares[y * (w + 1) + x + 1] + row_sq;
            }
        }
        Sums { w, planes, squares }
    }

    /// Sum of plane `k` (4: brightness squared) over columns x0..x1 and
    /// rows y0..y1.
    fn sum(&self, k: usize, x0: usize, y0: usize, x1: usize, y1: usize) -> f64 {
        let i = |x: usize, y: usize| y * (self.w + 1) + x;
        if k == 4 {
            let s = &self.squares;
            return s[i(x1, y1)] - s[i(x0, y1)] - s[i(x1, y0)] + s[i(x0, y0)];
        }
        let s = &self.planes[k];
        f64::from(s[i(x1, y1)] - s[i(x0, y1)] - s[i(x1, y0)] + s[i(x0, y0)])
    }
}

pub fn paint(e: &Effect, px: &mut Pixels) -> Result<(), String> {
    let strength = (e.number("strength") / 100.0) as f32;
    if strength <= 0.0 || px.width == 0 || px.height == 0 {
        return Ok(());
    }
    let (w, h) = (px.width, px.height);
    let long = e.full_width.max(e.full_height).max(1) as f64;
    // The brush in pixels of this render: up to 1.5 % of the long side
    // (and the manifest's margin).
    let r = ((e.number("brush") / 100.0) * 0.015 * long)
        .round()
        .clamp(1.0, 90.0) as usize;
    let style = e.choice("style");
    let lines = (e.number("lines") / 100.0) as f32;
    let levels = e.number("colours").round() as i32;
    let sums = Sums::new(px);
    let mean_box = |k: usize, x0: usize, y0: usize, x1: usize, y1: usize| {
        let n = ((x1 - x0) * (y1 - y0)).max(1) as f64;
        sums.sum(k, x0, y0, x1, y1) / n
    };
    // The Kuwahara filter at every pixel of a coarser grid (every second
    // pixel for a brush of 4 or more), placed on whole-photo positions so
    // strips match, then filled in between: strokes are smooth, so this
    // looks the same at a quarter of the work.
    let step = if r >= 4 { 2 } else { 1 };
    let first = |o: u32| (step - o as usize % step) % step;
    let (gx0, gy0) = (first(e.x), first(e.y));
    let (gw, gh) = (
        (w + step - 1 - gx0) / step + 1,
        (h + step - 1 - gy0) / step + 1,
    );
    let kuwahara = |x: usize, y: usize| {
        let (xl, xr) = (x.saturating_sub(r), (x + r + 1).min(w));
        let (yt, yb) = (y.saturating_sub(r), (y + r + 1).min(h));
        let quads = [
            (xl, yt, x + 1, y + 1),
            (x, yt, xr, y + 1),
            (xl, y, x + 1, yb),
            (x, y, xr, yb),
        ];
        let mut best = (f64::MAX, [0.0f32; 3]);
        for (x0, y0, x1, y1) in quads {
            let m = mean_box(3, x0, y0, x1, y1);
            let var = mean_box(4, x0, y0, x1, y1) - m * m;
            if var < best.0 {
                best = (var, [0, 1, 2].map(|k| mean_box(k, x0, y0, x1, y1) as f32));
            }
        }
        best.1
    };
    let mut grid = vec![[0.0f32; 3]; gw * gh];
    for j in 0..gh {
        for i in 0..gw {
            let (x, y) = ((gx0 + i * step).min(w - 1), (gy0 + j * step).min(h - 1));
            grid[j * gw + i] = kuwahara(x, y);
        }
    }
    // Grid cell and weight for a pixel along one axis.
    let cell = |p: usize, g0: usize, n: usize| {
        let f = (p as f32 - g0 as f32) / step as f32;
        let i = (f.floor().max(0.0) as usize).min(n - 1);
        let i1 = (i + 1).min(n - 1);
        (i, i1, (f - i as f32).clamp(0.0, 1.0))
    };
    for y in 0..h {
        let (j, j1, ty) = cell(y, gy0, gh);
        for x in 0..w {
            let (i, i1, tx) = cell(x, gx0, gw);
            let (a, b, c0, d) = (
                grid[j * gw + i],
                grid[j * gw + i1],
                grid[j1 * gw + i],
                grid[j1 * gw + i1],
            );
            let best = (
                0.0,
                [0, 1, 2].map(|k| {
                    let top = a[k] + (b[k] - a[k]) * tx;
                    let bottom = c0[k] + (d[k] - c0[k]) * tx;
                    top + (bottom - top) * ty
                }),
            );
            let mut c = best.1;
            if style == "Edges" || style == "Neon" {
                // Visualise Spots' view: the edges glowing on black, white
                // (Edges) or in the photo's own colours, made vivid (Neon).
                let a = r.div_ceil(4).max(1);
                let (xl, xr) = (x.saturating_sub(a), (x + a + 1).min(w));
                let (yt, yb) = (y.saturating_sub(a), (y + a + 1).min(h));
                let gx = mean_box(3, x + 1, yt, xr.max(x + 2).min(w), yb)
                    - mean_box(3, xl, yt, x.max(xl + 1), yb);
                let gy = mean_box(3, xl, y + 1, xr, yb.max(y + 2).min(h))
                    - mean_box(3, xl, yt, xr, y.max(yt + 1));
                let edge = (gx.hypot(gy) as f32 * 10.0 - 0.04)
                    .clamp(0.0, 1.0)
                    .powf(0.8);
                c = if style == "Edges" {
                    [edge; 3]
                } else {
                    // The colour round the edge (a pixel on its dark side has little).
                    let o = [0, 1, 2].map(|k| mean_box(k, xl, yt, xr, yb) as f32);
                    let l = luma(o);
                    let vivid = o.map(|v| (l + (v - l) * 3.0).clamp(0.0, 1.0));
                    let peak = vivid[0].max(vivid[1]).max(vivid[2]).max(1e-3);
                    vivid.map(|v| (v / peak * edge * 1.3).min(1.0))
                };
            } else if style == "Poster" || levels >= 2 {
                // Flat bands of brightness, keeping the colour.
                let n = if levels >= 2 { levels } else { 4 } as f32 - 1.0;
                let l = luma(c).max(1e-4);
                let q = (l * n).round() / n;
                c = c.map(|v| (v * q / l).clamp(0.0, 1.0));
            }
            if lines > 0.0 {
                // Ink where the brightness dips below its surroundings.
                let (a, b) = (r.div_ceil(3).max(1), r.max(2));
                let near = mean_box(
                    3,
                    x.saturating_sub(a),
                    y.saturating_sub(a),
                    (x + a + 1).min(w),
                    (y + a + 1).min(h),
                );
                let far = mean_box(
                    3,
                    x.saturating_sub(b),
                    y.saturating_sub(b),
                    (x + b + 1).min(w),
                    (y + b + 1).min(h),
                );
                let dip = ((far - near) as f32 * 12.0).clamp(0.0, 1.0);
                let ink = 1.0 - lines * dip;
                c = c.map(|v| v * ink);
            }
            if style == "Cartoon" {
                // Brighter, cleaner colour.
                let l = luma(c);
                c = c.map(|v| (l + (v - l) * 1.3).clamp(0.0, 1.0));
            }
            // Each pixel is read (here) before it is written; the sums
            // were taken before any.
            let o = px.get(x, y);
            px.set(x, y, [0, 1, 2].map(|k| o[k] + (c[k] - o[k]) * strength));
        }
    }
    Ok(())
}

amb_plugin_sdk::export_effect!(pixels: paint);

#[cfg(test)]
mod tests {
    use super::*;

    fn run(params: serde_json::Value, data: &mut [f32], w: usize, h: usize) {
        let e = Effect::for_test(params, w as u32, h as u32);
        paint(
            &e,
            &mut Pixels {
                data,
                width: w,
                height: h,
            },
        )
        .unwrap();
    }

    #[test]
    fn oil_smooths_noise_and_keeps_an_edge() {
        let (w, h) = (80, 40);
        // Left dark, right bright, with noise.
        let mut data: Vec<f32> = (0..w * h)
            .flat_map(|i| {
                let base = if i % w < 40 { 0.2 } else { 0.8 };
                let n = ((i * 7919) % 13) as f32 / 13.0 * 0.2 - 0.1;
                [base + n; 3]
            })
            .collect();
        let rough = |d: &[f32]| {
            (1..w - 1)
                .filter(|x| *x != 39 && *x != 40)
                .map(|x| (d[(20 * w + x) * 3] - d[(20 * w + x + 1) * 3]).abs())
                .sum::<f32>()
        };
        let before = rough(&data);
        run(
            serde_json::json!({ "style": "Oil", "brush": 100, "lines": 0, "colours": 0, "strength": 100 }),
            &mut data,
            w,
            h,
        );
        assert!(rough(&data) < before * 0.5, "{} vs {before}", rough(&data));
        // The edge stays sharp: dark left of it, bright right of it.
        assert!(data[(20 * w + 36) * 3] < 0.4 && data[(20 * w + 43) * 3] > 0.6);
    }

    #[test]
    fn cartoon_draws_ink_on_a_dark_line_and_poster_has_few_tones() {
        let (w, h) = (60, 30);
        let mut data: Vec<f32> = (0..w * h)
            .flat_map(|i| if i % w == 30 { [0.1; 3] } else { [0.7; 3] })
            .collect();
        run(
            serde_json::json!({ "style": "Cartoon", "brush": 20, "lines": 100, "colours": 0, "strength": 100 }),
            &mut data,
            w,
            h,
        );
        assert!(data[(15 * w + 30) * 3] < 0.5);
        let mut ramp: Vec<f32> = (0..w * h)
            .flat_map(|i| [(i % w) as f32 / w as f32; 3])
            .collect();
        run(
            serde_json::json!({ "style": "Poster", "brush": 1, "lines": 0, "colours": 3, "strength": 100 }),
            &mut ramp,
            w,
            h,
        );
        let mut tones: Vec<i32> = (0..w)
            .map(|x| (ramp[(15 * w + x) * 3] * 100.0).round() as i32)
            .collect();
        tones.sort();
        tones.dedup();
        assert!(tones.len() <= 6, "{tones:?}");
    }

    #[test]
    fn edges_glow_on_black_and_neon_keeps_the_colour() {
        let (w, h) = (60, 30);
        let photo: Vec<f32> = (0..w * h)
            .flat_map(|i| {
                if i % w < 30 {
                    [0.1, 0.1, 0.1]
                } else {
                    [0.9, 0.3, 0.2]
                }
            })
            .collect();
        let mut data = photo.clone();
        run(
            serde_json::json!({ "style": "Edges", "brush": 10, "lines": 0, "colours": 0, "strength": 100 }),
            &mut data,
            w,
            h,
        );
        assert!(
            data[(15 * w + 30) * 3] > 0.5,
            "the edge: {}",
            data[(15 * w + 30) * 3]
        );
        assert!(data[(15 * w + 10) * 3] < 0.05 && data[(15 * w + 50) * 3] < 0.05);
        let mut neon = photo;
        run(
            serde_json::json!({ "style": "Neon", "brush": 10, "lines": 0, "colours": 0, "strength": 100 }),
            &mut neon,
            w,
            h,
        );
        let c = &neon[(15 * w + 29) * 3..][..3];
        assert!(c[0] > c[1] && c[0] > c[2] && c[0] > 0.3, "{c:?}");
    }
}
