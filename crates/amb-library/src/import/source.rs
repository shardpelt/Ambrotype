// SPDX-License-Identifier: GPL-3.0-or-later
//! Import's source side: finding the photos on a card or folder, grouping them with
//! their companions, and reading when each was taken.

use super::*;

pub(super) fn is_photo(path: &Path) -> bool {
    amb_engine::raw::is_raw_path(path) || amb_engine::raster::is_raster(path)
}

fn ext_of(path: &Path) -> String {
    path.extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

fn hidden(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.starts_with('.'))
}

/// The photo files and sidecars under `dir`.
pub(super) fn scan(dir: &Path, subfolders: bool, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut entries: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if hidden(&p) {
            continue;
        }
        if p.is_dir() {
            if subfolders {
                scan(&p, true, out)?;
            }
        } else if is_photo(&p) || ext_of(&p) == "xmp" {
            out.push(p);
        }
    }
    Ok(())
}

/// A photo and its companions in the source.
pub(super) struct Group {
    pub(super) main: PathBuf,
    pub(super) others: Vec<PathBuf>,
}

/// A file's photo name and the rest of its name: `IMG_1` and `.CR2` for
/// `IMG_1.CR2`, `.xmp` for its sidecar, `.copy-2.xmp` for a virtual copy,
/// `.jpg.xmp` for a JPEG's sidecar `IMG_1.jpg.xmp`.
pub(crate) fn split_name(f: &Path) -> (String, String) {
    let name = f
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let mut base = match name.len().checked_sub(4) {
        Some(i) if name.is_char_boundary(i) && name[i..].eq_ignore_ascii_case(".xmp") => {
            name[..i].to_string()
        }
        _ => {
            let stem = f.file_stem().unwrap_or_default().to_string_lossy();
            return (stem.to_string(), name[stem.len()..].to_string());
        }
    };
    if let Some((b, n)) = base.rsplit_once(".copy-") {
        if !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()) {
            base = b.to_string();
        }
    }
    if amb_engine::raster::is_raster(Path::new(&base)) {
        if let Some((b, _)) = base.rsplit_once('.') {
            base = b.to_string();
        }
    }
    let rest = name[base.len()..].to_string();
    (base, rest)
}

/// Group files by folder and photo name: the raw (or else the first photo)
/// leads; JPEGs, sidecars and virtual copies follow it.
pub(super) fn group(files: Vec<PathBuf>) -> Vec<Group> {
    let mut by_stem: BTreeMap<(PathBuf, String), Vec<PathBuf>> = BTreeMap::new();
    for f in files {
        let dir = f.parent().map(Path::to_path_buf).unwrap_or_default();
        let stem = split_name(&f).0.to_lowercase();
        by_stem.entry((dir, stem)).or_default().push(f);
    }
    by_stem
        .into_values()
        .filter_map(|mut files| {
            let lead = files
                .iter()
                .position(|f| amb_engine::raw::is_raw_path(f))
                .or_else(|| files.iter().position(|f| is_photo(f)))?;
            let main = files.remove(lead);
            Some(Group {
                main,
                others: files,
            })
        })
        .collect()
}

pub(super) fn capture_time(path: &Path) -> (When, String) {
    let meta = amb_engine::raw::read_metadata(path).ok();
    let camera = meta
        .as_ref()
        .map(|(_, m)| m.model.trim().to_string())
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| "camera".into());
    let exif = meta
        .and_then(|(_, m)| m.date_time_original)
        .and_then(|t| When::from_exif(&t));
    let when = exif.unwrap_or_else(|| {
        let secs = std::fs::metadata(path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_secs() as i64);
        When::from_unix(secs)
    });
    (when, clean(&camera))
}
