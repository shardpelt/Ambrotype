// SPDX-License-Identifier: GPL-3.0-or-later
//! Select object: the mask under a click or inside a box.

use super::geometry::{bounds, fill, fill_snapped, map_at, shadow_of};
use super::{CONTEXT, Selection, Target, frame};
use amb_engine::raw::CameraImage;
use amb_engine::subject::{MapKind, Prompt};
use amb_presets::Value;
use std::collections::BTreeMap;

/// The object at `target`: its outline on the photo.
pub fn select(
    img: &CameraImage,
    settings: &BTreeMap<String, Value>,
    target: &Target,
) -> Result<Selection, String> {
    amb_presets::ai_free::check()?;
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
