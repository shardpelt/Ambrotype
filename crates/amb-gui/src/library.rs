// SPDX-License-Identifier: GPL-3.0-or-later
//! The library: a folder of raws as a grid, to sort, filter, rate, label and
//! keyword many photos at once, paste one photo's edit onto others, and
//! export a selection.
//!
//! Everything here reads and writes the photos' XMP sidecars, the same files
//! the editor and Lightroom use; the catalogue (`folder::catalog`) only
//! remembers what was read from them, until they change. Edits go through
//! `amb-protocol` commands like every other edit.

use crate::folder::{self, FolderPhoto};
use amb_protocol::sidecar::{self, PhotoInfo};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// A photo in the grid: its folder entry and the shot (when it was taken,
/// camera, lens, exposure) from the raw's header.
#[derive(Debug, Clone, Serialize)]
pub struct LibraryPhoto {
    #[serde(flatten)]
    pub photo: FolderPhoto,
    #[serde(flatten)]
    pub shot: amb_library::catalog::Shot,
    /// On a drive that isn't plugged in: listed from the catalogue.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub offline: bool,
}

/// The raws in `dir` and every subfolder, in name order, with their capture
/// times (read from the raw headers in parallel; no image is decoded).
pub fn list_tree(dir: &Path) -> Result<Vec<LibraryPhoto>, String> {
    Ok(with_times(folder::list_tree(dir)?))
}

/// The photos of several folders in one grid, the first folder's first, each
/// listed once. With more than one folder, names start with their folder's
/// name (`2026-07/IMG_1.nef`) so photos with the same name can be told apart.
pub fn list_many(dirs: &[PathBuf], subfolders: bool) -> Result<Vec<LibraryPhoto>, String> {
    let mut seen = std::collections::HashSet::new();
    let mut all = Vec::new();
    for dir in dirs {
        let photos = if subfolders {
            folder::list_tree(dir)?
        } else {
            folder::list_dir(dir)?
        };
        let prefix = dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .filter(|_| dirs.len() > 1);
        all.extend(
            photos
                .into_iter()
                .filter(|p| seen.insert(p.path.clone()))
                .map(|mut p| {
                    if let Some(f) = &prefix {
                        p.name = format!("{f}/{}", p.name);
                    }
                    p
                }),
        );
    }
    Ok(with_times(all))
}

/// The photos under a smart collection's folders that meet its rules.
pub fn smart_photos(s: &amb_library::smart::SmartCollection) -> Vec<LibraryPhoto> {
    let mut seen = std::collections::HashSet::new();
    s.folders
        .iter()
        .filter_map(|f| list_tree(f).ok())
        .flatten()
        .filter(|p| seen.insert(p.photo.path.clone()))
        .filter(|p| {
            let facts = amb_library::smart::Facts {
                name: &p.photo.name,
                rating: p.photo.rating,
                label: p.photo.label.as_deref(),
                flag: p.photo.flag.map(|f| f.name()),
                keywords: &p.photo.keywords,
                edited: p.photo.edited,
                captured: p.shot.captured.as_deref(),
                camera: p.shot.camera.as_deref(),
                lens: p.shot.lens.as_deref(),
                iso: p.shot.iso,
                focal: p.shot.focal,
            };
            s.matches(&facts)
        })
        .collect()
}

/// Every photo the catalogue knows (every folder shown in the library so
/// far), newest first, each named with its folder (`2026-07/IMG_1.nef`).
pub fn all_photos() -> Vec<LibraryPhoto> {
    folder::catalog()
        .all()
        .into_iter()
        .map(|e| {
            let (mut photo, shot) = folder::from_record(&e.path, e.record);
            let parent = amb_engine::source_path(&e.path);
            if let Some(f) = parent.parent().and_then(|d| d.file_name()) {
                photo.name = format!("{}/{}", f.to_string_lossy(), photo.name);
            }
            LibraryPhoto {
                photo,
                shot,
                offline: e.offline,
            }
        })
        .collect()
}

/// The photos of a collection, in its order, and the paths no longer there.
pub fn list_paths(paths: &[PathBuf]) -> (Vec<LibraryPhoto>, Vec<String>) {
    let (here, gone): (Vec<PathBuf>, Vec<PathBuf>) =
        paths.iter().cloned().partition(|p| p.is_file());
    (
        with_times(folder::describe(here)),
        gone.iter().map(|p| p.display().to_string()).collect(),
    )
}

/// The shots from the raw headers: from the catalogue, or read in parallel.
fn with_times(photos: Vec<FolderPhoto>) -> Vec<LibraryPhoto> {
    let paths: Vec<PathBuf> = photos.iter().map(|p| PathBuf::from(&p.path)).collect();
    let records = folder::catalog().records(&paths);
    photos
        .into_iter()
        .zip(records)
        .map(|(photo, r)| LibraryPhoto {
            photo,
            shot: r.shot,
            offline: false,
        })
        .collect()
}

/// A change to many photos' rating, label and keywords. Fields left out
/// stay as they are.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct InfoChange {
    /// 1–5, or 0 to clear.
    #[serde(default)]
    pub rating: Option<u8>,
    /// A colour label, or "" to clear.
    #[serde(default)]
    pub label: Option<String>,
    /// "pick", "reject", or "" to clear.
    #[serde(default)]
    pub flag: Option<String>,
    #[serde(default)]
    pub add_keywords: Vec<String>,
    #[serde(default)]
    pub remove_keywords: Vec<String>,
    /// Title, caption, creator and copyright: the text, or "" to clear.
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub caption: Option<String>,
    #[serde(default)]
    pub creator: Option<String>,
    #[serde(default)]
    pub copyright: Option<String>,
    /// Put in a stack, or out of one (an empty id).
    #[serde(default)]
    pub stack: Option<sidecar::StackRef>,
}

impl InfoChange {
    pub fn apply(&self, info: &PhotoInfo) -> PhotoInfo {
        let mut out = info.clone();
        if let Some(r) = self.rating {
            out.rating = (1..=5).contains(&r).then_some(r);
        }
        if let Some(l) = &self.label {
            out.label = Some(l.clone()).filter(|l| !l.trim().is_empty());
        }
        if let Some(f) = &self.flag {
            out.flag = sidecar::Flag::parse(f);
        }
        for (field, change) in [
            (&mut out.title, &self.title),
            (&mut out.caption, &self.caption),
            (&mut out.creator, &self.creator),
            (&mut out.copyright, &self.copyright),
        ] {
            if let Some(v) = change {
                *field = Some(v.clone());
            }
        }
        if let Some(s) = &self.stack {
            out.stack = Some(s.clone());
        }
        let remove: Vec<String> = self
            .remove_keywords
            .iter()
            .map(|k| k.trim().to_lowercase())
            .collect();
        out.keywords
            .retain(|k| !remove.contains(&k.trim().to_lowercase()));
        for k in &self.add_keywords {
            if !out
                .keywords
                .iter()
                .any(|x| x.eq_ignore_ascii_case(k.trim()))
            {
                out.keywords.push(k.trim().to_string());
            }
        }
        out.cleaned()
    }
}

/// Write a change to one photo's sidecar (not the open photo: the session
/// owns that one). Returns the new info.
pub fn change_info(photo: &Path, change: &InfoChange) -> Result<PhotoInfo, String> {
    let path = crate::session::sidecar_path(photo);
    let on_disk = std::fs::read_to_string(&path).ok();
    let info = change.apply(
        &on_disk
            .as_deref()
            .map(sidecar::read_info)
            .unwrap_or_default(),
    );
    let text = sidecar::with_info(on_disk.as_deref(), &info).map_err(|e| e.to_string())?;
    sidecar::write_atomic(&path, &text).map_err(|e| e.to_string())?;
    Ok(sidecar::read_info(&text))
}

/// Make a virtual copy of `photo` with `settings` (its edit as it is now)
/// and its keywords, but not its rating, label or flag, as in Lightroom.
/// Returns the copy's path (its sidecar).
pub fn create_virtual_copy(
    photo: &Path,
    settings: &std::collections::BTreeMap<String, amb_presets::Value>,
    keywords: Vec<String>,
) -> Result<PathBuf, String> {
    let copy = amb_engine::new_virtual_copy_path(photo);
    let info = PhotoInfo {
        keywords,
        ..PhotoInfo::default()
    };
    let text = sidecar::merge_with_info("", settings, Some(&info)).map_err(|e| e.to_string())?;
    sidecar::write_atomic(&copy, &text).map_err(|e| e.to_string())?;
    Ok(copy)
}

/// Delete a virtual copy (its sidecar); never a photo or its own sidecar.
pub fn delete_virtual_copy(copy: &Path) -> Result<(), String> {
    if amb_engine::virtual_copy_of(copy).is_none() {
        return Err(format!("{} is not a virtual copy", copy.display()));
    }
    std::fs::remove_file(copy).map_err(|e| format!("{}: {e}", copy.display()))
}

/// The groups of settings "Paste settings" offers (not metadata).
#[derive(Debug, Clone, Serialize)]
pub struct Group {
    pub name: String,
    pub label: String,
    /// Ticked unless the user changes it: everything but crop and geometry,
    /// which rarely carry over from one photo to another.
    pub default: bool,
}

pub fn paste_groups() -> Vec<Group> {
    amb_protocol::catalog::Catalog::builtin()
        .groups()
        .iter()
        .filter(|g| !["metadata", "process"].contains(&g.name.as_str()))
        .map(|g| Group {
            name: g.name.clone(),
            label: g.label.clone(),
            default: !["crop", "transform", "local", "retouch"].contains(&g.name.as_str()),
        })
        .collect()
}

/// The commands that give a photo `source`'s settings in `groups`: those
/// groups reset, then the source applied as a preset limited to them (so a
/// setting the source leaves at its default is reset on the target too).
pub fn paste_commands(source: &Path, groups: &[String]) -> Vec<amb_protocol::Command> {
    use amb_protocol::Command;
    let mut cmds: Vec<Command> = groups
        .iter()
        .map(|g| Command::ResetGroup { group: g.clone() })
        .collect();
    if source.is_file() {
        cmds.push(Command::ApplyPreset {
            path: source.display().to_string(),
            amount: 100.0,
            include_groups: Some(groups.to_vec()),
            exclude_groups: None,
        });
    }
    cmds
}

/// Paste onto a photo that isn't open: through the protocol, into its sidecar.
pub fn paste_to_file(photo: &Path, commands: &[amb_protocol::Command]) -> Result<(), String> {
    let text = serde_json::to_string(commands).map_err(|e| e.to_string())?;
    amb_protocol::edit::run(&amb_protocol::edit::EditRequest {
        sidecar: &crate::session::sidecar_path(photo),
        commands: &text,
        out: None,
        dry_run: false,
    })
    .map(|_| ())
    .map_err(|e| format!("{}: {e}", photo.display()))
}

/// `dir/<name>.<ext>`, or `<name>-2.<ext>`, ... when that's taken (by an
/// earlier file or one written in this batch).
pub fn unique_path(dir: &Path, stem: &str, ext: &str, taken: &mut Vec<PathBuf>) -> PathBuf {
    let mut n = 1;
    loop {
        let name = if n == 1 {
            format!("{stem}.{ext}")
        } else {
            format!("{stem}-{n}.{ext}")
        };
        let p = dir.join(name);
        if !p.exists() && !taken.contains(&p) {
            taken.push(p.clone());
            return p;
        }
        n += 1;
    }
}

/// What a batch export did.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ExportedMany {
    pub written: Vec<String>,
    pub errors: Vec<String>,
}

/// Export each photo with its own edit, rating, label and keywords into
/// `dir`, with the same export `settings`. One failure doesn't stop the
/// rest. `progress(done, total, name)` is called before each photo.
pub fn export_many(
    paths: &[String],
    dir: &Path,
    name_template: Option<&str>,
    settings: &amb_export::ExportSettings,
    include_tags: bool,
    lens: &crate::LensSources,
    progress: &mut dyn FnMut(usize, usize, &str),
) -> ExportedMany {
    let mut taken = Vec::new();
    let mut out = ExportedMany::default();
    for (i, path) in paths.iter().enumerate() {
        let p = Path::new(path);
        let name = p
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        progress(i, paths.len(), &name);
        let own = || {
            p.file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        };
        let stem = match name_template {
            Some(t) => amb_library::import::file_name(t, p, i as u32 + 1).unwrap_or_else(|_| own()),
            None => own(),
        };
        let dest = unique_path(dir, &stem, settings.format.extension(), &mut taken);
        let result = crate::session::Session::open(p).and_then(|mut s| {
            let mut settings = settings.clone();
            if include_tags {
                settings.tags = amb_export::exif::Tags {
                    rating: s.info.rating,
                    keywords: s.info.keywords.clone(),
                    label: s.info.label.clone(),
                    title: s.info.title.clone(),
                    caption: s.info.caption.clone(),
                    creator: s.info.creator.clone(),
                    copyright: s.info.copyright.clone(),
                };
            }
            s.export(&dest, &settings, lens)
        });
        match result {
            Ok(_) => out.written.push(dest.display().to_string()),
            Err(e) => out.errors.push(format!("{name}: {e}")),
        }
    }
    progress(paths.len(), paths.len(), "");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("amb-library-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn dng(dir: &Path, name: &str) -> PathBuf {
        let p = dir.join(name);
        amb_testkit::dng::write(
            &p,
            &amb_testkit::patterns::generate("ramp").unwrap(),
            &Default::default(),
        )
        .unwrap();
        p
    }

    #[test]
    fn lists_several_folders_once() {
        let root = temp("many");
        let (one, two) = (root.join("one"), root.join("two"));
        std::fs::create_dir_all(&one).unwrap();
        std::fs::create_dir_all(&two).unwrap();
        dng(&one, "a.dng");
        dng(&two, "a.dng");
        dng(&two, "b.dng");
        let names = |dirs: &[PathBuf]| -> Vec<String> {
            list_many(dirs, false)
                .unwrap()
                .into_iter()
                .map(|p| p.photo.name)
                .collect()
        };
        assert_eq!(names(std::slice::from_ref(&one)), ["a.dng"]);
        assert_eq!(
            names(&[two.clone(), one.clone(), two.clone()]),
            ["two/a.dng", "two/b.dng", "one/a.dng"]
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn changes_many_photos_info() {
        let dir = temp("info");
        let (a, b) = (dng(&dir, "a.dng"), dng(&dir, "b.dng"));
        let tag = |add: &[&str], remove: &[&str]| InfoChange {
            add_keywords: add.iter().map(|s| s.to_string()).collect(),
            remove_keywords: remove.iter().map(|s| s.to_string()).collect(),
            ..InfoChange::default()
        };
        for p in [&a, &b] {
            change_info(
                p,
                &InfoChange {
                    rating: Some(4),
                    label: Some("Green".into()),
                    ..tag(&["beach", "sun"], &[])
                },
            )
            .unwrap();
        }
        let info = change_info(&b, &tag(&["Beach", "family"], &["sun"])).unwrap();
        assert_eq!(info.keywords, ["beach", "family"]);
        assert_eq!(
            (info.rating, info.label.as_deref()),
            (Some(4), Some("Green"))
        );
        // 0 and "" clear.
        let info = change_info(
            &a,
            &InfoChange {
                rating: Some(0),
                label: Some(String::new()),
                ..InfoChange::default()
            },
        )
        .unwrap();
        assert_eq!((info.rating, info.label), (None, None));
        let listed = list_many(std::slice::from_ref(&dir), false).unwrap();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[1].photo.keywords, ["beach", "family"]);
        assert_eq!(listed[1].photo.label.as_deref(), Some("Green"));
        // Flags: set, listed, cleared; the stars stay.
        let flag = |f: &str| InfoChange {
            flag: Some(f.into()),
            ..InfoChange::default()
        };
        let info = change_info(&b, &flag("reject")).unwrap();
        assert_eq!(
            (info.flag, info.rating),
            (Some(sidecar::Flag::Reject), Some(4))
        );
        assert_eq!(
            list_many(std::slice::from_ref(&dir), false).unwrap()[1]
                .photo
                .flag,
            Some(sidecar::Flag::Reject)
        );
        assert_eq!(change_info(&b, &flag("")).unwrap().flag, None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn virtual_copies_are_listed_after_their_photo() {
        let dir = temp("copies");
        let (a, b) = (dng(&dir, "a.dng"), dng(&dir, "b.dng"));
        change_info(
            &a,
            &InfoChange {
                rating: Some(5),
                add_keywords: vec!["sea".into()],
                ..InfoChange::default()
            },
        )
        .unwrap();
        let mut settings = std::collections::BTreeMap::new();
        settings.insert("Exposure2012".to_string(), amb_presets::Value::Number(1.0));
        let copy = create_virtual_copy(&a, &settings, vec!["sea".into()]).unwrap();
        assert!(copy.ends_with("a.copy-2.xmp"));
        let listed = list_many(std::slice::from_ref(&dir), false).unwrap();
        let names: Vec<&str> = listed.iter().map(|p| p.photo.name.as_str()).collect();
        assert_eq!(names, ["a.dng", "a.dng · Copy 2", "b.dng"]);
        let c = &listed[1].photo;
        assert_eq!((c.copy, c.rating, c.edited), (Some(2), None, true));
        assert_eq!(c.keywords, ["sea"]);
        // It opens as its photo with its own edit.
        let s = crate::session::Session::open(&copy).unwrap();
        assert_eq!(
            s.state.settings().get("Exposure2012"),
            Some(&amb_presets::Value::Number(1.0))
        );
        assert!(delete_virtual_copy(&b).is_err());
        delete_virtual_copy(&copy).unwrap();
        assert_eq!(
            list_many(std::slice::from_ref(&dir), false).unwrap().len(),
            2
        );
        assert!(a.is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pastes_settings_by_group() {
        let dir = temp("paste");
        let (src, dst) = (dng(&dir, "src.dng"), dng(&dir, "dst.dng"));
        let src_xmp = crate::session::sidecar_path(&src);
        let set = |p: &Path, json: &str| {
            paste_to_file(p, &amb_protocol::parse_commands(json).unwrap()).unwrap()
        };
        set(
            &src,
            r#"[{"cmd":"set_param","name":"Exposure2012","value":1.5},{"cmd":"set_param","name":"Clarity2012","value":20},{"cmd":"set_param","name":"Vibrance","value":30}]"#,
        );
        // The target has its own Contrast (tone, pasted: back to default) and Vibrance.
        set(
            &dst,
            r#"[{"cmd":"set_param","name":"Contrast2012","value":40},{"cmd":"set_param","name":"Vibrance","value":-10}]"#,
        );
        let groups = vec!["tone".to_string()];
        paste_to_file(&dst, &paste_commands(&src_xmp, &groups)).unwrap();
        let state = amb_protocol::DevelopState::load(&crate::session::sidecar_path(&dst)).unwrap();
        let get = |n: &str| state.settings().get(n).and_then(|v| v.as_f64());
        assert_eq!(get("Exposure2012"), Some(1.5));
        assert_eq!(get("Contrast2012"), None, "tone was reset to the source's");
        assert_eq!(get("Vibrance"), Some(-10.0), "presence wasn't pasted");
        assert!(
            paste_groups()
                .iter()
                .any(|g| g.name == "crop" && !g.default)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unique_names() {
        let dir = temp("names");
        std::fs::write(dir.join("a.jpg"), "x").unwrap();
        let mut taken = Vec::new();
        assert_eq!(
            unique_path(&dir, "a", "jpg", &mut taken),
            dir.join("a-2.jpg")
        );
        assert_eq!(
            unique_path(&dir, "a", "jpg", &mut taken),
            dir.join("a-3.jpg")
        );
        assert_eq!(unique_path(&dir, "b", "jpg", &mut taken), dir.join("b.jpg"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn exports_many_with_their_own_tags() {
        let dir = temp("export");
        let a = dng(&dir, "a.dng");
        change_info(
            &a,
            &InfoChange {
                rating: Some(5),
                label: Some("Blue".into()),
                ..InfoChange::default()
            },
        )
        .unwrap();
        std::fs::write(dir.join("broken.dng"), "not a raw").unwrap();
        let out_dir = dir.join("out");
        std::fs::create_dir_all(&out_dir).unwrap();
        let paths = vec![
            a.display().to_string(),
            dir.join("broken.dng").display().to_string(),
            a.display().to_string(),
        ];
        let mut seen = Vec::new();
        let r = export_many(
            &paths,
            &out_dir,
            None,
            &amb_export::ExportSettings::new(amb_export::Format::Jpeg),
            true,
            &crate::LensSources::default(),
            &mut |d, n, name| seen.push((d, n, name.to_string())),
        );
        assert_eq!(r.written.len(), 2, "{:?}", r.errors);
        assert_eq!(r.errors.len(), 1);
        assert!(r.written[1].ends_with("a-2.jpg"), "{:?}", r.written);
        let bytes = std::fs::read(&r.written[0]).unwrap();
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("xmp:Rating=\"5\"") && text.contains("xmp:Label=\"Blue\""));
        assert_eq!(seen.last().unwrap().0, 3);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
