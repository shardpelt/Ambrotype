//! Detail panel: sharpening and noise reduction.
//!
//! STUB: identity until implemented. Interface contract:
//! - both run on the display-linear ProPhoto image near the end of the
//!   pipeline (noise reduction first, then sharpening);
//! - parameters use Camera Raw units and ranges (see the field docs);
//! - each must be a no-op at its neutral values (amount 0) and keep values finite.

use serde::{Deserialize, Serialize};

use crate::buffer::Image;

/// `Sharpness` 0..150, `SharpenRadius` 0.5..3.0, `SharpenDetail` 0..100, `SharpenEdgeMasking` 0..100.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sharpening {
    pub amount: f64,
    pub radius: f64,
    pub detail: f64,
    pub masking: f64,
}

impl Default for Sharpening {
    /// Camera Raw's defaults for raw files.
    fn default() -> Self {
        Sharpening {
            amount: 40.0,
            radius: 1.0,
            detail: 25.0,
            masking: 0.0,
        }
    }
}

/// `LuminanceSmoothing`, `LuminanceNoiseReductionDetail`, `LuminanceNoiseReductionContrast`,
/// `ColorNoiseReduction`, `ColorNoiseReductionDetail`, `ColorNoiseReductionSmoothness`; all 0..100.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NoiseReduction {
    pub luminance: f64,
    pub luminance_detail: f64,
    pub luminance_contrast: f64,
    pub color: f64,
    pub color_detail: f64,
    pub color_smoothness: f64,
}

impl Default for NoiseReduction {
    /// Camera Raw's defaults for raw files.
    fn default() -> Self {
        NoiseReduction {
            luminance: 0.0,
            luminance_detail: 50.0,
            luminance_contrast: 0.0,
            color: 25.0,
            color_detail: 50.0,
            color_smoothness: 50.0,
        }
    }
}

/// Tunable constants (fitted later against Lightroom sweeps).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DetailStrengths {}

pub fn apply_noise_reduction(_img: &mut Image, _p: &NoiseReduction, _k: &DetailStrengths) {}

pub fn apply_sharpening(_img: &mut Image, _p: &Sharpening, _k: &DetailStrengths) {}
