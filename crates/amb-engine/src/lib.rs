// SPDX-License-Identifier: GPL-3.0-or-later
//! Ambrotype processing engine, v0.
//!
//! A first end-to-end path: decode a raw ([`raw`]), develop it in linear
//! ProPhoto with white balance, Exposure, the Basic tone sliders ([`tone`]),
//! colour sliders ([`color`]) and curves ([`pipeline`]), and
//! write sRGB output ([`output`]). The full tiled, GPU-backed graph described in
//! docs/SCOPE.md §9 replaces this incrementally; this crate fixes the colour
//! maths and gives the calibration harness something to compare against.

pub mod brush;
pub mod buffer;
pub mod calibration;
pub mod color;
pub mod crop;
pub mod curve;
pub mod dehaze;
pub mod detail;
pub mod effects;
pub mod filters;
pub mod lens;
pub mod local;
pub mod local_tone;
pub mod mask;
pub mod negative;
pub mod output;
pub mod pipeline;
pub mod primaries;
pub mod raster;
pub mod raw;
pub mod retouch;
pub mod tone;
pub mod transform;
pub mod upright;
pub mod xtrans;

/// Where a photo's edits are kept. A raw follows Lightroom: `photo.nef` →
/// `photo.xmp`. A rendered image keeps its extension, `photo.jpg` →
/// `photo.jpg.xmp`, so a JPEG shot next to a raw of the same name never
/// shares the raw's sidecar. (Lightroom writes JPEG edits into the JPEG
/// itself; we read those but don't write into the file.)
///
/// A virtual copy ([`virtual_copy_of`]) is its own sidecar.
pub fn sidecar_path(photo: &std::path::Path) -> std::path::PathBuf {
    if virtual_copy_of(photo).is_some() {
        return photo.to_path_buf();
    }
    if raster::is_raster(photo) {
        let mut name = photo.as_os_str().to_owned();
        name.push(".xmp");
        name.into()
    } else {
        photo.with_extension("xmp")
    }
}

/// A virtual copy: a second edit of a photo, kept as its own sidecar next
/// to it, `IMG_0001.copy-2.xmp` for `IMG_0001.nef` (`photo.jpg.copy-2.xmp`
/// for `photo.jpg`). Lightroom reads only `IMG_0001.xmp`, so it doesn't see
/// copies. Returns the photo the copy belongs to and the copy's number, or
/// `None` when `path` isn't a copy (no such photo next to it).
pub fn virtual_copy_of(path: &std::path::Path) -> Option<(std::path::PathBuf, u32)> {
    let name = path.file_name()?.to_str()?;
    let stem = name
        .strip_suffix(".xmp")
        .or_else(|| name.strip_suffix(".XMP"))?;
    let (base, n) = stem.rsplit_once(".copy-")?;
    let n: u32 = n.parse().ok().filter(|n| *n >= 2)?;
    let dir = path.parent()?;
    // A rendered image keeps its extension in the base name.
    let direct = dir.join(base);
    if raster::is_raster(&direct) && direct.is_file() {
        return Some((direct, n));
    }
    raw::RAW_EXTENSIONS
        .iter()
        .flat_map(|e| [e.to_string(), e.to_uppercase()])
        .map(|e| dir.join(format!("{base}.{e}")))
        .find(|p| p.is_file())
        .map(|p| (p, n))
}

/// The image file to decode for `photo`: the photo itself, or for a
/// virtual copy the photo it belongs to.
pub fn source_path(photo: &std::path::Path) -> std::path::PathBuf {
    virtual_copy_of(photo).map_or_else(|| photo.to_path_buf(), |(p, _)| p)
}

/// Where a new virtual copy of `photo` (or of the photo a copy belongs
/// to) goes: the next free `.copy-N.xmp`, from 2.
pub fn new_virtual_copy_path(photo: &std::path::Path) -> std::path::PathBuf {
    let source = source_path(photo);
    let base = if raster::is_raster(&source) {
        source
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned()
    } else {
        source
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned()
    };
    let dir = source.parent().unwrap_or(std::path::Path::new("."));
    (2..)
        .map(|n| dir.join(format!("{base}.copy-{n}.xmp")))
        .find(|p| !p.exists())
        .expect("some copy number is free")
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("decode failed: {0}")]
    Decode(String),
    #[error("unsupported input: {0}")]
    Unsupported(String),
    #[error("output failed: {0}")]
    Output(String),
}

#[cfg(test)]
mod copy_tests {
    use super::*;

    #[test]
    fn virtual_copies_point_at_their_photo() {
        let dir = std::env::temp_dir().join(format!("amb-copies-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let raw = dir.join("IMG_1.NEF");
        let jpg = dir.join("phone.jpg");
        std::fs::write(&raw, b"raw").unwrap();
        std::fs::write(&jpg, b"jpg").unwrap();
        let copy = new_virtual_copy_path(&raw);
        assert_eq!(copy, dir.join("IMG_1.copy-2.xmp"));
        std::fs::write(&copy, b"").unwrap();
        assert_eq!(virtual_copy_of(&copy), Some((raw.clone(), 2)));
        assert_eq!(new_virtual_copy_path(&copy), dir.join("IMG_1.copy-3.xmp"));
        assert_eq!(sidecar_path(&copy), copy);
        assert_eq!(source_path(&copy), raw);
        assert_eq!(sidecar_path(&raw), dir.join("IMG_1.xmp"));
        let jcopy = new_virtual_copy_path(&jpg);
        assert_eq!(jcopy, dir.join("phone.jpg.copy-2.xmp"));
        assert_eq!(virtual_copy_of(&jcopy), Some((jpg, 2)));
        // Not copies: a plain sidecar, number 1, or no photo next to it.
        assert!(virtual_copy_of(&dir.join("IMG_1.xmp")).is_none());
        assert!(virtual_copy_of(&dir.join("IMG_1.copy-1.xmp")).is_none());
        assert!(virtual_copy_of(&dir.join("other.copy-2.xmp")).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
