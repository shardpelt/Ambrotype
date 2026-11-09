// SPDX-License-Identifier: GPL-3.0-or-later
//! Estimating lateral chromatic aberration from the image.

use super::camera::sample_bilinear;
use super::*;

/// Estimate lateral CA from the image: at strong edges that run across the
/// radial direction, find the radial offset that best aligns red (and blue)
/// with green, then fit δ(r) = a·r + b·r³ robustly. `None` when there are too
/// few usable edges.
pub fn estimate_ca(rgb: &[f32], w: usize, h: usize, max_shift: f64) -> Option<[RadialShift; 2]> {
    if w < 64 || h < 64 {
        return None;
    }
    let (cx, cy, half_diag) = centre(w, h);
    // Candidate edge points on a coarse grid, strongest per radial bin.
    const BINS: usize = 12;
    const PER_BIN: usize = 300;
    let stride = ((w * h) as f64 / 400_000.0).sqrt().ceil().max(2.0) as usize;
    let green = |x: usize, y: usize| f64::from(rgb[(y * w + x) * 3 + 1]);
    let mut bins: Vec<Vec<(f64, usize, usize)>> = vec![Vec::new(); BINS];
    let margin = (max_shift.ceil() as usize + 10).min(w / 4).min(h / 4);
    for y in (margin..h - margin).step_by(stride) {
        for x in (margin..w - margin).step_by(stride) {
            let (dx, dy) = (x as f64 - cx, y as f64 - cy);
            let rp = dx.hypot(dy);
            let r = rp / half_diag;
            if !(0.2..0.98).contains(&r) {
                continue;
            }
            let gx = green(x + 1, y) - green(x - 1, y);
            let gy = green(x, y + 1) - green(x, y - 1);
            let mag = gx.hypot(gy);
            let level = green(x, y).max(1e-4);
            if mag / level < 0.15 {
                continue;
            }
            let radial = (gx * dx + gy * dy).abs() / (rp * mag);
            if radial < 0.85 {
                continue;
            }
            let bin = ((r * BINS as f64) as usize).min(BINS - 1);
            bins[bin].push((mag / level, x, y));
        }
    }
    let points: Vec<(usize, usize)> = bins
        .into_iter()
        .flat_map(|mut b| {
            b.sort_by(|a, b| b.0.total_cmp(&a.0));
            b.truncate(PER_BIN);
            b.into_iter().map(|(_, x, y)| (x, y))
        })
        .collect();
    if points.len() < 60 {
        return None;
    }
    // Radial shift of channel c against green at each point.
    let measure = |c: usize| -> Vec<(f64, f64)> {
        points
            .par_iter()
            .filter_map(|&(x, y)| {
                let (dx, dy) = (x as f64 - cx, y as f64 - cy);
                let rp = dx.hypot(dy);
                let (ex, ey) = (dx / rp, dy / rp);
                let shift = radial_shift(rgb, w, h, c, x as f64, y as f64, ex, ey, max_shift)?;
                Some((rp / half_diag, shift))
            })
            .collect()
    };
    let fit = |samples: Vec<(f64, f64)>| robust_fit(&samples, max_shift);
    let red = fit(measure(0))?;
    let blue = fit(measure(2))?;
    Some([red, blue])
}

/// Offset t (pixels along (ex, ey)) such that channel c at s + t matches green
/// at s, around the edge point (x, y). `None` if the match is ambiguous.
#[allow(clippy::too_many_arguments)]
fn radial_shift(
    rgb: &[f32],
    w: usize,
    h: usize,
    c: usize,
    x: f64,
    y: f64,
    ex: f64,
    ey: f64,
    max_shift: f64,
) -> Option<f64> {
    const HALF: i32 = 6;
    let at = |ch: usize, t: f64| sample_bilinear(rgb, w, h, ch, x + ex * t, y + ey * t);
    let standardise = |v: &mut Vec<f64>| -> bool {
        let n = v.len() as f64;
        let mean = v.iter().sum::<f64>() / n;
        let var = v.iter().map(|a| (a - mean) * (a - mean)).sum::<f64>() / n;
        if var <= 1e-12 {
            return false;
        }
        let sd = var.sqrt();
        v.iter_mut().for_each(|a| *a = (*a - mean) / sd);
        true
    };
    let ts: Vec<f64> = (-HALF..=HALF).map(|i| i as f64 * 0.5).collect();
    let mut g: Vec<f64> = ts.iter().map(|t| at(1, *t)).collect();
    if !standardise(&mut g) {
        return None;
    }
    let cost = |d: f64| -> f64 {
        let mut v: Vec<f64> = ts.iter().map(|t| at(c, t + d)).collect();
        if !standardise(&mut v) {
            return f64::INFINITY;
        }
        v.iter().zip(&g).map(|(a, b)| (a - b) * (a - b)).sum()
    };
    let step = 0.1;
    let n = (max_shift / step).round() as i32;
    let costs: Vec<f64> = (-n..=n).map(|i| cost(i as f64 * step)).collect();
    let (best, &cmin) = costs.iter().enumerate().min_by(|a, b| a.1.total_cmp(b.1))?;
    if best == 0 || best == costs.len() - 1 || !cmin.is_finite() {
        return None;
    }
    // Parabolic refinement; reject flat minima.
    let (cl, cr) = (costs[best - 1], costs[best + 1]);
    let curvature = cl + cr - 2.0 * cmin;
    if curvature <= 1e-6 || cmin > 0.25 * ts.len() as f64 {
        return None;
    }
    let offset = 0.5 * (cl - cr) / curvature;
    Some((best as i32 - n) as f64 * step + offset * step)
}

/// Least-squares fit of δ = a·r + b·r³ with iterative outlier rejection.
fn robust_fit(samples: &[(f64, f64)], max_shift: f64) -> Option<RadialShift> {
    let mut pts: Vec<(f64, f64)> = samples.to_vec();
    let mut model = RadialShift { a: 0.0, b: 0.0 };
    for _ in 0..4 {
        if pts.len() < 40 {
            return None;
        }
        // Normal equations for basis (r, r³).
        let (mut s11, mut s13, mut s33, mut t1, mut t3) = (0.0, 0.0, 0.0, 0.0, 0.0);
        for &(r, d) in &pts {
            let r3 = r * r * r;
            s11 += r * r;
            s13 += r * r3;
            s33 += r3 * r3;
            t1 += r * d;
            t3 += r3 * d;
        }
        let det = s11 * s33 - s13 * s13;
        if det.abs() < 1e-12 {
            return None;
        }
        model = RadialShift {
            a: (t1 * s33 - t3 * s13) / det,
            b: (s11 * t3 - s13 * t1) / det,
        };
        let mut residuals: Vec<f64> = pts.iter().map(|&(r, d)| (d - model.at(r)).abs()).collect();
        residuals.sort_by(f64::total_cmp);
        let mad = residuals[residuals.len() / 2].max(0.02);
        pts.retain(|&(r, d)| (d - model.at(r)).abs() <= 2.5 * mad);
    }
    // Shifts larger than the search range are not trustworthy.
    if model.at(1.0).abs() > max_shift || !model.a.is_finite() || !model.b.is_finite() {
        return None;
    }
    Some(model)
}
