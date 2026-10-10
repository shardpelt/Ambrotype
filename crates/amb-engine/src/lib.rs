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
pub mod raster;
pub mod raw;
pub mod retouch;
pub mod tone;
pub mod xtrans;

/// Where a photo's edits are kept. A raw follows Lightroom: `photo.nef` →
/// `photo.xmp`. A rendered image keeps its extension, `photo.jpg` →
/// `photo.jpg.xmp`, so a JPEG shot next to a raw of the same name never
/// shares the raw's sidecar. (Lightroom writes JPEG edits into the JPEG
/// itself; we read those but don't write into the file.)
pub fn sidecar_path(photo: &std::path::Path) -> std::path::PathBuf {
    if raster::is_raster(photo) {
        let mut name = photo.as_os_str().to_owned();
        name.push(".xmp");
        name.into()
    } else {
        photo.with_extension("xmp")
    }
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
