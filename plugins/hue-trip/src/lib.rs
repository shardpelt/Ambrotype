// SPDX-License-Identifier: GPL-3.0-or-later
//! Hue trip: colours turned round the wheel by their brightness.
//!
//! A colour effect: the app asks it once for a grid of colours and renders
//! the result as a colour table. The turn is a rotation round the grey
//! axis, so greys stay grey and the brightness stays close.

use amb_plugin_sdk::Effect;
use std::f32::consts::TAU;

fn luma(c: [f32; 3]) -> f32 {
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}

/// `c` turned by `deg` round the grey axis (Rodrigues' rotation).
fn turn(c: [f32; 3], deg: f32) -> [f32; 3] {
    let (s, k) = deg.to_radians().sin_cos();
    let a = (1.0 - k) / 3.0;
    let b = s / 3f32.sqrt();
    let m = [
        [k + a, a - b, a + b],
        [a + b, k + a, a - b],
        [a - b, a + b, k + a],
    ];
    [0, 1, 2].map(|i| m[i][0] * c[0] + m[i][1] * c[1] + m[i][2] * c[2])
}

pub fn trip(e: &Effect, c: [f32; 3]) -> [f32; 3] {
    let bands = e.number("bands").round() as i32;
    let mut c = c;
    if bands >= 2 {
        let n = (bands - 1) as f32;
        c = c.map(|v| (v.clamp(0.0, 1.0) * n).round() / n);
    }
    let y = luma(c).clamp(0.0, 1.0);
    let deg = e.number("turn") as f32 + 360.0 * e.number("cycles") as f32 * y;
    let t = turn(c, deg);
    let colour = (e.number("colour") / 100.0) as f32;
    let g = luma(t);
    let t = t.map(|v| (g + (v - g) * colour).clamp(0.0, 1.0));
    // Paint: a rainbow by tone mixed in, so greys and dull colours trip
    // too; at the pixel's own brightness.
    let paint = (e.number("paint") / 100.0) as f32;
    if paint <= 0.0 {
        return t;
    }
    let h = deg / 360.0;
    let r = [0.0, 1.0 / 3.0, 2.0 / 3.0].map(|o| 0.5 + 0.5 * (TAU * (h - o)).cos());
    let k = y / luma(r).max(1e-4);
    let r = r.map(|v| (v * k).clamp(0.0, 1.0));
    [0, 1, 2].map(|i| t[i] + (r[i] - t[i]) * paint)
}

amb_plugin_sdk::export_effect!(color: trip);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turns_colours_and_keeps_greys() {
        let e = Effect::for_test(
            serde_json::json!({ "turn": 120, "cycles": 0, "colour": 100, "bands": 0, "paint": 0 }),
            1,
            1,
        );
        // A third of the way round: red becomes green.
        let r = trip(&e, [0.8, 0.1, 0.1]);
        assert!(r[1] > r[0] && r[1] > r[2], "{r:?}");
        let g = trip(&e, [0.4, 0.4, 0.4]);
        assert!(g.iter().all(|v| (v - 0.4).abs() < 1e-4), "{g:?}");
        // Posterised: few tones.
        let p = Effect::for_test(
            serde_json::json!({ "turn": 0, "cycles": 0, "colour": 100, "bands": 3, "paint": 0 }),
            1,
            1,
        );
        assert_eq!(trip(&p, [0.3, 0.3, 0.3]), [0.5, 0.5, 0.5]);
        // Paint colours a grey, at its brightness.
        let q = Effect::for_test(
            serde_json::json!({ "turn": 0, "cycles": 1, "colour": 100, "bands": 0, "paint": 100 }),
            1,
            1,
        );
        let c = trip(&q, [0.4, 0.4, 0.4]);
        assert!(
            c[0] - c[2] > 0.1 || c[2] - c[0] > 0.1 || (c[1] - c[0]).abs() > 0.1,
            "{c:?}"
        );
        assert!((luma(c) - 0.4).abs() < 0.06, "{c:?}");
    }
}
