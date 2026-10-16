// SPDX-License-Identifier: GPL-3.0-or-later
//! Helpers the Detail stages share: blurs, the guided filter, downscaling,
//! lookup tables and the sRGB transfer.

use super::*;

/// Normalised Gaussian kernel (radius ⌈3σ⌉), the one `filters::gaussian_blur`
/// uses for σ ≤ 3; identity below σ = 0.3, as there.
pub(super) fn gaussian_kernel(sigma: f32) -> Vec<f32> {
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
pub(super) fn convolve_row(
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

/// Guided-filter coefficients (He, Sun & Tang) of several sources sharing one
/// guide: per source the box-averaged `(a, b)` with `q ≈ a · guide + b`, ready
/// to be upsampled and applied to a full-resolution guide. `|a|` is capped at
/// `max_slope` (with `b` recomputed, so local means are still reproduced).
pub(super) fn guided_coefficients(
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
pub(super) fn box_blur_frac(plane: &[f32], w: usize, h: usize, r: f32) -> Vec<f32> {
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
pub(super) fn downscale(img: &Image, f: usize) -> (Vec<f32>, usize, usize) {
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
pub(super) fn lerp_position(i: usize, f: usize, n: usize) -> (usize, usize, f32) {
    let s = ((i as f32 + 0.5) / f as f32 - 0.5).clamp(0.0, (n - 1) as f32);
    let lo = (s.floor() as usize).min(n - 1);
    (lo, (lo + 1).min(n - 1), s - lo as f32)
}

/// Change a pixel's linear luminance from `y` to `y_new` keeping its
/// chromaticity: scale RGB by the ratio, capped at `max_ratio`; any further
/// brightening (and any change to a pixel too dark to scale) is added as
/// neutral grey, which raises luminance one for one (the weights sum to 1).
#[inline]
pub(super) fn set_luminance(px: &mut [f32], y: f32, y_new: f32, max_ratio: f32) {
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
pub(super) fn encoded_luminance(img: &Image, offset: f32) -> Vec<f32> {
    let t = transfer();
    let lw = luminance_weights();
    img.rgb
        .par_chunks_exact(3)
        .map(|p| t.encode(bounded(dot(lw, p))) - offset)
        .collect()
}

/// Linear luminance made safe to filter: clamped to ±[`MAX_LINEAR`], NaN to 0.
#[inline]
pub(super) fn bounded(y: f32) -> f32 {
    if y.is_nan() {
        0.0
    } else {
        y.clamp(-MAX_LINEAR, MAX_LINEAR)
    }
}

/// Slider value clamped to its range; non-finite values fall back to `default`.
pub(super) fn slider(v: f64, default: f64, lo: f64, hi: f64) -> f64 {
    if v.is_finite() { v } else { default }.clamp(lo, hi)
}

#[inline]
pub(super) fn dot(w: [f32; 3], p: &[f32]) -> f32 {
    w[0] * p[0] + w[1] * p[1] + w[2] * p[2]
}

#[inline]
pub(super) fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    if e1 <= e0 {
        return if x >= e0 { 1.0 } else { 0.0 };
    }
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Linear interpolation in a table sampling 0..1 at `M - 1` steps (fixed size,
/// so the compiler drops the bounds checks). `v` must be in 0..1.
#[inline]
pub(super) fn interp<const M: usize>(t: &[f32; M], v: f32) -> f32 {
    let steps = M - 1;
    let pos = v * steps as f32;
    let i = (pos as usize).min(steps - 1);
    t[i] + (t[i + 1] - t[i]) * (pos - i as f32)
}

/// `x` clamped to 0..1, NaN to 0.
#[inline]
pub(super) fn unit(x: f32) -> f32 {
    if x.is_nan() { 0.0 } else { x.clamp(0.0, 1.0) }
}

pub(super) const LUT_STEPS: usize = 256;

/// A function of encoded luminance (0..1, clamped; NaN reads as 0) sampled
/// for per-pixel use.
pub(super) struct Lut(pub(super) Box<[f32; LUT_STEPS + 1]>);

impl Lut {
    pub(super) fn from_fn(f: impl Fn(f32) -> f32) -> Lut {
        let v: Vec<f32> = (0..=LUT_STEPS)
            .map(|i| f(i as f32 / LUT_STEPS as f32))
            .collect();
        Lut(v.into_boxed_slice().try_into().expect("LUT length"))
    }

    #[inline]
    pub(super) fn at(&self, x: f32) -> f32 {
        interp(&self.0, unit(x))
    }

    pub(super) fn max(&self) -> f32 {
        self.0.iter().copied().fold(f32::NEG_INFINITY, f32::max)
    }
}

pub(super) fn srgb_encode(v: f32) -> f32 {
    if v <= 0.003_130_8 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    }
}

pub(super) fn srgb_decode(v: f32) -> f32 {
    if v <= 0.040_45 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

pub(super) const ENC_STEPS: usize = 1 << 14;

pub(super) const DEC_STEPS: usize = 1 << 12;

/// The sRGB transfer as interpolated tables (exact formula outside 0..1, odd
/// extension below 0). The encode table is fine enough (2^14 steps) that its
/// error near black, where the curve bends most, stays around 1e-6.
pub(super) struct Transfer {
    pub(super) enc: Box<[f32; ENC_STEPS + 1]>,
    pub(super) dec: Box<[f32; DEC_STEPS + 1]>,
}

pub(super) fn transfer() -> &'static Transfer {
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
    pub(super) fn encode(&self, v: f32) -> f32 {
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
    pub(super) fn decode(&self, v: f32) -> f32 {
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
