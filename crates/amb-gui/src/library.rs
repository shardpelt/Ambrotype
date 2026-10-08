// SPDX-License-Identifier: GPL-3.0-or-later
//! The library: a folder of raws as a grid, to sort, filter, rate, label and
//! keyword many photos at once, paste one photo's edit onto others, and
//! export a selection.
//!
//! Everything here reads and writes the photos' XMP sidecars, the same files
//! the editor and Lightroom use; nothing else is stored. Edits go through
//! `amb-protocol` commands like every other edit.

use crate::folder::{self, FolderPhoto};
use amb_protocol::sidecar::{self, PhotoInfo};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// A photo in the grid: its folder entry and when it was taken.
#[derive(Debug, Clone, Serialize)]
pub struct LibraryPhoto {
    #[serde(flatten)]
    pub photo: FolderPhoto,
    /// "YYYY-MM-DD HH:MM:SS" from the raw's EXIF, when it has one.
    pub captured: Option<String>,
}

/// The raws in `dir`, in name order, with their capture times (read from the
/// raw headers in parallel; no image is decoded).
pub fn list(dir: &Path) -> Result<Vec<LibraryPhoto>, String> {
    let photos = folder::list_dir(dir)?;
    Ok(photos
        .into_par_iter()
        .map(|photo| {
            let captured = amb_engine::raw::read_metadata(Path::new(&photo.path))
                .ok()
                .and_then(|(_, m)| m.date_time_original)
                .map(|d| {
                    // EXIF writes "2026:05:04 12:30:00".
                    let (date, time) = d.split_once(' ').unwrap_or((&d, ""));
                    format!("{} {time}", date.replace(':', "-"))
                        .trim()
                        .to_string()
                });
            LibraryPhoto { photo, captured }
        })
        .collect())
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
    #[serde(default)]
    pub add_keywords: Vec<String>,
    #[serde(default)]
    pub remove_keywords: Vec<String>,
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
        let stem = p
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let dest = unique_path(dir, &stem, settings.format.extension(), &mut taken);
        let result = crate::session::Session::open(p).and_then(|mut s| {
            let mut settings = settings.clone();
            if include_tags {
                settings.tags = amb_export::exif::Tags {
                    rating: s.info.rating,
                    keywords: s.info.keywords.clone(),
                    label: s.info.label.clone(),
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
        let listed = list(&dir).unwrap();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[1].photo.keywords, ["beach", "family"]);
        assert_eq!(listed[1].photo.label.as_deref(), Some("Green"));
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
