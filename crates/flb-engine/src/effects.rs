//! Effects panel: post-crop vignette and grain.
//!
//! STUB: identity until implemented. Interface contract:
//! - both run last, on the display-linear ProPhoto image (vignette, then grain);
//! - parameters use Camera Raw units (see field docs); no-op when amount is 0;
//! - grain must be deterministic for a given seed and image size.

use serde::{Deserialize, Serialize};

use crate::buffer::Image;

/// `PostCropVignetteAmount` -100..100, `...Midpoint` 0..100 (50), `...Roundness` -100..100 (0),
/// `...Feather` 0..100 (50), `...HighlightContrast` 0..100 (0), `...Style` 1 = Highlight
/// Priority, 2 = Color Priority, 3 = Paint Overlay.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vignette {
    pub amount: f64,
    pub midpoint: f64,
    pub roundness: f64,
    pub feather: f64,
    pub highlights: f64,
    pub style: u8,
}

impl Default for Vignette {
    fn default() -> Self {
        Vignette {
            amount: 0.0,
            midpoint: 50.0,
            roundness: 0.0,
            feather: 50.0,
            highlights: 0.0,
            style: 1,
        }
    }
}

/// `GrainAmount` 0..100, `GrainSize` 0..100 (25), `GrainFrequency` (roughness) 0..100 (50).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Grain {
    pub amount: f64,
    pub size: f64,
    pub roughness: f64,
    pub seed: u64,
}

impl Default for Grain {
    fn default() -> Self {
        Grain {
            amount: 0.0,
            size: 25.0,
            roughness: 50.0,
            seed: 0,
        }
    }
}

/// Tunable constants (fitted later against Lightroom sweeps).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EffectsStrengths {}

pub fn apply_vignette(_img: &mut Image, _p: &Vignette, _k: &EffectsStrengths) {}

pub fn apply_grain(_img: &mut Image, _p: &Grain, _k: &EffectsStrengths) {}
