// SPDX-License-Identifier: GPL-3.0-or-later
//! Mask commands: `add_mask`, `set_mask`, `paint_mask`, `remove_mask`,
//! `duplicate_mask`, `move_mask`, and
//! the `list_masks` description. They edit `MaskGroupBasedCorrections` in
//! Lightroom's own layout ([`amb_presets::masks`]), so masks made here open in
//! Lightroom and the other way round. Masks and parts are addressed by their
//! position (0-based) as `list_masks` shows them.

mod describe;
mod fields;

use std::collections::BTreeMap;

use amb_presets::Value;
use amb_presets::masks::{
    self, COLOR_SAMPLES, Combination, LOCAL_SLIDERS, LocalSlider, MASK_TAG, MaskKind, Mode,
    boolean, number,
};
use serde_json::{Map, json};

use crate::Error;
use crate::convert::{Clamp, json_number};

pub use describe::describe;
use fields::*;

type Fields = BTreeMap<String, Value>;

/// Kinds `add_mask` can make (a brush is made by `paint_mask`).
pub const ADD_KINDS: &[&str] = &[
    "linear",
    "radial",
    "luminance",
    "color",
    "sky",
    "subject",
    "background",
    "people",
];

/// Most dabs one stroke may hold.
pub(crate) const MAX_DABS: usize = 20_000;

// ---------------------------------------------------------------- commands

/// `add_mask`: a new mask (or, with `mask`, a new part of that mask).
pub fn add(
    next: &mut BTreeMap<String, Value>,
    kind: &str,
    mask: Option<usize>,
    mode: Option<&str>,
    values: Option<&Map<String, serde_json::Value>>,
    clamped: &mut Vec<Clamp>,
) -> Result<(), Error> {
    let kind = kind.trim().to_ascii_lowercase();
    if kind == "brush" {
        return Err(invalid(
            "kind",
            "brush masks are made by painting: use paint_mask",
        ));
    }
    let mode = mode.map(parse_mode).transpose()?.unwrap_or(Mode::Add);
    if mask.is_none() && mode != Mode::Add {
        return Err(invalid(
            "mode",
            "a new mask's first part adds; give `mask` to subtract or intersect",
        ));
    }
    let mut list = corrections(next);
    let index = mask.unwrap_or(list.len());
    let mut e = new_component(
        &kind,
        Combination {
            mode,
            // Lightroom's Select Background is the subject, inverted.
            inverted: kind == "background",
        },
    )?;
    if let Some(values) = values {
        set_component_values(&mut e, &format!("mask {index} new {kind}"), values, clamped)?;
    }
    match mask {
        Some(m) => {
            let c = correction_mut(&mut list, m)?;
            components_mut(c).push(Value::Struct(e));
        }
        None => {
            let mut c = new_correction(index);
            components_mut(&mut c).push(Value::Struct(e));
            list.push(Value::Struct(c));
        }
    }
    store(next, list);
    Ok(())
}

/// How many masks the edit has.
pub fn count(settings: &BTreeMap<String, Value>) -> usize {
    corrections(settings).len()
}

/// `set_mask`: change a mask's sliders and settings, or one part's.
pub fn set(
    next: &mut BTreeMap<String, Value>,
    mask: usize,
    part: Option<usize>,
    values: &Map<String, serde_json::Value>,
    clamped: &mut Vec<Clamp>,
) -> Result<(), Error> {
    let mut list = corrections(next);
    let c = correction_mut(&mut list, mask)?;
    match part {
        None => set_correction_values(c, mask, values, clamped)?,
        Some(p) => {
            let range = part_range(c, mask, p)?;
            let at = format!("mask {mask} part {p}");
            let comps = components_mut(c);
            let Some(Value::Struct(first)) = comps.get(range.start) else {
                return Err(invalid(&at, "is not a mask component"));
            };
            if MaskKind::of(first) == MaskKind::Brush {
                // A brush part: its strokes share mode, inversion and name.
                let mut comb = Combination::of(first);
                for (key, v) in values {
                    let name = format!("{at} {key}");
                    match key.as_str() {
                        "mode" => {
                            let m = v
                                .as_str()
                                .ok_or_else(|| invalid(&name, "expects a string"))?;
                            comb.mode = parse_mode(m)?;
                        }
                        "invert" => comb.inverted = bool_arg(&name, v)?,
                        "active" | "name" => {}
                        _ => {
                            return Err(invalid(
                                &name,
                                "a brush part takes mode, invert, active and name; paint more with paint_mask",
                            ));
                        }
                    }
                }
                let aggregate = masks::text(first, "What").as_deref() == Some("Mask/Aggregate");
                if aggregate {
                    // The aggregate carries the combination; strokes keep theirs.
                    let Some(Value::Struct(agg)) = comps.get_mut(range.start) else {
                        unreachable!()
                    };
                    set_combination(agg, comb);
                } else {
                    set_brush_combination(&mut comps[range.clone()], comb, None);
                }
                for s in &mut comps[range] {
                    let Value::Struct(m) = s else { continue };
                    if let Some(v) = values.get("active") {
                        m.insert("MaskActive".into(), boolean(bool_arg(&at, v)?));
                    }
                    if let Some(v) = values.get("name").and_then(|v| v.as_str()) {
                        m.insert("MaskName".into(), Value::Text(v.to_string()));
                    }
                }
            } else {
                let Some(Value::Struct(e)) = comps.get_mut(range.start) else {
                    unreachable!()
                };
                set_component_values(e, &at, values, clamped)?;
            }
        }
    }
    store(next, list);
    Ok(())
}

/// One stroke for `paint_mask`.
pub struct Stroke<'a> {
    pub points: &'a [[f64; 2]],
    /// Dabs of their own, `[x, y, radius]`, in place of `points`.
    pub dabs: Option<&'a [[f64; 3]]>,
    /// Brush radius as a fraction of the image's width.
    pub size: f64,
    /// 0-100.
    pub feather: f64,
    pub flow: f64,
    pub density: f64,
    pub erase: bool,
}

/// Dabs along a stroke through `points`, 0.2 × the radius `size` apart, as
/// Lightroom spaces them. The radius is in frame widths and points are
/// per-axis fractions, so stepping both axes by 0.2 × size is never coarser
/// than that.
pub(crate) fn dabs_along(points: &[[f64; 2]], size: f64) -> Result<Vec<(f64, f64)>, Error> {
    let step = size * 0.2;
    let mut dabs: Vec<(f64, f64)> = Vec::new();
    for p in points {
        let (x, y) = (p[0], p[1]);
        if let Some(&(px, py)) = dabs.last() {
            let gap = (x - px).abs().max((y - py).abs());
            let n = (gap / step).ceil() as usize;
            for i in 1..n {
                let t = i as f64 / n as f64;
                dabs.push((px + (x - px) * t, py + (y - py) * t));
            }
            if gap == 0.0 {
                continue;
            }
        }
        dabs.push((x, y));
        if dabs.len() > MAX_DABS {
            return Err(invalid(
                "points",
                format!("a stroke may have at most {MAX_DABS} dabs; split it"),
            ));
        }
    }
    Ok(dabs)
}

/// `paint_mask`: add a stroke to a brush part (made when `part` is absent,
/// in a new mask when `mask` is absent too).
pub fn paint(
    next: &mut BTreeMap<String, Value>,
    mask: Option<usize>,
    part: Option<usize>,
    mode: Option<&str>,
    stroke: &Stroke,
    clamped: &mut Vec<Clamp>,
) -> Result<(), Error> {
    if stroke.points.is_empty() && stroke.dabs.is_none_or(|d| d.is_empty()) {
        return Err(invalid(
            "points",
            "a stroke needs at least one [x, y] point",
        ));
    }
    if stroke.points.iter().flatten().any(|v| !v.is_finite())
        || stroke
            .dabs
            .into_iter()
            .flatten()
            .flatten()
            .any(|v| !v.is_finite())
    {
        return Err(invalid("points", "points must be finite numbers"));
    }
    if stroke.dabs.is_some_and(|d| d.len() > MAX_DABS) {
        return Err(invalid("dabs", format!("at most {MAX_DABS} dabs")));
    }
    // Given dabs: the brush's radius is the first one's.
    let size = match stroke.dabs.and_then(|d| d.first()) {
        Some(d) => d[2].clamp(0.0005, 0.5),
        None => clamp("size", stroke.size, 0.001, 0.5, clamped),
    };
    let feather = clamp("feather", stroke.feather, 0.0, 100.0, clamped);
    let flow = clamp("flow", stroke.flow, 1.0, 100.0, clamped);
    let density = clamp("density", stroke.density, 1.0, 100.0, clamped) / 100.0;
    let mode = mode.map(parse_mode).transpose()?;
    if part.is_some() && mode.is_some() {
        return Err(invalid("mode", "set an existing part's mode with set_mask"));
    }
    if mask.is_none() && part.is_some() {
        return Err(invalid("part", "a part needs its mask"));
    }
    if mask.is_none() && mode.is_some_and(|m| m != Mode::Add) {
        return Err(invalid(
            "mode",
            "a new mask's first part adds; give `mask` to subtract or intersect",
        ));
    }
    // The Dabs list: a stroke's, or the given dabs with an `r` token
    // wherever the radius changes.
    let tokens: Vec<Value> = match stroke.dabs {
        Some(given) => {
            let mut out = Vec::with_capacity(given.len() + 8);
            let mut r = size;
            for &[x, y, radius] in given {
                let radius = radius.clamp(0.0005, 0.5);
                if (radius - r).abs() > 1e-6 {
                    out.push(Value::Text(format!("r {radius:.6}")));
                    r = radius;
                }
                out.push(Value::Text(masks::format_dab(x, y)));
            }
            out
        }
        None => dabs_along(stroke.points, size)?
            .iter()
            .map(|&(x, y)| Value::Text(masks::format_dab(x, y)))
            .collect(),
    };
    let paint_entry = |blend: u8, inverted: bool, sync_id: String, value: f64| -> Fields {
        let mut e: Fields = BTreeMap::new();
        e.insert("What".into(), Value::Text("Mask/Paint".into()));
        e.insert("MaskActive".into(), boolean(true));
        e.insert("MaskBlendMode".into(), Value::Text(blend.to_string()));
        e.insert("MaskInverted".into(), boolean(inverted));
        e.insert("MaskSyncID".into(), Value::Text(sync_id));
        e.insert("MaskValue".into(), number(value));
        e.insert("Radius".into(), number(size));
        e.insert("Flow".into(), number(flow / 100.0));
        e.insert("CenterWeight".into(), number(1.0 - feather / 100.0));
        e.insert("Dabs".into(), Value::Seq(tokens.clone()));
        e
    };

    let mut list = corrections(next);
    let index = mask.unwrap_or(list.len());
    if mask.is_none() {
        list.push(Value::Struct(new_correction(index)));
    }
    let c = correction_mut(&mut list, index)?;
    let Some(p) = part else {
        if stroke.erase {
            return Err(invalid(
                "erase",
                "there is nothing to erase in a new brush part",
            ));
        }
        // A new brush: one aggregate carrying the combination, its strokes
        // inside (Lightroom's current layout).
        let mut agg: Fields = BTreeMap::new();
        agg.insert("What".into(), Value::Text("Mask/Aggregate".into()));
        agg.insert("MaskActive".into(), boolean(true));
        agg.insert("MaskName".into(), Value::Text("Brush".into()));
        agg.insert("MaskSyncID".into(), Value::Text(masks::new_sync_id()));
        set_combination(
            &mut agg,
            Combination {
                mode: mode.unwrap_or(Mode::Add),
                inverted: false,
            },
        );
        let e = paint_entry(0, false, masks::new_sync_id(), density);
        agg.insert("Masks".into(), Value::Seq(vec![Value::Struct(e)]));
        components_mut(c).push(Value::Struct(agg));
        store(next, list);
        return Ok(());
    };
    let range = part_range(c, index, p)?;
    let at = format!("mask {index} part {p}");
    let comps = components_mut(c);
    let Some(Value::Struct(first)) = comps.get_mut(range.start) else {
        return Err(invalid(&at, "is not a mask component"));
    };
    if MaskKind::of(first) != MaskKind::Brush {
        return Err(invalid(
            &at,
            format!("is a {}, not a brush", MaskKind::of(first).label()),
        ));
    }
    if masks::text(first, "What").as_deref() == Some("Mask/Aggregate") {
        if masks::uses_brush_table(first) {
            return Err(invalid(
                &at,
                "this brush keeps its strokes in Lightroom's .acr file, which can't be edited here yet",
            ));
        }
        // Strokes paint at their density; an erasing one has density 0.
        let value = if stroke.erase { 0.0 } else { density };
        let e = paint_entry(0, false, masks::new_sync_id(), value);
        match first.get_mut("Masks") {
            Some(Value::Seq(items)) => items.push(Value::Struct(e)),
            _ => {
                first.insert("Masks".into(), Value::Seq(vec![Value::Struct(e)]));
            }
        }
    } else {
        // An older file's brush: strokes straight in CorrectionMasks, each
        // carrying the part's combination.
        let sync_id = masks::text(first, "MaskSyncID").unwrap_or_default();
        let comb = Combination::of(first);
        let first_zero = masks::num(first, "MaskValue").unwrap_or(1.0) == 0.0;
        let (blend, _, inverted) = comb.stored();
        let value = match (stroke.erase, first_zero) {
            (false, false) => density,
            (false, true) => 0.0,
            (true, false) => 0.0,
            (true, true) => 1.0,
        };
        let e = paint_entry(blend, inverted, sync_id, value);
        comps.insert(range.end, Value::Struct(e));
    }
    store(next, list);
    Ok(())
}

/// `remove_mask`: a whole mask, or one part (the mask goes with its last part).
pub fn remove(
    next: &mut BTreeMap<String, Value>,
    mask: usize,
    part: Option<usize>,
) -> Result<(), Error> {
    let mut list = corrections(next);
    let c = correction_mut(&mut list, mask)?;
    let empty = match part {
        None => true,
        Some(p) => {
            let range = part_range(c, mask, p)?;
            let comps = components_mut(c);
            comps.drain(range);
            comps.is_empty()
        }
    };
    if empty {
        list.remove(mask);
    }
    store(next, list);
    Ok(())
}

/// `duplicate_mask`: a copy of a mask just after it, named "… copy", with
/// sync IDs of its own so Lightroom treats it as a separate mask.
pub fn duplicate(next: &mut BTreeMap<String, Value>, mask: usize) -> Result<(), Error> {
    let mut list = corrections(next);
    let mut copy = correction_mut(&mut list, mask)?.clone();
    copy.insert("CorrectionSyncID".into(), Value::Text(masks::new_sync_id()));
    let name = masks::text(&copy, "CorrectionName").unwrap_or_else(|| format!("Mask {}", mask + 1));
    copy.insert("CorrectionName".into(), Value::Text(format!("{name} copy")));
    // Strokes of one brush part share a MaskSyncID; keep them together.
    let mut ids = BTreeMap::new();
    for e in components_mut(&mut copy).iter_mut() {
        if let Value::Struct(e) = e
            && let Some(old) = masks::text(e, "MaskSyncID")
        {
            let id = ids.entry(old).or_insert_with(masks::new_sync_id).clone();
            e.insert("MaskSyncID".into(), Value::Text(id));
        }
    }
    list.insert(mask + 1, Value::Struct(copy));
    store(next, list);
    Ok(())
}

/// `move_mask`: put a mask at position `to` (0-based), the others keeping
/// their order. The order is the panel's; Lightroom applies masks in it.
pub fn move_to(next: &mut BTreeMap<String, Value>, mask: usize, to: usize) -> Result<(), Error> {
    let mut list = corrections(next);
    correction_mut(&mut list, mask)?;
    if to >= list.len() {
        return Err(invalid(
            "to",
            format!("there are {} mask(s), numbered from 0", list.len()),
        ));
    }
    let m = list.remove(mask);
    list.insert(to, m);
    store(next, list);
    Ok(())
}

/// `move_mask` with a part: put that part at position `to` among the mask's
/// parts. Parts combine in order, so the first one must add.
pub fn move_part(
    next: &mut BTreeMap<String, Value>,
    mask: usize,
    part: usize,
    to: usize,
) -> Result<(), Error> {
    let mut list = corrections(next);
    let c = correction_mut(&mut list, mask)?;
    part_range(c, mask, part)?;
    let ranges = masks::parts(masks::components(c));
    if to >= ranges.len() {
        return Err(invalid(
            "to",
            format!("mask {mask} has {} part(s), numbered from 0", ranges.len()),
        ));
    }
    let comps = components_mut(c);
    let mut chunks: Vec<Vec<Value>> = ranges.iter().map(|r| comps[r.clone()].to_vec()).collect();
    let moved = chunks.remove(part);
    chunks.insert(to, moved);
    let adds = |chunk: &[Value]| match chunk.first() {
        Some(Value::Struct(m)) => masks::Combination::of(m).mode == masks::Mode::Add,
        _ => true,
    };
    if !adds(&chunks[0]) {
        return Err(invalid(
            "to",
            "the first part of a mask must add: subtract and intersect act on the parts before them",
        ));
    }
    *comps = chunks.concat();
    store(next, list);
    Ok(())
}

/// Masks from a preset or another photo join the current ones; a mask with
/// the same `CorrectionSyncID` is replaced. `amount` (0-2) scales their local
/// sliders, as Lightroom's preset Amount does.
pub fn merge(next: &mut BTreeMap<String, Value>, incoming: &Value, amount: f64) {
    let Value::Seq(items) = incoming else { return };
    let mut list = corrections(next);
    for item in items {
        let Value::Struct(c) = item else { continue };
        let mut c = c.clone();
        if amount != 1.0 {
            for s in LOCAL_SLIDERS.iter().filter(|s| s.tag != "LocalToningHue") {
                if let Some(v) = masks::num(&c, s.tag) {
                    let ui = (v * s.scale * amount).clamp(s.min, s.max);
                    c.insert(s.tag.into(), number(ui / s.scale));
                }
            }
        }
        let id = masks::text(&c, "CorrectionSyncID");
        let same = |v: &Value| match v {
            Value::Struct(m) => id.is_some() && masks::text(m, "CorrectionSyncID") == id,
            _ => false,
        };
        match list.iter().position(same) {
            Some(i) => list[i] = Value::Struct(c),
            None => list.push(Value::Struct(c)),
        }
    }
    store(next, list);
}

#[cfg(test)]
mod tests;
