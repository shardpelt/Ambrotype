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
pub mod model;

pub use geometry::{Geometry, ImageFrame};
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
