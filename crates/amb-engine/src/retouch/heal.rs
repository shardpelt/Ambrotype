// SPDX-License-Identifier: GPL-3.0-or-later
//! The clone and heal maths: finding where a spot's donor lines up, and blending it in.

use super::{Placed, sample};
use crate::buffer::{Image, luminance_weights};
use crate::filters::box_blur;

/// The donor offset (output pixels) whose surroundings best match the
/// spot's, for the spot's own shape (a round spot or a brushed stroke).
/// Candidates lie in 24 directions, just clear of the spot along each one and
/// further out; each is scored on the band around the spot: how far the
/// donor's band differs from the spot's (log luminance; for a heal, which
/// evens out brightness, mostly in shape, a little in level), plus how far
/// the donor's inside strays from what its own band predicts (a donor
/// holding an object or a line does), how different its fine texture is from
/// the band's, and its distance. A donor may not overlap the spot.
pub(super) fn search_offset(
    img: &Image,
    p: &Placed,
    clone: bool,
    (x0, y0, x1, y1): (f64, f64, f64, f64),
) -> (f64, f64) {
    let margin = (p.radius * 0.35).clamp(3.0, 40.0) as usize;
    let (ex0, ey0) = (p.x0.saturating_sub(margin), p.y0.saturating_sub(margin));
    let ex1 = (p.x0 + p.w + margin).min(img.width);
    let ey1 = (p.y0 + p.h + margin).min(img.height);
    let (ew, eh) = (ex1 - ex0, ey1 - ey0);
    let mut mask = vec![0.0f32; ew * eh];
    for j in 0..p.h {
        for i in 0..p.w {
            if p.alpha[j * p.w + i] > 0.01 {
                mask[(p.y0 - ey0 + j) * ew + (p.x0 - ex0 + i)] = 1.0;
            }
        }
    }
    let near = box_blur(&mask, ew, eh, margin);
    let (mut band, mut inside) = (Vec::new(), Vec::new());
    for j in 0..eh {
        for i in 0..ew {
            let k = j * ew + i;
            let pt = ((ex0 + i) as f64, (ey0 + j) as f64);
            if mask[k] > 0.0 {
                inside.push(pt);
            } else if near[k] > 0.0 {
                band.push(pt);
            }
        }
    }
    // A few hundred points each is plenty to compare.
    let thin = |v: Vec<(f64, f64)>, n: usize| -> Vec<(f64, f64)> {
        let step = v.len().div_ceil(n).max(1);
        v.into_iter().step_by(step).collect()
    };
    let (band, inside) = (thin(band, 600), thin(inside, 400));
    if band.is_empty() || inside.is_empty() {
        return (p.radius.max(1.0) * 2.2, 0.0);
    }
    // Log luminance with a floor, as the eye sees it: plain logs blow the
    // noise of near-black areas up into differences that swamp the rest.
    let lw = luminance_weights();
    let luma = |pts: &[(f64, f64)], (dx, dy): (f64, f64)| -> Vec<f64> {
        pts.iter()
            .map(|&(x, y)| {
                let p = sample(img, x + dx, y + dy);
                ((lw[0] * p[0] + lw[1] * p[1] + lw[2] * p[2]).max(0.0) as f64 + 0.005).ln()
            })
            .collect()
    };
    let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len() as f64;
    let var = |v: &[f64]| {
        let m = mean(v);
        v.iter().map(|x| (x - m).powi(2)).sum::<f64>() / v.len() as f64
    };
    // Fine texture: the difference to the pixel two to the right.
    let fine = |pts: &[(f64, f64)], (dx, dy): (f64, f64)| -> f64 {
        let a = luma(pts, (dx, dy));
        let b = luma(pts, (dx + 2.0, dy));
        (a.iter().zip(&b).map(|(a, b)| (a - b).powi(2)).sum::<f64>() / a.len() as f64).sqrt()
    };
    let target = luma(&band, (0.0, 0.0));
    let target_fine = fine(&band, (0.0, 0.0));
    // How each inside point is guessed from the band (inverse squared
    // distance weights); the same for every candidate, as both move together.
    let guess: Vec<Vec<f32>> = inside
        .iter()
        .map(|&(x, y)| {
            let w: Vec<f64> = band
                .iter()
                .map(|&(bx, by)| 1.0 / ((bx - x).powi(2) + (by - y).powi(2)).max(1.0))
                .collect();
            let total: f64 = w.iter().sum();
            w.iter().map(|v| (v / total) as f32).collect()
        })
        .collect();
    let in_mask = |x: f64, y: f64| {
        let (i, j) = (x.round() - ex0 as f64, y.round() - ey0 as f64);
        i >= 0.0
            && j >= 0.0
            && (i as usize) < ew
            && (j as usize) < eh
            && mask[j as usize * ew + i as usize] > 0.0
    };
    // The spot's width along each direction, so a donor clears it.
    let clearance: Vec<(f64, f64, f64)> = (0..24)
        .map(|d| {
            let t = d as f64 / 24.0 * std::f64::consts::TAU;
            let (ux, uy) = (t.cos(), t.sin());
            let proj = inside.iter().map(|&(x, y)| x * ux + y * uy);
            let (lo, hi) = proj.fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), v| {
                (a.min(v), b.max(v))
            });
            (ux, uy, hi - lo + margin as f64)
        })
        .collect();
    // Distance counts in the spot's narrowest width: beside a long stroke
    // is near, along it is far.
    let thinnest = clearance
        .iter()
        .map(|c| c.2)
        .fold(f64::INFINITY, f64::min)
        .max(1.0);
    let mut best = ((p.radius.max(1.0) * 2.2, 0.0), f64::INFINITY);
    for &(ux, uy, clear) in &clearance {
        for k in [1.0, 1.35, 1.8, 2.5] {
            let off = ((ux * clear * k).round(), (uy * clear * k).round());
            // The donor and its band must be in the picture.
            let (bx0, by0, bx1, by1) = (
                ex0 as f64 + off.0,
                ey0 as f64 + off.1,
                ex1 as f64 + off.0,
                ey1 as f64 + off.1,
            );
            if bx0 < x0 || by0 < y0 || bx1 > x1 || by1 > y1 {
                continue;
            }
            let overlap = inside
                .iter()
                .filter(|&&(x, y)| in_mask(x + off.0, y + off.1))
                .count();
            if overlap * 50 > inside.len() {
                continue;
            }
            let around = luma(&band, off);
            let diff: Vec<f64> = around.iter().zip(&target).map(|(a, t)| a - t).collect();
            let shape = if clone {
                diff.iter().map(|e| e * e).sum::<f64>() / diff.len() as f64
            } else {
                // The heal evens out brightness, but brightening a much
                // darker donor brings up its noise and faint lines with it.
                var(&diff) + 0.5 * mean(&diff).powi(2)
            };
            // Something in the donor that its own band doesn't explain (an
            // object, a streak) would be carried into the spot: how far its
            // inside is from what its band predicts there.
            let inner = luma(&inside, off);
            let structure = inner
                .iter()
                .zip(&guess)
                .map(|(v, w)| {
                    let g: f64 = w.iter().zip(&around).map(|(&w, a)| w as f64 * a).sum();
                    (v - g).powi(2)
                })
                .sum::<f64>()
                / inner.len() as f64;
            let texture = (fine(&inside, off) - target_fine).powi(2);
            // Near donors are likelier to match: the scene changes with
            // distance, which the band alone may not show.
            let far = 0.004 * (off.0.hypot(off.1) / thinnest);
            let score = shape + structure + texture + far;
            if score < best.1 {
                best = (off, score);
            }
        }
    }
    best.0
}

pub(super) fn heal_one(img: &mut Image, p: &Placed, offset: (f64, f64), clone: bool) {
    // Whole pixels: a fractional offset would blur the donor's texture.
    let offset = (offset.0.round(), offset.1.round());
    let snapshot = img.clone();
    let field = if clone {
        None
    } else {
        Some(correction(&snapshot, p, offset))
    };
    for j in 0..p.h {
        for i in 0..p.w {
            let a = p.alpha[j * p.w + i];
            if a <= 0.0 {
                continue;
            }
            let (x, y) = ((p.x0 + i) as f64, (p.y0 + j) as f64);
            let mut d = sample(&snapshot, x + offset.0, y + offset.1);
            if let Some((f, fw, (fx0, fy0))) = &field {
                let k = (p.y0 + j - fy0) * fw + (p.x0 + i - fx0);
                for c in 0..3 {
                    d[c] *= f[c][k].clamp(-6.0, 6.0).exp();
                }
            }
            let t = snapshot.get(p.x0 + i, p.y0 + j);
            let out = [0, 1, 2].map(|c| t[c] + (d[c] - t[c]) * a);
            img.set(p.x0 + i, p.y0 + j, out);
        }
    }
}

/// The heal's correction, log(target) − log(donor) per channel, over the
/// spot's box and a margin: measured in a band just outside the spot,
/// smoothed there, and carried smoothly across the inside (a membrane, by
/// push-pull interpolation). Brightness, colour and curved shading carry
/// over from the edge all round; the donor's texture stays. Returns the
/// three planes, their width and their origin.
#[allow(clippy::type_complexity)]
fn correction(
    snapshot: &Image,
    p: &Placed,
    offset: (f64, f64),
) -> ([Vec<f32>; 3], usize, (usize, usize)) {
    let margin = (p.radius * 0.35).clamp(3.0, 40.0) as usize;
    let (ex0, ey0) = (p.x0.saturating_sub(margin), p.y0.saturating_sub(margin));
    let ex1 = (p.x0 + p.w + margin).min(snapshot.width);
    let ey1 = (p.y0 + p.h + margin).min(snapshot.height);
    let (ew, eh) = (ex1 - ex0, ey1 - ey0);
    let mut inside = vec![0.0f32; ew * eh];
    for j in 0..p.h {
        for i in 0..p.w {
            if p.alpha[j * p.w + i] > 0.01 {
                inside[(p.y0 - ey0 + j) * ew + (p.x0 - ex0 + i)] = 1.0;
            }
        }
    }
    let near = box_blur(&inside, ew, eh, margin);
    // Known values: the band outside the spot.
    let known: Vec<f32> = (0..ew * eh)
        .map(|k| f32::from(inside[k] == 0.0 && near[k] > 0.0))
        .collect();
    let smooth = (p.radius * 0.15).clamp(1.0, 12.0) as usize;
    let weight = box_blur(&known, ew, eh, smooth);
    let planes = [0, 1, 2].map(|c| {
        let mut v = vec![0.0f32; ew * eh];
        for j in 0..eh {
            for i in 0..ew {
                let k = j * ew + i;
                if known[k] > 0.0 {
                    let (x, y) = ((ex0 + i) as f64, (ey0 + j) as f64);
                    let t = snapshot.get(ex0 + i, ey0 + j)[c];
                    let d = sample(snapshot, x + offset.0, y + offset.1)[c];
                    v[k] = t.max(1e-5).ln() - d.max(1e-5).ln();
                }
            }
        }
        // Smooth along the band so the texture of either side doesn't print
        // into the correction.
        let sum = box_blur(&v, ew, eh, smooth);
        let band: Vec<f32> = (0..ew * eh)
            .map(|k| {
                if known[k] > 0.0 && weight[k] > 0.0 {
                    sum[k] / weight[k]
                } else {
                    0.0
                }
            })
            .collect();
        let mut filled = push_pull(&band, &known, ew, eh);
        // A last smoothing inside only, so coarse levels leave no steps.
        let soft = box_blur(&filled, ew, eh, smooth.max(2));
        for k in 0..ew * eh {
            if known[k] == 0.0 {
                filled[k] = soft[k];
            }
        }
        filled
    });
    (planes, ew, (ex0, ey0))
}

/// Fill the unknown (`weight` 0) values of a plane smoothly from the known
/// ones: average down to coarser levels until everything is covered, then
/// fill each level's gaps from the one above (Gortler et al.'s push-pull).
fn push_pull(values: &[f32], weight: &[f32], w: usize, h: usize) -> Vec<f32> {
    if w <= 1 && h <= 1 || weight.iter().all(|&k| k >= 1.0) {
        return values.to_vec();
    }
    let (cw, ch) = (w.div_ceil(2), h.div_ceil(2));
    let mut cv = vec![0.0f32; cw * ch];
    let mut cwgt = vec![0.0f32; cw * ch];
    for j in 0..h {
        for i in 0..w {
            let (k, ck) = (j * w + i, (j / 2) * cw + i / 2);
            cv[ck] += values[k] * weight[k];
            cwgt[ck] += weight[k];
        }
    }
    for k in 0..cw * ch {
        if cwgt[k] > 0.0 {
            cv[k] /= cwgt[k];
        }
        cwgt[k] = cwgt[k].min(1.0);
    }
    if cwgt.iter().all(|&k| k == 0.0) {
        return values.to_vec();
    }
    let coarse = push_pull(&cv, &cwgt, cw, ch);
    // Bilinear up from the coarse level, for the parts this level lacks.
    let up = |x: f32, y: f32| {
        let x = (x / 2.0 - 0.25).clamp(0.0, (cw - 1) as f32);
        let y = (y / 2.0 - 0.25).clamp(0.0, (ch - 1) as f32);
        let (x0, y0) = (x.floor() as usize, y.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(cw - 1), (y0 + 1).min(ch - 1));
        let (fx, fy) = (x - x0 as f32, y - y0 as f32);
        let g = |xx: usize, yy: usize| coarse[yy * cw + xx];
        let top = g(x0, y0) + (g(x1, y0) - g(x0, y0)) * fx;
        let bot = g(x0, y1) + (g(x1, y1) - g(x0, y1)) * fx;
        top + (bot - top) * fy
    };
    (0..w * h)
        .map(|k| {
            let a = weight[k].min(1.0);
            values[k] * a + up((k % w) as f32, (k / w) as f32) * (1.0 - a)
        })
        .collect()
}
