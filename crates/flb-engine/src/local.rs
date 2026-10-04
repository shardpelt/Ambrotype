//! Texture and Clarity: local contrast at two scales.
//!
//! **Model v1 (ours, not Adobe's).** Camera Raw's Clarity adds "punch" through
//! large-scale local contrast, mainly in the midtones (negative values give a
//! soft glow); Texture enhances or smooths medium-sized detail (skin, bark,
//! foliage) while leaving the finest detail and noise mostly alone. Adobe's
//! implementation is closed (likely Local-Laplacian-style, docs/research/05 §5);
//! this version is a guided-filter base/detail decomposition whose constants
//! live in [`LocalStrengths`], to be fitted against Lightroom sweeps
//! (docs/lightroom-trial-plan.md).
//!
//! Everything runs on perceptually encoded luminance `L = sRGB-encode(Y)`, with
//! `Y` the ProPhoto luminance of the display-linear image:
//!
//! - **Clarity.** `Lc` = `L` pre-blurred at a fine scale (so pixel noise is not
//!   amplified); `base` = self-guided filter of `Lc` with a radius of
//!   [`LocalStrengths::clarity_radius_frac`] of the image diagonal and an `eps`
//!   that keeps strong edges in the base (no halos there); `detail = Lc - base`.
//!   Positive Clarity adds `gain · detail · w(base)`, `w` a midtone weight that
//!   tapers to 0 at black and white; negative Clarity blends towards the base.
//! - **Texture.** A band-pass `band = Lt - base_t`: `Lt` is `L` blurred at a
//!   fine scale (~1.2 px at 24 MP, leaving noise alone) and `base_t` a self-guided
//!   filter of `Lt` at a medium radius (~10 px at 24 MP), so strong edges stay in
//!   the base. Large band values are soft-limited, so the control targets
//!   low-amplitude detail. Positive Texture adds `gain · band`, negative removes
//!   up to all of it (smoothing that keeps large edges and fine grain).
//!
//! The summed change is soft-limited to the headroom left below white and above
//! black (no clipping, no crushed blacks), then applied to RGB as a luminance
//! ratio, which preserves hue and saturation. Spatial scales are resolution
//! independent: they refer to a 24 MP (6000 × 4000) frame and scale with the
//! image diagonal, so a preview and the full-size export look the same.
//!
//! Interface contract (do not change without updating pipeline.rs):
//! - runs on the display-linear ProPhoto image, after the base look and the
//!   Basic tone sliders, before colour;
//! - `texture` and `clarity` are Camera Raw slider values in -100..100;
//! - must be a no-op when both are 0, must not shift global brightness or hue,
//!   and must keep values finite (clamping to 0..1 is the caller's job).

use std::borrow::Cow;

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::buffer::{Image, luminance_weights};
use crate::filters::{box_blur, gaussian_blur};

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct LocalContrast {
    pub texture: f64,
    pub clarity: f64,
}

impl LocalContrast {
    pub fn is_neutral(&self) -> bool {
        self.texture == 0.0 && self.clarity == 0.0
    }
}

/// Tunable constants. Starting guesses, to be fitted against Lightroom sweeps.
/// Lengths in `_px` are pixels of a 24 MP (6000 × 4000) frame and scale with
/// the image diagonal. Luminance quantities are in encoded (sRGB-transfer) units.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LocalStrengths {
    /// Clarity scale: guided-filter box radius as a fraction of the image
    /// diagonal (0.015 ≈ 108 px at 24 MP).
    pub clarity_radius_frac: f32,
    /// Clarity edge threshold: guided-filter regularisation (encoded L squared).
    /// Local structure with a standard deviation well below `sqrt(eps)` counts
    /// as detail and is boosted; stronger edges stay in the base (no halos).
    pub clarity_edge_eps: f32,
    /// Detail finer than this Gaussian sigma (px at 24 MP) is left to Texture and
    /// Sharpening, so Clarity does not amplify pixel noise. 0 = use `L` itself.
    pub clarity_fine_sigma_px: f32,
    /// Detail gain at Clarity +100: `L += gain · w · detail`.
    pub clarity_gain: f32,
    /// Fraction of the way from the image to the base at Clarity -100 (capped at 1).
    pub clarity_soften: f32,
    /// Soft amplitude limit of the detail (encoded L): `detail → c · sat(detail / c)`
    /// with `sat(x) = x / sqrt(1 + x²)`.
    /// Like the range parameter of a local Laplacian filter: small local
    /// contrast is scaled, large steps (hard edges) much less, which keeps the
    /// rim next to a hard edge small. 0 = off.
    pub clarity_detail_limit: f32,
    /// Midtone weight exponent `p` in `w(b) = (4 b (1 - b))^p`, evaluated at the
    /// local base level `b`. 0 = no tonal weighting; larger = more midtone-only.
    pub clarity_midtone_power: f32,
    /// Texture's lower cutoff: Gaussian sigma (px at 24 MP) removing the finest
    /// detail and noise from the band.
    pub texture_fine_sigma_px: f32,
    /// Texture's upper cutoff: guided-filter box radius (px at 24 MP).
    pub texture_medium_radius_px: f32,
    /// Texture edge threshold: guided-filter regularisation (encoded L squared).
    pub texture_edge_eps: f32,
    /// Band gain at Texture +100: `L += gain · band`.
    pub texture_gain: f32,
    /// Fraction of the band removed at Texture -100 (capped at 1).
    pub texture_smooth: f32,
    /// Soft amplitude limit of the band (encoded L): `band → c · sat(band / c)`,
    /// so Texture acts on low-amplitude detail rather than strong edges. 0 = off.
    pub texture_band_limit: f32,
    /// Midtone weight exponent for Texture (milder than Clarity's).
    pub texture_midtone_power: f32,
}

impl Default for LocalStrengths {
    fn default() -> Self {
        DEFAULT_STRENGTHS
    }
}

pub const DEFAULT_STRENGTHS: LocalStrengths = LocalStrengths {
    clarity_radius_frac: 0.015,
    clarity_edge_eps: 0.01,
    clarity_fine_sigma_px: 1.2,
    clarity_gain: 1.0,
    clarity_soften: 0.8,
    clarity_detail_limit: 0.05,
    clarity_midtone_power: 0.75,
    texture_fine_sigma_px: 1.2,
    texture_medium_radius_px: 10.0,
    texture_edge_eps: 0.004,
    texture_gain: 1.2,
    texture_smooth: 0.85,
    texture_band_limit: 0.08,
    texture_midtone_power: 0.3,
};

/// Diagonal of the 24 MP (6000 × 4000) reference frame.
const REFERENCE_DIAGONAL: f32 = 7211.1025;

/// Below this display-linear luminance the change fades from an RGB ratio
/// (hue preserving) to a neutral offset, so near-black noise is not amplified
/// into strong colour.
const RATIO_KNEE_Y: f32 = 1e-3;

/// The guided filters run on copies subsampled so that their box radius is
/// about this many low-resolution pixels (He & Sun's fast guided filter). More
/// = closer to the exact filter (less rim at hard edges), slower.
const CLARITY_LOW_RES_RADIUS: f32 = 10.0;
const TEXTURE_LOW_RES_RADIUS: f32 = 5.0;
/// Upper bound on that subsampling factor.
const GUIDED_MAX_SUBSAMPLE: usize = 8;

const WEIGHT_LUT_SIZE: usize = 1025;
/// sRGB decode table on 0..1 (linear interpolation error < 1e-8).
const DECODE_LUT_SIZE: usize = 8193;

/// Image-size scale relative to the 24 MP reference (by diagonal).
pub fn reference_scale(width: usize, height: usize) -> f32 {
    (width as f32).hypot(height as f32) / REFERENCE_DIAGONAL
}

/// Apply Texture and Clarity in place, with spatial scales relative to this
/// image's size.
pub fn apply(img: &mut Image, p: &LocalContrast, k: &LocalStrengths) {
    let scale = reference_scale(img.width, img.height);
    apply_scaled(img, p, k, scale);
}

/// As [`apply`], with an explicit spatial scale (1.0 = the 24 MP reference).
/// Useful for previews of a larger original and for tests on small images.
pub fn apply_scaled(img: &mut Image, p: &LocalContrast, k: &LocalStrengths, scale: f32) {
    if p.is_neutral() || img.pixel_count() == 0 {
        return;
    }
    let (w, h) = (img.width, img.height);
    let lw = luminance_weights();
    let l: Vec<f32> = img
        .rgb
        .par_chunks(3)
        .map(|c| srgb_encode(lw[0] * c[0] + lw[1] * c[1] + lw[2] * c[2]))
        .collect();

    // Pre-blurred luminance for each control (shared when the scales agree).
    let sigma_c = k.clarity_fine_sigma_px * scale;
    let sigma_t = k.texture_fine_sigma_px * scale;
    let src_c = (p.clarity != 0.0).then(|| prefilter(&l, w, h, sigma_c));
    let share = src_c.is_some() && sigma_t == sigma_c;
    let src_t = (p.texture != 0.0 && !share).then(|| prefilter(&l, w, h, sigma_t));

    let clarity = src_c.as_deref().map(|src| Band {
        src,
        base: GuidedBase::new(
            src,
            w,
            h,
            (k.clarity_radius_frac * REFERENCE_DIAGONAL * scale).max(1.0),
            k.clarity_edge_eps,
            CLARITY_LOW_RES_RADIUS,
        ),
        amount: signed_amount(p.clarity, k.clarity_gain, k.clarity_soften),
        weight: midtone_weight_lut(k.clarity_midtone_power),
        limit: k.clarity_detail_limit,
    });
    let texture = (p.texture != 0.0).then(|| {
        let src = src_t.as_deref().or(src_c.as_deref()).unwrap_or(&l);
        Band {
            src,
            base: GuidedBase::new(
                src,
                w,
                h,
                (k.texture_medium_radius_px * scale).max(1.0),
                k.texture_edge_eps,
                TEXTURE_LOW_RES_RADIUS,
            ),
            amount: signed_amount(p.texture, k.texture_gain, k.texture_smooth),
            weight: midtone_weight_lut(k.texture_midtone_power),
            limit: k.texture_band_limit,
        }
    });
    let bands: Vec<&Band> = clarity.iter().chain(texture.iter()).collect();

    let decode_lut: Vec<f32> = (0..DECODE_LUT_SIZE)
        .map(|i| srgb_decode(i as f32 / (DECODE_LUT_SIZE - 1) as f32))
        .collect();
    // One fused pass per row: evaluate the bases, sum the changes, apply to RGB.
    img.rgb.par_chunks_mut(3 * w).enumerate().for_each_init(
        || (vec![0.0f32; w], vec![0.0f32; w]),
        |(delta, base_row), (y, row)| {
            let span = y * w..(y + 1) * w;
            delta.fill(0.0);
            for band in &bands {
                band.add_row(y, base_row, delta);
            }
            for ((px, &lv), &d) in row.chunks_exact_mut(3).zip(&l[span]).zip(delta.iter()) {
                if d == 0.0 {
                    continue;
                }
                let target = lv + headroom_limit(lv, d);
                let y_new = if (0.0..=1.0).contains(&target) {
                    lookup(&decode_lut, target)
                } else {
                    srgb_decode(target)
                };
                apply_luminance(px, lw, y_new);
            }
        },
    );
}

/// One control's contribution: `amount · w(base) · limit(src - base)`.
struct Band<'a> {
    src: &'a [f32],
    base: GuidedBase,
    amount: f32,
    weight: Vec<f32>,
    limit: f32,
}

impl Band<'_> {
    /// Add this control's change for row `y` to `delta` (`base_row` is scratch).
    fn add_row(&self, y: usize, base_row: &mut [f32], delta: &mut [f32]) {
        let w = delta.len();
        let src = &self.src[y * w..(y + 1) * w];
        self.base.row(y, src, base_row);
        for ((d, &s), &b) in delta.iter_mut().zip(src).zip(base_row.iter()) {
            *d += self.amount * lookup(&self.weight, b) * soft_limit(s - b, self.limit);
        }
    }
}

/// Scale a working-space pixel to luminance `y_new`: by the RGB ratio (hue and
/// saturation preserving) above [`RATIO_KNEE_Y`], blending to a neutral offset
/// towards black. Both give luminance exactly `y_new`.
fn apply_luminance(px: &mut [f32], lw: [f32; 3], y_new: f32) {
    let y = lw[0] * px[0] + lw[1] * px[1] + lw[2] * px[2];
    if y >= RATIO_KNEE_Y {
        let ratio = y_new / y;
        px.iter_mut().for_each(|c| *c *= ratio);
    } else {
        let t = (y / RATIO_KNEE_Y).max(0.0);
        let ratio = if y > 0.0 { y_new / y } else { 1.0 };
        let offset = y_new - y;
        px.iter_mut().for_each(|c| {
            let shifted = *c + offset;
            *c = shifted + t * (*c * ratio - shifted);
        });
    }
}

/// Slider → signed amount: positive scales the detail by `gain`, negative removes
/// a fraction of it (never more than all of it, so detail is never inverted).
fn signed_amount(slider: f64, gain: f32, soften: f32) -> f32 {
    let s = (slider / 100.0) as f32;
    if s >= 0.0 {
        gain * s
    } else {
        -(soften * -s).clamp(0.0, 1.0)
    }
}

/// Smooth saturation `x / sqrt(1 + x²)`: slope 1 at 0, tends to ±1. A cheaper
/// stand-in for tanh (no transcendental call in the per-pixel loop).
fn sat(x: f32) -> f32 {
    x / (1.0 + x * x).sqrt()
}

/// `c · sat(v / c)`: linear for `|v| << c`, saturating at `±c`. `c <= 0` = no limit.
fn soft_limit(v: f32, c: f32) -> f32 {
    if c > 0.0 { c * sat(v / c) } else { v }
}

/// Soft-limit a change `d` of encoded luminance `l` to the room left before
/// white (for brightening) or black (for darkening): `room · sat(d / room)`.
/// Slope 1 for small changes, never crossing 0 or 1.
fn headroom_limit(l: f32, d: f32) -> f32 {
    let room = if d > 0.0 { 1.0 - l } else { l };
    if room <= 0.0 {
        0.0
    } else {
        room * sat(d / room)
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

/// `(4 x (1 - x))^power` on 0..1 as a table: 1 at mid-grey, 0 at black and white.
fn midtone_weight_lut(power: f32) -> Vec<f32> {
    let power = power.max(0.0);
    (0..WEIGHT_LUT_SIZE)
        .map(|i| {
            let x = i as f32 / (WEIGHT_LUT_SIZE - 1) as f32;
            (4.0 * x * (1.0 - x)).clamp(0.0, 1.0).powf(power)
        })
        .collect()
}

fn lookup(lut: &[f32], v: f32) -> f32 {
    let pos = v.clamp(0.0, 1.0) * (lut.len() - 1) as f32;
    let i = (pos as usize).min(lut.len() - 2);
    let f = pos - i as f32;
    lut[i] + (lut[i + 1] - lut[i]) * f
}

/// Gaussian pre-blur, borrowing the input when the blur would be negligible.
fn prefilter(plane: &[f32], w: usize, h: usize, sigma: f32) -> Cow<'_, [f32]> {
    if sigma < 0.3 {
        Cow::Borrowed(plane)
    } else {
        Cow::Owned(gaussian_blur(plane, w, h, sigma))
    }
}

/// Edge-preserving base of a plane: the guided filter with the plane as its
/// own guide, box radius `radius` (pixels) and regularisation `eps`, stored as
/// its linear coefficients (`base = A · src + B`). Radii above
/// `low_res_radius` use the fast guided filter (He & Sun, 2015): `A` and `B` are
/// found on a box-subsampled copy and bilinearly upsampled when a row is
/// evaluated, so edges stay as sharp as the input while cost and memory drop
/// with the square of the subsampling and no full-size base plane is stored.
struct GuidedBase {
    a: Vec<f32>,
    b: Vec<f32>,
    /// Subsampling factor and low-resolution size.
    s: usize,
    wl: usize,
    hl: usize,
    /// Horizontal interpolation (left, right, fraction) per full-res column.
    xs: Vec<(usize, usize, f32)>,
}

impl GuidedBase {
    fn new(src: &[f32], w: usize, h: usize, radius: f32, eps: f32, low_res_radius: f32) -> Self {
        let eps = eps.max(1e-8);
        let s = ((radius / low_res_radius.max(1.0)).round() as usize)
            .clamp(1, GUIDED_MAX_SUBSAMPLE)
            .min(w.min(h).max(1));
        let r = ((radius / s as f32).round() as usize).max(1);
        let (wl, hl) = (w.div_ceil(s), h.div_ceil(s));
        let low = if s == 1 {
            Cow::Borrowed(src)
        } else {
            Cow::Owned(downsample(src, w, h, s))
        };
        let mean = box_blur(&low, wl, hl, r);
        let squares: Vec<f32> = low.par_iter().map(|v| v * v).collect();
        let corr = box_blur(&squares, wl, hl, r);
        drop(squares);
        let (a, b): (Vec<f32>, Vec<f32>) = mean
            .par_iter()
            .zip(corr.par_iter())
            .map(|(&m, &c)| {
                let var = (c - m * m).max(0.0);
                let a = var / (var + eps);
                (a, m - a * m)
            })
            .unzip();
        drop((mean, corr));
        let a = box_blur(&a, wl, hl, r);
        let b = box_blur(&b, wl, hl, r);
        let xs = (0..w).map(|x| Self::coord(x, s, wl)).collect();
        GuidedBase {
            a,
            b,
            s,
            wl,
            hl,
            xs,
        }
    }

    /// Low-res sample j sits at the centre of block j, i.e. at full-res
    /// coordinate (j + 0.5) · s - 0.5. Returns (left, right, fraction).
    fn coord(x: usize, s: usize, n: usize) -> (usize, usize, f32) {
        let f = ((x as f32 + 0.5) / s as f32 - 0.5).clamp(0.0, (n - 1) as f32);
        let i0 = (f as usize).min(n - 1);
        (i0, (i0 + 1).min(n - 1), f - i0 as f32)
    }

    /// Base values of full-res row `y`, whose input values are `src_row`.
    fn row(&self, y: usize, src_row: &[f32], out: &mut [f32]) {
        let wl = self.wl;
        let (y0, y1, ty) = Self::coord(y, self.s, self.hl);
        let (a0, a1) = (&self.a[y0 * wl..][..wl], &self.a[y1 * wl..][..wl]);
        let (b0, b1) = (&self.b[y0 * wl..][..wl], &self.b[y1 * wl..][..wl]);
        for ((o, &i), &(x0, x1, tx)) in out.iter_mut().zip(src_row).zip(&self.xs) {
            let bilerp = |p0: &[f32], p1: &[f32]| {
                let top = p0[x0] + (p0[x1] - p0[x0]) * tx;
                let bottom = p1[x0] + (p1[x1] - p1[x0]) * tx;
                top + (bottom - top) * ty
            };
            *o = bilerp(a0, a1) * i + bilerp(b0, b1);
        }
    }
}

/// Full-size base plane (tests and diagnostics).
#[cfg(test)]
fn guided_base(
    src: &[f32],
    w: usize,
    h: usize,
    radius: f32,
    eps: f32,
    low_res_radius: f32,
) -> Vec<f32> {
    let g = GuidedBase::new(src, w, h, radius, eps, low_res_radius);
    let mut out = vec![0.0f32; w * h];
    out.par_chunks_mut(w)
        .enumerate()
        .for_each(|(y, row)| g.row(y, &src[y * w..(y + 1) * w], row));
    out
}

/// Box-average `s × s` blocks (partial blocks at the right and bottom edges
/// average the pixels they have).
fn downsample(src: &[f32], w: usize, h: usize, s: usize) -> Vec<f32> {
    let (wl, hl) = (w.div_ceil(s), h.div_ceil(s));
    let mut out = vec![0.0f32; wl * hl];
    out.par_chunks_mut(wl).enumerate().for_each(|(j, row)| {
        let (y0, y1) = (j * s, ((j + 1) * s).min(h));
        for (i, o) in row.iter_mut().enumerate() {
            let (x0, x1) = (i * s, ((i + 1) * s).min(w));
            let sum: f32 = (y0..y1)
                .map(|y| src[y * w + x0..y * w + x1].iter().sum::<f32>())
                .sum();
            *o = sum / ((x1 - x0) * (y1 - y0)) as f32;
        }
    });
    debug_assert_eq!(out.len(), wl * hl);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const K: LocalStrengths = DEFAULT_STRENGTHS;

    fn lc(texture: f64, clarity: f64) -> LocalContrast {
        LocalContrast { texture, clarity }
    }

    /// Grey image from an encoded-luminance function.
    fn grey(w: usize, h: usize, f: impl Fn(usize, usize) -> f32 + Sync) -> Image {
        Image::from_fn(w, h, |x, y| [srgb_decode(f(x, y)); 3])
    }

    /// Encoded luminance plane of an image.
    fn encoded(img: &Image) -> Vec<f32> {
        img.luminance().into_iter().map(srgb_encode).collect()
    }

    fn row(img: &Image, y: usize) -> Vec<f32> {
        let l = encoded(img);
        l[y * img.width..(y + 1) * img.width].to_vec()
    }

    fn std_dev(v: &[f32]) -> f32 {
        let m = v.iter().sum::<f32>() / v.len() as f32;
        (v.iter().map(|x| (x - m) * (x - m)).sum::<f32>() / v.len() as f32).sqrt()
    }

    fn rms_change(a: &Image, b: &Image) -> f32 {
        let (la, lb) = (encoded(a), encoded(b));
        let s: f32 = la.iter().zip(&lb).map(|(x, y)| (x - y) * (x - y)).sum();
        (s / la.len() as f32).sqrt()
    }

    /// A busy synthetic scene: gradients, sinusoids at several scales, hard-edged
    /// rectangles and deterministic pseudo-noise, all in encoded luminance.
    fn scene(w: usize, h: usize) -> Image {
        grey(w, h, |x, y| {
            let (xf, yf) = (x as f32, y as f32);
            let mut v = 0.15 + 0.6 * xf / w as f32;
            v += 0.08 * (xf / 37.0).sin() * (yf / 23.0).cos();
            v += 0.04 * (xf / 5.0 + yf / 7.0).sin();
            if (w / 5..w / 3).contains(&x) && (h / 4..h / 2).contains(&y) {
                v += 0.25;
            }
            if (2 * w / 3..4 * w / 5).contains(&x) && (h / 2..3 * h / 4).contains(&y) {
                v -= 0.3;
            }
            let hash = ((x * 7919 + y * 104_729) % 1013) as f32 / 1013.0;
            (v + 0.02 * (hash - 0.5)).clamp(0.0, 1.0)
        })
    }

    #[test]
    fn neutral_is_an_exact_no_op() {
        let mut img = scene(97, 61);
        let before = img.rgb.clone();
        apply(&mut img, &lc(0.0, 0.0), &K);
        assert_eq!(img.rgb, before);
        apply_scaled(&mut img, &lc(0.0, 0.0), &K, 1.0);
        assert_eq!(img.rgb, before);
    }

    #[test]
    fn flat_image_is_unchanged() {
        for v in [0.0, 0.003, 0.18, 0.5, 1.0] {
            for p in [lc(100.0, 100.0), lc(-100.0, -100.0), lc(100.0, -100.0)] {
                for scale in [0.05, 1.0] {
                    let mut img = Image::from_fn(64, 48, |_, _| [v; 3]);
                    apply_scaled(&mut img, &p, &K, scale);
                    let worst = img.rgb.iter().map(|c| (c - v).abs()).fold(0.0, f32::max);
                    assert!(worst < 1e-5, "v {v} {p:?} scale {scale}: {worst}");
                }
            }
        }
    }

    #[test]
    fn clarity_changes_large_scale_local_contrast() {
        // End to end through `apply`: the radius follows the image size.
        let (w, h) = (600, 64);
        let radius = K.clarity_radius_frac * (w as f32).hypot(h as f32);
        let period = 4.5 * radius;
        let tau = std::f32::consts::TAU;
        let wave = |x: usize, _: usize| 0.5 + 0.08 * (tau * x as f32 / period).sin();
        let original = grey(w, h, wave);
        let amp = |img: &Image| std_dev(&row(img, h / 2)[60..w - 60]);
        let a0 = amp(&original);

        let mut plus = original.clone();
        apply(&mut plus, &lc(0.0, 100.0), &K);
        let mut minus = original.clone();
        apply(&mut minus, &lc(0.0, -100.0), &K);
        let (ap, am) = (amp(&plus), amp(&minus));
        assert!(ap > 1.3 * a0, "+100 boosts the undulation: {a0} -> {ap}");
        assert!(am < 0.7 * a0, "-100 flattens it: {a0} -> {am}");

        // A soft large-scale edge: contrast across it grows with +, shrinks with -.
        let edge = |x: usize, _: usize| {
            0.4 + 0.15 / (1.0 + (-(x as f32 - w as f32 / 2.0) / (0.6 * radius)).exp())
        };
        let original = grey(w, h, edge);
        let (lo, hi) = (w / 2 - radius as usize, w / 2 + radius as usize);
        let contrast = |img: &Image| {
            let r = row(img, h / 2);
            r[hi] - r[lo]
        };
        let c0 = contrast(&original);
        let mut plus = original.clone();
        apply(&mut plus, &lc(0.0, 100.0), &K);
        let mut minus = original.clone();
        apply(&mut minus, &lc(0.0, -100.0), &K);
        assert!(contrast(&plus) > 1.15 * c0, "{c0} -> {}", contrast(&plus));
        assert!(contrast(&minus) < 0.9 * c0, "{c0} -> {}", contrast(&minus));
    }

    #[test]
    fn clarity_favours_midtones() {
        // The same small undulation around mid-grey and in deep shadow.
        let (w, h) = (400, 32);
        let radius = K.clarity_radius_frac * (w as f32).hypot(h as f32);
        let tau = std::f32::consts::TAU;
        let at = |level: f32| {
            let original = grey(w, h, |x, _| {
                level + 0.02 * (tau * x as f32 / (4.0 * radius)).sin()
            });
            let mut out = original.clone();
            apply(&mut out, &lc(0.0, 100.0), &K);
            rms_change(&original, &out)
        };
        let (mid, dark) = (at(0.5), at(0.05));
        assert!(mid > 2.0 * dark, "midtones {mid} vs shadows {dark}");
    }

    #[test]
    fn texture_targets_medium_detail_not_pixel_noise_or_large_shapes() {
        let (w, h) = (128, 128);
        let tau = std::f32::consts::TAU;
        let grating = grey(w, h, |x, y| {
            0.5 + 0.05 * (tau * x as f32 / 24.0).sin() * (tau * y as f32 / 24.0).cos()
        });
        let checker = grey(w, h, |x, y| {
            0.5 + 0.05 * if (x + y) % 2 == 0 { 1.0 } else { -1.0 }
        });
        let large = grey(512, 64, |x, _| 0.5 + 0.05 * (tau * x as f32 / 400.0).sin());
        let effect = |img: &Image, t: f64| {
            let mut out = img.clone();
            apply_scaled(&mut out, &lc(t, 0.0), &K, 1.0);
            rms_change(img, &out)
        };
        for t in [100.0, -100.0] {
            let (g, c, l) = (effect(&grating, t), effect(&checker, t), effect(&large, t));
            assert!(g > 0.01, "texture {t} moves the grating: {g}");
            assert!(g > 10.0 * c, "texture {t}: grating {g} vs 1-px checker {c}");
            assert!(g > 5.0 * l, "texture {t}: grating {g} vs large waves {l}");
        }
        // Positive enhances the grating, negative smooths it.
        let amp = |img: &Image| std_dev(&encoded(img));
        let mut plus = grating.clone();
        apply_scaled(&mut plus, &lc(100.0, 0.0), &K, 1.0);
        let mut minus = grating.clone();
        apply_scaled(&mut minus, &lc(-100.0, 0.0), &K, 1.0);
        let a0 = amp(&grating);
        assert!(amp(&plus) > 1.3 * a0, "{a0} -> {}", amp(&plus));
        assert!(amp(&minus) < 0.6 * a0, "{a0} -> {}", amp(&minus));
    }

    #[test]
    fn negative_texture_keeps_strong_edges() {
        let (w, h) = (128, 16);
        let original = grey(w, h, |x, _| if x < w / 2 { 0.25 } else { 0.75 });
        let mut out = original.clone();
        apply_scaled(&mut out, &lc(-100.0, 0.0), &K, 1.0);
        let (r0, r1) = (row(&original, h / 2), row(&out, h / 2));
        for x in [w / 2 - 2, w / 2 - 1, w / 2, w / 2 + 1] {
            assert!(
                (r1[x] - r0[x]).abs() < 0.03,
                "x {x}: {} -> {}",
                r0[x],
                r1[x]
            );
        }
    }

    /// Largest excursion (encoded L) outside the original [lo, hi] range.
    fn overshoot(img: &Image, lo: f32, hi: f32) -> f32 {
        encoded(img)
            .iter()
            .map(|&v| (v - hi).max(lo - v).max(0.0))
            .fold(0.0, f32::max)
    }

    /// Halo bound: next to a hard 0.2 → 0.8 edge (encoded), +100 Clarity and/or
    /// Texture may overshoot by at most 0.05 encoded (~13 levels in 8-bit). A
    /// Gaussian band-pass at the Texture scale and gain overshoots more than twice that.
    const HALO_LIMIT: f32 = 0.05;

    #[test]
    fn halos_at_hard_edges_stay_bounded() {
        let (w, h) = (512, 24);
        let original = grey(w, h, |x, _| if x < w / 2 { 0.2 } else { 0.8 });
        for scale in [reference_scale(w, h), 0.3, 1.0] {
            for p in [lc(100.0, 0.0), lc(0.0, 100.0), lc(100.0, 100.0)] {
                let mut out = original.clone();
                apply_scaled(&mut out, &p, &K, scale);
                let o = overshoot(&out, 0.2, 0.8);
                assert!(o < HALO_LIMIT, "{p:?} scale {scale}: overshoot {o}");
            }
        }
        // For reference: a plain (non edge-aware) unsharp mask of the Texture band.
        let l = encoded(&original);
        let fine = gaussian_blur(&l, w, h, K.texture_fine_sigma_px);
        let coarse = gaussian_blur(&l, w, h, K.texture_medium_radius_px);
        let naive = l
            .iter()
            .zip(fine.iter().zip(&coarse))
            .map(|(&v, (&f, &c))| v + K.texture_gain * (f - c))
            .map(|v| (v - 0.8).max(0.2 - v).max(0.0))
            .fold(0.0, f32::max);
        assert!(naive > 2.0 * HALO_LIMIT, "naive overshoot {naive}");
    }

    /// Mean encoded luminance (perceived brightness) is what the model keeps:
    /// detail is zero-mean around an edge-preserving base. Mean linear Y may
    /// rise a little with positive settings because decoding is convex (more
    /// contrast in L means a higher linear mean); bounded loosely here, and
    /// largest when the Clarity radius spans much of a small test image.
    #[test]
    fn global_brightness_is_preserved() {
        let original = scene(320, 200);
        let l0 = encoded(&original);
        let mean = |v: &[f32]| v.iter().map(|&x| f64::from(x)).sum::<f64>() / v.len() as f64;
        let y0 = mean(&original.luminance());
        for p in [
            lc(100.0, 0.0),
            lc(-100.0, 0.0),
            lc(0.0, 100.0),
            lc(0.0, -100.0),
            lc(100.0, 100.0),
            lc(-100.0, -100.0),
        ] {
            for scale in [reference_scale(320, 200), 1.0] {
                let mut out = original.clone();
                apply_scaled(&mut out, &p, &K, scale);
                let dl = mean(&encoded(&out)) - mean(&l0);
                let dy = mean(&out.luminance()) / y0 - 1.0;
                assert!(dl.abs() < 0.005, "{p:?} scale {scale}: mean L moved {dl}");
                assert!(dy.abs() < 0.08, "{p:?} scale {scale}: mean Y moved {dy}");
                if scale < 0.1 {
                    // At the image's own scale the linear mean barely moves either.
                    assert!(dy.abs() < 0.02, "{p:?}: mean Y moved {dy}");
                }
            }
        }
    }

    #[test]
    fn hue_and_saturation_are_preserved() {
        let tint = [0.9f32, 0.45, 0.2];
        let original = Image::from_fn(160, 120, |x, y| {
            let (xf, yf) = (x as f32, y as f32);
            let f = 0.05 + 0.5 * xf / 160.0 + 0.2 * (xf / 9.0).sin() * (yf / 13.0).cos();
            tint.map(|c| c * f.max(0.01))
        });
        let mut out = original.clone();
        apply_scaled(&mut out, &lc(100.0, 100.0), &K, 0.5);
        assert!(
            rms_change(&original, &out) > 0.005,
            "the edit did something"
        );
        let w = luminance_weights();
        for (a, b) in original.rgb.chunks(3).zip(out.rgb.chunks(3)) {
            if w[0] * a[0] + w[1] * a[1] + w[2] * a[2] < RATIO_KNEE_Y {
                continue;
            }
            for c in [0, 2] {
                let (ra, rb) = (a[c] / a[1], b[c] / b[1]);
                assert!((ra - rb).abs() < 1e-4 * ra, "{a:?} -> {b:?}");
            }
        }
    }

    #[test]
    fn extremes_stay_finite_and_in_range() {
        let images = [
            Image::from_fn(40, 30, |_, _| [0.0; 3]),
            Image::from_fn(40, 30, |_, _| [1.0; 3]),
            Image::from_fn(1, 1, |_, _| [0.3, 0.2, 0.1]),
            Image::from_fn(2, 3, |x, y| [x as f32 * 0.5, y as f32 * 0.3, 0.2]),
            Image::from_fn(3, 2, |x, y| [1.0 - x as f32 * 0.4, 0.0, y as f32]),
            scene(37, 23),
            // Out-of-range working values (negative channel, above white).
            Image::from_fn(33, 17, |x, y| {
                [
                    (x as f32 / 10.0) - 0.3,
                    (y as f32 / 8.0),
                    0.5 - (x + y) as f32 / 40.0,
                ]
            }),
        ];
        let sliders = [-1000.0, -100.0, -37.0, 0.0, 55.0, 100.0, 1000.0];
        for img in &images {
            for &t in &sliders {
                for &c in &sliders {
                    for scale in [reference_scale(img.width, img.height), 1.0, 3.0] {
                        let mut out = img.clone();
                        apply_scaled(&mut out, &lc(t, c), &K, scale);
                        assert!(
                            out.rgb.iter().all(|v| v.is_finite()),
                            "{}x{} t {t} c {c} scale {scale}",
                            img.width,
                            img.height
                        );
                        // In-range inputs keep their luminance in range.
                        if img.rgb.iter().all(|v| (0.0..=1.0).contains(v)) {
                            for y in out.luminance() {
                                assert!((-1e-6..=1.0 + 1e-5).contains(&y), "Y {y}");
                            }
                        }
                    }
                }
            }
        }
        // Black stays black and white stays white.
        for v in [0.0f32, 1.0] {
            let mut img = Image::from_fn(50, 40, |_, _| [v; 3]);
            apply(&mut img, &lc(100.0, 100.0), &K);
            assert!(img.rgb.iter().all(|c| (c - v).abs() < 1e-6));
        }
    }

    #[test]
    fn spatial_scale_follows_image_size() {
        // The same scene at two resolutions gets the same relative change.
        let tau = std::f32::consts::TAU;
        let pattern = |w: usize, h: usize| {
            grey(w, h, move |x, y| {
                let (u, v) = (x as f32 / w as f32, y as f32 / h as f32);
                let edge = if u > 0.6 { 0.15 } else { 0.0 };
                0.35 + edge + 0.06 * (tau * 12.0 * u).sin() * (tau * 5.0 * v).cos()
            })
        };
        let change = |w: usize, h: usize, p: LocalContrast, scale: Option<f32>| {
            let original = pattern(w, h);
            let mut out = original.clone();
            match scale {
                Some(s) => apply_scaled(&mut out, &p, &K, s),
                None => apply(&mut out, &p, &K),
            }
            rms_change(&original, &out)
        };
        // Clarity through `apply` (scale from the image size).
        let (big, small) = (
            change(800, 400, lc(0.0, 100.0), None),
            change(400, 200, lc(0.0, 100.0), None),
        );
        assert!((big / small - 1.0).abs() < 0.15, "clarity {big} vs {small}");
        // Texture at its reference scale and at half of it.
        let (big, small) = (
            change(512, 256, lc(100.0, 0.0), Some(1.0)),
            change(256, 128, lc(100.0, 0.0), Some(0.5)),
        );
        assert!((big / small - 1.0).abs() < 0.2, "texture {big} vs {small}");
    }

    #[test]
    fn limiters_and_amounts() {
        // Headroom limiter: slope 1 near zero, never leaves 0..1.
        assert!((headroom_limit(0.5, 0.001) - 0.001).abs() < 1e-6);
        for l in [0.0f32, 0.01, 0.3, 0.9, 0.999, 1.0] {
            for d in [-5.0f32, -0.3, -0.01, 0.01, 0.3, 5.0] {
                let out = l + headroom_limit(l, d);
                assert!((0.0..=1.0).contains(&out), "l {l} d {d} -> {out}");
                assert!(headroom_limit(l, d) * d >= 0.0, "never reverses the change");
            }
        }
        assert!((soft_limit(0.001, 0.05) - 0.001).abs() < 1e-6);
        assert!(soft_limit(10.0, 0.05) < 0.05 && soft_limit(-10.0, 0.05) > -0.05);
        assert_eq!(soft_limit(0.3, 0.0), 0.3);
        // Negative sliders never remove more than all of the detail.
        assert_eq!(signed_amount(-100.0, 1.0, 5.0), -1.0);
        assert_eq!(signed_amount(50.0, 1.2, 0.8), 0.6);
        assert_eq!(signed_amount(0.0, 1.2, 0.8), 0.0);
        // Midtone weight: 1 at mid-grey, 0 at the ends, symmetric.
        let w = midtone_weight_lut(0.75);
        assert!((lookup(&w, 0.5) - 1.0).abs() < 1e-6);
        assert!(lookup(&w, 0.0) == 0.0 && lookup(&w, 1.0) == 0.0);
        assert!((lookup(&w, 0.2) - lookup(&w, 0.8)).abs() < 1e-5);
    }

    #[test]
    fn deterministic() {
        let original = scene(211, 157);
        let run = || {
            let mut img = original.clone();
            apply_scaled(&mut img, &lc(63.0, -41.0), &K, 0.7);
            img.rgb
        };
        assert_eq!(run(), run());
    }

    #[test]
    fn subsampled_guided_base_matches_full_resolution() {
        // The fast (subsampled) guided filter stays close to the exact one, at
        // Clarity-like and Texture-like radii.
        let (w, h) = (300, 200);
        let l = encoded(&scene(w, h));
        for (radius, eps, low) in [
            (24.0, 0.01, CLARITY_LOW_RES_RADIUS),
            (60.0, 0.01, CLARITY_LOW_RES_RADIUS),
            (10.0, 0.004, TEXTURE_LOW_RES_RADIUS),
        ] {
            let fast = guided_base(&l, w, h, radius, eps, low);
            let exact = crate::filters::guided_filter(&l, &l, w, h, radius as usize, eps);
            let err: Vec<f32> = fast
                .iter()
                .zip(&exact)
                .map(|(a, b)| (a - b).abs())
                .collect();
            let mean_err = err.iter().sum::<f32>() / err.len() as f32;
            let max_err = err.iter().cloned().fold(0.0, f32::max);
            assert!(mean_err < 0.003, "r {radius}: mean abs error {mean_err}");
            assert!(max_err < 0.05, "r {radius}: max abs error {max_err}");
        }
    }

    #[test]
    fn strengths_round_trip_through_json() {
        let k: LocalStrengths = serde_json::from_str(r#"{"clarity_gain": 0.5}"#).unwrap();
        assert_eq!(k.clarity_gain, 0.5);
        assert_eq!(k.texture_gain, DEFAULT_STRENGTHS.texture_gain);
        let round: LocalStrengths =
            serde_json::from_str(&serde_json::to_string(&k).unwrap()).unwrap();
        assert_eq!(round, k);
        assert_eq!(LocalStrengths::default(), DEFAULT_STRENGTHS);
    }

    /// Timing on a 24 MP frame: `cargo test --release -p flb-engine -- --ignored --nocapture local`.
    #[test]
    #[ignore]
    fn timing_24mp() {
        let original = scene(6000, 4000);
        for p in [lc(0.0, 50.0), lc(50.0, 0.0), lc(50.0, 50.0)] {
            let mut img = original.clone();
            let t = std::time::Instant::now();
            apply(&mut img, &p, &K);
            eprintln!("{p:?}: {:.0} ms", t.elapsed().as_secs_f64() * 1e3);
        }
    }
}
