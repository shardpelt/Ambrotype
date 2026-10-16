// SPDX-License-Identifier: GPL-3.0-or-later
//! Sharpening: a luminance unsharp mask with Detail's halo limit and an edge
//! mask (see the module docs above).

use super::*;

/// Sharpening as numbers (for other backends): the unsharp mask's `gain`
/// and blur `kernel` on encoded luminance, the halo window (radius `win`,
/// blended towards `win + 1` by `win_frac`) and `margin`, the luminance
/// ratio cap, and the edge mask when Masking is on.
#[derive(Debug, Clone, PartialEq)]
pub struct SharpenPlan {
    pub gain: f32,
    pub kernel: Vec<f32>,
    pub win: usize,
    pub win_frac: f32,
    pub margin: f32,
    pub max_ratio: f32,
    pub mask: Option<EdgeMaskPlan>,
}

/// The sharpening mask: smoothstep from `e0` to `e1` of the gradient
/// magnitude of the luminance blurred by `blur`, then blurred by `smooth`.
#[derive(Debug, Clone, PartialEq)]
pub struct EdgeMaskPlan {
    pub blur: crate::filters::GaussianPlan,
    pub smooth: crate::filters::GaussianPlan,
    pub e0: f32,
    pub e1: f32,
}

/// [`apply_sharpening`]'s numbers, or `None` when it does nothing.
pub fn sharpen_plan(p: &Sharpening, k: &DetailStrengths) -> Option<SharpenPlan> {
    if !p.amount.is_finite() || p.amount <= 0.0 {
        return None;
    }
    let gain = (p.amount.min(150.0) / 100.0 * k.sharpen_gain.max(0.0)) as f32;
    if !gain.is_finite() || gain <= 0.0 {
        return None;
    }
    let radius = slider(p.radius, 1.0, 0.5, 3.0);
    let detail = slider(p.detail, 25.0, 0.0, 100.0) / 100.0;
    let masking = slider(p.masking, 0.0, 0.0, 100.0) / 100.0;
    let sigma = (radius * k.sharpen_sigma_per_radius.max(0.0)).min(MAX_SHARPEN_SIGMA);
    let win_r = (radius * k.halo_window_per_radius.max(0.0)).clamp(1.0, MAX_HALO_WINDOW);
    let win = win_r.floor() as usize;
    Some(SharpenPlan {
        gain,
        kernel: gaussian_kernel(sigma as f32),
        win,
        win_frac: (win_r - win as f64) as f32,
        margin: halo_margin(detail, k),
        max_ratio: k.max_luminance_ratio.max(1.0) as f32,
        mask: (masking > 0.0).then(|| {
            let threshold = (k.mask_threshold_at_100.max(0.0)
                * masking.powf(k.mask_threshold_gamma.max(0.01)))
                as f32;
            let soft = k.mask_softness.clamp(0.0, 1.0) as f32;
            EdgeMaskPlan {
                blur: crate::filters::gaussian_plan(
                    k.mask_blur_sigma.clamp(0.0, MAX_BLUR_SIGMA) as f32
                ),
                smooth: crate::filters::gaussian_plan(
                    k.mask_smooth_sigma.clamp(0.0, MAX_BLUR_SIGMA) as f32,
                ),
                e0: threshold * (1.0 - soft),
                e1: threshold * (1.0 + soft),
            }
        }),
    })
}

pub fn apply_sharpening(img: &mut Image, p: &Sharpening, k: &DetailStrengths) {
    let (w, h) = (img.width, img.height);
    if w == 0 || h == 0 {
        return;
    }
    let Some(plan) = sharpen_plan(p, k) else {
        return;
    };
    let SharpenPlan {
        gain,
        kernel,
        win,
        win_frac,
        margin,
        max_ratio,
        mask,
    } = plan;
    let t = transfer();
    let lw = luminance_weights();

    // 1. Encoded luminance; the unsharp-mask blur is computed row by row below.
    let l = encoded_luminance(img, 0.0);

    // 2. Edge mask (Masking > 0 only).
    let mask = mask.map(|m| edge_mask(&l, w, h, &m));

    // 3. Sharpen with overshoot limited to the local min/max plus a margin. The
    //    window radius is fractional: the bounds are interpolated between the
    //    windows of radius `win` and `win + 1`, so they change continuously
    //    with the Radius slider.
    img.rgb.par_chunks_mut(w * 3).enumerate().for_each_init(
        || SharpenRows::new(w, kernel.len() / 2),
        |s, (y, row)| {
            convolve_row(&l, w, h, y, &kernel, &mut s.tmp, &mut s.pad, &mut s.blur);
            // Vertical min/max over the window's rows (and over one more row
            // either side for the outer window); horizontal per pixel below.
            let (y0, y1) = (y.saturating_sub(win), (y + win).min(h - 1));
            s.vmin.copy_from_slice(&l[y0 * w..(y0 + 1) * w]);
            s.vmax.copy_from_slice(&s.vmin);
            for yy in y0 + 1..=y1 {
                min_max_row(&mut s.vmin, &mut s.vmax, &l[yy * w..(yy + 1) * w]);
            }
            if win_frac > 0.0 {
                s.omin.copy_from_slice(&s.vmin);
                s.omax.copy_from_slice(&s.vmax);
                for yy in [y.saturating_sub(win + 1), (y + win + 1).min(h - 1)] {
                    min_max_row(&mut s.omin, &mut s.omax, &l[yy * w..(yy + 1) * w]);
                }
            }
            let lrow = &l[y * w..(y + 1) * w];
            let mrow = mask.as_ref().map(|m| &m[y * w..(y + 1) * w]);
            for (x, px) in row.chunks_exact_mut(3).enumerate() {
                let lv = lrow[x];
                let g = mrow.map_or(gain, |m| gain * m[x]);
                let v = lv + g * (lv - s.blur[x]);
                if v == lv {
                    continue;
                }
                let y_old = dot(lw, px);
                if !y_old.is_finite() {
                    continue;
                }
                let span = |r: usize| x.saturating_sub(r)..=(x + r).min(w - 1);
                let fold_min = |v: &[f32]| v.iter().fold(f32::INFINITY, |a, &b| a.min(b));
                let fold_max = |v: &[f32]| v.iter().fold(f32::NEG_INFINITY, |a, &b| a.max(b));
                let (mut lo, mut hi) = (fold_min(&s.vmin[span(win)]), fold_max(&s.vmax[span(win)]));
                if win_frac > 0.0 {
                    let (olo, ohi) = (
                        fold_min(&s.omin[span(win + 1)]),
                        fold_max(&s.omax[span(win + 1)]),
                    );
                    lo += (olo - lo) * win_frac;
                    hi += (ohi - hi) * win_frac;
                }
                let v = soft_limit(v, lo, hi, margin);
                // Change in linear luminance through the same transfer both ways,
                // so tiny changes stay tiny (no round-trip error).
                let y_new = (y_old + t.decode(v) - t.decode(lv)).max(y_old.min(0.0));
                set_luminance(px, y_old, y_new, max_ratio);
            }
        },
    );
}

/// Per-thread row buffers for the streaming sharpening pass.
pub(super) struct SharpenRows {
    pub(super) tmp: Vec<f32>,
    pub(super) pad: Vec<f32>,
    pub(super) blur: Vec<f32>,
    pub(super) vmin: Vec<f32>,
    pub(super) vmax: Vec<f32>,
    pub(super) omin: Vec<f32>,
    pub(super) omax: Vec<f32>,
}

impl SharpenRows {
    pub(super) fn new(w: usize, kernel_radius: usize) -> SharpenRows {
        SharpenRows {
            tmp: vec![0.0; w],
            pad: vec![0.0; w + 2 * kernel_radius],
            blur: vec![0.0; w],
            vmin: vec![0.0; w],
            vmax: vec![0.0; w],
            omin: vec![0.0; w],
            omax: vec![0.0; w],
        }
    }
}

/// Fold one row into running per-column minima and maxima.
#[inline]
pub(super) fn min_max_row(lo: &mut [f32], hi: &mut [f32], src: &[f32]) {
    for ((lo, hi), &v) in lo.iter_mut().zip(hi.iter_mut()).zip(src) {
        *lo = lo.min(v);
        *hi = hi.max(v);
    }
}

/// Overshoot margin (encoded) for a Detail value in 0..1.
pub(super) fn halo_margin(detail: f64, k: &DetailStrengths) -> f32 {
    let m0 = k.halo_margin_detail_0.max(1e-6);
    let m1 = k.halo_margin_detail_100.max(1e-6);
    (m0 * (m1 / m0).powf(detail)) as f32
}

/// Keep `v` within `[lo, hi]` except for an excess that approaches `margin`
/// asymptotically (slope 1 at the bounds, so there is no visible kink).
#[inline]
pub(super) fn soft_limit(v: f32, lo: f32, hi: f32, margin: f32) -> f32 {
    if v > hi {
        let e = v - hi;
        hi + e * margin / (e + margin)
    } else if v < lo {
        let e = lo - v;
        lo - e * margin / (e + margin)
    } else {
        v
    }
}

/// Sharpening mask from encoded luminance: smoothstep of the gradient magnitude
/// of a lightly blurred copy around a threshold set by Masking (0..1), softened.
pub(super) fn edge_mask(l: &[f32], w: usize, h: usize, m: &EdgeMaskPlan) -> Vec<f32> {
    let blurred = gaussian_by(l, w, h, &m.blur);
    let (e0, e1) = (m.e0, m.e1);
    let mut raw = vec![0.0f32; w * h];
    raw.par_chunks_mut(w).enumerate().for_each(|(y, out)| {
        let row = &blurred[y * w..(y + 1) * w];
        let up = &blurred[y.saturating_sub(1) * w..][..w];
        let down = &blurred[(y + 1).min(h - 1) * w..][..w];
        for (x, o) in out.iter_mut().enumerate() {
            let gx = 0.5 * (row[(x + 1).min(w - 1)] - row[x.saturating_sub(1)]);
            let gy = 0.5 * (down[x] - up[x]);
            *o = smoothstep(e0, e1, (gx * gx + gy * gy).sqrt());
        }
    });
    gaussian_by(&raw, w, h, &m.smooth)
}
