// SPDX-License-Identifier: GPL-3.0-or-later
//! Holographic: rainbow foil screened over the photo.
//!
//! A pixel effect. The bands run across the whole photo (positions in
//! whole-photo coordinates, so a photo split up to render matches), bend
//! with each pixel's brightness, and show most in the bright parts, as
//! foil catches the light.

use amb_plugin_sdk::{Effect, Pixels};
use std::f32::consts::TAU;

/// A smooth rainbow: `t` 0..1 goes once round red, green and blue.
fn rainbow(t: f32) -> [f32; 3] {
    [0.0, 1.0 / 3.0, 2.0 / 3.0].map(|o| 0.5 + 0.5 * (TAU * (t - o)).cos())
}

pub fn foil(e: &Effect, px: &mut Pixels) -> Result<(), String> {
    let strength = (e.number("strength") / 100.0) as f32;
    if strength <= 0.0 {
        return Ok(());
    }
    let bands = e.number("bands") as f32;
    let (sin, cos) = (e.number("angle") as f32).to_radians().sin_cos();
    let follow = (e.number("follow") / 100.0) as f32;
    let shift = (e.number("shift") / 360.0) as f32;
    let pastel = (e.number("pastel") / 100.0) as f32;
    let (fw, fh) = (e.full_width.max(1) as f32, e.full_height.max(1) as f32);
    // Bands are measured along the longer side, so they're as wide on a
    // portrait photo as on a landscape one.
    let long = fw.max(fh);
    // The rainbow as a table, read with a linear step between entries:
    // three cosines a pixel would be most of the effect's time.
    const N: usize = 256;
    let table: Vec<[f32; 3]> = (0..=N)
        .map(|i| rainbow(i as f32 / N as f32).map(|k| k + (1.0 - k) * pastel))
        .collect();
    let look = |t: f32| {
        let f = t.rem_euclid(1.0) * N as f32;
        let i = (f as usize).min(N - 1);
        let w = f - i as f32;
        let (a, b) = (table[i], table[i + 1]);
        [0, 1, 2].map(|k| a[k] + (b[k] - a[k]) * w)
    };
    // Along a row the band position grows by the same step each pixel.
    let dt = bands * cos / long;
    let width = px.width;
    for (y, row) in px.data.chunks_exact_mut(width * 3).enumerate() {
        let v = (e.y as f32 + y as f32 + 0.5) / long;
        let mut along = bands * ((e.x as f32 + 0.5) / long * cos + v * sin) + shift;
        for c in row.chunks_exact_mut(3) {
            let l = (0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]).clamp(0.0, 1.0);
            let r = look(along + follow * 3.0 * l);
            along += dt;
            // Foil shines where the light is: a little in the shadows.
            let w = strength * (0.3 + 0.7 * l);
            let tint = 0.35 + l;
            for i in 0..3 {
                let screen = 1.0 - (1.0 - c[i]) * (1.0 - r[i] * 0.85);
                // Mixed with the colour itself, so it reads as a tint and
                // not only as a brighter photo.
                let tinted = 0.5 * screen + 0.5 * r[i] * tint;
                c[i] = (c[i] + (tinted - c[i]) * w).clamp(0.0, 1.0);
            }
        }
    }
    Ok(())
}

amb_plugin_sdk::export_effect!(pixels: foil);

#[cfg(test)]
mod tests {
    use super::*;

    /// A `w` × `h` part of a `full`-sized photo starting at row `y`; the
    /// input depends on the pixel's place in the whole photo.
    fn run(params: &serde_json::Value, w: usize, h: usize, y: u32, full: (u32, u32)) -> Vec<f32> {
        let mut data: Vec<f32> = (0..w * h * 3)
            .map(|i| ((i / 3 % w + (i / 3 / w + y as usize) * 5 + i % 3) % 7) as f32 / 8.0)
            .collect();
        let mut e = Effect::for_test(params.clone(), full.0, full.1);
        e.y = y;
        foil(
            &e,
            &mut Pixels {
                data: &mut data,
                width: w,
                height: h,
            },
        )
        .unwrap();
        data
    }

    #[test]
    fn colours_a_grey_photo_in_bands() {
        let p = serde_json::json!({ "strength": 80, "bands": 4, "angle": 0, "follow": 0, "shift": 0, "pastel": 0 });
        let (w, h) = (64, 8);
        let mut g = vec![0.6f32; w * h * 3];
        let e = Effect::for_test(p, w as u32, h as u32);
        foil(
            &e,
            &mut Pixels {
                data: &mut g,
                width: w,
                height: h,
            },
        )
        .unwrap();
        let at = |x: usize| [g[x * 3], g[x * 3 + 1], g[x * 3 + 2]];
        let spread = |c: [f32; 3]| {
            c.iter().cloned().fold(0.0, f32::max) - c.iter().cloned().fold(1.0, f32::min)
        };
        assert!(spread(at(3)) > 0.1, "{:?}", at(3));
        assert!(
            (0..3).any(|i| (at(3)[i] - at(11)[i]).abs() > 0.1),
            "{:?} {:?}",
            at(3),
            at(11)
        );
    }

    #[test]
    fn is_the_same_in_parts_and_nothing_at_zero() {
        let p = serde_json::json!({ "strength": 70, "bands": 6, "angle": 30, "follow": 50, "shift": 0, "pastel": 30 });
        let whole = run(&p, 32, 16, 0, (32, 16));
        let bottom = run(&p, 32, 8, 8, (32, 16));
        assert_eq!(&whole[8 * 32 * 3..], &bottom[..]);
        let off = serde_json::json!({ "strength": 0 });
        let plain = run(&off, 4, 4, 0, (4, 4));
        let mut e = Effect::for_test(off, 4, 4);
        e.y = 0;
        let mut again = plain.clone();
        foil(
            &e,
            &mut Pixels {
                data: &mut again,
                width: 4,
                height: 4,
            },
        )
        .unwrap();
        assert_eq!(plain, again);
    }
}
