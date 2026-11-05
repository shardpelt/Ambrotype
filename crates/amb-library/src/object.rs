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

/// The long edge the photo is looked at, in pixels.
const EDGE: usize = 2048;
/// Context round the box the model sees, as a share of the box's size.
const CONTEXT: f64 = 0.15;
/// The outline is widened by this share of the box's longer side (and at
/// least `MIN_WIDEN` pixels at `EDGE`): shadows, soft edges and the model's
/// blur round the object go with it.
const WIDEN: f64 = 0.05;
const MIN_WIDEN: f64 = 4.0;

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
    let (mut develop, _) = DevelopSettings::from_settings(settings);
    // The whole frame, as spots are placed on it.
    develop.crop.enabled = false;
    let small = raw::downscale(img, EDGE);
    let out = amb_engine::pipeline::render(&small, &develop);
    let (w, h) = (out.width, out.height);
    let rgb = amb_engine::output::to_srgb(&out);
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
            fill(&mut mask, w, h, (0, 0, w, h), |px, py| {
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
                    fill(&mut mask, w, h, inner, |px, py| {
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
    let (mut lo, mut hi) = ((w, h), (0, 0));
    for (i, _) in mask.iter().enumerate().filter(|(_, m)| **m) {
        let (x, y) = (i % w, i / w);
        lo = (lo.0.min(x), lo.1.min(y));
        hi = (hi.0.max(x), hi.1.max(y));
    }
    Ok(Selection {
        share: inside as f64 / area.max(1) as f64,
        mask,
        width: w,
        height: h,
        selected,
        lo,
        hi,
    })
}

impl Selection {
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
        let (w, h) = (self.width, self.height);
        let dist = distance_inside(&self.mask, w, h);
        let mut order: Vec<usize> = (0..w * h).filter(|&i| self.mask[i]).collect();
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
}
