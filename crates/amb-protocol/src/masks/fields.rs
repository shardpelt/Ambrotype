// SPDX-License-Identifier: GPL-3.0-or-later
//! Helpers that read and write a mask's fields: lookups, new masks and parts,
//! argument parsing and applying values.

use super::*;

pub(super) fn invalid(name: impl Into<String>, reason: impl Into<String>) -> Error {
    Error::InvalidValue {
        name: name.into(),
        reason: reason.into(),
    }
}

pub(super) fn corrections(settings: &BTreeMap<String, Value>) -> Vec<Value> {
    match settings.get(MASK_TAG) {
        Some(Value::Seq(items)) => items.clone(),
        _ => Vec::new(),
    }
}

pub(super) fn store(next: &mut BTreeMap<String, Value>, list: Vec<Value>) {
    if list.is_empty() {
        next.remove(MASK_TAG);
    } else {
        next.insert(MASK_TAG.to_string(), Value::Seq(list));
    }
}

pub(super) fn correction_mut(list: &mut [Value], mask: usize) -> Result<&mut Fields, Error> {
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

pub(super) fn components_mut(c: &mut Fields) -> &mut Vec<Value> {
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

pub(super) fn part_range(
    c: &Fields,
    mask: usize,
    part: usize,
) -> Result<std::ops::Range<usize>, Error> {
    let parts = masks::parts(masks::components(c));
    let n = parts.len();
    parts.get(part).cloned().ok_or_else(|| {
        invalid(
            format!("mask {mask} part {part}"),
            format!("mask {mask} has {n} part(s), numbered from 0"),
        )
    })
}

pub(super) fn new_correction(index: usize) -> Fields {
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

pub(super) fn parse_mode(m: &str) -> Result<Mode, Error> {
    Mode::parse(m)
        .ok_or_else(|| invalid("mode", format!("{m:?} is not add, subtract or intersect")))
}

/// Write a combination into a single (non-brush) component, with the
/// mirrors Lightroom keeps.
pub(super) fn set_combination(e: &mut Fields, comb: Combination) {
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
pub(super) fn set_brush_combination(
    strokes: &mut [Value],
    comb: Combination,
    density: Option<f64>,
) {
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

pub(super) fn new_component(kind: &str, comb: Combination) -> Result<Fields, Error> {
    let mut e: Fields = BTreeMap::new();
    let what = match kind {
        "linear" => "Mask/Gradient",
        "radial" => "Mask/CircularGradient",
        "luminance" | "color" => "Mask/RangeMask",
        "brush" => "Mask/Paint",
        // Lightroom's Select Sky; we find the sky with our own rules.
        "sky" => "Mask/Image",
        // Lightroom's Select Subject, and Select Background (the subject,
        // inverted); found by a local model (amb-ai).
        "subject" | "background" => "Mask/Image",
        // Lightroom's Select People; everyone, from a local model.
        "people" => "Mask/Image",
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
        "sky" => "Sky",
        "subject" => "Subject",
        "background" => "Background",
        "people" => "People",
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
        "sky" => {
            e.insert("MaskSubType".into(), Value::Text("2".into()));
        }
        "subject" | "background" => {
            e.insert("MaskSubType".into(), Value::Text("1".into()));
        }
        "people" => {
            e.insert("MaskSubType".into(), Value::Text("3".into()));
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
pub(super) fn number_arg(name: &str, v: &serde_json::Value) -> Result<f64, Error> {
    json_number(v)
        .filter(|n| n.is_finite())
        .ok_or_else(|| invalid(name, format!("expects a number, got {v}")))
}

pub(super) fn bool_arg(name: &str, v: &serde_json::Value) -> Result<bool, Error> {
    match v {
        serde_json::Value::Bool(b) => Ok(*b),
        serde_json::Value::String(s) => Value::Text(s.clone())
            .as_bool()
            .ok_or_else(|| invalid(name, format!("expects true or false, got {v}"))),
        _ => Err(invalid(name, format!("expects true or false, got {v}"))),
    }
}

pub(super) fn clamp(name: &str, v: f64, lo: f64, hi: f64, clamped: &mut Vec<Clamp>) -> f64 {
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
pub(super) fn slider_for(key: &str) -> Option<&'static LocalSlider> {
    let k = key.trim();
    LOCAL_SLIDERS
        .iter()
        .find(|s| s.tag.eq_ignore_ascii_case(k) || s.label.eq_ignore_ascii_case(k))
}

pub(super) const MASK_KEYS: &[&str] = &["name", "active", "amount"];
pub(super) const PART_KEYS: &[&str] = &[
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
pub(super) fn set_correction_values(
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
pub(super) fn set_component_values(
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

pub(super) fn range_struct(e: &mut Fields) -> &mut Fields {
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

pub(super) fn lum_range_arg(name: &str, v: &serde_json::Value) -> Result<[f64; 4], Error> {
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

pub(super) fn points_arg(name: &str, v: &serde_json::Value) -> Result<Vec<(f64, f64)>, Error> {
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
