//! Detail panel: sharpening and noise reduction.
//!
//! Interface contract (pipeline.rs depends on it):
//! - both run on the display-linear ProPhoto image near the end of the
//!   pipeline (noise reduction first, then sharpening);
//! - parameters use Camera Raw units and ranges (see the field docs);
//! - each is an exact no-op at its neutral values (amount 0) and keeps values finite.
//!
//! **Model v1 (ours, not Adobe's).** Camera Raw's sharpening and noise
//! reduction are closed source; what is known (docs/research/05 §12–13) is that
//! sharpening is a luminance-only unsharp mask whose Detail slider trades halo
//! suppression against texture, with an edge mask (Masking), and that noise
//! reduction is edge-preserving smoothing in an opponent space. Every operator
//! here works on *encoded* values (the sRGB transfer, as for the curves in
//! pipeline.rs), where noise and edge contrast are roughly perceptually uniform,
//! and writes back to the linear image as a luminance ratio (sharpening,
//! luminance NR) or with the luminance held fixed (colour NR), so none of them
//! shifts hue or puts colour fringes on edges.
//!
//! **Sharpening** (`Sharpness`, `SharpenRadius`, `SharpenDetail`, `SharpenEdgeMasking`):
//! 1. `L` = encoded luminance; high-pass `hf = L − gauss(L, σ)` with σ
//!    proportional to the radius, in pixels (Lightroom's radius is in pixels).
//! 2. Sharpened `L' = L + gain·mask·hf`.
//! 3. *Detail* limits overshoot: `L'` may leave the min/max of `L` over a small
//!    window (about one radius) only by a soft-limited margin that grows
//!    exponentially with Detail. Low Detail sharpens edges with almost no halo
//!    and leaves fine texture mostly alone (texture peaks are local extremes);
//!    high Detail is a plain unsharp mask that brings texture out fully.
//! 4. *Masking* builds `mask` from the gradient magnitude of a lightly blurred
//!    `L` through a smoothstep whose threshold rises with the slider, then
//!    softens it so it covers the overshoot shoulders of an edge. 0 sharpens
//!    everywhere (no mask is computed), 100 only the strongest edges.
//!
//! **Luminance noise reduction** (`LuminanceSmoothing`, `...Detail`, `...Contrast`):
//! 1. Self-guided filters on `L` at two scales (fine radius growing with the
//!    slider, then a coarser one). The guided filter's regularisation `eps` is
//!    the variance of the noise the slider targets, so regions whose local
//!    variance is below it are flattened and stronger edges kept.
//! 2. *Detail* returns removed detail where its local RMS clearly exceeds that
//!    noise level (a soft threshold on a 3×3 energy), scaled by the slider.
//! 3. *Contrast* adds back a low-pass of what is still removed: pixel noise
//!    averages out of it, lost local contrast does not.
//!
//! **Colour noise reduction** (`ColorNoiseReduction`, `...Detail`, `...Smoothness`):
//! 1. Chroma = encoded R, G, B minus encoded luminance (three opponent channels).
//! 2. A box-downscaled copy (factor chosen so the filter radius is a few
//!    low-resolution pixels) is smoothed with a guided filter steered by encoded
//!    luminance; the radius grows with strength and Smoothness (Smoothness
//!    removes the larger, low-frequency blotches). Its linear coefficients are
//!    upsampled and applied to the full-resolution luminance (He & Sun's fast
//!    guided filter), so chroma follows full-resolution luminance edges.
//! 3. *Detail* protects colour edges the luminance does not explain (isoluminant
//!    red/green boundaries): where the low-resolution residual energy exceeds a
//!    threshold that falls as Detail rises, the original chroma is kept.
//! 4. Each pixel is rebuilt from the new chroma and rescaled to its original
//!    linear luminance, so colour NR never changes brightness.
//!
//! All radii are in pixels, not scaled with the image size: noise and
//! sharpening act at the sensor's pixel scale, as Lightroom's do at 100%.
//! Camera Raw's defaults (Sharpness 40, Color 25) run on every render, so the
//! default path costs a few full-resolution passes (≈0.5 s per 24 MP frame on 4
//! cores). Every constant lives in [`DetailStrengths`] for the calibration
//! harness to fit against Lightroom sweeps (docs/lightroom-trial-plan.md).

use std::sync::OnceLock;

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::buffer::{Image, luminance_weights};
use crate::filters::{box_blur, gaussian_blur};

/// `Sharpness` 0..150, `SharpenRadius` 0.5..3.0, `SharpenDetail` 0..100, `SharpenEdgeMasking` 0..100.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sharpening {
    pub amount: f64,
    pub radius: f64,
    pub detail: f64,
    pub masking: f64,
}

impl Default for Sharpening {
    /// Camera Raw's defaults for raw files.
    fn default() -> Self {
        Sharpening {
            amount: 40.0,
            radius: 1.0,
            detail: 25.0,
            masking: 0.0,
        }
    }
}

/// `LuminanceSmoothing`, `LuminanceNoiseReductionDetail`, `LuminanceNoiseReductionContrast`,
/// `ColorNoiseReduction`, `ColorNoiseReductionDetail`, `ColorNoiseReductionSmoothness`; all 0..100.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NoiseReduction {
    pub luminance: f64,
    pub luminance_detail: f64,
    pub luminance_contrast: f64,
    pub color: f64,
    pub color_detail: f64,
    pub color_smoothness: f64,
}

impl Default for NoiseReduction {
    /// Camera Raw's defaults for raw files.
    fn default() -> Self {
        NoiseReduction {
            luminance: 0.0,
            luminance_detail: 50.0,
            luminance_contrast: 0.0,
            color: 25.0,
            color_detail: 50.0,
            color_smoothness: 50.0,
        }
    }
}

/// Tunable constants. Fitted later against Lightroom sweeps; these are starting
/// values. "Encoded" means sRGB-transfer-encoded luminance or chroma (0..1).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DetailStrengths {
    /// Unsharp-mask gain at Sharpness 100 (150 gives 1.5× this).
    pub sharpen_gain: f64,
    /// Unsharp-mask Gaussian sigma, in pixels per pixel of `SharpenRadius`.
    pub sharpen_sigma_per_radius: f64,
    /// Overshoot (encoded) allowed beyond the local min/max at Detail 0 ...
    pub halo_margin_detail_0: f64,
    /// ... and at Detail 100 (interpolated exponentially; large = no limit).
    pub halo_margin_detail_100: f64,
    /// Radius of the min/max window, in units of `SharpenRadius` (at least 1 px).
    pub halo_window_per_radius: f64,
    /// Masking: Gaussian pre-blur (pixels) before the gradient is taken.
    pub mask_blur_sigma: f64,
    /// Masking: gradient magnitude (encoded per pixel) at the centre of the
    /// mask's threshold at Masking 100.
    pub mask_threshold_at_100: f64,
    /// Masking: threshold = `mask_threshold_at_100 · (masking / 100)^gamma`.
    pub mask_threshold_gamma: f64,
    /// Masking: relative half-width of the smoothstep around the threshold
    /// (0 = hard step, 1 = from 0 to twice the threshold).
    pub mask_softness: f64,
    /// Masking: Gaussian sigma (pixels) softening the mask so it covers an
    /// edge's overshoot shoulders.
    pub mask_smooth_sigma: f64,
    /// Largest luminance ratio applied to one pixel (sharpening and luminance
    /// NR); any further brightening is added as neutral grey, so near-black
    /// pixels do not have their colour noise multiplied.
    pub max_luminance_ratio: f64,

    /// Luminance NR: noise standard deviation (encoded) that Luminance 100
    /// removes; proportional to the slider. Its square is the guided filter's eps.
    pub luminance_noise_sigma: f64,
    /// Luminance NR: fine-scale guided-filter radius (pixels) at slider 0 ...
    pub luminance_radius: f64,
    /// ... plus this many pixels at slider 100.
    pub luminance_radius_per_strength: f64,
    /// Luminance NR: coarse-scale radius as a multiple of the fine radius.
    pub luminance_coarse_radius: f64,
    /// Luminance NR: coarse-scale eps as a multiple of the fine eps.
    pub luminance_coarse_eps: f64,
    /// Luminance NR Detail: local RMS of the removed signal, in multiples of the
    /// targeted noise sigma, above which it counts as detail rather than noise.
    pub luminance_detail_threshold: f64,
    /// Luminance NR Detail: share of above-threshold detail returned at Detail 100.
    pub luminance_detail_restore: f64,
    /// Luminance NR Contrast: Gaussian sigma (pixels) of the low-pass of the
    /// removed signal that is added back.
    pub luminance_contrast_sigma: f64,
    /// Luminance NR Contrast: share of that low-pass added back at Contrast 100.
    pub luminance_contrast_restore: f64,

    /// Colour NR: smoothing radius (full-resolution pixels) at slider 0 ...
    pub color_radius: f64,
    /// ... plus this many pixels at Color 100 ...
    pub color_radius_per_strength: f64,
    /// ... plus this many at Smoothness 100.
    pub color_radius_per_smoothness: f64,
    /// Colour NR: slider value (0..1) from which smoothed chroma fully replaces
    /// the original; below it the two are blended linearly.
    pub color_full_strength: f64,
    /// Colour NR: guided-filter eps (encoded luminance squared). Luminance
    /// structure with a variance well above it steers the chroma.
    pub color_guide_eps: f64,
    /// Colour NR: largest |slope| of chroma against luminance the guided filter
    /// may use. Near weak luminance edges the slope can get large and would
    /// copy full-resolution luminance noise into the chroma.
    pub color_guide_max_slope: f64,
    /// Colour NR Detail: coherent residual chroma (encoded) above which the
    /// original chroma is kept, at Detail 100 ...
    pub color_edge_threshold: f64,
    /// ... times `(detail / 100)^-power` below that (no protection at Detail 0).
    pub color_edge_detail_power: f64,
    /// Colour NR Detail: relative rise of that threshold at Color 100 (more
    /// noise is expected at higher strengths).
    pub color_edge_threshold_per_strength: f64,
    /// Colour NR Detail: relative half-width of the smoothstep around the threshold.
    pub color_edge_softness: f64,
    /// Colour NR Detail: box radius (low-resolution pixels) over which the
    /// residual is averaged before its magnitude is taken.
    pub color_edge_coherence_radius: f64,
    /// Colour NR: target guided-filter radius on the downscaled chroma (sets the
    /// downscale factor: full radius / this, rounded down).
    pub color_low_res_radius: f64,
    /// Colour NR: largest downscale factor.
    pub color_max_downscale: f64,
}

impl Default for DetailStrengths {
    fn default() -> Self {
        DEFAULT_STRENGTHS
    }
}

pub const DEFAULT_STRENGTHS: DetailStrengths = DetailStrengths {
    sharpen_gain: 1.0,
    sharpen_sigma_per_radius: 1.0,
    halo_margin_detail_0: 0.004,
    halo_margin_detail_100: 1.0,
    halo_window_per_radius: 1.0,
    mask_blur_sigma: 1.0,
    mask_threshold_at_100: 0.05,
    mask_threshold_gamma: 2.0,
    mask_softness: 0.6,
    mask_smooth_sigma: 1.0,
    max_luminance_ratio: 4.0,

    luminance_noise_sigma: 0.06,
    luminance_radius: 1.0,
    luminance_radius_per_strength: 3.0,
    luminance_coarse_radius: 2.0,
    luminance_coarse_eps: 0.5,
    luminance_detail_threshold: 2.0,
    luminance_detail_restore: 1.0,
    luminance_contrast_sigma: 2.0,
    luminance_contrast_restore: 1.0,

    color_radius: 1.0,
    color_radius_per_strength: 16.0,
    color_radius_per_smoothness: 8.0,
    color_full_strength: 0.25,
    color_guide_eps: 1e-3,
    color_guide_max_slope: 2.0,
    color_edge_threshold: 0.012,
    color_edge_detail_power: 1.5,
    color_edge_threshold_per_strength: 0.5,
    color_edge_softness: 0.5,
    color_edge_coherence_radius: 1.0,
    color_low_res_radius: 2.0,
    color_max_downscale: 4.0,
};

/// Linear luminance below which a pixel is too dark to scale; changes are
/// added as neutral grey instead (sharpening, luminance NR) or skipped (colour NR).
const MIN_SCALE_Y: f32 = 1e-6;

/// Encoded luminance is centred on this value inside the guided filters, which
/// keeps their running sums small and their variances accurate in `f32`.
const CENTRE: f32 = 0.5;

/// Noise reduction: luminance first (it cleans the guide), then colour.
pub fn apply_noise_reduction(img: &mut Image, p: &NoiseReduction, k: &DetailStrengths) {
    if img.width == 0 || img.height == 0 {
        return;
    }
    if p.luminance > 0.0 && p.luminance.is_finite() {
        luminance_noise_reduction(img, p, k);
    }
    if p.color > 0.0 && p.color.is_finite() {
        color_noise_reduction(img, p, k);
    }
}

pub fn apply_sharpening(img: &mut Image, p: &Sharpening, k: &DetailStrengths) {
    let (w, h) = (img.width, img.height);
    if w == 0 || h == 0 || !(p.amount > 0.0) || !p.amount.is_finite() {
        return;
    }
    let gain = (p.amount.min(150.0) / 100.0 * k.sharpen_gain.max(0.0)) as f32;
    let radius = slider(p.radius, 1.0, 0.5, 3.0);
    let detail = slider(p.detail, 25.0, 0.0, 100.0) / 100.0;
    let masking = slider(p.masking, 0.0, 0.0, 100.0) / 100.0;
    if gain <= 0.0 {
        return;
    }
    let t = transfer();
    let lw = luminance_weights();
    let max_ratio = k.max_luminance_ratio.max(1.0) as f32;

    // 1. Encoded luminance and its unsharp-mask blur.
    let l: Vec<f32> = img
        .rgb
        .par_chunks_exact(3)
        .map(|p| t.encode(dot(lw, p)))
        .collect();
    let sigma = (radius * k.sharpen_sigma_per_radius.max(0.0)) as f32;
    let blur = gaussian_blur(&l, w, h, sigma);

    // 2. Edge mask (Masking > 0 only).
    let mask = (masking > 0.0).then(|| {
        let mask_sigma = k.mask_blur_sigma.max(0.0) as f32;
        if (mask_sigma - sigma).abs() < 1e-6 {
            edge_mask(&blur, w, h, masking, k)
        } else {
            edge_mask(&gaussian_blur(&l, w, h, mask_sigma), w, h, masking, k)
        }
    });

    // 3. Sharpen with overshoot limited to the local min/max plus a margin.
    let margin = halo_margin(detail, k);
    let win = ((radius * k.halo_window_per_radius.max(0.0)).round() as usize).max(1);
    img.rgb.par_chunks_mut(w * 3).enumerate().for_each_init(
        || (vec![0.0f32; w], vec![0.0f32; w]),
        |(vmin, vmax), (y, row)| {
            // Vertical min/max over the window's rows, then horizontal per pixel.
            let (y0, y1) = (y.saturating_sub(win), (y + win).min(h - 1));
            vmin.copy_from_slice(&l[y0 * w..(y0 + 1) * w]);
            vmax.copy_from_slice(vmin);
            for yy in y0 + 1..=y1 {
                let src = &l[yy * w..(yy + 1) * w];
                for ((lo, hi), &v) in vmin.iter_mut().zip(vmax.iter_mut()).zip(src) {
                    *lo = lo.min(v);
                    *hi = hi.max(v);
                }
            }
            let lrow = &l[y * w..(y + 1) * w];
            let brow = &blur[y * w..(y + 1) * w];
            let mrow = mask.as_ref().map(|m| &m[y * w..(y + 1) * w]);
            for (x, px) in row.chunks_exact_mut(3).enumerate() {
                let lv = lrow[x];
                let g = mrow.map_or(gain, |m| gain * m[x]);
                let v = lv + g * (lv - brow[x]);
                if v == lv {
                    continue;
                }
                let (x0, x1) = (x.saturating_sub(win), (x + win).min(w - 1));
                let mut lo = vmin[x0];
                let mut hi = vmax[x0];
                for xx in x0 + 1..=x1 {
                    lo = lo.min(vmin[xx]);
                    hi = hi.max(vmax[xx]);
                }
                let v = soft_limit(v, lo, hi, margin);
                let y_old = dot(lw, px);
                // Change in linear luminance through the same transfer both ways,
                // so tiny changes stay tiny (no round-trip error).
                let y_new = (y_old + t.decode(v) - t.decode(lv)).max(y_old.min(0.0));
                set_luminance(px, y_old, y_new, max_ratio);
            }
        },
    );
}

/// Overshoot margin (encoded) for a Detail value in 0..1.
fn halo_margin(detail: f64, k: &DetailStrengths) -> f32 {
    let m0 = k.halo_margin_detail_0.max(1e-6);
    let m1 = k.halo_margin_detail_100.max(1e-6);
    (m0 * (m1 / m0).powf(detail)) as f32
}

/// Keep `v` within `[lo, hi]` except for an excess that approaches `margin`
/// asymptotically (slope 1 at the bounds, so there is no visible kink).
#[inline]
fn soft_limit(v: f32, lo: f32, hi: f32, margin: f32) -> f32 {
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

/// Sharpening mask from a blurred encoded luminance: smoothstep of the gradient
/// magnitude around a threshold set by Masking (0..1), then softened.
fn edge_mask(blurred: &[f32], w: usize, h: usize, masking: f64, k: &DetailStrengths) -> Vec<f32> {
    let threshold =
        (k.mask_threshold_at_100.max(0.0) * masking.powf(k.mask_threshold_gamma.max(0.01))) as f32;
    let soft = k.mask_softness.clamp(0.0, 1.0) as f32;
    let (e0, e1) = (threshold * (1.0 - soft), threshold * (1.0 + soft));
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
    gaussian_blur(&raw, w, h, k.mask_smooth_sigma.max(0.0) as f32)
}

fn luminance_noise_reduction(img: &mut Image, p: &NoiseReduction, k: &DetailStrengths) {
    let (w, h) = (img.width, img.height);
    let s = p.luminance.min(100.0) / 100.0;
    let detail = slider(p.luminance_detail, 50.0, 0.0, 100.0) / 100.0;
    let contrast = slider(p.luminance_contrast, 0.0, 0.0, 100.0) / 100.0;
    let sigma = (k.luminance_noise_sigma.max(0.0) * s) as f32;
    let eps = sigma * sigma;
    if !(eps > 0.0) {
        return;
    }
    let t = transfer();
    let lw = luminance_weights();
    let max_ratio = k.max_luminance_ratio.max(1.0) as f32;

    // 1. Two-scale self-guided smoothing of the centred encoded luminance.
    let l: Vec<f32> = img
        .rgb
        .par_chunks_exact(3)
        .map(|p| t.encode(dot(lw, p)) - CENTRE)
        .collect();
    let r_fine = (k.luminance_radius + k.luminance_radius_per_strength * s)
        .round()
        .max(1.0);
    let r_coarse = (r_fine * k.luminance_coarse_radius.max(0.0)).round() as usize;
    let eps_coarse = eps * k.luminance_coarse_eps.max(0.0) as f32;
    let mut smooth = self_guided(&l, w, h, r_fine as usize, eps);
    if r_coarse > 0 && eps_coarse > 0.0 {
        smooth = self_guided(&smooth, w, h, r_coarse, eps_coarse);
    }

    // 2. Detail: share of the removed signal returned per pixel, from its local energy.
    let removed: Vec<f32> = l.par_iter().zip(&smooth).map(|(a, b)| a - b).collect();
    let restore = (k.luminance_detail_restore.max(0.0) * detail) as f32;
    let tau = k.luminance_detail_threshold.max(0.0) as f32 * sigma;
    let inv_tau2 = if tau > 0.0 { 1.0 / (tau * tau) } else { 0.0 };
    let kept: Option<Vec<f32>> = (restore > 0.0).then(|| {
        let sq: Vec<f32> = removed.par_iter().map(|v| v * v).collect();
        let mut energy = box_blur(&sq, w, h, 1);
        energy.par_iter_mut().for_each(|e| {
            // Soft threshold: ~0 for noise-level energy, → `restore` for detail.
            let q = *e * inv_tau2;
            *e = if inv_tau2 > 0.0 {
                restore * q * q / (1.0 + q * q)
            } else {
                restore
            };
        });
        energy
    });

    // 3. Contrast: low-pass of what is still removed after the Detail step.
    let contrast_gain = (k.luminance_contrast_restore.max(0.0) * contrast) as f32;
    let lowpass: Option<Vec<f32>> = (contrast_gain > 0.0).then(|| {
        let rest: Vec<f32> = match &kept {
            Some(kept) => removed
                .par_iter()
                .zip(kept)
                .map(|(r, f)| r * (1.0 - f))
                .collect(),
            None => removed.clone(),
        };
        gaussian_blur(&rest, w, h, k.luminance_contrast_sigma.max(0.0) as f32)
    });

    img.rgb
        .par_chunks_mut(w * 3)
        .enumerate()
        .for_each(|(y, row)| {
            let span = y * w..(y + 1) * w;
            let (lrow, srow, rrow) = (
                &l[span.clone()],
                &smooth[span.clone()],
                &removed[span.clone()],
            );
            let krow = kept.as_ref().map(|v| &v[span.clone()]);
            let prow = lowpass.as_ref().map(|v| &v[span.clone()]);
            for (x, px) in row.chunks_exact_mut(3).enumerate() {
                let mut v = srow[x];
                if let Some(krow) = krow {
                    v += rrow[x] * krow[x];
                }
                if let Some(prow) = prow {
                    v += contrast_gain * prow[x];
                }
                let lv = lrow[x];
                if v == lv {
                    continue;
                }
                let y_old = dot(lw, px);
                let y_new =
                    (y_old + t.decode(v + CENTRE) - t.decode(lv + CENTRE)).max(y_old.min(0.0));
                set_luminance(px, y_old, y_new, max_ratio);
            }
        });
}

/// Self-guided filter (guide = source): edge-preserving smoothing where local
/// variance below `eps` is flattened. Variance is clamped at zero so `f32`
/// rounding in the running sums can never make the coefficient leave 0..1.
fn self_guided(src: &[f32], w: usize, h: usize, r: usize, eps: f32) -> Vec<f32> {
    let mean = box_blur(src, w, h, r);
    let sq: Vec<f32> = src.par_iter().map(|v| v * v).collect();
    let corr = box_blur(&sq, w, h, r);
    drop(sq);
    let (a, b): (Vec<f32>, Vec<f32>) = mean
        .par_iter()
        .zip(&corr)
        .map(|(&m, &c)| {
            let var = (c - m * m).max(0.0);
            let a = var / (var + eps);
            (a, m * (1.0 - a))
        })
        .unzip();
    drop((mean, corr));
    let mean_a = box_blur(&a, w, h, r);
    drop(a);
    let mean_b = box_blur(&b, w, h, r);
    src.par_iter()
        .zip(&mean_a)
        .zip(&mean_b)
        .map(|((&i, &a), &b)| a * i + b)
        .collect()
}

fn color_noise_reduction(img: &mut Image, p: &NoiseReduction, k: &DetailStrengths) {
    let (w, h) = (img.width, img.height);
    let s = p.color.min(100.0) / 100.0;
    let detail = slider(p.color_detail, 50.0, 0.0, 100.0) / 100.0;
    let smoothness = slider(p.color_smoothness, 50.0, 0.0, 100.0) / 100.0;
    let mix = (s / k.color_full_strength.max(1e-6)).min(1.0) as f32;
    if !(mix > 0.0) {
        return;
    }
    let t = transfer();
    let lw = luminance_weights();

    // Scale: full-resolution radius → downscale factor and low-resolution radius.
    let radius = (k.color_radius
        + k.color_radius_per_strength * s
        + k.color_radius_per_smoothness * smoothness)
        .max(1.0);
    let max_factor = k.color_max_downscale.max(1.0).min(w.min(h) as f64);
    let factor = (radius / k.color_low_res_radius.max(0.5))
        .floor()
        .clamp(1.0, max_factor.max(1.0)) as usize;
    let r_low = ((radius / factor as f64).round() as usize).max(1);

    // 1. Low resolution: centred encoded luminance (the guide) and chroma.
    let (small, sw, sh) = downscale(img, factor);
    let n = sw * sh;
    let planes: Vec<[f32; 4]> = small
        .par_chunks_exact(3)
        .map(|p| {
            let le = t.encode(dot(lw, p));
            [
                le - CENTRE,
                t.encode(p[0]) - le,
                t.encode(p[1]) - le,
                t.encode(p[2]) - le,
            ]
        })
        .collect();
    drop(small);
    let plane = |c: usize| -> Vec<f32> { planes.par_iter().map(|v| v[c]).collect() };
    let guide = plane(0);
    let chroma = [plane(1), plane(2), plane(3)];
    drop(planes);

    // 2. Guided-filter coefficients for the three chroma channels.
    let eps = k.color_guide_eps.max(1e-9) as f32;
    let max_slope = k.color_guide_max_slope.max(0.0) as f32;
    let coefs = guided_coefficients(&guide, &chroma, sw, sh, r_low, eps, max_slope);

    // 3. Colour-edge protection: the part of the residual that the luminance
    //    does not explain and that is spatially coherent (box-averaged, so noise
    //    cancels while chroma dragged across a colour edge does not).
    let threshold = color_edge_threshold(detail, s, k);
    let soft = k.color_edge_softness.clamp(0.0, 1.0) as f32;
    let (e0, e1) = (threshold * (1.0 - soft), threshold * (1.0 + soft));
    let coherence = (k.color_edge_coherence_radius.max(0.0).round() as usize).max(1);
    let kappa: Option<Vec<f32>> = threshold.is_finite().then(|| {
        let mut energy = vec![0.0f32; n];
        for (c, (a, b)) in coefs.iter().enumerate() {
            let residual: Vec<f32> = (0..n)
                .into_par_iter()
                .map(|i| chroma[c][i] - (a[i] * guide[i] + b[i]))
                .collect();
            let coherent = box_blur(&residual, sw, sh, coherence);
            energy
                .par_iter_mut()
                .zip(&coherent)
                .for_each(|(e, v)| *e += v * v);
        }
        // Spread one pixel so the sign change right at an edge leaves no gap.
        box_blur(&energy, sw, sh, 1)
            .par_iter()
            .map(|&e| smoothstep(e0, e1, e.max(0.0).sqrt()))
            .collect()
    });
    drop((guide, chroma));

    // 4. Full resolution: blend towards the smoothed chroma, keep luminance.
    let mut grids: Vec<&[f32]> = vec![
        &coefs[0].0,
        &coefs[1].0,
        &coefs[2].0,
        &coefs[0].1,
        &coefs[1].1,
        &coefs[2].1,
    ];
    if let Some(kappa) = &kappa {
        grids.push(kappa);
    }
    let protect = kappa.is_some();
    let columns: Vec<(usize, usize, f32)> = (0..w).map(|x| lerp_position(x, factor, sw)).collect();
    img.rgb.par_chunks_mut(w * 3).enumerate().for_each_init(
        || vec![0.0f32; grids.len() * sw],
        |rows, (y, row)| {
            // Interpolate the coefficient grids vertically once per row.
            let (y0, y1, wy) = lerp_position(y, factor, sh);
            for (g, dst) in grids.iter().zip(rows.chunks_exact_mut(sw)) {
                let (p0, p1) = (&g[y0 * sw..(y0 + 1) * sw], &g[y1 * sw..(y1 + 1) * sw]);
                for ((d, &a), &b) in dst.iter_mut().zip(p0).zip(p1) {
                    *d = a + (b - a) * wy;
                }
            }
            for (px, &(x0, x1, wx)) in row.chunks_exact_mut(3).zip(&columns) {
                let at = |g: usize| {
                    let r = &rows[g * sw..(g + 1) * sw];
                    r[x0] + (r[x1] - r[x0]) * wx
                };
                let y_old = dot(lw, px);
                if !(y_old > MIN_SCALE_Y) {
                    continue;
                }
                let blend = if protect { mix * (1.0 - at(6)) } else { mix };
                if !(blend > 0.0) {
                    continue;
                }
                let le = t.encode(y_old);
                let mut out = [0.0f32; 3];
                for c in 0..3 {
                    let e = t.encode(px[c]);
                    let d = e - le;
                    let smoothed = at(c) * (le - CENTRE) + at(3 + c);
                    // Never push a channel below zero that was not negative already.
                    let e_new = (le + d + blend * (smoothed - d)).max(e.min(0.0));
                    out[c] = (px[c] + t.decode(e_new) - t.decode(e)).max(px[c].min(0.0));
                }
                let y_new = dot(lw, &out);
                if y_new > MIN_SCALE_Y {
                    let scale = y_old / y_new;
                    for (o, v) in px.iter_mut().zip(out) {
                        *o = v * scale;
                    }
                }
            }
        },
    );
}

/// Residual chroma RMS above which colour NR keeps the original chroma.
/// Infinite (no protection) at Detail 0.
fn color_edge_threshold(detail: f64, s: f64, k: &DetailStrengths) -> f32 {
    if !(detail > 0.0) {
        return f32::INFINITY;
    }
    let at_100 = k.color_edge_threshold.max(1e-6);
    let falloff = detail.powf(-k.color_edge_detail_power.max(0.0));
    (at_100 * falloff * (1.0 + k.color_edge_threshold_per_strength.max(0.0) * s)) as f32
}

/// Guided-filter coefficients (He, Sun & Tang) of several sources sharing one
/// guide: per source the box-averaged `(a, b)` with `q ≈ a · guide + b`, ready
/// to be upsampled and applied to a full-resolution guide. `|a|` is capped at
/// `max_slope` (with `b` recomputed, so local means are still reproduced).
fn guided_coefficients(
    guide: &[f32],
    srcs: &[Vec<f32>],
    w: usize,
    h: usize,
    r: usize,
    eps: f32,
    max_slope: f32,
) -> Vec<(Vec<f32>, Vec<f32>)> {
    let mean_i = box_blur(guide, w, h, r);
    let ii: Vec<f32> = guide.par_iter().map(|v| v * v).collect();
    let var_i: Vec<f32> = box_blur(&ii, w, h, r)
        .par_iter()
        .zip(&mean_i)
        .map(|(c, m)| (c - m * m).max(0.0))
        .collect();
    srcs.iter()
        .map(|src| {
            let mean_p = box_blur(src, w, h, r);
            let ip: Vec<f32> = guide.par_iter().zip(src).map(|(a, b)| a * b).collect();
            let corr_ip = box_blur(&ip, w, h, r);
            let (a, b): (Vec<f32>, Vec<f32>) = (0..w * h)
                .into_par_iter()
                .map(|i| {
                    let cov = corr_ip[i] - mean_i[i] * mean_p[i];
                    let a = (cov / (var_i[i] + eps)).clamp(-max_slope, max_slope);
                    (a, mean_p[i] - a * mean_i[i])
                })
                .unzip();
            (box_blur(&a, w, h, r), box_blur(&b, w, h, r))
        })
        .collect()
}

/// Box-average downscale by an integer factor (partial blocks at the right and
/// bottom edges average what they cover).
fn downscale(img: &Image, f: usize) -> (Vec<f32>, usize, usize) {
    let (w, h) = (img.width, img.height);
    if f <= 1 {
        return (img.rgb.clone(), w, h);
    }
    let (sw, sh) = (w.div_ceil(f), h.div_ceil(f));
    let mut out = vec![0.0f32; sw * sh * 3];
    out.par_chunks_mut(sw * 3).enumerate().for_each(|(j, row)| {
        let (y0, y1) = (j * f, ((j + 1) * f).min(h));
        for y in y0..y1 {
            let src = &img.rgb[y * w * 3..(y + 1) * w * 3];
            for (i, acc) in row.chunks_exact_mut(3).enumerate() {
                let (x0, x1) = (i * f, ((i + 1) * f).min(w));
                for p in src[x0 * 3..x1 * 3].chunks_exact(3) {
                    acc[0] += p[0];
                    acc[1] += p[1];
                    acc[2] += p[2];
                }
            }
        }
        for (i, acc) in row.chunks_exact_mut(3).enumerate() {
            let cover = ((y1 - y0) * (((i + 1) * f).min(w) - i * f)) as f32;
            acc.iter_mut().for_each(|v| *v /= cover);
        }
    });
    (out, sw, sh)
}

/// Bilinear source position of full-resolution index `i` in a grid downscaled
/// by `f` with `n` cells (cell centres aligned): (lower, upper, weight of upper).
fn lerp_position(i: usize, f: usize, n: usize) -> (usize, usize, f32) {
    let s = ((i as f32 + 0.5) / f as f32 - 0.5).clamp(0.0, (n - 1) as f32);
    let lo = (s.floor() as usize).min(n - 1);
    (lo, (lo + 1).min(n - 1), s - lo as f32)
}

/// Change a pixel's linear luminance from `y` to `y_new` keeping its
/// chromaticity: scale RGB by the ratio, capped at `max_ratio`; any further
/// brightening (and any change to a pixel too dark to scale) is added as
/// neutral grey, which raises luminance one for one (the weights sum to 1).
#[inline]
fn set_luminance(px: &mut [f32], y: f32, y_new: f32, max_ratio: f32) {
    if y > MIN_SCALE_Y {
        let ratio = y_new / y;
        if ratio <= max_ratio {
            px.iter_mut().for_each(|v| *v *= ratio);
        } else {
            let add = y_new - y * max_ratio;
            px.iter_mut().for_each(|v| *v = *v * max_ratio + add);
        }
    } else {
        let add = y_new - y;
        px.iter_mut().for_each(|v| *v += add);
    }
}

/// Slider value clamped to its range; non-finite values fall back to `default`.
fn slider(v: f64, default: f64, lo: f64, hi: f64) -> f64 {
    if v.is_finite() { v } else { default }.clamp(lo, hi)
}

#[inline]
fn dot(w: [f32; 3], p: &[f32]) -> f32 {
    w[0] * p[0] + w[1] * p[1] + w[2] * p[2]
}

#[inline]
fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    if e1 <= e0 {
        return if x >= e0 { 1.0 } else { 0.0 };
    }
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn srgb_encode(v: f32) -> f32 {
    if v <= 0.003_130_8 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    }
}

fn srgb_decode(v: f32) -> f32 {
    if v <= 0.040_45 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

/// The sRGB transfer as interpolated tables (exact formula outside 0..1, odd
/// extension below 0). The encode table is fine enough (2^14 steps) that its
/// error near black, where the curve bends most, stays around 1e-6.
struct Transfer {
    enc: Vec<f32>,
    dec: Vec<f32>,
}

const ENC_STEPS: usize = 1 << 14;
const DEC_STEPS: usize = 1 << 12;

fn transfer() -> &'static Transfer {
    static TRANSFER: OnceLock<Transfer> = OnceLock::new();
    TRANSFER.get_or_init(|| Transfer {
        enc: (0..=ENC_STEPS)
            .map(|i| srgb_encode(i as f32 / ENC_STEPS as f32))
            .collect(),
        dec: (0..=DEC_STEPS)
            .map(|i| srgb_decode(i as f32 / DEC_STEPS as f32))
            .collect(),
    })
}

#[inline]
fn table(t: &[f32], steps: usize, v: f32) -> f32 {
    let pos = v * steps as f32;
    let i = (pos as usize).min(steps - 1);
    let f = pos - i as f32;
    t[i] + (t[i + 1] - t[i]) * f
}

impl Transfer {
    #[inline]
    fn encode(&self, v: f32) -> f32 {
        if (0.0..=1.0).contains(&v) {
            table(&self.enc, ENC_STEPS, v)
        } else if v > 1.0 {
            srgb_encode(v)
        } else if v < 0.0 {
            -self.encode(-v)
        } else {
            0.0 // NaN
        }
    }

    #[inline]
    fn decode(&self, v: f32) -> f32 {
        if (0.0..=1.0).contains(&v) {
            table(&self.dec, DEC_STEPS, v)
        } else if v > 1.0 {
            srgb_decode(v)
        } else if v < 0.0 {
            -self.decode(-v)
        } else {
            0.0 // NaN
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OFF: Sharpening = Sharpening {
        amount: 0.0,
        radius: 1.0,
        detail: 25.0,
        masking: 0.0,
    };

    fn nr_off() -> NoiseReduction {
        NoiseReduction {
            color: 0.0,
            ..NoiseReduction::default()
        }
    }

    /// Deterministic approximately standard-normal noise (Irwin–Hall of 4 uniforms).
    fn gauss(i: usize, seed: u64) -> f32 {
        let mut x = (i as u64)
            .wrapping_mul(0x9E37_79B9_7F4A_7C15)
            .wrapping_add(seed.wrapping_mul(0xD1B5_4A32_D192_ED03));
        let mut sum = 0.0f32;
        for _ in 0..4 {
            x ^= x >> 30;
            x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
            x ^= x >> 27;
            x = x.wrapping_mul(0x94D0_49BB_1331_11EB);
            x ^= x >> 31;
            sum += (x >> 40) as f32 / (1u64 << 24) as f32;
        }
        (sum - 2.0) * 3.0f32.sqrt()
    }

    fn grey(v: f32) -> [f32; 3] {
        [v; 3]
    }

    fn lum(p: [f32; 3]) -> f32 {
        dot(luminance_weights(), &p)
    }

    fn enc(v: f32) -> f32 {
        srgb_encode(v)
    }

    /// Encoded-luminance row profile of an image.
    fn row_profile(img: &Image, y: usize) -> Vec<f32> {
        (0..img.width).map(|x| enc(lum(img.get(x, y)))).collect()
    }

    /// Vertical step edge from `a` to `b` (linear grey), blurred with a Gaussian of `blur` px.
    fn blurred_step(w: usize, h: usize, a: f32, b: f32, blur: f32) -> Image {
        let plane: Vec<f32> = (0..w * h)
            .map(|i| if i % w < w / 2 { a } else { b })
            .collect();
        let plane = gaussian_blur(&plane, w, h, blur);
        Image::from_fn(w, h, |x, y| grey(plane[y * w + x]))
    }

    fn std_dev(v: &[f32]) -> f32 {
        let n = v.len() as f64;
        let mean = v.iter().map(|&x| x as f64).sum::<f64>() / n;
        (v.iter().map(|&x| (x as f64 - mean).powi(2)).sum::<f64>() / n).sqrt() as f32
    }

    fn assert_finite(img: &Image, what: &str) {
        assert!(img.rgb.iter().all(|v| v.is_finite()), "{what}: non-finite");
    }

    fn sharpen(img: &Image, p: Sharpening) -> Image {
        let mut out = img.clone();
        apply_sharpening(&mut out, &p, &DetailStrengths::default());
        out
    }

    fn denoise(img: &Image, p: NoiseReduction) -> Image {
        let mut out = img.clone();
        apply_noise_reduction(&mut out, &p, &DetailStrengths::default());
        out
    }

    #[test]
    fn strengths_serde_round_trip_and_partial_json() {
        let k = DetailStrengths::default();
        let json = serde_json::to_string(&k).unwrap();
        assert_eq!(serde_json::from_str::<DetailStrengths>(&json).unwrap(), k);
        let partial: DetailStrengths = serde_json::from_str(r#"{"sharpen_gain": 1.3}"#).unwrap();
        assert_eq!(partial.sharpen_gain, 1.3);
        assert_eq!(partial.color_guide_eps, DEFAULT_STRENGTHS.color_guide_eps);
    }

    #[test]
    fn transfer_tables_match_the_formula_and_invert() {
        let t = transfer();
        for i in 0..=2000 {
            let v = i as f32 / 1000.0 - 0.5; // -0.5..1.5
            let exact = v.signum() * srgb_encode(v.abs());
            assert!((t.encode(v) - exact).abs() < 2e-5, "encode {v}");
            assert!((t.decode(t.encode(v)) - v).abs() < 2e-5, "round trip {v}");
        }
        assert_eq!(t.encode(f32::NAN), 0.0);
        let sum: f32 = luminance_weights().iter().sum();
        assert!(
            (sum - 1.0).abs() < 1e-5,
            "neutral add needs weights summing to 1"
        );
    }

    fn textured(w: usize, h: usize) -> Image {
        Image::from_fn(w, h, |x, y| {
            let i = y * w + x;
            let base = 0.05 + 0.6 * (x as f32 / w as f32);
            [
                base * (1.0 + 0.1 * gauss(i, 1)),
                base * (0.8 + 0.1 * gauss(i, 2)),
                base * (0.6 + 0.1 * gauss(i, 3)),
            ]
        })
    }

    #[test]
    fn neutral_settings_are_exact_no_ops() {
        let img = textured(37, 23);
        assert_eq!(sharpen(&img, OFF).rgb, img.rgb);
        for amount in [-10.0, f64::NAN, f64::INFINITY] {
            assert_eq!(sharpen(&img, Sharpening { amount, ..OFF }).rgb, img.rgb);
        }
        // Zero strengths with every other slider moved.
        let p = NoiseReduction {
            luminance: 0.0,
            luminance_detail: 90.0,
            luminance_contrast: 80.0,
            color: 0.0,
            color_detail: 10.0,
            color_smoothness: 100.0,
        };
        assert_eq!(denoise(&img, p).rgb, img.rgb);
        assert_eq!(denoise(&img, nr_off()).rgb, img.rgb);
    }

    #[test]
    fn sharpening_increases_acutance_and_leaves_flats_alone() {
        let (w, h) = (64, 8);
        let img = blurred_step(w, h, 0.05, 0.4, 1.0);
        let out = sharpen(
            &img,
            Sharpening {
                amount: 100.0,
                ..Sharpening::default()
            },
        );
        let max_slope = |p: &[f32]| p.windows(2).map(|s| s[1] - s[0]).fold(0.0f32, f32::max);
        let (before, after) = (row_profile(&img, 4), row_profile(&out, 4));
        assert!(
            max_slope(&after) > 1.2 * max_slope(&before),
            "slope {} → {}",
            max_slope(&before),
            max_slope(&after)
        );
        // Far from the edge nothing changes.
        for y in 0..h {
            for x in (0..12).chain(w - 12..w) {
                let (a, b) = (img.get(x, y), out.get(x, y));
                for c in 0..3 {
                    assert!((a[c] - b[c]).abs() < 1e-6, "({x},{y}) {a:?} → {b:?}");
                }
            }
        }
        // Default settings sharpen too, more gently.
        let gentle = row_profile(&sharpen(&img, Sharpening::default()), 4);
        assert!(max_slope(&gentle) > max_slope(&before));
        assert!(max_slope(&gentle) < max_slope(&after));
    }

    #[test]
    fn sharpening_keeps_chromaticity() {
        let (w, h) = (48, 6);
        let img = Image::from_fn(w, h, |x, _| {
            let v = if x < w / 2 { 0.1 } else { 0.5 };
            [v * 1.2, v * 0.9, v * 0.4]
        });
        let soft = {
            let mut s = img.clone();
            for c in 0..3 {
                let plane: Vec<f32> = s.rgb.iter().skip(c).step_by(3).copied().collect();
                let b = gaussian_blur(&plane, w, h, 1.2);
                for (i, v) in b.into_iter().enumerate() {
                    s.rgb[i * 3 + c] = v;
                }
            }
            s
        };
        let out = sharpen(
            &soft,
            Sharpening {
                amount: 150.0,
                detail: 100.0,
                ..Sharpening::default()
            },
        );
        assert_ne!(out.rgb, soft.rgb);
        for (a, b) in soft.rgb.chunks(3).zip(out.rgb.chunks(3)) {
            assert!((a[0] / a[1] - b[0] / b[1]).abs() < 1e-4);
            assert!((a[2] / a[1] - b[2] / b[1]).abs() < 1e-4);
        }
    }

    /// Low-contrast texture (left half) next to a strong blurred edge (right half).
    fn texture_and_edge(w: usize, h: usize) -> Image {
        let edge: Vec<f32> = (0..w * h)
            .map(|i| if i % w < 3 * w / 4 { 0.12 } else { 0.6 })
            .collect();
        let edge = gaussian_blur(&edge, w, h, 1.0);
        Image::from_fn(w, h, |x, y| {
            let v = if x < w / 2 {
                // ±~2% encoded sinusoidal texture around mid-grey.
                let t = 0.18 * (1.0 + 0.08 * ((x as f32 * 1.7).sin() * (y as f32 * 1.3).cos()));
                t
            } else {
                edge[y * w + x]
            };
            grey(v)
        })
    }

    fn mean_abs_change(a: &Image, b: &Image, xs: std::ops::Range<usize>) -> f32 {
        let mut sum = 0.0;
        let mut n = 0;
        for y in 0..a.height {
            for x in xs.clone() {
                sum += (enc(lum(a.get(x, y))) - enc(lum(b.get(x, y)))).abs();
                n += 1;
            }
        }
        sum / n as f32
    }

    #[test]
    fn masking_spares_low_contrast_texture_but_not_strong_edges() {
        let (w, h) = (96, 24);
        let img = texture_and_edge(w, h);
        let p = Sharpening {
            amount: 120.0,
            detail: 100.0,
            ..Sharpening::default()
        };
        let open = sharpen(&img, p);
        let masked = sharpen(
            &img,
            Sharpening {
                masking: 100.0,
                ..p
            },
        );
        let texture = 4..w / 2 - 4;
        let edge = 3 * w / 4 - 4..3 * w / 4 + 4;
        let (t_open, t_masked) = (
            mean_abs_change(&img, &open, texture.clone()),
            mean_abs_change(&img, &masked, texture),
        );
        let (e_open, e_masked) = (
            mean_abs_change(&img, &open, edge.clone()),
            mean_abs_change(&img, &masked, edge),
        );
        assert!(
            t_open > 1e-3,
            "texture is sharpened without a mask: {t_open}"
        );
        assert!(t_masked < 0.05 * t_open, "texture {t_open} → {t_masked}");
        assert!(e_masked > 0.6 * e_open, "edge {e_open} → {e_masked}");
        // Intermediate masking sits in between on the texture.
        let half = sharpen(&img, Sharpening { masking: 30.0, ..p });
        let t_half = mean_abs_change(&img, &half, 4..w / 2 - 4);
        assert!(
            t_masked <= t_half && t_half <= t_open,
            "{t_masked} {t_half} {t_open}"
        );
    }

    #[test]
    fn low_detail_limits_overshoot() {
        let (w, h) = (64, 6);
        let img = blurred_step(w, h, 0.05, 0.4, 1.5);
        let overshoot = |detail: f64| {
            let out = sharpen(
                &img,
                Sharpening {
                    amount: 150.0,
                    radius: 2.0,
                    detail,
                    masking: 0.0,
                },
            );
            let p = row_profile(&out, 3);
            let (lo, hi) = (enc(0.05), enc(0.4));
            let under = p.iter().map(|v| lo - v).fold(0.0f32, f32::max);
            let over = p.iter().map(|v| v - hi).fold(0.0f32, f32::max);
            (under.max(over), p)
        };
        let (low, p_low) = overshoot(0.0);
        let (mid, _) = overshoot(50.0);
        let (high, _) = overshoot(100.0);
        assert!(high > 0.03, "plain unsharp mask overshoots: {high}");
        assert!(low < 0.006, "Detail 0 overshoot {low}");
        assert!(low < mid && mid < high, "{low} {mid} {high}");
        // Detail 0 still steepens the edge.
        let max_slope = |p: &[f32]| p.windows(2).map(|s| s[1] - s[0]).fold(0.0f32, f32::max);
        assert!(max_slope(&p_low) > 1.1 * max_slope(&row_profile(&img, 3)));
    }

    #[test]
    fn high_detail_brings_out_texture_more() {
        let img = texture_and_edge(96, 24);
        let change = |detail: f64| {
            let out = sharpen(
                &img,
                Sharpening {
                    amount: 100.0,
                    detail,
                    ..Sharpening::default()
                },
            );
            mean_abs_change(&img, &out, 4..44)
        };
        assert!(change(100.0) > 1.5 * change(0.0));
    }

    /// Flat grey patch with luminance noise (left) and a sharp step (right half).
    fn noisy_patch_and_step(w: usize, h: usize, sigma: f32) -> Image {
        Image::from_fn(w, h, |x, y| {
            let i = y * w + x;
            let base = if x < 3 * w / 4 { 0.18 } else { 0.6 };
            // Noise in encoded units, applied as a luminance change (neutral).
            let e = (enc(base) + sigma * gauss(i, 7)).clamp(0.0, 1.0);
            grey(srgb_decode(e))
        })
    }

    #[test]
    fn luminance_nr_flattens_noise_and_keeps_edges() {
        let (w, h) = (128, 64);
        let sigma = 0.02;
        let img = noisy_patch_and_step(w, h, sigma);
        let out = denoise(
            &img,
            NoiseReduction {
                luminance: 60.0,
                ..nr_off()
            },
        );
        assert_finite(&out, "luminance NR");
        let patch = |im: &Image| -> Vec<f32> {
            let mut v = Vec::new();
            for y in 12..h - 12 {
                for x in 12..3 * w / 4 - 16 {
                    v.push(enc(lum(im.get(x, y))));
                }
            }
            v
        };
        let (before, after) = (std_dev(&patch(&img)), std_dev(&patch(&out)));
        assert!(before > 0.8 * sigma);
        assert!(after < before / 6.0, "noise {before} → {after}");
        // The step stays sharp: 2 px either side keep (almost) the full contrast.
        let edge = 3 * w / 4;
        for y in 20..h - 20 {
            let (a, b) = (
                enc(lum(out.get(edge - 3, y))),
                enc(lum(out.get(edge + 2, y))),
            );
            let full = enc(0.6) - enc(0.18);
            assert!(b - a > 0.85 * full, "row {y}: {a} {b}");
        }
        // Colour NR is off, so chromaticity of every pixel is kept (it is grey).
        for p in out.rgb.chunks(3) {
            assert!((p[0] - p[1]).abs() < 1e-5 && (p[2] - p[1]).abs() < 1e-5);
        }
    }

    #[test]
    fn luminance_nr_detail_and_contrast_restore_texture() {
        let (w, h) = (96, 48);
        // Texture of ~3x the noise amplitude plus noise.
        let img = Image::from_fn(w, h, |x, y| {
            let i = y * w + x;
            let t = 0.04 * ((x as f32 * 0.9).sin() * (y as f32 * 0.7).sin());
            grey(srgb_decode(
                (0.45 + t + 0.012 * gauss(i, 3)).clamp(0.0, 1.0),
            ))
        });
        let texture_std = |p: NoiseReduction| {
            let out = denoise(&img, p);
            let v: Vec<f32> = (8..h - 8)
                .flat_map(|y| (8..w - 8).map(move |x| (x, y)))
                .map(|(x, y)| enc(lum(out.get(x, y))))
                .collect();
            std_dev(&v)
        };
        let base = NoiseReduction {
            luminance: 70.0,
            luminance_detail: 0.0,
            ..nr_off()
        };
        let flat = texture_std(base);
        let detailed = texture_std(NoiseReduction {
            luminance_detail: 100.0,
            ..base
        });
        let contrasty = texture_std(NoiseReduction {
            luminance_contrast: 100.0,
            ..base
        });
        assert!(detailed > 1.1 * flat, "detail {flat} → {detailed}");
        assert!(contrasty > 1.1 * flat, "contrast {flat} → {contrasty}");
    }

    /// Grey patch with independent per-channel (chroma) noise on the left, a
    /// red/green colour edge on the right.
    fn chroma_noise_and_colour_edge(w: usize, h: usize, sigma: f32) -> Image {
        Image::from_fn(w, h, |x, y| {
            let i = y * w + x;
            if x < w / 2 {
                let n = [gauss(i, 11), gauss(i, 12), gauss(i, 13)];
                [0, 1, 2].map(|c| srgb_decode((enc(0.2) + sigma * n[c]).clamp(0.0, 1.0)))
            } else if x < 3 * w / 4 {
                [0.5, 0.12, 0.08]
            } else {
                [0.12, 0.35, 0.08]
            }
        })
    }

    /// RMS of chroma (encoded channel minus encoded luminance) around its mean.
    fn chroma_noise(img: &Image, xs: std::ops::Range<usize>, ys: std::ops::Range<usize>) -> f32 {
        let mut total = 0.0;
        for c in 0..3 {
            let v: Vec<f32> = ys
                .clone()
                .flat_map(|y| xs.clone().map(move |x| (x, y)))
                .map(|(x, y)| {
                    let p = img.get(x, y);
                    enc(p[c]) - enc(lum(p))
                })
                .collect();
            total += std_dev(&v).powi(2);
        }
        total.sqrt()
    }

    #[test]
    fn colour_nr_removes_chroma_noise_keeps_colour_edges_and_luminance() {
        let (w, h) = (160, 64);
        let img = chroma_noise_and_colour_edge(w, h, 0.03);
        let out = denoise(
            &img,
            NoiseReduction {
                color: 50.0,
                ..nr_off()
            },
        );
        assert_finite(&out, "colour NR");
        let (xs, ys) = (8..w / 2 - 24, 8..h - 8);
        let (before, after) = (
            chroma_noise(&img, xs.clone(), ys.clone()),
            chroma_noise(&out, xs, ys),
        );
        assert!(after < before / 3.0, "chroma noise {before} → {after}");
        // Luminance is unchanged everywhere.
        for (a, b) in img.rgb.chunks(3).zip(out.rgb.chunks(3)) {
            let (ya, yb) = (lum([a[0], a[1], a[2]]), lum([b[0], b[1], b[2]]));
            assert!((ya - yb).abs() <= 1e-5 * ya.max(1e-3), "{ya} → {yb}");
        }
        // The red/green edge keeps its colours two pixels either side.
        let edge = 3 * w / 4;
        for y in 8..h - 8 {
            for (x, want) in [
                (edge - 2, [0.5, 0.12, 0.08]),
                (edge + 1, [0.12, 0.35, 0.08]),
            ] {
                let p = out.get(x, y);
                for c in 0..3 {
                    assert!(
                        (enc(p[c]) - enc(want[c])).abs() < 0.03,
                        "({x},{y}) {p:?} vs {want:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn colour_nr_detail_protects_isoluminant_edges() {
        // Red/green pair of (nearly) equal luminance: the guide cannot see the edge.
        let (w, h) = (96, 32);
        let lw = luminance_weights();
        let red = [0.4f32, 0.0, 0.1];
        let y_red = dot(lw, &red);
        let g = (y_red - lw[0] * 0.05 - lw[2] * 0.1) / lw[1];
        let green = [0.05, g, 0.1];
        let img = Image::from_fn(w, h, |x, _| if x < w / 2 { red } else { green });
        let bleed = |detail: f64| {
            let out = denoise(
                &img,
                NoiseReduction {
                    color: 100.0,
                    color_detail: detail,
                    ..nr_off()
                },
            );
            let p = out.get(w / 2 - 2, h / 2);
            (enc(p[0]) - enc(red[0])).abs()
        };
        let (none, full) = (bleed(0.0), bleed(100.0));
        assert!(none > 0.02, "Detail 0 smooths across the edge: {none}");
        assert!(
            full < 0.25 * none,
            "Detail 100 protects it: {none} → {full}"
        );
    }

    #[test]
    fn colour_nr_smoothness_removes_larger_blotches() {
        // Chroma blotches of ~6 px: low-frequency colour noise.
        let (w, h) = (128, 64);
        let img = Image::from_fn(w, h, |x, y| {
            let cell = (y / 6) * 64 + x / 6;
            let n = [gauss(cell, 21), gauss(cell, 22), gauss(cell, 23)];
            [0, 1, 2].map(|c| srgb_decode(enc(0.2) + 0.03 * n[c]))
        });
        let residual = |smoothness: f64| {
            let out = denoise(
                &img,
                NoiseReduction {
                    color: 40.0,
                    color_smoothness: smoothness,
                    ..nr_off()
                },
            );
            chroma_noise(&out, 16..w - 16, 16..h - 16)
        };
        assert!(residual(100.0) < 0.8 * residual(0.0));
    }

    #[test]
    fn operators_are_deterministic() {
        let img = textured(203, 77);
        let nr = NoiseReduction {
            luminance: 40.0,
            luminance_contrast: 30.0,
            color: 60.0,
            ..NoiseReduction::default()
        };
        let sh = Sharpening {
            amount: 90.0,
            masking: 40.0,
            ..Sharpening::default()
        };
        let run = || sharpen(&denoise(&img, nr), sh);
        let (a, b) = (run(), run());
        assert_eq!(a.rgb, b.rgb);
        assert_finite(&a, "textured");
    }

    #[test]
    fn extreme_settings_keep_tiny_odd_black_and_white_images_finite() {
        let strong_nr = NoiseReduction {
            luminance: 100.0,
            luminance_detail: 100.0,
            luminance_contrast: 100.0,
            color: 100.0,
            color_detail: 0.0,
            color_smoothness: 100.0,
        };
        let strong_sharp = Sharpening {
            amount: 150.0,
            radius: 3.0,
            detail: 100.0,
            masking: 50.0,
        };
        let cases: Vec<(&str, Image)> = vec![
            ("1x1", Image::from_fn(1, 1, |_, _| [0.3, 0.2, 0.1])),
            (
                "2x3",
                Image::from_fn(2, 3, |x, y| [0.1 * x as f32, 0.3, 0.2 * y as f32]),
            ),
            ("1x9", Image::from_fn(1, 9, |_, y| grey(y as f32 / 8.0))),
            ("17x13", textured(17, 13)),
            ("black", Image::new(31, 19)),
            ("white", Image::from_fn(31, 19, |_, _| grey(1.0))),
            (
                "out of range",
                Image::from_fn(23, 11, |x, y| {
                    [
                        -0.05 * x as f32,
                        1.5 + y as f32 * 0.1,
                        (x * y) as f32 * 0.01,
                    ]
                }),
            ),
            ("empty", Image::new(0, 0)),
        ];
        for (name, img) in cases {
            for sh in [strong_sharp, Sharpening::default()] {
                let out = sharpen(&denoise(&img, strong_nr), sh);
                assert_finite(&out, name);
                let out = sharpen(&denoise(&img, NoiseReduction::default()), sh);
                assert_finite(&out, name);
            }
        }
        // Uniform frames stay (almost exactly) as they are.
        for v in [0.0, 1.0, 0.18] {
            let img = Image::from_fn(31, 19, |_, _| grey(v));
            let out = sharpen(&denoise(&img, strong_nr), strong_sharp);
            assert!(out.rgb.iter().all(|o| (o - v).abs() < 1e-5), "flat {v}");
        }
    }

    #[test]
    fn non_finite_sliders_fall_back_to_defaults() {
        let img = textured(40, 20);
        let p = Sharpening {
            amount: 60.0,
            radius: f64::NAN,
            detail: f64::INFINITY,
            masking: f64::NAN,
        };
        assert_finite(&sharpen(&img, p), "sharpen");
        let q = NoiseReduction {
            luminance: 30.0,
            luminance_detail: f64::NAN,
            luminance_contrast: f64::NEG_INFINITY,
            color: 30.0,
            color_detail: f64::NAN,
            color_smoothness: f64::NAN,
        };
        assert_finite(&denoise(&img, q), "denoise");
    }

    /// Timing on a 24 MP frame (run with `--release -- --ignored --nocapture`).
    #[test]
    #[ignore]
    fn timing_24mp_defaults() {
        let (w, h) = (6000, 4000);
        let img = Image::from_fn(w, h, |x, y| {
            let i = y * w + x;
            let v = 0.05 + 0.5 * ((x as f32 * 0.01).sin() * (y as f32 * 0.013).cos()).abs();
            [
                v * (1.0 + 0.05 * gauss(i, 1)),
                v * (1.0 + 0.05 * gauss(i, 2)),
                v * (1.0 + 0.05 * gauss(i, 3)),
            ]
        });
        let k = DetailStrengths::default();
        let time = |label: &str, f: &dyn Fn(&mut Image)| {
            let mut im = img.clone();
            let start = std::time::Instant::now();
            f(&mut im);
            eprintln!("{label}: {:.3} s", start.elapsed().as_secs_f64());
        };
        time("defaults (colour NR 25 + sharpening 40)", &|im| {
            apply_noise_reduction(im, &NoiseReduction::default(), &k);
            apply_sharpening(im, &Sharpening::default(), &k);
        });
        time("colour NR 25", &|im| {
            apply_noise_reduction(im, &NoiseReduction::default(), &k)
        });
        time("sharpening 40", &|im| {
            apply_sharpening(im, &Sharpening::default(), &k)
        });
        time("sharpening 100, radius 3, masking 50", &|im| {
            let p = Sharpening {
                amount: 100.0,
                radius: 3.0,
                detail: 50.0,
                masking: 50.0,
            };
            apply_sharpening(im, &p, &k)
        });
        time("luminance NR 50 + contrast 50", &|im| {
            let p = NoiseReduction {
                luminance: 50.0,
                luminance_contrast: 50.0,
                color: 0.0,
                ..NoiseReduction::default()
            };
            apply_noise_reduction(im, &p, &k)
        });
        time("colour NR 100, smoothness 100", &|im| {
            let p = NoiseReduction {
                color: 100.0,
                color_smoothness: 100.0,
                ..NoiseReduction::default()
            };
            apply_noise_reduction(im, &p, &k)
        });
    }
}
