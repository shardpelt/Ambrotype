// SPDX-License-Identifier: GPL-3.0-or-later
//! Light leak: a glow bleeding in from one edge, screened over the photo.
//!
//! A pixel effect. Positions are worked out in whole-photo coordinates, so
//! the leak lands in the same place however the photo is split up to
//! render, and at any size.

use amb_plugin_sdk::{Effect, Pixels};

fn colour(name: &str) -> [f32; 3] {
    match name {
        "Red" => [1.0, 0.25, 0.1],
        "Pink" => [1.0, 0.35, 0.55],
        "Gold" => [1.0, 0.78, 0.3],
        _ => [1.0, 0.55, 0.15],
    }
}

/// A smooth bump: 1 at 0, 0 from 1.
fn falloff(d: f32) -> f32 {
    let t = (1.0 - d).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub fn leak(e: &Effect, px: &mut Pixels) -> Result<(), String> {
    let strength = (e.number("strength") / 100.0) as f32;
    if strength <= 0.0 {
        return Ok(());
    }
    let size = (e.number("size") / 100.0) as f32;
    let tint = colour(e.choice("colour"));
    let (fw, fh) = (e.full_width.max(1) as f32, e.full_height.max(1) as f32);
    // The leak's centre along its edge and its spread, from the seed and
    // the Variation number, so each photo (and each variation) differs.
    let mut rng = amb_plugin_sdk::Effect {
        seed: e.seed ^ (e.number("variation") as u64).wrapping_mul(0x9E37_79B9),
        ..e.clone()
    }
    .rng();
    let along = rng.range(0.2, 0.8) as f32;
    let spread = rng.range(0.6, 1.4) as f32;
    let streak = rng.range(0.0, 1.0) as f32;
    let edge = match e.choice("edge") {
        "Right" => 1,
        "Top" => 2,
        "Bottom" => 3,
        _ => 0,
    };
    let reach = size * 0.9 + 0.05;
    let streak_at = along + 0.15 * (streak - 0.5);
    for y in 0..px.height {
        for x in 0..px.width {
            // Whole-photo position, 0..1 on both axes.
            let u = (e.x as f32 + x as f32 + 0.5) / fw;
            let v = (e.y as f32 + y as f32 + 0.5) / fh;
            // Distance from the edge, and along it, in the edge's frame.
            let (across, pos) = match edge {
                1 => (1.0 - u, v),
                2 => (v, u),
                3 => (1.0 - v, u),
                _ => (u, v),
            };
            // Far from the edge: nothing to add.
            if across > reach * 1.6 {
                continue;
            }
            let band = falloff(((pos - along) / (0.35 * spread)).abs());
            let mut g = falloff(across / reach) * band;
            // A faint streak running into the photo.
            g += 0.35 * falloff(((pos - streak_at) / 0.04).abs()) * falloff(across / (reach * 1.6));
            let g = (g * strength).min(1.0);
            if g <= 0.0 {
                continue;
            }
            let c = px.get(x, y);
            // Screen blend: light added, never darkening.
            px.set(
                x,
                y,
                [0, 1, 2].map(|i| 1.0 - (1.0 - c[i]) * (1.0 - g * tint[i])),
            );
        }
    }
    Ok(())
}

amb_plugin_sdk::export_effect!(pixels: leak);

#[cfg(test)]
mod tests {
    use super::*;

    fn run(
        params: serde_json::Value,
        w: usize,
        h: usize,
        x: u32,
        y: u32,
        full: (u32, u32),
    ) -> Vec<f32> {
        let mut data = vec![0.2f32; w * h * 3];
        let mut e = Effect::for_test(params, full.0, full.1);
        e.seed = 7;
        e.x = x;
        e.y = y;
        leak(
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
    fn brightens_the_chosen_edge_and_is_the_same_in_parts() {
        let p = serde_json::json!({ "strength": 80, "size": 40, "edge": "Left", "colour": "Amber", "variation": 0 });
        let whole = run(p.clone(), 64, 48, 0, 0, (64, 48));
        // Left side brighter than the right, and never darker than before.
        let left: f32 = (0..48).map(|y| whole[(y * 64) * 3]).sum();
        let right: f32 = (0..48).map(|y| whole[(y * 64 + 63) * 3]).sum();
        assert!(left > right + 1.0, "{left} vs {right}");
        assert!(whole.iter().all(|v| *v >= 0.2 - 1e-6));
        // The bottom half rendered on its own matches.
        let bottom = run(p, 64, 24, 0, 24, (64, 48));
        assert_eq!(&whole[24 * 64 * 3..], &bottom[..]);
    }
}
