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
    /// A file in the app's own presets folder: it can be moved to the trash.
    pub deletable: bool,
}

/// Whether a path is a preset file the app may delete: a real file with a
/// preset extension inside `own`, the app's own presets folder. Presets in
/// folders the user added, or in Lightroom's, belong to other programs too.
/// `..` and symlinks are resolved first, so they can't lead out.
pub fn preset_deletable(path: &Path, own: &Path) -> bool {
    let (Ok(real), Ok(root)) = (std::fs::canonicalize(path), std::fs::canonicalize(own)) else {
        return false;
    };
    real.is_file() && is_preset_extension(&real) && real.starts_with(root)
}

fn is_preset_extension(p: &Path) -> bool {
    p.extension()
        .and_then(|x| x.to_str())
        .is_some_and(|x| x.eq_ignore_ascii_case("xmp") || x.eq_ignore_ascii_case("lrtemplate"))
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

/// A name made safe for a file or folder name.
fn safe_file_name(s: &str) -> String {
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
}

/// A fresh preset id (Lightroom keeps them unique), from `seed` and the time.
fn new_uuid(seed: &str) -> String {
    use sha2::{Digest, Sha256};
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    Sha256::digest(format!("{seed}/{stamp}"))
        .iter()
        .take(16)
        .map(|b| format!("{b:02X}"))
        .collect()
}

fn alt(s: &str) -> Value {
    Value::Alt(vec![("x-default".into(), s.to_string())])
}

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
    let uuid = new_uuid(&format!("{group}/{name}"));
    out.insert("PresetType".into(), Value::Text("Normal".into()));
    out.insert("Name".into(), alt(name));
    out.insert("Group".into(), alt(group));
    out.insert("UUID".into(), Value::Text(uuid));
    out.insert("HasSettings".into(), Value::Text("True".into()));
    out.insert("SupportsAmount".into(), Value::Text("True".into()));
    // Into a folder per group, or straight into a folder the user chose.
    let folder = if in_group_folder {
        dir.join(safe_file_name(group))
    } else {
        dir.to_path_buf()
    };
    std::fs::create_dir_all(&folder).map_err(|e| format!("{}: {e}", folder.display()))?;
    let path = folder.join(format!("{}.xmp", safe_file_name(name)));
    let text = amb_presets::xmp_writer::write_packet(&out, amb_presets::is_signed_tag);
    amb_protocol::sidecar::write_atomic(&path, &text).map_err(|e| e.to_string())?;
    Ok(path)
}

/// A setting a preset holds, for the Edit dialog.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Held {
    pub name: String,
    pub label: String,
    /// The settings group ("" when the name isn't in the registry).
    pub group: String,
}

/// Whether Apply takes `name` from a preset as a setting: not the header
/// (name, group, id) and not the process tags.
fn is_held(name: &str) -> bool {
    amb_presets::registry::Registry::builtin()
        .lookup(name)
        .is_none_or(|i| i.group != "metadata" && i.group != "process")
}

fn held(name: &str) -> Held {
    let group = amb_presets::registry::Registry::builtin()
        .lookup(name)
        .map(|i| i.group)
        .unwrap_or_default();
    let label = amb_protocol::catalog::Catalog::builtin()
        .text(name)
        .map_or_else(|| name.to_string(), |t| t.label.clone());
    Held {
        name: name.to_string(),
        label,
        group,
    }
}

/// The settings a preset holds: what Apply takes from it.
pub fn held_settings(settings: &BTreeMap<String, Value>) -> Vec<Held> {
    settings
        .keys()
        .filter(|k| is_held(k))
        .map(|k| held(k))
        .collect()
}

/// A photo's settings a preset could take in: set to something other than
/// their default, not in the preset, and not about this photo alone (crop,
/// masks, retouching, geometry).
pub fn photo_additions(
    photo: &BTreeMap<String, Value>,
    preset: &BTreeMap<String, Value>,
) -> Vec<Held> {
    let registry = amb_presets::registry::Registry::builtin();
    photo
        .iter()
        .filter(|(k, v)| {
            is_held(k)
                && !preset.contains_key(*k)
                && !is_photo_specific(k)
                && !matches!((v, registry.numeric_default(k)),
                    (Value::Number(n), Some(d)) if *n == d)
        })
        .map(|(k, _)| held(k))
        .collect()
}

fn is_photo_specific(name: &str) -> bool {
    amb_presets::registry::Registry::builtin()
        .lookup(name)
        .is_some_and(|i| PHOTO_SPECIFIC_GROUPS.contains(&i.group.as_str()))
}

/// `folder/<base>.xmp`, or `<base> 2`, `<base> 3`, … when that file exists.
/// Returns the name used and the path.
fn free_target(folder: &Path, base: &str) -> (String, PathBuf) {
    let mut n = 1;
    loop {
        let name = if n == 1 {
            base.to_string()
        } else {
            format!("{base} {n}")
        };
        let path = folder.join(format!("{}.xmp", safe_file_name(&name)));
        if !path.exists() {
            return (name, path);
        }
        n += 1;
    }
}

fn read_preset(path: &Path) -> Result<amb_presets::Preset, String> {
    amb_presets::Preset::from_path(path).map_err(|e| format!("{}: {e}", path.display()))
}

/// The group a preset is listed under: its own, else its folder's name.
fn listed_group(preset: &amb_presets::Preset, path: &Path) -> String {
    preset
        .group()
        .map(str::to_string)
        .or_else(|| {
            path.parent()
                .and_then(|d| d.file_name())
                .map(|n| n.to_string_lossy().into_owned())
        })
        .filter(|g| !g.trim().is_empty())
        .unwrap_or_else(|| "Yours".into())
}

fn write_preset(path: &Path, settings: &BTreeMap<String, Value>) -> Result<(), String> {
    if let Some(folder) = path.parent() {
        std::fs::create_dir_all(folder).map_err(|e| format!("{}: {e}", folder.display()))?;
    }
    let text = amb_presets::xmp_writer::write_packet(settings, amb_presets::is_signed_tag);
    amb_protocol::sidecar::write_atomic(path, &text).map_err(|e| e.to_string())
}

/// A copy of any preset in the app's own folder, "NAME copy" in the same
/// group, with a new id. The settings are read as Apply reads them and
/// written whole (masks, profile and `amb:` effects too, nothing filtered),
/// so applying the copy gives what applying the original gives. Both
/// formats (.xmp, .lrtemplate) come out as an .xmp.
pub fn duplicate_preset(src: &Path, own: &Path) -> Result<PathBuf, String> {
    let preset = read_preset(src)?;
    let stem = src
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let name = preset.name().map(str::to_string).unwrap_or(stem);
    let group = listed_group(&preset, src);
    let (copy, path) = free_target(
        &own.join(safe_file_name(&group)),
        &format!("{} copy", name.trim()),
    );
    let mut settings = preset.settings;
    settings.insert("Name".into(), alt(&copy));
    settings.insert("Group".into(), alt(&group));
    settings.insert(
        "UUID".into(),
        Value::Text(new_uuid(&format!("{group}/{copy}"))),
    );
    write_preset(&path, &settings)?;
    Ok(path)
}

/// Rewrite one of the app's own presets: a new `name` and `group` (the file
/// moves, "NAME 2" when the name is taken) and only the settings in `keep`.
/// Their values are the preset's own, or the photo's when `photo` is given
/// (the preset's own for one the photo lacks). A name in `keep` that neither
/// has is ignored. Returns the path afterwards.
pub fn update_preset(
    path: &Path,
    own: &Path,
    name: &str,
    group: &str,
    keep: &[String],
    photo: Option<&BTreeMap<String, Value>>,
) -> Result<PathBuf, String> {
    if !preset_deletable(path, own) {
        return Err(
            "Only presets saved in Ambrotype can be edited here. Duplicate it first.".into(),
        );
    }
    let name = name.trim();
    let group = match group.trim() {
        "" => "Yours",
        g => g,
    };
    if name.is_empty() {
        return Err("give the preset a name".into());
    }
    let old = read_preset(path)?;
    let (old_name, old_group) = (
        old.name().unwrap_or_default().to_string(),
        listed_group(&old, path),
    );
    let mut out: BTreeMap<String, Value> = old
        .settings
        .iter()
        .filter(|(k, _)| !is_held(k))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    for k in keep.iter().filter(|k| is_held(k)) {
        let from_photo = photo
            .and_then(|p| p.get(k))
            // The photo's own crop and masks never join a preset that lacks them.
            .filter(|_| old.settings.contains_key(k) || !is_photo_specific(k));
        if let Some(v) = from_photo.or_else(|| old.settings.get(k)) {
            out.insert(k.clone(), v.clone());
        }
    }
    if !out.keys().any(|k| is_held(k)) {
        return Err("a preset needs at least one setting".into());
    }
    let in_place = name == old_name
        && group == old_group
        && path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("xmp"));
    let (name, target) = if in_place {
        (name.to_string(), path.to_path_buf())
    } else {
        free_target(&own.join(safe_file_name(group)), name)
    };
    out.insert("Name".into(), alt(&name));
    out.insert("Group".into(), alt(group));
    write_preset(&target, &out)?;
    if !in_place {
        std::fs::remove_file(path).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(target)
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
/// `own` is the app's presets folder: only files in it are `deletable`.
pub fn scan_presets(dirs: &[PathBuf], own: Option<&Path>) -> Vec<PresetEntry> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>, depth: usize) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() && depth < 6 {
                walk(&p, out, depth + 1);
            } else if is_preset_extension(&p) && !is_photo_sidecar(&p) {
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
                deletable: own.is_some_and(|o| preset_deletable(p, o)),
            })
        })
        .collect();
    presets.sort_by(|a, b| {
        (a.group.to_lowercase(), a.name.to_lowercase())
            .cmp(&(b.group.to_lowercase(), b.name.to_lowercase()))
    });
    presets
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("amb-preset-del-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn deletable_only_inside_the_own_folder() {
        let base = tmp("roots");
        let own = base.join("own");
        let added = base.join("added");
        std::fs::create_dir_all(own.join("Group")).unwrap();
        std::fs::create_dir_all(&added).unwrap();
        let inside = own.join("Group/a.xmp");
        let in_added = added.join("b.xmp");
        let text = own.join("Group/notes.txt");
        for f in [&inside, &in_added, &text] {
            std::fs::write(f, "x").unwrap();
        }
        assert!(preset_deletable(&inside, &own), "a file in the own folder");
        assert!(
            !preset_deletable(&in_added, &own),
            "a file in an added folder"
        );
        let tricky = own.join("Group/../../added/b.xmp");
        assert!(!preset_deletable(&tricky, &own), ".. leads out");
        assert!(!preset_deletable(&text, &own), "not a preset extension");
        assert!(!preset_deletable(&own.join("Group"), &own), "a folder");
        assert!(!preset_deletable(&own.join("gone.xmp"), &own), "missing");
        assert!(
            !preset_deletable(&inside, &base.join("nope")),
            "no own folder"
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    #[cfg(unix)]
    #[test]
    fn a_link_out_of_the_folder_is_refused() {
        let base = tmp("link");
        let own = base.join("own");
        std::fs::create_dir_all(&own).unwrap();
        let target = base.join("elsewhere.xmp");
        std::fs::write(&target, "x").unwrap();
        let link = own.join("l.xmp");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(!preset_deletable(&link, &own));
        let _ = std::fs::remove_dir_all(&base);
    }

    fn num(n: f64) -> Value {
        Value::Number(n)
    }

    /// A preset in `dir` with a few settings, a mask-like list and an `amb:` one.
    fn sample_xmp(dir: &Path, name: &str) -> PathBuf {
        let mut s: BTreeMap<String, Value> = BTreeMap::new();
        s.insert("Exposure2012".into(), num(0.5));
        s.insert("Contrast2012".into(), num(-12.0));
        s.insert("Vibrance".into(), num(20.0));
        s.insert("amb:WBShiftBlue".into(), num(-4.0));
        s.insert(
            "ToneCurvePV2012".into(),
            Value::Seq(vec![num(0.0), num(30.0)]),
        );
        s.insert("PresetType".into(), Value::Text("Normal".into()));
        s.insert("Name".into(), alt(name));
        s.insert("Group".into(), alt("Pack"));
        s.insert("UUID".into(), Value::Text("AAAA".into()));
        std::fs::create_dir_all(dir).unwrap();
        let path = dir.join(format!("{name}.xmp"));
        write_preset(&path, &s).unwrap();
        path
    }

    /// What Apply reads from a preset, as text, header left out.
    fn applied(path: &Path) -> String {
        let p = amb_presets::Preset::from_path(path).unwrap();
        let s: BTreeMap<_, _> = p.settings.into_iter().filter(|(k, _)| is_held(k)).collect();
        amb_presets::xmp_writer::write_packet(&s, amb_presets::is_signed_tag)
    }

    fn names(path: &Path) -> Vec<String> {
        let p = amb_presets::Preset::from_path(path).unwrap();
        held_settings(&p.settings)
            .into_iter()
            .map(|h| h.name)
            .collect()
    }

    #[test]
    fn a_copy_applies_as_the_original_does() {
        let base = tmp("dup");
        let own = base.join("own");
        let src = sample_xmp(&base.join("pack"), "Warm");
        let copy = duplicate_preset(&src, &own).unwrap();
        assert!(copy.starts_with(own.join("Pack")), "same group: {copy:?}");
        assert_eq!(applied(&copy), applied(&src));
        let c = amb_presets::Preset::from_path(&copy).unwrap();
        assert_eq!(c.name(), Some("Warm copy"));
        assert_eq!(c.group(), Some("Pack"));
        assert_ne!(c.uuid(), Some("AAAA"), "a new id");
        assert!(preset_deletable(&copy, &own));
        // "copy 2", "copy 3" when taken.
        let c2 = duplicate_preset(&src, &own).unwrap();
        let c3 = duplicate_preset(&src, &own).unwrap();
        let n = |p: &Path| {
            amb_presets::Preset::from_path(p)
                .unwrap()
                .name()
                .map(str::to_string)
        };
        assert_eq!(n(&c2).as_deref(), Some("Warm copy 2"));
        assert_eq!(n(&c3).as_deref(), Some("Warm copy 3"));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn an_old_lrtemplate_copies_to_the_same_settings() {
        let base = tmp("lrt");
        let own = base.join("own");
        std::fs::create_dir_all(&base).unwrap();
        let src = base.join("Matte.lrtemplate");
        std::fs::write(
            &src,
            "s = {\n id = \"3E6E\",\n title = \"Matte\",\n value = {\n settings = {\n \
             Exposure2012 = 0.35,\n Contrast2012 = -12,\n ToneCurvePV2012 = { 0, 18, 255, 245, },\n },\n \
             uuid = \"3E6E\",\n },\n}",
        )
        .unwrap();
        let copy = duplicate_preset(&src, &own).unwrap();
        assert_eq!(copy.extension().and_then(|e| e.to_str()), Some("xmp"));
        let (a, b) = (
            amb_presets::Preset::from_path(&src).unwrap(),
            amb_presets::Preset::from_path(&copy).unwrap(),
        );
        assert_eq!(b.name(), Some("Matte copy"));
        for k in ["Exposure2012", "Contrast2012", "ToneCurvePV2012"] {
            let text = |p: &amb_presets::Preset| {
                let m: BTreeMap<_, _> = [(k.to_string(), p.settings[k].clone())].into();
                amb_presets::xmp_writer::write_packet(&m, amb_presets::is_signed_tag)
            };
            assert_eq!(text(&a), text(&b), "{k}");
        }
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn update_drops_what_is_not_kept() {
        let base = tmp("upd");
        let own = base.join("own");
        let src = duplicate_preset(&sample_xmp(&base.join("pack"), "Warm"), &own).unwrap();
        let keep: Vec<String> = ["Exposure2012", "Vibrance", "amb:WBShiftBlue", "Nonsense"]
            .map(String::from)
            .into();
        let out = update_preset(&src, &own, "Warm copy", "Pack", &keep, None).unwrap();
        assert_eq!(out, src, "same name and group: the same file");
        let mut got = names(&out);
        got.sort();
        assert_eq!(got, ["Exposure2012", "Vibrance", "amb:WBShiftBlue"]);
        let p = amb_presets::Preset::from_path(&out).unwrap();
        assert!(p.get("Contrast2012").is_none() && p.get("ToneCurvePV2012").is_none());
        assert_eq!(p.name(), Some("Warm copy"));
        let none = update_preset(&out, &own, "x", "", &[], None);
        assert!(none.unwrap_err().contains("at least one"));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn update_can_take_the_photos_values_and_add_its_settings() {
        let base = tmp("photo");
        let own = base.join("own");
        let src = duplicate_preset(&sample_xmp(&base.join("pack"), "Warm"), &own).unwrap();
        let mut photo: BTreeMap<String, Value> = BTreeMap::new();
        photo.insert("Exposure2012".into(), num(1.5));
        photo.insert("Saturation".into(), num(10.0));
        photo.insert("Clarity2012".into(), num(0.0));
        photo.insert("CropTop".into(), num(0.1));
        let p = amb_presets::Preset::from_path(&src).unwrap();
        let extra: Vec<String> = photo_additions(&photo, &p.settings)
            .into_iter()
            .map(|h| h.name)
            .collect();
        assert_eq!(
            extra,
            ["Saturation"],
            "not a default, not the crop, not in the preset"
        );
        let keep: Vec<String> = ["Exposure2012", "Contrast2012", "Saturation", "CropTop"]
            .map(String::from)
            .into();
        let out = update_preset(&src, &own, "Warm copy", "Pack", &keep, Some(&photo)).unwrap();
        let q = amb_presets::Preset::from_path(&out).unwrap();
        assert_eq!(
            q.get("Exposure2012").and_then(Value::as_f64),
            Some(1.5),
            "from the photo"
        );
        assert_eq!(
            q.get("Saturation").and_then(Value::as_f64),
            Some(10.0),
            "added"
        );
        assert_eq!(
            q.get("Contrast2012").and_then(Value::as_f64),
            Some(-12.0),
            "the preset's, the photo lacks it"
        );
        assert!(q.get("CropTop").is_none(), "a crop never joins");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn rename_moves_the_file_and_never_overwrites() {
        let base = tmp("ren");
        let own = base.join("own");
        let pack = base.join("pack");
        let a = duplicate_preset(&sample_xmp(&pack, "Warm"), &own).unwrap();
        let b = duplicate_preset(&sample_xmp(&pack, "Cool"), &own).unwrap();
        let keep: Vec<String> = vec!["Exposure2012".into()];
        let moved = update_preset(&a, &own, "Mine", "Looks", &keep, None).unwrap();
        assert!(moved.starts_with(own.join("Looks")) && moved.is_file() && !a.exists());
        let taken = update_preset(&b, &own, "Mine", "Looks", &keep, None).unwrap();
        assert_ne!(taken, moved, "the first one is untouched");
        let t = amb_presets::Preset::from_path(&taken).unwrap();
        assert_eq!(t.name(), Some("Mine 2"));
        assert!(moved.is_file() && taken.is_file() && !b.exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn update_refuses_a_preset_outside_the_own_folder() {
        let base = tmp("out");
        let own = base.join("own");
        std::fs::create_dir_all(&own).unwrap();
        let src = sample_xmp(&base.join("pack"), "Warm");
        let keep: Vec<String> = vec!["Exposure2012".into()];
        let err = update_preset(&src, &own, "X", "Y", &keep, None).unwrap_err();
        assert!(err.contains("saved in Ambrotype"), "{err}");
        assert!(
            src.is_file() && amb_presets::Preset::from_path(&src).unwrap().name() == Some("Warm")
        );
        let _ = std::fs::remove_dir_all(&base);
    }
}
