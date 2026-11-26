// SPDX-License-Identifier: GPL-3.0-or-later
//! Halftone: the photo as print. Dither (two inks, random dots), Rings and
//! Lines (a line screen whose thickness follows the shadows) and Rainbow
//! (false colour bands with a relief).
//!
//! A pixel effect that needs the whole photo (`"whole": true`) only for its
//! coordinates: every position is in the whole photo's pixels, so a preview
//! and an export match. This file holds the dispatch, the inks and the
//! helpers the styles share; the styles are in `screens.rs` and `rainbow.rs`.

mod rainbow;
mod screens;

use amb_plugin_sdk::{Effect, Pixels};

// Ranges repeat those in plugin.json.
/// Spacing as a percent of the photo's short side.
const SIZE: (f64, f64) = (0.1, 5.0);
const CONTRAST: (f64, f64) = (0.0, 200.0);
/// Contrast 100 leaves the brightness as it is.
const CONTRAST_NEUTRAL: f32 = 100.0;
/// The brightness the contrast turns about: mid grey.
const CONTRAST_PIVOT: f32 = 0.5;
const PERCENT: f32 = 100.0;

/// Rec. 709 luma weights, on the sRGB values.
const LUMA: [f32; 3] = [0.2126, 0.7152, 0.0722];

/// The inks: name, dark, light. "Photo colour" is a special case, see
/// [`inks`].
const INKS: [(&str, [f32; 3], [f32; 3]); 5] = [
    ("Game Boy", [0.25, 0.32, 0.24], [0.77, 0.94, 0.85]),
    ("Black and white", [0.05, 0.05, 0.05], [0.97, 0.97, 0.95]),
    ("Blueprint", [0.08, 0.2, 0.45], [0.85, 0.92, 1.0]),
    ("Riso pink", [0.1, 0.08, 0.2], [1.0, 0.45, 0.65]),
    ("Photo colour", [0.0, 0.0, 0.0], [1.0, 1.0, 1.0]),
];
const PHOTO_COLOUR: &str = "Photo colour";

/// Brightness of an sRGB pixel, 0..1.
fn luma(c: [f32; 3]) -> f32 {
    c[0] * LUMA[0] + c[1] * LUMA[1] + c[2] * LUMA[2]
}

/// `v` with the contrast setting applied about mid grey, kept in 0..1.
fn contrasted(v: f32, contrast: f32) -> f32 {
    ((v - CONTRAST_PIVOT) * contrast / CONTRAST_NEUTRAL + CONTRAST_PIVOT).clamp(0.0, 1.0)
}

/// The dark and light ink for a pixel `c`: the chosen pair, or, for
/// "Photo colour", black and the photo's own colour.
fn inks(name: &str, c: [f32; 3]) -> ([f32; 3], [f32; 3]) {
    let (_, dark, light) = INKS.iter().find(|i| i.0 == name).unwrap_or(&INKS[0]);
    if name == PHOTO_COLOUR {
        (*dark, c)
    } else {
        (*dark, *light)
    }
}

/// Spacing of the dots or lines in pixels of the whole photo, at least `min`.
fn spacing(e: &Effect, min: f32) -> f32 {
    let short = e.full_width.min(e.full_height).max(1) as f32;
    let size = e.number("size").clamp(SIZE.0, SIZE.1) as f32;
    (short * size / PERCENT).max(min)
}

/// The settings every style reads.
struct Common {
    contrast: f32,
    strength: f32,
}

impl Common {
    fn of(e: &Effect) -> Common {
        Common {
            contrast: e.number("contrast").clamp(CONTRAST.0, CONTRAST.1) as f32,
            strength: (e.number("strength") as f32 / PERCENT).clamp(0.0, 1.0),
        }
    }

    /// `original` mixed towards `result` by the strength.
    fn mix(&self, original: [f32; 3], result: [f32; 3]) -> [f32; 3] {
        if self.strength >= 1.0 {
            return result; // exact, not off by rounding
        }
        [0, 1, 2].map(|k| original[k] + (result[k] - original[k]) * self.strength)
    }
}

pub fn halftone(e: &Effect, px: &mut Pixels) -> Result<(), String> {
    if px.width == 0 || px.height == 0 {
        return Ok(());
    }
    let common = Common::of(e);
    if common.strength <= 0.0 {
        return Ok(());
    }
    match e.choice("style") {
        "Rings" => screens::rings(e, px, &common),
        "Lines" => screens::lines(e, px, &common),
        "Rainbow" => rainbow::rainbow(e, px, &common),
        _ => screens::dither(e, px, &common),
    }
    Ok(())
}

amb_plugin_sdk::export_effect!(pixels: halftone);

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    /// An effect with every parameter at its default, `over` on top.
    pub(crate) fn effect(over: Value, w: u32, h: u32) -> Effect {
        let mut p = json!({
            "style": "Dither", "ink": "Game Boy", "size": 0.3, "contrast": 100,
            "fringe": 0, "angle": 45, "x": 50, "y": 50, "bands": 4, "strength": 100
        });
        for (k, v) in over.as_object().cloned().unwrap_or_default() {
            p[k] = v;
        }
        Effect::for_test(p, w, h)
    }

    /// A left-to-right grey ramp.
    pub(crate) fn ramp(w: usize, h: usize) -> Vec<f32> {
        (0..w * h)
            .flat_map(|i| {
                let v = (i % w) as f32 / (w - 1) as f32;
                [v, v, v]
            })
            .collect()
    }

    pub(crate) fn run(e: &Effect, data: &mut [f32], w: usize, h: usize) {
        halftone(
            e,
            &mut Pixels {
                data,
                width: w,
                height: h,
            },
        )
        .unwrap();
    }

    #[test]
    fn strength_zero_leaves_every_style_unchanged() {
        let (w, h) = (40, 30);
        for style in ["Dither", "Rings", "Lines", "Rainbow"] {
            let mut data = ramp(w, h);
            let before = data.clone();
            let e = effect(json!({ "style": style, "strength": 0 }), w as u32, h as u32);
            run(&e, &mut data, w, h);
            assert_eq!(data, before, "{style}");
        }
    }

    #[test]
    fn photo_colour_ink_uses_the_pixel() {
        let (dark, light) = inks("Photo colour", [0.2, 0.4, 0.6]);
        assert_eq!(dark, [0.0; 3]);
        assert_eq!(light, [0.2, 0.4, 0.6]);
    }

    #[test]
    fn every_style_stays_in_range() {
        let (w, h) = (50, 40);
        for style in ["Dither", "Rings", "Lines", "Rainbow"] {
            let mut data = ramp(w, h);
            let e = effect(
                json!({ "style": style, "fringe": 60, "size": 1 }),
                w as u32,
                h as u32,
            );
            run(&e, &mut data, w, h);
            assert!(
                data.iter()
                    .all(|v| v.is_finite() && (0.0..=1.0).contains(v)),
                "{style}"
            );
        }
    }
}
