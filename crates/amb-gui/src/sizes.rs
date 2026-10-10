// SPDX-License-Identifier: GPL-3.0-or-later
//! Size presets: named pixel sizes (Instagram portrait 1080×1350, A4 at
//! 300 ppi, ...). The crop tool locks its aspect to one; export renders
//! exactly that size. The built-in list is `data/size-presets.json`
//! (read-only, easy to edit as sites change); users add their own, which
//! are kept in the app's settings file.

use serde::{Deserialize, Serialize};

const BUILTIN_JSON: &str = include_str!("../data/size-presets.json");

/// The largest side we accept (JPEG's limit).
const MAX_SIDE: u32 = 65_535;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SizePreset {
    /// `builtin:<group>/<name>` or `custom:<n>`; empty for a new preset.
    #[serde(default)]
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub group: String,
    pub width: u32,
    pub height: u32,
    /// Built-ins can be cloned but not edited or deleted.
    #[serde(default)]
    pub builtin: bool,
}

#[derive(Deserialize)]
struct BuiltinFile {
    groups: Vec<BuiltinGroup>,
}

#[derive(Deserialize)]
struct BuiltinGroup {
    group: String,
    presets: Vec<BuiltinSize>,
}

#[derive(Deserialize)]
struct BuiltinSize {
    name: String,
    width: u32,
    height: u32,
}

/// The built-in presets, in file order.
pub fn builtin() -> Vec<SizePreset> {
    let file: BuiltinFile =
        serde_json::from_str(BUILTIN_JSON).expect("data/size-presets.json is valid");
    file.groups
        .into_iter()
        .flat_map(|g| {
            let group = g.group;
            g.presets.into_iter().map(move |p| SizePreset {
                id: format!("builtin:{group}/{}", p.name),
                name: p.name,
                group: group.clone(),
                width: p.width,
                height: p.height,
                builtin: true,
            })
        })
        .collect()
}

/// Built-ins followed by the user's presets (as saved; invalid ones dropped).
pub fn all(custom: &[SizePreset]) -> Vec<SizePreset> {
    let mut out = builtin();
    out.extend(custom.iter().filter_map(|p| {
        let p = validate(p).ok()?;
        p.id.starts_with("custom:").then_some(p)
    }));
    out
}

/// A cleaned-up copy of `p`, or why it cannot be saved.
fn validate(p: &SizePreset) -> Result<SizePreset, String> {
    let name = p.name.trim();
    if name.is_empty() {
        return Err("a size preset needs a name".into());
    }
    if !(1..=MAX_SIDE).contains(&p.width) || !(1..=MAX_SIDE).contains(&p.height) {
        return Err(format!(
            "width and height must be 1 to {MAX_SIDE} pixels (got {}×{})",
            p.width, p.height
        ));
    }
    let group = p.group.trim();
    Ok(SizePreset {
        id: p.id.clone(),
        name: name.to_string(),
        group: if group.is_empty() { "Custom" } else { group }.to_string(),
        width: p.width,
        height: p.height,
        builtin: false,
    })
}

/// Add or update a user preset; returns its id. A preset without an id, or
/// with a built-in's id (a clone), is added as new.
pub fn upsert(custom: &mut Vec<SizePreset>, p: &SizePreset) -> Result<String, String> {
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

/// Delete a user preset. Built-ins cannot be deleted.
pub fn remove(custom: &mut Vec<SizePreset>, id: &str) -> Result<(), String> {
    if id.starts_with("builtin:") {
        return Err("built-in size presets cannot be deleted".into());
    }
    let before = custom.len();
    custom.retain(|c| c.id != id);
    if custom.len() == before {
        return Err(format!("no size preset {id}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn preset(id: &str, name: &str, w: u32, h: u32) -> SizePreset {
        SizePreset {
            id: id.into(),
            name: name.into(),
            group: String::new(),
            width: w,
            height: h,
            builtin: false,
        }
    }

    #[test]
    fn builtins_load_with_unique_ids_and_sane_sizes() {
        let b = builtin();
        assert!(b.len() > 20);
        let ig = b
            .iter()
            .find(|p| p.group == "Instagram" && p.name.starts_with("Portrait 4:5"))
            .expect("Instagram portrait");
        assert_eq!((ig.width, ig.height), (1080, 1350));
        for site in [
            "Facebook",
            "Reddit",
            "TikTok",
            "Snapchat",
            "YouTube",
            "Pinterest",
            "LinkedIn",
        ] {
            assert!(b.iter().any(|p| p.group == site), "{site}");
        }
        let mut ids: Vec<&str> = b.iter().map(|p| p.id.as_str()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), b.len());
        assert!(b.iter().all(|p| p.builtin && validate(p).is_ok()));
    }

    #[test]
    fn custom_presets_merge_after_builtins_and_round_trip() {
        let mut custom = Vec::new();
        let id = upsert(&mut custom, &preset("", "  Blog header ", 1600, 600)).unwrap();
        assert_eq!(id, "custom:1");
        assert_eq!(custom[0].name, "Blog header");
        assert_eq!(custom[0].group, "Custom");
        // Cloning a built-in makes a new user preset.
        let mut clone = builtin()[0].clone();
        clone.name = "Mine".into();
        let id2 = upsert(&mut custom, &clone).unwrap();
        assert_eq!(id2, "custom:2");
        // Editing keeps the id.
        let mut edit = custom[0].clone();
        edit.width = 1700;
        assert_eq!(upsert(&mut custom, &edit).unwrap(), "custom:1");
        assert_eq!(custom.len(), 2);
        assert_eq!(custom[0].width, 1700);
        // Bad sizes and names are refused.
        assert!(upsert(&mut custom, &preset("", "x", 0, 10)).is_err());
        assert!(upsert(&mut custom, &preset("", " ", 10, 10)).is_err());

        // Saved as JSON (in the settings file) and read back.
        let text = serde_json::to_string(&custom).unwrap();
        let loaded: Vec<SizePreset> = serde_json::from_str(&text).unwrap();
        let merged = all(&loaded);
        let n = builtin().len();
        assert_eq!(merged.len(), n + 2);
        assert!(merged[..n].iter().all(|p| p.builtin));
        assert_eq!(merged[n].name, "Blog header");
        assert!(!merged[n].builtin);
        // A hand-edited file cannot add built-ins or broken entries.
        let tampered = vec![
            SizePreset {
                builtin: true,
                ..preset("builtin:X/Y", "Y", 10, 10)
            },
            preset("custom:9", "zero", 0, 0),
        ];
        assert_eq!(all(&tampered).len(), n);

        assert!(remove(&mut custom, "builtin:Instagram/Square").is_err());
        remove(&mut custom, "custom:1").unwrap();
        assert!(remove(&mut custom, "custom:1").is_err());
        assert_eq!(custom.len(), 1);
    }
}
