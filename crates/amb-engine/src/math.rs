// SPDX-License-Identifier: GPL-3.0-or-later
//! Small numeric helpers shared by the pipeline's stages.

/// 0 below `e0`, 1 above `e1`, a smooth S between. Equal edges give a
/// step at `e0`.
pub fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    if e1 <= e0 {
        return if x >= e0 { 1.0 } else { 0.0 };
    }
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// [`smoothstep`] in `f64`.
pub fn smoothstep_f64(e0: f64, e1: f64, x: f64) -> f64 {
    if e1 <= e0 {
        return if x >= e0 { 1.0 } else { 0.0 };
    }
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
