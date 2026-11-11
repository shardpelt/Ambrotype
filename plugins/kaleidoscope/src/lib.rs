// SPDX-License-Identifier: GPL-3.0-or-later
//! Kaleidoscope: the photo in mirrored slices round a centre, or folded
//! into itself again and again (a fractal).
//!
//! A pixel effect that needs the whole photo (`"whole": true`): each pixel
//! is read from somewhere else in it. Every output pixel is mapped back to
//! a place in the photo and read there smoothly; outside the photo it
//! reflects back in, so there are no empty corners.

use amb_plugin_sdk::{Effect, Pixels};
use std::f32::consts::TAU;

/// Fold `p` into the first slice of `n` round the origin, mirrored, so
/// neighbouring slices meet as mirror images.
fn fold(p: [f32; 2], n: f32) -> [f32; 2] {
    let wedge = TAU / n;
    let r = p[0].hypot(p[1]);
    let mut a = p[1].atan2(p[0]).rem_euclid(wedge);
    if a > wedge / 2.0 {
        a = wedge - a;
    }
    [r * a.cos(), r * a.sin()]
}

/// `v` reflected into 0..len, as a mirror at each edge would.
fn reflect(v: f32, len: f32) -> f32 {
    let m = v.rem_euclid(2.0 * len);
    if m >= len { 2.0 * len - m } else { m }
}

pub fn kaleidoscope(e: &Effect, px: &mut Pixels) -> Result<(), String> {
    let (w, h) = (px.width, px.height);
    if w < 2 || h < 2 {
        return Ok(());
    }
    let fractal = e.choice("style") == "Fractal";
    let n = e.number("slices").clamp(2.0, 32.0).round() as f32;
    let (ts, tc) = (e.number("turn") as f32).to_radians().sin_cos();
    let zoom = (e.number("zoom") / 100.0).max(0.05) as f32;
    let depth = e.number("depth").clamp(1.0, 8.0) as usize;
    let twist = (e.number("twist") as f32).to_radians();
    // Positions in the whole photo's pixels, so a preview and an export
    // match; the buffer is the photo (or, at worst, the part we were given).
    let (fw, fh) = (e.full_width.max(1) as f32, e.full_height.max(1) as f32);
    let (ox, oy) = (e.x as f32, e.y as f32);
    let centre = [
        fw * e.number("x") as f32 / 100.0,
        fh * e.number("y") as f32 / 100.0,
    ];
    let radius = fw.min(fh) / 2.0;
    let src = px.data.to_vec();
    let read = |x: f32, y: f32| -> [f32; 3] {
        let x = reflect(x - ox, w as f32 - 1.0);
        let y = reflect(y - oy, h as f32 - 1.0);
        let (x0, y0) = (x as usize, y as usize);
        let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
        let (tx, ty) = (x - x0 as f32, y - y0 as f32);
        let at = |x: usize, y: usize, k: usize| src[(y * w + x) * 3 + k];
        [0, 1, 2].map(|k| {
            let top = at(x0, y0, k) + (at(x1, y0, k) - at(x0, y0, k)) * tx;
            let bottom = at(x0, y1, k) + (at(x1, y1, k) - at(x0, y1, k)) * tx;
            top + (bottom - top) * ty
        })
    };
    // Fractal folds: each turned a little more, shifted and grown, so the
    // photo repeats inside itself.
    let folds: Vec<(f32, f32)> = (0..depth).map(|i| (twist * i as f32).sin_cos()).collect();
    for y in 0..h {
        for x in 0..w {
            let mut p = [
                (ox + x as f32 + 0.5 - centre[0]) / radius / zoom,
                (oy + y as f32 + 0.5 - centre[1]) / radius / zoom,
            ];
            if fractal {
                for &(s, c) in &folds {
                    p = fold(p, n);
                    p = [c * p[0] - s * p[1], s * p[0] + c * p[1]];
                    p = [p[0] * 1.7 - 0.55, p[1] * 1.7];
                }
                p = fold(p, n);
            } else {
                p = fold(p, n);
            }
            // Turn picks which part of the photo the slice shows.
            let q = [tc * p[0] - ts * p[1], ts * p[0] + tc * p[1]];
            let c = read(centre[0] + q[0] * radius, centre[1] + q[1] * radius);
            px.data[(y * w + x) * 3..(y * w + x) * 3 + 3].copy_from_slice(&c);
        }
    }
    Ok(())
}

amb_plugin_sdk::export_effect!(pixels: kaleidoscope);

#[cfg(test)]
mod tests {
    use super::*;

    fn photo(w: usize, h: usize) -> Vec<f32> {
        (0..w * h)
            .flat_map(|i| {
                let (x, y) = ((i % w) as f32 / w as f32, (i / w) as f32 / h as f32);
                [x, y, 0.5]
            })
            .collect()
    }

    #[test]
    fn mirror_slices_are_symmetric_round_the_centre() {
        let (w, h) = (64, 64);
        let mut data = photo(w, h);
        let e = Effect::for_test(
            serde_json::json!({ "style": "Mirrors", "slices": 4, "zoom": 100, "x": 50, "y": 50 }),
            w as u32,
            h as u32,
        );
        kaleidoscope(
            &e,
            &mut Pixels {
                data: &mut data,
                width: w,
                height: h,
            },
        )
        .unwrap();
        let at = |x: usize, y: usize| &data[(y * w + x) * 3..(y * w + x) * 3 + 3];
        // Four mirrored slices: left and right, top and bottom match.
        for (a, b) in [
            ((10, 20), (53, 20)),
            ((20, 10), (20, 53)),
            ((10, 10), (53, 53)),
        ] {
            let (p, q) = (at(a.0, a.1), at(b.0, b.1));
            for k in 0..3 {
                assert!((p[k] - q[k]).abs() < 0.03, "{a:?} {p:?} vs {b:?} {q:?}");
            }
        }
    }

    #[test]
    fn the_fractal_fills_the_photo_with_its_own_colours() {
        let (w, h) = (48, 32);
        let mut data = photo(w, h);
        let e = Effect::for_test(
            serde_json::json!({ "style": "Fractal", "slices": 6, "depth": 5, "twist": 30, "zoom": 100, "x": 50, "y": 50 }),
            w as u32,
            h as u32,
        );
        kaleidoscope(
            &e,
            &mut Pixels {
                data: &mut data,
                width: w,
                height: h,
            },
        )
        .unwrap();
        assert!(
            data.iter()
                .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
        );
        assert!(
            data.chunks_exact(3).all(|c| (c[2] - 0.5).abs() < 1e-4),
            "read from the photo"
        );
    }
}
