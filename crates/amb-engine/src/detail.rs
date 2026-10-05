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
//! reduction is edge-preserving smoothing in an opponent space that adapts to
//! how noisy the image is (the same Color 25 default is used at every ISO).
//! Every operator here works on *encoded* values (the sRGB transfer, as for the
//! curves in pipeline.rs), where shot noise and edge contrast are roughly
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
    if w == 0 || h == 0 || !p.amount.is_finite() || p.amount <= 0.0 {
        return;
    }
    let gain = (p.amount.min(150.0) / 100.0 * k.sharpen_gain.max(0.0)) as f32;
    if !gain.is_finite() || gain <= 0.0 {
        return;
    }
    let radius = slider(p.radius, 1.0, 0.5, 3.0);
    let detail = slider(p.detail, 25.0, 0.0, 100.0) / 100.0;
    let masking = slider(p.masking, 0.0, 0.0, 100.0) / 100.0;
    let t = transfer();
    let lw = luminance_weights();
    let max_ratio = k.max_luminance_ratio.max(1.0) as f32;

    // 1. Encoded luminance; the unsharp-mask blur is computed row by row below.
    let l = encoded_luminance(img, 0.0);
    let sigma = (radius * k.sharpen_sigma_per_radius.max(0.0)).min(MAX_SHARPEN_SIGMA);
    let kernel = gaussian_kernel(sigma as f32);

    // 2. Edge mask (Masking > 0 only).
    let mask = (masking > 0.0).then(|| edge_mask(&l, w, h, masking, k));

    // 3. Sharpen with overshoot limited to the local min/max plus a margin. The
    //    window radius is fractional: the bounds are interpolated between the
    //    windows of radius `win` and `win + 1`, so they change continuously
    //    with the Radius slider.
    let margin = halo_margin(detail, k);
    let win_r = (radius * k.halo_window_per_radius.max(0.0)).clamp(1.0, MAX_HALO_WINDOW);
    let win = win_r.floor() as usize;
    let win_frac = (win_r - win as f64) as f32;
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
struct SharpenRows {
    tmp: Vec<f32>,
    pad: Vec<f32>,
    blur: Vec<f32>,
    vmin: Vec<f32>,
    vmax: Vec<f32>,
    omin: Vec<f32>,
    omax: Vec<f32>,
}

impl SharpenRows {
    fn new(w: usize, kernel_radius: usize) -> SharpenRows {
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
fn min_max_row(lo: &mut [f32], hi: &mut [f32], src: &[f32]) {
    for ((lo, hi), &v) in lo.iter_mut().zip(hi.iter_mut()).zip(src) {
        *lo = lo.min(v);
        *hi = hi.max(v);
    }
}

/// Normalised Gaussian kernel (radius ⌈3σ⌉), the one `filters::gaussian_blur`
/// uses for σ ≤ 3; identity below σ = 0.3, as there.
fn gaussian_kernel(sigma: f32) -> Vec<f32> {
    if sigma.is_nan() || sigma < 0.3 {
        return vec![1.0];
    }
    let r = (3.0 * sigma).ceil() as isize;
    let mut k: Vec<f32> = (-r..=r)
        .map(|i| (-(i * i) as f32 / (2.0 * sigma * sigma)).exp())
        .collect();
    let s: f32 = k.iter().sum();
    k.iter_mut().for_each(|v| *v /= s);
    k
}

/// Row `y` of `plane` convolved with a separable odd `kernel`, edges clamped:
/// vertical pass into `tmp`, edge-padded into `pad`, horizontal pass into `out`.
/// Streaming (no full-size intermediate, no transposes), so cheap enough for
/// the sharpening that runs on every render.
#[allow(clippy::too_many_arguments)]
fn convolve_row(
    plane: &[f32],
    w: usize,
    h: usize,
    y: usize,
    kernel: &[f32],
    tmp: &mut [f32],
    pad: &mut [f32],
    out: &mut [f32],
) {
    let r = kernel.len() / 2;
    tmp.fill(0.0);
    for (j, &kj) in kernel.iter().enumerate() {
        let yy = (y + j).saturating_sub(r).min(h - 1);
        for (t, &v) in tmp.iter_mut().zip(&plane[yy * w..(yy + 1) * w]) {
            *t += kj * v;
        }
    }
    pad[..r].fill(tmp[0]);
    pad[r..r + w].copy_from_slice(tmp);
    pad[r + w..].fill(tmp[w - 1]);
    out.fill(0.0);
    for (j, &kj) in kernel.iter().enumerate() {
        for (o, &v) in out.iter_mut().zip(&pad[j..j + w]) {
            *o += kj * v;
        }
    }
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

/// Sharpening mask from encoded luminance: smoothstep of the gradient magnitude
/// of a lightly blurred copy around a threshold set by Masking (0..1), softened.
fn edge_mask(l: &[f32], w: usize, h: usize, masking: f64, k: &DetailStrengths) -> Vec<f32> {
    let blurred = gaussian_blur(l, w, h, k.mask_blur_sigma.clamp(0.0, MAX_BLUR_SIGMA) as f32);
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
    gaussian_blur(
        &raw,
        w,
        h,
        k.mask_smooth_sigma.clamp(0.0, MAX_BLUR_SIGMA) as f32,
    )
}

fn luminance_noise_reduction(img: &mut Image, p: &NoiseReduction, k: &DetailStrengths) {
    let (w, h) = (img.width, img.height);
    let s = p.luminance.min(100.0) / 100.0;
    let detail = (slider(p.luminance_detail, 50.0, 0.0, 100.0) / 100.0) as f32;
    let contrast = (slider(p.luminance_contrast, 0.0, 0.0, 100.0) / 100.0) as f32;
    let gain = (s * k.luminance_noise_gain.max(0.0)) as f32;
    if !gain.is_finite() || gain <= 0.0 {
        return;
    }
    let t = transfer();
    let lw = luminance_weights();
    let max_ratio = k.max_luminance_ratio.max(1.0) as f32;

    // 1. Noise level per luminance, then two-scale self-guided smoothing of the
    //    centred encoded luminance with eps set by it.
    let l = encoded_luminance(img, CENTRE);
    let noise = NoiseProfile::from_tiles(&luminance_tiles(&l, w, h), k.noise_percentile);
    let floor = k.luminance_noise_floor.max(0.0) as f32;
    let sigma_n = Lut::from_fn(|v| noise.at(v) + floor);
    let eps = Lut::from_fn(|v| (gain * sigma_n.at(v)).powi(2));
    if !(eps.max() > 0.0 && eps.max().is_finite()) {
        return;
    }
    // Fractional radii (box windows with partial edge taps), so the result
    // changes continuously with the slider.
    let r_fine = (k.luminance_radius + k.luminance_radius_per_strength.max(0.0) * s)
        .clamp(1.0, MAX_LUMINANCE_RADIUS);
    let r_coarse = (r_fine * k.luminance_coarse_radius.max(0.0)).min(MAX_LUMINANCE_RADIUS);
    let coarse_eps = k.luminance_coarse_eps.max(0.0) as f32;
    let mut smooth = self_guided(&l, w, h, r_fine as f32, &eps, 1.0);
    if r_coarse > 0.0 && coarse_eps > 0.0 {
        smooth = self_guided(&smooth, w, h, r_coarse as f32, &eps, coarse_eps);
    }

    // 2. Detail: share of the removed signal returned per pixel, from its local
    //    energy against the noise level.
    let removed: Vec<f32> = l.par_iter().zip(&smooth).map(|(a, b)| a - b).collect();
    let restore = k.luminance_detail_restore.max(0.0) as f32 * detail;
    let threshold = k.luminance_detail_threshold.max(0.0) as f32;
    let kept: Option<Vec<f32>> = (restore > 0.0).then(|| {
        let sq: Vec<f32> = removed.par_iter().map(|v| v * v).collect();
        let mut energy = box_blur(&sq, w, h, 1);
        energy.par_iter_mut().zip(&smooth).for_each(|(e, &m)| {
            // Soft threshold: ~0 for noise-level energy, → `restore` for detail.
            let tau = threshold * sigma_n.at(m + CENTRE);
            *e = if tau > 0.0 {
                let q = *e / (tau * tau);
                restore * q * q / (1.0 + q * q)
            } else {
                restore
            };
        });
        energy
    });

    // 3. Contrast: low-pass of what is still removed after the Detail step.
    let contrast_gain = k.luminance_contrast_restore.max(0.0) as f32 * contrast;
    let lowpass: Option<Vec<f32>> = (contrast_gain > 0.0).then(|| {
        let rest: Vec<f32> = match &kept {
            Some(kept) => removed
                .par_iter()
                .zip(kept)
                .map(|(r, f)| r * (1.0 - f))
                .collect(),
            None => removed.clone(),
        };
        gaussian_blur(
            &rest,
            w,
            h,
            k.luminance_contrast_sigma.clamp(0.0, MAX_BLUR_SIGMA) as f32,
        )
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
                if !y_old.is_finite() {
                    continue;
                }
                let y_new =
                    (y_old + t.decode(v + CENTRE) - t.decode(lv + CENTRE)).max(y_old.min(0.0));
                set_luminance(px, y_old, y_new, max_ratio);
            }
        });
}

/// Self-guided filter (guide = source): edge-preserving smoothing where local
/// variance below `eps_scale · eps(window mean)` is flattened. Variance is
/// clamped at zero so `f32` rounding in the running sums can never make the
/// coefficient leave 0..1.
fn self_guided(src: &[f32], w: usize, h: usize, r: f32, eps: &Lut, eps_scale: f32) -> Vec<f32> {
    let mean = box_blur_frac(src, w, h, r);
    let sq: Vec<f32> = src.par_iter().map(|v| v * v).collect();
    let corr = box_blur_frac(&sq, w, h, r);
    drop(sq);
    let (a, b): (Vec<f32>, Vec<f32>) = mean
        .par_iter()
        .zip(&corr)
        .map(|(&m, &c)| {
            let var = (c - m * m).max(0.0);
            let e = eps_scale * eps.at(m + CENTRE);
            let a = if var + e > 0.0 { var / (var + e) } else { 1.0 };
            (a, m * (1.0 - a))
        })
        .unzip();
    drop((mean, corr));
    let mean_a = box_blur_frac(&a, w, h, r);
    drop(a);
    let mean_b = box_blur_frac(&b, w, h, r);
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
    if !mix.is_finite() || mix <= 0.0 {
        return;
    }
    let t = transfer();
    let lw = luminance_weights();

    // Scale: a fixed downscale factor and the full-resolution radius divided by
    // it (fractional), so the output changes continuously with the sliders.
    let radius = (k.color_radius
        + k.color_radius_per_strength.max(0.0) * s
        + k.color_radius_per_smoothness.max(0.0) * smoothness)
        .clamp(1.0, MAX_COLOR_RADIUS);
    let factor = k
        .color_max_downscale
        .round()
        .clamp(1.0, MAX_DOWNSCALE)
        .min(w.min(h) as f64) as usize;
    let r_low = (radius / factor as f64) as f32;

    // 1. Low resolution: centred encoded luminance (the guide) and chroma.
    let (small, sw, sh) = downscale(img, factor);
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
    let detail_factor = color_detail_factor(detail, s, k);
    // Haar tiles of the low-resolution chroma (for the noise ceiling, below).
    let low_tiles = detail_factor.map(|_| {
        let tile = ((NOISE_TILE / factor) & !1).max(2);
        let srcs = [&chroma[0][..], &chroma[1][..], &chroma[2][..]];
        haar_tiles(&guide, CENTRE, &srcs, sw, sh, tile, true)
    });

    // 2. Guided-filter coefficients for the three chroma channels.
    let eps = k.color_guide_eps.max(1e-9) as f32;
    let max_slope = k.color_guide_max_slope.max(0.0) as f32;
    let coefs = guided_coefficients(&guide, &chroma, sw, sh, r_low, eps, max_slope);
    drop((guide, chroma));
    let grids = CoefGrids::new(&coefs, w, sw, sh, factor);

    // 3. Detail: residual energy per pixel, and the garrote threshold per
    //    luminance from its noise level.
    let detail_test: Option<(Vec<f32>, Lut)> = detail_factor.map(|f| {
        let (energy, tiles, fine_tiles) = residual_energy(img, &grids);
        let q = k.noise_percentile;
        let noise = NoiseProfile::from_tiles(&tiles, q);
        // Ceiling from the directional chroma detail at full and low resolution
        // (white noise of σ reads σ/factor on the downscaled chroma).
        let fine = NoiseProfile::from_tiles(&fine_tiles, q);
        let coarse = NoiseProfile::from_tiles(low_tiles.as_deref().unwrap_or(&[]), q);
        let ceiling = k.color_noise_ceiling.max(0.0) as f32;
        let floor = k.color_noise_floor.max(0.0) as f32;
        let tau = Lut::from_fn(|v| {
            let cap = ceiling * fine.at(v).max(factor as f32 * coarse.at(v));
            f * (noise.at(v).min(cap) + floor)
        });
        (energy, tau)
    });
    let garrote = Garrote::new(k.color_garrote_power);

    // 4. Full resolution: denoised chroma = smoothed + garrote(residual), blended
    //    in by `mix`; luminance kept.
    img.rgb.par_chunks_mut(w * 3).enumerate().for_each_init(
        || vec![0.0f32; 6 * sw],
        |rows, (y, row)| {
            grids.interpolate_rows(y, rows);
            for (x, px) in row.chunks_exact_mut(3).enumerate() {
                let Some(c) = chroma_residual(px, &grids, rows, x) else {
                    continue;
                };
                let r2 = c.r[0] * c.r[0] + c.r[1] * c.r[1] + c.r[2] * c.r[2];
                let keep = detail_test.as_ref().map_or(0.0, |(energy, tau)| {
                    let tv = tau.at(c.le);
                    garrote.keep(tv * tv / neighbour_energy(energy, w, h, x, y))
                });
                // New chroma `smoothed + keep·r`, blended by `mix`: a change of
                // `mix·(keep − 1)·r` to each encoded channel.
                let f = mix * (keep - 1.0);
                if f == 0.0 || r2 == 0.0 {
                    continue;
                }
                let mut out = [0.0f32; 3];
                for (o, (&e, &r)) in out.iter_mut().zip(c.e.iter().zip(&c.r)) {
                    // Never push a channel below zero that was not negative already.
                    *o = t.decode((e + f * r).max(e.min(0.0)));
                }
                let y_new = dot(lw, &out);
                if y_new > MIN_SCALE_Y {
                    let scale = c.y / y_new;
                    for (v, o) in px.iter_mut().zip(out) {
                        *v = o * scale;
                    }
                }
            }
        },
    );
}

/// Mean residual energy of the 8 neighbours of `(x, y)` (edges clamped): the
/// evidence that the pixel's residual is detail. Edges and texture show it on
/// both sides; an isolated speckle, however strong, does not.
#[inline]
fn neighbour_energy(energy: &[f32], w: usize, h: usize, x: usize, y: usize) -> f32 {
    let (xm, xp) = (x.saturating_sub(1), (x + 1).min(w - 1));
    let row = |yy: usize| &energy[yy * w..(yy + 1) * w];
    let (up, mid, down) = (row(y.saturating_sub(1)), row(y), row((y + 1).min(h - 1)));
    (up[xm] + up[x] + up[xp] + mid[xm] + mid[xp] + down[xm] + down[x] + down[xp]) / 8.0
}

/// Garrote threshold factor (in noise sigmas) for a Detail value in 0..1 and a
/// strength in 0..1; `None` (keep no residual) at Detail 0.
fn color_detail_factor(detail: f64, s: f64, k: &DetailStrengths) -> Option<f32> {
    if detail.is_nan() || detail <= 0.0 {
        return None;
    }
    let at_100 = k.color_detail_threshold.max(0.0);
    let falloff = detail.powf(-k.color_detail_power.max(0.0));
    let f = at_100 * falloff * (1.0 + k.color_threshold_per_strength.max(0.0) * s);
    f.is_finite().then_some(f as f32)
}

/// Share `max(0, 1 − (τ/|r|)^p)` of a residual kept by the garrote, from
/// `q = τ²/|r|²` (fast paths for the common exponents).
#[derive(Debug, Clone, Copy)]
enum Garrote {
    Two,
    Four,
    Power(f32),
}

impl Garrote {
    fn new(p: f64) -> Garrote {
        let p = slider(p, DEFAULT_STRENGTHS.color_garrote_power, 0.1, 64.0);
        if p == 2.0 {
            Garrote::Two
        } else if p == 4.0 {
            Garrote::Four
        } else {
            Garrote::Power((p / 2.0) as f32)
        }
    }

    #[inline]
    fn keep(self, q: f32) -> f32 {
        // q ≥ 1 (|r| ≤ τ) and NaN (r = τ = 0) keep nothing.
        if q.is_nan() || q >= 1.0 {
            return 0.0;
        }
        1.0 - match self {
            Garrote::Two => q,
            Garrote::Four => q * q,
            Garrote::Power(half) => q.powf(half),
        }
    }
}

/// Upsampling of the low-resolution guided-filter coefficients: per output row
/// the six grids (`a` then `b` for R, G, B) are interpolated vertically into a
/// row buffer once, then horizontally per pixel.
struct CoefGrids<'a> {
    grids: [&'a [f32]; 6],
    sw: usize,
    sh: usize,
    factor: usize,
    columns: Vec<(usize, usize, f32)>,
}

impl<'a> CoefGrids<'a> {
    fn new(
        coefs: &'a [(Vec<f32>, Vec<f32>)],
        w: usize,
        sw: usize,
        sh: usize,
        factor: usize,
    ) -> Self {
        CoefGrids {
            grids: [
                &coefs[0].0,
                &coefs[1].0,
                &coefs[2].0,
                &coefs[0].1,
                &coefs[1].1,
                &coefs[2].1,
            ],
            sw,
            sh,
            factor,
            columns: (0..w).map(|x| lerp_position(x, factor, sw)).collect(),
        }
    }

    /// Vertical interpolation of every grid for full-resolution row `y` into `rows` (6 × sw).
    fn interpolate_rows(&self, y: usize, rows: &mut [f32]) {
        let sw = self.sw;
        let (y0, y1, wy) = lerp_position(y, self.factor, self.sh);
        for (g, dst) in self.grids.iter().zip(rows.chunks_exact_mut(sw)) {
            let (p0, p1) = (&g[y0 * sw..(y0 + 1) * sw], &g[y1 * sw..(y1 + 1) * sw]);
            for ((d, &a), &b) in dst.iter_mut().zip(p0).zip(p1) {
                *d = a + (b - a) * wy;
            }
        }
    }

    /// Coefficients at column `x` from rows filled by [`Self::interpolate_rows`].
    #[inline]
    fn at(&self, rows: &[f32], x: usize) -> [f32; 6] {
        let (x0, x1, wx) = self.columns[x];
        std::array::from_fn(|g| {
            let r = &rows[g * self.sw..(g + 1) * self.sw];
            r[x0] + (r[x1] - r[x0]) * wx
        })
    }
}

/// One pixel's encoded channels and its chroma residual against the smoothed
/// chroma (`a · (le − CENTRE) + b` per channel).
struct ChromaResidual {
    /// Linear luminance.
    y: f32,
    /// Encoded luminance.
    le: f32,
    /// Encoded R, G, B.
    e: [f32; 3],
    /// Chroma minus smoothed chroma.
    r: [f32; 3],
}

#[inline]
fn chroma_residual(
    px: &[f32],
    grids: &CoefGrids,
    rows: &[f32],
    x: usize,
) -> Option<ChromaResidual> {
    let t = transfer();
    let y = dot(luminance_weights(), px);
    // Too dark to rescale, or not finite (left untouched).
    if !(y > MIN_SCALE_Y && y.is_finite() && px.iter().all(|v| v.is_finite())) {
        return None;
    }
    let le = t.encode(y);
    let lc = le - CENTRE;
    let coef = grids.at(rows, x);
    let e = [t.encode(px[0]), t.encode(px[1]), t.encode(px[2])];
    let r = std::array::from_fn(|c| e[c] - le - (coef[c] * lc + coef[3 + c]));
    Some(ChromaResidual { y, le, e, r })
}

/// The colour-NR residual energy `|r|²` (summed over channels) per pixel (0
/// where the pixel is too dark to scale or not finite), its tile statistics,
/// and from the same pass the tile statistics of the directional detail of
/// the chroma (the least of the three [`haar_powers`], summed over channels).
fn residual_energy(img: &Image, grids: &CoefGrids) -> (Vec<f32>, Vec<TileStat>, Vec<TileStat>) {
    let (w, h) = (img.width, img.height);
    let t = transfer();
    let lw = luminance_weights();
    let mut energy = vec![0.0f32; w * h];
    let (tiles, fine): (Vec<Vec<TileStat>>, Vec<Vec<TileStat>>) = energy
        .par_chunks_mut(NOISE_TILE * w)
        .enumerate()
        .map(|(ty, band)| {
            let mut rows = vec![0.0f32; 6 * grids.sw];
            let mut acc = vec![TileAcc::default(); w.div_ceil(NOISE_TILE)];
            let mut haar = vec![TileAcc::default(); w.div_ceil(NOISE_TILE)];
            // Encoded luminance and chroma of the previous and current rows.
            let (mut prev, mut cur) = (vec![[0.0f32; 4]; w], vec![[0.0f32; 4]; w]);
            for (dy, out) in band.chunks_exact_mut(w).enumerate() {
                let y = ty * NOISE_TILE + dy;
                grids.interpolate_rows(y, &mut rows);
                let row = &img.rgb[y * w * 3..(y + 1) * w * 3];
                for (x, ((o, px), lc)) in out
                    .iter_mut()
                    .zip(row.chunks_exact(3))
                    .zip(cur.iter_mut())
                    .enumerate()
                {
                    if let Some(c) = chroma_residual(px, grids, &rows, x) {
                        *o = c.r[0] * c.r[0] + c.r[1] * c.r[1] + c.r[2] * c.r[2];
                        acc[x / NOISE_TILE].add([*o; 3], c.le, c.le, c.le);
                        *lc = [c.le, c.e[0] - c.le, c.e[1] - c.le, c.e[2] - c.le];
                    } else {
                        *lc = [t.encode(bounded(dot(lw, px))), 0.0, 0.0, 0.0];
                    }
                }
                if dy % 2 == 1 {
                    for (cx, (p0, p1)) in prev.chunks_exact(2).zip(cur.chunks_exact(2)).enumerate()
                    {
                        let block = |i: usize| [p0[0][i], p0[1][i], p1[0][i], p1[1][i]];
                        let power = (1..4).fold([0.0; 3], |p, i| add3(p, haar_powers(block(i))));
                        add_haar_block(&mut haar[2 * cx / NOISE_TILE], block(0), 0.0, power);
                    }
                }
                std::mem::swap(&mut prev, &mut cur);
            }
            let finish = |a: Vec<TileAcc>| a.into_iter().filter_map(TileAcc::finish).collect();
            (finish(acc), finish(haar))
        })
        .unzip();
    (energy, tiles.concat(), fine.concat())
}

/// Tile statistics of the Haar diagonal detail of the centred encoded
/// luminance (see [`haar_tiles`]).
fn luminance_tiles(l: &[f32], w: usize, h: usize) -> Vec<TileStat> {
    haar_tiles(l, CENTRE, &[l], w, h, NOISE_TILE, false)
}

/// Powers of three detail signals of a 2×2 block `[a, b; c, d]`, each of
/// variance σ² for white noise of σ: the Haar diagonal `(a − b − c + d)/2`
/// (blind to horizontal and vertical structure and to gradients), and the
/// differences `(b − c)/√2` and `(a − d)/√2` along the two diagonals (blind to
/// structure along them). The smallest tile sum of the three reads noise but
/// not texture that runs along one of the four directions (stripes, weaves).
#[inline]
fn haar_powers(v: [f32; 4]) -> [f32; 3] {
    let hv = 0.5 * (v[0] - v[1] - v[2] + v[3]);
    let (d1, d2) = (v[1] - v[2], v[0] - v[3]);
    [hv * hv, 0.5 * d1 * d1, 0.5 * d2 * d2]
}

#[inline]
fn add3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

/// Add one 2×2 block with encoded luminance `lum` (minus `offset`) and noise
/// power `power` to a tile.
#[inline]
fn add_haar_block(acc: &mut TileAcc, lum: [f32; 4], offset: f32, power: [f32; 3]) {
    let lo = lum[0].min(lum[1]).min(lum[2].min(lum[3])) + offset;
    let hi = lum[0].max(lum[1]).max(lum[2].max(lum[3])) + offset;
    let mean = 0.25 * (lum[0] + lum[1] + lum[2] + lum[3]) + offset;
    acc.add(power, mean, lo, hi);
}

/// Statistics over `tile`×`tile`-pixel tiles of the detail of 2×2 blocks,
/// its power summed over the `srcs` planes; banded and clipped by `lum`
/// (encoded luminance minus `offset`). The detail is the Haar diagonal, or
/// with `directional` the least of the three [`haar_powers`].
fn haar_tiles(
    lum: &[f32],
    offset: f32,
    srcs: &[&[f32]],
    w: usize,
    h: usize,
    tile: usize,
    directional: bool,
) -> Vec<TileStat> {
    let (cw, ch) = (w / 2, h / 2);
    let t = (tile / 2).max(1);
    (0..ch.div_ceil(t))
        .into_par_iter()
        .flat_map_iter(|ty| {
            let mut acc = vec![TileAcc::default(); cw.div_ceil(t)];
            for cy in ty * t..((ty + 1) * t).min(ch) {
                let (i0, i1) = (2 * cy * w, (2 * cy + 1) * w);
                let block = |p: &[f32], cx: usize| {
                    [
                        p[i0 + 2 * cx],
                        p[i0 + 2 * cx + 1],
                        p[i1 + 2 * cx],
                        p[i1 + 2 * cx + 1],
                    ]
                };
                for cx in 0..cw {
                    let power = srcs
                        .iter()
                        .fold([0.0; 3], |a, p| add3(a, haar_powers(block(p, cx))));
                    let power = if directional { power } else { [power[0]; 3] };
                    add_haar_block(&mut acc[cx / t], block(lum, cx), offset, power);
                }
            }
            acc.into_iter().filter_map(TileAcc::finish)
        })
        .collect()
}

/// Tile side (pixels) of the noise estimate.
const NOISE_TILE: usize = 16;
/// Luminance bands of the noise estimate.
const NOISE_BANDS: usize = 8;
/// Fewest tiles that give a band its own estimate.
const MIN_BAND_TILES: usize = 4;
/// Fewest samples that make a tile count.
const MIN_TILE_SAMPLES: u32 = 4;
/// Tiles reaching outside this encoded range are skipped: clipping hides noise.
const CLIP_LO: f32 = 0.002;
const CLIP_HI: f32 = 0.99;

#[derive(Debug, Clone, Copy)]
struct TileStat {
    /// Mean encoded luminance.
    lum: f32,
    /// Mean noise power.
    power: f32,
}

/// Tile accumulator; `power` holds up to three noise signals (identical when
/// there is one), of which the least is taken.
#[derive(Debug, Clone, Copy)]
struct TileAcc {
    power: [f32; 3],
    lum: f32,
    lo: f32,
    hi: f32,
    n: u32,
}

impl Default for TileAcc {
    fn default() -> Self {
        TileAcc {
            power: [0.0; 3],
            lum: 0.0,
            lo: f32::INFINITY,
            hi: f32::NEG_INFINITY,
            n: 0,
        }
    }
}

impl TileAcc {
    #[inline]
    fn add(&mut self, power: [f32; 3], lum: f32, lo: f32, hi: f32) {
        self.power = add3(self.power, power);
        self.lum += lum;
        self.lo = self.lo.min(lo);
        self.hi = self.hi.max(hi);
        self.n += 1;
    }

    fn finish(self) -> Option<TileStat> {
        let n = self.n as f32;
        let stat = TileStat {
            lum: self.lum / n,
            power: self.power[0].min(self.power[1]).min(self.power[2]) / n,
        };
        (self.n >= MIN_TILE_SAMPLES
            && self.lo >= CLIP_LO
            && self.hi <= CLIP_HI
            && stat.power.is_finite())
        .then_some(stat)
    }
}

/// Noise sigma (encoded) per band of encoded luminance.
#[derive(Debug, Clone, Copy, PartialEq)]
struct NoiseProfile {
    sigma: [f32; NOISE_BANDS],
}

impl NoiseProfile {
    /// Per band, the square root of the `percentile` of its tile powers. Bands
    /// with too few tiles are interpolated from the nearest populated ones
    /// (constant beyond them); with no populated band all tiles are pooled, and
    /// with no tile at all the profile is zero (the floors then apply).
    fn from_tiles(tiles: &[TileStat], percentile: f64) -> NoiseProfile {
        let q = slider(percentile, DEFAULT_STRENGTHS.noise_percentile, 0.0, 1.0);
        let pick = |v: &mut [f32]| -> f32 {
            let i = ((v.len() - 1) as f64 * q).round() as usize;
            let (_, p, _) = v.select_nth_unstable_by(i, f32::total_cmp);
            p.max(0.0).sqrt()
        };
        let mut bands: Vec<Vec<f32>> = vec![Vec::new(); NOISE_BANDS];
        for t in tiles {
            let b = ((t.lum * NOISE_BANDS as f32) as usize).min(NOISE_BANDS - 1);
            bands[b].push(t.power);
        }
        let known: Vec<(usize, f32)> = bands
            .iter_mut()
            .enumerate()
            .filter(|(_, v)| v.len() >= MIN_BAND_TILES)
            .map(|(i, v)| (i, pick(v)))
            .collect();
        if known.is_empty() {
            let mut all: Vec<f32> = tiles.iter().map(|t| t.power).collect();
            let sigma = if all.is_empty() { 0.0 } else { pick(&mut all) };
            return NoiseProfile {
                sigma: [sigma; NOISE_BANDS],
            };
        }
        let sigma = std::array::from_fn(|i| {
            let left = known.iter().rev().find(|(j, _)| *j <= i);
            let right = known.iter().find(|(j, _)| *j >= i);
            match (left, right) {
                (Some(&(a, va)), Some(&(b, vb))) if b > a => {
                    va + (vb - va) * (i - a) as f32 / (b - a) as f32
                }
                (Some(&(_, v)), _) | (None, Some(&(_, v))) => v,
                (None, None) => 0.0,
            }
        });
        NoiseProfile { sigma }
    }

    /// Sigma at encoded luminance `l` (linear between band centres).
    fn at(&self, l: f32) -> f32 {
        let pos = (unit(l) * NOISE_BANDS as f32 - 0.5).clamp(0.0, (NOISE_BANDS - 1) as f32);
        let i = (pos as usize).min(NOISE_BANDS - 2);
        let f = pos - i as f32;
        self.sigma[i] + (self.sigma[i + 1] - self.sigma[i]) * f
    }
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
    r: f32,
    eps: f32,
    max_slope: f32,
) -> Vec<(Vec<f32>, Vec<f32>)> {
    let mean_i = box_blur_frac(guide, w, h, r);
    let ii: Vec<f32> = guide.par_iter().map(|v| v * v).collect();
    let var_i: Vec<f32> = box_blur_frac(&ii, w, h, r)
        .par_iter()
        .zip(&mean_i)
        .map(|(c, m)| (c - m * m).max(0.0))
        .collect();
    srcs.iter()
        .map(|src| {
            let mean_p = box_blur_frac(src, w, h, r);
            let ip: Vec<f32> = guide.par_iter().zip(src).map(|(a, b)| a * b).collect();
            let corr_ip = box_blur_frac(&ip, w, h, r);
            let (a, b): (Vec<f32>, Vec<f32>) = (0..w * h)
                .into_par_iter()
                .map(|i| {
                    let cov = corr_ip[i] - mean_i[i] * mean_p[i];
                    let a = (cov / (var_i[i] + eps)).clamp(-max_slope, max_slope);
                    (a, mean_p[i] - a * mean_i[i])
                })
                .unzip();
            (box_blur_frac(&a, w, h, r), box_blur_frac(&b, w, h, r))
        })
        .collect()
}

/// Separable box blur of radius `r` (pixels, fractional): the window holds
/// the `2⌊r⌋ + 1` nearest samples at full weight and the next one on each side
/// at weight `r − ⌊r⌋`, so the result changes continuously with `r`. Edges
/// clamped, running sums (cost independent of `r`); the vertical pass runs on
/// bands of rows, so no transposes are needed.
fn box_blur_frac(plane: &[f32], w: usize, h: usize, r: f32) -> Vec<f32> {
    debug_assert_eq!(plane.len(), w * h);
    // Radii beyond the larger side (runaway calibration values only) are capped there.
    let r = if r.is_nan() {
        0.0
    } else {
        r.min(w.max(h) as f32)
    };
    if r <= 0.0 || plane.is_empty() {
        return plane.to_vec();
    }
    let ri = r.floor() as usize;
    let f = r - ri as f32;
    let norm = ((2 * ri + 1) as f32 + 2.0 * f).recip();
    let mut horiz = vec![0.0f32; plane.len()];
    horiz
        .par_chunks_mut(w)
        .zip(plane.par_chunks(w))
        .for_each(|(dst, src)| {
            let n = src.len() as isize;
            let at = |i: isize| src[i.clamp(0, n - 1) as usize];
            let (ri, rn) = (ri as isize, ri as isize + 1);
            let mut sum: f32 = (-ri..=ri).map(at).sum();
            for (i, d) in dst.iter_mut().enumerate() {
                let i = i as isize;
                *d = (sum + f * (at(i - rn) + at(i + rn))) * norm;
                sum += at(i + rn) - at(i - ri);
            }
        });
    let band = (4 * ri).max(64);
    let mut out = vec![0.0f32; plane.len()];
    out.par_chunks_mut(band * w)
        .enumerate()
        .for_each(|(b, dst)| {
            let row = |y: isize| {
                let y = y.clamp(0, h as isize - 1) as usize;
                &horiz[y * w..(y + 1) * w]
            };
            let (ri, rn) = (ri as isize, ri as isize + 1);
            let y0 = (b * band) as isize;
            let mut sum = vec![0.0f32; w];
            for j in -ri..=ri {
                sum.iter_mut().zip(row(y0 + j)).for_each(|(s, &v)| *s += v);
            }
            for (dy, d) in dst.chunks_exact_mut(w).enumerate() {
                let y = y0 + dy as isize;
                let (above, below) = (row(y - rn), row(y + rn));
                for (((d, &s), &a), &b) in d.iter_mut().zip(&sum).zip(above).zip(below) {
                    *d = (s + f * (a + b)) * norm;
                }
                let (enter, leave) = (row(y + rn), row(y - ri));
                for ((s, &e), &l) in sum.iter_mut().zip(enter).zip(leave) {
                    *s += e - l;
                }
            }
        });
    out
}

/// Box-average downscale by an integer factor (partial blocks at the right and
/// bottom edges average what they cover). Non-finite pixels are left out and
/// channels clamped to ±[`MAX_LINEAR`], so one bad pixel cannot poison the
/// guided filter's running sums.
fn downscale(img: &Image, f: usize) -> (Vec<f32>, usize, usize) {
    let (w, h) = (img.width, img.height);
    let (sw, sh) = (w.div_ceil(f), h.div_ceil(f));
    let mut out = vec![0.0f32; sw * sh * 3];
    out.par_chunks_mut(sw * 3).enumerate().for_each(|(j, row)| {
        let (y0, y1) = (j * f, ((j + 1) * f).min(h));
        let mut count = vec![0u32; sw];
        for y in y0..y1 {
            let src = &img.rgb[y * w * 3..(y + 1) * w * 3];
            for (i, (acc, n)) in row.chunks_exact_mut(3).zip(&mut count).enumerate() {
                let (x0, x1) = (i * f, ((i + 1) * f).min(w));
                for p in src[x0 * 3..x1 * 3].chunks_exact(3) {
                    if p.iter().all(|v| v.is_finite()) {
                        for (a, &v) in acc.iter_mut().zip(p) {
                            *a += v.clamp(-MAX_LINEAR, MAX_LINEAR);
                        }
                        *n += 1;
                    }
                }
            }
        }
        for (acc, &n) in row.chunks_exact_mut(3).zip(&count) {
            if n > 0 {
                acc.iter_mut().for_each(|v| *v /= n as f32);
            }
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

/// Encoded luminance plane minus `offset`.
fn encoded_luminance(img: &Image, offset: f32) -> Vec<f32> {
    let t = transfer();
    let lw = luminance_weights();
    img.rgb
        .par_chunks_exact(3)
        .map(|p| t.encode(bounded(dot(lw, p))) - offset)
        .collect()
}

/// Linear luminance made safe to filter: clamped to ±[`MAX_LINEAR`], NaN to 0.
#[inline]
fn bounded(y: f32) -> f32 {
    if y.is_nan() {
        0.0
    } else {
        y.clamp(-MAX_LINEAR, MAX_LINEAR)
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

/// Linear interpolation in a table sampling 0..1 at `M - 1` steps (fixed size,
/// so the compiler drops the bounds checks). `v` must be in 0..1.
#[inline]
fn interp<const M: usize>(t: &[f32; M], v: f32) -> f32 {
    let steps = M - 1;
    let pos = v * steps as f32;
    let i = (pos as usize).min(steps - 1);
    t[i] + (t[i + 1] - t[i]) * (pos - i as f32)
}

/// `x` clamped to 0..1, NaN to 0.
#[inline]
fn unit(x: f32) -> f32 {
    if x.is_nan() { 0.0 } else { x.clamp(0.0, 1.0) }
}

const LUT_STEPS: usize = 256;

/// A function of encoded luminance (0..1, clamped; NaN reads as 0) sampled
/// for per-pixel use.
struct Lut(Box<[f32; LUT_STEPS + 1]>);

impl Lut {
    fn from_fn(f: impl Fn(f32) -> f32) -> Lut {
        let v: Vec<f32> = (0..=LUT_STEPS)
            .map(|i| f(i as f32 / LUT_STEPS as f32))
            .collect();
        Lut(v.into_boxed_slice().try_into().expect("LUT length"))
    }

    #[inline]
    fn at(&self, x: f32) -> f32 {
        interp(&self.0, unit(x))
    }

    fn max(&self) -> f32 {
        self.0.iter().copied().fold(f32::NEG_INFINITY, f32::max)
    }
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

const ENC_STEPS: usize = 1 << 14;
const DEC_STEPS: usize = 1 << 12;

/// The sRGB transfer as interpolated tables (exact formula outside 0..1, odd
/// extension below 0). The encode table is fine enough (2^14 steps) that its
/// error near black, where the curve bends most, stays around 1e-6.
struct Transfer {
    enc: Box<[f32; ENC_STEPS + 1]>,
    dec: Box<[f32; DEC_STEPS + 1]>,
}

fn transfer() -> &'static Transfer {
    static TRANSFER: OnceLock<Transfer> = OnceLock::new();
    TRANSFER.get_or_init(|| {
        let enc: Vec<f32> = (0..=ENC_STEPS)
            .map(|i| srgb_encode(i as f32 / ENC_STEPS as f32))
            .collect();
        let dec: Vec<f32> = (0..=DEC_STEPS)
            .map(|i| srgb_decode(i as f32 / DEC_STEPS as f32))
            .collect();
        Transfer {
            enc: enc.into_boxed_slice().try_into().expect("table length"),
            dec: dec.into_boxed_slice().try_into().expect("table length"),
        }
    })
}

impl Transfer {
    #[inline]
    fn encode(&self, v: f32) -> f32 {
        if (0.0..=1.0).contains(&v) {
            interp(&self.enc, v)
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
            interp(&self.dec, v)
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
                0.18 * (1.0 + 0.08 * ((x as f32 * 1.7).sin() * (y as f32 * 1.3).cos()))
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

    #[test]
    fn streaming_blur_matches_the_shared_gaussian() {
        let (w, h) = (53, 31);
        let plane: Vec<f32> = (0..w * h).map(|i| 0.5 + 0.2 * gauss(i, 5)).collect();
        for sigma in [0.2, 0.5, 1.0, 1.7, 3.0] {
            let want = gaussian_blur(&plane, w, h, sigma);
            let kernel = gaussian_kernel(sigma);
            let r = kernel.len() / 2;
            let (mut tmp, mut pad, mut out) = (vec![0.0; w], vec![0.0; w + 2 * r], vec![0.0; w]);
            for y in 0..h {
                convolve_row(&plane, w, h, y, &kernel, &mut tmp, &mut pad, &mut out);
                for x in 0..w {
                    let d = (out[x] - want[y * w + x]).abs();
                    assert!(d < 1e-5, "sigma {sigma} ({x},{y}): {d}");
                }
            }
        }
        // Narrow images (narrower than the kernel) clamp the same way.
        let (w, h) = (2, 9);
        let plane: Vec<f32> = (0..w * h).map(|i| i as f32).collect();
        let want = gaussian_blur(&plane, w, h, 2.0);
        let kernel = gaussian_kernel(2.0);
        let r = kernel.len() / 2;
        let (mut tmp, mut pad, mut out) = (vec![0.0; w], vec![0.0; w + 2 * r], vec![0.0; w]);
        for y in 0..h {
            convolve_row(&plane, w, h, y, &kernel, &mut tmp, &mut pad, &mut out);
            for x in 0..w {
                assert!((out[x] - want[y * w + x]).abs() < 1e-4);
            }
        }
    }

    #[test]
    fn noise_estimate_tracks_noise_not_edges_or_gradients() {
        // Left half: encoded luminance noise of known sigma at two levels (dark
        // band noisier), at the centres of bands 1 and 4; right half: clean
        // step edges and a gradient (bands 2-3 and 6-7), which must not pull
        // the estimate up.
        let (w, h) = (256, 192);
        let (dark, light) = (1.5 / 8.0, 4.5 / 8.0);
        let truth = |l: f32| if l < 0.3 { 0.02 } else { 0.008 };
        let l: Vec<f32> = (0..w * h)
            .map(|i| {
                let (x, y) = (i % w, i / w);
                if x < w / 2 {
                    let base = if y < h / 2 { dark } else { light };
                    base + truth(base) * gauss(i, 9)
                } else {
                    let step = if ((x + 1) / 24) % 2 == 0 { 0.3 } else { 0.76 };
                    step + 0.001 * y as f32
                }
            })
            .map(|v| v - CENTRE)
            .collect();
        let tiles = luminance_tiles(&l, w, h);
        let noise = NoiseProfile::from_tiles(&tiles, DEFAULT_STRENGTHS.noise_percentile);
        for (lum, want) in [(dark, 0.02), (light, 0.008)] {
            let got = noise.at(lum);
            assert!(
                got > 0.8 * want && got < 1.1 * want,
                "at {lum}: {got} vs {want}"
            );
        }
        // Clean frames read zero; clipped tiles are skipped; empty input is zero.
        let flat = vec![0.1f32; w * h];
        let zero = NoiseProfile::from_tiles(&luminance_tiles(&flat, w, h), 0.15);
        assert!(zero.sigma.iter().all(|&s| s == 0.0));
        let clipped = vec![1.0 - CENTRE; w * h];
        assert!(luminance_tiles(&clipped, w, h).is_empty());
        assert_eq!(
            NoiseProfile::from_tiles(&[], 0.15).sigma,
            [0.0; NOISE_BANDS]
        );
    }

    #[test]
    fn noise_profile_fills_empty_bands() {
        let tiles: Vec<TileStat> = (0..20)
            .flat_map(|_| {
                [
                    TileStat {
                        lum: 0.2,
                        power: 0.01f32.powi(2),
                    },
                    TileStat {
                        lum: 0.7,
                        power: 0.03f32.powi(2),
                    },
                ]
            })
            .collect();
        let p = NoiseProfile::from_tiles(&tiles, 0.5);
        // Band 1 holds 0.2, band 5 holds 0.7; between them linear, outside constant.
        assert!((p.sigma[0] - 0.01).abs() < 1e-6);
        assert!((p.sigma[1] - 0.01).abs() < 1e-6);
        assert!((p.sigma[3] - 0.02).abs() < 1e-6);
        assert!((p.sigma[7] - 0.03).abs() < 1e-6);
        assert!((p.at(0.0) - 0.01).abs() < 1e-6 && (p.at(1.0) - 0.03).abs() < 1e-6);
        // Too few tiles for any band: all pooled.
        let few = [TileStat {
            lum: 0.5,
            power: 4e-4,
        }];
        assert!((NoiseProfile::from_tiles(&few, 0.15).at(0.9) - 0.02).abs() < 1e-6);
    }

    /// Colour texture (isoluminant green/tan stripes, 3 px wide) on the left,
    /// flat grey on the right, with per-channel encoded noise of `sigma` everywhere.
    fn colour_texture(w: usize, h: usize, amplitude: f32, sigma: f32) -> Image {
        let lw = luminance_weights();
        Image::from_fn(w, h, |x, y| {
            let i = y * w + x;
            let tint = if x < w / 2 {
                if (x / 3) % 2 == 0 { 1.0 } else { -1.0 }
            } else {
                0.0
            };
            // ±amplitude on R−B (encoded), luminance restored through G.
            let e = [0.5 + tint * amplitude, 0.5, 0.5 - tint * amplitude];
            let mut p = e.map(srgb_decode);
            let target = srgb_decode(0.5);
            p[1] += (target - dot(lw, &p)) / lw[1];
            let n = [gauss(i, 31), gauss(i, 32), gauss(i, 33)];
            [0, 1, 2].map(|c| srgb_decode(srgb_encode(p[c]) + sigma * n[c]))
        })
    }

    #[test]
    fn colour_nr_adapts_to_the_noise_level() {
        let (w, h) = (128, 96);
        let texture = (8..w / 2 - 8, 8..h - 8);
        let flat = (w / 2 + 8..w - 8, 8..h - 8);
        // Clean frame: the default keeps most of the colour texture.
        let clean = colour_texture(w, h, 0.03, 0.002);
        let out = denoise(&clean, NoiseReduction::default());
        let (before, after) = (
            chroma_noise(&clean, texture.0.clone(), texture.1.clone()),
            chroma_noise(&out, texture.0.clone(), texture.1.clone()),
        );
        assert!(after > 0.7 * before, "clean texture {before} → {after}");
        // Noisy frame (noise as strong as the texture): the noise goes.
        let noisy = colour_texture(w, h, 0.03, 0.03);
        let out = denoise(&noisy, NoiseReduction::default());
        let (before, after) = (
            chroma_noise(&noisy, flat.0.clone(), flat.1.clone()),
            chroma_noise(&out, flat.0, flat.1),
        );
        assert!(after < before / 3.0, "noise {before} → {after}");
        // Detail 0 smooths the clean texture away regardless.
        let bled = denoise(
            &clean,
            NoiseReduction {
                color_detail: 0.0,
                ..NoiseReduction::default()
            },
        );
        let left = chroma_noise(&bled, texture.0, texture.1);
        assert!(left < 0.5 * before, "Detail 0 keeps {left} of {before}");
    }

    #[test]
    fn colour_nr_removes_isolated_speckles_but_keeps_thin_lines() {
        let (w, h) = (96, 64);
        let magenta = |p: [f32; 3]| [p[0] * 1.6, p[1] * 0.75, p[2] * 1.6];
        let img = Image::from_fn(w, h, |x, y| {
            let i = y * w + x;
            let n = [gauss(i, 41), gauss(i, 42), gauss(i, 43)];
            let p = [0, 1, 2].map(|c| srgb_decode(0.45 + 0.004 * n[c]));
            let speckle = (x, y) == (20, 20) || (x, y) == (30, 40);
            let line = (60..62).contains(&x);
            if speckle || line { magenta(p) } else { p }
        });
        let out = denoise(&img, NoiseReduction::default());
        let chroma = |im: &Image, x: usize, y: usize| {
            let p = im.get(x, y);
            let le = enc(lum(p));
            (0..3).map(|c| (enc(p[c]) - le).powi(2)).sum::<f32>().sqrt()
        };
        for (x, y) in [(20, 20), (30, 40)] {
            let (a, b) = (chroma(&img, x, y), chroma(&out, x, y));
            assert!(b < 0.3 * a, "speckle ({x},{y}) {a} → {b}");
        }
        for y in 10..h - 10 {
            let (a, b) = (chroma(&img, 60, y), chroma(&out, 60, y));
            assert!(b > 0.7 * a, "line row {y}: {a} → {b}");
        }
    }

    #[test]
    fn luminance_nr_adapts_to_the_noise_level() {
        let (w, h) = (128, 96);
        // Fine texture (encoded ±0.02) on the left, flat on the right.
        let make = |sigma: f32| {
            Image::from_fn(w, h, |x, y| {
                let i = y * w + x;
                let t = if x < w / 2 {
                    0.02 * ((x as f32 * 1.1).sin() * (y as f32 * 0.8).sin())
                } else {
                    0.0
                };
                grey(srgb_decode(0.5 + t + sigma * gauss(i, 51)))
            })
        };
        let std_in = |im: &Image, xs: std::ops::Range<usize>| {
            let v: Vec<f32> = (8..h - 8)
                .flat_map(|y| xs.clone().map(move |x| (x, y)))
                .map(|(x, y)| enc(lum(im.get(x, y))))
                .collect();
            std_dev(&v)
        };
        let p = NoiseReduction {
            luminance: 30.0,
            ..nr_off()
        };
        // Clean: the texture survives almost intact.
        let clean = make(0.001);
        let out = denoise(&clean, p);
        let (a, b) = (std_in(&clean, 8..w / 2 - 8), std_in(&out, 8..w / 2 - 8));
        assert!(b > 0.85 * a, "clean texture {a} → {b}");
        // Noisy: the same slider removes most of the noise in the flat half.
        let noisy = make(0.02);
        let out = denoise(&noisy, p);
        let (a, b) = (
            std_in(&noisy, w / 2 + 8..w - 8),
            std_in(&out, w / 2 + 8..w - 8),
        );
        assert!(b < 0.5 * a, "noise {a} → {b}");
    }

    #[test]
    fn fractional_box_blur_matches_integer_box_and_brute_force() {
        let (w, h) = (37, 29);
        let plane: Vec<f32> = (0..w * h).map(|i| 0.5 + 0.2 * gauss(i, 81)).collect();
        for r in [1usize, 2, 5, 30] {
            let (want, got) = (
                box_blur(&plane, w, h, r),
                box_blur_frac(&plane, w, h, r as f32),
            );
            assert!(
                want.iter().zip(&got).all(|(a, b)| (a - b).abs() < 1e-5),
                "r {r}"
            );
        }
        for r in [0.3f32, 1.4, 2.75] {
            let got = box_blur_frac(&plane, w, h, r);
            let ri = r.floor() as isize;
            let weight = |d: isize| match d.abs() {
                d if d <= ri => 1.0,
                d if d == ri + 1 => r - ri as f32,
                _ => 0.0,
            };
            let at = |x: isize, y: isize| {
                plane[y.clamp(0, h as isize - 1) as usize * w + x.clamp(0, w as isize - 1) as usize]
            };
            for y in 0..h as isize {
                for x in 0..w as isize {
                    let (mut acc, mut norm) = (0.0, 0.0);
                    for dy in -ri - 1..=ri + 1 {
                        for dx in -ri - 1..=ri + 1 {
                            let wt = weight(dx) * weight(dy);
                            acc += wt * at(x + dx, y + dy);
                            norm += wt;
                        }
                    }
                    let d = (got[y as usize * w + x as usize] - acc / norm).abs();
                    assert!(d < 1e-5, "r {r} ({x},{y}): {d}");
                }
            }
        }
    }

    type Tint<'a> = dyn Fn(usize, usize) -> f32 + Sync + 'a;

    /// Isoluminant colour texture `tint(x, y)` (in units of `amp`) over the
    /// whole frame (no flat region for the noise estimate to fall back on),
    /// with per-channel encoded noise.
    fn full_frame_colour_texture(
        w: usize,
        h: usize,
        tint: &Tint<'_>,
        amp: f32,
        sigma: f32,
    ) -> Image {
        let lw = luminance_weights();
        Image::from_fn(w, h, |x, y| {
            let i = y * w + x;
            let tint = tint(x, y);
            let e = [0.5 + tint * amp, 0.5, 0.5 - tint * amp];
            let mut p = e.map(srgb_decode);
            p[1] += (srgb_decode(0.5) - dot(lw, &p)) / lw[1];
            let n = [gauss(i, 62), gauss(i, 63), gauss(i, 64)];
            [0, 1, 2].map(|c| srgb_decode(srgb_encode(p[c]) + sigma * n[c]))
        })
    }

    #[test]
    fn colour_nr_keeps_clean_colour_texture_that_fills_the_frame() {
        let (w, h) = (128, 96);
        let (xs, ys) = (8..w - 8, 8..h - 8);
        let stripe = |v: usize| if (v / 3) % 2 == 0 { 1.0 } else { -1.0 };
        let patterns: [(&str, &Tint<'_>); 5] = [
            ("vertical", &|x, _| stripe(x)),
            ("horizontal", &|_, y| stripe(y)),
            ("diagonal", &|x, y| stripe(x + y)),
            ("anti-diagonal", &|x, y| stripe(x + 2 * h - y)),
            ("plaid", &|x, y| 0.5 * (stripe(x) + stripe(y / 2 + 1))),
        ];
        for (name, tint) in patterns {
            for sigma in [0.0, 0.002] {
                let img = full_frame_colour_texture(w, h, tint, 0.03, sigma);
                let out = denoise(&img, NoiseReduction::default());
                let (before, after) = (
                    chroma_noise(&img, xs.clone(), ys.clone()),
                    chroma_noise(&out, xs.clone(), ys.clone()),
                );
                assert!(
                    after > 0.7 * before,
                    "{name}, sigma {sigma}: {before} → {after}"
                );
            }
        }
        // Real colour noise over the whole frame still goes.
        let noisy = full_frame_colour_texture(w, h, &|_, _| 0.0, 0.0, 0.03);
        let out = denoise(&noisy, NoiseReduction::default());
        let (before, after) = (
            chroma_noise(&noisy, xs.clone(), ys.clone()),
            chroma_noise(&out, xs, ys),
        );
        assert!(after < before / 3.0, "noise {before} → {after}");
    }

    /// Largest per-pixel change of encoded luminance or chroma between two images.
    fn max_change(a: &Image, b: &Image) -> f32 {
        a.rgb
            .chunks(3)
            .zip(b.rgb.chunks(3))
            .map(|(p, q)| {
                let (lp, lq) = (enc(lum([p[0], p[1], p[2]])), enc(lum([q[0], q[1], q[2]])));
                (0..3)
                    .map(|c| ((enc(p[c]) - lp) - (enc(q[c]) - lq)).abs())
                    .fold((lp - lq).abs(), f32::max)
            })
            .fold(0.0, f32::max)
    }

    /// Steps of 0.1 across `at` change the output by no more than a small
    /// multiple of the steps on either side (no jump at `at`).
    fn assert_continuous(what: &str, at: f64, f: &dyn Fn(f64) -> Image) {
        let v = [at - 0.15, at - 0.05, at + 0.05, at + 0.15].map(f);
        let d = [0, 1, 2].map(|i| max_change(&v[i], &v[i + 1]));
        assert!(
            d[1] <= 3.0 * d[0].max(d[2]) + 2e-4,
            "{what} jumps at {at}: steps {d:?}"
        );
    }

    #[test]
    fn outputs_change_continuously_with_the_sliders() {
        let chroma = chroma_noise_and_colour_edge(160, 64, 0.03);
        for at in [6.25, 18.75, 31.25] {
            assert_continuous("Color", at, &|v| {
                denoise(
                    &chroma,
                    NoiseReduction {
                        color: v,
                        ..NoiseReduction::default()
                    },
                )
            });
        }
        assert_continuous("Smoothness", 62.5, &|v| {
            denoise(
                &chroma,
                NoiseReduction {
                    color_smoothness: v,
                    ..NoiseReduction::default()
                },
            )
        });
        let luma = noisy_patch_and_step(96, 64, 0.03);
        for at in [16.67, 50.0, 83.33] {
            assert_continuous("Luminance", at, &|v| {
                denoise(
                    &luma,
                    NoiseReduction {
                        luminance: v,
                        ..nr_off()
                    },
                )
            });
        }
        let step = blurred_step(64, 8, 0.05, 0.4, 1.0);
        for at in [1.5, 2.5] {
            // Radius steps of 0.01 (the slider's range is 0.5..3).
            assert_continuous("Radius", 100.0 * at, &|v| {
                sharpen(
                    &step,
                    Sharpening {
                        amount: 150.0,
                        radius: v / 100.0,
                        detail: 0.0,
                        masking: 0.0,
                    },
                )
            });
        }
    }

    #[test]
    fn a_non_finite_pixel_does_not_spread() {
        let (w, h) = (128, 128);
        let img = Image::from_fn(w, h, |x, y| {
            let i = y * w + x;
            let n = [gauss(i, 71), gauss(i, 72), gauss(i, 73)];
            [0, 1, 2].map(|c| srgb_decode(0.45 + 0.03 * n[c]))
        });
        let clean = denoise(&img, NoiseReduction::default());
        let (xs, ys) = (64..120, 64..120);
        let want = chroma_noise(&clean, xs.clone(), ys.clone());
        assert!(want < 0.2 * chroma_noise(&img, xs.clone(), ys.clone()));
        for bad in [f32::INFINITY, f32::NAN, f32::NEG_INFINITY] {
            let mut dirty = img.clone();
            dirty.set(10, 10, [bad; 3]);
            dirty.set(110, 20, [0.2, bad, 0.2]);
            let out = denoise(&dirty, NoiseReduction::default());
            let got = chroma_noise(&out, xs.clone(), ys.clone());
            assert!(got < 1.5 * want, "{bad}: chroma {want} → {got}");
            // Pixels away from the bad ones are NR'd as usual.
            for y in 30..70 {
                for x in 30..70 {
                    let (a, b) = (out.get(x, y), clean.get(x, y));
                    assert!(
                        (0..3).all(|c| (a[c] - b[c]).abs() < 0.02),
                        "{bad} ({x},{y}): {a:?} vs {b:?}"
                    );
                }
            }
            // Sharpening: neighbours of a bad pixel are not blacked out, and
            // pixels outside the kernel are exactly as without it.
            let flat = Image::from_fn(32, 32, |_, _| grey(0.2));
            let mut dirty = flat.clone();
            dirty.set(16, 16, [bad; 3]);
            let out = sharpen(&dirty, Sharpening::default());
            let want = sharpen(&flat, Sharpening::default());
            for y in 0..32 {
                for x in 0..32 {
                    if (x, y) == (16, 16) {
                        continue;
                    }
                    let p = out.get(x, y);
                    assert!(
                        p.iter().all(|v| (v - 0.2).abs() < 0.05),
                        "{bad} ({x},{y}): {p:?}"
                    );
                    if x.abs_diff(16) > 4 || y.abs_diff(16) > 4 {
                        assert_eq!(p, want.get(x, y), "{bad} ({x},{y})");
                    }
                }
            }
            // Luminance NR likewise.
            let out = denoise(
                &dirty,
                NoiseReduction {
                    luminance: 50.0,
                    luminance_contrast: 50.0,
                    ..NoiseReduction::default()
                },
            );
            for (i, p) in out.rgb.chunks(3).enumerate() {
                if i != 16 * 32 + 16 {
                    assert!(p.iter().all(|v| (v - 0.2).abs() < 0.05), "{bad} {i}: {p:?}");
                }
            }
        }
    }

    #[test]
    fn huge_calibration_constants_neither_panic_nor_hang() {
        let img = textured(8, 8);
        let strong_nr = NoiseReduction {
            luminance: 100.0,
            luminance_contrast: 100.0,
            color: 100.0,
            color_smoothness: 100.0,
            ..NoiseReduction::default()
        };
        let strong_sharp = Sharpening {
            amount: 150.0,
            radius: 3.0,
            detail: 50.0,
            masking: 50.0,
        };
        for json in [
            r#"{"sharpen_sigma_per_radius": 1e300}"#,
            r#"{"halo_window_per_radius": 1e300}"#,
            r#"{"mask_blur_sigma": 1e300, "mask_smooth_sigma": 1e300}"#,
            r#"{"color_radius": 1e13}"#,
            r#"{"color_radius_per_strength": 1e300, "color_radius_per_smoothness": 1e300}"#,
            r#"{"color_max_downscale": 1e300, "color_low_res_radius": 1e-300}"#,
            r#"{"luminance_radius": 1e13, "luminance_coarse_radius": 1e300}"#,
            r#"{"luminance_contrast_sigma": 1e300}"#,
        ] {
            let k: DetailStrengths = serde_json::from_str(json).unwrap();
            let mut out = img.clone();
            apply_noise_reduction(&mut out, &strong_nr, &k);
            apply_sharpening(&mut out, &strong_sharp, &k);
            assert_finite(&out, json);
        }
    }

    /// Timing on a 24 MP frame (run with `--release -- --ignored --nocapture`):
    /// the first run of each case (cold allocations, as in a one-shot render)
    /// and the best of three.
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
            let runs: Vec<f64> = (0..3)
                .map(|_| {
                    let mut im = img.clone();
                    let start = std::time::Instant::now();
                    f(&mut im);
                    start.elapsed().as_secs_f64()
                })
                .collect();
            let best = runs.iter().copied().fold(f64::INFINITY, f64::min);
            eprintln!("{label}: first {:.3} s, best {best:.3} s", runs[0]);
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
