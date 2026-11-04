// SPDX-License-Identifier: GPL-3.0-or-later
//! Remove object: a box drawn round something, its outline found by the
//! Subject model on just that box, and one AI Remove spot over all of it.
//!
//! Removing an object in several spots goes wrong: each fill sees the rest
//! of the object and brings it back. One spot over the whole outline,
//! widened a little, fills well. The spot is a brushed Remove spot whose
//! dabs cover the outline (`add_spot` with `separate`), so it is an
//! ordinary Lightroom spot: it undoes, moves and shows like one by hand.

use std::collections::BTreeMap;

use amb_engine::pipeline::DevelopSettings;
use amb_engine::raw::{self, CameraImage};
use amb_engine::subject::MapKind;
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

/// What was found: the spot to add and how much of the box it covers.
#[derive(Debug)]
pub struct Object {
    pub command: Command,
    /// The share of the box the object fills (0-1).
    pub share: f64,
}

/// The Remove spot for the object in `rect` ([x0, y0, x1, y1], fractions
/// of the uncropped photo as stored, as spots are), with the edit's
/// `settings`. Needs the Subject model (`amb_engine::subject::set_finder`).
pub fn find(
    img: &CameraImage,
    settings: &BTreeMap<String, Value>,
    rect: [f64; 4],
) -> Result<Object, String> {
    let [x0, y0, x1, y1] = rect.map(|v| v.clamp(0.0, 1.0));
    let (x0, x1) = (x0.min(x1), x0.max(x1));
    let (y0, y1) = (y0.min(y1), y0.max(y1));
    let (mut develop, _) = DevelopSettings::from_settings(settings);
    // The whole frame, as spots are placed on it.
    develop.crop.enabled = false;
    let small = raw::downscale(img, EDGE);
    let out = amb_engine::pipeline::render(&small, &develop);
    let (w, h) = (out.width, out.height);
    let rgb = amb_engine::output::to_srgb(&out);
    // The box in pixels, and the part the model sees, with some context.
    let (bx0, by0) = ((x0 * w as f64), (y0 * h as f64));
    let (bx1, by1) = ((x1 * w as f64), (y1 * h as f64));
    let (bw, bh) = (bx1 - bx0, by1 - by0);
    if bw < 4.0 || bh < 4.0 {
        return Err("draw a bigger box round the object".into());
    }
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
    // The object: the model's map over the box (not the context round it).
    let mut mask = vec![false; w * h];
    let mut inside = 0usize;
    let (ix0, iy0) = (bx0.floor() as usize, by0.floor() as usize);
    let (ix1, iy1) = ((bx1.ceil() as usize).min(w), (by1.ceil() as usize).min(h));
    for y in iy0..iy1 {
        for x in ix0..ix1 {
            let u = (x - cx0) as f64 / cw as f64;
            let v = (y - cy0) as f64 / ch as f64;
            if map_at(&map, u, v) >= 0.5 {
                mask[y * w + x] = true;
                inside += 1;
            }
        }
    }
    let area = ((ix1 - ix0) * (iy1 - iy0)).max(1);
    let share = inside as f64 / area as f64;
    if inside < 16 {
        return Err(
            "nothing stood out in the box: draw it closer round the object, or brush over it"
                .into(),
        );
    }
    let r = (WIDEN * bw.max(bh)).max(MIN_WIDEN);
    let dabs = cover(&mask, w, h, r);
    let points: Vec<[f64; 2]> = dabs
        .iter()
        .map(|&(x, y)| [(x + 0.5) / w as f64, (y + 0.5) / h as f64])
        .collect();
    let at = [(x0 + x1) / 2.0, (y0 + y1) / 2.0];
    Ok(Object {
        command: Command::AddSpot {
            at,
            radius: r / w as f64,
            kind: Some("remove".into()),
            source: None,
            source_auto: true,
            feather: 30.0,
            opacity: 100.0,
            points: Some(points),
            separate: true,
        },
        share,
    })
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
