// SPDX-License-Identifier: GPL-3.0-or-later
//! Lens profiles for Lens Corrections.
//!
//! Profile data comes from two places, neither shipped by us:
//! - the **lensfun** database (CC BY-SA 3.0), from a system install or a
//!   directory the user points at ([`lensfun`]);
//! - **Adobe LCP** files the user already has, e.g. from an existing Camera Raw
//!   or Lightroom install ([`lcp`]). We read them in place and never copy or
//!   redistribute them.
//!
//! Either way a profile is resolved for one shot (camera, lens, focal length,
//! aperture, focus distance) into a [`Correction`]: the interpolated
//! distortion, lateral chromatic aberration and vignetting models. [`Geometry`]
//! evaluates a correction on a concrete image; the engine does the resampling.

pub mod geometry;
mod interp;
pub mod lcp;
pub mod lensfun;
pub mod model;
mod names;
mod xml;

pub use geometry::{Geometry, ImageFrame};
pub use lcp::{LcpFile, LcpLibrary, LcpMatch};
pub use lensfun::{LensMatch, LensfunDb};
pub use model::{Correction, Distortion, Frame, Source, Tca, Vignetting};

/// What we know about the shot, from the raw file's metadata.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ShotInfo {
    pub camera_make: String,
    pub camera_model: String,
    pub lens_make: Option<String>,
    pub lens_model: Option<String>,
    /// Focal length in mm.
    pub focal_mm: Option<f64>,
    /// Aperture as an f-number (e.g. 2.8).
    pub aperture: Option<f64>,
    /// Focus distance in metres.
    pub distance_m: Option<f64>,
    /// Crop factor of the camera sensor (1.0 = 36×24 mm), when known.
    pub crop_factor: Option<f64>,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{path}: {message}")]
    Parse { path: String, message: String },
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

/// The profile a Lightroom/Camera Raw edit names (`crs:LensProfileName`,
/// `crs:LensProfileFilename`), when the sidecar has one.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProfileHint {
    pub name: Option<String>,
    pub filename: Option<String>,
}

/// The crop factor the lensfun database gives for a camera.
pub fn camera_crop(db: &LensfunDb, make: &str, model: &str) -> Option<f64> {
    db.camera_crop(make, model)
}

/// Resolve the lens correction for a shot, in order: the LCP file the edit
/// names, an LCP matched by lens name, the lensfun database. A missing crop
/// factor is filled from the lensfun camera database first.
pub fn resolve(
    shot: &ShotInfo,
    hint: &ProfileHint,
    lcp: Option<&LcpLibrary>,
    db: Option<&LensfunDb>,
) -> Option<Correction> {
    let mut shot = shot.clone();
    if shot.crop_factor.is_none() {
        shot.crop_factor = db.and_then(|db| db.camera_crop(&shot.camera_make, &shot.camera_model));
    }
    if let Some(lib) = lcp {
        let named = hint
            .filename
            .as_deref()
            .and_then(|name| lib.find_by_filename(name))
            .and_then(|path| lib.load(path).ok())
            .and_then(|file| file.correction(&shot));
        if named.is_some() {
            return named;
        }
        if let Some(c) = lib.find(&shot).and_then(|m| m.file.correction(&shot)) {
            return Some(c);
        }
    }
    let db = db?;
    let m = db.find_lens(&shot)?;
    let c = db.correction(&m, &shot);
    (c.distortion.is_some() || c.tca.is_some() || c.vignetting.is_some()).then_some(c)
}
