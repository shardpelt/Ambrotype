//! Highlights and Shadows as local, edge-aware operators.
//!
//! **Model v2 (ours, not Adobe's).** Lightroom's PV2012 Highlights and Shadows
//! adapt to the image: lifting the shadows brightens dark areas without
//! flattening the detail in them, and without a glow along the edge of a
//! bright sky (docs/research/05 §4). Here:
//!
//! 1. `L` = encoded luminance of the display-linear image.
//! 2. `base` = self-guided filter of `L` (the fast guided filter Clarity uses),
//!    with a radius of [`ToneStrengths::local_radius_frac`] of the diagonal and
//!    an `eps` that keeps strong edges in the base.
//! 3. The region curve ([`tone::region_lut`]) is evaluated on the base, and its
//!    change is added to the pixel: `L' = L + (curve(base) - base)`. A whole
//!    area moves by the same amount, so its detail (`L - base`) is kept, and
//!    since the base stops at strong edges, so does the change.
//!
//! The change is limited to the room before white or black, and the pixel's
//! colour is scaled with its luminance (hue preserving), as in [`crate::local`].

use rayon::prelude::*;

use crate::buffer::{Image, luminance_weights};
use crate::local::{
    CLARITY_LOW_RES_RADIUS, DECODE_LUT_SIZE, GuidedBase, LEVEL_BOUND, REFERENCE_DIAGONAL,
    apply_luminance, channel_cap, decode_level, filter_level, headroom_limit, lookup,
    reference_scale, scaled_radius, srgb_decode,
};
use crate::tone::{self, ToneControls, ToneStrengths};

const LUT: usize = 1025;

/// Apply Highlights and Shadows in place, with spatial scales relative to
/// this image's size.
pub fn apply(img: &mut Image, c: &ToneControls, k: &ToneStrengths) {
    let scale = reference_scale(img.width, img.height);
    apply_scaled(img, c, k, scale);
}

/// As [`apply`], with an explicit spatial scale (1.0 = the 24 MP reference).
pub fn apply_scaled(img: &mut Image, c: &ToneControls, k: &ToneStrengths, scale: f32) {
    if (c.highlights == 0.0 && c.shadows == 0.0) || img.pixel_count() == 0 {
        return;
    }
    let (w, h) = (img.width, img.height);
    let lw = luminance_weights();
    let l: Vec<f32> = img
        .rgb
        .par_chunks(3)
        .map(|p| filter_level(lw[0] * p[0] + lw[1] * p[1] + lw[2] * p[2]))
        .collect();
    let base = GuidedBase::new(
        &l,
        w,
        h,
        scaled_radius(k.local_radius_frac as f32 * REFERENCE_DIAGONAL, scale),
        k.local_edge_eps as f32,
        CLARITY_LOW_RES_RADIUS,
    );
    // The change at each base level.
    let change: Vec<f32> = tone::region_lut(c, k, LUT)
        .iter()
        .enumerate()
        .map(|(i, v)| v - i as f32 / (LUT - 1) as f32)
        .collect();
    let decode_lut: Vec<f32> = (0..DECODE_LUT_SIZE)
        .map(|i| srgb_decode(i as f32 / (DECODE_LUT_SIZE - 1) as f32))
        .collect();
    img.rgb.par_chunks_mut(3 * w).enumerate().for_each_init(
        || vec![0.0f32; w],
        |base_row, (y, row)| {
            let src = &l[y * w..(y + 1) * w];
            base.row(y, src, base_row);
            for ((px, &lv), &b) in row.chunks_exact_mut(3).zip(src).zip(base_row.iter()) {
                let d = lookup(&change, b);
                if d == 0.0 || lv.abs() >= LEVEL_BOUND || !px.iter().all(|v| v.is_finite()) {
                    continue;
                }
                let mut y_new = decode_level(&decode_lut, lv + headroom_limit(lv, d));
                if d > 0.0 {
                    y_new = y_new.min(channel_cap(px, lw, d, &decode_lut));
                }
                apply_luminance(px, lw, y_new);
            }
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local::srgb_encode as encode;
    use crate::tone::DEFAULT_STRENGTHS;

    /// Grey image from an encoded-luminance function.
    fn grey(w: usize, h: usize, f: impl Fn(usize, usize) -> f32 + Sync) -> Image {
        Image::from_fn(w, h, |x, y| [srgb_decode(f(x, y)); 3])
    }

    fn level(img: &Image, x: usize, y: usize) -> f32 {
        encode(img.luminance()[y * img.width + x])
    }

    fn tone(highlights: f64, shadows: f64) -> ToneControls {
        ToneControls {
            highlights,
            shadows,
            ..Default::default()
        }
    }

    /// A dark half with fine texture next to a bright half (a roof against the sky).
    fn scene() -> Image {
        grey(240, 120, |x, y| {
            let texture = if (x / 2 + y / 2) % 2 == 0 {
                0.03
            } else {
                -0.03
            };
            if x < 120 { 0.2 + texture } else { 0.85 }
        })
    }

    #[test]
    fn shadows_lift_dark_areas_and_keep_their_detail() {
        let mut img = scene();
        let before: Vec<f32> = (0..4).map(|x| level(&img, 40 + x, 60)).collect();
        apply_scaled(&mut img, &tone(0.0, 100.0), &DEFAULT_STRENGTHS, 0.05);
        let after: Vec<f32> = (0..4).map(|x| level(&img, 40 + x, 60)).collect();
        // Lifted…
        assert!(after[0] - before[0] > 0.04, "{before:?} → {after:?}");
        // …with the same texture amplitude (a global curve would compress it).
        let amp = |v: &[f32]| {
            v.iter().cloned().fold(f32::MIN, f32::max) - v.iter().cloned().fold(f32::MAX, f32::min)
        };
        assert!(
            (amp(&after) - amp(&before)).abs() < 0.01,
            "{before:?} → {after:?}"
        );
        // The bright half barely moves, right up to the edge (no glow).
        assert!(
            (level(&img, 124, 60) - 0.85).abs() < 0.01,
            "{}",
            level(&img, 124, 60)
        );
    }

    #[test]
    fn highlights_recover_bright_areas_only() {
        let mut img = scene();
        apply_scaled(&mut img, &tone(-100.0, 0.0), &DEFAULT_STRENGTHS, 0.05);
        assert!(level(&img, 200, 60) < 0.8, "{}", level(&img, 200, 60));
        assert!(
            (level(&img, 40, 60) - 0.23).abs() < 0.015,
            "{}",
            level(&img, 40, 60)
        );
    }

    #[test]
    fn neutral_and_flat_images_are_untouched() {
        let mut img = scene();
        let before = img.rgb.clone();
        apply_scaled(&mut img, &tone(0.0, 0.0), &DEFAULT_STRENGTHS, 0.05);
        assert_eq!(img.rgb, before);
        // White stays white, black stays black.
        let mut ends = grey(64, 64, |x, _| if x < 32 { 0.0 } else { 1.0 });
        apply_scaled(&mut ends, &tone(-100.0, 100.0), &DEFAULT_STRENGTHS, 0.05);
        assert!(level(&ends, 10, 10) < 1e-3 && level(&ends, 50, 10) > 0.999);
    }
}
