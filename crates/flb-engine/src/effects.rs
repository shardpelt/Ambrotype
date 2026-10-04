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
//! (smoothstep; Feather 0 is a hard edge, antialiased over about a pixel), and
//! the mask is renormalised so it is exactly 0 at the centre. Everything is in
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
//!   in a gamma-encoded space; HighlightContrast does not apply.
//!
//! *Grain.* Deterministic white noise from an integer hash of `(x, y, seed)` is
//! blurred at two scales: `fine` at a sigma set by GrainSize (defined at a
//! 24 MP / 6000 × 4000 reference and scaled with the image's linear size) and
//! `coarse` at a multiple of it. Roughness mixes them as `fine − (1 − r)·coarse`:
//! Roughness 0 is a band-pass (regular grains of one size), 100 keeps the
//! coarse octave (clumpy, uneven grain). The field is normalised to zero mean
//! and unit deviation, then applied as a monochrome luminance ratio
//! `exp(a·n − a²/2)` (mean-preserving, never negative), with `a` weighted by
//! `4·L·(1 − L)` on CIE lightness so grain is strongest in the midtones and
//! fades towards black and white. Colours do not shift.

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::buffer::{Image, luminance_weights};
use crate::filters::gaussian_blur;

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
    /// Centre of the vignette transition at Midpoint 100.
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
            vignette_midpoint_max: 1.0,
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
    /// Smoothstep value at the centre (non-zero only when the transition starts before it).
    s0: f32,
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

        let mid = k.vignette_midpoint_min
            + (k.vignette_midpoint_max - k.vignette_midpoint_min)
                * (v.midpoint / 100.0).clamp(0.0, 1.0);
        // Half-width of the transition; at least half a pixel (along the diagonal).
        let half_diag_px = 0.5 * (wf * wf + hf * hf).sqrt();
        let half = (0.5 * k.vignette_feather_width.max(0.0) * (v.feather / 100.0).clamp(0.0, 1.0))
            .max(0.5 / half_diag_px);
        let inner = (mid - half) as f32;
        let outer = (mid + half) as f32;
        let s0 = smoothstep(inner, outer, 0.0);
        let q_skip = if s0 == 0.0 {
            (inner.max(0.0) as f64).powf(p) as f32
        } else {
            -1.0
        };
        VignetteMask {
            col,
            row,
            p: p as f32,
            inner,
            outer,
            s0,
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
        let s = smoothstep(self.inner, self.outer, d);
        ((s - self.s0) / (1.0 - self.s0)).max(0.0)
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
            let hc = (v.highlights / 100.0).clamp(0.0, 1.0);
            let h = base + (k.vignette_highlight_contrast_protect - base) * hc;
            (amount * k.vignette_dark_stops, h)
        } else {
            (amount * k.vignette_light_stops, k.vignette_lighten_shoulder)
        };
        VignetteOp {
            style,
            stops: stops as f32,
            protect: protect.clamp(0.0, 1.0) as f32,
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
                let inv = 1.0 / self.gamma;
                c.map(|v| {
                    let e = v.max(0.0).powf(inv);
                    (e + (self.target - e) * a).powf(self.gamma)
                })
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

/// Exposure gain `g` with highlight protection `h` in 0..1:
/// `v·g / (1 + (g − 1)·h·v)` on 0..1 (slope `g` at black; white pinned for `h = 1`),
/// continued linearly outside 0..1. Identity for `g = 1`; monotonic.
#[inline]
fn protected_exposure(v: f32, g: f32, h: f32) -> f32 {
    let c = v.clamp(0.0, 1.0);
    // g > 0 and h <= 1 keep the denominator >= min(g, 1) > 0.
    let den = 1.0 + (g - 1.0) * h * c;
    let out = c * g / den;
    if v < 0.0 {
        out + v * g
    } else if v > 1.0 {
        out + (v - 1.0) * g / (den * den)
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

/// Uniform noise in [-1, 1) for pixel (x, y); `seed_key` is `mix64` of the seed.
#[inline]
fn hash_noise(x: usize, y: usize, seed_key: u64) -> f32 {
    let key = ((y as u64) << 32) | (x as u64 & 0xFFFF_FFFF);
    let z = mix64(
        key.wrapping_mul(0x9E37_79B9_7F4A_7C15)
            .wrapping_add(seed_key),
    );
    // Top 24 bits → [0, 1) exactly representable in f32.
    (z >> 40) as f32 * (2.0 / (1u32 << 24) as f32) - 1.0
}

/// Grain blur sigma in pixels: GrainSize mapped at the 24 MP reference, scaled
/// by the image's linear size relative to it.
fn grain_sigma_px(size: f64, w: usize, h: usize, k: &EffectsStrengths) -> f32 {
    let t = (size / 100.0).clamp(0.0, 1.0);
    let sigma_ref = k.grain_sigma_min_px
        + (k.grain_sigma_max_px - k.grain_sigma_min_px) * t.powf(k.grain_size_exponent);
    let scale = ((w * h) as f64 / REFERENCE_PIXELS).sqrt();
    (sigma_ref.max(0.0) * scale) as f32
}

/// Normalise to zero mean and unit deviation (deterministic: per-row f64 sums,
/// added in row order). A constant field becomes all zeros.
fn normalise(n: &mut [f32], w: usize) {
    let rows: Vec<(f64, f64)> = n
        .par_chunks(w)
        .map(|row| {
            row.iter().fold((0.0, 0.0), |(s, s2), &v| {
                let v = v as f64;
                (s + v, s2 + v * v)
            })
        })
        .collect();
    let (s, s2) = rows
        .iter()
        .fold((0.0, 0.0), |(a, b), &(s, s2)| (a + s, b + s2));
    let count = n.len() as f64;
    let mean = s / count;
    let var = s2 / count - mean * mean;
    if var.is_nan() || var <= 1e-12 {
        n.iter_mut().for_each(|v| *v = 0.0);
        return;
    }
    let (mean, inv) = (mean as f32, (1.0 / var.sqrt()) as f32);
    n.par_iter_mut().for_each(|v| *v = (*v - mean) * inv);
}

/// Zero-mean, unit-deviation grain field (`w * h` values) at blur sigma `sigma`
/// pixels, roughness `r` in 0..1.
fn grain_field(
    w: usize,
    h: usize,
    sigma: f32,
    r: f32,
    seed: u64,
    k: &EffectsStrengths,
) -> Vec<f32> {
    let seed_key = mix64(seed ^ 0x6772_6169_6E5F_7631); // "grain_v1"
    let white: Vec<f32> = (0..w * h)
        .into_par_iter()
        .map(|i| hash_noise(i % w, i / w, seed_key))
        .collect();
    // The coarse octave is at least a pixel wide so the band-pass (Roughness 0)
    // never collapses to nothing on small images.
    let coarse_sigma = (sigma * k.grain_octave_ratio.max(1.0) as f32).max(1.0);
    let coarse = gaussian_blur(&white, w, h, coarse_sigma);
    let mut field = gaussian_blur(&white, w, h, sigma);
    drop(white);
    let c = 1.0 - r.clamp(0.0, 1.0);
    field
        .par_iter_mut()
        .zip(coarse.par_iter())
        .for_each(|(f, &q)| *f -= c * q);
    normalise(&mut field, w);
    field
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

/// Film-like grain (see the module docs for the model). No-op at amount 0.
pub fn apply_grain(img: &mut Image, p: &Grain, k: &EffectsStrengths) {
    let (w, h) = (img.width, img.height);
    let amount = (p.amount / 100.0).clamp(0.0, 1.0);
    if amount == 0.0 || amount.is_nan() || w == 0 || h == 0 {
        return;
    }
    let sigma = grain_sigma_px(p.size, w, h, k);
    let field = grain_field(w, h, sigma, (p.roughness / 100.0) as f32, p.seed, k);
    let strength = (amount * k.grain_strength.max(0.0)) as f32;
    let tone_power = k.grain_tone_power.max(0.0) as f32;
    let lw = luminance_weights();
    img.rgb
        .par_chunks_mut(3)
        .zip(field.par_iter())
        .for_each(|(px, &n)| {
            let y = lw[0] * px[0] + lw[1] * px[1] + lw[2] * px[2];
            if y.is_nan() || y <= 0.0 {
                return;
            }
            let l = lightness(y.min(1.0));
            let mut weight = (4.0 * l * (1.0 - l)).clamp(0.0, 1.0);
            if tone_power != 1.0 {
                weight = weight.powf(tone_power);
            }
            let a = strength * weight;
            // Log-normal ratio: E[ratio] = 1 for unit-variance n, and never negative.
            let ratio = (a * n - 0.5 * a * a).exp();
            px.iter_mut().for_each(|v| *v *= ratio);
        });
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let field = |sigma| high_frequency_ratio(&grain_field(300, 200, sigma, 0.5, 3, &k()), 300);
        let (f1, f2, f4) = (field(1.0), field(2.0), field(4.0));
        assert!(f1 > f2 && f2 > f4, "{f1} {f2} {f4}");
    }

    #[test]
    fn roughness_adds_coarse_structure() {
        // Low-frequency share: variance left after a wide blur of the unit-variance field.
        let coarse_share = |r: f32| {
            let f = grain_field(256, 256, 1.5, r, 11, &k());
            let blurred = gaussian_blur(&f, 256, 256, 6.0);
            std(&blurred)
        };
        assert!(coarse_share(1.0) > 2.0 * coarse_share(0.0));
    }

    #[test]
    fn grain_field_is_normalised() {
        let f = grain_field(200, 100, 1.2, 0.5, 42, &k());
        assert!(mean(&f).abs() < 1e-5);
        assert!((std(&f) - 1.0).abs() < 1e-3);
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

    /// Timing on a 24 MP frame: `cargo test --release -p flb-engine -- --ignored --nocapture effects`.
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
