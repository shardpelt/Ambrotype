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
#[derive(Debug, Clone, Copy)]
pub enum Target {
    Box([f64; 4]),
    Click([f64; 2]),
    /// A click on a part of something: only that part.
    Part([f64; 2]),
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
        Target::Click([x, y]) | Target::Part([x, y]) => {
            let (x, y) = (x.clamp(0.0, 1.0), y.clamp(0.0, 1.0));
            let prompt = match target {
                Target::Part(_) => Prompt::Part { x, y },
                _ => Prompt::Point {
                    x,
                    y,
                    positive: true,
                },
            };
            let map = amb_engine::subject::select(&rgb, w, h, &[prompt])?;
            fill(&mut mask, w, h, (0, 0, w, h), |px, py| {
                map_at(&map, px / wf, py / hf)
            });
            (true, w * h)
        }
        Target::Box(rect) => {
            let [x0, y0, x1, y1] = rect.map(|v| v.clamp(0.0, 1.0));
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
            Target::Click(_) | Target::Part(_) => {
                "nothing to select there: click on the object itself, or drag a box round it".into()
            }
            Target::Box(_) => {
                "nothing stood out in the box: draw it closer round the object, or brush over it"
                    .into()
            }
        });
    }
    // Widened by a share of the object's size.
    let (mut lo, mut hi) = ((w, h), (0, 0));
    for (i, _) in mask.iter().enumerate().filter(|(_, m)| **m) {
        let (x, y) = (i % w, i / w);
        lo = (lo.0.min(x), lo.1.min(y));
        hi = (hi.0.max(x), hi.1.max(y));
    }
    let size = (hi.0 - lo.0).max(hi.1 - lo.1) as f64;
    let r = (WIDEN * size).max(MIN_WIDEN);
    let dabs = cover(&mask, w, h, r);
    let points: Vec<[f64; 2]> = dabs
        .iter()
        .map(|&(x, y)| [(x + 0.5) / wf, (y + 0.5) / hf])
        .collect();
    let at = [
        (lo.0 + hi.0) as f64 / 2.0 / wf,
        (lo.1 + hi.1) as f64 / 2.0 / hf,
    ];
    Ok(Object {
        command: Command::AddSpot {
            at,
            radius: r / wf,
            kind: Some("remove".into()),
            source: None,
            source_auto: true,
            feather: 30.0,
            opacity: 100.0,
            points: Some(points),
            separate: true,
        },
        share: inside as f64 / area.max(1) as f64,
        selected,
    })
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
}
