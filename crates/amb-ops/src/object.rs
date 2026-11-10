// SPDX-License-Identifier: GPL-3.0-or-later
//! Remove object: a click on something (or a box round it), its outline
//! found by the object selection model (SAM, when installed; for a box
//! without it, the Subject model on just the box), and one AI Remove spot
//! over all of it.
//!
//! Removing an object in several spots goes wrong: each fill sees the rest
//! of the object and brings it back. One spot over the whole outline,
//! widened a little, fills well. The spot is a brushed Remove spot whose
//! dabs cover the outline (`add_spot` with `separate`), so it is an
//! ordinary Lightroom spot: it undoes, moves and shows like one by hand.

use std::collections::BTreeMap;

use amb_engine::pipeline::DevelopSettings;
use amb_engine::raw::{self, CameraImage};
use amb_engine::subject::{MapKind, Prompt};
use amb_presets::Value;
use amb_protocol::Command;
use rayon::prelude::*;

/// The long edge the photo is looked at, in pixels.
const EDGE: usize = 2048;
/// Context round the box the model sees, as a share of the box's size.
const CONTEXT: f64 = 0.15;
/// The outline is widened by this share of the box's longer side (and at
/// least `MIN_WIDEN` pixels at `EDGE`): shadows, soft edges and the model's
/// blur round the object go with it.
const WIDEN: f64 = 0.05;
const MIN_WIDEN: f64 = 4.0;
/// The outline snaps to the photo's edges within this many pixels (at
/// `EDGE`; the model's map has a cell every 8), following colour edges at
/// least this strong (`snap::guided`).
const SNAP_RADIUS: usize = 8;
const SNAP_EPS: f32 = 1e-3;

/// What to remove: a box round it, or a click on it (fractions of the
/// uncropped photo as stored, as spots are).
#[derive(Debug, Clone)]
pub enum Target {
    Box([f64; 4]),
    Click([f64; 2]),
    /// A click on a part of something: only that part.
    Part([f64; 2]),
    /// Several clicks: on it (true) and on what is not it (false).
    Clicks(Vec<([f64; 2], bool)>),
}

/// An object selected on the photo: where it is, at `EDGE` pixels on the
/// long side (the whole frame, as spots and masks are placed on it).
#[derive(Debug, Clone)]
pub struct Selection {
    pub mask: Vec<bool>,
    pub width: usize,
    pub height: usize,
    /// The share of the box (or, for clicks, the photo) it fills (0-1).
    pub share: f64,
    /// Found by the object selection model, not the Subject model on a box.
    pub selected: bool,
    lo: (usize, usize),
    hi: (usize, usize),
    /// The object alone, and its shadow (the darker patch joined to it;
    /// empty when there's none), which `set_shadow` adds to `mask`.
    object: Vec<bool>,
    shadow: Vec<bool>,
    with_shadow: bool,
}

/// What was found: the spot to add, how much of the box (or, for a click,
/// the photo) the object fills, and which model found it.
#[derive(Debug)]
pub struct Object {
    pub command: Command,
    /// 0-1.
    pub share: f64,
    /// The object selection model (a click, or a box when it's installed)
    /// rather than the Subject model on the box.
    pub selected: bool,
}

/// The Remove spot for the object at `target`, with the edit's `settings`.
/// A click needs the object selection model (`subject::set_selector`); a
/// box uses it when it's there, else the Subject model on just the box.
pub fn find(
    img: &CameraImage,
    settings: &BTreeMap<String, Value>,
    target: Target,
) -> Result<Object, String> {
    Ok(select(img, settings, &target)?.remove())
}

/// The object at `target`: its outline on the photo.
pub fn select(
    img: &CameraImage,
    settings: &BTreeMap<String, Value>,
    target: &Target,
) -> Result<Selection, String> {
    let (rgb, w, h) = frame(img, settings);
    let (wf, hf) = (w as f64, h as f64);
    let mut mask = vec![false; w * h];
    let (selected, area) = match target {
        Target::Click(_) | Target::Part(_) | Target::Clicks(_) => {
            let point = |[x, y]: [f64; 2], positive| Prompt::Point {
                x: x.clamp(0.0, 1.0),
                y: y.clamp(0.0, 1.0),
                positive,
            };
            let prompts: Vec<Prompt> = match target {
                Target::Part([x, y]) => vec![Prompt::Part {
                    x: x.clamp(0.0, 1.0),
                    y: y.clamp(0.0, 1.0),
                }],
                Target::Click(p) => vec![point(*p, true)],
                Target::Clicks(list) => list.iter().map(|(p, on)| point(*p, *on)).collect(),
                Target::Box(_) => unreachable!(),
            };
            if !prompts.iter().any(|p| {
                !matches!(
                    p,
                    Prompt::Point {
                        positive: false,
                        ..
                    }
                )
            }) {
                return Err("click on the object first".into());
            }
            let map = amb_engine::subject::select(&rgb, w, h, &prompts)?;
            fill_snapped(&mut mask, &rgb, w, h, (0, 0, w, h), |px, py| {
                map_at(&map, px / wf, py / hf)
            });
            (true, w * h)
        }
        Target::Box(rect) => {
            let [x0, y0, x1, y1] = rect.map(|v: f64| v.clamp(0.0, 1.0));
            let (x0, x1) = (x0.min(x1), x0.max(x1));
            let (y0, y1) = (y0.min(y1), y0.max(y1));
            let (bx0, by0, bx1, by1) = (x0 * wf, y0 * hf, x1 * wf, y1 * hf);
            let (bw, bh) = (bx1 - bx0, by1 - by0);
            if bw < 4.0 || bh < 4.0 {
                return Err("draw a bigger box round the object".into());
            }
            // The object stays in the box, give or take a little.
            let slack = 0.03 * bw.max(bh);
            let inner = (
                (bx0 - slack).floor().max(0.0) as usize,
                (by0 - slack).floor().max(0.0) as usize,
                ((bx1 + slack).ceil() as usize).min(w),
                ((by1 + slack).ceil() as usize).min(h),
            );
            let area = (inner.2 - inner.0) * (inner.3 - inner.1);
            let prompt = Prompt::Box { x0, y0, x1, y1 };
            match amb_engine::subject::select(&rgb, w, h, &[prompt]) {
                Ok(map) => {
                    fill_snapped(&mut mask, &rgb, w, h, inner, |px, py| {
                        map_at(&map, px / wf, py / hf)
                    });
                    (true, area)
                }
                Err(_) => {
                    // The Subject model on the box with some context round it.
                    let m = CONTEXT * bw.max(bh);
                    let cx0 = (bx0 - m).floor().max(0.0) as usize;
                    let cy0 = (by0 - m).floor().max(0.0) as usize;
                    let cx1 = ((bx1 + m).ceil() as usize).min(w);
                    let cy1 = ((by1 + m).ceil() as usize).min(h);
                    let (cw, ch) = (cx1 - cx0, cy1 - cy0);
                    let mut part = Vec::with_capacity(cw * ch * 3);
                    for y in cy0..cy1 {
                        part.extend_from_slice(&rgb[(y * w + cx0) * 3..(y * w + cx1) * 3]);
                    }
                    let map = amb_engine::subject::find(MapKind::Subject, &part, cw, ch)?;
                    let boxed = (
                        bx0.floor() as usize,
                        by0.floor() as usize,
                        (bx1.ceil() as usize).min(w),
                        (by1.ceil() as usize).min(h),
                    );
                    fill(&mut mask, w, h, boxed, |px, py| {
                        map_at(
                            &map,
                            (px - cx0 as f64) / cw as f64,
                            (py - cy0 as f64) / ch as f64,
                        )
                    });
                    (false, (boxed.2 - boxed.0) * (boxed.3 - boxed.1))
                }
            }
        }
    };
    let inside = mask.iter().filter(|m| **m).count();
    if inside < 16 {
        return Err(match target {
            Target::Click(_) | Target::Part(_) | Target::Clicks(_) => {
                "nothing to select there: click on the object itself, or drag a box round it".into()
            }
            Target::Box(_) => {
                "nothing stood out in the box: draw it closer round the object, or brush over it"
                    .into()
            }
        });
    }
    let (lo, hi) = bounds(&mask, w);
    let size = (hi.0 - lo.0).max(hi.1 - lo.1) as f64;
    let shadow = shadow_of(&mask, &rgb, w, h, size);
    Ok(Selection {
        share: inside as f64 / area.max(1) as f64,
        object: mask.clone(),
        mask,
        width: w,
        height: h,
        selected,
        lo,
        hi,
        shadow,
        with_shadow: false,
    })
}

/// The whole frame (as spots and masks are placed on it), developed with
/// `settings`, `EDGE` pixels on the long side: sRGB 0–1, interleaved.
fn frame(img: &CameraImage, settings: &BTreeMap<String, Value>) -> (Vec<f32>, usize, usize) {
    let (mut develop, _) = DevelopSettings::from_settings(settings);
    develop.crop.enabled = false;
    let small = raw::downscale(img, EDGE);
    let out = amb_engine::pipeline::render(&small, &develop);
    let (w, h) = (out.width, out.height);
    (amb_engine::output::to_srgb(&out), w, h)
}

/// Points the object selector is asked about, across the frame, to find
/// what's in it (`select_words`).
const GRID: (usize, usize) = (3, 2);
/// In `select_words`, a smaller thing inside the best one this close to
/// its score is what was meant; other things this close to the one chosen
/// are more of it (several cars).
const NEAR: f32 = 0.012;
const ALSO: f32 = 0.004;

/// How like some words a picture is (sRGB 0–1, interleaved, `w`×`h`).
pub type Like<'a> = dyn Fn(&[f32], usize, usize) -> Result<f32, String> + Sync + 'a;

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

/// Intersection over union of two masks of the same size.
fn overlap(a: &[bool], b: &[bool]) -> f64 {
    let (mut both, mut either) = (0usize, 0usize);
    for (x, y) in a.iter().zip(b) {
        both += usize::from(*x && *y);
        either += usize::from(*x || *y);
    }
    both as f64 / either.max(1) as f64
}

/// The smallest and largest x and y set in `mask`.
fn bounds(mask: &[bool], w: usize) -> ((usize, usize), (usize, usize)) {
    let (mut lo, mut hi) = ((usize::MAX, usize::MAX), (0, 0));
    for (i, _) in mask.iter().enumerate().filter(|(_, m)| **m) {
        let (x, y) = (i % w, i / w);
        lo = (lo.0.min(x), lo.1.min(y));
        hi = (hi.0.max(x), hi.1.max(y));
    }
    (lo, hi)
}

/// The shadow of `object` (`size` pixels across): what round where it
/// stands (its lowest quarter) is clearly darker than the photo a little further out, joined to it, and
/// within 20 % of its size. Nothing when that would be more than half the
/// object (a dark background, not a shadow).
fn shadow_of(object: &[bool], rgb: &[f32], w: usize, h: usize, size: f64) -> Vec<bool> {
    let n = w * h;
    let none = Vec::new();
    if size < 8.0 {
        return none;
    }
    let lum: Vec<f32> = (0..n)
        .map(|i| 0.2126 * rgb[i * 3] + 0.7152 * rgb[i * 3 + 1] + 0.0722 * rgb[i * 3 + 2])
        .collect();
    let outside: Vec<bool> = object.iter().map(|m| !m).collect();
    // How far each pixel outside it is from it.
    let dist = distance_inside(&outside, w, h);
    let reach = (0.2 * size) as f32;
    // The photo round it, beyond where a shadow would be: its brightness
    // nearby, as a mean over just those pixels.
    let far: Vec<f32> = (0..n)
        .map(|i| f32::from(u8::from(outside[i] && dist[i] > 0.2 * size as f32)))
        .collect();
    let lit: Vec<f32> = (0..n).map(|i| lum[i] * far[i]).collect();
    let r = ((0.3 * size) as usize).max(4);
    let (sum, count) = (
        crate::snap::box_mean(&lit, w, h, r),
        crate::snap::box_mean(&far, w, h, r),
    );
    // A shadow it casts, lit from above, starts where it stands: near its
    // lowest quarter and below its middle; dark folds and corners beside
    // or above it are not its shadow.
    let (lo, hi) = bounds(object, w);
    let mid = lo.1 + (hi.1 - lo.1) / 2;
    let foot_y = lo.1 + (hi.1 - lo.1) * 3 / 4;
    let foot: Vec<bool> = (0..n).map(|i| object[i] && i / w >= foot_y).collect();
    let not_foot: Vec<bool> = foot.iter().map(|f| !f).collect();
    let from_foot = distance_inside(&not_foot, w, h);
    let place = |i: usize| i / w > mid && from_foot[i] <= reach;
    let dark = |i: usize| {
        if !outside[i] || !place(i) || count[i] < 0.05 {
            return false;
        }
        let around = sum[i] / count[i];
        lum[i] < 0.7 * around && lum[i] < around - 0.02
    };
    // Grown out from the object through dark pixels.
    let mut shadow = vec![false; n];
    let mut queue: std::collections::VecDeque<usize> = (0..n).filter(|&i| foot[i]).collect();
    while let Some(i) = queue.pop_front() {
        let (x, y) = ((i % w) as isize, (i / w) as isize);
        for (dx, dy) in [
            (-1, 0),
            (1, 0),
            (0, -1),
            (0, 1),
            (-1, -1),
            (1, -1),
            (-1, 1),
            (1, 1),
        ] {
            let (nx, ny) = (x + dx, y + dy);
            if nx < 0 || ny < 0 || nx as usize >= w || ny as usize >= h {
                continue;
            }
            let j = ny as usize * w + nx as usize;
            if !shadow[j] && dark(j) {
                shadow[j] = true;
                queue.push_back(j);
            }
        }
    }
    // Its specks and holes evened out.
    let soft: Vec<f32> = shadow.iter().map(|&s| f32::from(u8::from(s))).collect();
    let soft = crate::snap::box_mean(&soft, w, h, 2);
    let shadow: Vec<bool> = (0..n)
        .map(|i| outside[i] && place(i) && soft[i] >= 0.4)
        .collect();
    let (found, own) = (
        shadow.iter().filter(|s| **s).count(),
        object.iter().filter(|s| **s).count(),
    );
    // A few pixels are only the edge's own shading; more than half the
    // object is a dark background.
    if found * 50 < own || found * 2 > own {
        return none;
    }
    shadow
}

impl Selection {
    /// Its shadow's size, as a share of the object's (0 when none was found).
    pub fn shadow_share(&self) -> f64 {
        let own = self.object.iter().filter(|s| **s).count();
        self.shadow.iter().filter(|s| **s).count() as f64 / own.max(1) as f64
    }

    /// Take its shadow with it (or not): what Remove, Move and the masks act on.
    pub fn set_shadow(&mut self, on: bool) {
        let on = on && !self.shadow.is_empty();
        self.with_shadow = on;
        self.mask = if on {
            self.object
                .iter()
                .zip(&self.shadow)
                .map(|(a, b)| *a || *b)
                .collect()
        } else {
            self.object.clone()
        };
        (self.lo, self.hi) = bounds(&self.mask, self.width);
    }

    pub fn with_shadow(&self) -> bool {
        self.with_shadow
    }

    fn size(&self) -> f64 {
        (self.hi.0 - self.lo.0).max(self.hi.1 - self.lo.1) as f64
    }

    /// The middle of its bounding box, in fractions.
    pub fn centre(&self) -> [f64; 2] {
        [
            (self.lo.0 + self.hi.0) as f64 / 2.0 / self.width as f64,
            (self.lo.1 + self.hi.1) as f64 / 2.0 / self.height as f64,
        ]
    }

    /// One Remove spot over all of it, widened a little (shadows, soft
    /// edges and the model's blur go with it).
    pub fn remove(&self) -> Object {
        let (wf, hf) = (self.width as f64, self.height as f64);
        let r = (WIDEN * self.size()).max(MIN_WIDEN);
        let points: Vec<[f64; 2]> = cover(&self.mask, self.width, self.height, r)
            .iter()
            .map(|&(x, y)| [(x + 0.5) / wf, (y + 0.5) / hf])
            .collect();
        Object {
            command: Command::AddSpot {
                at: self.centre(),
                radius: r / wf,
                kind: Some("remove".into()),
                source: None,
                source_auto: true,
                feather: 30.0,
                opacity: 100.0,
                points: Some(points),
                separate: true,
            },
            share: self.share,
            selected: self.selected,
        }
    }

    /// Move it by `by` (fractions): a clone of it at the new place, then
    /// one Remove spot where it was (in this order, so the clone copies
    /// the object, not the fill), leaving out what the clone now covers
    /// (spots apply one after another, so it would wipe the clone).
    /// A copy of it `by` (fractions of the width and height) away, as one
    /// clone spot: Move without filling where it was.
    pub fn copy_by(&self, by: [f64; 2]) -> Vec<Command> {
        self.move_by(by).into_iter().take(1).collect()
    }

    pub fn move_by(&self, by: [f64; 2]) -> Vec<Command> {
        let (wf, hf) = (self.width as f64, self.height as f64);
        let r = (0.02 * self.size()).max(MIN_WIDEN);
        let points: Vec<[f64; 2]> = cover(&self.mask, self.width, self.height, r)
            .iter()
            .map(|&(x, y)| [(x + 0.5) / wf + by[0], (y + 0.5) / hf + by[1]])
            .collect();
        let c = self.centre();
        let (w, h) = (self.width, self.height);
        let (dx, dy) = ((by[0] * wf).round() as isize, (by[1] * hf).round() as isize);
        // Where the clone lands, in pixels.
        let moved = |x: isize, y: isize| {
            let (sx, sy) = (x - dx, y - dy);
            sx >= 0
                && sy >= 0
                && (sx as usize) < w
                && (sy as usize) < h
                && self.mask[sy as usize * w + sx as usize]
        };
        let rest: Vec<bool> = (0..w * h)
            .map(|i| self.mask[i] && !moved((i % w) as isize, (i / w) as isize))
            .collect();
        let rr = (WIDEN * self.size()).max(MIN_WIDEN);
        // The Remove spot's dabs, none reaching into the clone.
        let remove: Vec<[f64; 2]> = cover(&rest, w, h, rr)
            .into_iter()
            .filter(|&(x, y)| {
                let reach = 1.6 * rr;
                (0..12).all(|k| {
                    let a = k as f64 * std::f64::consts::TAU / 12.0;
                    !moved(
                        (x + reach * a.cos()) as isize,
                        (y + reach * a.sin()) as isize,
                    )
                }) && !moved(x as isize, y as isize)
            })
            .map(|(x, y)| [(x + 0.5) / wf, (y + 0.5) / hf])
            .collect();
        let mut out = vec![Command::AddSpot {
            at: [c[0] + by[0], c[1] + by[1]],
            radius: r / wf,
            kind: Some("clone".into()),
            source: Some(c),
            source_auto: false,
            feather: 25.0,
            opacity: 100.0,
            points: Some(points),
            separate: true,
        }];
        if !remove.is_empty() {
            out.push(Command::AddSpot {
                at: c,
                radius: rr / wf,
                kind: Some("remove".into()),
                source: None,
                source_auto: true,
                feather: 30.0,
                opacity: 100.0,
                points: Some(remove),
                separate: true,
            });
        }
        out
    }

    /// A brush mask's dabs covering it, `[x, y, radius]` in fractions (the
    /// radius of the width): each dab as big as fits inside (its distance
    /// to the edge), so a few big ones fill it and small ones follow the
    /// outline.
    pub fn mask_dabs(&self) -> Vec<[f64; 3]> {
        dabs_inside(&self.mask, self.width, self.height)
    }

    /// Its shadow's dabs alone, as `mask_dabs` (empty when none was found).
    pub fn shadow_dabs(&self) -> Vec<[f64; 3]> {
        if self.shadow.is_empty() {
            return Vec::new();
        }
        dabs_inside(&self.shadow, self.width, self.height)
    }
}

/// A brush mask's dabs covering `mask` (see `Selection::mask_dabs`).
fn dabs_inside(mask: &[bool], w: usize, h: usize) -> Vec<[f64; 3]> {
    let dist = distance_inside(mask, w, h);
    let mut order: Vec<usize> = (0..w * h).filter(|&i| mask[i]).collect();
    order.sort_by(|&a, &b| dist[b].total_cmp(&dist[a]));
    let mut covered = vec![false; w * h];
    let mut out = Vec::new();
    const SMALLEST: f32 = 2.0;
    for i in order {
        if covered[i] {
            continue;
        }
        let r = (dist[i] + 0.5).max(SMALLEST);
        let (cx, cy) = ((i % w) as f32, (i / w) as f32);
        // Marked within a little less than the dab, so neighbours overlap.
        let mark = r * 0.85;
        let m = mark.ceil() as isize;
        for dy in -m..=m {
            for dx in -m..=m {
                if (dx * dx + dy * dy) as f32 > mark * mark {
                    continue;
                }
                let (x, y) = (cx as isize + dx, cy as isize + dy);
                if x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h {
                    covered[y as usize * w + x as usize] = true;
                }
            }
        }
        covered[i] = true;
        out.push([
            (f64::from(cx) + 0.5) / w as f64,
            (f64::from(cy) + 0.5) / h as f64,
            f64::from(r) / w as f64,
        ]);
    }
    out
}

/// For each pixel of `mask`, about how far (pixels) it is from the nearest
/// pixel outside it (a two-pass chamfer distance).
fn distance_inside(mask: &[bool], w: usize, h: usize) -> Vec<f32> {
    let big = (w + h) as f32;
    let mut d: Vec<f32> = mask.iter().map(|&m| if m { big } else { 0.0 }).collect();
    let (a, b) = (1.0f32, std::f32::consts::SQRT_2);
    let at = |d: &[f32], x: isize, y: isize| {
        if x < 0 || y < 0 || x as usize >= w || y as usize >= h {
            0.0
        } else {
            d[y as usize * w + x as usize]
        }
    };
    for y in 0..h as isize {
        for x in 0..w as isize {
            let i = y as usize * w + x as usize;
            if d[i] == 0.0 {
                continue;
            }
            let v = d[i]
                .min(at(&d, x - 1, y) + a)
                .min(at(&d, x, y - 1) + a)
                .min(at(&d, x - 1, y - 1) + b)
                .min(at(&d, x + 1, y - 1) + b);
            d[i] = v;
        }
    }
    for y in (0..h as isize).rev() {
        for x in (0..w as isize).rev() {
            let i = y as usize * w + x as usize;
            if d[i] == 0.0 {
                continue;
            }
            let v = d[i]
                .min(at(&d, x + 1, y) + a)
                .min(at(&d, x, y + 1) + a)
                .min(at(&d, x + 1, y + 1) + b)
                .min(at(&d, x - 1, y + 1) + b);
            d[i] = v;
        }
    }
    d
}

/// Set `mask` where `value` (at a pixel's centre) is at least a half,
/// within `area` (x0, y0, x1, y1 pixels).
fn fill(
    mask: &mut [bool],
    w: usize,
    _h: usize,
    area: (usize, usize, usize, usize),
    value: impl Fn(f64, f64) -> f32,
) {
    let (x0, y0, x1, y1) = area;
    for y in y0..y1 {
        for x in x0..x1 {
            if value(x as f64 + 0.5, y as f64 + 0.5) >= 0.5 {
                mask[y * w + x] = true;
            }
        }
    }
}

/// Like `fill`, with the map's edge snapped to the photo's first: the
/// object selection model's map is coarse, so its outline would come out
/// in steps (`snap`). Only round where the map is, to keep it quick.
fn fill_snapped(
    mask: &mut [bool],
    rgb: &[f32],
    w: usize,
    _h: usize,
    area: (usize, usize, usize, usize),
    value: impl Fn(f64, f64) -> f32,
) {
    let (x0, y0, x1, y1) = area;
    let (aw, ah) = (x1 - x0, y1 - y0);
    let soft: Vec<f32> = (0..aw * ah)
        .map(|i| value((x0 + i % aw) as f64 + 0.5, (y0 + i / aw) as f64 + 0.5))
        .collect();
    let (mut lo, mut hi) = ((aw, ah), (0, 0));
    for (i, _) in soft.iter().enumerate().filter(|(_, v)| **v >= 0.02) {
        let (x, y) = (i % aw, i / aw);
        lo = (lo.0.min(x), lo.1.min(y));
        hi = (hi.0.max(x), hi.1.max(y));
    }
    if lo.0 > hi.0 {
        return;
    }
    let grow = 3 * SNAP_RADIUS;
    let (cx0, cy0) = (lo.0.saturating_sub(grow), lo.1.saturating_sub(grow));
    let (cx1, cy1) = ((hi.0 + grow + 1).min(aw), (hi.1 + grow + 1).min(ah));
    let (cw, chh) = (cx1 - cx0, cy1 - cy0);
    let mut p = Vec::with_capacity(cw * chh);
    let mut colours = Vec::with_capacity(cw * chh * 3);
    for y in cy0..cy1 {
        p.extend_from_slice(&soft[y * aw + cx0..y * aw + cx1]);
        let row = (y0 + y) * w + x0;
        colours.extend_from_slice(&rgb[(row + cx0) * 3..(row + cx1) * 3]);
    }
    let q = crate::snap::guided(&p, &colours, cw, chh, SNAP_RADIUS, SNAP_EPS);
    for y in 0..chh {
        for x in 0..cw {
            if q[y * cw + x] >= 0.5 {
                mask[(y0 + cy0 + y) * w + x0 + cx0 + x] = true;
            }
        }
    }
}

/// The model's map at (u, v), fractions of the part it saw (bilinear).
fn map_at(m: &amb_engine::subject::Map, u: f64, v: f64) -> f32 {
    let fx = (u * m.width as f64 - 0.5).clamp(0.0, (m.width - 1) as f64);
    let fy = (v * m.height as f64 - 0.5).clamp(0.0, (m.height - 1) as f64);
    let (x, y) = (fx as usize, fy as usize);
    let (x1, y1) = ((x + 1).min(m.width - 1), (y + 1).min(m.height - 1));
    let (tx, ty) = ((fx - x as f64) as f32, (fy - y as f64) as f32);
    let at = |x: usize, y: usize| m.data[y * m.width + x];
    let top = at(x, y) + (at(x1, y) - at(x, y)) * tx;
    let bottom = at(x, y1) + (at(x1, y1) - at(x, y1)) * tx;
    top + (bottom - top) * ty
}

/// Dab centres (pixels) of radius `r` that cover every pixel of `mask`: a
/// grid of step 0.7 r, a dab wherever the mask comes within 0.6 r (more
/// than half a step's diagonal, with rounding), so each masked pixel is
/// under a dab and nothing reaches more than about 1.6 r past the outline.
pub fn cover(mask: &[bool], w: usize, h: usize, r: f64) -> Vec<(f64, f64)> {
    let step = (0.7 * r).max(1.0);
    let reach = (0.6 * r).ceil() as isize + 1;
    let (mut lo_x, mut lo_y, mut hi_x, mut hi_y) = (w, h, 0, 0);
    for y in 0..h {
        for x in 0..w {
            if mask[y * w + x] {
                (lo_x, lo_y, hi_x, hi_y) = (lo_x.min(x), lo_y.min(y), hi_x.max(x), hi_y.max(y));
            }
        }
    }
    let mut dabs = Vec::new();
    if lo_x > hi_x {
        return dabs;
    }
    let near = |gx: f64, gy: f64| {
        let (cx, cy) = (gx.round() as isize, gy.round() as isize);
        for dy in -reach..=reach {
            for dx in -reach..=reach {
                if dx * dx + dy * dy > reach * reach {
                    continue;
                }
                let (x, y) = (cx + dx, cy + dy);
                if x >= 0
                    && y >= 0
                    && (x as usize) < w
                    && (y as usize) < h
                    && mask[y as usize * w + x as usize]
                {
                    return true;
                }
            }
        }
        false
    };
    let mut gy = lo_y as f64;
    while gy <= hi_y as f64 + step {
        let mut gx = lo_x as f64;
        while gx <= hi_x as f64 + step {
            if near(gx, gy) {
                dabs.push((gx, gy));
            }
            gx += step;
        }
        gy += step;
    }
    dabs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dabs_cover_a_shape_and_leave_its_gaps() {
        // Two blobs with a wide gap between them.
        let (w, h) = (120, 40);
        let mut mask = vec![false; w * h];
        for y in 10..30 {
            for x in (10..30).chain(90..110) {
                mask[y * w + x] = true;
            }
        }
        let r = 6.0;
        let dabs = cover(&mask, w, h, r);
        // Every masked pixel is under a dab.
        for y in 0..h {
            for x in 0..w {
                if mask[y * w + x] {
                    let (px, py) = (x as f64, y as f64);
                    assert!(
                        dabs.iter().any(|&(dx, dy)| (dx - px).hypot(dy - py) <= r),
                        "{x},{y}"
                    );
                }
            }
        }
        // Nothing in the middle of the gap.
        assert!(
            dabs.iter().all(|&(x, _)| !(45.0..75.0).contains(&x)),
            "{dabs:?}"
        );
    }

    #[test]
    fn copy_is_moves_clone_alone_and_leaves_the_object() {
        let (w, h) = (200, 120);
        let mut mask = vec![false; w * h];
        for y in 40..80 {
            for x in 30..70 {
                mask[y * w + x] = true;
            }
        }
        let sel = Selection {
            mask: mask.clone(),
            width: w,
            height: h,
            share: 1.0,
            selected: true,
            lo: (30, 40),
            hi: (69, 79),
            object: mask,
            shadow: Vec::new(),
            with_shadow: false,
        };
        let by = [0.5, 0.0];
        let moved = sel.move_by(by);
        let copied = sel.copy_by(by);
        // Move clones it there and removes it here; Copy only clones.
        assert_eq!(moved.len(), 2);
        assert_eq!(copied.len(), 1);
        assert!(matches!(&copied[0], Command::AddSpot { kind: Some(k), .. } if k == "clone"));
        assert_eq!(format!("{:?}", copied[0]), format!("{:?}", moved[0]));
    }

    #[test]
    fn mask_dabs_fill_a_shape_with_few_big_dabs_and_stay_inside() {
        let (w, h) = (200, 120);
        let mut mask = vec![false; w * h];
        for y in 20..100 {
            for x in 30..170 {
                mask[y * w + x] = true;
            }
        }
        let sel = Selection {
            mask: mask.clone(),
            width: w,
            height: h,
            share: 1.0,
            selected: true,
            lo: (30, 20),
            hi: (169, 99),
            object: mask.clone(),
            shadow: Vec::new(),
            with_shadow: false,
        };
        let dabs = sel.mask_dabs();
        let px: Vec<(f64, f64, f64)> = dabs
            .iter()
            .map(|d| (d[0] * w as f64, d[1] * h as f64, d[2] * w as f64))
            .collect();
        // Every pixel of the shape is under a dab.
        for y in 0..h {
            for x in 0..w {
                if mask[y * w + x] {
                    let (fx, fy) = (x as f64 + 0.5, y as f64 + 0.5);
                    assert!(
                        px.iter()
                            .any(|&(cx, cy, r)| (cx - fx).hypot(cy - fy) <= r + 0.5),
                        "{x},{y}"
                    );
                }
            }
        }
        // Not far outside, and far fewer dabs than pixels.
        for &(cx, cy, r) in &px {
            assert!(
                cx - r > 27.0 && cx + r < 173.0 && cy - r > 17.0 && cy + r < 103.0,
                "{cx},{cy} r {r}"
            );
        }
        assert!(px.len() < 600, "{} dabs", px.len());
    }

    #[test]
    fn a_shadow_joined_to_the_object_is_found_and_a_dark_patch_away_from_it_is_not() {
        // A bright disc on mid grey, a dark ellipse under it, touching it,
        // and a dark square far off.
        let (w, h) = (400, 300);
        let mut rgb = vec![0.6f32; w * h * 3];
        let mut object = vec![false; w * h];
        let mut truth = vec![false; w * h];
        for y in 0..h {
            for x in 0..w {
                let (fx, fy) = (x as f64, y as f64);
                let i = y * w + x;
                let disc = (fx - 200.0).hypot(fy - 120.0) < 60.0;
                let under = ((fx - 200.0) / 70.0).hypot((fy - 185.0) / 12.0) < 1.0;
                let patch = (20..60).contains(&x) && (20..60).contains(&y);
                let v = if disc {
                    0.9
                } else if under || patch {
                    0.25
                } else {
                    0.6
                };
                rgb[i * 3..][..3].fill(v);
                object[i] = disc;
                truth[i] = under && !disc;
            }
        }
        let (lo, hi) = bounds(&object, w);
        let size = (hi.0 - lo.0).max(hi.1 - lo.1) as f64;
        let shadow = shadow_of(&object, &rgb, w, h, size);
        assert!(!shadow.is_empty());
        let found = (0..w * h).filter(|&i| shadow[i] && truth[i]).count();
        let want = truth.iter().filter(|t| **t).count();
        assert!(found * 10 > want * 7, "{found} of {want}");
        let stray = (0..w * h).filter(|&i| shadow[i] && !truth[i]).count();
        assert!(stray * 10 < want, "{stray} stray");
        assert!((0..60 * w).all(|i| !shadow[i] || i % w > 60));
        // On a dark background there's no shadow to take.
        let mut dark = rgb.clone();
        for (i, o) in object.iter().enumerate() {
            if !o {
                dark[i * 3..][..3].fill(0.1);
            }
        }
        assert!(shadow_of(&object, &dark, w, h, size).is_empty());
    }
}
