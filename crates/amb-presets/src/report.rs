//! Per-preset fidelity/inspection report: which settings a preset carries, how
//! each maps onto our implementation status, and warnings about content that
//! cannot be reproduced as-is (profiles, AI masks, legacy process versions, ...).

use std::collections::BTreeMap;

use serde::Serialize;

use crate::registry::{Registry, Status};
use crate::value::Value;
use crate::{Preset, SourceFormat};

#[derive(Debug, Serialize)]
pub struct TagEntry {
    pub name: String,
    pub group: String,
    pub status: Option<Status>,
    /// Short rendering of the value for display.
    pub value: String,
    /// Set when a numeric value falls outside the registry's known range.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub out_of_range: Option<(f64, f64)>,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub format: SourceFormat,
    pub name: Option<String>,
    pub group: Option<String>,
    pub process_version: Option<String>,
    pub supports_amount: bool,
    pub tags: Vec<TagEntry>,
    pub unknown: Vec<String>,
    pub counts: BTreeMap<String, usize>,
    pub warnings: Vec<String>,
}

fn summarize(value: &Value) -> String {
    match value {
        Value::Text(s) => {
            let mut s: String = s.chars().take(60).collect();
            if s.len() < value.as_str().map_or(0, str::len) {
                s.push('…');
            }
            s
        }
        Value::Number(n) => format!("{n}"),
        Value::Bool(b) => format!("{b}"),
        Value::Seq(items) => format!("[{} items]", items.len()),
        Value::Alt(items) => value
            .display_text()
            .map_or_else(|| format!("[{} langs]", items.len()), str::to_string),
        Value::Struct(map) => format!("{{{} fields}}", map.len()),
    }
}

fn status_key(status: Option<Status>) -> String {
    match status {
        Some(s) => serde_json::to_value(s)
            .ok()
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_default(),
        None => "unknown".into(),
    }
}

/// Mask kinds that depend on Adobe's AI models; we must regenerate these ourselves.
fn ai_mask_kinds(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Struct(map) => {
            for (k, v) in map {
                if k == "What" || k == "MaskSubType" || k == "MaskName" {
                    if let Some(s) = v.as_str() {
                        let lower = s.to_ascii_lowercase();
                        if [
                            "subject",
                            "sky",
                            "person",
                            "people",
                            "background",
                            "object",
                            "landscape",
                            "depth",
                        ]
                        .iter()
                        .any(|w| lower.contains(w))
                        {
                            out.push(s.to_string());
                        }
                    }
                }
                ai_mask_kinds(v, out);
            }
        }
        Value::Seq(items) => items.iter().for_each(|v| ai_mask_kinds(v, out)),
        _ => {}
    }
}

pub fn inspect(preset: &Preset, registry: &Registry) -> Report {
    let mut tags = Vec::new();
    let mut unknown = Vec::new();
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();

    for (name, value) in &preset.settings {
        let info = registry.lookup(name);
        let status = info.as_ref().map(|i| i.status);
        *counts.entry(status_key(status)).or_default() += 1;
        let out_of_range = info.as_ref().and_then(|i| {
            let v = value.as_f64()?;
            let (lo, hi) = (i.min?, i.max?);
            (i.kind == "number" && (v < lo || v > hi)).then_some((lo, hi))
        });
        if info.is_none() {
            unknown.push(name.clone());
        }
        tags.push(TagEntry {
            name: name.clone(),
            group: info.map_or_else(|| "unknown".into(), |i| i.group),
            status,
            value: summarize(value),
            out_of_range,
        });
    }

    let mut warnings = Vec::new();
    match preset.process_version() {
        Some(pv) if pv.starts_with("5.") || pv.starts_with("6.") => warnings.push(format!(
            "process version {pv} is pre-2012 (PV2003/PV2010): legacy tone tags need conversion and will only be approximated"
        )),
        None if preset.format != SourceFormat::XmpSidecar => {
            warnings.push("no ProcessVersion declared; assuming the current process version".into())
        }
        _ => {}
    }
    if let Some(profile) = preset.get("CameraProfile").and_then(Value::as_str) {
        warnings.push(format!(
            "references camera profile \"{profile}\": Adobe profiles are not bundled; uses the user's installed copy or a fitted substitute"
        ));
    }
    if let Some(look) = preset.get("Look").and_then(Value::as_struct) {
        let name = look
            .get("Name")
            .and_then(Value::display_text)
            .unwrap_or("(unnamed)");
        let has_table = look.values().any(|v| matches!(v, Value::Struct(m) if m.keys().any(|k| k.starts_with("Table_") || k == "RGBTable")));
        warnings.push(format!(
            "applies profile/look \"{name}\"{}",
            if has_table {
                " with an embedded LUT (decoding not implemented yet)"
            } else {
                ""
            }
        ));
    }
    let mut ai = Vec::new();
    for key in [
        "MaskGroupBasedCorrections",
        "GradientBasedCorrections",
        "CircularGradientBasedCorrections",
        "PaintBasedCorrections",
    ] {
        if let Some(v) = preset.get(key) {
            ai_mask_kinds(v, &mut ai);
        }
    }
    if !ai.is_empty() {
        ai.sort();
        ai.dedup();
        warnings.push(format!(
            "contains AI-dependent masks ({}): must be regenerated with our own segmentation",
            ai.join(", ")
        ));
    }
    if preset.get("RetouchAreas").is_some() || preset.get("RetouchInfo").is_some() {
        warnings.push(
            "contains spot-removal data, which is image-specific and not applied from presets"
                .into(),
        );
    }
    let out_of_range: Vec<&str> = tags
        .iter()
        .filter(|t| t.out_of_range.is_some())
        .map(|t| t.name.as_str())
        .collect();
    if !out_of_range.is_empty() {
        warnings.push(format!(
            "values outside the known range: {}",
            out_of_range.join(", ")
        ));
    }
    if !unknown.is_empty() {
        warnings.push(format!(
            "{} tag(s) not in the registry; preserved but ignored",
            unknown.len()
        ));
    }

    Report {
        format: preset.format,
        name: preset.name().map(str::to_string),
        group: preset.group().map(str::to_string),
        process_version: preset.process_version().map(str::to_string),
        supports_amount: preset.supports_amount(),
        tags,
        unknown,
        counts,
        warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_profiles_unknowns_and_ranges() {
        let lua = r#"s = { title = "T", value = { settings = {
            ProcessVersion = "6.7", Exposure2012 = 9, CameraProfile = "Adobe Color", MadeUpTag = 1,
        } } }"#;
        let preset = Preset::parse(lua).unwrap();
        let report = inspect(&preset, Registry::builtin());
        assert_eq!(report.unknown, vec!["MadeUpTag".to_string()]);
        let joined = report.warnings.join("\n");
        assert!(joined.contains("pre-2012"));
        assert!(joined.contains("Adobe Color"));
        assert!(joined.contains("Exposure2012"));
        assert_eq!(report.counts["unknown"], 1);
    }
}
