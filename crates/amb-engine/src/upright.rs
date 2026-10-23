// SPDX-License-Identifier: GPL-3.0-or-later
//! Upright: find a perspective correction from the photo's own lines
//! (`PerspectiveUpright` Auto, Level, Vertical and Full),
//! docs/design/transform.md.
//!
//! **Model v1 (ours, not Adobe's).**
//!
//! 1. The photo is shrunk to at most [`ANALYSIS_EDGE`] pixels (an
//!    integer-factor area average, so the editor and export see the same
//!    pixels) and lens-corrected, because straight lines must be straight.
//! 2. Straight edges are found with a small line-segment detector: pixels
//!    whose gradient is strong grow into regions of the same edge direction,
//!    and long, thin, well-filled regions become segments.
//! 3. Segments within [`UprightStrengths::candidate`] degrees of vertical or
//!    horizontal are candidates. A virtual camera is turned (tilt about the
//!    frame's x and y axes, then roll) until they are as vertical and
//!    horizontal as possible: a robust fit first, then a least-squares fit
//!    on the segments that fit within [`UprightStrengths::inlier`] degrees.
//!
//! Level fits the roll only; Vertical the roll and the vertical tilt;
//! Full all three; Auto all three, with a larger penalty on the tilts so
//! strong corrections are made only partly. A small penalty in Vertical and
//! Full keeps a tilt that few lines settle from running away. The result is a
//! [`crate::transform::Keystone`] in front of the Transform sliders.

use rayon::prelude::*;

use crate::lens::{self, LensCorrections, LensStrengths};
use crate::raw::CameraImage;
use crate::transform::{Homography, Keystone, UprightMode};

/// Long edge of the analysis image, at most.
pub const ANALYSIS_EDGE: usize = 1024;

/// Detector and solver constants.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct UprightStrengths {
    /// Smallest gradient (on the square root of luminance, white = 1) that
    /// can be part of an edge.
    pub gradient: f64,
    /// Largest difference in edge direction within one segment, degrees.
    pub direction_tolerance: f64,
    /// Shortest segment, as a fraction of the analysis image's long edge.
    pub min_length: f64,
    /// Segments within this many degrees of vertical or horizontal are
    /// candidates.
    pub candidate: f64,
    /// After the robust fit, segments within this many degrees count.
    pub inlier: f64,
    /// Penalty on the squared tilts (radians), against the mean squared
    /// line error: a little for Vertical and Full, so a tilt the lines
    /// barely settle stays small, and more for Auto, so strong corrections
    /// are made only partly.
    pub tilt_damping: f64,
    pub auto_damping: f64,
    /// The same for the final least-squares fit of Vertical and Full.
    pub fit_damping: f64,
    /// Largest tilt and roll found, degrees.
    pub max_tilt: f64,
    pub max_roll: f64,
    /// Focal length (35 mm equivalent) when the photo doesn't say.
    pub default_focal_35mm: f64,
}

impl Default for UprightStrengths {
    fn default() -> Self {
        DEFAULT_STRENGTHS
    }
}

pub const DEFAULT_STRENGTHS: UprightStrengths = UprightStrengths {
    gradient: 0.015,
    direction_tolerance: 22.5,
    min_length: 0.03,
    candidate: 25.0,
    inlier: 2.5,
    tilt_damping: 0.003,
    auto_damping: 0.02,
    fit_damping: 0.0003,
    max_tilt: 35.0,
    max_roll: 20.0,
    default_focal_35mm: 35.0,
};

/// A straight edge, in normalised coordinates (centre 0, half diagonal 1,
/// y down).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Segment {
    pub a: (f64, f64),
    pub b: (f64, f64),
}

impl Segment {
    fn length(&self) -> f64 {
        (self.b.0 - self.a.0).hypot(self.b.1 - self.a.1)
    }
}

/// The Upright correction for `img` in `mode`: `None` when the mode isn't
/// solved from lines or the photo has too few of them.
pub fn analyse(
    img: &CameraImage,
    lc: &LensCorrections,
    lens_k: &LensStrengths,
    mode: UprightMode,
    k: &UprightStrengths,
) -> Option<Keystone> {
    if !mode.is_solved() || img.width < 16 || img.height < 16 {
        return None;
    }
    let (w, h, small) = shrink(&img.rgb, img.width, img.height, ANALYSIS_EDGE);
    let pass = lens::apply_camera(&small, w, h, img.shot.crop_factor, lc, lens_k);
    let lum = luminance(&pass.rgb);
    let segments = detect(&lum, pass.width, pass.height, pass.blank.as_deref(), k);
    let focal_35mm = match (img.shot.focal_mm, img.shot.crop_factor) {
        (Some(f), Some(c)) if f > 0.0 && c > 0.0 => f * c,
        _ => k.default_focal_35mm,
    };
    solve(&segments, mode, focal_35mm / (0.5 * 43.267), k)
}

/// The photo's long straight edges near horizontal or vertical (the lines
/// Upright fits), longest first, at most `max`: each as two points in
/// fractions of the (stored, lens-corrected) frame's width and height.
pub fn guide_lines(
    img: &CameraImage,
    lc: &LensCorrections,
    lens_k: &LensStrengths,
    k: &UprightStrengths,
    max: usize,
) -> Vec<[f64; 4]> {
    if img.width < 16 || img.height < 16 {
        return Vec::new();
    }
    let (w, h, small) = shrink(&img.rgb, img.width, img.height, ANALYSIS_EDGE);
    let pass = lens::apply_camera(&small, w, h, img.shot.crop_factor, lc, lens_k);
    let lum = luminance(&pass.rgb);
    let mut segs = detect(&lum, pass.width, pass.height, pass.blank.as_deref(), k);
    let near = k.candidate.to_radians().sin();
    segs.retain(|s| {
        let len = s.length();
        len > 0.0 && {
            let (ux, uy) = ((s.b.0 - s.a.0) / len, (s.b.1 - s.a.1) / len);
            ux.abs() < near || uy.abs() < near
        }
    });
    segs.sort_by(|a, b| b.length().total_cmp(&a.length()));
    segs.truncate(max);
    let (_, back) = crate::transform::normalising(pass.width, pass.height);
    let (fw, fh) = (pass.width as f64, pass.height as f64);
    segs.iter()
        .map(|s| {
            let a = back.apply(s.a.0, s.a.1);
            let b = back.apply(s.b.0, s.b.1);
            [
                (a.0 + 0.5) / fw,
                (a.1 + 0.5) / fh,
                (b.0 + 0.5) / fw,
                (b.1 + 0.5) / fh,
            ]
        })
        .collect()
}

/// Integer-factor area average to a long edge of at most `max_edge`.
fn shrink(rgb: &[f32], width: usize, height: usize, max_edge: usize) -> (usize, usize, Vec<f32>) {
    let factor = width.max(height).div_ceil(max_edge).max(1);
    if factor == 1 {
        return (width, height, rgb.to_vec());
    }
    let (w, h) = (width / factor, height / factor);
    let mut out = vec![0.0f32; w * h * 3];
    let n = (factor * factor) as f32;
    out.par_chunks_mut(w * 3).enumerate().for_each(|(y, row)| {
        for x in 0..w {
            let mut acc = [0.0f32; 3];
            for dy in 0..factor {
                let src = (y * factor + dy) * width;
                for dx in 0..factor {
                    let i = (src + x * factor + dx) * 3;
                    acc[0] += rgb[i];
                    acc[1] += rgb[i + 1];
                    acc[2] += rgb[i + 2];
                }
            }
            row[x * 3..x * 3 + 3].copy_from_slice(&acc.map(|v| v / n));
        }
    });
    (w, h, out)
}

/// Square root of luminance, scaled so the 99th percentile is 1.
fn luminance(rgb: &[f32]) -> Vec<f32> {
    let mut lum: Vec<f32> = rgb
        .chunks_exact(3)
        .map(|p| (0.25 * (p[0] + 2.0 * p[1] + p[2])).max(0.0))
        .collect();
    let mut sorted: Vec<f32> = lum.iter().step_by(7).copied().collect();
    sorted.sort_by(f32::total_cmp);
    let top = sorted
        .get(sorted.len() * 99 / 100)
        .copied()
        .unwrap_or(1.0)
        .max(1e-6);
    for v in &mut lum {
        *v = (*v / top).min(4.0).sqrt();
    }
    lum
}

/// Straight edges in a `w`×`h` plane. Pixels next to `blank` ones (areas the
/// lens corrections left empty) are ignored, as is a 2-pixel border.
pub fn detect(
    v: &[f32],
    w: usize,
    h: usize,
    blank: Option<&[bool]>,
    k: &UprightStrengths,
) -> Vec<Segment> {
    if w < 8 || h < 8 {
        return Vec::new();
    }
    let n = w * h;
    // A light blur, so stair-stepped and noisy edges keep one direction.
    let v = &blur(v, w, h);
    let mut mag = vec![0.0f32; n];
    let mut angle = vec![0.0f32; n];
    let near_blank = |x: usize, y: usize| {
        blank.is_some_and(|b| (y - 1..=y + 1).any(|yy| (x - 1..=x + 1).any(|xx| b[yy * w + xx])))
    };
    for y in 2..h - 2 {
        for x in 2..w - 2 {
            if near_blank(x, y) {
                continue;
            }
            let p = |dx: isize, dy: isize| {
                v[(y as isize + dy) as usize * w + (x as isize + dx) as usize]
            };
            let gx =
                (p(1, -1) + 2.0 * p(1, 0) + p(1, 1) - p(-1, -1) - 2.0 * p(-1, 0) - p(-1, 1)) / 8.0;
            let gy =
                (p(-1, 1) + 2.0 * p(0, 1) + p(1, 1) - p(-1, -1) - 2.0 * p(0, -1) - p(1, -1)) / 8.0;
            let i = y * w + x;
            mag[i] = gx.hypot(gy);
            // The direction along the edge, with the bright side on the left
            // (so edges of opposite contrast don't join).
            angle[i] = gx.atan2(-gy);
        }
    }

    // Seeds in order of falling gradient (bucketed).
    let threshold = k.gradient as f32;
    let top = mag.iter().copied().fold(0.0f32, f32::max);
    if top <= threshold {
        return Vec::new();
    }
    const BINS: usize = 1024;
    let mut buckets: Vec<Vec<u32>> = vec![Vec::new(); BINS];
    for (i, &m) in mag.iter().enumerate() {
        if m > threshold {
            let b = (((m - threshold) / (top - threshold)) * (BINS - 1) as f32) as usize;
            buckets[BINS - 1 - b.min(BINS - 1)].push(i as u32);
        }
    }
    let tolerance = (k.direction_tolerance as f32).to_radians();
    let min_length = k.min_length * w.max(h) as f64;
    let mut used = vec![false; n];
    let mut region: Vec<usize> = Vec::new();
    let mut out = Vec::new();
    for seed in buckets.iter().flatten() {
        let seed = *seed as usize;
        if used[seed] {
            continue;
        }
        // Grow a region of pixels whose edge direction is within the
        // tolerance of the region's.
        region.clear();
        region.push(seed);
        used[seed] = true;
        let (mut sx, mut sy) = (angle[seed].cos(), angle[seed].sin());
        let mut region_angle = angle[seed];
        let mut next = 0;
        while next < region.len() {
            let i = region[next];
            next += 1;
            let (x, y) = (i % w, i / w);
            for yy in y - 1..=y + 1 {
                for xx in x - 1..=x + 1 {
                    let j = yy * w + xx;
                    if used[j] || mag[j] <= threshold {
                        continue;
                    }
                    let mut d = (angle[j] - region_angle).abs();
                    if d > std::f32::consts::PI {
                        d = 2.0 * std::f32::consts::PI - d;
                    }
                    if d < tolerance {
                        used[j] = true;
                        region.push(j);
                        sx += angle[j].cos();
                        sy += angle[j].sin();
                        region_angle = sy.atan2(sx);
                    }
                }
            }
        }
        if (region.len() as f64) < 0.5 * min_length {
            continue;
        }
        if let Some(s) = fit_segment(&region, &mag, w, min_length) {
            out.push(s);
        }
    }
    // Normalised coordinates.
    let d = 0.5 * (w as f64).hypot(h as f64);
    let (cx, cy) = ((w as f64 - 1.0) / 2.0, (h as f64 - 1.0) / 2.0);
    let norm = |p: (f64, f64)| ((p.0 - cx) / d, (p.1 - cy) / d);
    out.into_iter()
        .map(|s: Segment| Segment {
            a: norm(s.a),
            b: norm(s.b),
        })
        .collect()
}

/// Separable Gaussian blur, σ = 1 pixel.
fn blur(v: &[f32], w: usize, h: usize) -> Vec<f32> {
    const K: [f32; 5] = [0.054, 0.244, 0.403, 0.244, 0.054];
    let mut tmp = vec![0.0f32; w * h];
    for y in 0..h {
        for x in 0..w {
            tmp[y * w + x] = (0..5)
                .map(|i| K[i] * v[y * w + (x + i).saturating_sub(2).min(w - 1)])
                .sum();
        }
    }
    let mut out = vec![0.0f32; w * h];
    for y in 0..h {
        for x in 0..w {
            out[y * w + x] = (0..5)
                .map(|i| K[i] * tmp[(y + i).saturating_sub(2).min(h - 1) * w + x])
                .sum();
        }
    }
    out
}

/// The segment a region of edge pixels stands for, in pixels: its
/// gradient-weighted principal axis, kept if it is long, thin and well
/// filled.
fn fit_segment(region: &[usize], mag: &[f32], w: usize, min_length: f64) -> Option<Segment> {
    let (mut sw, mut mx, mut my) = (0.0f64, 0.0f64, 0.0f64);
    for &i in region {
        let m = mag[i] as f64;
        sw += m;
        mx += m * (i % w) as f64;
        my += m * (i / w) as f64;
    }
    let (mx, my) = (mx / sw, my / sw);
    let (mut sxx, mut syy, mut sxy) = (0.0f64, 0.0f64, 0.0f64);
    for &i in region {
        let m = mag[i] as f64;
        let (dx, dy) = ((i % w) as f64 - mx, (i / w) as f64 - my);
        sxx += m * dx * dx;
        syy += m * dy * dy;
        sxy += m * dx * dy;
    }
    let phi = 0.5 * (2.0 * sxy).atan2(sxx - syy);
    let (ux, uy) = (phi.cos(), phi.sin());
    let (mut t0, mut t1, mut s0, mut s1) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
    for &i in region {
        let (dx, dy) = ((i % w) as f64 - mx, (i / w) as f64 - my);
        let t = dx * ux + dy * uy;
        let s = -dx * uy + dy * ux;
        t0 = t0.min(t);
        t1 = t1.max(t);
        s0 = s0.min(s);
        s1 = s1.max(s);
    }
    let length = t1 - t0 + 1.0;
    let width = s1 - s0 + 1.0;
    let density = region.len() as f64 / (length * width);
    (length >= min_length && width <= (0.15 * length).max(3.0) && density >= 0.4).then_some(
        Segment {
            a: (mx + t0 * ux, my + t0 * uy),
            b: (mx + t1 * ux, my + t1 * uy),
        },
    )
}

#[derive(Clone, Copy, PartialEq)]
enum Axis {
    Vertical,
    Horizontal,
}

/// The correction that makes `segments` vertical and horizontal in `mode`,
/// for a camera of focal length `focal` half diagonals.
pub fn solve(
    segments: &[Segment],
    mode: UprightMode,
    focal: f64,
    k: &UprightStrengths,
) -> Option<Keystone> {
    let candidate = k.candidate.to_radians().sin();
    let lines: Vec<(Segment, Axis, f64)> = segments
        .iter()
        .filter_map(|s| {
            let len = s.length();
            if len <= 0.0 {
                return None;
            }
            let (ux, uy) = ((s.b.0 - s.a.0) / len, (s.b.1 - s.a.1) / len);
            if ux.abs() < candidate {
                Some((*s, Axis::Vertical, len))
            } else if uy.abs() < candidate {
                Some((*s, Axis::Horizontal, len))
            } else {
                None
            }
        })
        .collect();
    // How much each kind of line counts in each mode.
    let (wv, wh) = match mode {
        UprightMode::Level => (0.5, 1.0),
        UprightMode::Vertical => (1.0, 0.3),
        UprightMode::Full | UprightMode::Auto => (1.0, 1.0),
        _ => return None,
    };
    let weight = |axis: Axis, len: f64| len * if axis == Axis::Vertical { wv } else { wh };
    let total: f64 = lines.iter().map(|(_, a, l)| weight(*a, *l)).sum();
    // Too little to go on: about two lines of a fifth of the frame.
    if total < 0.4 {
        return None;
    }
    let free = match mode {
        UprightMode::Level => [false, false, true],
        UprightMode::Vertical => [true, false, true],
        _ => [true, true, true],
    };
    let (damping, final_damping) = if mode == UprightMode::Auto {
        (k.auto_damping, k.auto_damping)
    } else {
        (k.tilt_damping, k.fit_damping)
    };
    let keystone = |p: &[f64; 3]| Keystone {
        alpha: p[0],
        beta: p[1],
        theta: p[2],
        focal,
    };
    let residual = |h: &Homography, s: &Segment, axis: Axis| -> Option<f64> {
        let a = h.apply(s.a.0, s.a.1);
        let b = h.apply(s.b.0, s.b.1);
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let len = dx.hypot(dy);
        if !(len.is_finite() && len > 0.0) {
            return None;
        }
        Some(if axis == Axis::Vertical { dx } else { dy } / len)
    };
    let limits = [
        k.max_tilt.to_radians(),
        k.max_tilt.to_radians(),
        k.max_roll.to_radians(),
    ];
    let cost = |p: &[f64; 3], inliers: Option<&[bool]>, robust: Option<f64>, damping: f64| -> f64 {
        if p.iter().zip(&limits).any(|(v, l)| v.abs() > *l) {
            return f64::MAX;
        }
        let Some(h) = keystone(p).matrix() else {
            return f64::MAX;
        };
        let mut sum = 0.0;
        let mut wsum = 0.0;
        for (i, (s, axis, len)) in lines.iter().enumerate() {
            if inliers.is_some_and(|m| !m[i]) {
                continue;
            }
            let Some(r) = residual(&h, s, *axis) else {
                return f64::MAX;
            };
            let w = weight(*axis, *len);
            sum += w * match robust {
                Some(c) => (r * r) / (r * r + c * c) * c * c,
                None => r * r,
            };
            wsum += w;
        }
        if wsum <= 0.0 {
            return f64::MAX;
        }
        sum / wsum + damping * (p[0] * p[0] + p[1] * p[1])
    };
    // 1. Robust fit from no correction.
    let c = (5.0f64).to_radians().sin();
    let first = nelder_mead(|p| cost(p, None, Some(c), damping), [0.0; 3], free, 0.03);
    // 2. Least squares on the lines that now fit.
    let h = keystone(&first).matrix()?;
    let tol = k.inlier.to_radians().sin();
    let inliers: Vec<bool> = lines
        .iter()
        .map(|(s, axis, _)| residual(&h, s, *axis).is_some_and(|r| r.abs() < tol))
        .collect();
    let kept: f64 = lines
        .iter()
        .zip(&inliers)
        .filter(|(_, i)| **i)
        .map(|((_, a, l), _)| weight(*a, *l))
        .sum();
    let best = if kept >= 0.25 {
        nelder_mead(
            |p| cost(p, Some(&inliers), None, final_damping),
            first,
            free,
            0.01,
        )
    } else {
        first
    };
    let found = keystone(&best);
    found.matrix().map(|_| found)
}

/// Guided Upright's result: the Transform sliders that make the guides
/// vertical or horizontal.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GuidedSliders {
    /// `PerspectiveVertical`, `PerspectiveHorizontal` (−100..100) and
    /// `PerspectiveRotate` (−10..10 degrees).
    pub vertical: f64,
    pub horizontal: f64,
    pub rotate: f64,
    /// The largest angle a guide is still off by afterwards, degrees.
    pub worst_degrees: f64,
}

/// Guided Upright (**ours**): `guides` are lines the user drew on the
/// lens-corrected frame, as fractions of a `width`×`height` frame. Each is
/// made vertical or horizontal, whichever it is closer to, by the manual
/// Transform sliders: Rotate always, Vertical when two or more guides are
/// vertical, Horizontal when two or more are horizontal. Lightroom stores
/// its own guides in a way we can't read, so the result is the sliders,
/// which Lightroom reads. `None` with no usable guide.
pub fn guided(
    guides: &[Segment],
    width: usize,
    height: usize,
    k: &crate::transform::TransformStrengths,
) -> Option<GuidedSliders> {
    use crate::transform::Transform;
    let (w, h) = (width as f64, height as f64);
    type Point = (f64, f64);
    let lines: Vec<(Point, Point, Axis)> = guides
        .iter()
        .map(|g| ((g.a.0 * w, g.a.1 * h), (g.b.0 * w, g.b.1 * h)))
        .filter(|(a, b)| (b.0 - a.0).hypot(b.1 - a.1) > 4.0)
        .map(|(a, b)| {
            let axis = if (b.1 - a.1).abs() >= (b.0 - a.0).abs() {
                Axis::Vertical
            } else {
                Axis::Horizontal
            };
            (a, b, axis)
        })
        .collect();
    if lines.is_empty() || width == 0 || height == 0 {
        return None;
    }
    let count = |axis: Axis| lines.iter().filter(|l| l.2 == axis).count();
    let free = [
        count(Axis::Vertical) >= 2,
        count(Axis::Horizontal) >= 2,
        true,
    ];
    // p: Vertical / 100, Horizontal / 100, Rotate / 10.
    let sliders = |p: &[f64; 3]| Transform {
        vertical: p[0] * 100.0,
        horizontal: p[1] * 100.0,
        rotate: p[2] * 10.0,
        ..Transform::default()
    };
    let errors = |p: &[f64; 3]| -> Option<Vec<f64>> {
        let m = sliders(p).forward(width, height, k)?;
        lines
            .iter()
            .map(|(a, b, axis)| {
                let (pa, pb) = (m.apply(a.0, a.1), m.apply(b.0, b.1));
                let (dx, dy) = (pb.0 - pa.0, pb.1 - pa.1);
                let len = dx.hypot(dy);
                (len.is_finite() && len > 0.0)
                    .then(|| if *axis == Axis::Vertical { dx } else { dy } / len)
            })
            .collect()
    };
    let cost = |p: &[f64; 3]| -> f64 {
        if p.iter().any(|v| v.abs() > 1.0) {
            return f64::MAX;
        }
        match errors(p) {
            // A little pull towards no change settles what the guides don't.
            Some(e) => {
                e.iter().map(|r| r * r).sum::<f64>() + 1e-6 * p.iter().map(|v| v * v).sum::<f64>()
            }
            None => f64::MAX,
        }
    };
    let mut best = nelder_mead(cost, [0.0; 3], free, 0.05);
    // Restart once from the result, for a tighter fit.
    best = nelder_mead(cost, best, free, 0.01);
    let worst = errors(&best)?
        .iter()
        .map(|r| r.abs().min(1.0).asin().to_degrees())
        .fold(0.0, f64::max);
    let t = sliders(&best);
    let round = |v: f64, step: f64| (v / step).round() * step;
    Some(GuidedSliders {
        vertical: round(t.vertical, 1.0),
        horizontal: round(t.horizontal, 1.0),
        rotate: round(t.rotate, 0.01),
        worst_degrees: worst,
    })
}

/// Minimise `f` over the `free` coordinates from `start` (simplex steps of
/// `step`).
fn nelder_mead(
    f: impl Fn(&[f64; 3]) -> f64,
    start: [f64; 3],
    free: [bool; 3],
    step: f64,
) -> [f64; 3] {
    let dims: Vec<usize> = (0..3).filter(|i| free[*i]).collect();
    let mut simplex: Vec<([f64; 3], f64)> = vec![(start, f(&start))];
    for &d in &dims {
        let mut p = start;
        p[d] += step;
        simplex.push((p, f(&p)));
    }
    let n = dims.len();
    for _ in 0..400 {
        simplex.sort_by(|a, b| a.1.total_cmp(&b.1));
        let spread = simplex[n].1 - simplex[0].1;
        let size = simplex
            .iter()
            .map(|(p, _)| {
                dims.iter()
                    .map(|&d| (p[d] - simplex[0].0[d]).abs())
                    .fold(0.0, f64::max)
            })
            .fold(0.0, f64::max);
        if size < 1e-6 || (spread.abs() < 1e-14 && size < 1e-4) {
            break;
        }
        let mut centroid = [0.0; 3];
        for (p, _) in &simplex[..n] {
            for d in 0..3 {
                centroid[d] += p[d] / n as f64;
            }
        }
        let worst = simplex[n];
        let along = |t: f64| {
            let mut p = centroid;
            for d in 0..3 {
                p[d] = centroid[d] + t * (worst.0[d] - centroid[d]);
            }
            p
        };
        let r = along(-1.0);
        let fr = f(&r);
        if fr < simplex[0].1 {
            let e = along(-2.0);
            let fe = f(&e);
            simplex[n] = if fe < fr { (e, fe) } else { (r, fr) };
        } else if fr < simplex[n - 1].1 {
            simplex[n] = (r, fr);
        } else {
            let c = if fr < worst.1 {
                along(-0.5)
            } else {
                along(0.5)
            };
            let fc = f(&c);
            if fc < worst.1.min(fr) {
                simplex[n] = (c, fc);
            } else {
                let best = simplex[0].0;
                for item in simplex.iter_mut().skip(1) {
                    let mut p = best;
                    for d in 0..3 {
                        p[d] = best[d] + 0.5 * (item.0[d] - best[d]);
                    }
                    *item = (p, f(&p));
                }
            }
        }
    }
    simplex.sort_by(|a, b| a.1.total_cmp(&b.1));
    simplex[0].0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guided_finds_the_sliders_that_straighten_the_guides() {
        use crate::transform::{DEFAULT_STRENGTHS as TK, Transform};
        let (w, h) = (3000usize, 2000usize);
        let truth = Transform {
            vertical: -40.0,
            rotate: 3.0,
            ..Transform::default()
        };
        // Lines that are vertical (and one horizontal) after `truth`, drawn
        // where they are on the frame before it.
        let back = truth.forward(w, h, &TK).unwrap().inverse().unwrap();
        let guide = |a: (f64, f64), b: (f64, f64)| {
            let (pa, pb) = (back.apply(a.0, a.1), back.apply(b.0, b.1));
            Segment {
                a: (pa.0 / w as f64, pa.1 / h as f64),
                b: (pb.0 / w as f64, pb.1 / h as f64),
            }
        };
        let guides = [
            guide((600.0, 300.0), (600.0, 1700.0)),
            guide((2300.0, 200.0), (2300.0, 1800.0)),
        ];
        let g = guided(&guides, w, h, &TK).unwrap();
        assert!((g.vertical + 40.0).abs() <= 1.0, "{g:?}");
        assert!(g.horizontal == 0.0, "{g:?}");
        assert!((g.rotate - 3.0).abs() < 0.05, "{g:?}");
        assert!(g.worst_degrees < 0.1, "{g:?}");
        // One guide only turns the photo.
        let one = guided(&guides[..1], w, h, &TK).unwrap();
        assert_eq!(one.vertical, 0.0);
        assert!(one.worst_degrees < 0.1, "{one:?}");
        assert!(guided(&[], w, h, &TK).is_none());
    }

    const K: UprightStrengths = DEFAULT_STRENGTHS;
    const F: f64 = 35.0 / 21.6335;

    /// What a pinhole camera turned by `alpha` (about x), `beta` (about y)
    /// and `roll` (degrees) sees of a scene in front of it: H = K·R·K⁻¹.
    fn camera(alpha: f64, beta: f64, roll: f64) -> Homography {
        let (sa, ca) = alpha.to_radians().sin_cos();
        let (sb, cb) = beta.to_radians().sin_cos();
        let rx = Homography([[1.0, 0.0, 0.0], [0.0, ca, -sa], [0.0, sa, ca]]);
        let ry = Homography([[cb, 0.0, sb], [0.0, 1.0, 0.0], [-sb, 0.0, cb]]);
        let kk = Homography([[F, 0.0, 0.0], [0.0, F, 0.0], [0.0, 0.0, 1.0]]);
        let ki = Homography([[1.0 / F, 0.0, 0.0], [0.0, 1.0 / F, 0.0], [0.0, 0.0, 1.0]]);
        crate::transform::rotation(roll.to_radians()).mul(&kk.mul(&ry.mul(&rx)).mul(&ki))
    }

    /// Lines of a building seen through `distort` (normalised coordinates).
    fn building(distort: &Homography) -> Vec<Segment> {
        let mut out = Vec::new();
        for i in -4..=4 {
            let x = i as f64 * 0.15;
            out.push(Segment {
                a: distort.apply(x, -0.5),
                b: distort.apply(x, 0.5),
            });
        }
        for j in -3..=3 {
            let y = j as f64 * 0.12;
            out.push(Segment {
                a: distort.apply(-0.6, y),
                b: distort.apply(0.6, y),
            });
        }
        // A diagonal roof line and a short clutter line, to be ignored.
        out.push(Segment {
            a: distort.apply(-0.6, -0.5),
            b: distort.apply(0.0, -0.8),
        });
        out
    }

    fn angles_after(k: &Keystone, segs: &[Segment]) -> (f64, f64) {
        let h = k.matrix().unwrap();
        let (mut worst_v, mut worst_h) = (0.0f64, 0.0f64);
        for s in segs {
            let a = h.apply(s.a.0, s.a.1);
            let b = h.apply(s.b.0, s.b.1);
            let (dx, dy) = (b.0 - a.0, b.1 - a.1);
            let len = dx.hypot(dy);
            if (dx / len).abs() < 0.4 {
                worst_v = worst_v.max((dx / len).abs());
            } else if (dy / len).abs() < 0.4 {
                worst_h = worst_h.max((dy / len).abs());
            }
        }
        (worst_v.asin().to_degrees(), worst_h.asin().to_degrees())
    }

    #[test]
    fn full_undoes_a_tilted_and_turned_camera() {
        // A camera tilted up 12°, turned 6° and rolled 3°.
        let distort = camera(12.0, -6.0, 3.0);
        let segs = building(&distort);
        let (v0, h0) = angles_after(
            &Keystone {
                alpha: 0.0,
                beta: 0.0,
                theta: 0.0,
                focal: F,
            },
            &segs,
        );
        assert!(v0 > 5.0 && h0 > 3.0, "{v0} {h0}");
        let k = solve(&segs, UprightMode::Full, F, &K).expect("a fit");
        let (v, h) = angles_after(&k, &segs);
        assert!(v < 0.1 && h < 0.1, "{v} {h} {k:?}");
    }

    #[test]
    fn level_only_turns_and_vertical_leaves_horizontal_tilt() {
        let distort = camera(10.0, 0.0, 4.0);
        let segs = building(&distort);
        let level = solve(&segs, UprightMode::Level, F, &K).unwrap();
        assert_eq!((level.alpha, level.beta), (0.0, 0.0));
        assert!((level.theta.to_degrees() + 4.0).abs() < 1.0, "{level:?}");
        let vertical = solve(&segs, UprightMode::Vertical, F, &K).unwrap();
        assert_eq!(vertical.beta, 0.0);
        let (v, _) = angles_after(&vertical, &segs);
        assert!(v < 0.2, "{v}");
        // Auto corrects less than Full.
        let auto = solve(&segs, UprightMode::Auto, F, &K).unwrap();
        let full = solve(&segs, UprightMode::Full, F, &K).unwrap();
        assert!(
            auto.alpha.abs() < full.alpha.abs() && auto.alpha.abs() > 0.3 * full.alpha.abs(),
            "{auto:?} {full:?}"
        );
    }

    #[test]
    fn too_few_lines_mean_no_correction() {
        let segs = [Segment {
            a: (0.0, -0.1),
            b: (0.01, 0.1),
        }];
        assert!(solve(&segs, UprightMode::Full, F, &K).is_none());
        assert!(solve(&[], UprightMode::Off, F, &K).is_none());
    }

    #[test]
    fn detector_finds_the_edges_of_a_drawn_box() {
        // A light box with a 5° turn on a dark ground.
        let (w, h) = (400, 300);
        let (s, c) = 5f64.to_radians().sin_cos();
        let v: Vec<f32> = (0..w * h)
            .map(|i| {
                let (x, y) = ((i % w) as f64 - 200.0, (i / w) as f64 - 150.0);
                let (u, t) = (x * c + y * s, -x * s + y * c);
                if u.abs() < 120.0 && t.abs() < 80.0 {
                    0.8
                } else {
                    0.1
                }
            })
            .collect();
        let segs = detect(&v, w, h, None, &K);
        assert!(segs.len() >= 4, "{segs:?}");
        let d = 0.5 * (w as f64).hypot(h as f64);
        let long = segs.iter().filter(|s| s.length() * d > 140.0).count();
        assert!(long >= 4, "{segs:?}");
        for seg in &segs {
            let (dx, dy) = (seg.b.0 - seg.a.0, seg.b.1 - seg.a.1);
            let ang = dy.atan2(dx).to_degrees().rem_euclid(90.0);
            assert!((ang - 5.0).abs() < 1.0, "{ang} {seg:?}");
        }
    }
}
