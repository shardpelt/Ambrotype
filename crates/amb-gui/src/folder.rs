//! The photos next to the open one (the filmstrip): the raws in its folder,
//! in name order, with their rating and whether they have edits, and small
//! thumbnails from the previews cameras embed in raws (quick: no raw
//! decoding). Thumbnails are cached on disk by path, size and modified time.

use serde::Serialize;
use std::cmp::Ordering;
use std::path::{Path, PathBuf};

/// Raw file extensions the editor opens (lower case).
pub const RAW_EXTENSIONS: [&str; 21] = [
    "dng", "nef", "nrw", "cr2", "cr3", "crw", "arw", "srf", "sr2", "raf", "orf", "rw2", "pef",
    "srw", "3fr", "iiq", "erf", "mef", "mos", "kdc", "dcr",
];

/// The long edge of filmstrip thumbnails, in pixels (sharp at 2× on a ~110 px strip).
pub const THUMB_EDGE: u32 = 256;

#[derive(Debug, Clone, Serialize)]
pub struct FolderPhoto {
    pub path: String,
    pub name: String,
    pub rating: Option<u8>,
    /// The sidecar has develop settings.
    pub edited: bool,
}

pub fn is_raw(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| RAW_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
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
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && is_raw(p))
        .filter(|p| {
            !p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with('.'))
        })
        .collect();
    paths.sort_by(|a, b| {
        natural(
            &a.file_name().unwrap_or_default().to_string_lossy(),
            &b.file_name().unwrap_or_default().to_string_lossy(),
        )
    });
    Ok(paths
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
                name: p
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string(),
                path: p.display().to_string(),
                rating: info.rating,
                edited,
            }
        })
        .collect())
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
}
