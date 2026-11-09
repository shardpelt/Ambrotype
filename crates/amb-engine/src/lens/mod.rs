// SPDX-License-Identifier: GPL-3.0-or-later
//! Lens Corrections: profile corrections (distortion, vignetting, lateral
//! chromatic aberration), Remove Chromatic Aberration, manual Distortion and
//! Lens Vignetting, Defringe, and Constrain Crop.
//!
//! **Model v1 (ours, not Adobe's).** Profile data is a [`amb_lens::Correction`]
//! resolved by the caller (lensfun database or the user's own Adobe LCP files).
//! The geometric part works on camera RGB before the colour matrix, in the
//! sensor's orientation:
//!
//! 1. vignetting gains (profile, then manual) on the source pixels;
//! 2. Remove CA: a radial red/blue shift model, estimated from the image's
//!    edges (or the profile's model if estimation finds too few edges);
//! 3. one resampling pass that composes crop and straighten
//!    ([`crate::crop`]) → Transform ([`crate::transform`]) → zoom → manual
//!    distortion → profile distortion → per-channel CA, with Catmull-Rom
//!    interpolation.
//!
//! Profile corrections scale to fill the frame (Lightroom auto-scales profile
//! corrections); manual distortion leaves blank areas unless Constrain Crop is
//! on. Defringe runs later, on the working-space image ([`apply_defringe`]).
//! All guessed constants live in [`LensStrengths`].

use std::borrow::Cow;

use amb_lens::{Correction, Geometry, ImageFrame};
use rayon::prelude::*;

use crate::buffer::{Image, Rect, Tile};
use crate::color::OkLab;
use crate::crop::CropGeometry;
use crate::filters::transpose;
use crate::math::smoothstep_f64 as smoothstep;
use crate::transform::Homography;

/// Strength and shape constants, fitted later against Lightroom sweeps.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct LensStrengths {
    /// Manual Distortion ±100: radial coefficient k of r' = r·(1 + k r²)/(1 + k),
    /// r = 1 at the corner (corners stay put, edge midpoints move).
    pub manual_distortion: f64,
    /// Manual Lens Vignetting ±100: corner brightening/darkening in stops.
    pub manual_vignette_stops: f64,
    /// Radius (fraction of the half diagonal) where the manual vignetting
    /// starts, at Midpoint 0 and at Midpoint 100.
    pub manual_vignette_start_min: f64,
    pub manual_vignette_start_max: f64,
    /// Apply a profile's lateral-CA model even without Remove Chromatic Aberration.
    pub profile_tca_always: bool,
    /// Display-linear grey level for areas the corrected image no longer covers.
    pub blank_fill: f64,
    /// Remove CA estimation: largest red/blue shift searched, in pixels.
    pub ca_max_shift: f64,
    /// Defringe search radius in pixels per Amount unit, at 24 MP (scales with resolution).
    pub defringe_radius: f64,
    /// Defringe edge detection: OkLab lightness range (max − min in the window)
    /// where the edge weight goes from 0 to 1.
    pub defringe_contrast_lo: f64,
    pub defringe_contrast_hi: f64,
    /// Hue slider 0..100 → OkLab hue degrees: start + slider/100 · span.
    pub purple_hue_start: f64,
    pub purple_hue_span: f64,
    pub green_hue_start: f64,
    pub green_hue_span: f64,
}

impl Default for LensStrengths {
    fn default() -> Self {
        DEFAULT_STRENGTHS
    }
}

pub const DEFAULT_STRENGTHS: LensStrengths = LensStrengths {
    manual_distortion: 0.15,
    manual_vignette_stops: 1.5,
    manual_vignette_start_min: 0.0,
    manual_vignette_start_max: 0.85,
    profile_tca_always: false,
    blank_fill: 1.0,
    ca_max_shift: 4.0,
    defringe_radius: 0.35,
    defringe_contrast_lo: 0.08,
    defringe_contrast_hi: 0.25,
    purple_hue_start: 230.0,
    purple_hue_span: 160.0,
    green_hue_start: 70.0,
    green_hue_span: 140.0,
};

/// Defringe sliders: Amount 0..20, hue ranges 0..100.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Defringe {
    pub purple_amount: f64,
    pub purple_hue_lo: f64,
    pub purple_hue_hi: f64,
    pub green_amount: f64,
    pub green_hue_lo: f64,
    pub green_hue_hi: f64,
    /// `amb:DefringeAuto`: find and remove fringes without hue bands
    /// ([`apply_auto_defringe_in`]).
    pub auto: bool,
}

impl Default for Defringe {
    fn default() -> Self {
        Defringe {
            purple_amount: 0.0,
            purple_hue_lo: 30.0,
            purple_hue_hi: 70.0,
            green_amount: 0.0,
            green_hue_lo: 40.0,
            green_hue_hi: 60.0,
            auto: false,
        }
    }
}

impl Defringe {
    pub fn is_neutral(&self) -> bool {
        self.purple_amount <= 0.0 && self.green_amount <= 0.0 && !self.auto
    }
}

/// The Lens Corrections panel.
#[derive(Debug, Clone, PartialEq)]
pub struct LensCorrections {
    /// Enable Profile Corrections.
    pub profile_enable: bool,
    /// Profile Distortion / Vignetting / CA amounts, 0..200 (100 = as profiled).
    pub distortion_scale: f64,
    pub vignetting_scale: f64,
    pub ca_scale: f64,
    /// Remove Chromatic Aberration.
    pub auto_ca: bool,
    /// Manual Distortion, −100..100 (positive corrects barrel distortion).
    pub manual_distortion: f64,
    /// Manual Lens Vignetting Amount −100..100 (positive lightens corners) and Midpoint 0..100.
    pub vignette_amount: f64,
    pub vignette_midpoint: f64,
    pub defringe: Defringe,
    /// Constrain Crop: scale up so no blank areas remain.
    pub constrain_crop: bool,
    /// The profile chosen in Lightroom (`LensProfileName` / `LensProfileFilename`), if any.
    pub profile_name: Option<String>,
    pub profile_filename: Option<String>,
    /// The profile resolved for this image by the caller. `None` with
    /// `profile_enable` means no profile was found: nothing profile-based is applied.
    pub profile: Option<Correction>,
    /// The raw file's own correction (DNG opcodes, [`LensCorrections::for_image`]):
    /// always applied, in place of any profile, as Camera Raw does.
    pub embedded: Option<Correction>,
}

impl Default for LensCorrections {
    fn default() -> Self {
        LensCorrections {
            profile_enable: false,
            distortion_scale: 100.0,
            vignetting_scale: 100.0,
            ca_scale: 100.0,
            auto_ca: false,
            manual_distortion: 0.0,
            vignette_amount: 0.0,
            vignette_midpoint: 50.0,
            defringe: Defringe::default(),
            constrain_crop: false,
            profile_name: None,
            profile_filename: None,
            profile: None,
            embedded: None,
        }
    }
}

impl LensCorrections {
    /// The correction applied: the photo's own, or the chosen profile.
    pub fn active_profile(&self) -> Option<&Correction> {
        self.embedded
            .as_ref()
            .or_else(|| self.profile.as_ref().filter(|_| self.profile_enable))
    }

    /// These settings for a photo: with the corrections it carries itself.
    pub fn for_image(&self, shot: &amb_lens::ShotInfo) -> Cow<'_, LensCorrections> {
        if shot.embedded.is_none() || self.embedded == shot.embedded {
            return Cow::Borrowed(self);
        }
        Cow::Owned(LensCorrections {
            embedded: shot.embedded.clone(),
            ..self.clone()
        })
    }

    /// Whether anything happens before the colour matrix.
    pub fn needs_camera_pass(&self) -> bool {
        self.active_profile().is_some()
            || self.auto_ca
            || self.manual_distortion != 0.0
            || self.vignette_amount != 0.0
    }
}

/// Result of the camera-space pass.
pub struct CameraPass {
    /// Output size: the input's, or the crop's.
    pub width: usize,
    pub height: usize,
    /// Corrected interleaved camera RGB, `width × height`.
    pub rgb: Vec<f32>,
    /// Pixels that the corrected image no longer covers (to fill with
    /// [`LensStrengths::blank_fill`] at the end of the pipeline).
    pub blank: Option<Vec<bool>>,
}

/// Radial red/blue displacement in pixels, outward positive:
/// δ(r) = a·r + b·r³ with r = 1 at the half diagonal.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RadialShift {
    pub a: f64,
    pub b: f64,
}

impl RadialShift {
    fn at(&self, r: f64) -> f64 {
        self.a * r + self.b * r * r * r
    }
}

/// Image centre and half diagonal in pixels.
fn centre(w: usize, h: usize) -> (f64, f64, f64) {
    let (cx, cy) = ((w as f64 - 1.0) / 2.0, (h as f64 - 1.0) / 2.0);
    (cx, cy, cx.hypot(cy).max(1.0))
}

pub use ca::estimate_ca;
pub(crate) use camera::sample_bicubic_rgb;
pub use camera::{apply_camera, apply_camera_cropped, apply_camera_window, fill_blank};
pub use defringe::{
    apply_auto_defringe_in, apply_defringe, apply_defringe_in, apply_defringe_weighted,
    defringe_radius,
};

mod ca;
mod camera;
mod defringe;
#[cfg(test)]
mod tests;
