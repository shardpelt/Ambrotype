// SPDX-License-Identifier: GPL-3.0-or-later
//! The photos next to the open one (the filmstrip): the raws and rendered
//! images (JPEG, PNG, TIFF, WebP) in its folder, in name order, with their
//! rating and whether they have edits, and small thumbnails from the
//! previews cameras embed in raws (quick: no raw decoding). Thumbnails are
//! cached on disk by path, size and modified time.

use serde::Serialize;
use std::cmp::Ordering;
use std::path::{Path, PathBuf};

pub use amb_engine::raw::RAW_EXTENSIONS;

/// The long edge of filmstrip thumbnails, in pixels (sharp at 2× on a ~110 px strip).
pub const THUMB_EDGE: u32 = 256;

#[derive(Debug, Clone, Serialize)]
pub struct FolderPhoto {
    pub path: String,
    pub name: String,
    pub rating: Option<u8>,
    /// Colour label ("Red", ...), from the sidecar.
    pub label: Option<String>,
    pub keywords: Vec<String>,
    /// A virtual copy's number (its `path` is its sidecar).
    pub copy: Option<u32>,
    /// "pick" or "reject", from the sidecar.
    pub flag: Option<amb_protocol::sidecar::Flag>,
    /// The sidecar has develop settings.
    pub edited: bool,
    /// Title, caption, creator and copyright (Lightroom's Metadata panel).
    pub title: Option<String>,
    pub caption: Option<String>,
    pub creator: Option<String>,
    pub copyright: Option<String>,
}

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
fn natural(a: &str, b: &str) -> Ordering {
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

/// The raws in `photo`'s folder (including it), in name order.
pub fn list(photo: &Path) -> Result<Vec<FolderPhoto>, String> {
    let dir = photo
        .parent()
        .filter(|d| !d.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    list_dir(dir)
}

/// The photos in `dir`, in name order; a JPEG with a raw of the same name
/// is left out (the raw stands for both).
pub fn list_dir(dir: &Path) -> Result<Vec<FolderPhoto>, String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let found: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && is_photo(p))
        .filter(|p| {
            !p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with('.'))
        })
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
    paths.sort_by(|a, b| {
        natural(
            &a.file_name().unwrap_or_default().to_string_lossy(),
            &b.file_name().unwrap_or_default().to_string_lossy(),
        )
    });
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
    Ok(describe(ordered))
}

/// The photos in `dir` and all its subfolders (hidden ones left out): the
/// folder's own first, then each subfolder's in name order. Names of
/// photos in subfolders start with their folder (`2026-07/IMG_1.nef`).
pub fn list_tree(dir: &Path) -> Result<Vec<FolderPhoto>, String> {
    let mut out = list_dir(dir)?;
    let mut subdirs: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .filter(|p| {
            !p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with('.'))
        })
        .collect();
    subdirs.sort_by(|a, b| {
        natural(
            &a.file_name().unwrap_or_default().to_string_lossy(),
            &b.file_name().unwrap_or_default().to_string_lossy(),
        )
    });
    for sub in subdirs {
        let folder = sub
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        // A folder that can't be read is skipped, not the whole tree.
        for mut p in list_tree(&sub).unwrap_or_default() {
            p.name = format!("{folder}/{}", p.name);
            out.push(p);
        }
    }
    Ok(out)
}

/// A photo's name as shown: its file name, and "Copy N" for a virtual copy.
pub fn display_name(path: &Path) -> String {
    let file = |p: &Path| {
        p.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned()
    };
    match amb_engine::virtual_copy_of(path) {
        Some((photo, n)) => format!("{} · Copy {n}", file(&photo)),
        None => file(path),
    }
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

/// Name, rating and "edited" for each of `paths` (kept in the given order).
pub fn describe(paths: Vec<PathBuf>) -> Vec<FolderPhoto> {
    paths
        .into_iter()
        .map(|p| {
            let sidecar = std::fs::read_to_string(crate::session::sidecar_path(&p)).ok();
            let info = sidecar
                .as_deref()
                .map(amb_protocol::sidecar::read_info)
                .unwrap_or_default();
            let edited = sidecar
                .as_deref()
                .and_then(|t| amb_presets::xmp::parse_crs(t).ok())
                .is_some_and(|s| !s.is_empty());
            FolderPhoto {
                name: display_name(&p),
                copy: amb_engine::virtual_copy_of(&p).map(|(_, n)| n),
                path: p.display().to_string(),
                rating: info.rating,
                label: info.label,
                keywords: info.keywords,
                flag: info.flag,
                edited,
                title: info.title,
                caption: info.caption,
                creator: info.creator,
                copyright: info.copyright,
            }
        })
        .collect()
}

fn cache_key(path: &Path, edge: u32) -> Option<String> {
    use sha2::{Digest, Sha256};
    let meta = std::fs::metadata(path).ok()?;
    let modified = meta
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_nanos();
    let mut h = Sha256::new();
    h.update(format!(
        "v1|{}|{}|{modified}|{edge}",
        path.display(),
        meta.len()
    ));
    Some(
        h.finalize()
            .iter()
            .take(16)
            .map(|b| format!("{b:02x}"))
            .collect(),
    )
}

fn encode_jpeg(rgb: &[u8], w: u32, h: u32) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let enc = jpeg_encoder::Encoder::new(&mut out, 82);
    let (w, h) = (
        u16::try_from(w).map_err(|e| e.to_string())?,
        u16::try_from(h).map_err(|e| e.to_string())?,
    );
    enc.encode(rgb, w, h, jpeg_encoder::ColorType::Rgb)
        .map_err(|e| e.to_string())?;
    Ok(out)
}

/// A JPEG thumbnail of `path`: the embedded preview (cached in `cache`), or
/// when a file has none, a small render of the raw (slower).
pub fn thumbnail(path: &Path, edge: u32, cache: Option<&Path>) -> Result<Vec<u8>, String> {
    // A virtual copy shows its photo's camera preview.
    let path = &amb_engine::source_path(path);
    let edge = edge.clamp(64, 2048);
    let key = cache
        .zip(cache_key(path, edge))
        .map(|(dir, k)| dir.join(format!("{k}.jpg")));
    if let Some(bytes) = key.as_ref().and_then(|k| std::fs::read(k).ok()) {
        return Ok(bytes);
    }
    let jpeg = match amb_engine::raw::embedded_thumbnail(path, edge) {
        Ok(Some(img)) => encode_jpeg(img.as_raw(), img.width(), img.height())?,
        _ => {
            let mut s = crate::session::Session::open(path)?;
            let p = s.preview(
                edge as usize,
                &Default::default(),
                false,
                false,
                &crate::LensSources::default(),
            );
            p.jpeg
        }
    };
    if let Some(k) = key {
        if let Some(dir) = k.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(&k, &jpeg);
    }
    Ok(jpeg)
}

/// A JPEG of `path` with its sidecar's edits (crop included), cached in
/// `cache` until the raw or the sidecar changes. Slower the first time: the
/// raw is decoded.
pub fn edited_thumbnail(
    path: &Path,
    edge: u32,
    cache: Option<&Path>,
    lens: &crate::LensSources,
) -> Result<Vec<u8>, String> {
    let edge = edge.clamp(64, 2048);
    let sidecar = crate::session::sidecar_path(path);
    let stamp = std::fs::metadata(&sidecar).ok().and_then(|m| {
        Some((
            m.len(),
            m.modified()
                .ok()?
                .duration_since(std::time::UNIX_EPOCH)
                .ok()?
                .as_nanos(),
        ))
    });
    let key = cache
        .zip(cache_key(path, edge))
        .zip(stamp)
        .map(|((dir, k), (len, t))| dir.join(format!("e-{k}-{len}-{t}.jpg")));
    if let Some(bytes) = key.as_ref().and_then(|k| std::fs::read(k).ok()) {
        return Ok(bytes);
    }
    let mut s = crate::session::Session::open(path)?;
    let jpeg = s
        .preview(edge as usize, &Default::default(), false, false, lens)
        .jpeg;
    if let Some(k) = key {
        if let Some(dir) = k.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(&k, &jpeg);
    }
    Ok(jpeg)
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

    #[test]
    fn lists_raws_with_ratings_and_makes_thumbnails() {
        let dir = std::env::temp_dir().join(format!("amb-folder-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for name in ["b.dng", "a.dng"] {
            amb_testkit::dng::write(
                &dir.join(name),
                &amb_testkit::patterns::generate("ramp").unwrap(),
                &Default::default(),
            )
            .unwrap();
        }
        std::fs::write(dir.join("notes.txt"), "x").unwrap();
        std::fs::write(
            dir.join("a.xmp"),
            amb_protocol::sidecar::with_info(
                None,
                &amb_protocol::sidecar::PhotoInfo {
                    rating: Some(3),
                    keywords: vec![],
                    label: None,
                    flag: None,
                    ..Default::default()
                },
            )
            .unwrap(),
        )
        .unwrap();
        let list = list(&dir.join("b.dng")).unwrap();
        let names: Vec<_> = list.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["a.dng", "b.dng"]);
        assert_eq!((list[0].rating, list[0].edited), (Some(3), false));
        // Our synthetic DNGs carry no preview: the render fallback, then the cache.
        let cache = dir.join("cache");
        let first = thumbnail(&dir.join("a.dng"), THUMB_EDGE, Some(&cache)).unwrap();
        assert_eq!(&first[..2], [0xFF, 0xD8]);
        assert_eq!(std::fs::read_dir(&cache).unwrap().count(), 1);
        assert_eq!(
            thumbnail(&dir.join("a.dng"), THUMB_EDGE, Some(&cache)).unwrap(),
            first
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_jpeg_next_to_its_raw_is_one_photo() {
        let dir = std::env::temp_dir().join(format!("amb-pairs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        amb_testkit::dng::write(
            &dir.join("DSCF0001.dng"),
            &amb_testkit::patterns::generate("ramp").unwrap(),
            &Default::default(),
        )
        .unwrap();
        let jpeg = image::RgbImage::from_pixel(8, 8, image::Rgb([120, 130, 140]));
        for name in ["DSCF0001.JPG", "phone.jpg", "scan.png"] {
            jpeg.save_with_format(dir.join(name), image::ImageFormat::Png)
                .unwrap();
        }
        let names: Vec<_> = list_dir(&dir)
            .unwrap()
            .into_iter()
            .map(|p| p.name)
            .collect();
        assert_eq!(names, ["DSCF0001.dng", "phone.jpg", "scan.png"]);
        // Each keeps its own sidecar.
        assert_eq!(
            crate::session::sidecar_path(&dir.join("phone.jpg")),
            dir.join("phone.jpg.xmp")
        );
        assert_eq!(
            crate::session::sidecar_path(&dir.join("DSCF0001.dng")),
            dir.join("DSCF0001.xmp")
        );
        // A lone JPEG opens and renders a thumbnail.
        assert_eq!(
            &thumbnail(&dir.join("phone.jpg"), 64, None).unwrap()[..2],
            [0xFF, 0xD8]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
