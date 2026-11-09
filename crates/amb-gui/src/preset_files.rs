// SPDX-License-Identifier: GPL-3.0-or-later
//! Presets on disk: the list of saved presets, and saving one.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use amb_presets::Value;
use rayon::prelude::*;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct PresetEntry {
    pub name: String,
    pub group: String,
    pub path: String,
}

/// Settings groups a saved preset leaves out: they belong to one photo.
const PHOTO_SPECIFIC_GROUPS: [&str; 6] = [
    "crop",
    "local",
    "retouch",
    "transform",
    "metadata",
    "lens_blur",
];

/// Save `settings` as a Lightroom-style XMP preset `dir/<group>/<name>.xmp`
/// (crop, masks, retouching and geometry left out). Returns its path.
pub fn save_preset(
    dir: &Path,
    name: &str,
    group: &str,
    settings: &BTreeMap<String, Value>,
    in_group_folder: bool,
) -> Result<PathBuf, String> {
    let name = name.trim();
    let group = if group.trim().is_empty() {
        "Yours"
    } else {
        group.trim()
    };
    if name.is_empty() {
        return Err("give the preset a name".into());
    }
    let safe = |s: &str| -> String {
        s.chars()
            .map(|c| {
                if c.is_alphanumeric() || " -_".contains(c) {
                    c
                } else {
                    '-'
                }
            })
            .collect::<String>()
            .trim()
            .to_string()
    };
    let catalog = amb_protocol::catalog::Catalog::builtin();
    let mut out: BTreeMap<String, Value> = settings
        .iter()
        .filter(|(k, _)| {
            catalog
                .lookup(k)
                .is_none_or(|t| !PHOTO_SPECIFIC_GROUPS.contains(&t.group.as_str()))
        })
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    if out.is_empty() {
        return Err("this photo has no edits to save".into());
    }
    let uuid: String = {
        use sha2::{Digest, Sha256};
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        Sha256::digest(format!("{group}/{name}/{stamp}"))
            .iter()
            .take(16)
            .map(|b| format!("{b:02X}"))
            .collect()
    };
    let alt = |s: &str| Value::Alt(vec![("x-default".into(), s.to_string())]);
    out.insert("PresetType".into(), Value::Text("Normal".into()));
    out.insert("Name".into(), alt(name));
    out.insert("Group".into(), alt(group));
    out.insert("UUID".into(), Value::Text(uuid));
    out.insert("HasSettings".into(), Value::Text("True".into()));
    out.insert("SupportsAmount".into(), Value::Text("True".into()));
    // Into a folder per group, or straight into a folder the user chose.
    let folder = if in_group_folder {
        dir.join(safe(group))
    } else {
        dir.to_path_buf()
    };
    std::fs::create_dir_all(&folder).map_err(|e| format!("{}: {e}", folder.display()))?;
    let path = folder.join(format!("{}.xmp", safe(name)));
    let text = amb_presets::xmp_writer::write_packet(&out, amb_presets::is_signed_tag);
    amb_protocol::sidecar::write_atomic(&path, &text).map_err(|e| e.to_string())?;
    Ok(path)
}

/// Lightroom/Camera Raw's own preset folders, when present (read in place).
pub fn default_preset_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(appdata) = std::env::var("APPDATA") {
        dirs.push(PathBuf::from(appdata).join("Adobe/CameraRaw/Settings"));
    }
    if let Ok(home) = std::env::var("HOME") {
        dirs.push(
            PathBuf::from(&home).join("Library/Application Support/Adobe/CameraRaw/Settings"),
        );
        dirs.push(PathBuf::from(&home).join(".local/share/ambrotype/presets"));
    }
    dirs.into_iter().filter(|d| d.is_dir()).collect()
}

/// All presets (.xmp, .lrtemplate) under `dirs`, sorted by group and name.
pub fn scan_presets(dirs: &[PathBuf]) -> Vec<PresetEntry> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>, depth: usize) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() && depth < 6 {
                walk(&p, out, depth + 1);
            } else if p.extension().and_then(|x| x.to_str()).is_some_and(|x| {
                x.eq_ignore_ascii_case("xmp") || x.eq_ignore_ascii_case("lrtemplate")
            }) && !is_photo_sidecar(&p)
            {
                out.push(p);
            }
        }
    }
    // A photo's own sidecar (IMG_1234.xmp next to IMG_1234.NEF,
    // photo.jpg.xmp next to photo.jpg, or a virtual copy's), not a preset:
    // a folder of presets can hold photos.
    fn is_photo_sidecar(p: &Path) -> bool {
        if amb_engine::virtual_copy_of(p).is_some() {
            return true;
        }
        // A JPEG's (or other rendered photo's) sidecar is its whole name plus .xmp.
        if p.file_stem().is_some_and(|s| p.with_file_name(s).is_file()) {
            return true;
        }
        let (Some(dir), Some(stem)) = (p.parent(), p.file_stem()) else {
            return false;
        };
        std::fs::read_dir(dir).is_ok_and(|entries| {
            entries.flatten().any(|e| {
                let q = e.path();
                q != p
                    && q.file_stem() == Some(stem)
                    && q.extension().and_then(|x| x.to_str()).is_some_and(|x| {
                        !x.eq_ignore_ascii_case("xmp") && !x.eq_ignore_ascii_case("lrtemplate")
                    })
            })
        })
    }
    let mut files = Vec::new();
    for d in dirs {
        walk(d, &mut files, 0);
    }
    let mut presets: Vec<PresetEntry> = files
        .par_iter()
        .filter_map(|p| {
            let preset = amb_presets::Preset::from_path(p).ok()?;
            let stem = p.file_stem()?.to_string_lossy().into_owned();
            let parent = p
                .parent()
                .and_then(|d| d.file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            Some(PresetEntry {
                name: preset.name().map(str::to_string).unwrap_or(stem),
                group: preset.group().map(str::to_string).unwrap_or(parent),
                path: p.display().to_string(),
            })
        })
        .collect();
    presets.sort_by(|a, b| {
        (a.group.to_lowercase(), a.name.to_lowercase())
            .cmp(&(b.group.to_lowercase(), b.name.to_lowercase()))
    });
    presets
}
