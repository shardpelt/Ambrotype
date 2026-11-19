// SPDX-License-Identifier: GPL-3.0-or-later
//! Dehaze: remove (positive) or add (negative) atmospheric haze.
//!
//! Interface contract (pipeline/ depends on it):
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

use crate::math::lerp_position;

mod filters;
#[cfg(test)]
mod tests;

use filters::{deterministic_mean, downscale, guided_coefficients, min_filter, resample};
