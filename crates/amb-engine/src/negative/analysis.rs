// SPDX-License-Identifier: GPL-3.0-or-later
//! Analysing a scan: the film stock list lookup, the base, levels and midtone.

use super::*;

/// Look up a stock by id (case-insensitive); unknown ids fall back to Neutral.
pub fn stock(id: &str) -> &'static Stock {
    STOCKS
        .iter()
        .find(|s| s.id.eq_ignore_ascii_case(id.trim()))
        .unwrap_or(&STOCKS[0])
}

/// Block-averaged copy of an interleaved RGB image (about `target` samples).
pub(super) fn downsample(
    rgb: &[f32],
    w: usize,
    h: usize,
    target: usize,
) -> (Vec<[f64; 3]>, usize, usize) {
    let block = (((w * h) as f64 / target as f64).sqrt().round() as usize).max(1);
    let (bw, bh) = ((w / block).max(1), (h / block).max(1));
    let blocks: Vec<[f64; 3]> = (0..bw * bh)
        .into_par_iter()
        .map(|i| {
            let (bx, by) = (i % bw, i / bw);
            let mut acc = [0.0f64; 3];
            let mut n = 0.0f64;
            for y in by * block..((by + 1) * block).min(h) {
                for x in bx * block..((bx + 1) * block).min(w) {
                    let p = &rgb[(y * w + x) * 3..(y * w + x) * 3 + 3];
                    for c in 0..3 {
                        acc[c] += f64::from(p[c]);
                    }
                    n += 1.0;
                }
            }
            acc.map(|v| v / n.max(1.0))
        })
        .collect();
    (blocks, bw, bh)
}

pub(super) fn percentile(values: &mut [f64], q: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let i = ((values.len() - 1) as f64 * q.clamp(0.0, 1.0)).round() as usize;
    *values.select_nth_unstable_by(i, f64::total_cmp).1
}

fn density(v: f64, base: f64) -> f64 {
    -(v.max(1e-6) / base.max(1e-6)).log10()
}

/// What the levels work on: density for a negative (more = brighter
/// scene), log transmittance for a slide (the same direction).
pub(super) fn log_exposure(v: f64, base: f64, positive: bool) -> f64 {
    if positive {
        -density(v, 1.0)
    } else {
        density(v, base)
    }
}

/// The per-channel median of the first `n` of `order` (brightest first).
fn top_median(blocks: &[[f64; 3]], order: &[usize], n: usize) -> [f64; 3] {
    let top = &order[..n.clamp(1, order.len())];
    std::array::from_fn(|c| {
        let mut v: Vec<f64> = top.iter().map(|&i| blocks[i][c]).collect();
        percentile(&mut v, 0.5).max(1e-4)
    })
}

/// Estimate the film base from the scan: the brightest unclipped film (the
/// rebate between frames, or the thinnest shadows).
///
/// **Ours.** Film can't be brighter than its base, so a brighter area is bare
/// light: sprocket holes, or a gap between the film and the holder, when the
/// scan didn't clip them. The brightest area counts as bare light when a flat
/// area at least `light_gap` denser in every channel borders most of its edge
/// (holes sit inside the rebate), and on colour film that area is orange
/// against it (denser in blue than in red by `mask_tilt`). The base is then
/// that area. A dark picture edge next to the rebate is neither all around it
/// nor orange against it.
pub fn estimate_base(
    rgb: &[f32],
    w: usize,
    h: usize,
    kind: NegativeType,
    k: &NegativeStrengths,
) -> [f64; 3] {
    let (blocks, bw, bh) = downsample(rgb, w, h, 250_000);
    let usable = |p: &[f64; 3]| p.iter().all(|v| *v < 0.97 && *v > 0.0);
    let mut order: Vec<usize> = (0..blocks.len()).filter(|&i| usable(&blocks[i])).collect();
    if order.is_empty() {
        order = (0..blocks.len()).collect();
    }
    let brightness = |i: usize| -> f64 { blocks[i].iter().map(|v| v.max(1e-6).ln()).sum() };
    order.sort_by(|&a, &b| brightness(b).total_cmp(&brightness(a)));
    let n = (order.len() as f64 * k.base_fraction).ceil() as usize;
    let brightest = top_median(&blocks, &order, n);

    let film: Vec<usize> = order
        .iter()
        .copied()
        .filter(|&i| (0..3).all(|c| density(blocks[i][c], brightest[c]) >= k.light_gap))
        .collect();
    if film.len() < n {
        return brightest;
    }
    let below = top_median(&blocks, &film, n);
    let gap: [f64; 3] = std::array::from_fn(|c| density(below[c], brightest[c]));
    if kind == NegativeType::Color && gap[2] - gap[0] < k.mask_tilt {
        return brightest;
    }
    // 1 = like the brightest area, 2 = like the area below it.
    let near = |p: &[f64; 3], to: &[f64; 3]| (0..3).all(|c| density(p[c], to[c]).abs() < 0.05);
    let class: Vec<u8> = blocks
        .iter()
        .map(|p| {
            if !usable(p) {
                0
            } else if near(p, &brightest) {
                1
            } else if near(p, &below) {
                2
            } else {
                0
            }
        })
        .collect();
    if class.iter().filter(|c| **c == 2).count() < n {
        return brightest;
    }
    // The brightest area's edge: how much of it has the area below within
    // a few blocks (past the soft blocks that mix the two).
    const REACH: usize = 3;
    let (mut edge, mut bordered) = (0usize, 0usize);
    for y in 0..bh {
        for x in 0..bw {
            if class[y * bw + x] != 1 {
                continue;
            }
            let outside = [(0, -1), (0, 1), (-1, 0), (1, 0)].iter().any(|(dx, dy)| {
                let (nx, ny) = (x as isize + dx, y as isize + dy);
                nx >= 0
                    && ny >= 0
                    && (nx as usize) < bw
                    && (ny as usize) < bh
                    && class[ny as usize * bw + nx as usize] != 1
            });
            if !outside {
                continue;
            }
            edge += 1;
            let found = (y.saturating_sub(REACH)..(y + REACH + 1).min(bh)).any(|ny| {
                (x.saturating_sub(REACH)..(x + REACH + 1).min(bw))
                    .any(|nx| class[ny * bw + nx] == 2)
            });
            bordered += usize::from(found);
        }
    }
    if edge > 0 && bordered as f64 >= k.light_surround * edge as f64 {
        below
    } else {
        brightest
    }
}

/// Full automatic analysis of a scanned colour negative.
pub fn analyze(rgb: &[f32], w: usize, h: usize, k: &NegativeStrengths) -> Analysis {
    analyze_as(rgb, w, h, NegativeType::Color, k)
}

/// [`analyze`] for a kind of film. A slide has no film base (it is 1).
pub fn analyze_as(
    rgb: &[f32],
    w: usize,
    h: usize,
    kind: NegativeType,
    k: &NegativeStrengths,
) -> Analysis {
    let scan = Scan {
        rgb,
        width: w,
        height: h,
    };
    let n = Negative {
        enabled: true,
        kind,
        ..Default::default()
    };
    resolve(&n, scan, scan, k)
}

/// The part being developed as log exposure per block (density for a
/// negative), without a border for the film holder.
/// Also the width of their grid (blocks per row), for [`detailed`].
pub(super) fn samples_grid(
    scan: Scan,
    base: [f64; 3],
    positive: bool,
    k: &NegativeStrengths,
) -> (Vec<[f64; 3]>, usize) {
    let (blocks, bw, bh) = downsample(scan.rgb, scan.width, scan.height, 250_000);
    let (x0, x1) = (
        (bw as f64 * k.border) as usize,
        bw - (bw as f64 * k.border) as usize,
    );
    let (y0, y1) = (
        (bh as f64 * k.border) as usize,
        bh - (bh as f64 * k.border) as usize,
    );
    let (xs, xe) = (x0, x1.max(x0 + 1).min(bw));
    let v = (y0..y1.max(y0 + 1).min(bh))
        .flat_map(|y| (xs..xe).map(move |x| (x, y)))
        .map(|(x, y)| {
            let p = blocks[y * bw + x];
            std::array::from_fn(|c| log_exposure(p[c], base[c], positive))
        })
        .collect();
    (v, xe.saturating_sub(xs).max(1))
}

/// The samples with detail: blocks whose mean density differs from their
/// neighbours' by at least `detail_floor`, the busier `detail_share` of
/// them. Flat areas (an empty sky, a blank wall, the film's unexposed
/// shadows) are left out, so they don't decide brightness or contrast.
/// `None` when too few have detail (a fog, a flat test frame).
fn detailed(samples: &[[f64; 3]], w: usize, k: &NegativeStrengths) -> Option<Vec<[f64; 3]>> {
    let h = samples.len() / w.max(1);
    if w < 3 || h < 3 {
        return None;
    }
    let lum = |p: &[f64; 3]| (p[0] + p[1] + p[2]) / 3.0;
    let mut activity: Vec<(f64, usize)> = Vec::new();
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let i = y * w + x;
            let c = lum(&samples[i]);
            let a = [i - 1, i + 1, i - w, i + w]
                .iter()
                .map(|&j| (lum(&samples[j]) - c).abs())
                .sum::<f64>()
                / 4.0;
            activity.push((a, i));
        }
    }
    let mut sorted: Vec<f64> = activity.iter().map(|a| a.0).collect();
    let cut = percentile(&mut sorted, 1.0 - k.detail_share).max(k.detail_floor);
    let keep: Vec<[f64; 3]> = activity
        .iter()
        .filter(|a| a.0 >= cut)
        .map(|a| samples[a.1])
        .collect();
    (keep.len() * 20 >= activity.len()).then_some(keep)
}

/// Auto Density and Auto Grade on top of the extremes' levels `a`. **Ours**,
/// after the published idea of metering a print on the parts with detail
/// (expired US 5,724,456 and US 4,731,671): the brightness and the contrast
/// come from the detailed blocks, each pulled only part-way from a typical
/// print; the holds keep the detail's own shadows and highlights inside
/// the print.
pub(super) fn auto_density_grade(
    samples: &[[f64; 3]],
    w: usize,
    a: &Analysis,
    n: &Negative,
    k: &NegativeStrengths,
) -> Analysis {
    let Some(detail) = detailed(samples, w, k) else {
        return *a;
    };
    let mut out = *a;
    if n.auto_grade {
        // The detail's levels per layer, then the range scaled from them.
        let mut lo = [0.0; 3];
        let mut hi = [0.0; 3];
        for c in 0..3 {
            let mut d: Vec<f64> = detail.iter().map(|p| p[c]).collect();
            lo[c] = percentile(&mut d, k.grade_low);
            hi[c] = percentile(&mut d, k.grade_high).max(lo[c] + 0.05);
        }
        let measured = hi[1] - lo[1];
        let range = n.range.max(0.1) + k.grade_pull * (measured - n.range.max(0.1));
        // Holds: never narrower than the detail itself (it must not clip),
        // nor more than twice as wide (a flat frame stays soft, not grey).
        let range = range.clamp(measured, 2.0 * measured);
        let scale = range / measured;
        for c in 0..3 {
            let mid = (lo[c] + hi[c]) / 2.0;
            let half = (hi[c] - lo[c]) / 2.0 * scale;
            out.density_low[c] = mid - half;
            out.density_high[c] = mid + half;
        }
    }
    if n.auto_density {
        let m = midtone(&detail, out.density_low, out.density_high);
        out.midtone = (k.fixed_midtone + k.density_pull * (m - k.fixed_midtone)).clamp(0.05, 0.95);
    } else if n.auto_grade {
        out.midtone = midtone(samples, out.density_low, out.density_high);
    }
    out
}

/// Levels and midtone measured on `samples`, for a given base.
pub(super) fn levels(samples: &[[f64; 3]], base: [f64; 3], k: &NegativeStrengths) -> Analysis {
    let mut density_low = [0.0; 3];
    let mut density_high = [1.0; 3];
    for c in 0..3 {
        let mut d: Vec<f64> = samples.iter().map(|p| p[c]).collect();
        density_low[c] = percentile(&mut d, k.level_low);
        density_high[c] = percentile(&mut d, k.level_high).max(density_low[c] + 0.05);
    }
    Analysis {
        base,
        density_low,
        density_high,
        midtone: midtone(samples, density_low, density_high),
        crossover: NO_CROSSOVER,
    }
}

/// The midtone: the median of the samples between the levels.
pub(super) fn midtone(samples: &[[f64; 3]], low: [f64; 3], high: [f64; 3]) -> f64 {
    let mut mids: Vec<f64> = samples
        .iter()
        .map(|p| {
            (0..3)
                .map(|c| (p[c] - low[c]) / (high[c] - low[c]))
                .sum::<f64>()
                / 3.0
        })
        .collect();
    percentile(&mut mids, 0.5).clamp(0.25, 0.75)
}

/// The samples between the levels, 0..1 per layer.
pub(super) fn normalise(samples: &[[f64; 3]], a: &Analysis) -> Vec<[f64; 3]> {
    samples
        .iter()
        .map(|p| {
            std::array::from_fn(|c| {
                (p[c] - a.density_low[c]) / (a.density_high[c] - a.density_low[c])
            })
        })
        .collect()
}
