// SPDX-License-Identifier: GPL-3.0-or-later
//! Small numeric helpers shared by the pipeline's stages.

/// 0 below `e0`, 1 above `e1`, a smooth S between. Equal edges give a
/// step at `e0`.
#[inline]
pub fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    if e1 <= e0 {
        return if x >= e0 { 1.0 } else { 0.0 };
    }
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// [`smoothstep`] in `f64`.
#[inline]
pub fn smoothstep_f64(e0: f64, e1: f64, x: f64) -> f64 {
    if e1 <= e0 {
        return if x >= e0 { 1.0 } else { 0.0 };
    }
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Linear light to the sRGB transfer curve (0..1 to 0..1).
#[inline]
pub(crate) fn srgb_encode(v: f32) -> f32 {
    if v <= 0.003_130_8 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    }
}

/// The sRGB transfer curve back to linear light.
#[inline]
pub(crate) fn srgb_decode(v: f32) -> f32 {
    if v <= 0.040_45 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

/// Bilinear source position of full-resolution index `i` in a grid downscaled
/// by `f` with `n` cells (cell centres aligned): (lower, upper, weight of upper).
pub(crate) fn lerp_position(i: usize, f: usize, n: usize) -> (usize, usize, f32) {
    let s = ((i as f32 + 0.5) / f as f32 - 0.5).clamp(0.0, (n - 1) as f32);
    let lo = (s.floor() as usize).min(n - 1);
    (lo, (lo + 1).min(n - 1), s - lo as f32)
}
