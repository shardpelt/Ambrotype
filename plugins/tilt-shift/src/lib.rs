// SPDX-License-Identifier: GPL-3.0-or-later
//! Tilt-shift: a sharp band across the photo, blur growing above and below.
//!
//! A pixel effect. The blur is two box blurs with a radius that changes by
//! row (from whole-photo positions, so strips match), each from integral
//! images, so a big blur costs no more than a small one.

use amb_plugin_sdk::{Effect, Pixels};

fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// One box blur of `data` (w × h, RGB) with radius `radius(y)` for row y.
fn box_blur(data: &mut [f32], w: usize, h: usize, radius: &dyn Fn(usize) -> usize) {
    let stride = w + 1;
    let mut sums: [Vec<f32>; 3] = std::array::from_fn(|_| vec![0.0; stride * (h + 1)]);
    for y in 0..h {
        let mut row = [0.0f32; 3];
        for x in 0..w {
            for (k, plane) in sums.iter_mut().enumerate() {
                row[k] += data[(y * w + x) * 3 + k];
                plane[(y + 1) * stride + x + 1] = plane[y * stride + x + 1] + row[k];
            }
        }
    }
    for y in 0..h {
        let r = radius(y);
        if r == 0 {
            continue;
        }
        let (y0, y1) = (y.saturating_sub(r), (y + r + 1).min(h));
        for x in 0..w {
            let (x0, x1) = (x.saturating_sub(r), (x + r + 1).min(w));
            let n = ((x1 - x0) * (y1 - y0)) as f32;
            for (k, s) in sums.iter().enumerate() {
                let v = s[y1 * stride + x1] - s[y0 * stride + x1] - s[y1 * stride + x0]
                    + s[y0 * stride + x0];
                data[(y * w + x) * 3 + k] = v / n;
            }
        }
    }
}

pub fn tilt(e: &Effect, px: &mut Pixels) -> Result<(), String> {
    let blur = (e.number("blur") / 100.0) as f32;
    if blur <= 0.0 || px.width == 0 || px.height == 0 {
        return Ok(());
    }
    let (w, h) = (px.width, px.height);
    let fh = e.full_height.max(1) as f32;
    let long = e.full_width.max(e.full_height).max(1) as f32;
    let centre = (e.number("position") / 100.0) as f32;
    let half = (e.number("band") / 200.0) as f32;
    let falloff = (e.number("falloff") / 100.0) as f32;
    // Two passes of up to 45 px each stay inside the 96 px margin.
    let most = (blur * 0.012 * long).min(45.0);
    let radius = |y: usize| {
        let v = (e.y as f32 + y as f32 + 0.5) / fh;
        let away = ((v - centre).abs() - half).max(0.0);
        (most * smoothstep(away / falloff.max(0.01))).round() as usize
    };
    box_blur(px.data, w, h, &radius);
    box_blur(px.data, w, h, &radius);
    Ok(())
}

amb_plugin_sdk::export_effect!(pixels: tilt);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_band_stays_sharp_and_the_top_blurs() {
        let (w, h) = (80, 100);
        // Fine stripes everywhere.
        let mut data: Vec<f32> = (0..w * h)
            .flat_map(|i| if i % 2 == 0 { [0.9; 3] } else { [0.1; 3] })
            .collect();
        let e = Effect::for_test(
            serde_json::json!({ "position": 60, "band": 20, "blur": 100, "falloff": 20 }),
            1600,
            2000,
        );
        let e = Effect {
            full_width: w as u32,
            full_height: h as u32,
            ..e
        };
        tilt(
            &e,
            &mut Pixels {
                data: &mut data,
                width: w,
                height: h,
            },
        )
        .unwrap();
        let contrast = |y: usize| (data[(y * w + 10) * 3] - data[(y * w + 11) * 3]).abs();
        assert!(contrast(60) > 0.7, "the band {}", contrast(60));
        assert!(contrast(5) < 0.2, "the top {}", contrast(5));
    }
}
