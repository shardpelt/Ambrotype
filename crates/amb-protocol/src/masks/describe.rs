// SPDX-License-Identifier: GPL-3.0-or-later
//! The `list_masks` description of a settings map's masks.

use super::*;

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
        "entries": entries.len(),
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
            let strokes = masks::brush_strokes(entries);
            d["strokes"] = json!(strokes.len());
            if masks::uses_brush_table(first) {
                d["in_acr_file"] = json!(true);
                d["rendered"] = json!(false);
            }
            d["dabs"] = json!(
                strokes
                    .iter()
                    .map(|e| match e.get("Dabs") {
                        Some(Value::Seq(s)) => s
                            .iter()
                            .filter_map(Value::as_str)
                            .filter(|t| masks::parse_dab(t).is_some())
                            .count(),
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
