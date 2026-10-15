// SPDX-License-Identifier: GPL-3.0-or-later
//! The photos in a folder: raws and rendered images (JPEG, PNG, TIFF,
//! WebP), in name order, with their virtual copies, as the library, the
//! filmstrip and the catalogue list them.

use std::cmp::Ordering;
use std::path::{Path, PathBuf};

pub use amb_engine::raw::RAW_EXTENSIONS;

pub fn is_raw(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| RAW_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

/// Every extension the editor opens: raws, then rendered images.
pub fn photo_extensions() -> Vec<&'static str> {
    RAW_EXTENSIONS
        .iter()
        .chain(amb_engine::raster::EXTENSIONS)
        .copied()
        .collect()
}

pub fn is_photo(path: &Path) -> bool {
    is_raw(path) || amb_engine::raster::is_raster(path)
}

/// A JPEG shot together with a raw of the same name: like Lightroom, the
/// pair is one photo, the raw.
fn is_raw_companion(path: &Path, raw_stems: &std::collections::HashSet<String>) -> bool {
    let jpeg = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "jpg" | "jpeg"));
    jpeg && path
        .file_stem()
        .is_some_and(|s| raw_stems.contains(&s.to_string_lossy().to_lowercase()))
}

/// Compare names the way people expect: "IMG_2" before "IMG_10".
pub fn natural(a: &str, b: &str) -> Ordering {
    let (mut a, mut b) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (a.peek().copied(), b.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, _) => return Ordering::Less,
            (_, None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let take = |it: &mut std::iter::Peekable<std::str::Chars>| {
                    let mut s = String::new();
                    while let Some(c) = it.peek().copied().filter(char::is_ascii_digit) {
                        s.push(c);
                        it.next();
                    }
                    s
                };
                let (na, nb) = (take(&mut a), take(&mut b));
                let (ta, tb) = (na.trim_start_matches('0'), nb.trim_start_matches('0'));
                let ord = ta
                    .len()
                    .cmp(&tb.len())
                    .then_with(|| ta.cmp(tb))
                    .then_with(|| na.len().cmp(&nb.len()));
                if ord != Ordering::Equal {
                    return ord;
                }
            }
            (Some(x), Some(y)) => {
                let ord = x.to_lowercase().cmp(y.to_lowercase());
                if ord != Ordering::Equal {
                    return ord;
                }
                a.next();
                b.next();
            }
        }
    }
}

/// The photos in `dir`, in name order, each virtual copy right after its
/// photo; a JPEG with a raw of the same name is left out (the raw stands
/// for both). Hidden files are skipped.
pub fn photo_paths(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let found: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && is_photo(p))
        .filter(|p| !hidden(p))
        .collect();
    let raw_stems: std::collections::HashSet<String> = found
        .iter()
        .filter(|p| is_raw(p))
        .filter_map(|p| p.file_stem().map(|s| s.to_string_lossy().to_lowercase()))
        .collect();
    let mut paths: Vec<PathBuf> = found
        .into_iter()
        .filter(|p| !is_raw_companion(p, &raw_stems))
        .collect();
    paths.sort_by(|a, b| natural(&file_name(a), &file_name(b)));
    // Virtual copies right after their photo, in number order.
    let mut copies = copies_in(dir, &paths);
    copies.sort_by_key(|(_, _, n)| *n);
    let mut ordered = Vec::with_capacity(paths.len() + copies.len());
    for p in paths {
        let mine: Vec<PathBuf> = copies
            .iter()
            .filter(|(_, photo, _)| *photo == p)
            .map(|(c, _, _)| c.clone())
            .collect();
        ordered.push(p);
        ordered.extend(mine);
    }
    Ok(ordered)
}

/// The photos in `dir` and all its subfolders (hidden ones left out): the
/// folder's own first, then each subfolder's in name order. Each comes with
/// its folder relative to `dir` ("" for `dir`'s own, "2026-07/b" deeper).
pub fn tree_paths(dir: &Path) -> Result<Vec<(PathBuf, String)>, String> {
    let mut out: Vec<(PathBuf, String)> = photo_paths(dir)?
        .into_iter()
        .map(|p| (p, String::new()))
        .collect();
    let mut subdirs: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir() && !hidden(p))
        .collect();
    subdirs.sort_by(|a, b| natural(&file_name(a), &file_name(b)));
    for sub in subdirs {
        let folder = file_name(&sub);
        // A folder that can't be read is skipped, not the whole tree.
        for (p, inner) in tree_paths(&sub).unwrap_or_default() {
            let rel = if inner.is_empty() {
                folder.clone()
            } else {
                format!("{folder}/{inner}")
            };
            out.push((p, rel));
        }
    }
    Ok(out)
}

fn file_name(p: &Path) -> String {
    p.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

fn hidden(p: &Path) -> bool {
    p.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.starts_with('.'))
}

/// The virtual copies in `dir` of the photos `photos` (sidecars named
/// `<photo>.copy-N.xmp`), each with the photo it belongs to.
fn copies_in(dir: &Path, photos: &[PathBuf]) -> Vec<(PathBuf, PathBuf, u32)> {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter_map(|p| {
            let (photo, n) = amb_engine::virtual_copy_of(&p)?;
            photos.contains(&photo).then_some((p, photo, n))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natural_order() {
        let mut names = vec![
            "IMG_10.NEF",
            "img_2.nef",
            "IMG_1.NEF",
            "DSC_0100.NEF",
            "DSC_099.NEF",
        ];
        names.sort_by(|a, b| natural(a, b));
        assert_eq!(
            names,
            [
                "DSC_099.NEF",
                "DSC_0100.NEF",
                "IMG_1.NEF",
                "img_2.nef",
                "IMG_10.NEF"
            ]
        );
    }
}
