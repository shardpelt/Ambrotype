// SPDX-License-Identifier: GPL-3.0-or-later
//! Detail panel: sharpening and noise reduction.
//!
//! Interface contract (pipeline/ depends on it):
//! - both run on the display-linear ProPhoto image near the end of the
//!   pipeline (noise reduction first, then sharpening);
//! - parameters use Camera Raw units and ranges (see the field docs);
//! - each is an exact no-op at its neutral values (amount 0) and keeps values finite.
//!
//! **Model v1 (ours, not Adobe's).** Camera Raw's sharpening and noise
//! reduction are closed source; what is known (docs/research/05 §12–13) is that
//! sharpening is a luminance-only unsharp mask whose Detail slider trades halo
//! suppression against texture, with an edge mask (Masking), and that noise
//! reduction is edge-preserving smoothing in an opponent space that adapts to
//! how noisy the image is (the same Color 25 default is used at every ISO).
//! Every operator here works on *encoded* values (the sRGB transfer, as for the
//! curves in pipeline/mod.rs), where shot noise and edge contrast are roughly
//! uniform, and writes back to the linear image as a luminance ratio
//! (sharpening, luminance NR) or with the luminance held fixed (colour NR), so
//! none of them shifts hue or puts colour fringes on edges.
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
//! **Noise estimate** (both NR operators). There is no camera noise profile at
//! this stage, so the noise is measured on the image: over 16×16-pixel tiles,
//! the power of a fine-scale noise signal (Haar diagonal detail of `L` for
//! luminance; the residual left after chroma smoothing for colour), and per
//! band of encoded luminance a low percentile of the tile powers. Flat tiles
//! show pure noise and textured tiles read higher, so the percentile tracks the
//! noise and not the texture; clipped tiles are skipped. A small floor keeps the
//! sliders effective on very clean images. The colour residual sees all colour
//! structure finer than the filter, so when no tile is flat (colour texture
//! filling the frame) its estimate is capped by a multiple of one that is
//! blind to stripes, edges and gradients in any of four directions (the least
//! of the Haar diagonal and the two diagonal differences of 2×2 blocks, at
//! full and at low resolution). Fine random colour texture with no flat
//! area left anywhere cannot be told from colour noise and is treated as noise.
//!
//! **Luminance noise reduction** (`LuminanceSmoothing`, `...Detail`, `...Contrast`):
//! 1. Self-guided filters on `L` at two scales (fine radius growing with the
//!    slider, then a coarser one). The guided filter's regularisation `eps` is
//!    `(slider · gain · σ_noise(L))²`, so regions whose local variance is below
//!    a multiple of the measured noise are flattened and stronger edges kept.
//! 2. *Detail* returns removed detail where its local RMS clearly exceeds the
//!    noise level (a soft threshold on a 3×3 energy), scaled by the slider.
//! 3. *Contrast* adds back a low-pass of what is still removed: pixel noise
//!    averages out of it, lost local contrast does not.
//!
//! **Colour noise reduction** (`ColorNoiseReduction`, `...Detail`, `...Smoothness`):
//! 1. Chroma = encoded R, G, B minus encoded luminance (three opponent channels).
//! 2. A box-downscaled copy (fixed factor 4) is smoothed with a guided filter
//!    steered by encoded luminance; the radius grows with strength and
//!    Smoothness (Smoothness removes the larger, low-frequency blotches). Its
//!    linear coefficients are upsampled and applied to the full-resolution
//!    luminance (He & Sun's fast guided filter), so smoothed chroma follows
//!    full-resolution luminance edges.
//! 3. *Detail* decides per pixel how much of the full-resolution residual `r`
//!    (original minus smoothed chroma) survives: a garrote
//!    `r·max(0, 1 − (τ/e)^p)`, where `e` is the RMS residual of the pixel's 8
//!    neighbours and `τ` a multiple of the measured residual noise that falls
//!    as Detail rises. Noise is removed; colour edges, thin colour lines and
//!    colour texture well above the noise (isoluminant red/green boundaries,
//!    grass against soil in a clean frame) are kept, while an isolated speckle,
//!    however saturated, has no support among its neighbours and goes. Detail
//!    0 keeps nothing (bleeds), high Detail lets some noise through.
//! 4. Each pixel is rebuilt from the new chroma and rescaled to its original
//!    linear luminance, so colour NR never changes brightness.
//!
//! All radii are in pixels, not scaled with the image size: noise and
//! sharpening act at the sensor's pixel scale, as Lightroom's do at 100%.
//! None is rounded to whole pixels (box windows take fractional edge taps, the
//! sharpening min/max window interpolates between two sizes), so every output
//! changes continuously with its sliders. Non-finite pixels are left as they
//! are and cannot spread: they enter the filters as a bounded stand-in
//! (sharpening, luminance NR) or not at all (colour NR).
//! Camera Raw's defaults (Sharpness 40, Color 25) run on every render, so the
//! default path is kept to a few streaming full-resolution passes with no
//! full-size blur: ≈1.0 s per 24 MP frame on 4 cores from cold (0.25 s
//! sharpening, 0.65 s colour NR), see `timing_24mp_defaults`; Luminance NR
//! (off by default) adds ≈1.4 s.
//! Every constant lives in [`DetailStrengths`] for the calibration harness to
//! fit against Lightroom sweeps (docs/lightroom-trial-plan.md).

use std::sync::OnceLock;

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::buffer::{Image, luminance_weights};
use crate::filters::{box_blur, gaussian_by, gaussian_reach};

mod color_nr;
mod luminance_nr;
mod noise;
mod sharpen;
mod util;

pub use color_nr::*;
pub use luminance_nr::*;
use noise::*;
pub use sharpen::*;
use util::*;

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

    /// Noise estimate: percentile (0..1) of the tile noise powers within one
    /// luminance band that is taken as that band's noise power. Low values
    /// pick the flattest tiles; textured tiles read higher.
    pub noise_percentile: f64,

    /// Luminance NR: noise sigma (encoded) added to the measured one, so the
    /// slider still smooths a very clean image.
    pub luminance_noise_floor: f64,
    /// Luminance NR: smoothing threshold at Luminance 100 in multiples of the
    /// noise sigma (proportional to the slider). The guided filter's eps is
    /// the square of threshold · sigma.
    pub luminance_noise_gain: f64,
    /// Luminance NR: fine-scale guided-filter radius (pixels) at slider 0 ...
    pub luminance_radius: f64,
    /// ... plus this many pixels at slider 100.
    pub luminance_radius_per_strength: f64,
    /// Luminance NR: coarse-scale radius as a multiple of the fine radius.
    pub luminance_coarse_radius: f64,
    /// Luminance NR: coarse-scale eps as a multiple of the fine eps.
    pub luminance_coarse_eps: f64,
    /// Luminance NR Detail: local RMS of the removed signal, in multiples of the
    /// noise sigma, above which it counts as detail rather than noise.
    pub luminance_detail_threshold: f64,
    /// Luminance NR Detail: share of above-threshold detail returned at Detail 100.
    pub luminance_detail_restore: f64,
    /// Luminance NR Contrast: Gaussian sigma (pixels) of the low-pass of the
    /// removed signal that is added back.
    pub luminance_contrast_sigma: f64,
    /// Luminance NR Contrast: share of that low-pass added back at Contrast 100.
    pub luminance_contrast_restore: f64,

    /// Colour NR: residual noise sigma (encoded chroma) added to the measured
    /// one, so a perfectly clean image still gets a non-zero Detail threshold.
    pub color_noise_floor: f64,
    /// Colour NR: ceiling on the measured residual noise sigma, in multiples of
    /// a second chroma noise estimate that is blind to texture running along
    /// the horizontal, vertical or diagonal directions (the least of three
    /// 2×2-block detail signals, at full resolution and on the downscaled
    /// chroma, which sees noise correlated over a few pixels). The residual
    /// holds colour texture too, so in a frame with no flat tiles its
    /// percentile reads texture; the ceiling keeps such texture. On real raws
    /// the second estimate reads ≈ 0.9–1.5× the residual one, so 2 leaves
    /// their noise estimate alone.
    pub color_noise_ceiling: f64,
    /// Colour NR: smoothing radius (full-resolution pixels) at slider 0 ...
    pub color_radius: f64,
    /// ... plus this many pixels at Color 100 ...
    pub color_radius_per_strength: f64,
    /// ... plus this many at Smoothness 100.
    pub color_radius_per_smoothness: f64,
    /// Colour NR: slider value (0..1) from which the denoised chroma fully
    /// replaces the original; below it the two are blended linearly.
    pub color_full_strength: f64,
    /// Colour NR: guided-filter eps (encoded luminance squared). Luminance
    /// structure with a variance well above it steers the chroma.
    pub color_guide_eps: f64,
    /// Colour NR: largest |slope| of chroma against luminance the guided filter
    /// may use. Near weak luminance edges the slope can get large and would
    /// copy full-resolution luminance noise into the chroma.
    pub color_guide_max_slope: f64,
    /// Colour NR Detail: garrote threshold on the neighbourhood RMS residual, in
    /// residual noise sigmas, at Detail 100 ...
    pub color_detail_threshold: f64,
    /// ... times `(detail / 100)^-power` below that (nothing kept at Detail 0).
    pub color_detail_power: f64,
    /// Colour NR Detail: garrote exponent `p` in `r·max(0, 1 − (τ/|r|)^p)`; 2 is
    /// the classic non-negative garrote, larger values approach a hard
    /// threshold (edges kept more fully, noise just above `τ` too).
    pub color_garrote_power: f64,
    /// Colour NR Detail: relative rise of that threshold at Color 100.
    pub color_threshold_per_strength: f64,
    /// Colour NR: unused, kept so that older calibration files still load. It
    /// used to pick the downscale factor from the radius, which made the output
    /// jump where the factor changed; the factor is now `color_max_downscale`.
    pub color_low_res_radius: f64,
    /// Colour NR: downscale factor of the guided filter's low-resolution pass
    /// (rounded, 1..64, at most the image's smaller side). Fixed, so the output
    /// changes continuously with the sliders; the low-resolution radius is the
    /// full-resolution one divided by it (fractional box windows).
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

    noise_percentile: 0.15,

    luminance_noise_floor: 0.001,
    luminance_noise_gain: 4.0,
    luminance_radius: 1.0,
    luminance_radius_per_strength: 3.0,
    luminance_coarse_radius: 2.0,
    luminance_coarse_eps: 0.5,
    luminance_detail_threshold: 2.0,
    luminance_detail_restore: 1.0,
    luminance_contrast_sigma: 2.0,
    luminance_contrast_restore: 1.0,

    color_noise_floor: 0.002,
    color_noise_ceiling: 2.0,
    color_radius: 1.0,
    color_radius_per_strength: 16.0,
    color_radius_per_smoothness: 8.0,
    color_full_strength: 0.25,
    color_guide_eps: 1e-3,
    color_guide_max_slope: 2.0,
    color_detail_threshold: 1.25,
    color_detail_power: 1.0,
    color_garrote_power: 4.0,
    color_threshold_per_strength: 0.5,
    color_low_res_radius: 2.0,
    color_max_downscale: 4.0,
};

/// Linear luminance below which a pixel is too dark to scale; changes are
/// added as neutral grey instead (sharpening, luminance NR) or skipped (colour NR).
const MIN_SCALE_Y: f32 = 1e-6;

/// Encoded luminance is centred on this value inside the guided filters, which
/// keeps their running sums small and their variances accurate in `f32`.
const CENTRE: f32 = 0.5;

/// Linear luminance is clamped to ±this before it is encoded (and so are the
/// channels averaged by the colour-NR downscale): an extreme highlight then
/// acts like a bright point instead of swamping the running sums of the box
/// filters. Non-finite pixels are left as they are and skipped by the filters.
const MAX_LINEAR: f32 = 1e3;

/// Upper bounds on pixel quantities derived from [`DetailStrengths`], so a
/// runaway calibration value can neither exhaust memory nor stall a render.
const MAX_SHARPEN_SIGMA: f64 = 10.0;
const MAX_HALO_WINDOW: f64 = 16.0;
const MAX_BLUR_SIGMA: f64 = 32.0;
const MAX_LUMINANCE_RADIUS: f64 = 64.0;
const MAX_COLOR_RADIUS: f64 = 256.0;
const MAX_DOWNSCALE: f64 = 64.0;

/// How far (pixels) noise reduction reaches: a pixel's result depends only on
/// the image within this distance, apart from the noise level, which is
/// measured over the whole image.
pub fn noise_reduction_reach(p: &NoiseReduction, k: &DetailStrengths) -> usize {
    let mut reach = 0;
    if p.luminance > 0.0 && p.luminance.is_finite() {
        let s = p.luminance.min(100.0) / 100.0;
        let r_fine = (k.luminance_radius + k.luminance_radius_per_strength.max(0.0) * s)
            .clamp(1.0, MAX_LUMINANCE_RADIUS);
        let r_coarse = (r_fine * k.luminance_coarse_radius.max(0.0)).min(MAX_LUMINANCE_RADIUS);
        let guided = |r: f64| 2 * (r.ceil() as usize + 1);
        reach += guided(r_fine)
            + guided(r_coarse)
            + 1
            + gaussian_reach(k.luminance_contrast_sigma.clamp(0.0, MAX_BLUR_SIGMA) as f32);
    }
    if p.color > 0.0 && p.color.is_finite() {
        let s = p.color.min(100.0) / 100.0;
        let smoothness = slider(p.color_smoothness, 50.0, 0.0, 100.0) / 100.0;
        let radius = (k.color_radius
            + k.color_radius_per_strength.max(0.0) * s
            + k.color_radius_per_smoothness.max(0.0) * smoothness)
            .clamp(1.0, MAX_COLOR_RADIUS);
        let factor = k.color_max_downscale.round().clamp(1.0, MAX_DOWNSCALE) as usize;
        let r_low = radius / factor as f64;
        reach += (2 * (r_low.ceil() as usize + 1) + 2) * factor + 2;
    }
    reach
}

/// How far (pixels) sharpening reaches.
pub fn sharpening_reach(p: &Sharpening, k: &DetailStrengths) -> usize {
    if !p.amount.is_finite() || p.amount <= 0.0 {
        return 0;
    }
    let radius = slider(p.radius, 1.0, 0.5, 3.0);
    let sigma = (radius * k.sharpen_sigma_per_radius.max(0.0)).min(MAX_SHARPEN_SIGMA);
    let window = (radius * k.halo_window_per_radius.max(0.0)).clamp(1.0, MAX_HALO_WINDOW);
    let masking = slider(p.masking, 0.0, 0.0, 100.0) > 0.0;
    let mask = if masking {
        gaussian_reach(k.mask_blur_sigma.clamp(0.0, MAX_BLUR_SIGMA) as f32)
            + 1
            + gaussian_reach(k.mask_smooth_sigma.clamp(0.0, MAX_BLUR_SIGMA) as f32)
    } else {
        0
    };
    ((3.0 * sigma).ceil() as usize)
        .max(window.ceil() as usize + 1)
        .max(mask)
        + 1
}

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

#[cfg(test)]
mod tests;
