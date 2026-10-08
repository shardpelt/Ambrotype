// SPDX-License-Identifier: GPL-3.0-or-later
//! Border presets for export: named borders (`amb_export::border::Border`).
//! The built-ins are `data/border-presets.json` (read-only); users add their
//! own, kept in the app's settings file like size presets.

use amb_export::border::Border;
use serde::{Deserialize, Serialize};

const BUILTIN_JSON: &str = include_str!("../data/border-presets.json");

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BorderPreset {
    /// `builtin:<slug>` or `custom:<n>`; empty for a new preset.
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub builtin: bool,
    #[serde(flatten)]
    pub border: Border,
}

#[derive(Deserialize)]
struct BuiltinFile {
    presets: Vec<Border>,
}

fn slug(name: &str) -> String {
    let s: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    s.split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// The built-in presets, in file order.
pub fn builtin() -> Vec<BorderPreset> {
    let file: BuiltinFile =
        serde_json::from_str(BUILTIN_JSON).expect("data/border-presets.json is valid");
    file.presets
        .into_iter()
        .map(|border| BorderPreset {
            id: format!("builtin:{}", slug(&border.name)),
            builtin: true,
            border,
        })
        .collect()
}

/// Built-ins followed by the user's (invalid ones dropped).
pub fn all(custom: &[BorderPreset]) -> Vec<BorderPreset> {
    let mut out = builtin();
    out.extend(
        custom
            .iter()
            .filter(|p| p.id.starts_with("custom:") && validate(p).is_ok())
            .cloned(),
    );
    out
}

fn validate(p: &BorderPreset) -> Result<BorderPreset, String> {
    let mut p = p.clone();
    p.border.name = p.border.name.trim().to_string();
    if p.border.name.is_empty() {
        return Err("a border preset needs a name".into());
    }
    if p.border.name.len() > 80 {
        return Err("that name is too long".into());
    }
    p.border.validate()?;
    p.builtin = false;
    Ok(p)
}

/// Add a user preset (or a changed copy of a built-in), or update one. Returns its id.
pub fn upsert(custom: &mut Vec<BorderPreset>, p: &BorderPreset) -> Result<String, String> {
    let mut p = validate(p)?;
    if let Some(existing) = custom.iter_mut().find(|c| !p.id.is_empty() && c.id == p.id) {
        *existing = p.clone();
        return Ok(p.id);
    }
    let next = custom
        .iter()
        .filter_map(|c| c.id.strip_prefix("custom:")?.parse::<u64>().ok())
        .max()
        .map_or(1, |n| n + 1);
    p.id = format!("custom:{next}");
    custom.push(p.clone());
    Ok(p.id)
}

/// Delete a user preset. Built-ins can't be deleted.
pub fn remove(custom: &mut Vec<BorderPreset>, id: &str) -> Result<(), String> {
    if id.starts_with("builtin:") {
        return Err("built-in border presets can't be deleted".into());
    }
    let before = custom.len();
    custom.retain(|c| c.id != id);
    if custom.len() == before {
        return Err(format!("no border preset {id}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_are_valid_and_unique() {
        let b = builtin();
        assert!(b.len() >= 6);
        let mut ids: Vec<_> = b.iter().map(|p| p.id.clone()).collect();
        for p in &b {
            p.border
                .validate()
                .unwrap_or_else(|e| panic!("{}: {e}", p.id));
        }
        ids.dedup();
        assert_eq!(ids.len(), b.len());
        assert!(b.iter().any(|p| p.border.fit));
    }

    #[test]
    fn add_update_delete() {
        let mut custom = Vec::new();
        let mut p = builtin()[0].clone();
        p.id = String::new();
        p.border.name = "  Mine  ".into();
        let id = upsert(&mut custom, &p).unwrap();
        assert_eq!(id, "custom:1");
        assert_eq!(custom[0].border.name, "Mine");
        assert!(!custom[0].builtin);
        p.id = id.clone();
        p.border.width = 9.0;
        assert_eq!(upsert(&mut custom, &p).unwrap(), id);
        assert_eq!(custom.len(), 1);
        p.border.color = "pink".into();
        assert!(upsert(&mut custom, &p).is_err());
        assert_eq!(all(&custom).last().unwrap().border.width, 9.0);
        assert!(remove(&mut custom, "builtin:thin-white").is_err());
        remove(&mut custom, &id).unwrap();
        assert!(custom.is_empty());
    }
}
