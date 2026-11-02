// SPDX-License-Identifier: GPL-3.0-or-later
//! Highlight roll-off (`amb:HighlightRolloff`), **ours**: highlights ease
//! into white the way a print from film does, instead of clipping.
//!
//! The base look maps each colour channel on its own and clips it near
//! display white, so a bright, saturated light (a sunset, a neon sign)
//! turns yellow or white abruptly. Here, in scene-linear light before the
//! base look, each pixel is scaled by one factor taken from its brightest
//! channel: below a knee nothing changes; above it the brightest channel
//! approaches a ceiling the base look still renders without clipping. One
//! factor for all three channels keeps the hue. Near the top the colour
//! then fades toward white, as paper does where its dyes run out.

use rayon::prelude::*;

use crate::buffer::{Image, luminance_weights};

/// Scene-linear value the brightest channel approaches (the base look
/// renders it at about 0.98 of white).
const CEILING: f32 = 4.0;
/// The knee at amount 100 (2 stops over 18 % grey); at amount 1 it is 3
/// stops higher.
const KNEE_AT_FULL: f32 = 0.72;
/// How much colour the very top loses at amount 100.
const FADE: f32 = 0.6;

/// The knee for `amount` (0..1): lower as the amount grows.
fn knee(amount: f32) -> f32 {
    KNEE_AT_FULL * 2f32.powf(3.0 * (1.0 - amount))
}

/// The brightest channel `m` after the roll-off: `m` up to the knee, then
/// an exponential approach to the ceiling with slope 1 at the knee (no
/// kink).
fn shoulder(m: f32, k: f32) -> f32 {
    if m <= k || k >= CEILING {
        return m;
    }
    let span = CEILING - k;
    k + span * (1.0 - (-(m - k) / span).exp())
}

/// Apply the roll-off at `amount` 0..100 to `img` (scene-linear ProPhoto).
pub fn apply(img: &mut Image, amount: f64) {
    let a = (amount / 100.0).clamp(0.0, 1.0) as f32;
    if a <= 0.0 {
        return;
    }
    let k = knee(a);
    let lum = luminance_weights();
    img.rgb.par_chunks_exact_mut(3).for_each(|p| {
        let m = p[0].max(p[1]).max(p[2]);
        // Also skips NaN.
        if m.is_nan() || m <= k {
            return;
        }
        let s = shoulder(m, k) / m;
        let c = [p[0] * s, p[1] * s, p[2] * s];
        // How far into the shoulder, 0..1, and the paper's fade there.
        let t = ((shoulder(m, k) - k) / (CEILING - k)).clamp(0.0, 1.0);
        let fade = FADE * a * t * t;
        let y = lum[0] * c[0] + lum[1] * c[1] + lum[2] * c[2];
        for i in 0..3 {
            p[i] = c[i] + (y - c[i]) * fade;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn midtones_stay_and_highlights_ease_in_without_a_kink() {
        let k = knee(1.0);
        assert!((shoulder(0.18, k) - 0.18).abs() < 1e-7);
        // Continuous with slope 1 at the knee.
        let d = (shoulder(k + 1e-3, k) - k) / 1e-3;
        assert!((d - 1.0).abs() < 1e-2, "{d}");
        // Always rising, and below the ceiling.
        let mut last = 0.0;
        for i in 1..200 {
            let v = shoulder(i as f32 * 0.1, k);
            assert!(v > last && v < CEILING);
            last = v;
        }
    }

    #[test]
    fn a_bright_colour_keeps_its_hue_and_fades_toward_white() {
        let mut img = Image {
            width: 2,
            height: 1,
            rgb: vec![8.0, 2.0, 0.5, 0.1, 0.05, 0.02],
        };
        apply(&mut img, 100.0);
        let p = &img.rgb[..3];
        assert!(p[0] < CEILING, "compressed: {p:?}");
        // Same order of channels, less spread (paler), still orange.
        assert!(p[0] > p[1] && p[1] > p[2]);
        assert!(p[2] / p[0] > 0.5 / 8.0, "faded toward white: {p:?}");
        // A dark pixel is untouched.
        assert_eq!(&img.rgb[3..], &[0.1, 0.05, 0.02]);
        // Nothing at 0.
        let mut same = Image {
            width: 1,
            height: 1,
            rgb: vec![8.0, 2.0, 0.5],
        };
        apply(&mut same, 0.0);
        assert_eq!(same.rgb, vec![8.0, 2.0, 0.5]);
    }
}
