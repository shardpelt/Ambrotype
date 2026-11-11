// SPDX-License-Identifier: GPL-3.0-or-later
//! Heat camera: false colour by brightness, as a thermal camera shows
//! heat (dark is cold, bright is hot).
//!
//! A colour effect: the app asks it once for a grid of colours and renders
//! the result as a colour table.

use amb_plugin_sdk::Effect;

/// Each palette, cold to hot, as evenly spaced stops.
fn palette(name: &str) -> &'static [[f32; 3]] {
    match name {
        "Rainbow" => &[
            [0.0, 0.0, 0.15],
            [0.0, 0.0, 1.0],
            [0.0, 0.9, 1.0],
            [0.0, 1.0, 0.0],
            [1.0, 1.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 1.0],
        ],
        "White hot" => &[[0.0, 0.0, 0.0], [1.0, 1.0, 1.0]],
        "Black hot" => &[[1.0, 1.0, 1.0], [0.0, 0.0, 0.0]],
        "Arctic" => &[
            [0.0, 0.02, 0.12],
            [0.0, 0.25, 0.65],
            [0.2, 0.7, 1.0],
            [0.85, 0.95, 1.0],
            [1.0, 0.8, 0.3],
            [1.0, 0.45, 0.0],
        ],
        // Ironbow: black, deep blue, purple, red, orange, yellow, white.
        _ => &[
            [0.0, 0.0, 0.0],
            [0.12, 0.0, 0.4],
            [0.5, 0.0, 0.6],
            [0.85, 0.12, 0.2],
            [1.0, 0.5, 0.0],
            [1.0, 0.88, 0.15],
            [1.0, 1.0, 1.0],
        ],
    }
}

/// The colour at `t` (0–1) along the stops.
fn along(stops: &[[f32; 3]], t: f32) -> [f32; 3] {
    let f = t.clamp(0.0, 1.0) * (stops.len() - 1) as f32;
    let i = (f as usize).min(stops.len() - 2);
    let w = f - i as f32;
    [0, 1, 2].map(|k| stops[i][k] + (stops[i + 1][k] - stops[i][k]) * w)
}

pub fn heat(e: &Effect, c: [f32; 3]) -> [f32; 3] {
    let y = (0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]).clamp(0.0, 1.0);
    let contrast = (e.number("contrast") / 100.0) as f32;
    let shift = (e.number("shift") / 100.0) as f32;
    let cycles = e.number("cycles").max(1.0) as f32;
    // Contrast round the middle, then hotter or colder.
    let t = ((y - 0.5) * contrast + 0.5 + shift).clamp(0.0, 1.0);
    // Past one cycle the palette repeats, there and back, so rings meet
    // without a jump.
    let t = if cycles > 1.0 {
        let u = (t * cycles).rem_euclid(2.0);
        if u > 1.0 { 2.0 - u } else { u }
    } else {
        t
    };
    along(palette(e.choice("palette")), t)
}

amb_plugin_sdk::export_effect!(color: heat);

#[cfg(test)]
mod tests {
    use super::*;

    /// The manifest's defaults with `palette` and more.
    fn effect(extra: serde_json::Value) -> Effect {
        let mut p =
            serde_json::json!({ "palette": "Ironbow", "contrast": 100, "shift": 0, "cycles": 1 });
        for (k, v) in extra.as_object().unwrap() {
            p[k] = v.clone();
        }
        Effect::for_test(p, 1, 1)
    }

    #[test]
    fn dark_is_cold_and_bright_is_hot() {
        let e = effect(serde_json::json!({ "palette": "Ironbow", "contrast": 100 }));
        let (cold, hot) = (heat(&e, [0.02, 0.02, 0.02]), heat(&e, [0.98, 0.98, 0.98]));
        assert!(cold.iter().sum::<f32>() < 0.2, "{cold:?}");
        assert!(hot.iter().all(|v| *v > 0.85), "{hot:?}");
        let mid = heat(&e, [0.5, 0.5, 0.5]);
        assert!(mid[0] > mid[1], "the middle is red-ish: {mid:?}");
        let black_hot = effect(serde_json::json!({ "palette": "Black hot" }));
        assert!(heat(&black_hot, [1.0, 1.0, 1.0])[0] < 0.05);
    }

    #[test]
    fn bands_repeat_the_palette() {
        let e = effect(serde_json::json!({ "palette": "White hot", "cycles": 2 }));
        // Halfway up is the top of the first cycle: white; the end is black again.
        assert!(heat(&e, [0.5, 0.5, 0.5])[0] > 0.95);
        let end = heat(&e, [1.0, 1.0, 1.0]);
        assert!(end[0] < 0.05, "{end:?}");
    }
}
