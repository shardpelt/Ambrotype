// SPDX-License-Identifier: GPL-3.0-or-later
//! Dehaze: remove (positive) or add (negative) atmospheric haze.
//!
//! Interface contract (pipeline.rs depends on it):
//! - runs on the scene-linear ProPhoto image, after Exposure and before the base look;
//! - `amount` is the Camera Raw slider value in -100..100; no-op at 0;
//! - values may exceed 1.0 (scene-linear); output stays finite and non-negative.
//!
//! **Model v1 (ours, not Adobe's).** Lightroom's Dehaze is closed source; what
//! is known (docs/research/05 §6) is that positive values remove haze, raising
//! contrast and saturation where the image is hazy and darkening a little, and
//! negative values lay a fog-like veil over it. This version is He, Sun & Tang's
//! dark channel prior (CVPR 2009 / PAMI 2011) applied to the scene-linear image,
//! where the haze model `I = J·t + A·(1 − t)` (scene radiance `J`, transmission
//! `t`, airlight `A`) is physically meaningful:
//!
//! 1. **Estimate at low resolution.** A box-downscaled copy (long side about
//!    [`DehazeStrengths::estimate_long_side`]) carries all the statistics.
//! 2. **Airlight.** Dark channel = per-pixel minimum over RGB, then minimum over
//!    a square patch whose radius is a fraction of the short side (so the
//!    result does not depend on resolution). `A` is the mean colour of the
//!    brightest [`DehazeStrengths::airlight_fraction`] dark-channel pixels,
//!    partly neutralised (a strongly coloured `A` would shift every hue) and
//!    floored so black frames stay finite. The search runs on a copy whose
//!    channels are capped at a high quantile
//!    ([`DehazeStrengths::airlight_cap_quantile`]): scene-linear data is
//!    unbounded, and a small sun, lamp or sunlit wall must not pass for haze.
//! 3. **Haze map.** `d = patch-min(min_c(I_c / A_c))` in 0..1, refined with a
//!    guided filter steered by encoded luminance. Its linear coefficients are
//!    upsampled bilinearly and applied to the full-resolution guide (He & Sun's
//!    fast guided filter), so the map follows full-resolution edges and leaves
//!    no low-resolution halos.
//! 4. **Positive amounts** recover `J = A' + (I − A') / t` with
//!    `t = max(1 − ω·amount·d, t0)` and `A' = A·(1 + headroom)`
//!    ([`DehazeStrengths::airlight_headroom`]), so the haziest areas (sky, at
//!    `A`) darken and deepen rather than keep their level. Only the part of
//!    each channel below `A'` is stretched; anything brighter passes unchanged,
//!    so highlights are never multiplied by `1 / t`. Instead of clipping
//!    negatives, `t` is raised per pixel just enough for the darkest channel to
//!    keep a fraction ([`DehazeStrengths::shadow_keep`]) of its value: one `t`
//!    for all three channels, so hues stay consistent where shadows get crushed.
//! 5. **Negative amounts** blend towards a fog colour derived from `A`:
//!    `J = I·t' + A_fog·(1 − t')`, `t' = 1 − |amount|·fog·(uniform + depth·d)`,
//!    a mostly uniform, nearly neutral veil that is a little denser where the
//!    image is already hazy (far away).
//! 6. A mild exposure compensation undoes part of the change in mean lightness.
//!    When removing haze it is weighted per pixel by `1 − d`, so it lifts the
//!    clearer areas without undoing the darkening of the haziest ones.
//!
//! Non-finite input values are treated as 0 so one bad pixel cannot poison the
//! frame-wide statistics.
//!
//! The model is scale-equivariant (doubling the input doubles the output) as
//! long as the airlight is above its floor, so Exposure and Dehaze commute.
//! Every constant lives in [`DehazeStrengths`] for the calibration harness to
//! fit against Lightroom sweeps (docs/lightroom-trial-plan.md).

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::buffer::{Image, Tile, luminance_weights};
use crate::filters::box_blur;

/// Tunable constants. Fitted later against Lightroom sweeps; these are starting values.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DehazeStrengths {
    /// Long side, in pixels, of the downscaled copy the haze is estimated on.
    pub estimate_long_side: f64,
    /// Dark-channel patch radius as a fraction of the image's short side.
    pub patch_radius: f64,
    /// Fraction of the brightest dark-channel pixels averaged into the airlight.
    pub airlight_fraction: f64,
    /// Per-channel quantile (0..1) of the estimate that caps every pixel before
    /// the airlight search, so a bright object smaller than `1 − quantile` of
    /// the frame (sun, lamp, sunlit wall, specular) cannot pass for haze.
    pub airlight_cap_quantile: f64,
    /// Share of the measured airlight colour kept (0 = neutral grey of the same
    /// luminance, 1 = as measured).
    pub airlight_saturation: f64,
    /// Floor on the airlight luminance (scene-linear), so black frames stay finite.
    pub airlight_min: f64,
    /// He's ω at +100: fraction of the estimated haze removed.
    pub omega: f64,
    /// Haze removal treats the airlight as this much brighter than measured
    /// (0.1 = 10 %). The haziest areas (sky), which sit at the measured airlight,
    /// then darken and deepen with positive amounts instead of keeping their level.
    pub airlight_headroom: f64,
    /// Guided-filter radius for the haze map, as a fraction of the short side.
    pub refine_radius: f64,
    /// Guided-filter regularisation (guide = sqrt(luminance / airlight luminance)).
    pub refine_eps: f64,
    /// Lower bound on the transmission: caps the contrast gain in very hazy
    /// areas (sky) at `1 / min_transmission`.
    pub min_transmission: f64,
    /// Fraction of a pixel's darkest channel that haze removal always keeps
    /// (soft floor instead of clipping crushed shadows to zero).
    pub shadow_keep: f64,
    /// Veil opacity at -100 where the fog is densest.
    pub fog: f64,
    /// Share of the fog that follows the haze map (denser far away); the rest is uniform.
    pub fog_depth: f64,
    /// Fog colour luminance as a multiple of the airlight luminance.
    pub fog_level: f64,
    /// Share of the measured airlight colour the fog keeps (0 = neutral grey; at
    /// most `airlight_saturation`). Lower than that by default: without real
    /// haze the airlight is often a sunlit (warm) surface, and a tinted veil
    /// reads as a colour cast.
    pub fog_saturation: f64,
    /// Fraction of the change in mean lightness that is undone (0 = none, 1 = all).
    /// Removing haze lifts each pixel by the factor raised to `1 − d` (haze map
    /// `d`), so the haziest areas keep their darkening; adding fog applies it uniformly.
    pub exposure_compensation: f64,
}

impl Default for DehazeStrengths {
    fn default() -> Self {
        DEFAULT_STRENGTHS
    }
}

pub const DEFAULT_STRENGTHS: DehazeStrengths = DehazeStrengths {
    estimate_long_side: 1536.0,
    patch_radius: 0.01,
    airlight_fraction: 0.001,
    airlight_cap_quantile: 0.99,
    airlight_saturation: 0.5,
    airlight_min: 0.01,
    omega: 0.9,
    airlight_headroom: 0.1,
    refine_radius: 0.04,
    refine_eps: 1e-3,
    min_transmission: 0.3,
    shadow_keep: 0.15,
    fog: 0.5,
    fog_depth: 0.3,
    fog_level: 0.9,
    fog_saturation: 0.25,
    exposure_compensation: 0.3,
};

/// Bounds on the exposure compensation factor, whatever the strengths say.
const MAX_COMPENSATION: f32 = 2.0;

pub fn apply(img: &mut Image, amount: f64, k: &DehazeStrengths) {
    if amount == 0.0 || !amount.is_finite() || img.width == 0 || img.height == 0 {
        return;
    }
    sanitise(img);
    let est = Estimate::measure(img, k, (img.width, img.height));
    apply_with(img, amount, &est, k, &Tile::of(img));
}

/// Where a dehaze pass gets its haze estimate.
#[derive(Debug, Clone, Copy)]
pub enum Haze<'a> {
    /// Measure it on the image: a whole output (the full-size one, or a
    /// small render of it whose full size is `full`).
    Measure { full: (usize, usize) },
    /// Use one measured on the whole output before (for a region of it).
    Given(&'a Estimate),
}

/// Dehaze `img`, which holds `tile` of the output, with the estimate `haze`
/// says. A measured estimate is returned, laid out for the full output.
pub fn apply_haze(
    img: &mut Image,
    amount: f64,
    k: &DehazeStrengths,
    tile: &Tile,
    haze: Haze,
) -> Option<Estimate> {
    if amount == 0.0 || !amount.is_finite() || img.width == 0 || img.height == 0 {
        return None;
    }
    match haze {
        Haze::Given(est) => {
            apply_with(img, amount, est, k, tile);
            None
        }
        Haze::Measure { full } => {
            sanitise(img);
            let own = (img.width, img.height);
            let here = Estimate::measure(img, k, own);
            let kept = if full == own {
                here.clone()
            } else {
                Estimate::measure(img, k, full)
            };
            // Measured on this buffer, so laid out on it.
            apply_with(img, amount, &here, k, &Tile::of(img));
            Some(kept)
        }
    }
}

/// Garbage pixels (NaN, ±inf) would poison the frame-wide statistics: treat
/// them as black.
fn sanitise(img: &mut Image) {
    img.rgb.par_iter_mut().for_each(|v| {
        if !v.is_finite() {
            *v = 0.0;
        }
    });
}

/// Dehaze `img`, a part (`tile`) of an output whose haze was measured as
/// `est` (over the whole output).
pub fn apply_with(img: &mut Image, amount: f64, est: &Estimate, k: &DehazeStrengths, tile: &Tile) {
    if amount == 0.0 || !amount.is_finite() || img.width == 0 || img.height == 0 {
        return;
    }
    let amount = (amount / 100.0).clamp(-1.0, 1.0) as f32;
    sanitise(img);
    let op = Recovery::new(amount, est, k);
    let comp = est.compensation(&op, k);
    let log_comp = comp.ln();
    let removing = op.omega > 0.0;
    let (w, sw, sh) = (img.width, est.sw, est.sh);
    let lum = luminance_weights();

    // Bilinear sampling positions of the low-resolution coefficient grid
    // (pixel centres aligned), computed once per column.
    let columns: Vec<(usize, usize, f32)> = (0..w)
        .map(|x| lerp_position(tile.x + x, est.factor, sw))
        .collect();
    img.rgb
        .par_chunks_mut(w * 3)
        .enumerate()
        .for_each(|(y, row)| {
            let (y0, y1, wy) = lerp_position(tile.y + y, est.factor, sh);
            let (a0, a1) = (&est.coef_a[y0 * sw..][..sw], &est.coef_a[y1 * sw..][..sw]);
            let (b0, b1) = (&est.coef_b[y0 * sw..][..sw], &est.coef_b[y1 * sw..][..sw]);
            for (px, &(x0, x1, wx)) in row.chunks_exact_mut(3).zip(&columns) {
                let bilerp = |p: &[f32], q: &[f32]| {
                    let top = p[x0] + (p[x1] - p[x0]) * wx;
                    let bottom = q[x0] + (q[x1] - q[x0]) * wx;
                    top + (bottom - top) * wy
                };
                let p = [px[0], px[1], px[2]];
                let g = guide_value(p, lum, est.inv_airlight_y);
                let d = (bilerp(a0, a1) * g + bilerp(b0, b1)).clamp(0.0, 1.0);
                let out = op.apply(p, d);
                let gain = if removing {
                    (log_comp * (1.0 - d)).exp()
                } else {
                    comp
                };
                for (o, v) in px.iter_mut().zip(out) {
                    *o = (v * gain).max(0.0);
                }
            }
        });
}

/// Low-resolution statistics: airlight and the haze map's guided-filter
/// coefficients, over a whole output.
#[derive(Debug, Clone)]
pub struct Estimate {
    /// Integer downscale factor and the downscaled size.
    factor: usize,
    sw: usize,
    sh: usize,
    /// Downscaled RGB (interleaved) and its guide plane, kept for the exposure compensation.
    small: Vec<f32>,
    guide: Vec<f32>,
    airlight: [f32; 3],
    inv_airlight_y: f32,
    /// Fast-guided-filter coefficients: haze map d ≈ coef_a · guide + coef_b.
    coef_a: Vec<f32>,
    coef_b: Vec<f32>,
}

impl Estimate {
    /// Measure the haze of an output of size `full` on `img`: the output
    /// itself, or a smaller render of all of it (resampled to the grid the
    /// full-size output would be measured on).
    pub fn measure(img: &Image, k: &DehazeStrengths, full: (usize, usize)) -> Estimate {
        let factor = estimate_factor(full, k);
        let (small, sw, sh) = if (img.width, img.height) == full {
            downscale(img, factor)
        } else {
            let (sw, sh) = (full.0.div_ceil(factor), full.1.div_ceil(factor));
            (resample(img, sw, sh), sw, sh)
        };
        Estimate::from_small(small, sw, sh, factor, k)
    }

    /// The estimate from the `sw × sh` copy (interleaved RGB, non-finite
    /// values already 0) of an output downscaled by `factor` (block
    /// averages, as `downscale` makes them).
    pub fn from_small(
        small: Vec<f32>,
        sw: usize,
        sh: usize,
        factor: usize,
        k: &DehazeStrengths,
    ) -> Estimate {
        let short = sw.min(sh) as f64;
        let patch = ((k.patch_radius * short).round() as usize).max(1);
        let refine = ((k.refine_radius * short).round() as usize).max(1);
        let lum = luminance_weights();

        // Airlight: mean colour of the brightest dark-channel pixels, searched
        // on a copy whose channels are capped at a high quantile.
        let capped = cap_channels(&small, k.airlight_cap_quantile);
        let dark: Vec<f32> = capped
            .par_chunks_exact(3)
            .map(|p| p[0].min(p[1]).min(p[2]))
            .collect();
        let dark = min_filter(&dark, sw, sh, patch);
        let airlight = airlight(&capped, &dark, k);
        let airlight_y = dot(lum, airlight);
        let inv_airlight_y = 1.0 / airlight_y;
        let inv_airlight = airlight.map(|v| 1.0 / v);

        // Haze map: patch minimum of the airlight-normalised minimum channel.
        let normalised: Vec<f32> = small
            .par_chunks_exact(3)
            .map(|p| {
                (p[0] * inv_airlight[0])
                    .min(p[1] * inv_airlight[1])
                    .min(p[2] * inv_airlight[2])
                    .clamp(0.0, 1.0)
            })
            .collect();
        let haze = min_filter(&normalised, sw, sh, patch);
        let guide: Vec<f32> = small
            .par_chunks_exact(3)
            .map(|p| guide_value([p[0], p[1], p[2]], lum, inv_airlight_y))
            .collect();
        let (coef_a, coef_b) =
            guided_coefficients(&guide, &haze, sw, sh, refine, k.refine_eps as f32);
        Estimate {
            factor,
            sw,
            sh,
            small,
            guide,
            airlight,
            inv_airlight_y,
            coef_a,
            coef_b,
        }
    }

    /// Exposure factor that undoes `exposure_compensation` of the change in mean
    /// lightness (cube root of luminance), measured on the downscaled copy.
    fn compensation(&self, op: &Recovery, k: &DehazeStrengths) -> f32 {
        let strength = k.exposure_compensation as f32;
        if strength == 0.0 {
            return 1.0;
        }
        let lum = luminance_weights();
        let n = self.sw * self.sh;
        let lightness = |p: [f32; 3]| (dot(lum, p).max(0.0) * self.inv_airlight_y).cbrt() as f64;
        let pixel = |i: usize| {
            [
                self.small[3 * i],
                self.small[3 * i + 1],
                self.small[3 * i + 2],
            ]
        };
        let before = deterministic_mean(n, |i| lightness(pixel(i)));
        let after = deterministic_mean(n, |i| {
            let d = (self.coef_a[i] * self.guide[i] + self.coef_b[i]).clamp(0.0, 1.0);
            lightness(op.apply(pixel(i), d).map(|v| v.max(0.0)))
        });
        if !(before > 1e-9 && after > 1e-9) {
            return 1.0;
        }
        let factor = ((before / after) as f32).powf(3.0 * strength);
        if factor.is_finite() {
            factor.clamp(1.0 / MAX_COMPENSATION, MAX_COMPENSATION)
        } else {
            1.0
        }
    }
}

/// The downscale factor the estimate of an output of size `full` is measured at.
pub fn estimate_factor(full: (usize, usize), k: &DehazeStrengths) -> usize {
    let long = full.0.max(full.1);
    let target = k.estimate_long_side.max(16.0) as usize;
    long.div_ceil(target).max(1)
}

/// The per-pixel numbers of [`apply_with`] once the haze is measured (for
/// other backends): the recovery or fog operator, the exposure
/// compensation, and the haze map's coefficients on the `sw × sh` grid.
#[derive(Debug, Clone)]
pub struct ApplyNumbers {
    pub omega: f32,
    pub min_transmission: f32,
    pub shadow_keep: f32,
    pub airlight: [f32; 3],
    pub inv_airlight: [f32; 3],
    pub fog: f32,
    pub fog_depth: f32,
    pub fog_colour: [f32; 3],
    /// Exposure compensation; per pixel `comp^(1 − d)` when removing haze.
    pub comp: f32,
    pub inv_airlight_y: f32,
    pub factor: usize,
    pub sw: usize,
    pub sh: usize,
    pub coef_a: Vec<f32>,
    pub coef_b: Vec<f32>,
}

/// [`ApplyNumbers`] for slider `amount` (−100..100; not 0) and estimate `est`.
pub fn apply_numbers(amount: f64, est: &Estimate, k: &DehazeStrengths) -> ApplyNumbers {
    let amount = (amount / 100.0).clamp(-1.0, 1.0) as f32;
    let op = Recovery::new(amount, est, k);
    let comp = est.compensation(&op, k);
    ApplyNumbers {
        omega: op.omega,
        min_transmission: op.min_transmission,
        shadow_keep: op.shadow_keep,
        airlight: op.airlight,
        inv_airlight: op.inv_airlight,
        fog: op.fog,
        fog_depth: op.fog_depth,
        fog_colour: op.fog_colour,
        comp,
        inv_airlight_y: est.inv_airlight_y,
        factor: est.factor,
        sw: est.sw,
        sh: est.sh,
        coef_a: est.coef_a.clone(),
        coef_b: est.coef_b.clone(),
    }
}

/// The per-pixel haze removal / fog operator for one slider value.
struct Recovery {
    /// Positive: ω·amount (haze removed). Zero when adding fog.
    omega: f32,
    min_transmission: f32,
    shadow_keep: f32,
    /// Airlight for haze removal (the measured one plus `airlight_headroom`).
    airlight: [f32; 3],
    inv_airlight: [f32; 3],
    /// Negative: fog·|amount| (veil opacity). Zero when removing haze.
    fog: f32,
    fog_depth: f32,
    fog_colour: [f32; 3],
}

impl Recovery {
    fn new(amount: f32, est: &Estimate, k: &DehazeStrengths) -> Recovery {
        let removing = amount > 0.0;
        let headroom = 1.0 + (k.airlight_headroom as f32).clamp(0.0, 1.0);
        let removal_airlight = est.airlight.map(|v| v * headroom);
        Recovery {
            omega: if removing {
                (k.omega as f32 * amount).clamp(0.0, 1.0)
            } else {
                0.0
            },
            min_transmission: (k.min_transmission as f32).clamp(0.01, 1.0),
            shadow_keep: (k.shadow_keep as f32).clamp(0.0, 0.99),
            airlight: removal_airlight,
            inv_airlight: removal_airlight.map(|v| 1.0 / v),
            fog: if removing {
                0.0
            } else {
                (k.fog as f32 * -amount).clamp(0.0, 1.0)
            },
            fog_depth: (k.fog_depth as f32).clamp(0.0, 1.0),
            fog_colour: fog_colour(est.airlight, k),
        }
    }

    /// Pixel `p` with haze map value `d` (0..1) → dehazed or fogged pixel (before
    /// exposure compensation and the final non-negativity clamp).
    fn apply(&self, p: [f32; 3], d: f32) -> [f32; 3] {
        if self.fog > 0.0 {
            let veil = (self.fog * (1.0 - self.fog_depth + self.fog_depth * d)).min(1.0);
            return [0, 1, 2].map(|c| p[c] + (self.fog_colour[c] - p[c]) * veil);
        }
        let mut t = (1.0 - self.omega * d).max(self.min_transmission);
        // Soft shadow floor: J_c >= keep·I_c for every channel needs
        // t >= (1 − m) / (1 − keep·m) with m the pixel's smallest I_c / A_c.
        let m = (p[0] * self.inv_airlight[0])
            .min(p[1] * self.inv_airlight[1])
            .min(p[2] * self.inv_airlight[2]);
        if m < 1.0 {
            t = t.max((1.0 - m) / (1.0 - self.shadow_keep * m));
        }
        let inv_t = 1.0 / t.min(1.0);
        // Only the part of each channel below the airlight is stretched; what
        // lies above it (highlights brighter than the haze) passes unchanged.
        [0, 1, 2].map(|c| {
            let a = self.airlight[c];
            let below = p[c].min(a) - a;
            a + below * inv_t + (p[c] - a).max(0.0)
        })
    }
}

/// Fog colour: the airlight, scaled by `fog_level` and pulled towards the grey
/// of equal luminance. The airlight already keeps `airlight_saturation` of the
/// measured colour; the fog keeps `fog_saturation` of it (capped at the airlight's share).
fn fog_colour(airlight: [f32; 3], k: &DehazeStrengths) -> [f32; 3] {
    let y = dot(luminance_weights(), airlight);
    let kept = (k.fog_saturation / k.airlight_saturation.max(1e-6)).clamp(0.0, 1.0) as f32;
    let level = k.fog_level.max(0.0) as f32;
    airlight.map(|v| (y + (v - y) * kept) * level)
}

fn dot(w: [f32; 3], p: [f32; 3]) -> f32 {
    w[0] * p[0] + w[1] * p[1] + w[2] * p[2]
}

/// Guide for the haze map: luminance relative to the airlight, square-root
/// encoded so shadow edges count, and scale-invariant (Exposure does not change it).
fn guide_value(p: [f32; 3], lum: [f32; 3], inv_airlight_y: f32) -> f32 {
    (dot(lum, p).max(0.0) * inv_airlight_y).sqrt()
}

/// Airlight: mean colour of the pixels whose dark channel is in the top
/// `airlight_fraction`, partly neutralised and floored.
fn airlight(small: &[f32], dark: &[f32], k: &DehazeStrengths) -> [f32; 3] {
    let n = dark.len();
    let top = ((k.airlight_fraction.clamp(0.0, 1.0) * n as f64).ceil() as usize).clamp(1, n);
    let mut sorted = dark.to_vec();
    let (_, &mut threshold, _) = sorted.select_nth_unstable_by(n - top, |a, b| a.total_cmp(b));
    let mut sum = [0.0f64; 3];
    let mut count = 0usize;
    for (p, &dc) in small.chunks_exact(3).zip(dark) {
        if dc >= threshold {
            for c in 0..3 {
                sum[c] += p[c].max(0.0) as f64;
            }
            count += 1;
        }
    }
    let mean = sum.map(|s| (s / count.max(1) as f64) as f32);
    let mean = if mean.iter().all(|v| v.is_finite()) {
        mean
    } else {
        [0.0; 3]
    };
    let y = dot(luminance_weights(), mean);
    let y_floor = (k.airlight_min as f32).max(1e-6);
    let sat = (k.airlight_saturation as f32).clamp(0.0, 1.0);
    if y <= y_floor {
        // Too dark to trust the colour: neutral airlight at the floor.
        return [y_floor; 3];
    }
    // Neutralise towards the grey of equal luminance (ProPhoto Y of (1,1,1) is 1),
    // keeping every channel well away from zero so I / A stays bounded.
    mean.map(|v| (y + (v - y) * sat).max(0.1 * y))
}

/// Copy of the interleaved RGB `small` with every channel clipped at its own
/// `quantile` over the frame (scale-equivariant). A bright object covering
/// less than `1 − quantile` of the frame then looks like the frame's typical
/// bright colour (in a landscape, the sky) instead of standing out.
fn cap_channels(small: &[f32], quantile: f64) -> Vec<f32> {
    let n = small.len() / 3;
    if n == 0 || quantile >= 1.0 {
        return small.to_vec();
    }
    let rank = ((quantile.max(0.0) * (n - 1) as f64).round() as usize).min(n - 1);
    let cap: [f32; 3] = [0, 1, 2].map(|c| {
        let mut v: Vec<f32> = small.iter().skip(c).step_by(3).copied().collect();
        let (_, &mut q, _) = v.select_nth_unstable_by(rank, |a, b| a.total_cmp(b));
        q
    });
    let mut out = small.to_vec();
    out.par_chunks_exact_mut(3).for_each(|p| {
        for c in 0..3 {
            p[c] = p[c].min(cap[c]);
        }
    });
    out
}

/// Box-average downscale by an integer factor (partial blocks at the right and
/// bottom edges average what they cover).
fn downscale(img: &Image, f: usize) -> (Vec<f32>, usize, usize) {
    let (w, h) = (img.width, img.height);
    if f == 1 {
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

/// `img` area-averaged (or, where it is smaller, bilinearly sampled) to
/// `sw × sh`, pixel areas aligned.
fn resample(img: &Image, sw: usize, sh: usize) -> Vec<f32> {
    let (w, h) = (img.width, img.height);
    let (fx, fy) = (w as f64 / sw as f64, h as f64 / sh as f64);
    // Per output cell: the source span [a, b) along one axis, with fractional
    // ends, or a bilinear position when the cell is under a pixel wide.
    let spans = |n: usize, f: f64, len: usize| -> Vec<Vec<(usize, f32)>> {
        (0..n)
            .map(|i| {
                if f < 1.0 {
                    let (lo, hi, t) = lerp_position_f(i, f, len);
                    return vec![(lo, 1.0 - t), (hi, t)];
                }
                let (a, b) = (i as f64 * f, ((i + 1) as f64 * f).min(len as f64));
                let mut taps = Vec::new();
                let mut x = a.floor() as usize;
                while (x as f64) < b && x < len {
                    let cover = (b.min(x as f64 + 1.0) - a.max(x as f64)).max(0.0);
                    if cover > 0.0 {
                        taps.push((x, (cover / (b - a)) as f32));
                    }
                    x += 1;
                }
                taps
            })
            .collect()
    };
    let (xs, ys) = (spans(sw, fx, w), spans(sh, fy, h));
    let mut out = vec![0.0f32; sw * sh * 3];
    out.par_chunks_mut(sw * 3).enumerate().for_each(|(j, row)| {
        for &(y, wy) in &ys[j] {
            let src = &img.rgb[y * w * 3..(y + 1) * w * 3];
            for (acc, taps) in row.chunks_exact_mut(3).zip(&xs) {
                for &(x, wx) in taps {
                    let p = &src[x * 3..x * 3 + 3];
                    for c in 0..3 {
                        acc[c] += wy * wx * p[c];
                    }
                }
            }
        }
    });
    out
}

/// [`lerp_position`] for a fractional step `f` < 1 (upsampling).
fn lerp_position_f(i: usize, f: f64, n: usize) -> (usize, usize, f32) {
    let s = ((i as f64 + 0.5) * f - 0.5).clamp(0.0, (n - 1) as f64);
    let lo = (s.floor() as usize).min(n - 1);
    (lo, (lo + 1).min(n - 1), (s - lo as f64) as f32)
}

/// Bilinear source position of full-resolution index `i` in a grid downscaled
/// by `f` with `n` cells (cell centres aligned): (lower, upper, weight of upper).
fn lerp_position(i: usize, f: usize, n: usize) -> (usize, usize, f32) {
    let s = ((i as f32 + 0.5) / f as f32 - 0.5).clamp(0.0, (n - 1) as f32);
    let lo = (s.floor() as usize).min(n - 1);
    (lo, (lo + 1).min(n - 1), s - lo as f32)
}

/// Transpose a `w × h` plane into `h × w`.
fn transpose(src: &[f32], w: usize, h: usize) -> Vec<f32> {
    let mut out = vec![0.0f32; src.len()];
    out.par_chunks_mut(h).enumerate().for_each(|(x, col)| {
        for (y, v) in col.iter_mut().enumerate() {
            *v = src[y * w + x];
        }
    });
    out
}

/// Square minimum filter of radius `r` (window (2r+1)²), clamped edges.
fn min_filter(plane: &[f32], w: usize, h: usize, r: usize) -> Vec<f32> {
    let rows = min_filter_rows(plane, w, r);
    let cols = min_filter_rows(&transpose(&rows, w, h), h, r);
    transpose(&cols, h, w)
}

fn min_filter_rows(src: &[f32], w: usize, r: usize) -> Vec<f32> {
    let mut out = vec![0.0f32; src.len()];
    // A window wider than the row sees the whole row either way.
    let r = r.min(w.saturating_sub(1));
    out.par_chunks_mut(w).zip(src.par_chunks(w)).for_each_init(
        || (Vec::new(), Vec::new()),
        |(fwd, bwd), (d, s)| min_row(s, d, r, fwd, bwd),
    );
    out
}

/// Running minimum over a window of 2r+1 (van Herk / Gil-Werman): three
/// comparisons per sample whatever the radius. Out-of-range samples are +inf,
/// which equals clamping the edges for a minimum.
fn min_row(src: &[f32], dst: &mut [f32], r: usize, fwd: &mut Vec<f32>, bwd: &mut Vec<f32>) {
    let n = src.len();
    if r == 0 {
        dst.copy_from_slice(src);
        return;
    }
    let k = 2 * r + 1;
    let len = n + 2 * r;
    let at = |i: usize| {
        if (r..r + n).contains(&i) {
            src[i - r]
        } else {
            f32::INFINITY
        }
    };
    fwd.clear();
    fwd.resize(len, 0.0);
    bwd.clear();
    bwd.resize(len, 0.0);
    // fwd: minimum from the start of i's block of k samples; bwd: to its end.
    for i in 0..len {
        fwd[i] = if i % k == 0 {
            at(i)
        } else {
            fwd[i - 1].min(at(i))
        };
    }
    for i in (0..len).rev() {
        bwd[i] = if i % k == k - 1 || i == len - 1 {
            at(i)
        } else {
            bwd[i + 1].min(at(i))
        };
    }
    for (j, d) in dst.iter_mut().enumerate() {
        *d = bwd[j].min(fwd[j + 2 * r]);
    }
}

/// Guided filter (He, Sun & Tang 2010) returning the box-averaged linear
/// coefficients instead of the filtered plane, so they can be upsampled and
/// applied to a full-resolution guide (He & Sun 2015, "fast guided filter").
fn guided_coefficients(
    guide: &[f32],
    src: &[f32],
    w: usize,
    h: usize,
    r: usize,
    eps: f32,
) -> (Vec<f32>, Vec<f32>) {
    let mean = |p: &[f32]| box_blur(p, w, h, r);
    let mean_i = mean(guide);
    let mean_p = mean(src);
    let ii: Vec<f32> = guide.par_iter().map(|v| v * v).collect();
    let ip: Vec<f32> = guide
        .par_iter()
        .zip(src.par_iter())
        .map(|(a, b)| a * b)
        .collect();
    let corr_i = mean(&ii);
    let corr_ip = mean(&ip);
    let eps = eps.max(1e-6);
    let (a, b): (Vec<f32>, Vec<f32>) = (0..w * h)
        .into_par_iter()
        .map(|k| {
            let var_i = (corr_i[k] - mean_i[k] * mean_i[k]).max(0.0);
            let cov_ip = corr_ip[k] - mean_i[k] * mean_p[k];
            let a = cov_ip / (var_i + eps);
            (a, mean_p[k] - a * mean_i[k])
        })
        .unzip();
    (mean(&a), mean(&b))
}

/// Mean of `f(0..n)` summed in fixed chunks, so the result does not depend on
/// how rayon schedules the work.
fn deterministic_mean(n: usize, f: impl Fn(usize) -> f64 + Sync) -> f64 {
    const CHUNK: usize = 4096;
    let partial: Vec<f64> = (0..n.div_ceil(CHUNK))
        .into_par_iter()
        .map(|c| (c * CHUNK..((c + 1) * CHUNK).min(n)).map(&f).sum())
        .collect();
    partial.iter().sum::<f64>() / n.max(1) as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    const AIR: [f32; 3] = [0.85, 0.9, 0.95];

    /// Haze-free scene radiance: saturated colours whose darkest channel is near
    /// zero in every small patch (the dark channel prior holds), with texture.
    fn scene(x: usize, y: usize) -> [f32; 3] {
        const PALETTE: [[f32; 3]; 5] = [
            [0.55, 0.05, 0.008],
            [0.01, 0.35, 0.06],
            [0.006, 0.08, 0.45],
            [0.40, 0.30, 0.005],
            [0.012, 0.012, 0.012],
        ];
        let c = PALETTE[(x / 2 + 3 * (y / 2)) % 5];
        let shade = 0.7 + 0.3 * ((x as f32 * 0.21).sin() * (y as f32 * 0.17).cos()).abs();
        c.map(|v| v * shade)
    }

    /// Transmission of the synthetic hazy image: a pure-haze "sky" band on top,
    /// then haze thinning from left (t = 0.35) to right (t = 0.9).
    fn transmission(x: usize, y: usize, w: usize, h: usize) -> f32 {
        if y < h / 6 {
            0.0
        } else {
            0.35 + 0.55 * x as f32 / (w - 1) as f32
        }
    }

    fn hazy(w: usize, h: usize) -> Image {
        Image::from_fn(w, h, |x, y| {
            let t = transmission(x, y, w, h);
            let j = scene(x, y);
            [0, 1, 2].map(|c| j[c] * t + AIR[c] * (1.0 - t))
        })
    }

    /// Pixels below the sky band (where a ground truth radiance exists).
    fn ground(img: &Image) -> Vec<[f32; 3]> {
        (img.height / 6 + 2..img.height)
            .flat_map(|y| (0..img.width).map(move |x| (x, y)))
            .map(|(x, y)| img.get(x, y))
            .collect()
    }

    fn mean(px: &[[f32; 3]]) -> [f32; 3] {
        let mut s = [0.0f64; 3];
        for p in px {
            for c in 0..3 {
                s[c] += p[c] as f64;
            }
        }
        s.map(|v| (v / px.len() as f64) as f32)
    }

    /// Standard deviation over all channels (a contrast measure).
    fn spread(px: &[[f32; 3]]) -> f32 {
        let m = mean(px);
        let var: f64 = px
            .iter()
            .map(|p| (0..3).map(|c| ((p[c] - m[c]) as f64).powi(2)).sum::<f64>())
            .sum::<f64>()
            / (3 * px.len()) as f64;
        var.sqrt() as f32
    }

    /// Mean absolute difference between horizontal neighbours below the sky
    /// band (local contrast, insensitive to the left-to-right haze gradient).
    fn local_contrast(img: &Image) -> f32 {
        let mut s = 0.0f64;
        let mut n = 0;
        for y in img.height / 6 + 2..img.height {
            for x in 0..img.width - 1 {
                s += distance(img.get(x, y), img.get(x + 1, y)) as f64;
                n += 1;
            }
        }
        (s / n as f64) as f32
    }

    fn rms_error(a: &[[f32; 3]], b: &[[f32; 3]]) -> f32 {
        let s: f64 = a
            .iter()
            .zip(b)
            .map(|(p, q)| (0..3).map(|c| ((p[c] - q[c]) as f64).powi(2)).sum::<f64>())
            .sum();
        (s / (3 * a.len()) as f64).sqrt() as f32
    }

    fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
        (0..3).map(|c| (a[c] - b[c]).powi(2)).sum::<f32>().sqrt()
    }

    fn dehazed(img: &Image, amount: f64) -> Image {
        let mut out = img.clone();
        apply(&mut out, amount, &DehazeStrengths::default());
        out
    }

    fn assert_valid(img: &Image, what: &str) {
        assert!(
            img.rgb.iter().all(|v| v.is_finite() && *v >= 0.0),
            "{what}: non-finite or negative value"
        );
    }

    #[test]
    fn zero_amount_is_an_exact_no_op() {
        let img = hazy(97, 61);
        let mut out = img.clone();
        apply(&mut out, 0.0, &DehazeStrengths::default());
        assert_eq!(out.rgb, img.rgb);
        apply(&mut out, f64::NAN, &DehazeStrengths::default());
        assert_eq!(out.rgb, img.rgb);
    }

    #[test]
    fn positive_amount_recovers_the_hazy_scene() {
        let (w, h) = (180, 120);
        let img = hazy(w, h);
        let truth = ground(&Image::from_fn(w, h, scene));
        let out = dehazed(&img, 100.0);
        let (before, after) = (ground(&img), ground(&out));
        let (err_before, err_after) = (rms_error(&before, &truth), rms_error(&after, &truth));
        assert!(
            err_after < 0.5 * err_before,
            "error to J: {err_before} → {err_after}"
        );
        let (c_before, c_after) = (local_contrast(&img), local_contrast(&out));
        let c_truth = local_contrast(&Image::from_fn(w, h, scene));
        assert!(
            c_after > 1.5 * c_before && c_after < 1.3 * c_truth,
            "contrast: {c_before} → {c_after} (haze-free {c_truth})"
        );
        // Haze removal darkens (the airlight veil is taken away).
        let lum = luminance_weights();
        assert!(dot(lum, mean(&after)) < dot(lum, mean(&before)));
    }

    #[test]
    fn removal_is_stronger_where_the_haze_is_thicker() {
        let (w, h) = (180, 120);
        let img = hazy(w, h);
        let out = dehazed(&img, 100.0);
        // Mean absolute change in a thick-haze column band vs a thin-haze one.
        let change = |x0: usize, x1: usize| {
            let mut s = 0.0;
            let mut n = 0;
            for y in h / 6 + 2..h {
                for x in x0..x1 {
                    let (a, b) = (img.get(x, y), out.get(x, y));
                    s += distance(a, b);
                    n += 1;
                }
            }
            s / n as f32
        };
        let thick = change(5, 35);
        let thin = change(w - 35, w - 5);
        assert!(thick > 2.0 * thin, "thick {thick} thin {thin}");
    }

    #[test]
    fn effect_grows_with_the_amount() {
        let img = hazy(120, 90);
        let base = local_contrast(&img);
        let c50 = local_contrast(&dehazed(&img, 50.0));
        let c100 = local_contrast(&dehazed(&img, 100.0));
        assert!(base < c50 && c50 < c100, "{base} {c50} {c100}");
        // And a tiny amount changes the image only a little (no jump at 0).
        let tiny = dehazed(&img, 0.5);
        let max_change = img
            .rgb
            .iter()
            .zip(&tiny.rgb)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0f32, f32::max);
        assert!(max_change < 0.02, "max change at +0.5: {max_change}");
    }

    #[test]
    fn negative_amount_adds_haze() {
        let (w, h) = (150, 100);
        // A clear scene with a bright region so the airlight is a light grey.
        let img = Image::from_fn(w, h, |x, y| {
            if x < 20 && y < 20 {
                [0.9, 0.9, 0.9]
            } else {
                scene(x, y)
            }
        });
        let all =
            |i: &Image| -> Vec<[f32; 3]> { i.rgb.chunks(3).map(|p| [p[0], p[1], p[2]]).collect() };
        let before = all(&img);
        let fog50 = all(&dehazed(&img, -50.0));
        let fog100 = all(&dehazed(&img, -100.0));
        let grey = [0.9f32; 3];
        let (d0, d50, d100) = (
            distance(mean(&before), grey),
            distance(mean(&fog50), grey),
            distance(mean(&fog100), grey),
        );
        assert!(
            d100 < d50 && d50 < d0,
            "mean → fog colour: {d0} {d50} {d100}"
        );
        let (c0, c50, c100) = (spread(&before), spread(&fog50), spread(&fog100));
        assert!(c100 < c50 && c50 < c0, "contrast: {c0} {c50} {c100}");
        assert!(c100 < 0.7 * c0, "contrast at -100: {c0} → {c100}");
    }

    #[test]
    fn haze_free_regions_barely_change() {
        let (w, h) = (150, 100);
        // Every patch holds a near-black channel: no haze to remove, except the
        // white block, which defines the airlight.
        let img = Image::from_fn(w, h, |x, y| {
            if x < 20 && y < 20 {
                [0.9, 0.9, 0.9]
            } else {
                scene(x, y)
            }
        });
        let out = dehazed(&img, 100.0);
        let mut worst = 0.0f32;
        let mut total = 0.0f32;
        let mut n = 0;
        for y in 0..h {
            for x in 0..w {
                if x < 40 && y < 40 {
                    continue;
                }
                let (a, b) = (img.get(x, y), out.get(x, y));
                let rel = distance(a, b) / (distance(a, [0.0; 3]) + 0.02);
                worst = worst.max(rel);
                total += rel;
                n += 1;
            }
        }
        let avg = total / n as f32;
        assert!(avg < 0.12, "mean relative change {avg} (worst {worst})");
    }

    #[test]
    fn neutral_input_stays_neutral() {
        let img = Image::from_fn(80, 60, |x, y| {
            let v = 0.05 + 0.8 * ((x * 7 + y * 3) % 23) as f32 / 23.0;
            let v = if y < 10 { 0.9 } else { v * 0.5 };
            [v, v, v]
        });
        for amount in [100.0, -100.0] {
            let out = dehazed(&img, amount);
            for p in out.rgb.chunks(3) {
                let spread = p[0].max(p[1]).max(p[2]) - p[0].min(p[1]).min(p[2]);
                assert!(spread < 1e-4 * (1.0 + p[1]), "{amount}: {p:?}");
            }
        }
    }

    #[test]
    fn scale_equivariant_in_scene_linear() {
        // Exposure then Dehaze equals Dehaze then Exposure (values above 1 included).
        let img = hazy(90, 60);
        let mut bright = img.clone();
        bright.rgb.iter_mut().for_each(|v| *v *= 4.0);
        for amount in [70.0, -70.0] {
            let a = dehazed(&img, amount);
            let b = dehazed(&bright, amount);
            for (x, y) in a.rgb.iter().zip(&b.rgb) {
                assert!(
                    (x * 4.0 - y).abs() <= 1e-3 * (1.0 + y),
                    "{amount}: {x} vs {y}"
                );
            }
        }
    }

    #[test]
    fn resolution_independent() {
        // The same continuous scene at two resolutions gives the same result.
        let field = |u: f32, v: f32| -> [f32; 3] {
            let t = if v < 0.15 { 0.05 } else { 0.3 + 0.6 * u };
            let j = [
                0.5 * (6.0 * u).sin().abs(),
                0.3 * (5.0 * v).cos().abs(),
                0.02 + 0.4 * u * v,
            ];
            [0, 1, 2].map(|c| j[c] * t + AIR[c] * (1.0 - t))
        };
        let render = |w: usize, h: usize| {
            let img = Image::from_fn(w, h, |x, y| {
                field((x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32)
            });
            dehazed(&img, 80.0)
        };
        let small = render(150, 100);
        let large = render(300, 200);
        let mut diff = 0.0f32;
        for y in 0..100 {
            for x in 0..150 {
                // 2×2 average of the large render vs the small render.
                let mut avg = [0.0f32; 3];
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let p = large.get(2 * x + dx, 2 * y + dy);
                    (0..3).for_each(|c| avg[c] += p[c] / 4.0);
                }
                diff += distance(avg, small.get(x, y));
            }
        }
        let diff = diff / (150.0 * 100.0);
        assert!(diff < 0.02, "mean difference between resolutions {diff}");
    }

    #[test]
    fn downscaled_estimate_matches_full_resolution() {
        // Forcing a 4× downscale for the estimate changes the result only a little.
        let img = hazy(240, 160);
        let full = dehazed(&img, 100.0);
        let k = DehazeStrengths {
            estimate_long_side: 60.0,
            ..DehazeStrengths::default()
        };
        let mut coarse = img.clone();
        apply(&mut coarse, 100.0, &k);
        let truth = ground(&Image::from_fn(240, 160, scene));
        let (e_full, e_coarse) = (
            rms_error(&ground(&full), &truth),
            rms_error(&ground(&coarse), &truth),
        );
        let e_hazy = rms_error(&ground(&img), &truth);
        assert!(e_coarse < 0.7 * e_hazy, "{e_hazy} {e_full} {e_coarse}");
        assert_valid(&coarse, "coarse estimate");
    }

    #[test]
    fn extremes_and_tiny_images_stay_finite() {
        let k = DehazeStrengths::default();
        let cases = [
            ("black", Image::new(64, 48)),
            ("white", Image::from_fn(64, 48, |_, _| [1.0, 1.0, 1.0])),
            (
                "very bright",
                Image::from_fn(33, 17, |_, _| [40.0, 35.0, 50.0]),
            ),
            ("1x1", Image::from_fn(1, 1, |_, _| [0.3, 0.2, 0.1])),
            ("1x1 black", Image::new(1, 1)),
            (
                "2x3",
                Image::from_fn(2, 3, |x, y| [0.1 * x as f32, 0.2 * y as f32, 0.5]),
            ),
            ("odd", hazy(37, 23)),
            (
                "row",
                Image::from_fn(57, 1, |x, _| [x as f32 / 57.0, 0.4, 0.2]),
            ),
            (
                "column",
                Image::from_fn(1, 41, |_, y| [0.6, y as f32 / 41.0, 0.2]),
            ),
            (
                "inf pixel",
                Image::from_fn(16, 16, |x, y| {
                    if (x, y) == (5, 7) {
                        [f32::INFINITY, 0.3, f32::NAN]
                    } else {
                        [0.4, 0.5, 0.6]
                    }
                }),
            ),
            (
                "negative input",
                Image::from_fn(16, 16, |x, y| {
                    [x as f32 * 0.05 - 0.2, 0.3, y as f32 * 0.04 - 0.1]
                }),
            ),
        ];
        for (name, img) in cases {
            for amount in [100.0, 37.0, -37.0, -100.0, 250.0, -250.0] {
                let mut out = img.clone();
                apply(&mut out, amount, &k);
                assert_valid(&out, &format!("{name} at {amount}"));
            }
        }
        // Black stays black when removing haze. A uniform white frame is all
        // haze: it stays uniform and neutral and darkens moderately.
        let black = dehazed(&Image::new(20, 20), 100.0);
        assert!(black.rgb.iter().all(|&v| v == 0.0));
        let white = dehazed(&Image::from_fn(20, 20, |_, _| [1.0; 3]), 100.0);
        let v0 = white.rgb[0];
        assert!(
            white.rgb.iter().all(|&v| (v - v0).abs() < 1e-6) && v0 > 0.6 && v0 < 1.0,
            "{:?}",
            &white.rgb[..3]
        );
    }

    #[test]
    fn positive_amount_darkens_the_sky() {
        // The pure-haze sky band is the haziest region: removing haze darkens
        // it, more so as the amount grows, and never brightens anything there.
        let img = hazy(180, 120);
        let lum = luminance_weights();
        let sky = |i: &Image| dot(lum, i.get(90, 5));
        let mut last = sky(&img);
        for amount in [25.0, 50.0, 100.0] {
            let y = sky(&dehazed(&img, amount));
            assert!(
                y < last,
                "sky luminance at +{amount}: {y} (previous {last})"
            );
            last = y;
        }
        let out = dehazed(&img, 100.0);
        for y in 0..img.height / 6 {
            for x in 0..img.width {
                let (a, b) = (img.get(x, y), out.get(x, y));
                assert!(
                    dot(lum, b) <= dot(lum, a) * 1.001,
                    "({x},{y}): {a:?} → {b:?}"
                );
            }
        }
    }

    #[test]
    fn highlights_brighter_than_the_airlight_are_not_boosted() {
        // Small speculars and a "sun" far above the haze level: haze removal
        // must not multiply them.
        let mut img = hazy(180, 120);
        let mut spots = Vec::new();
        for (x0, y0) in [(30, 60), (100, 80), (150, 40)] {
            for y in y0..y0 + 4 {
                for x in x0..x0 + 4 {
                    img.set(x, y, [5.0; 3]);
                    spots.push((x, y));
                }
            }
        }
        for y in 8..14 {
            for x in 60..66 {
                img.set(x, y, [20.0, 19.0, 17.0]);
                spots.push((x, y));
            }
        }
        // Haze removal alone leaves them as they are; the exposure compensation
        // may then lift them like their surroundings, no more.
        let no_comp = DehazeStrengths {
            exposure_compensation: 0.0,
            ..DehazeStrengths::default()
        };
        for amount in [30.0, 60.0, 100.0] {
            for (k, bound) in [(no_comp, 1.0), (DehazeStrengths::default(), 1.3)] {
                let mut out = img.clone();
                apply(&mut out, amount, &k);
                assert_valid(&out, "speculars");
                for &(x, y) in &spots {
                    let (a, b) = (img.get(x, y), out.get(x, y));
                    for c in 0..3 {
                        assert!(
                            b[c] <= a[c] * bound + 1e-5,
                            "+{amount} ({x},{y}): {a:?} → {b:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn bright_object_does_not_take_over_the_airlight() {
        // A neutral highlight 1.5–8× brighter than the haze (a sunlit wall, a
        // lamp, a clipped sun) is not haze: the result elsewhere stays close to
        // the result without it.
        let (w, h) = (300, 200);
        let clean = hazy(w, h);
        let reference = dehazed(&clean, 100.0);
        let lum = luminance_weights();
        let (bx, by, size) = (200usize, 120usize, 15usize);
        let outside = |x: usize, y: usize| {
            !(bx.saturating_sub(10)..bx + size + 10).contains(&x)
                || !(by.saturating_sub(10)..by + size + 10).contains(&y)
        };
        for level in [1.5f32, 2.0, 8.0] {
            let mut img = clean.clone();
            for y in by..by + size {
                for x in bx..bx + size {
                    img.set(x, y, [level; 3]);
                }
            }
            let out = dehazed(&img, 100.0);
            let mut diff = 0.0f32;
            let mut n = 0;
            for y in 0..h {
                for x in 0..w {
                    if outside(x, y) {
                        diff += distance(out.get(x, y), reference.get(x, y));
                        n += 1;
                    }
                }
            }
            let diff = diff / n as f32;
            assert!(
                diff < 0.03,
                "block at {level}: mean change elsewhere {diff}"
            );
            let (sky_ref, sky) = (dot(lum, reference.get(50, 12)), dot(lum, out.get(50, 12)));
            assert!(
                sky > 0.9 * sky_ref,
                "block at {level}: sky {sky} vs {sky_ref} without it"
            );
        }
    }

    #[test]
    fn non_finite_input_pixels_stay_local() {
        // One garbage pixel (inf or NaN) must not disable anything frame-wide
        // or leak non-finite values.
        let img = hazy(300, 200);
        for amount in [60.0, -60.0] {
            let reference = dehazed(&img, amount);
            for bad in [f32::INFINITY, f32::NAN, f32::NEG_INFINITY] {
                let mut garbage = img.clone();
                garbage.set(150, 100, [bad; 3]);
                let out = dehazed(&garbage, amount);
                assert_valid(&out, &format!("{bad} at {amount}"));
                let mut worst = 0.0f32;
                for y in 0..200usize {
                    for x in 0..300usize {
                        if x.abs_diff(150) > 20 || y.abs_diff(100) > 20 {
                            worst = worst.max(distance(out.get(x, y), reference.get(x, y)));
                        }
                    }
                }
                assert!(
                    worst < 1e-3,
                    "{bad} at {amount}: far pixels moved by {worst}"
                );
            }
        }
    }

    #[test]
    fn deterministic() {
        let img = hazy(131, 77);
        for amount in [65.0, -65.0] {
            assert_eq!(dehazed(&img, amount).rgb, dehazed(&img, amount).rgb);
        }
    }

    #[test]
    fn min_filter_matches_brute_force() {
        let (w, h) = (23, 17);
        let p: Vec<f32> = (0..w * h)
            .map(|i| ((i * 7919) % 101) as f32 / 101.0)
            .collect();
        for r in [0usize, 1, 2, 5, 30] {
            let fast = min_filter(&p, w, h, r);
            for y in 0..h {
                for x in 0..w {
                    let mut m = f32::INFINITY;
                    for yy in y.saturating_sub(r)..(y + r + 1).min(h) {
                        for xx in x.saturating_sub(r)..(x + r + 1).min(w) {
                            m = m.min(p[yy * w + xx]);
                        }
                    }
                    assert_eq!(fast[y * w + x], m, "r={r} ({x},{y})");
                }
            }
        }
    }

    #[test]
    fn downscale_averages_blocks() {
        let img = Image::from_fn(5, 3, |x, y| [x as f32, y as f32, 1.0]);
        let (small, sw, sh) = downscale(&img, 2);
        assert_eq!((sw, sh), (3, 2));
        // Top-left block: x ∈ {0,1}, y ∈ {0,1}; bottom-right partial block: x = 4, y = 2.
        assert_eq!(&small[..3], &[0.5, 0.5, 1.0]);
        assert_eq!(&small[small.len() - 3..], &[4.0, 2.0, 1.0]);
    }

    #[test]
    fn strengths_serde_round_trip_and_defaults() {
        let k: DehazeStrengths = serde_json::from_str(r#"{"omega": 0.7}"#).unwrap();
        assert_eq!(k.omega, 0.7);
        assert_eq!(k.min_transmission, DEFAULT_STRENGTHS.min_transmission);
        let back: DehazeStrengths =
            serde_json::from_str(&serde_json::to_string(&k).unwrap()).unwrap();
        assert_eq!(back, k);
        assert_eq!(DehazeStrengths::default(), DEFAULT_STRENGTHS);
    }

    /// Timing on a 24 MP frame: `cargo test --release -p amb-engine dehaze_speed -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn dehaze_speed_24mp() {
        let img = hazy(6000, 4000);
        for amount in [60.0, -60.0] {
            let mut out = img.clone();
            let start = std::time::Instant::now();
            apply(&mut out, amount, &DehazeStrengths::default());
            println!(
                "dehaze {amount:+} on 6000x4000: {:.3}s",
                start.elapsed().as_secs_f64()
            );
            assert_valid(&out, "24 MP");
        }
    }
}
