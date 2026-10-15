// SPDX-License-Identifier: GPL-3.0-or-later
//! Duotone: the photo in two inks, one for the shadows and one for the
//! highlights, keeping each pixel's brightness.
//!
//! A colour effect: the app asks it once for a grid of colours and renders
//! the result as a colour table.

use amb_plugin_sdk::Effect;

fn luma(c: [f32; 3]) -> f32 {
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}

/// A fully saturated colour of `hue` (degrees), mixed with grey by `sat`.
fn ink(hue: f32, sat: f32) -> [f32; 3] {
    let h = (hue.rem_euclid(360.0)) / 60.0;
    let x = 1.0 - (h % 2.0 - 1.0).abs();
    let pure = match h as u32 {
        0 => [1.0, x, 0.0],
        1 => [x, 1.0, 0.0],
        2 => [0.0, 1.0, x],
        3 => [0.0, x, 1.0],
        4 => [x, 0.0, 1.0],
        _ => [1.0, 0.0, x],
    };
    pure.map(|v| 0.5 + (v - 0.5) * sat)
}

pub fn duotone(e: &Effect, c: [f32; 3]) -> [f32; 3] {
    let sat = (e.number("colour") / 100.0) as f32;
    let balance = (e.number("balance") / 100.0) as f32;
    let y = luma(c).clamp(0.0, 1.0);
    // Where along the ramp: balance moves the midpoint.
    let t = y.powf(2f32.powf(balance)).clamp(0.0, 1.0);
    let (a, b) = (
        ink(e.number("shadows") as f32, sat),
        ink(e.number("highlights") as f32, sat),
    );
    let tint = [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * t);
    // The same brightness as the original pixel.
    let k = y / luma(tint).max(1e-4);
    tint.map(|v| (v * k).clamp(0.0, 1.0))
}

amb_plugin_sdk::export_effect!(color: duotone);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_brightness_and_tints_shadows_and_highlights() {
        let e = Effect::for_test(
            serde_json::json!({ "shadows": 220, "highlights": 40, "colour": 60, "balance": 0 }),
            1,
            1,
        );
        for c in [[0.2, 0.3, 0.4], [0.8, 0.7, 0.6], [0.5, 0.5, 0.5]] {
            let out = duotone(&e, c);
            assert!((luma(out) - luma(c)).abs() < 0.02, "{c:?} -> {out:?}");
        }
        let dark = duotone(&e, [0.15, 0.15, 0.15]);
        let light = duotone(&e, [0.85, 0.85, 0.85]);
        assert!(dark[2] > dark[0], "blue shadows {dark:?}");
        assert!(light[0] > light[2], "gold highlights {light:?}");
        // No colour: black and white.
        let grey = Effect::for_test(serde_json::json!({ "colour": 0 }), 1, 1);
        let g = duotone(&grey, [0.9, 0.2, 0.1]);
        assert!((g[0] - g[1]).abs() < 1e-4 && (g[1] - g[2]).abs() < 1e-4);
    }
}
