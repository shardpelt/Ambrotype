// SPDX-License-Identifier: GPL-3.0-or-later
//! Effects panel: post-crop vignette and grain.
//!
//! Both run last, on the display-linear ProPhoto image (nominally 0..1):
//! vignette first, then grain. Parameters are Camera Raw slider values; each
//! effect is an exact no-op when its amount is 0, and grain is deterministic
//! for a given seed and image size.
//!
//! **Model v1 (ours, not Adobe's).** Every tunable constant lives in
//! [`EffectsStrengths`] so the calibration harness can fit it against
//! Lightroom sweeps (docs/lightroom-trial-plan.md).
//!
//! *Vignette.* Each pixel gets a radius `d` in normalised, aspect-aware frame
//! coordinates: `u, v` run −1..1 edge to edge and `d` is scaled so it is 0 at
//! the centre and 1 at the frame corners for every shape. The shape is a
//! superellipse `(|u·ax|^p + |v·ay|^p)^(1/p)`: Roundness 0 is the ellipse with
//! the frame's aspect (`p = 2`, `ax = ay = 1`), Roundness +100 blends the axis
//! scales to a circle, and Roundness −100 raises `p` towards a rounded
//! rectangle that follows the frame. Midpoint places the centre of the
//! transition (lower reaches further into the image), Feather sets its width
//! (smoothstep; Feather 0 is a hard edge, antialiased over about a pixel, and
//! kept just inside the frame corner at every Midpoint so it always shows). When
//! a low Midpoint and a wide Feather start the transition before the centre,
//! the profile is mirrored about the centre (`S(d) + S(−d)`) and renormalised,
//! so the mask is exactly 0 and flat there rather than a cone. Everything is in
//! normalised coordinates, so the vignette is resolution independent.
//!
//! The mask then drives one of three styles:
//! - **Highlight Priority** (1): an exposure-like gain `G = 2^(stops · amount · mask)`
//!   applied per channel through `v·G / (1 + (G − 1)·h·v)`, which is a plain
//!   exposure change for `h = 0` and pins white for `h = 1`. Darkening uses a
//!   small base protection that HighlightContrast raises (bright corners keep
//!   their contrast); lightening uses a full shoulder so nothing clips. Being
//!   per channel, it shifts saturation slightly, as Adobe's help warns.
//! - **Color Priority** (2): the same curve on luminance only, applied as an
//!   RGB ratio so hue and saturation are preserved; brightening is capped so
//!   no channel is pushed past 1.
//! - **Paint Overlay** (3): blend towards black (negative) or white (positive)
//!   in a gamma-encoded space; HighlightContrast does not apply. Mixing with
//!   black in a power-law encoding is a plain gain in linear (the corners can
//!   reach black); mixing with white lifts the blacks, the flat look Adobe
//!   describes.
//!
//! *Grain.* Zero-mean white noise from an integer hash of `(x, y, seed)` is
//! blurred at two scales: `fine` at a sigma set by GrainSize (defined at a
//! 24 MP / 6000 × 4000 reference and scaled with the image's linear size) and
//! `coarse` at a multiple of it. Roughness mixes them as `fine − (1 − r)·coarse`:
//! Roughness 0 is a band-pass (regular grains of one size), 100 keeps the
//! coarse octave (clumpy, uneven grain). Strength is resolution independent
//! too: the field has unit deviation at the reference size, and at any other
//! size the deviation of the reference field box-averaged to that size, so a
//! small render (or preview) shows what the full render scaled down shows.
//! Both are exact expected variances, computed from the blur kernels (the
//! 2-D kernel's autocorrelation is a sum of separable terms). The field `n`
//! of variance `V` is applied as a monochrome luminance ratio
//! `exp(a·n − a²·V/2)` (mean-preserving, never negative), with `a` weighted by
//! `4·L·(1 − L)` on CIE lightness so grain is strongest in the midtones and
//! fades towards black and white. Colours do not shift.
//!
//! The field is computed tile by tile with a margin of procedural noise around
//! each tile, so it never touches a frame-sized plane: memory stays small, the
//! image is read and written once, and there are no clamped-edge streaks
//! along the frame borders. The blurs are tile-local separable passes over
//! whole rows (no edge clamping or transposes, buffers reused per thread).
//! Each pixel's grain depends only on `(x, y, seed)` and the settings, bit for
//! bit, not on the tiling.

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::buffer::{Image, luminance_weights};

/// `PostCropVignetteAmount` -100..100, `...Midpoint` 0..100 (50), `...Roundness` -100..100 (0),
/// `...Feather` 0..100 (50), `...HighlightContrast` 0..100 (0), `...Style` 1 = Highlight
/// Priority, 2 = Color Priority, 3 = Paint Overlay.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vignette {
    pub amount: f64,
    pub midpoint: f64,
    pub roundness: f64,
    pub feather: f64,
    pub highlights: f64,
    pub style: u8,
}

impl Default for Vignette {
    fn default() -> Self {
        Vignette {
            amount: 0.0,
            midpoint: 50.0,
            roundness: 0.0,
            feather: 50.0,
            highlights: 0.0,
            style: 1,
        }
    }
}

/// `GrainAmount` 0..100, `GrainSize` 0..100 (25), `GrainFrequency` (roughness) 0..100 (50).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Grain {
    pub amount: f64,
    pub size: f64,
    pub roughness: f64,
    pub seed: u64,
}

impl Default for Grain {
    fn default() -> Self {
        Grain {
            amount: 0.0,
            size: 25.0,
            roughness: 50.0,
            seed: 0,
        }
    }
}

/// Tunable constants. Starting guesses, to be fitted against Lightroom sweeps.
///
/// Vignette radii are fractions of the centre-to-corner distance (0 = centre,
/// 1 = corner) in the normalised frame coordinates described in the module docs.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EffectsStrengths {
    /// Centre of the vignette transition at Midpoint 0.
    pub vignette_midpoint_min: f64,
    /// Centre of the vignette transition at Midpoint 100. Kept inside the frame
    /// (at most 1.5 px short of the corner), so a hard edge always shows.
    pub vignette_midpoint_max: f64,
    /// Full width of the transition at Feather 100 (Feather scales it linearly;
    /// Feather 0 is a hard edge about one pixel wide).
    pub vignette_feather_width: f64,
    /// Superellipse exponent at Roundness −100 (rounded rectangle). Roundness 0
    /// is 2 (ellipse); in between it is interpolated geometrically.
    pub vignette_rect_exponent: f64,
    /// Exposure change, in stops, where the mask is 1 at Amount −100
    /// (Highlight and Color Priority).
    pub vignette_dark_stops: f64,
    /// Exposure change, in stops, where the mask is 1 at Amount +100.
    pub vignette_light_stops: f64,
    /// Highlight protection `h` (0 = plain exposure, 1 = white pinned) for
    /// negative amounts at HighlightContrast 0, Highlight Priority.
    pub vignette_highlight_priority_protect: f64,
    /// The same for Color Priority.
    pub vignette_color_priority_protect: f64,
    /// Highlight protection at HighlightContrast 100 (both priority styles).
    pub vignette_highlight_contrast_protect: f64,
    /// Highlight protection (shoulder) for positive amounts; 1 never clips.
    pub vignette_lighten_shoulder: f64,
    /// Paint Overlay blend opacity where the mask is 1 at |Amount| 100.
    pub vignette_paint_opacity: f64,
    /// Encoding gamma of the Paint Overlay blend.
    pub vignette_paint_gamma: f64,
    /// Grain blur sigma in pixels at GrainSize 0, for a 24 MP reference image.
    pub grain_sigma_min_px: f64,
    /// Grain blur sigma in pixels at GrainSize 100, for a 24 MP reference image.
    pub grain_sigma_max_px: f64,
    /// Shape of the GrainSize → sigma mapping: `(size / 100)^exponent`.
    pub grain_size_exponent: f64,
    /// Sigma of the coarse grain octave relative to the fine one.
    pub grain_octave_ratio: f64,
    /// Log-luminance deviation of the grain in the midtones at GrainAmount 100.
    pub grain_strength: f64,
    /// Exponent on the midtone weight `4·L·(1 − L)`; higher confines grain
    /// more to the midtones.
    pub grain_tone_power: f64,
}

impl Default for EffectsStrengths {
    fn default() -> Self {
        EffectsStrengths {
            vignette_midpoint_min: 0.3,
            vignette_midpoint_max: 0.9,
            vignette_feather_width: 1.0,
            vignette_rect_exponent: 8.0,
            vignette_dark_stops: 2.5,
            vignette_light_stops: 2.0,
            vignette_highlight_priority_protect: 0.3,
            vignette_color_priority_protect: 0.0,
            vignette_highlight_contrast_protect: 1.0,
            vignette_lighten_shoulder: 1.0,
            vignette_paint_opacity: 1.0,
            vignette_paint_gamma: 2.2,
            grain_sigma_min_px: 0.5,
            grain_sigma_max_px: 4.0,
            grain_size_exponent: 1.5,
            grain_octave_ratio: 2.5,
            grain_strength: 0.3,
            grain_tone_power: 1.0,
        }
    }
}

/// Image area that the resolution-independent grain size is defined at (6000 × 4000).
const REFERENCE_PIXELS: f64 = 6000.0 * 4000.0;

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

// ---------------------------------------------------------------- vignette

/// Separable vignette mask: `d^p = col[x] + row[y]`, then a renormalised smoothstep on `d`.
struct VignetteMask {
    col: Vec<f32>,
    row: Vec<f32>,
    /// Superellipse exponent (2 takes a `sqrt` fast path).
    p: f32,
    inner: f32,
    outer: f32,
    /// The transition starts before the centre (`inner < 0`): add the mirrored
    /// profile `S(−d)` so the mask is even in `d`, hence flat at the centre.
    mirrored: bool,
    /// Profile value at the centre (`2·S(0)` when mirrored, else 0) and
    /// `1 / (1 − base)`: the mask is `(profile − base)·norm`.
    base: f32,
    norm: f32,
    /// `d^p` at or below which the mask is exactly 0 (negative when nothing can be skipped).
    q_skip: f32,
}

impl VignetteMask {
    fn new(w: usize, h: usize, v: &Vignette, k: &EffectsStrengths) -> VignetteMask {
        let roundness = (v.roundness / 100.0).clamp(-1.0, 1.0);
        let circle = roundness.max(0.0);
        let rect = (-roundness).max(0.0);
        let p = 2.0 * (k.vignette_rect_exponent.max(2.0) / 2.0).powf(rect);
        let (wf, hf) = (w as f64, h as f64);
        // Axis scales: 1 for the frame-aspect ellipse; for the circle, equal pixel
        // scales (sqrt(w/h), sqrt(h/w) in the normalised units).
        let ax = 1.0 + ((wf / hf).sqrt() - 1.0) * circle;
        let ay = 1.0 + ((hf / wf).sqrt() - 1.0) * circle;
        let corner = ax.powf(p) + ay.powf(p);
        // Pixel-centre distance from the centre line, normalised so the frame edge is 1.
        // Computed from integers so mirrored pixels get bit-identical values.
        let term = |i: usize, n: usize, a: f64| -> f32 {
            let u = (2.0 * i as f64 - (n as f64 - 1.0)).abs() / n as f64;
            ((u * a).powf(p) / corner) as f32
        };
        let col = (0..w).map(|x| term(x, w, ax)).collect();
        let row = (0..h).map(|y| term(y, h, ay)).collect();

        // One pixel along the diagonal, in units of `d`.
        let px = 1.0 / (0.5 * (wf * wf + hf * hf).sqrt());
        // The transition centre stays strictly outside the centre of the frame
        // (also for fitted constants), so the mirrored profile below stays monotonic,
        // and 1.5 px inside the corner, so the hard edge of Feather 0 (half a pixel
        // either side) never falls outside the frame and Feather only softens it.
        let mid = (k.vignette_midpoint_min
            + (k.vignette_midpoint_max - k.vignette_midpoint_min)
                * (v.midpoint / 100.0).clamp(0.0, 1.0))
        .min(1.0 - 1.5 * px)
        .max(0.01);
        // Half-width of the transition; at least half a pixel (along the diagonal).
        let half = (0.5 * k.vignette_feather_width.max(0.0) * (v.feather / 100.0).clamp(0.0, 1.0))
            .max(0.5 * px);
        let inner = (mid - half) as f32;
        let outer = (mid + half) as f32;
        // S(d) + S(−d) is even and, because the transition centre is > 0, rises
        // monotonically from 2·S(0) < 1 to 1 (at d = outer).
        let mirrored = inner < 0.0;
        let base = if mirrored {
            2.0 * smoothstep(inner, outer, 0.0)
        } else {
            0.0
        };
        let q_skip = if mirrored {
            -1.0
        } else {
            (inner as f64).powf(p) as f32
        };
        VignetteMask {
            col,
            row,
            p: p as f32,
            inner,
            outer,
            mirrored,
            base,
            norm: 1.0 / (1.0 - base),
            q_skip,
        }
    }

    /// Mask in 0..1 at pixel (x, y); exactly 0 at the centre.
    #[inline]
    fn at(&self, x: usize, y: usize) -> f32 {
        let q = self.col[x] + self.row[y];
        if q <= self.q_skip {
            return 0.0;
        }
        let d = if self.p == 2.0 {
            q.sqrt()
        } else {
            q.powf(1.0 / self.p)
        };
        let mut s = smoothstep(self.inner, self.outer, d);
        if self.mirrored {
            s += smoothstep(self.inner, self.outer, -d);
        }
        ((s - self.base) * self.norm).clamp(0.0, 1.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum VignetteStyle {
    HighlightPriority,
    ColorPriority,
    PaintOverlay,
}

/// Per-pixel tone operation of the vignette, given the mask value.
struct VignetteOp {
    style: VignetteStyle,
    /// Signed exposure change at mask 1, in stops (priority styles).
    stops: f32,
    /// Highlight protection `h` in 0..1 (priority styles).
    protect: f32,
    /// Paint Overlay: opacity at mask 1, target (0 black, 1 white) and gamma.
    opacity: f32,
    target: f32,
    gamma: f32,
    lum: [f32; 3],
}

impl VignetteOp {
    fn new(v: &Vignette, k: &EffectsStrengths, amount: f64) -> VignetteOp {
        let style = match v.style {
            2 => VignetteStyle::ColorPriority,
            3 => VignetteStyle::PaintOverlay,
            _ => VignetteStyle::HighlightPriority,
        };
        let (stops, protect) = if amount < 0.0 {
            let base = match style {
                VignetteStyle::ColorPriority => k.vignette_color_priority_protect,
                _ => k.vignette_highlight_priority_protect,
            };
            // `clamp` passes NaN through; a malformed slider value means 0.
            let hc = if v.highlights.is_nan() {
                0.0
            } else {
                (v.highlights / 100.0).clamp(0.0, 1.0)
            };
            let h = base + (k.vignette_highlight_contrast_protect - base) * hc;
            (amount * k.vignette_dark_stops, h)
        } else {
            (amount * k.vignette_light_stops, k.vignette_lighten_shoulder)
        };
        VignetteOp {
            style,
            stops: stops as f32,
            protect: if protect.is_nan() {
                0.0
            } else {
                protect.clamp(0.0, 1.0) as f32
            },
            opacity: (amount.abs() * k.vignette_paint_opacity).clamp(0.0, 1.0) as f32,
            target: if amount < 0.0 { 0.0 } else { 1.0 },
            gamma: k.vignette_paint_gamma.max(0.1) as f32,
            lum: luminance_weights(),
        }
    }

    #[inline]
    fn apply(&self, c: [f32; 3], m: f32) -> [f32; 3] {
        match self.style {
            VignetteStyle::PaintOverlay => {
                let a = self.opacity * m;
                if self.target == 0.0 {
                    // `(e·(1 − a))^γ = v·(1 − a)^γ`: one pow per pixel, and
                    // out-of-range values scale like everything else.
                    let g = (1.0 - a).powf(self.gamma);
                    c.map(|v| v * g)
                } else {
                    let inv = 1.0 / self.gamma;
                    c.map(|v| {
                        let e = signed_pow(v, inv);
                        signed_pow(e + (1.0 - e) * a, self.gamma)
                    })
                }
            }
            VignetteStyle::HighlightPriority => {
                let g = (self.stops * m).exp2();
                c.map(|v| protected_exposure(v, g, self.protect))
            }
            VignetteStyle::ColorPriority => {
                let g = (self.stops * m).exp2();
                let y = self.lum[0] * c[0] + self.lum[1] * c[1] + self.lum[2] * c[2];
                let mut scale = if y > 1e-9 {
                    protected_exposure(y, g, self.protect) / y
                } else {
                    g
                };
                if scale > 1.0 {
                    // Never push a channel past 1 (or further past it): clipping one
                    // channel would shift the hue this style exists to keep.
                    let max = c[0].max(c[1]).max(c[2]);
                    if max > 0.0 {
                        scale = scale.min((1.0 / max).max(1.0));
                    }
                }
                c.map(|v| v * scale)
            }
        }
    }
}

/// `|v|^e` with the sign of `v` (an odd extension, so negative values stay continuous).
#[inline]
fn signed_pow(v: f32, e: f32) -> f32 {
    v.abs().powf(e).copysign(v)
}

/// Exposure gain `g` with highlight protection `h` in 0..1:
/// `v·g / (1 + (g − 1)·h·v)` on 0..1 (slope `g` at black; white pinned for `h = 1`),
/// continued linearly outside 0..1. Identity for `g = 1`; monotonic and continuous.
///
/// Above 1 the continuation uses the curve's slope at 1 (`g / den²`), limited
/// to at most 1 when darkening and at least 1 when lightening: with strong
/// protection that slope is `1/g`, which would push a channel above 1 the
/// wrong way (a darkening vignette brightening it). Below 0 it is the plain
/// gain `g`, so out-of-gamut negatives scale like everything else.
#[inline]
fn protected_exposure(v: f32, g: f32, h: f32) -> f32 {
    let c = v.clamp(0.0, 1.0);
    // g > 0 and h <= 1 keep the denominator >= min(g, 1) > 0.
    let den = 1.0 + (g - 1.0) * h * c;
    let out = c * g / den;
    if v < 0.0 {
        out + v * g
    } else if v > 1.0 {
        let slope = g / (den * den);
        let slope = if g < 1.0 {
            slope.min(1.0)
        } else {
            slope.max(1.0)
        };
        out + (v - 1.0) * slope
    } else {
        out
    }
}

/// Post-crop vignette (see the module docs for the model). No-op at amount 0.
pub fn apply_vignette(img: &mut Image, p: &Vignette, k: &EffectsStrengths) {
    let (w, h) = (img.width, img.height);
    let amount = (p.amount / 100.0).clamp(-1.0, 1.0);
    if amount == 0.0 || amount.is_nan() || w == 0 || h == 0 {
        return;
    }
    let mask = VignetteMask::new(w, h, p, k);
    let op = VignetteOp::new(p, k, amount);
    img.rgb
        .par_chunks_mut(w * 3)
        .enumerate()
        .for_each(|(y, row)| {
            for (x, px) in row.chunks_exact_mut(3).enumerate() {
                let m = mask.at(x, y);
                if m > 0.0 {
                    let out = op.apply([px[0], px[1], px[2]], m);
                    px.copy_from_slice(&out);
                }
            }
        });
}

// ------------------------------------------------------------------- grain

/// SplitMix64 finaliser: a fast, well-mixed 64-bit integer hash.
#[inline]
fn mix64(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Variance of [`hash_noise`]: `(N² − 1) / (3·N²)` for its `N = 2^24` levels.
const WHITE_VARIANCE: f64 = (1.0 - 1.0 / (16_777_216.0 * 16_777_216.0)) / 3.0;

/// Uniform noise in (−1, 1) at integer coordinates (either sign, |x|, |y| < 2^31)
/// with mean exactly 0: the odd values `(2k + 1 − N) / N` for a 24-bit hash `k`,
/// all exact in f32. `seed_key` is `mix64` of the seed.
#[inline]
fn hash_noise(x: i64, y: i64, seed_key: u64) -> f32 {
    let key = ((y as u32 as u64) << 32) | (x as u32 as u64);
    let z = mix64(
        key.wrapping_mul(0x9E37_79B9_7F4A_7C15)
            .wrapping_add(seed_key),
    );
    let k = (z >> 40) as i32;
    (2 * k + 1 - (1 << 24)) as f32 * (1.0 / (1u32 << 24) as f32)
}

/// Grain blur sigma in pixels at the 24 MP reference for a GrainSize.
fn grain_sigma_ref(size: f64, k: &EffectsStrengths) -> f32 {
    let t = (size / 100.0).clamp(0.0, 1.0);
    let sigma_ref = k.grain_sigma_min_px
        + (k.grain_sigma_max_px - k.grain_sigma_min_px) * t.powf(k.grain_size_exponent);
    sigma_ref.max(0.0) as f32
}

/// The image's linear size relative to the 24 MP reference.
fn grain_scale(w: usize, h: usize) -> f32 {
    ((w * h) as f64 / REFERENCE_PIXELS).sqrt() as f32
}

/// Grain blur sigma in pixels: GrainSize mapped at the 24 MP reference, scaled
/// by the image's linear size relative to it.
#[cfg(test)]
fn grain_sigma_px(size: f64, w: usize, h: usize, k: &EffectsStrengths) -> f32 {
    grain_sigma_ref(size, k) * grain_scale(w, h)
}

/// Safety bound on a grain blur sigma in pixels (Size 100 at 24 MP is a 10 px
/// coarse octave), so extreme fitted constants or render sizes cannot ask for
/// a huge kernel.
const GRAIN_SIGMA_LIMIT_PX: f32 = 64.0;

/// Normalised Gaussian kernel of radius `ceil(3σ)`, as its centre tap followed
/// by one side (`k[t]` is the weight at offset ±t). `[1]` for σ = 0.
fn gaussian_half_kernel(sigma: f32) -> Vec<f32> {
    let sigma = sigma.clamp(0.0, GRAIN_SIGMA_LIMIT_PX) as f64;
    let r = (3.0 * sigma).ceil() as usize;
    if r == 0 {
        return vec![1.0];
    }
    let w: Vec<f64> = (0..=r)
        .map(|t| (-((t * t) as f64) / (2.0 * sigma * sigma)).exp())
        .collect();
    let sum = w[0] + 2.0 * w[1..].iter().sum::<f64>();
    w.iter().map(|&v| (v / sum) as f32).collect()
}

/// `S = Σ_d A(d)·T(d)` for two half kernels `x`, `y`: `A` is their cross-correlation
/// and `T` the autocorrelation of a box `l` pixels wide normalised to sum 1,
/// `T(d) ∝ max(l − |d|, 0)` (exact box averaging for integer `l`, a delta for `l <= 1`).
fn box_correlation(x: &[f32], y: &[f32], l: f64) -> f64 {
    let full =
        |h: &[f32]| -> Vec<f64> { h.iter().rev().chain(&h[1..]).map(|&v| v as f64).collect() };
    let (fx, fy) = (full(x), full(y));
    let (rx, ry) = (x.len() as isize - 1, y.len() as isize - 1);
    // Σ_{d ∈ Z} max(l − |d|, 0) = l + 2·(n·l − n(n + 1)/2), n = number of d >= 1 below l.
    let n = (l.ceil() - 1.0).max(0.0);
    let norm = l + 2.0 * (n * l - n * (n + 1.0) / 2.0);
    let mut s = 0.0;
    for d in -(rx + ry)..=(rx + ry) {
        let t = (l - d.unsigned_abs() as f64).max(0.0);
        if t == 0.0 {
            continue;
        }
        // A(d) = Σ_i x(i)·y(i + d), offsets from the kernel centres.
        let a: f64 = (-rx..=rx)
            .filter(|i| (i + d).abs() <= ry)
            .map(|i| fx[(i + rx) as usize] * fy[(i + d + ry) as usize])
            .sum();
        s += a * t;
    }
    s / norm
}

/// Variance (per unit white-noise variance) of the field `F⊗F − c·G⊗G` averaged
/// over `l × l` boxes; `l <= 1` is the per-pixel variance `Σ kernel²`. The
/// 2-D kernel's autocorrelation is a sum of separable terms, so this is
/// `S_ff² − 2c·S_fg² + c²·S_gg²` with the 1-D sums of [`box_correlation`].
fn box_variance(fine: &[f32], coarse: &[f32], c: f64, l: f64) -> f64 {
    let sff = box_correlation(fine, fine, l);
    if c == 0.0 {
        return sff * sff;
    }
    let sfg = box_correlation(fine, coarse, l);
    let sgg = box_correlation(coarse, coarse, l);
    (sff * sff - 2.0 * c * sfg * sfg + c * c * sgg * sgg).max(0.0)
}

/// Smallest tile edge of the grain field; tiles grow with the margin so the
/// procedural border stays a modest fraction of the work.
const GRAIN_TILE_MIN: usize = 256;

/// The grain field `n(x, y)`: zero-mean, a function of `(x, y, seed)` and the
/// settings only. Computed in tiles (see the module docs).
struct GrainField {
    /// Fine and coarse Gaussian half kernels (pixels at this render size).
    fine: Vec<f32>,
    coarse: Vec<f32>,
    /// Weight of the coarse octave subtracted from the fine one (`1 − roughness`).
    coarse_weight: f32,
    /// Procedural noise border around each tile: the blurs' reach.
    margin: usize,
    /// Scale from the filtered noise to the normalised field (0 when it is identically 0).
    inv_std: f32,
    /// Expected variance of the normalised field: 1 at the reference size, and
    /// what the reference field box-averaged to this size has elsewhere.
    variance: f32,
    seed_key: u64,
}

impl GrainField {
    /// `sigma_ref`: fine sigma in pixels at the 24 MP reference; `scale`: this
    /// render's linear size relative to the reference.
    fn new(
        sigma_ref: f32,
        scale: f32,
        roughness: f32,
        seed: u64,
        k: &EffectsStrengths,
    ) -> GrainField {
        let sigma_ref = if sigma_ref.is_finite() {
            sigma_ref.max(0.0)
        } else {
            0.0
        };
        let scale = if scale.is_finite() && scale > 0.0 {
            scale
        } else {
            1.0
        };
        let ratio = k.grain_octave_ratio.max(1.0) as f32;
        let r = if roughness.is_nan() {
            0.5
        } else {
            roughness.clamp(0.0, 1.0)
        };
        let coarse_weight = 1.0 - r;
        let c = coarse_weight as f64;
        // The coarse octave is at least a pixel wide so the band-pass (Roughness 0)
        // never collapses to nothing on small images.
        let kernels = |sigma: f32| {
            let sigma = sigma.min(GRAIN_SIGMA_LIMIT_PX);
            (
                gaussian_half_kernel(sigma),
                gaussian_half_kernel((sigma * ratio).max(1.0)),
            )
        };
        let (fine, coarse) = kernels(sigma_ref * scale);
        let margin = if coarse_weight != 0.0 {
            fine.len().max(coarse.len())
        } else {
            fine.len()
        } - 1;
        // Strength is defined at the reference size, so a render at another size
        // matches the reference render box-averaged to that size. Below the
        // reference the target is the reference field's variance averaged over
        // `1/scale`-pixel boxes; above it, the field is set so that averaging it
        // over `scale`-pixel boxes gives the reference's unit variance.
        let per_pixel = box_variance(&fine, &coarse, c, 1.0);
        let target = if scale < 1.0 {
            let (f_ref, c_ref) = kernels(sigma_ref);
            let unit = box_variance(&f_ref, &c_ref, c, 1.0);
            if unit > 1e-20 {
                box_variance(&f_ref, &c_ref, c, 1.0 / scale as f64) / unit
            } else {
                0.0
            }
        } else {
            let averaged = box_variance(&fine, &coarse, c, scale as f64);
            if averaged > 1e-20 {
                per_pixel / averaged
            } else {
                0.0
            }
        };
        let raw = WHITE_VARIANCE * per_pixel;
        let (inv_std, variance) = if raw > 1e-20 && target.is_finite() && target > 0.0 {
            ((target / raw).sqrt() as f32, target as f32)
        } else {
            (0.0, 0.0)
        };
        GrainField {
            fine,
            coarse,
            coarse_weight,
            margin,
            inv_std,
            variance,
            seed_key: mix64(seed ^ 0x6772_6169_6E5F_7631), // "grain_v1"
        }
    }

    /// Tile edge for this field's margin.
    fn tile_size(&self) -> usize {
        GRAIN_TILE_MIN.max(6 * self.margin)
    }

    /// Normalised field over `[x0, x0 + tw) × [y0, y0 + th)`, row-major, into
    /// `out`. The noise around the tile is generated too, so the blurs never
    /// clamp, and every value is summed in the same order whatever the tiling,
    /// so adjacent tiles join bit-identically.
    fn tile_into(
        &self,
        (x0, y0, tw, th): (usize, usize, usize, usize),
        scratch: &mut GrainScratch,
        out: &mut Vec<f32>,
    ) {
        let m = self.margin;
        let (pw, ph) = (tw + 2 * m, th + 2 * m);
        let GrainScratch {
            white,
            fine,
            coarse,
        } = scratch;
        white.clear();
        let x_start = x0 as i64 - m as i64;
        for j in 0..ph {
            let y = (y0 + j) as i64 - m as i64;
            white.extend((0..pw as i64).map(|i| hash_noise(x_start + i, y, self.seed_key)));
        }
        let white: &[f32] = white;
        // Horizontal passes: every padded row, the tile's columns only.
        let with_coarse = self.coarse_weight != 0.0;
        let horizontal = |kernel: &[f32], dst: &mut Vec<f32>| {
            dst.resize(ph * tw, 0.0);
            for (src, d) in white.chunks_exact(pw).zip(dst.chunks_exact_mut(tw)) {
                blur_taps(d, kernel, 1.0, false, |t| {
                    &src[(m as isize + t) as usize..][..tw]
                });
            }
        };
        horizontal(&self.fine, fine);
        if with_coarse {
            horizontal(&self.coarse, coarse);
        }
        let (fine, coarse): (&[f32], &[f32]) = (fine, coarse);
        // Vertical passes, adding whole rows: `inv_std·(fine − c·coarse)`.
        out.resize(tw * th, 0.0);
        let coarse_scale = -self.coarse_weight * self.inv_std;
        for (j, d) in out.chunks_exact_mut(tw).enumerate() {
            let row = move |t: isize| ((j + m) as isize + t) as usize * tw;
            blur_taps(d, &self.fine, self.inv_std, false, move |t| {
                &fine[row(t)..][..tw]
            });
            if with_coarse {
                blur_taps(d, &self.coarse, coarse_scale, true, move |t| {
                    &coarse[row(t)..][..tw]
                });
            }
        }
    }

    /// [`GrainField::tile_into`] with fresh buffers.
    #[cfg(test)]
    fn tile(&self, x0: usize, y0: usize, tw: usize, th: usize) -> Vec<f32> {
        let mut out = Vec::new();
        self.tile_into((x0, y0, tw, th), &mut GrainScratch::default(), &mut out);
        out
    }
}

/// Per-thread buffers of [`GrainField::tile_into`], reused from tile to tile.
#[derive(Default)]
struct GrainScratch {
    white: Vec<f32>,
    fine: Vec<f32>,
    coarse: Vec<f32>,
}

/// One separable blur pass over a row: `dst = scale·Σ_t k[|t|]·src(t)` for
/// `t` in `−r..=r` (`+=` when `add`), mirrored taps summed first. `src(t)` is
/// the input shifted by `t` (pixels or rows), at least `dst.len()` long. Plain
/// slice loops, so the compiler vectorises them.
#[inline]
fn blur_taps<'a>(
    dst: &mut [f32],
    k: &[f32],
    scale: f32,
    add: bool,
    src: impl Fn(isize) -> &'a [f32],
) {
    let n = dst.len();
    let k0 = k[0] * scale;
    let centre = &src(0)[..n];
    if add {
        dst.iter_mut().zip(centre).for_each(|(d, &v)| *d += k0 * v);
    } else {
        dst.iter_mut().zip(centre).for_each(|(d, &v)| *d = k0 * v);
    }
    for (t, &kt) in k.iter().enumerate().skip(1) {
        let kt = kt * scale;
        let (lo, hi) = (&src(-(t as isize))[..n], &src(t as isize)[..n]);
        dst.iter_mut()
            .zip(lo.iter().zip(hi))
            .for_each(|(d, (&a, &b))| *d += kt * (a + b));
    }
}

/// CIE lightness L* / 100 from relative luminance Y (both nominally 0..1).
#[inline]
fn lightness(y: f32) -> f32 {
    const EPS: f32 = 216.0 / 24389.0;
    const KAPPA: f32 = 24389.0 / 27.0;
    if y > EPS {
        1.16 * y.cbrt() - 0.16
    } else {
        y * KAPPA / 100.0
    }
}

/// Midtone weight `(4·L·(1 − L))^power` on CIE lightness, for luminance in 0..1.
fn grain_tone_weight(y: f32, power: f32) -> f32 {
    let l = lightness(y.clamp(0.0, 1.0));
    let weight = (4.0 * l * (1.0 - l)).clamp(0.0, 1.0);
    if power == 1.0 {
        weight
    } else {
        weight.powf(power)
    }
}

/// Entries of the tone-weight table over luminance 0..1 (linearly interpolated;
/// the weight is smooth there, so the error is far below a visible level).
const GRAIN_WEIGHT_LUT: usize = 4096;

/// Per-pixel grain application: a monochrome, midtone-weighted luminance ratio.
struct GrainOp {
    /// Log deviation at full weight for a unit-variance field.
    strength: f32,
    /// Expected variance of the field (see [`GrainField`]).
    variance: f32,
    /// [`grain_tone_weight`] at `i / GRAIN_WEIGHT_LUT`, `i = 0..=GRAIN_WEIGHT_LUT`, plus a
    /// repeated last entry so interpolation at 1 stays in bounds.
    weight: Vec<f32>,
    lum: [f32; 3],
}

impl GrainOp {
    fn new(strength: f32, variance: f32, tone_power: f32) -> GrainOp {
        let mut weight: Vec<f32> = (0..=GRAIN_WEIGHT_LUT)
            .map(|i| grain_tone_weight(i as f32 / GRAIN_WEIGHT_LUT as f32, tone_power))
            .collect();
        weight.push(weight[GRAIN_WEIGHT_LUT]);
        GrainOp {
            strength,
            variance,
            weight,
            lum: luminance_weights(),
        }
    }

    #[inline]
    fn apply(&self, px: &mut [f32], n: f32) {
        let y = self.lum[0] * px[0] + self.lum[1] * px[1] + self.lum[2] * px[2];
        if y.is_nan() || y <= 0.0 {
            return;
        }
        let t = y.min(1.0) * GRAIN_WEIGHT_LUT as f32;
        let i = t as usize;
        let weight = self.weight[i] + (self.weight[i + 1] - self.weight[i]) * (t - i as f32);
        let a = self.strength * weight;
        // Log-normal ratio: E[ratio] = 1 for Gaussian n of variance V, never negative.
        let ratio = (a * n - 0.5 * a * a * self.variance).exp();
        px.iter_mut().for_each(|v| *v *= ratio);
    }
}

/// Film-like grain (see the module docs for the model). No-op at amount 0.
pub fn apply_grain(img: &mut Image, p: &Grain, k: &EffectsStrengths) {
    let (w, h) = (img.width, img.height);
    let amount = (p.amount / 100.0).clamp(0.0, 1.0);
    if amount == 0.0 || amount.is_nan() || w == 0 || h == 0 {
        return;
    }
    let field = GrainField::new(
        grain_sigma_ref(p.size, k),
        grain_scale(w, h),
        (p.roughness / 100.0) as f32,
        p.seed,
        k,
    );
    let op = GrainOp::new(
        (amount * k.grain_strength.max(0.0)) as f32,
        field.variance,
        k.grain_tone_power.max(0.0) as f32,
    );
    if field.inv_std == 0.0 || op.strength == 0.0 {
        return;
    }
    // Bands of tile rows in parallel; tiles within a band in turn, each applied
    // while its field is still in cache.
    let tile = field.tile_size();
    img.rgb
        .par_chunks_mut(tile * w * 3)
        .enumerate()
        .for_each(|(band_index, band)| {
            let y0 = band_index * tile;
            let th = band.len() / (3 * w);
            let mut scratch = GrainScratch::default();
            let mut n = Vec::new();
            for x0 in (0..w).step_by(tile) {
                let tw = tile.min(w - x0);
                field.tile_into((x0, y0, tw, th), &mut scratch, &mut n);
                for (j, n_row) in n.chunks_exact(tw).enumerate() {
                    let row = &mut band[(j * w + x0) * 3..][..tw * 3];
                    for (px, &nv) in row.chunks_exact_mut(3).zip(n_row) {
                        op.apply(px, nv);
                    }
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filters::gaussian_blur;

    fn k() -> EffectsStrengths {
        EffectsStrengths::default()
    }

    fn flat(w: usize, h: usize, v: f32) -> Image {
        Image::from_fn(w, h, |_, _| [v, v, v])
    }

    /// A busy, coloured test image with values across 0..1.
    fn busy(w: usize, h: usize) -> Image {
        Image::from_fn(w, h, |x, y| {
            let t = (x as f32 * 0.37 + y as f32 * 0.11).sin() * 0.5 + 0.5;
            [t, 0.6 * t + 0.1, (1.0 - t) * 0.8]
        })
    }

    fn vig(amount: f64) -> Vignette {
        Vignette {
            amount,
            ..Default::default()
        }
    }

    fn grain(amount: f64) -> Grain {
        Grain {
            amount,
            seed: 7,
            ..Default::default()
        }
    }

    fn vignetted(img: &Image, v: &Vignette) -> Image {
        let mut out = img.clone();
        apply_vignette(&mut out, v, &k());
        out
    }

    /// The normalised grain field over `[0, w) × [0, h)` at an explicit sigma.
    fn grain_field(w: usize, h: usize, sigma: f32, r: f32, seed: u64) -> Vec<f32> {
        GrainField::new(sigma, 1.0, r, seed, &k()).tile(0, 0, w, h)
    }

    fn grained(img: &Image, g: &Grain) -> Image {
        let mut out = img.clone();
        apply_grain(&mut out, g, &k());
        out
    }

    fn all_finite(img: &Image) -> bool {
        img.rgb.iter().all(|v| v.is_finite())
    }

    fn mean(v: &[f32]) -> f64 {
        v.iter().map(|&x| x as f64).sum::<f64>() / v.len() as f64
    }

    fn std(v: &[f32]) -> f64 {
        let m = mean(v);
        (v.iter().map(|&x| (x as f64 - m).powi(2)).sum::<f64>() / v.len() as f64).sqrt()
    }

    // ----------------------------------------------------------- vignette

    #[test]
    fn vignette_amount_zero_is_exact_noop() {
        let img = busy(41, 29);
        for style in 1..=3 {
            let v = Vignette {
                amount: 0.0,
                style,
                feather: 0.0,
                roundness: -100.0,
                highlights: 100.0,
                ..Default::default()
            };
            assert_eq!(vignetted(&img, &v).rgb, img.rgb);
        }
    }

    #[test]
    fn vignette_centre_unchanged_corners_darker_or_brighter() {
        let (w, h) = (101, 67);
        let img = flat(w, h, 0.4);
        for style in 1..=3 {
            for (amount, darker) in [(-60.0, true), (60.0, false)] {
                let out = vignetted(
                    &img,
                    &Vignette {
                        style,
                        ..vig(amount)
                    },
                );
                assert_eq!(
                    out.get(w / 2, h / 2),
                    img.get(w / 2, h / 2),
                    "style {style}"
                );
                for (x, y) in [(0, 0), (w - 1, 0), (0, h - 1), (w - 1, h - 1)] {
                    let c = out.get(x, y)[1];
                    if darker {
                        assert!(c < 0.4 * 0.6, "style {style} amount {amount}: {c}");
                    } else {
                        assert!(
                            c > 0.4 * 1.4 && c <= 1.0,
                            "style {style} amount {amount}: {c}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn vignette_is_four_fold_symmetric() {
        for (w, h) in [(64, 40), (65, 41), (30, 47)] {
            let img = flat(w, h, 0.5);
            for (style, roundness, feather) in [(1, 0.0, 50.0), (2, -100.0, 0.0), (3, 70.0, 100.0)]
            {
                let out = vignetted(
                    &img,
                    &Vignette {
                        style,
                        roundness,
                        feather,
                        midpoint: 30.0,
                        ..vig(-70.0)
                    },
                );
                for y in 0..h {
                    for x in 0..w {
                        let p = out.get(x, y);
                        assert_eq!(p, out.get(w - 1 - x, y));
                        assert_eq!(p, out.get(x, h - 1 - y));
                        assert_eq!(p, out.get(w - 1 - x, h - 1 - y));
                    }
                }
            }
        }
    }

    #[test]
    fn vignette_darkening_grows_monotonically_towards_the_corner() {
        let (w, h) = (120, 80);
        let out = vignetted(&flat(w, h, 0.5), &vig(-80.0));
        let mut last = f32::MAX;
        for i in 0..40 {
            // Along the diagonal from the centre to the top-left corner.
            let v = out.get(59 - (i * 59 / 39), 39 - (i * 39 / 39))[0];
            assert!(v <= last + 1e-6, "step {i}: {v} > {last}");
            last = v;
        }
    }

    #[test]
    fn larger_midpoint_confines_the_effect() {
        let (w, h) = (120, 80);
        let img = flat(w, h, 0.5);
        let darkening = |midpoint: f64| -> (f64, usize) {
            let out = vignetted(
                &img,
                &Vignette {
                    midpoint,
                    ..vig(-80.0)
                },
            );
            let lost: f64 = img
                .rgb
                .iter()
                .zip(&out.rgb)
                .map(|(a, b)| (a - b) as f64)
                .sum();
            let touched = img
                .rgb
                .chunks(3)
                .zip(out.rgb.chunks(3))
                .filter(|(a, b)| (a[0] - b[0]).abs() > 1e-4)
                .count();
            (lost, touched)
        };
        let (lost_lo, touched_lo) = darkening(10.0);
        let (lost_mid, touched_mid) = darkening(50.0);
        let (lost_hi, touched_hi) = darkening(90.0);
        assert!(
            lost_lo > lost_mid && lost_mid > lost_hi,
            "{lost_lo} {lost_mid} {lost_hi}"
        );
        assert!(touched_lo > touched_mid && touched_mid > touched_hi);
        // At Midpoint 0 the effect reaches far into the frame, but never the centre.
        let out = vignetted(
            &flat(121, 81, 0.5),
            &Vignette {
                midpoint: 0.0,
                feather: 100.0,
                ..vig(-100.0)
            },
        );
        assert_eq!(out.get(60, 40), [0.5, 0.5, 0.5]);
        assert!(out.get(75, 40)[0] < 0.5);
    }

    #[test]
    fn vignette_is_flat_at_the_centre_even_when_the_transition_starts_before_it() {
        // Midpoint 0 with Feather 100 starts the smoothstep before the centre.
        // The mirrored profile makes the darkening grow quadratically there
        // (doubling the distance quadruples it), not linearly like a cone.
        let mask = VignetteMask::new(
            2001,
            1335,
            &Vignette {
                midpoint: 0.0,
                feather: 100.0,
                ..vig(-100.0)
            },
            &k(),
        );
        assert!(mask.mirrored);
        let (cx, cy) = (1000, 667);
        assert_eq!(mask.at(cx, cy), 0.0);
        let (m1, m2) = (mask.at(cx + 20, cy), mask.at(cx + 40, cy));
        assert!(m1 > 0.0 && m2 / m1 > 3.5, "{m1} {m2}");
        // It still rises monotonically all the way to the corner, reaching ~1.
        let mut last = 0.0;
        for i in 0..=1000 {
            let m = mask.at(cx + i, cy + i * 667 / 1000);
            assert!(m >= last && m <= 1.0, "step {i}: {m} < {last}");
            last = m;
        }
        assert!(last > 0.95, "{last}");
    }

    #[test]
    fn feather_controls_edge_sharpness() {
        let (w, h) = (201, 135);
        let img = flat(w, h, 0.5);
        let max_step = |feather: f64| -> f32 {
            let out = vignetted(
                &img,
                &Vignette {
                    feather,
                    ..vig(-100.0)
                },
            );
            // Largest jump between neighbours along the horizontal centre line.
            (1..w)
                .map(|x| (out.get(x, h / 2)[0] - out.get(x - 1, h / 2)[0]).abs())
                .fold(0.0, f32::max)
        };
        let hard = max_step(0.0);
        let mid = max_step(50.0);
        let soft = max_step(100.0);
        assert!(hard > 0.1, "hard edge step {hard}");
        assert!(hard > 4.0 * mid && mid > soft, "{hard} {mid} {soft}");
    }

    #[test]
    fn highlight_contrast_protects_bright_corners() {
        let (w, h) = (90, 60);
        for style in [1, 2] {
            let corner = |value: f32, highlights: f64| -> f32 {
                let out = vignetted(
                    &flat(w, h, value),
                    &Vignette {
                        style,
                        highlights,
                        ..vig(-80.0)
                    },
                );
                out.get(0, 0)[1]
            };
            let bright_plain = corner(0.9, 0.0);
            let bright_protected = corner(0.9, 100.0);
            assert!(
                bright_protected > bright_plain + 0.2,
                "{bright_plain} {bright_protected}"
            );
            // The protection is mostly for bright areas: dark corners are darkened nearly as much.
            let dark_gain = corner(0.05, 100.0) / corner(0.05, 0.0);
            let bright_gain = bright_protected / bright_plain;
            assert!(bright_gain > dark_gain, "{bright_gain} {dark_gain}");
            // HighlightContrast does nothing for positive amounts.
            let pos = |hc: f64| {
                vignetted(
                    &flat(w, h, 0.3),
                    &Vignette {
                        style,
                        highlights: hc,
                        ..vig(60.0)
                    },
                )
                .rgb
            };
            assert_eq!(pos(0.0), pos(100.0));
        }
        // Paint Overlay has no highlight control.
        let paint = |hc: f64| {
            vignetted(
                &flat(w, h, 0.9),
                &Vignette {
                    style: 3,
                    highlights: hc,
                    ..vig(-80.0)
                },
            )
            .rgb
        };
        assert_eq!(paint(0.0), paint(100.0));
    }

    #[test]
    fn paint_overlay_keeps_negative_values_continuous() {
        // Out-of-gamut negative channels scale with the overlay instead of
        // snapping to 0 inside the mask.
        let img = Image::from_fn(80, 60, |_, _| [-0.02, 0.3, 0.5]);
        let out = vignetted(
            &img,
            &Vignette {
                style: 3,
                ..vig(-60.0)
            },
        );
        for p in out.rgb.chunks(3) {
            assert!(p[0] < 0.0 && p[0] >= -0.02, "{p:?}");
            assert!((p[0] / p[1] + 0.02 / 0.3).abs() < 1e-5, "{p:?}");
        }
    }

    #[test]
    fn roundness_moves_between_rectangle_ellipse_and_circle() {
        let (w, h) = (150, 100);
        let img = flat(w, h, 0.5);
        let out = |roundness: f64| {
            vignetted(
                &img,
                &Vignette {
                    roundness,
                    ..vig(-80.0)
                },
            )
        };
        let darkening = |o: &Image, x: usize, y: usize| 0.5 - o.get(x, y)[0];
        // Ellipse: the frame's own aspect, so the left and top edge midpoints match.
        let e = out(0.0);
        let (left, top) = (darkening(&e, 0, h / 2), darkening(&e, w / 2, 0));
        assert!((left - top).abs() < 0.01, "{left} {top}");
        // Circle: the long sides are further from the centre, so darker.
        let c = out(100.0);
        assert!(darkening(&c, 0, h / 2) > darkening(&c, w / 2, 0) + 0.03);
        // Rounded rectangle: edges are nearly as dark as the corners.
        let r = out(-100.0);
        let ratio = |o: &Image| darkening(o, w / 2, 0) / darkening(o, 0, 0);
        assert!(ratio(&r) > ratio(&e) + 0.1, "{} {}", ratio(&r), ratio(&e));
    }

    #[test]
    fn paint_overlay_reaches_black_and_white() {
        let (w, h) = (80, 60);
        let img = busy(w, h);
        let hard = |amount: f64| Vignette {
            style: 3,
            feather: 0.0,
            ..vig(amount)
        };
        let black = vignetted(&img, &hard(-100.0));
        let white = vignetted(&img, &hard(100.0));
        for c in black.get(0, 0) {
            assert!(c.abs() < 1e-6, "{c}");
        }
        for c in white.get(w - 1, h - 1) {
            assert!((c - 1.0).abs() < 1e-5, "{c}");
        }
    }

    #[test]
    fn color_priority_preserves_chromaticity() {
        let img = Image::from_fn(60, 40, |_, _| [0.5, 0.3, 0.1]);
        for amount in [-90.0, 70.0] {
            let out = vignetted(
                &img,
                &Vignette {
                    style: 2,
                    highlights: 50.0,
                    ..vig(amount)
                },
            );
            let p = out.get(0, 0);
            assert!((p[0] / p[1] - 5.0 / 3.0).abs() < 1e-4, "{p:?}");
            assert!((p[2] / p[1] - 1.0 / 3.0).abs() < 1e-4, "{p:?}");
            assert!(p[0] <= 1.0);
        }
    }

    #[test]
    fn vignette_is_resolution_independent() {
        let small = vignetted(&flat(60, 40, 0.5), &vig(-70.0));
        let large = vignetted(&flat(600, 400, 0.5), &vig(-70.0));
        for (fx, fy) in [(0.1, 0.1), (0.25, 0.5), (0.5, 0.2), (0.9, 0.8)] {
            let a = small.get((fx * 60.0) as usize, (fy * 40.0) as usize)[0];
            let b = large.get((fx * 600.0) as usize + 4, (fy * 400.0) as usize + 4)[0];
            assert!((a - b).abs() < 0.02, "({fx},{fy}) {a} vs {b}");
        }
    }

    #[test]
    fn vignette_stays_finite_on_edge_cases() {
        let cases = [
            flat(1, 1, 0.5),
            flat(2, 3, 0.5),
            flat(3, 2, 1.0),
            flat(17, 9, 0.0),
            flat(16, 16, 1.0),
            Image::from_fn(13, 7, |x, y| {
                [(x as f32 - 3.0) * 0.3, y as f32 * 0.4, -0.05]
            }),
        ];
        for img in &cases {
            for style in 1..=3 {
                for amount in [-100.0, -1.0, 1.0, 100.0] {
                    for (roundness, feather, midpoint) in
                        [(-100.0, 0.0, 0.0), (0.0, 50.0, 50.0), (100.0, 100.0, 100.0)]
                    {
                        let v = Vignette {
                            amount,
                            roundness,
                            feather,
                            midpoint,
                            highlights: 100.0,
                            style,
                        };
                        let out = vignetted(img, &v);
                        assert!(all_finite(&out), "{v:?} {}x{}", img.width, img.height);
                        assert_eq!(out.rgb, vignetted(img, &v).rgb, "deterministic");
                    }
                }
            }
        }
        // A 1x1 image is all centre: untouched.
        assert_eq!(vignetted(&cases[0], &vig(-100.0)).rgb, cases[0].rgb);
    }

    #[test]
    fn protected_exposure_is_monotonic_and_continuous() {
        for &(g, h) in &[
            (0.2f32, 0.0f32),
            (0.2, 1.0),
            (0.5, 0.3),
            (4.0, 1.0),
            (1.0, 0.7),
        ] {
            let mut last = f32::MIN;
            for i in -50..=150 {
                let v = i as f32 / 100.0;
                let y = protected_exposure(v, g, h);
                assert!(y.is_finite() && y >= last, "g {g} h {h} v {v}");
                last = y;
            }
            assert!((protected_exposure(0.0, g, h)).abs() < 1e-7);
        }
        assert_eq!(protected_exposure(0.37, 1.0, 0.5), 0.37);
        assert!((protected_exposure(1.0, 0.25, 1.0) - 1.0).abs() < 1e-6);
        assert!((protected_exposure(0.5, 0.25, 0.0) - 0.125).abs() < 1e-7);
    }

    #[test]
    fn priority_styles_move_values_above_one_the_right_way() {
        // Channels above 1 reach this stage (no clipping before effects): a darkening
        // vignette must never raise them and a lightening one never lower them.
        for &h in &[0.0f32, 0.3, 0.7, 1.0] {
            for &(g, darken) in &[(0.177f32, true), (0.5, true), (2.0, false), (4.0, false)] {
                for i in -50..=300 {
                    let v = i as f32 / 100.0;
                    let y = protected_exposure(v, g, h);
                    if v >= 0.0 && darken {
                        assert!(y <= v + 1e-6, "g {g} h {h}: {v} -> {y}");
                    } else if v >= 0.0 {
                        assert!(y >= v - 1e-6, "g {g} h {h}: {v} -> {y}");
                    } else {
                        // Negative (out-of-gamut) values scale like a gain.
                        assert!((y - v * g).abs() < 1e-6, "g {g} h {h}: {v} -> {y}");
                    }
                }
            }
        }
        let img = Image::from_fn(40, 30, |_, _| [1.2, 0.3, 0.1]);
        for style in [1, 2] {
            for (amount, darker) in [(-100.0, true), (100.0, false)] {
                let out = vignetted(
                    &img,
                    &Vignette {
                        style,
                        feather: 0.0,
                        highlights: 100.0,
                        ..vig(amount)
                    },
                );
                let p = out.get(0, 0);
                if darker {
                    assert!(
                        p[0] <= 1.2 && p[1] <= 0.3 && p[2] <= 0.1,
                        "style {style}: {p:?}"
                    );
                } else {
                    assert!(
                        p[0] >= 1.2 && p[1] >= 0.3 && p[2] >= 0.1,
                        "style {style}: {p:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn nan_highlight_contrast_stays_finite() {
        let img = flat(30, 20, 0.6);
        for style in [1, 2] {
            let out = vignetted(
                &img,
                &Vignette {
                    style,
                    highlights: f64::NAN,
                    ..vig(-80.0)
                },
            );
            assert!(all_finite(&out), "style {style}");
            assert!(out.get(0, 0)[0] < 0.6);
        }
    }

    #[test]
    fn hard_edge_stays_inside_the_frame_at_every_midpoint() {
        // Amount -100 always darkens the corner, whatever Midpoint, Feather and
        // Roundness: Feather only softens the edge, it never switches it on.
        for (w, h) in [(600, 400), (6000, 4000), (31, 17)] {
            for midpoint in [0.0, 50.0, 100.0] {
                for feather in [0.0, 1.0, 10.0, 100.0] {
                    for roundness in [-100.0, 0.0, 100.0] {
                        let v = Vignette {
                            midpoint,
                            feather,
                            roundness,
                            ..vig(-100.0)
                        };
                        let m = VignetteMask::new(w, h, &v, &k()).at(0, 0);
                        assert!(m > 0.5, "{w}x{h} {v:?}: corner mask {m}");
                        if feather == 0.0 {
                            assert!(m > 0.99, "{w}x{h} {v:?}: corner mask {m}");
                        }
                    }
                }
            }
        }
    }

    // -------------------------------------------------------------- grain

    #[test]
    fn grain_amount_zero_is_exact_noop() {
        let img = busy(37, 23);
        assert_eq!(grained(&img, &grain(0.0)).rgb, img.rgb);
    }

    #[test]
    fn grain_is_deterministic_and_seeded() {
        let img = flat(64, 48, 0.3);
        let a = grained(&img, &grain(60.0));
        let b = grained(&img, &grain(60.0));
        assert_eq!(a.rgb, b.rgb);
        let c = grained(
            &img,
            &Grain {
                seed: 8,
                ..grain(60.0)
            },
        );
        let differing = a.rgb.iter().zip(&c.rgb).filter(|(x, y)| x != y).count();
        assert!(differing > a.rgb.len() * 9 / 10, "{differing}");
    }

    #[test]
    fn grain_preserves_mean_brightness() {
        let ramp = Image::from_fn(400, 300, |x, _| {
            let v = x as f32 / 399.0;
            [v, v * 0.8, v * 0.6]
        });
        for img in [flat(300, 200, 0.18), flat(300, 200, 0.6), ramp] {
            for (size, roughness) in [(25.0, 50.0), (0.0, 0.0), (100.0, 100.0)] {
                let out = grained(
                    &img,
                    &Grain {
                        size,
                        roughness,
                        ..grain(100.0)
                    },
                );
                let (m0, m1) = (mean(&img.rgb), mean(&out.rgb));
                assert!(((m1 - m0) / m0).abs() < 0.005, "{m0} -> {m1}");
            }
        }
    }

    #[test]
    fn grain_deviation_grows_with_amount() {
        let img = flat(200, 150, 0.2);
        let devs: Vec<f64> = [10.0, 25.0, 50.0, 100.0]
            .iter()
            .map(|&a| std(&grained(&img, &grain(a)).rgb))
            .collect();
        assert!(devs[0] > 0.0);
        for pair in devs.windows(2) {
            assert!(pair[1] > pair[0] * 1.5, "{devs:?}");
        }
    }

    /// Neighbour-difference energy relative to the total variance (2 for white noise).
    fn high_frequency_ratio(v: &[f32], w: usize) -> f64 {
        let m = mean(v);
        let var: f64 = v.iter().map(|&x| (x as f64 - m).powi(2)).sum::<f64>() / v.len() as f64;
        let mut diff = 0.0;
        let mut count = 0;
        for row in v.chunks(w) {
            for x in 1..w {
                diff += ((row[x] - row[x - 1]) as f64).powi(2);
                count += 1;
            }
        }
        diff / count as f64 / var
    }

    #[test]
    fn larger_grain_has_less_high_frequency_energy() {
        // Through the public path: 1200 x 800 is 0.2x the reference's linear size.
        let (w, h) = (1200, 800);
        let img = flat(w, h, 0.18);
        let hf = |size: f64| {
            let out = grained(
                &img,
                &Grain {
                    size,
                    ..grain(50.0)
                },
            );
            high_frequency_ratio(&out.luminance(), w)
        };
        let (small, large) = (hf(0.0), hf(100.0));
        assert!(large < 0.7 * small, "{small} {large}");
        // On the field itself, at explicit sigmas.
        let field = |sigma| high_frequency_ratio(&grain_field(300, 200, sigma, 0.5, 3), 300);
        let (f1, f2, f4) = (field(1.0), field(2.0), field(4.0));
        assert!(f1 > f2 && f2 > f4, "{f1} {f2} {f4}");
    }

    #[test]
    fn roughness_adds_coarse_structure() {
        // Low-frequency share: variance left after a wide blur of the unit-variance field.
        let coarse_share = |r: f32| {
            let f = grain_field(256, 256, 1.5, r, 11);
            let blurred = gaussian_blur(&f, 256, 256, 6.0);
            std(&blurred)
        };
        assert!(coarse_share(1.0) > 2.0 * coarse_share(0.0));
    }

    #[test]
    fn grain_field_is_normalised() {
        // Normalised by its expected variance, so the sample statistics match up
        // to sampling error, across both blur paths (exact kernel and box passes).
        for (sigma, r) in [(0.0, 0.5), (0.6, 0.0), (1.2, 0.5), (2.0, 1.0), (4.5, 0.3)] {
            let f = grain_field(600, 400, sigma, r, 42);
            assert!(
                mean(&f).abs() < 0.03,
                "sigma {sigma} r {r}: mean {}",
                mean(&f)
            );
            assert!(
                (std(&f) - 1.0).abs() < 0.03,
                "sigma {sigma} r {r}: std {}",
                std(&f)
            );
        }
        // The noise itself has mean exactly 0 and the documented variance.
        let key = mix64(5);
        let white: Vec<f32> = (0..200_000)
            .map(|i| hash_noise(i % 500, i / 500, key))
            .collect();
        assert!(mean(&white).abs() < 0.005);
        assert!((std(&white).powi(2) - WHITE_VARIANCE).abs() < 0.005);
        assert!(white.iter().all(|&v| v > -1.0 && v < 1.0));
        assert_eq!(hash_noise(-3, -7, key), hash_noise(-3, -7, key));
        assert_ne!(hash_noise(-3, 7, key), hash_noise(3, 7, key));
    }

    #[test]
    fn grain_tiles_join_seamlessly() {
        // The field depends only on (x, y, seed): any tiling gives the same values.
        // Every value is summed in the same order, so the tiles are bit-identical.
        for (sigma, scale, r) in [
            (1.0, 1.0, 0.5),
            (0.0, 1.0, 0.0),
            (3.5, 1.0, 0.0),
            (2.0, 0.3, 1.0),
        ] {
            let field = GrainField::new(sigma, scale, r, 9, &k());
            let (w, h) = (90, 70);
            let whole = field.tile(5, 11, w, h);
            let (sx, sy) = (37, 23);
            for (x0, y0, tw, th) in [(0, 0, sx, sy), (sx, 0, w - sx, sy), (0, sy, sx, h - sy)] {
                let part = field.tile(5 + x0, 11 + y0, tw, th);
                for j in 0..th {
                    for i in 0..tw {
                        let (a, b) = (part[j * tw + i], whole[(y0 + j) * w + x0 + i]);
                        assert_eq!(a, b, "sigma {sigma} ({i},{j})");
                    }
                }
            }
        }
    }

    #[test]
    fn grain_has_no_edge_streaks() {
        // Edge clamping would inflate the deviation along the borders; the
        // procedural margin keeps the border statistics the same as inside.
        let (w, h) = (400, 300);
        for sigma in [1.0, 4.0] {
            let f = grain_field(w, h, sigma, 0.0, 17);
            let border: Vec<f32> = (0..h).flat_map(|y| [f[y * w], f[y * w + w - 1]]).collect();
            let inner: Vec<f32> = (0..h).map(|y| f[y * w + w / 2]).collect();
            let ratio = std(&border) / std(&inner);
            assert!((ratio - 1.0).abs() < 0.25, "sigma {sigma}: {ratio}");
        }
    }

    #[test]
    fn grain_blur_matches_the_reference_gaussian() {
        // The tile-local blur is the same Gaussian as `filters::gaussian_blur`
        // (exact-kernel path) away from the clamped frame edges.
        for (sigma, r) in [(0.8f32, 0.0f32), (1.2, 0.5), (2.5, 1.0)] {
            let field = GrainField::new(sigma, 1.0, r, 21, &k());
            let (w, h, m) = (60usize, 50usize, field.margin);
            let (pw, ph) = (w + 2 * m, h + 2 * m);
            let white: Vec<f32> = (0..pw * ph)
                .map(|i| {
                    hash_noise(
                        (i % pw) as i64 - m as i64,
                        (i / pw) as i64 - m as i64,
                        field.seed_key,
                    )
                })
                .collect();
            let fine = gaussian_blur(&white, pw, ph, sigma);
            let coarse = gaussian_blur(&white, pw, ph, (sigma * 2.5).max(1.0));
            let tile = field.tile(0, 0, w, h);
            for y in 0..h {
                for x in 0..w {
                    let i = (y + m) * pw + x + m;
                    let expected = (fine[i] - (1.0 - r) * coarse[i]) * field.inv_std;
                    let got = tile[y * w + x];
                    assert!(
                        (got - expected).abs() < 1e-4,
                        "sigma {sigma} ({x},{y}): {got} {expected}"
                    );
                }
            }
        }
    }

    /// Box average of a `w × h` plane over `f × f` blocks.
    fn box_down(v: &[f32], w: usize, h: usize, f: usize) -> Vec<f32> {
        let (dw, dh) = (w / f, h / f);
        let mut out = vec![0.0f32; dw * dh];
        for y in 0..dh * f {
            for x in 0..dw * f {
                out[(y / f) * dw + x / f] += v[y * w + x] / (f * f) as f32;
            }
        }
        out
    }

    #[test]
    fn grain_strength_is_resolution_independent() {
        // A render at a reduced size has the grain of the reference render box-averaged
        // to that size (what a viewer sees when the full render is scaled down), and a
        // render above the reference averages down to the reference's unit deviation.
        let k = k();
        for size in [0.0, 25.0, 100.0] {
            let sigma_ref = grain_sigma_ref(size, &k);
            for r in [0.0f32, 0.5, 1.0] {
                let reference = GrainField::new(sigma_ref, 1.0, r, 5, &k).tile(0, 0, 1024, 1024);
                assert!((std(&reference) - 1.0).abs() < 0.05, "size {size} r {r}");
                for f in [2usize, 4] {
                    let averaged = std(&box_down(&reference, 1024, 1024, f));
                    let field = GrainField::new(sigma_ref, 1.0 / f as f32, r, 6, &k);
                    let n = 1024 / f;
                    let direct = std(&field.tile(0, 0, n, n));
                    assert!(
                        (direct / averaged - 1.0).abs() < 0.1,
                        "size {size} r {r} 1/{f}: direct {direct} vs averaged reference {averaged}"
                    );
                    // The field's own variance is the one the grain op is told about.
                    assert!((direct / (field.variance as f64).sqrt() - 1.0).abs() < 0.06);
                }
                let big = GrainField::new(sigma_ref, 2.0, r, 7, &k).tile(0, 0, 1024, 1024);
                let down = std(&box_down(&big, 1024, 1024, 2));
                assert!((down - 1.0).abs() < 0.1, "size {size} r {r} x2: {down}");
            }
        }
        // Through the public path, with grain coarse enough (10 px at the reference)
        // that the larger test render still resolves it: the small render matches
        // the larger one averaged down to the same size.
        let coarse = EffectsStrengths {
            grain_sigma_min_px: 10.0,
            ..k
        };
        let g = Grain {
            size: 0.0,
            ..grain(100.0)
        };
        let render = |w, h| {
            let mut img = flat(w, h, 0.18);
            apply_grain(&mut img, &g, &coarse);
            img.luminance()
        };
        let (large, small) = (render(1200, 800), render(300, 200));
        let (a, b) = (std(&box_down(&large, 1200, 800, 4)), std(&small));
        assert!((b / a - 1.0).abs() < 0.15, "{a} {b}");
    }

    #[test]
    fn grain_is_monochrome_and_strongest_in_midtones() {
        let coloured = Image::from_fn(120, 80, |_, _| [0.4, 0.2, 0.1]);
        let out = grained(&coloured, &grain(100.0));
        for p in out.rgb.chunks(3) {
            assert!((p[0] / p[1] - 2.0).abs() < 1e-4 && (p[2] / p[1] - 0.5).abs() < 1e-4);
        }
        // Relative (log) deviation, which is what the eye sees in each tone.
        let rel_dev = |v: f32| {
            let out = grained(&flat(120, 80, v), &grain(100.0));
            std(&out.rgb) / v as f64
        };
        let (dark, mid, bright) = (rel_dev(0.004), rel_dev(0.18), rel_dev(0.97));
        assert!(
            mid > 2.0 * dark && mid > 2.0 * bright,
            "{dark} {mid} {bright}"
        );
    }

    #[test]
    fn grain_size_scales_with_resolution() {
        let k = k();
        assert_eq!(grain_scale(6000, 4000), 1.0);
        let at_ref = grain_sigma_px(25.0, 6000, 4000, &k);
        let at_half = grain_sigma_px(25.0, 3000, 2000, &k);
        assert!((at_ref / at_half - 2.0).abs() < 1e-5);
        assert!(grain_sigma_px(100.0, 6000, 4000, &k) > grain_sigma_px(25.0, 6000, 4000, &k));
    }

    #[test]
    fn grain_stays_finite_on_edge_cases() {
        let cases = [
            flat(1, 1, 0.5),
            flat(2, 3, 0.5),
            flat(3, 2, 0.0),
            flat(31, 17, 0.0),
            flat(31, 17, 1.0),
            Image::from_fn(9, 5, |x, y| [(x as f32 - 3.0) * 0.4, y as f32 * 0.5, -0.02]),
        ];
        for img in &cases {
            for (size, roughness) in [(0.0, 0.0), (25.0, 50.0), (100.0, 100.0)] {
                let g = Grain {
                    size,
                    roughness,
                    ..grain(100.0)
                };
                let out = grained(img, &g);
                assert!(all_finite(&out), "{g:?} {}x{}", img.width, img.height);
                assert_eq!(out.rgb, grained(img, &g).rgb, "deterministic");
            }
        }
        // Black stays black and pure white stays white.
        assert!(
            grained(&cases[3], &grain(100.0))
                .rgb
                .iter()
                .all(|&v| v == 0.0)
        );
        assert!(
            grained(&cases[4], &grain(100.0))
                .rgb
                .iter()
                .all(|&v| v == 1.0)
        );
    }

    #[test]
    fn strengths_round_trip_through_json() {
        let k = k();
        let json = serde_json::to_string(&k).unwrap();
        assert_eq!(serde_json::from_str::<EffectsStrengths>(&json).unwrap(), k);
        let partial: EffectsStrengths = serde_json::from_str(r#"{"grain_strength": 0.5}"#).unwrap();
        assert_eq!(partial.grain_strength, 0.5);
        assert_eq!(partial.vignette_dark_stops, k.vignette_dark_stops);
    }

    /// Timing on a 24 MP frame: `cargo test --release -p amb-engine -- --ignored --nocapture effects`.
    #[test]
    #[ignore]
    fn effects_timing_24mp() {
        let mut img = busy(6000, 4000);
        for style in 1..=3 {
            let t = std::time::Instant::now();
            apply_vignette(
                &mut img,
                &Vignette {
                    style,
                    roundness: if style == 2 { -50.0 } else { 0.0 },
                    ..vig(-50.0)
                },
                &k(),
            );
            println!(
                "vignette style {style}: {:.0} ms",
                t.elapsed().as_secs_f64() * 1e3
            );
        }
        for size in [25.0, 100.0] {
            let t = std::time::Instant::now();
            apply_grain(
                &mut img,
                &Grain {
                    size,
                    ..grain(50.0)
                },
                &k(),
            );
            println!(
                "grain size {size}: {:.0} ms",
                t.elapsed().as_secs_f64() * 1e3
            );
        }
        assert!(all_finite(&img));
    }
}
