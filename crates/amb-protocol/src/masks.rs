// SPDX-License-Identifier: GPL-3.0-or-later
//! Mask commands: `add_mask`, `set_mask`, `paint_mask`, `remove_mask`, and
//! the `list_masks` description. They edit `MaskGroupBasedCorrections` in
//! Lightroom's own layout ([`amb_presets::masks`]), so masks made here open in
//! Lightroom and the other way round. Masks and parts are addressed by their
//! position (0-based) as `list_masks` shows them.

use std::collections::BTreeMap;

use amb_presets::Value;
use amb_presets::masks::{
    self, COLOR_SAMPLES, Combination, LOCAL_SLIDERS, LocalSlider, MASK_TAG, MaskKind, Mode,
    boolean, number,
};
use serde_json::{Map, json};

use crate::Error;
use crate::convert::{Clamp, json_number};

type Fields = BTreeMap<String, Value>;

/// Kinds `add_mask` can make (a brush is made by `paint_mask`).
pub const ADD_KINDS: &[&str] = &["linear", "radial", "luminance", "color"];

/// Most dabs one stroke may hold.
const MAX_DABS: usize = 20_000;

fn invalid(name: impl Into<String>, reason: impl Into<String>) -> Error {
    Error::InvalidValue {
        name: name.into(),
        reason: reason.into(),
    }
}

fn corrections(settings: &BTreeMap<String, Value>) -> Vec<Value> {
    match settings.get(MASK_TAG) {
        Some(Value::Seq(items)) => items.clone(),
        _ => Vec::new(),
    }
}

fn store(next: &mut BTreeMap<String, Value>, list: Vec<Value>) {
    if list.is_empty() {
        next.remove(MASK_TAG);
    } else {
        next.insert(MASK_TAG.to_string(), Value::Seq(list));
    }
}

fn correction_mut(list: &mut [Value], mask: usize) -> Result<&mut Fields, Error> {
    let n = list.len();
    match list.get_mut(mask) {
        Some(Value::Struct(m)) => Ok(m),
        Some(_) => Err(invalid(format!("mask {mask}"), "is not a correction")),
        None => Err(invalid(
            format!("mask {mask}"),
            if n == 0 {
                "there are no masks yet; add one with add_mask or paint_mask".to_string()
            } else {
                format!("there are {n} mask(s), numbered from 0")
            },
        )),
    }
}

fn components_mut(c: &mut Fields) -> &mut Vec<Value> {
    let entry = c
        .entry("CorrectionMasks".to_string())
        .or_insert_with(|| Value::Seq(Vec::new()));
    if !matches!(entry, Value::Seq(_)) {
        *entry = Value::Seq(Vec::new());
    }
    match entry {
        Value::Seq(items) => items,
        _ => unreachable!(),
    }
}

fn part_range(c: &Fields, mask: usize, part: usize) -> Result<std::ops::Range<usize>, Error> {
    let parts = masks::parts(masks::components(c));
    let n = parts.len();
    parts.get(part).cloned().ok_or_else(|| {
        invalid(
            format!("mask {mask} part {part}"),
            format!("mask {mask} has {n} part(s), numbered from 0"),
        )
    })
}

fn new_correction(index: usize) -> Fields {
    [
        ("What", Value::Text("Correction".into())),
        ("CorrectionAmount", number(1.0)),
        ("CorrectionActive", boolean(true)),
        ("CorrectionName", Value::Text(format!("Mask {}", index + 1))),
        ("CorrectionSyncID", Value::Text(masks::new_sync_id())),
        ("CorrectionMasks", Value::Seq(Vec::new())),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v))
    .collect()
}

fn parse_mode(m: &str) -> Result<Mode, Error> {
    Mode::parse(m)
        .ok_or_else(|| invalid("mode", format!("{m:?} is not add, subtract or intersect")))
}

/// Write a combination into a single (non-brush) component, with the
/// mirrors Lightroom keeps.
fn set_combination(e: &mut Fields, comb: Combination) {
    let (blend, value, inverted) = comb.stored();
    e.insert("MaskBlendMode".into(), Value::Text(blend.to_string()));
    e.insert("MaskValue".into(), number(value));
    e.insert("MaskInverted".into(), boolean(inverted));
    match masks::text(e, "What").as_deref() {
        Some("Mask/CircularGradient") => {
            e.insert("Flipped".into(), boolean(!inverted));
        }
        Some("Mask/RangeMask") => {
            if let Some(Value::Struct(r)) = e.get_mut("CorrectionRangeMask") {
                r.insert("Invert".into(), boolean(inverted));
            }
        }
        _ => {}
    }
}

/// Rewrite a brush part's strokes for `comb`, keeping which strokes erase.
fn set_brush_combination(strokes: &mut [Value], comb: Combination, density: Option<f64>) {
    let old_first_zero = strokes
        .first()
        .and_then(|s| match s {
            Value::Struct(m) => masks::num(m, "MaskValue"),
            _ => None,
        })
        .is_some_and(|v| v == 0.0);
    let old_density = strokes.first().and_then(|s| match s {
        Value::Struct(m) => masks::num(m, "MaskValue").filter(|v| *v > 0.0),
        _ => None,
    });
    let (blend, value, inverted) = comb.stored();
    let paint_value = if value == 0.0 {
        0.0
    } else {
        density.or(old_density).unwrap_or(1.0)
    };
    let erase_value = if paint_value == 0.0 { 1.0 } else { 0.0 };
    for s in strokes {
        let Value::Struct(m) = s else { continue };
        let was_zero = masks::num(m, "MaskValue").unwrap_or(1.0) == 0.0;
        let erase = was_zero != old_first_zero;
        m.insert("MaskBlendMode".into(), Value::Text(blend.to_string()));
        m.insert("MaskInverted".into(), boolean(inverted));
        m.insert(
            "MaskValue".into(),
            number(if erase { erase_value } else { paint_value }),
        );
    }
}

fn new_component(kind: &str, comb: Combination) -> Result<Fields, Error> {
    let mut e: Fields = BTreeMap::new();
    let what = match kind {
        "linear" => "Mask/Gradient",
        "radial" => "Mask/CircularGradient",
        "luminance" | "color" => "Mask/RangeMask",
        "brush" => "Mask/Paint",
        _ => {
            return Err(invalid(
                "kind",
                format!(
                    "{kind:?} is not one of: {}, brush (via paint_mask)",
                    ADD_KINDS.join(", ")
                ),
            ));
        }
    };
    let label = match kind {
        "linear" => "Linear Gradient",
        "radial" => "Radial Gradient",
        "luminance" => "Luminance Range",
        "color" => "Color Range",
        _ => "Brush",
    };
    e.insert("What".into(), Value::Text(what.into()));
    e.insert("MaskActive".into(), boolean(true));
    e.insert("MaskName".into(), Value::Text(label.into()));
    e.insert("MaskSyncID".into(), Value::Text(masks::new_sync_id()));
    let n = |v: f64| number(v);
    match kind {
        "linear" => {
            for (k, v) in [
                ("ZeroX", 0.5),
                ("ZeroY", 0.5),
                ("FullX", 0.5),
                ("FullY", 0.2),
            ] {
                e.insert(k.into(), n(v));
            }
        }
        "radial" => {
            for (k, v) in [
                ("Top", 0.25),
                ("Left", 0.25),
                ("Bottom", 0.75),
                ("Right", 0.75),
                ("Angle", 0.0),
                ("Midpoint", 50.0),
                ("Roundness", 0.0),
                ("Feather", 50.0),
            ] {
                e.insert(k.into(), n(v));
            }
            e.insert("Version".into(), Value::Text("2".into()));
        }
        "luminance" | "color" => {
            let mut r: Fields = BTreeMap::new();
            r.insert("Version".into(), Value::Text("3".into()));
            r.insert(
                "Type".into(),
                Value::Text(if kind == "color" { "1" } else { "2" }.into()),
            );
            r.insert("SampleType".into(), Value::Text("0".into()));
            if kind == "color" {
                r.insert("ColorAmount".into(), n(0.5));
                r.insert(COLOR_SAMPLES.into(), Value::Seq(Vec::new()));
            } else {
                r.insert(
                    "LumRange".into(),
                    Value::Text(masks::format_lum_range([0.5, 0.6, 1.0, 1.0])),
                );
            }
            e.insert("CorrectionRangeMask".into(), Value::Struct(r));
        }
        _ => {}
    }
    set_combination(&mut e, comb);
    Ok(e)
}

/// A finite number from a command value.
fn number_arg(name: &str, v: &serde_json::Value) -> Result<f64, Error> {
    json_number(v)
        .filter(|n| n.is_finite())
        .ok_or_else(|| invalid(name, format!("expects a number, got {v}")))
}

fn bool_arg(name: &str, v: &serde_json::Value) -> Result<bool, Error> {
    match v {
        serde_json::Value::Bool(b) => Ok(*b),
        serde_json::Value::String(s) => Value::Text(s.clone())
            .as_bool()
            .ok_or_else(|| invalid(name, format!("expects true or false, got {v}"))),
        _ => Err(invalid(name, format!("expects true or false, got {v}"))),
    }
}

fn clamp(name: &str, v: f64, lo: f64, hi: f64, clamped: &mut Vec<Clamp>) -> f64 {
    let c = v.clamp(lo, hi);
    if c != v {
        clamped.push(Clamp {
            name: name.to_string(),
            requested: v,
            applied: c,
            min: lo,
            max: hi,
        });
    }
    c
}

/// A local slider by tag (`LocalExposure2012`) or label (`Exposure`).
fn slider_for(key: &str) -> Option<&'static LocalSlider> {
    let k = key.trim();
    LOCAL_SLIDERS
        .iter()
        .find(|s| s.tag.eq_ignore_ascii_case(k) || s.label.eq_ignore_ascii_case(k))
}

const MASK_KEYS: &[&str] = &["name", "active", "amount"];
const PART_KEYS: &[&str] = &[
    "mode",
    "invert",
    "active",
    "name",
    "ZeroX",
    "ZeroY",
    "FullX",
    "FullY",
    "Top",
    "Left",
    "Bottom",
    "Right",
    "Angle",
    "Feather",
    "LumRange",
    "ColorSamples",
    "ColorAmount",
];

/// Correction-level values: name, active, amount (0-100) and local sliders
/// in UI units.
fn set_correction_values(
    c: &mut Fields,
    mask: usize,
    values: &Map<String, serde_json::Value>,
    clamped: &mut Vec<Clamp>,
) -> Result<(), Error> {
    for (key, v) in values {
        let at = format!("mask {mask} {key}");
        match key.as_str() {
            "name" => {
                let s = v.as_str().ok_or_else(|| invalid(&at, "expects a string"))?;
                c.insert("CorrectionName".into(), Value::Text(s.to_string()));
            }
            "active" => {
                c.insert("CorrectionActive".into(), boolean(bool_arg(&at, v)?));
            }
            "amount" => {
                let a = clamp(&at, number_arg(&at, v)?, 0.0, 100.0, clamped);
                c.insert("CorrectionAmount".into(), number(a / 100.0));
            }
            _ => {
                let Some(s) = slider_for(key) else {
                    let known: Vec<&str> = MASK_KEYS
                        .iter()
                        .copied()
                        .chain(LOCAL_SLIDERS.iter().map(|s| s.tag))
                        .collect();
                    return Err(invalid(
                        &at,
                        format!("unknown mask value; use one of: {}", known.join(", ")),
                    ));
                };
                let ui = clamp(
                    &format!("mask {mask} {}", s.tag),
                    number_arg(&at, v)?,
                    s.min,
                    s.max,
                    clamped,
                );
                c.insert(s.tag.into(), number(ui / s.scale));
            }
        }
    }
    Ok(())
}

/// Part-level values on one component (not a brush).
fn set_component_values(
    e: &mut Fields,
    at: &str,
    values: &Map<String, serde_json::Value>,
    clamped: &mut Vec<Clamp>,
) -> Result<(), Error> {
    let kind = MaskKind::of(e);
    let mut comb = Combination::of(e);
    let mut comb_changed = false;
    for (key, v) in values {
        let name = format!("{at} {key}");
        let geometry = |kinds: &[MaskKind]| -> Result<(), Error> {
            if kinds.contains(&kind) {
                Ok(())
            } else {
                Err(invalid(&name, format!("is not a {} setting", kind.label())))
            }
        };
        match key.as_str() {
            "mode" => {
                let m = v
                    .as_str()
                    .ok_or_else(|| invalid(&name, "expects a string"))?;
                comb.mode = parse_mode(m)?;
                comb_changed = true;
            }
            "invert" => {
                comb.inverted = bool_arg(&name, v)?;
                comb_changed = true;
            }
            "active" => {
                e.insert("MaskActive".into(), boolean(bool_arg(&name, v)?));
            }
            "name" => {
                let s = v
                    .as_str()
                    .ok_or_else(|| invalid(&name, "expects a string"))?;
                e.insert("MaskName".into(), Value::Text(s.to_string()));
            }
            "ZeroX" | "ZeroY" | "FullX" | "FullY" => {
                geometry(&[MaskKind::Linear])?;
                let x = clamp(&name, number_arg(&name, v)?, -10.0, 11.0, clamped);
                e.insert(key.clone(), number(x));
            }
            "Top" | "Left" | "Bottom" | "Right" => {
                geometry(&[MaskKind::Radial])?;
                let x = clamp(&name, number_arg(&name, v)?, -10.0, 11.0, clamped);
                e.insert(key.clone(), number(x));
            }
            "Angle" => {
                geometry(&[MaskKind::Radial])?;
                let a = clamp(&name, number_arg(&name, v)?, -180.0, 180.0, clamped);
                e.insert(key.clone(), number(a));
            }
            "Feather" => {
                geometry(&[MaskKind::Radial])?;
                let f = clamp(&name, number_arg(&name, v)?, 0.0, 100.0, clamped);
                e.insert(key.clone(), number(f));
            }
            "LumRange" => {
                geometry(&[MaskKind::Luminance])?;
                let r = lum_range_arg(&name, v)?;
                range_struct(e).insert(key.clone(), Value::Text(masks::format_lum_range(r)));
            }
            "ColorSamples" => {
                geometry(&[MaskKind::Color])?;
                let pts = points_arg(&name, v)?;
                let seq = pts
                    .into_iter()
                    .map(|(x, y)| Value::Text(format!("{x:.6} {y:.6}")))
                    .collect();
                range_struct(e).insert(COLOR_SAMPLES.into(), Value::Seq(seq));
            }
            "ColorAmount" => {
                geometry(&[MaskKind::Color])?;
                let a = clamp(&name, number_arg(&name, v)?, 0.0, 100.0, clamped);
                range_struct(e).insert(key.clone(), number(a / 100.0));
            }
            _ => {
                return Err(invalid(
                    &name,
                    format!("unknown part value; use one of: {}", PART_KEYS.join(", ")),
                ));
            }
        }
    }
    if comb_changed {
        set_combination(e, comb);
    }
    if kind == MaskKind::Radial {
        let g = |k: &str| masks::num(e, k).unwrap_or(0.0);
        if g("Top") >= g("Bottom") || g("Left") >= g("Right") {
            return Err(invalid(
                at,
                "a radial gradient needs Top < Bottom and Left < Right",
            ));
        }
    }
    if kind == MaskKind::Linear {
        let g = |k: &str| masks::num(e, k).unwrap_or(0.0);
        if (g("ZeroX") - g("FullX")).abs() < 1e-9 && (g("ZeroY") - g("FullY")).abs() < 1e-9 {
            return Err(invalid(at, "a linear gradient needs two different points"));
        }
    }
    Ok(())
}

fn range_struct(e: &mut Fields) -> &mut Fields {
    let r = e
        .entry("CorrectionRangeMask".to_string())
        .or_insert_with(|| Value::Struct(BTreeMap::new()));
    if !matches!(r, Value::Struct(_)) {
        *r = Value::Struct(BTreeMap::new());
    }
    match r {
        Value::Struct(m) => m,
        _ => unreachable!(),
    }
}

fn lum_range_arg(name: &str, v: &serde_json::Value) -> Result<[f64; 4], Error> {
    let nums: Vec<f64> = match v {
        serde_json::Value::Array(items) => items.iter().filter_map(json_number).collect(),
        serde_json::Value::String(s) => s
            .split_whitespace()
            .filter_map(|t| t.parse().ok())
            .collect(),
        _ => Vec::new(),
    };
    let ok = nums.len() == 4
        && nums.iter().all(|x| (0.0..=1.0).contains(x))
        && nums.windows(2).all(|w| w[0] <= w[1]);
    if !ok {
        return Err(invalid(
            name,
            "expects four numbers 0-1 in order: [none below, full from, full to, none above], e.g. [0.5, 0.6, 1, 1]",
        ));
    }
    Ok([nums[0], nums[1], nums[2], nums[3]])
}

fn points_arg(name: &str, v: &serde_json::Value) -> Result<Vec<(f64, f64)>, Error> {
    let items = v
        .as_array()
        .ok_or_else(|| invalid(name, "expects a list of [x, y] image fractions"))?;
    items
        .iter()
        .map(|p| match p.as_array().map(|a| a.as_slice()) {
            Some([x, y]) => match (json_number(x), json_number(y)) {
                (Some(x), Some(y)) if x.is_finite() && y.is_finite() => Ok((x, y)),
                _ => Err(invalid(name, format!("{p} is not [x, y]"))),
            },
            _ => Err(invalid(name, format!("{p} is not [x, y]"))),
        })
        .collect()
}

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
            inverted: false,
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
                set_brush_combination(&mut comps[range.clone()], comb, None);
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
    /// Brush radius as a fraction of the image's longer side.
    pub size: f64,
    /// 0-100.
    pub feather: f64,
    pub flow: f64,
    pub density: f64,
    pub erase: bool,
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
    if stroke.points.is_empty() {
        return Err(invalid(
            "points",
            "a stroke needs at least one [x, y] point",
        ));
    }
    if stroke.points.iter().flatten().any(|v| !v.is_finite()) {
        return Err(invalid("points", "points must be finite numbers"));
    }
    let size = clamp("size", stroke.size, 0.001, 0.5, clamped);
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
    // Dabs about a quarter of the size apart, so strokes come out smooth.
    let step = size * 0.25;
    let mut dabs: Vec<(f64, f64)> = Vec::new();
    for p in stroke.points {
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

    let mut list = corrections(next);
    let index = mask.unwrap_or(list.len());
    if mask.is_none() {
        list.push(Value::Struct(new_correction(index)));
    }
    let c = correction_mut(&mut list, index)?;
    let (insert_at, sync_id, comb, first_value) = match part {
        Some(p) => {
            let range = part_range(c, index, p)?;
            let comps = masks::components(c);
            let Some(Value::Struct(first)) = comps.get(range.start) else {
                return Err(invalid(
                    format!("mask {index} part {p}"),
                    "is not a mask component",
                ));
            };
            if MaskKind::of(first) != MaskKind::Brush {
                return Err(invalid(
                    format!("mask {index} part {p}"),
                    format!("is a {}, not a brush", MaskKind::of(first).label()),
                ));
            }
            (
                range.end,
                masks::text(first, "MaskSyncID").unwrap_or_default(),
                Combination::of(first),
                masks::num(first, "MaskValue").unwrap_or(1.0),
            )
        }
        None => {
            let comb = Combination {
                mode: mode.unwrap_or(Mode::Add),
                inverted: false,
            };
            (
                masks::components(c).len(),
                masks::new_sync_id(),
                comb,
                if comb.mode == Mode::Add { density } else { 0.0 },
            )
        }
    };
    if part.is_none() && stroke.erase {
        return Err(invalid(
            "erase",
            "there is nothing to erase in a new brush part",
        ));
    }
    let (blend, _, inverted) = comb.stored();
    let first_zero = first_value == 0.0;
    let value = match (stroke.erase, first_zero) {
        (false, false) => density,
        (false, true) => 0.0,
        (true, false) => 0.0,
        (true, true) => 1.0,
    };
    let mut e: Fields = BTreeMap::new();
    e.insert("What".into(), Value::Text("Mask/Paint".into()));
    e.insert("MaskActive".into(), boolean(true));
    e.insert("MaskName".into(), Value::Text("Brush".into()));
    e.insert("MaskBlendMode".into(), Value::Text(blend.to_string()));
    e.insert("MaskInverted".into(), boolean(inverted));
    e.insert("MaskSyncID".into(), Value::Text(sync_id));
    e.insert("MaskValue".into(), number(value));
    e.insert("Radius".into(), number(size));
    e.insert("Flow".into(), number(flow / 100.0));
    e.insert("CenterWeight".into(), number(1.0 - feather / 100.0));
    e.insert(
        "Dabs".into(),
        Value::Seq(
            dabs.into_iter()
                .map(|(x, y)| Value::Text(masks::format_dab(x, y)))
                .collect(),
        ),
    );
    components_mut(c).insert(insert_at, Value::Struct(e));
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

// ---------------------------------------------------------------- list_masks

/// The masks of `settings` as the editor and AI clients see them: names,
/// sliders in UI units, and parts with their geometry.
pub fn describe(settings: &BTreeMap<String, Value>) -> serde_json::Value {
    let list = corrections(settings);
    let masks: Vec<serde_json::Value> = list
        .iter()
        .enumerate()
        .filter_map(|(i, c)| match c {
            Value::Struct(c) => Some(describe_correction(i, c)),
            _ => None,
        })
        .collect();
    let mut out = json!({ "masks": masks });
    let legacy: Vec<&str> = masks::LEGACY_TAGS
        .iter()
        .map(|(t, _)| *t)
        .filter(|t| settings.contains_key(*t))
        .collect();
    if !legacy.is_empty() {
        out["legacy"] = json!(legacy);
        out["note"] =
            json!("older Lightroom masks are rendered but can only be edited in their raw tags");
    }
    out
}

fn describe_correction(index: usize, c: &Fields) -> serde_json::Value {
    let mut sliders = Map::new();
    for s in LOCAL_SLIDERS {
        if let Some(v) = masks::num(c, s.tag).filter(|v| *v != 0.0) {
            let ui = v * s.scale;
            sliders.insert(s.tag.into(), json!((ui * 1e4).round() / 1e4));
        }
    }
    let comps = masks::components(c);
    let parts: Vec<serde_json::Value> = masks::parts(comps)
        .into_iter()
        .enumerate()
        .filter_map(|(p, range)| {
            let entries: Vec<&Fields> = comps[range]
                .iter()
                .filter_map(|v| match v {
                    Value::Struct(m) => Some(m),
                    _ => None,
                })
                .collect();
            entries
                .first()
                .map(|first| describe_part(p, first, &entries))
        })
        .collect();
    json!({
        "index": index,
        "name": masks::text(c, "CorrectionName").unwrap_or_else(|| format!("Mask {}", index + 1)),
        "active": masks::flag(c, "CorrectionActive").unwrap_or(true),
        "amount": (masks::num(c, "CorrectionAmount").unwrap_or(1.0) * 100.0).round(),
        "sliders": sliders,
        "parts": parts,
    })
}

fn describe_part(index: usize, first: &Fields, entries: &[&Fields]) -> serde_json::Value {
    let kind = MaskKind::of(first);
    let comb = Combination::of(first);
    let mut d = json!({
        "index": index,
        "kind": kind.name(),
        "label": masks::text(first, "MaskName").unwrap_or_else(|| kind.label()),
        "mode": comb.mode.name(),
        "invert": comb.inverted,
        "active": masks::flag(first, "MaskActive").unwrap_or(true),
        "rendered": kind.rendered(),
    });
    let g = |k: &str| masks::num(first, k).map_or(serde_json::Value::Null, |v| json!(v));
    match kind {
        MaskKind::Linear => {
            for k in ["ZeroX", "ZeroY", "FullX", "FullY"] {
                d[k] = g(k);
            }
        }
        MaskKind::Radial => {
            for k in ["Top", "Left", "Bottom", "Right", "Angle", "Feather"] {
                d[k] = g(k);
            }
        }
        MaskKind::Luminance => {
            if let Some(r) = masks::range_mask(first).and_then(masks::lum_range) {
                d["LumRange"] = json!(r);
            }
        }
        MaskKind::Color => {
            if let Some(r) = masks::range_mask(first) {
                let samples: Vec<[f64; 2]> = masks::color_samples(r)
                    .into_iter()
                    .map(|(x, y)| [x, y])
                    .collect();
                d["ColorSamples"] = json!(samples);
                d["ColorAmount"] =
                    json!((masks::num(r, "ColorAmount").unwrap_or(0.5) * 100.0).round());
            }
        }
        MaskKind::Brush => {
            d["strokes"] = json!(entries.len());
            d["dabs"] = json!(
                entries
                    .iter()
                    .map(|e| match e.get("Dabs") {
                        Some(Value::Seq(s)) => s.len(),
                        _ => 0,
                    })
                    .sum::<usize>()
            );
        }
        MaskKind::Ai(sub) => d["subtype"] = json!(sub),
        _ => {}
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obj(v: serde_json::Value) -> Map<String, serde_json::Value> {
        v.as_object().unwrap().clone()
    }

    #[test]
    fn add_set_paint_and_remove() {
        let mut s = BTreeMap::new();
        let mut clamped = Vec::new();
        add(
            &mut s,
            "linear",
            None,
            None,
            Some(&obj(json!({"ZeroY": 0.6, "FullY": 0.3}))),
            &mut clamped,
        )
        .unwrap();
        set(
            &mut s,
            0,
            None,
            &obj(json!({"Exposure": -0.5, "LocalHighlights2012": -30, "name": "Sky"})),
            &mut clamped,
        )
        .unwrap();
        add(
            &mut s,
            "luminance",
            Some(0),
            Some("intersect"),
            Some(&obj(json!({"LumRange": [0.3, 0.5, 1, 1]}))),
            &mut clamped,
        )
        .unwrap();
        paint(
            &mut s,
            Some(0),
            None,
            Some("subtract"),
            &Stroke {
                points: &[[0.1, 0.1], [0.2, 0.1]],
                size: 0.05,
                feather: 50.0,
                flow: 100.0,
                density: 100.0,
                erase: false,
            },
            &mut clamped,
        )
        .unwrap();
        assert!(clamped.is_empty(), "{clamped:?}");

        let Value::Seq(list) = &s[MASK_TAG] else {
            panic!()
        };
        let Value::Struct(c) = &list[0] else { panic!() };
        // Stored as Lightroom stores them.
        assert_eq!(
            masks::text(c, "LocalExposure2012").as_deref(),
            Some("-0.125")
        );
        assert_eq!(
            masks::text(c, "LocalHighlights2012").as_deref(),
            Some("-0.3")
        );
        let comps = masks::components(c);
        let Value::Struct(range) = &comps[1] else {
            panic!()
        };
        assert_eq!(masks::text(range, "MaskBlendMode").as_deref(), Some("1"));
        assert_eq!(masks::text(range, "MaskValue").as_deref(), Some("0"));
        assert_eq!(masks::flag(range, "MaskInverted"), Some(true));
        assert_eq!(
            masks::range_mask(range).and_then(|r| masks::flag(r, "Invert")),
            Some(true)
        );

        let d = describe(&s);
        let m = &d["masks"][0];
        assert_eq!(m["name"], "Sky");
        assert_eq!(m["sliders"]["LocalExposure2012"], -0.5);
        assert_eq!(m["parts"][1]["mode"], "intersect");
        assert_eq!(m["parts"][2]["kind"], "brush");
        assert_eq!(m["parts"][2]["mode"], "subtract");
        // Dabs fill the gap between the two points.
        assert!(m["parts"][2]["dabs"].as_u64().unwrap() >= 8, "{m}");

        // A second stroke joins the brush part; then erase in it.
        let stroke = |erase| Stroke {
            points: &[[0.3, 0.3]],
            size: 0.05,
            feather: 0.0,
            flow: 100.0,
            density: 100.0,
            erase,
        };
        paint(&mut s, Some(0), Some(2), None, &stroke(false), &mut clamped).unwrap();
        paint(&mut s, Some(0), Some(2), None, &stroke(true), &mut clamped).unwrap();
        assert_eq!(describe(&s)["masks"][0]["parts"][2]["strokes"], 3);

        set(
            &mut s,
            0,
            Some(2),
            &obj(json!({"mode": "add"})),
            &mut clamped,
        )
        .unwrap();
        let d = describe(&s);
        assert_eq!(d["masks"][0]["parts"][2]["mode"], "add");

        remove(&mut s, 0, Some(1)).unwrap();
        assert_eq!(
            describe(&s)["masks"][0]["parts"].as_array().unwrap().len(),
            2
        );
        remove(&mut s, 0, None).unwrap();
        assert!(!s.contains_key(MASK_TAG));
    }

    #[test]
    fn mode_and_invert_keep_lightroom_mirrors() {
        let mut s = BTreeMap::new();
        let mut clamped = Vec::new();
        add(&mut s, "radial", None, None, None, &mut clamped).unwrap();
        set(
            &mut s,
            0,
            Some(0),
            &obj(json!({"invert": true, "Feather": 120})),
            &mut clamped,
        )
        .unwrap();
        assert_eq!(clamped.len(), 1, "Feather clamped");
        let Value::Seq(list) = &s[MASK_TAG] else {
            panic!()
        };
        let Value::Struct(c) = &list[0] else { panic!() };
        let Value::Struct(e) = &masks::components(c)[0] else {
            panic!()
        };
        assert_eq!(masks::flag(e, "MaskInverted"), Some(true));
        assert_eq!(masks::flag(e, "Flipped"), Some(false));
    }

    #[test]
    fn mistakes_are_explained() {
        let mut s = BTreeMap::new();
        let mut clamped = Vec::new();
        let err = set(&mut s, 0, None, &obj(json!({"Exposure": 1})), &mut clamped).unwrap_err();
        assert!(err.to_string().contains("no masks yet"), "{err}");
        add(&mut s, "radial", None, None, None, &mut clamped).unwrap();
        let err = set(
            &mut s,
            0,
            Some(0),
            &obj(json!({"ZeroX": 0.2})),
            &mut clamped,
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("not a Radial Gradient setting"),
            "{err}"
        );
        let err = set(&mut s, 0, Some(0), &obj(json!({"Top": 0.9})), &mut clamped).unwrap_err();
        assert!(err.to_string().contains("Top < Bottom"), "{err}");
        let err = set(&mut s, 0, None, &obj(json!({"Exposur": 1})), &mut clamped).unwrap_err();
        assert!(err.to_string().contains("LocalExposure2012"), "{err}");
        let err = add(&mut s, "sky", None, None, None, &mut clamped).unwrap_err();
        assert!(err.to_string().contains("linear"), "{err}");
    }

    #[test]
    fn preset_masks_join_and_scale() {
        let mut s = BTreeMap::new();
        let mut clamped = Vec::new();
        add(&mut s, "linear", None, None, None, &mut clamped).unwrap();
        set(&mut s, 0, None, &obj(json!({"Exposure": 1})), &mut clamped).unwrap();
        let incoming = s[MASK_TAG].clone();
        // Same id: replaced, scaled to half.
        merge(&mut s, &incoming, 0.5);
        let d = describe(&s);
        assert_eq!(d["masks"].as_array().unwrap().len(), 1);
        assert_eq!(d["masks"][0]["sliders"]["LocalExposure2012"], 0.5);
    }
}
