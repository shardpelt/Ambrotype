// SPDX-License-Identifier: GPL-3.0-or-later
//! The photos next to the open one (the filmstrip): the raws and rendered
//! images (JPEG, PNG, TIFF, WebP) in its folder, in name order, with their
//! rating and whether they have edits, and small thumbnails from the
//! previews cameras embed in raws (quick: no raw decoding). Thumbnails are
//! cached on disk by path, size and modified time.

use serde::Serialize;
use std::path::{Path, PathBuf};

/// The long edge of filmstrip thumbnails, in pixels (sharp at 2× on a ~110 px strip).
pub const THUMB_EDGE: u32 = 256;

/// The most the thumbnail cache keeps; past it, the thumbnails shown least
/// lately go first.
pub const THUMB_CACHE_BYTES: u64 = 2 << 30;

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
    /// The stack it's in, if any.
    pub stack: Option<amb_protocol::sidecar::StackRef>,
    /// Where it was taken (latitude, longitude), as set in the sidecar
    /// (the camera's own GPS is the shot's `camera_gps`).
    pub gps: Option<[f64; 2]>,
}

pub use amb_library::folder::photo_extensions;

/// The raws in `photo`'s folder (including it), in name order.
pub fn list(photo: &Path) -> Result<Vec<FolderPhoto>, String> {
    let dir = photo
        .parent()
        .filter(|d| !d.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    list_dir(dir)
}

/// The photos in `dir`, in name order (`amb_library::folder::photo_paths`).
pub fn list_dir(dir: &Path) -> Result<Vec<FolderPhoto>, String> {
    Ok(describe(amb_library::folder::photo_paths(dir)?))
}

/// The photos in `dir` and all its subfolders: the folder's own first, then
/// each subfolder's in name order. Names of photos in subfolders start with
/// their folder (`2026-07/IMG_1.nef`).
pub fn list_tree(dir: &Path) -> Result<Vec<FolderPhoto>, String> {
    let found = amb_library::folder::tree_paths(dir)?;
    let (paths, folders): (Vec<PathBuf>, Vec<String>) = found.into_iter().unzip();
    Ok(describe(paths)
        .into_iter()
        .zip(folders)
        .map(|(mut p, f)| {
            if !f.is_empty() {
                p.name = format!("{f}/{}", p.name);
            }
            p
        })
        .collect())
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

/// The editor's catalogue (`amb_library::catalog`): what was read from
/// each photo's files, kept until they change. In memory only when its file
/// can't be opened, and in tests.
pub fn catalog() -> &'static amb_library::catalog::Catalog {
    use amb_library::catalog::{self, Catalog};
    static CATALOG: std::sync::OnceLock<Catalog> = std::sync::OnceLock::new();
    CATALOG.get_or_init(|| {
        if cfg!(test) {
            return Catalog::in_memory();
        }
        catalog::default_path()
            .and_then(|p| {
                Catalog::open(&p)
                    .map_err(|e| eprintln!("catalogue: {e}; keeping it in memory"))
                    .ok()
            })
            .unwrap_or_else(Catalog::in_memory)
    })
}

/// Name, rating and "edited" for each of `paths` (kept in the given order),
/// from the catalogue when the files haven't changed.
pub fn describe(paths: Vec<PathBuf>) -> Vec<FolderPhoto> {
    describe_with(paths).into_iter().map(|(p, _)| p).collect()
}

/// As [`describe`], with what the raw's header says about each shot.
pub fn describe_with(paths: Vec<PathBuf>) -> Vec<(FolderPhoto, amb_library::catalog::Shot)> {
    let records = catalog().records(&paths);
    paths
        .into_iter()
        .zip(records)
        .map(|(p, r)| from_record(&p, r))
        .collect()
}

/// A photo as listed, from what the catalogue knows of it.
pub fn from_record(
    p: &Path,
    r: amb_library::catalog::Record,
) -> (FolderPhoto, amb_library::catalog::Shot) {
    let gps = r.info.place().map(|(a, b)| [a, b]);
    let info = r.info;
    let photo = FolderPhoto {
        gps,
        name: display_name(p),
        copy: amb_engine::virtual_copy_of(p).map(|(_, n)| n),
        path: p.display().to_string(),
        rating: info.rating,
        label: info.label,
        keywords: info.keywords,
        flag: info.flag,
        edited: r.edited,
        title: info.title,
        caption: info.caption,
        creator: info.creator,
        copyright: info.copyright,
        stack: info.stack,
    };
    (photo, r.shot)
}

/// The file's size and modified time (nanoseconds), or while it can't be
/// reached (an unplugged drive) the ones the catalogue saw last, so its
/// cached thumbnails still show.
fn file_stamp(path: &Path) -> Option<(u64, u128)> {
    match std::fs::metadata(path) {
        Ok(meta) => Some((
            meta.len(),
            meta.modified()
                .ok()?
                .duration_since(std::time::UNIX_EPOCH)
                .ok()?
                .as_nanos(),
        )),
        Err(_) => {
            let s = catalog().stamp_of(path)?;
            Some((s.size as u64, s.modified as u128))
        }
    }
}

fn cache_key(path: &Path, edge: u32) -> Option<String> {
    use sha2::{Digest, Sha256};
    let (len, modified) = file_stamp(path)?;
    let mut h = Sha256::new();
    h.update(format!("v1|{}|{len}|{modified}|{edge}", path.display()));
    Some(
        h.finalize()
            .iter()
            .take(16)
            .map(|b| format!("{b:02x}"))
            .collect(),
    )
}

/// A cached file, marked as just used so pruning keeps it.
fn read_cached(path: &Path) -> Option<Vec<u8>> {
    let bytes = std::fs::read(path).ok()?;
    if let Ok(f) = std::fs::File::options().append(true).open(path) {
        let _ = f.set_modified(std::time::SystemTime::now());
    }
    Some(bytes)
}

/// Keep the cache folder within `max` bytes: the files used least lately
/// (by modified time, which a cache hit renews) are deleted first. Returns
/// how many files went and how many bytes that freed.
pub fn prune_cache(dir: &Path, max: u64) -> (usize, u64) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return (0, 0);
    };
    let mut files: Vec<(std::time::SystemTime, u64, PathBuf)> = entries
        .flatten()
        .filter_map(|e| {
            let m = e.metadata().ok().filter(|m| m.is_file())?;
            Some((m.modified().ok()?, m.len(), e.path()))
        })
        .collect();
    let mut total: u64 = files.iter().map(|f| f.1).sum();
    if total <= max {
        return (0, 0);
    }
    files.sort();
    let (mut removed, mut freed) = (0, 0);
    for (_, len, path) in files {
        if total <= max {
            break;
        }
        if std::fs::remove_file(&path).is_ok() {
            total -= len;
            freed += len;
            removed += 1;
        }
    }
    (removed, freed)
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
    if let Some(bytes) = key.as_ref().and_then(|k| read_cached(k)) {
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
    let stamp = std::fs::metadata(&sidecar)
        .ok()
        .and_then(|m| {
            Some((
                m.len(),
                m.modified()
                    .ok()?
                    .duration_since(std::time::UNIX_EPOCH)
                    .ok()?
                    .as_nanos(),
            ))
        })
        // Offline: the sidecar as the catalogue saw it last.
        .or_else(|| {
            let s = catalog().stamp_of(path).filter(|_| !path.exists())?;
            (s.sidecar_size >= 0).then_some((s.sidecar_size as u64, s.sidecar_modified as u128))
        });
    let key = cache
        .zip(cache_key(path, edge))
        .zip(stamp)
        .map(|((dir, k), (len, t))| dir.join(format!("e-{k}-{len}-{t}.jpg")));
    if let Some(bytes) = key.as_ref().and_then(|k| read_cached(k)) {
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
    fn the_thumbnail_cache_lets_the_least_used_go_first() {
        let dir = std::env::temp_dir().join(format!("amb-thumb-cache-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let t0 = std::time::SystemTime::now() - std::time::Duration::from_secs(1000);
        for (i, name) in ["a.jpg", "b.jpg", "c.jpg", "d.jpg"].iter().enumerate() {
            let p = dir.join(name);
            std::fs::write(&p, [0u8; 100]).unwrap();
            let f = std::fs::File::options().append(true).open(&p).unwrap();
            f.set_modified(t0 + std::time::Duration::from_secs(i as u64 * 10))
                .unwrap();
        }
        // "a" is the oldest, but shown again just now: it stays.
        assert_eq!(read_cached(&dir.join("a.jpg")).unwrap().len(), 100);
        assert_eq!(prune_cache(&dir, 400), (0, 0));
        assert_eq!(prune_cache(&dir, 250), (2, 200));
        assert!(dir.join("a.jpg").exists() && dir.join("d.jpg").exists());
        assert!(!dir.join("b.jpg").exists() && !dir.join("c.jpg").exists());
        let _ = std::fs::remove_dir_all(&dir);
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
