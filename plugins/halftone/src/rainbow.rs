// SPDX-License-Identifier: GPL-3.0-or-later
//! Rainbow: false colour by brightness in repeating bands, with an emboss
//! relief along edges and a fine jitter per row, very saturated.

use super::{Common, contrasted, luma};
use amb_plugin_sdk::{Effect, Pixels};

/// Range of the bands setting, as in plugin.json.
const BANDS: (f64, f64) = (1.0, 12.0);
const DEGREES: f32 = 360.0;
/// Hue shift per unit of emboss, in degrees: edges bend the colours.
const EMBOSS_HUE: f32 = 40.0;
/// Brightness added per unit of emboss: the relief's highlight and shadow.
const EMBOSS_VALUE: f32 = 0.8;
/// Emboss gain on the luma difference across a pixel's diagonal.
const EMBOSS_GAIN: f32 = 2.0;
/// Each row's hue is moved by up to this many degrees either way.
const JITTER_DEG: f32 = 6.0;
/// Value runs from this at black up to full at white.
const VALUE_FLOOR: f32 = 0.35;

pub(crate) fn rainbow(e: &Effect, px: &mut Pixels, common: &Common) {
    let bands = e.number("bands").clamp(BANDS.0, BANDS.1) as f32;
    let (w, h) = (px.width, px.height);
    let v: Vec<f32> = px
        .data
        .chunks_exact(3)
        .map(|c| contrasted(luma([c[0], c[1], c[2]]), common.contrast))
        .collect();
    let at = |x: isize, y: isize| {
        v[y.clamp(0, h as isize - 1) as usize * w + x.clamp(0, w as isize - 1) as usize]
    };
    for y in 0..h {
        // The same jitter for the whole row, whatever part is rendered.
        let jitter = (e.noise(0, e.y + y as u32) - 0.5) * 2.0 * JITTER_DEG;
        for x in 0..w {
            let (xi, yi) = (x as isize, y as isize);
            let emboss = (at(xi + 1, yi + 1) - at(xi - 1, yi - 1)) * EMBOSS_GAIN;
            let level = v[y * w + x];
            let hue = level * bands * DEGREES + emboss * EMBOSS_HUE + jitter;
            let value =
                (VALUE_FLOOR + (1.0 - VALUE_FLOOR) * level + emboss * EMBOSS_VALUE).clamp(0.0, 1.0);
            let original = px.get(x, y);
            px.set(x, y, common.mix(original, hsv(hue, 1.0, value)));
        }
    }
}

/// HSV (hue in degrees) to sRGB.
fn hsv(hue: f32, s: f32, v: f32) -> [f32; 3] {
    let h = hue.rem_euclid(DEGREES) / 60.0;
    let f = |n: f32| {
        let k = (n + h) % 6.0;
        v - v * s * k.min(4.0 - k).clamp(0.0, 1.0)
    };
    [f(5.0), f(3.0), f(1.0)]
}

#[cfg(test)]
mod tests {
    use super::super::tests::{effect, ramp, run};
    use serde_json::json;

    #[test]
    fn a_grey_ramp_gives_several_hues() {
        let (w, h) = (120, 8);
        let mut data = ramp(w, h);
        let e = effect(
            json!({ "style": "Rainbow", "bands": 4, "contrast": 120 }),
            w as u32,
            h as u32,
        );
        run(&e, &mut data, w, h);
        assert!(
            data.iter()
                .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
        );
        let mut hues = std::collections::BTreeSet::new();
        for c in data.chunks_exact(3) {
            let (mx, mn) = (c[0].max(c[1]).max(c[2]), c[0].min(c[1]).min(c[2]));
            if mx - mn > 0.2 {
                let top = (0..3).find(|&k| c[k] == mx).unwrap_or(0);
                hues.insert((top, c[(top + 1) % 3] > c[(top + 2) % 3]));
            }
        }
        assert!(hues.len() >= 4, "{hues:?}");
    }
}
