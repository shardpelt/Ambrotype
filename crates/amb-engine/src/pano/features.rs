// SPDX-License-Identifier: GPL-3.0-or-later
//! Feature points for stitching: Harris corners spread over the frame by
//! adaptive non-maximal suppression, each with an oriented 8×8 patch
//! descriptor (Brown, Szeliski & Winder, 2005, "Multi-Image Matching using
//! Multi-Scale Oriented Patches"), matched by nearest neighbour with a ratio
//! test.

use rayon::prelude::*;

use crate::filters::gaussian_blur;
use crate::raw::CameraImage;

/// Descriptor length: 8 × 8 samples.
pub const DESC: usize = 64;

#[derive(Debug, Clone)]
pub struct Feature {
    /// Position in full-resolution pixels of the frame.
    pub x: f64,
    pub y: f64,
    /// Bias- and gain-normalised samples.
    pub desc: [f32; DESC],
}

/// Spacing of descriptor samples, in detection pixels (a 40-pixel window).
const SPACING: f32 = 5.0;
/// Corners closer than this to the border have no full window.
const BORDER: usize = 30;

/// A grey (gamma-encoded luminance) copy of `img` at most `max_side`
/// pixels on its long side, with its downscale factor.
pub fn grey(img: &CameraImage, max_side: usize) -> (Vec<f32>, usize, usize, usize) {
    let long = img.width.max(img.height);
    let s = long.div_ceil(max_side.max(1)).max(1);
    let (w, h) = (img.width / s, img.height / s);
    let n = (s * s) as f32;
    let g = (0..w * h)
        .into_par_iter()
        .map(|k| {
            let (x, y) = (k % w * s, k / w * s);
            let mut acc = 0.0;
            for dy in 0..s {
                let row = &img.rgb[((y + dy) * img.width + x) * 3..];
                for p in row[..s * 3].chunks_exact(3) {
                    acc += 0.25 * p[0] + 0.5 * p[1] + 0.25 * p[2];
                }
            }
            (acc / n).max(0.0).sqrt()
        })
        .collect();
    (g, w, h, s)
}

fn gradients(g: &[f32], w: usize, h: usize) -> (Vec<f32>, Vec<f32>) {
    let at = |x: usize, y: usize| g[y.min(h - 1) * w + x.min(w - 1)];
    (0..w * h)
        .into_par_iter()
        .map(|k| {
            let (x, y) = (k % w, k / w);
            let gx = 0.5 * (at(x + 1, y) - at(x.saturating_sub(1), y));
            let gy = 0.5 * (at(x, y + 1) - at(x, y.saturating_sub(1)));
            (gx, gy)
        })
        .unzip()
}

/// Bilinear sample of a plane (clamped).
fn sample(p: &[f32], w: usize, h: usize, x: f32, y: f32) -> f32 {
    let x = x.clamp(0.0, (w - 1) as f32);
    let y = y.clamp(0.0, (h - 1) as f32);
    let (x0, y0) = (x as usize, y as usize);
    let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    let top = p[y0 * w + x0] + (p[y0 * w + x1] - p[y0 * w + x0]) * fx;
    let bottom = p[y1 * w + x0] + (p[y1 * w + x1] - p[y1 * w + x0]) * fx;
    top + (bottom - top) * fy
}

/// Up to `count` features of `img`, found on a copy at most `max_side` pixels
/// on its long side.
pub fn detect(img: &CameraImage, max_side: usize, count: usize) -> Vec<Feature> {
    let (g, w, h, s) = grey(img, max_side);
    if w < 2 * BORDER + 4 || h < 2 * BORDER + 4 {
        return Vec::new();
    }
    // Harris: the harmonic mean of the structure tensor's eigenvalues.
    let smooth = gaussian_blur(&g, w, h, 1.0);
    let (gx, gy) = gradients(&smooth, w, h);
    let xx: Vec<f32> = gx.par_iter().map(|v| v * v).collect();
    let yy: Vec<f32> = gy.par_iter().map(|v| v * v).collect();
    let xy: Vec<f32> = gx.par_iter().zip(&gy).map(|(a, b)| a * b).collect();
    let (xx, yy, xy) = (
        gaussian_blur(&xx, w, h, 1.5),
        gaussian_blur(&yy, w, h, 1.5),
        gaussian_blur(&xy, w, h, 1.5),
    );
    let response: Vec<f32> = (0..w * h)
        .into_par_iter()
        .map(|k| {
            let det = xx[k] * yy[k] - xy[k] * xy[k];
            let tr = xx[k] + yy[k];
            if tr > 1e-12 { det / tr } else { 0.0 }
        })
        .collect();
    let peak = response.iter().copied().fold(0.0f32, f32::max);
    if peak <= 0.0 {
        return Vec::new();
    }
    // Local maxima (3×3) above a small share of the strongest.
    let floor = peak * 1e-3;
    let mut candidates: Vec<(f32, usize, usize)> = (BORDER..h - BORDER)
        .into_par_iter()
        .flat_map_iter(|y| {
            let r = &response;
            (BORDER..w - BORDER).filter_map(move |x| {
                let v = r[y * w + x];
                if v <= floor {
                    return None;
                }
                for dy in [-1i64, 0, 1] {
                    for dx in [-1i64, 0, 1] {
                        if (dx, dy) != (0, 0) {
                            let n = r[(y as i64 + dy) as usize * w + (x as i64 + dx) as usize];
                            if n > v || (n == v && (dy < 0 || (dy == 0 && dx < 0))) {
                                return None;
                            }
                        }
                    }
                }
                Some((v, x, y))
            })
        })
        .collect();
    candidates.sort_by(|a, b| b.0.total_cmp(&a.0));
    candidates.truncate(count * 5);
    // Adaptive non-maximal suppression: keep the corners that are the
    // strongest over the widest radius, so they spread over the frame.
    let radius: Vec<f32> = (0..candidates.len())
        .into_par_iter()
        .map(|i| {
            let (v, x, y) = candidates[i];
            candidates[..i]
                .iter()
                .filter(|c| v < 0.9 * c.0)
                .map(|c| {
                    let (dx, dy) = (c.1 as f32 - x as f32, c.2 as f32 - y as f32);
                    dx * dx + dy * dy
                })
                .fold(f32::INFINITY, f32::min)
        })
        .collect();
    let mut order: Vec<usize> = (0..candidates.len()).collect();
    order.sort_by(|&a, &b| radius[b].total_cmp(&radius[a]));
    order.truncate(count);

    let orient = gaussian_blur(&g, w, h, 4.5);
    let (ox, oy) = gradients(&orient, w, h);
    let patch = gaussian_blur(&g, w, h, 2.0);
    order
        .par_iter()
        .filter_map(|&i| {
            let (_, x, y) = candidates[i];
            // Sub-pixel peak (parabola through the neighbours).
            let r = |xx: usize, yy: usize| response[yy * w + xx];
            let vertex = |m: f32, c: f32, p: f32| {
                let d = m - 2.0 * c + p;
                if d < 0.0 {
                    (0.5 * (m - p) / d).clamp(-0.5, 0.5)
                } else {
                    0.0
                }
            };
            let sx = x as f32 + vertex(r(x - 1, y), r(x, y), r(x + 1, y));
            let sy = y as f32 + vertex(r(x, y - 1), r(x, y), r(x, y + 1));
            let (dx, dy) = (ox[y * w + x], oy[y * w + x]);
            let len = dx.hypot(dy);
            let (c, sn) = if len > 1e-12 {
                (dx / len, dy / len)
            } else {
                (1.0, 0.0)
            };
            let mut desc = [0.0f32; DESC];
            for (k, d) in desc.iter_mut().enumerate() {
                let (u, v) = (
                    ((k % 8) as f32 - 3.5) * SPACING,
                    ((k / 8) as f32 - 3.5) * SPACING,
                );
                *d = sample(&patch, w, h, sx + c * u - sn * v, sy + sn * u + c * v);
            }
            let mean = desc.iter().sum::<f32>() / DESC as f32;
            let var = desc.iter().map(|v| (v - mean) * (v - mean)).sum::<f32>() / DESC as f32;
            if var <= 1e-12 {
                return None;
            }
            let inv = 1.0 / var.sqrt();
            desc.iter_mut().for_each(|v| *v = (*v - mean) * inv);
            Some(Feature {
                x: (f64::from(sx) + 0.5) * s as f64 - 0.5,
                y: (f64::from(sy) + 0.5) * s as f64 - 0.5,
                desc,
            })
        })
        .collect()
}

/// Matches from `a` to `b`: each feature's nearest neighbour in `b`, kept
/// when it is clearly nearer than the second nearest (Lowe's ratio test)
/// and no other feature of `a` claims the same neighbour.
pub fn match_features(a: &[Feature], b: &[Feature]) -> Vec<(usize, usize)> {
    if b.len() < 2 {
        return Vec::new();
    }
    let dist = |p: &[f32; DESC], q: &[f32; DESC]| -> f32 {
        p.iter().zip(q).map(|(x, y)| (x - y) * (x - y)).sum()
    };
    let best: Vec<Option<(usize, f32)>> = a
        .par_iter()
        .map(|fa| {
            let (mut d1, mut d2, mut i1) = (f32::INFINITY, f32::INFINITY, 0);
            for (j, fb) in b.iter().enumerate() {
                let d = dist(&fa.desc, &fb.desc);
                if d < d1 {
                    (d2, d1, i1) = (d1, d, j);
                } else if d < d2 {
                    d2 = d;
                }
            }
            (d1 < 0.64 * d2).then_some((i1, d1))
        })
        .collect();
    // One feature of `a` per feature of `b`: the closest.
    let mut claim: Vec<Option<(usize, f32)>> = vec![None; b.len()];
    for (i, m) in best.iter().enumerate() {
        if let Some((j, d)) = *m {
            if claim[j].is_none_or(|(_, e)| d < e) {
                claim[j] = Some((i, d));
            }
        }
    }
    claim
        .iter()
        .enumerate()
        .filter_map(|(j, c)| c.map(|(i, _)| (i, j)))
        .collect()
}
