// SPDX-License-Identifier: GPL-3.0-or-later
//! The image filters Dehaze is built from: resampling, minimum filters, the guided filter's coefficients.

use crate::buffer::Image;
use crate::filters::box_blur;
use rayon::prelude::*;

/// Box-average downscale by an integer factor (partial blocks at the right and
/// bottom edges average what they cover).
pub(super) fn downscale(img: &Image, f: usize) -> (Vec<f32>, usize, usize) {
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
pub(super) fn resample(img: &Image, sw: usize, sh: usize) -> Vec<f32> {
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
pub(super) fn min_filter(plane: &[f32], w: usize, h: usize, r: usize) -> Vec<f32> {
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
pub(super) fn guided_coefficients(
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
pub(super) fn deterministic_mean(n: usize, f: impl Fn(usize) -> f64 + Sync) -> f64 {
    const CHUNK: usize = 4096;
    let partial: Vec<f64> = (0..n.div_ceil(CHUNK))
        .into_par_iter()
        .map(|c| (c * CHUNK..((c + 1) * CHUNK).min(n)).map(&f).sum())
        .collect();
    partial.iter().sum::<f64>() / n.max(1) as f64
}
