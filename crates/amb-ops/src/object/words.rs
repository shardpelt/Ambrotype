// SPDX-License-Identifier: GPL-3.0-or-later
//! Select in words: the parts of the photo a typed description names.

use super::geometry::{bounds, fill_snapped, map_at, overlap, shadow_of};
use super::{Like, Selection, frame};
use amb_engine::raw::CameraImage;
use amb_engine::subject::Prompt;
use amb_presets::Value;
use rayon::prelude::*;
use std::collections::BTreeMap;

/// Points the object selector is asked about, across the frame, to find
/// what's in it (`select_words`).
const GRID: (usize, usize) = (3, 2);

/// In `select_words`, a smaller thing inside the best one this close to
/// its score is what was meant; other things this close to the one chosen
/// are more of it (several cars).
const NEAR: f32 = 0.012;

const ALSO: f32 = 0.004;

/// The thing `like` scores best (a crop of the frame, sRGB 0–1, `w`×`h` →
/// how like the words it is): the object selector is asked about points
/// across the frame, each thing it finds once, and each is shown to `like`
/// with what's round it greyed out. Things as like the words as the best
/// are taken too (several cars). Needs the object selector.
pub fn select_words(
    img: &CameraImage,
    settings: &BTreeMap<String, Value>,
    like: &Like<'_>,
) -> Result<Selection, String> {
    let (rgb, w, h) = frame(img, settings);
    let (wf, hf) = (w as f64, h as f64);
    // Where to look: a coarse grid for big things (sky, water), and the
    // middles of the windows most like the words, at three sizes, for
    // small ones (a cow on a far hillside).
    let mut points: Vec<(f64, f64)> = Vec::new();
    for j in 0..GRID.1 {
        for i in 0..GRID.0 {
            points.push((
                (i as f64 + 0.5) / GRID.0 as f64,
                (j as f64 + 0.5) / GRID.1 as f64,
            ));
        }
    }
    // Windows of a half (overlapping by half) and a quarter of the long side.
    let mut boxes = Vec::new();
    for (parts, step_of) in [(2usize, 2usize), (4, 1)] {
        let side = (w.max(h) / parts).max(1);
        let step = (side / step_of).max(1);
        let mut y0 = 0;
        while y0 + side / 2 < h {
            let mut x0 = 0;
            while x0 + side / 2 < w {
                boxes.push((x0, y0, (x0 + side).min(w), (y0 + side).min(h)));
                x0 += step;
            }
            y0 += step;
        }
    }
    let score_boxes =
        |boxes: &[(usize, usize, usize, usize)]| -> Vec<(f32, (usize, usize, usize, usize))> {
            let mut out: Vec<(f32, (usize, usize, usize, usize))> = boxes
                .par_iter()
                .filter_map(|&(x0, y0, x1, y1)| {
                    let mut crop = Vec::with_capacity((x1 - x0) * (y1 - y0) * 3);
                    for y in y0..y1 {
                        crop.extend_from_slice(&rgb[(y * w + x0) * 3..(y * w + x1) * 3]);
                    }
                    Some((like(&crop, x1 - x0, y1 - y0).ok()?, (x0, y0, x1, y1)))
                })
                .collect();
            out.sort_by(|a, b| b.0.total_cmp(&a.0));
            out
        };
    let centre = |(x0, y0, x1, y1): (usize, usize, usize, usize)| {
        ((x0 + x1) as f64 / 2.0 / wf, (y0 + y1) as f64 / 2.0 / hf)
    };
    // The best windows, then the best of their quarters and middle, twice:
    // down to a sixteenth of the long side, small enough to land on a
    // small thing.
    let mut level = score_boxes(&boxes);
    points.extend(level.iter().take(2).map(|(_, b)| centre(*b)));
    for _ in 0..2 {
        let parts: Vec<(usize, usize, usize, usize)> = level
            .iter()
            .take(2)
            .flat_map(|&(_, (x0, y0, x1, y1))| {
                let (mx, my) = ((x0 + x1) / 2, (y0 + y1) / 2);
                let (qx, qy) = ((x1 - x0) / 4, (y1 - y0) / 4);
                [
                    (x0, y0, mx, my),
                    (mx, y0, x1, my),
                    (x0, my, mx, y1),
                    (mx, my, x1, y1),
                    (mx - qx, my - qy, mx + qx, my + qy),
                ]
            })
            .filter(|(x0, y0, x1, y1)| x1 > x0 && y1 > y0)
            .collect();
        level = score_boxes(&parts);
        points.extend(level.iter().take(2).map(|(_, b)| centre(*b)));
    }
    // What's there: the object under each point, once.
    let mut found: Vec<(amb_engine::subject::Map, Vec<bool>)> = Vec::new();
    for (x, y) in points {
        let map = amb_engine::subject::select(&rgb, w, h, &[Prompt::Part { x, y }])?;
        let cells: Vec<bool> = map.data.iter().map(|v| *v >= 0.5).collect();
        let area = cells.iter().filter(|c| **c).count();
        // Specks and most of the picture aren't things to ask about.
        if area * 2000 < cells.len() || area * 10 > cells.len() * 7 {
            continue;
        }
        if found.iter().any(|(_, other)| overlap(other, &cells) > 0.7) {
            continue;
        }
        found.push((map, cells));
    }
    if found.is_empty() {
        return Err("nothing stood out in the photo to choose from".into());
    }
    // Each, shown by itself: its box with some room, what isn't it greyed.
    let scores: Vec<f32> = found
        .par_iter()
        .map(|(map, cells)| {
            let (mw, mh) = (map.width, map.height);
            let (mut lo, mut hi) = ((mw, mh), (0, 0));
            for (k, _) in cells.iter().enumerate().filter(|(_, c)| **c) {
                lo = (lo.0.min(k % mw), lo.1.min(k / mw));
                hi = (hi.0.max(k % mw), hi.1.max(k / mw));
            }
            let room = 0.1 * (hi.0 - lo.0).max(hi.1 - lo.1) as f64 + 1.0;
            let fx = |c: f64| ((c / mw as f64) * wf).clamp(0.0, wf);
            let fy = |c: f64| ((c / mh as f64) * hf).clamp(0.0, hf);
            let (x0, y0) = (
                fx(lo.0 as f64 - room) as usize,
                fy(lo.1 as f64 - room) as usize,
            );
            let (x1, y1) = (
                fx(hi.0 as f64 + 1.0 + room) as usize,
                fy(hi.1 as f64 + 1.0 + room) as usize,
            );
            let (cw, ch) = (x1.saturating_sub(x0).max(1), y1.saturating_sub(y0).max(1));
            let mut crop = Vec::with_capacity(cw * ch * 3);
            for y in y0..y0 + ch {
                for x in x0..x0 + cw {
                    let (x, y) = (x.min(w - 1), y.min(h - 1));
                    let on = map_at(map, (x as f64 + 0.5) / wf, (y as f64 + 0.5) / hf) >= 0.5;
                    for c in 0..3 {
                        let v = rgb[(y * w + x) * 3 + c];
                        crop.push(if on { v } else { 0.5 + (v - 0.5) * 0.25 });
                    }
                }
            }
            like(&crop, cw, ch).unwrap_or(f32::MIN)
        })
        .collect();
    let area = |k: usize| found[k].1.iter().filter(|c| **c).count();
    // `small` mostly inside `big`.
    let within = |small: usize, big: usize| {
        let both = found[small]
            .1
            .iter()
            .zip(&found[big].1)
            .filter(|(a, b)| **a && **b)
            .count();
        both * 10 >= area(small) * 8
    };
    let Some(mut pick) = (0..found.len())
        .filter(|&k| scores[k] > f32::MIN)
        .max_by(|&a, &b| scores[a].total_cmp(&scores[b]))
    else {
        return Err("couldn't compare the photo with the words".into());
    };
    let best = scores[pick];
    // A big thing that has the one asked for in it scores as well as that
    // one (a hillside with the cow on it): the smallest thing inside it
    // that scores nearly as well is what was meant.
    for (k, score) in scores.iter().enumerate() {
        if k != pick && *score >= best - NEAR && area(k) < area(pick) && within(k, pick) {
            pick = k;
        }
    }
    // More of the same, apart from it (several cars), as like the words.
    let mut chosen = vec![pick];
    let wanted = scores[pick] - ALSO;
    for (k, score) in scores.iter().enumerate() {
        if *score >= wanted
            && chosen
                .iter()
                .all(|&c| overlap(&found[c].1, &found[k].1) < 0.1 && !within(c, k) && !within(k, c))
            && area(k) <= area(pick) * 3
        {
            chosen.push(k);
        }
    }
    let mut mask = vec![false; w * h];
    for &k in &chosen {
        let map = &found[k].0;
        fill_snapped(&mut mask, &rgb, w, h, (0, 0, w, h), |px, py| {
            map_at(map, px / wf, py / hf)
        });
    }
    let inside = mask.iter().filter(|m| **m).count();
    if inside < 16 {
        return Err("nothing like that found".into());
    }
    let (lo, hi) = bounds(&mask, w);
    let size = (hi.0 - lo.0).max(hi.1 - lo.1) as f64;
    let shadow = shadow_of(&mask, &rgb, w, h, size);
    Ok(Selection {
        share: inside as f64 / (w * h) as f64,
        object: mask.clone(),
        mask,
        width: w,
        height: h,
        selected: true,
        lo,
        hi,
        shadow,
        with_shadow: false,
    })
}
