// SPDX-License-Identifier: GPL-3.0-or-later
//! Removing a colour cast and the crossover of expired film.

use super::analysis::{midtone, normalise, percentile};
use super::*;

/// Cast removal along the greys. **Ours.** The levels make each layer's
/// ends meet, which takes a cast out only when the deepest shadow and the
/// brightest highlight are grey: a sunset's orange sky or a lamp's glow
/// puts a cast of their own into everything else. This reads the greys at
/// five levels of green instead. At each level, the colours (red and blue
/// against green) that at least `cast_share` of its pixels share are the
/// candidates. The greys are the straight line (a layer's lift and slope
/// against green) that the most pixels' candidates agree with, within
/// `cast_tolerance`: greys of one cast line up from shadows to highlights,
/// a coloured sky or wall sits off the line at its own level. The line
/// through two or more agreeing levels moves red's and blue's levels, by
/// `strength` (0..1). Without two, or when those greys are fewer than
/// `cast_coverage` of the frame's pixels (a night scene: its dark sky sits
/// in the film's toe and on the scanner's noise), nothing changes.
pub(super) fn remove_cast(
    samples: &[[f64; 3]],
    a: &Analysis,
    strength: f64,
    k: &NegativeStrengths,
) -> Analysis {
    let normal = normalise(samples, a);
    const BIN: f64 = 0.02;
    let key = |v: &[f64; 3]| {
        (
            ((v[0] - v[1]) / BIN).round() as i32,
            ((v[2] - v[1]) / BIN).round() as i32,
        )
    };
    let near = |a: (i32, i32), b: (i32, i32)| (a.0 - b.0).abs() <= 1 && (a.1 - b.1).abs() <= 1;
    // Candidates: (green level, red offset, blue offset, pixels).
    let mut candidates: Vec<(f64, f64, f64, f64)> = Vec::new();
    for &at in &k.cast_at {
        let level: Vec<&[f64; 3]> = normal
            .iter()
            .filter(|v| (v[1] - at).abs() <= k.cast_window)
            .collect();
        if level.len() < 64 {
            continue;
        }
        let mut counts = std::collections::BTreeMap::new();
        for v in &level {
            *counts.entry(key(v)).or_insert(0usize) += 1;
        }
        let around = |at: (i32, i32)| -> usize {
            counts
                .iter()
                .filter(|(k, _)| near(**k, at))
                .map(|(_, n)| n)
                .sum()
        };
        let enough = ((level.len() as f64 * k.cast_share).ceil() as usize).max(16);
        // Each colour shared widely enough, strongest first, one per patch.
        let mut bins: Vec<((i32, i32), usize)> = counts
            .keys()
            .map(|b| (*b, around(*b)))
            .filter(|(_, n)| *n >= enough)
            .collect();
        bins.sort_by(|x, y| y.1.cmp(&x.1).then(x.0.cmp(&y.0)));
        let mut taken: Vec<(i32, i32)> = Vec::new();
        for (b, _) in bins {
            if taken
                .iter()
                .any(|t| (t.0 - b.0).abs() <= 2 && (t.1 - b.1).abs() <= 2)
            {
                continue;
            }
            taken.push(b);
            let members: Vec<&&[f64; 3]> = level.iter().filter(|v| near(key(v), b)).collect();
            let mut r: Vec<f64> = members.iter().map(|v| v[0] - v[1]).collect();
            let mut bl: Vec<f64> = members.iter().map(|v| v[2] - v[1]).collect();
            candidates.push((
                at,
                percentile(&mut r, 0.5),
                percentile(&mut bl, 0.5),
                members.len() as f64,
            ));
        }
    }
    // A line is ((red lift, red slope), (blue lift, blue slope)); each level
    // agrees through its candidate nearest the line, if near enough.
    type Line = ((f64, f64), (f64, f64));
    let agreeing = |line: Line| -> Vec<(f64, f64, f64, f64)> {
        let mut out: Vec<(f64, f64, f64, f64)> = Vec::new();
        for &at in &k.cast_at {
            let miss = |c: &(f64, f64, f64, f64)| {
                (c.1 - line.0.0 - line.0.1 * at).hypot(c.2 - line.1.0 - line.1.1 * at)
            };
            if let Some(c) = candidates
                .iter()
                .filter(|c| c.0 == at && miss(c) <= k.cast_tolerance)
                .min_by(|x, y| miss(x).total_cmp(&miss(y)))
            {
                out.push(*c);
            }
        }
        out
    };
    let mut lines: Vec<Line> = vec![((0.0, 0.0), (0.0, 0.0))];
    for (i, p) in candidates.iter().enumerate() {
        for q in &candidates[i + 1..] {
            if q.0 != p.0 {
                let (sr, sb) = ((q.1 - p.1) / (q.0 - p.0), (q.2 - p.2) / (q.0 - p.0));
                lines.push(((p.1 - sr * p.0, sr), (p.2 - sb * p.0, sb)));
            }
        }
    }
    let weight = |cs: &[(f64, f64, f64, f64)]| cs.iter().map(|c| c.3).sum::<f64>();
    let Some(greys) = lines
        .into_iter()
        .map(agreeing)
        .filter(|cs| cs.len() >= 2)
        .max_by(|x, y| weight(x).total_cmp(&weight(y)))
    else {
        return *a;
    };
    // The weighted straight line through the agreeing greys, per layer.
    let w = weight(&greys);
    if w < k.cast_coverage * normal.len() as f64 {
        return *a;
    }
    let mx = greys.iter().map(|g| g.0 * g.3).sum::<f64>() / w;
    let sxx: f64 = greys.iter().map(|g| g.3 * (g.0 - mx).powi(2)).sum();
    let fit = |get: fn(&(f64, f64, f64, f64)) -> f64| {
        let my = greys.iter().map(|g| get(g) * g.3).sum::<f64>() / w;
        let sxy: f64 = greys.iter().map(|g| g.3 * (g.0 - mx) * (get(g) - my)).sum();
        let slope = (sxy / sxx.max(1e-9)).clamp(-k.cast_limit, k.cast_limit);
        ((my - slope * mx).clamp(-k.cast_limit, k.cast_limit), slope)
    };
    let line = [fit(|g| g.1), (0.0, 0.0), fit(|g| g.2)];
    // A grey of green level x sits at lift + (1 + slope)·x in this layer;
    // its levels move so that it lands on x.
    let mut out = *a;
    for c in [0, 2] {
        let (lift, slope) = (line[c].0 * strength, line[c].1 * strength);
        let range = a.density_high[c] - a.density_low[c];
        out.density_low[c] = a.density_low[c] + lift * range;
        out.density_high[c] =
            (a.density_low[c] + (lift + 1.0 + slope) * range).max(out.density_low[c] + 0.05);
    }
    out.midtone = midtone(samples, out.density_low, out.density_high);
    out
}

/// Expired film's crossover. **Ours.** Old film loses speed and contrast
/// unevenly per layer (most often the blue-sensitive one, on top), so a grey
/// shows a cast that changes from shadows to highlights. The levels match
/// the ends of each layer; this matches red and blue to green along the
/// grey axis, at a shadow, a midtone and a highlight level of green. At each,
/// the grey axis is the most common colour among the pixels of that level
/// (greys of one cast cluster, coloured things scatter); a level without
/// enough of them is left alone.
pub(super) fn crossover(samples: &[[f64; 3]], a: &Analysis, k: &NegativeStrengths) -> Crossover {
    let normal = normalise(samples, a);
    const BIN: f64 = 0.02;
    let key = |v: &[f64; 3]| {
        (
            ((v[0] - v[1]) / BIN).round() as i32,
            ((v[2] - v[1]) / BIN).round() as i32,
        )
    };
    let near = |a: (i32, i32), b: (i32, i32)| (a.0 - b.0).abs() <= 1 && (a.1 - b.1).abs() <= 1;
    let mut out = NO_CROSSOVER;
    for (j, &at) in k.crossover_at.iter().enumerate() {
        let level: Vec<&[f64; 3]> = normal
            .iter()
            .filter(|v| (v[1] - at).abs() <= k.crossover_window)
            .collect();
        if level.len() < 64 {
            continue;
        }
        let mut counts = std::collections::BTreeMap::new();
        for v in &level {
            *counts.entry(key(v)).or_insert(0usize) += 1;
        }
        let around = |at: (i32, i32)| -> usize {
            counts
                .iter()
                .filter(|(k, _)| near(**k, at))
                .map(|(_, n)| n)
                .sum()
        };
        let Some(axis) = counts.keys().copied().max_by_key(|k| around(*k)) else {
            continue;
        };
        let grey: Vec<&&[f64; 3]> = level.iter().filter(|v| near(key(v), axis)).collect();
        if grey.len() < level.len() / 20 {
            continue;
        }
        for c in [0, 2] {
            let mut d: Vec<f64> = grey.iter().map(|v| v[c] - v[1]).collect();
            out[c][j] = percentile(&mut d, 0.5).clamp(-k.crossover_limit, k.crossover_limit);
        }
    }
    // Each layer's curve must keep rising: its matched levels stay in order.
    for c in [0, 2] {
        for j in 1..3 {
            let floor = k.crossover_at[j - 1] + out[c][j - 1] + 0.1;
            out[c][j] = out[c][j].max(floor - k.crossover_at[j]);
        }
    }
    out
}

/// A layer's crossover curve: the level `v` where its neutral sits goes to
/// green's, through the matched levels and the ends (0 and 1 stay put),
/// straight lines between them and beyond.
pub(super) fn uncross(v: f64, offsets: [f64; 3], at: [f64; 3]) -> f64 {
    if offsets == [0.0; 3] {
        return v;
    }
    let from = [
        0.0,
        at[0] + offsets[0],
        at[1] + offsets[1],
        at[2] + offsets[2],
        1.0,
    ];
    let to = [0.0, at[0], at[1], at[2], 1.0];
    if v <= 0.0 || v >= 1.0 {
        return v;
    }
    let i = (1..5).find(|&i| v <= from[i]).unwrap_or(4);
    let t = (v - from[i - 1]) / (from[i] - from[i - 1]).max(1e-9);
    to[i - 1] + t * (to[i] - to[i - 1])
}
