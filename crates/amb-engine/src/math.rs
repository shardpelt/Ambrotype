// SPDX-License-Identifier: GPL-3.0-or-later
//! Small numeric helpers shared by the pipeline's stages.

use rayon::prelude::*;

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

/// Block-averaged copy of an interleaved RGB image (about `target` samples).
pub(crate) fn downsample(
    rgb: &[f32],
    w: usize,
    h: usize,
    target: usize,
) -> (Vec<[f64; 3]>, usize, usize) {
    let block = (((w * h) as f64 / target as f64).sqrt().round() as usize).max(1);
    let (bw, bh) = ((w / block).max(1), (h / block).max(1));
    let blocks: Vec<[f64; 3]> = (0..bw * bh)
        .into_par_iter()
        .map(|i| {
            let (bx, by) = (i % bw, i / bw);
            let mut acc = [0.0f64; 3];
            let mut n = 0.0f64;
            for y in by * block..((by + 1) * block).min(h) {
                for x in bx * block..((bx + 1) * block).min(w) {
                    let p = &rgb[(y * w + x) * 3..(y * w + x) * 3 + 3];
                    for c in 0..3 {
                        acc[c] += f64::from(p[c]);
                    }
                    n += 1.0;
                }
            }
            acc.map(|v| v / n.max(1.0))
        })
        .collect();
    (blocks, bw, bh)
}

pub(crate) fn percentile(values: &mut [f64], q: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let i = ((values.len() - 1) as f64 * q.clamp(0.0, 1.0)).round() as usize;
    *values.select_nth_unstable_by(i, f64::total_cmp).1
}
