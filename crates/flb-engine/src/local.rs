//! Texture and Clarity: local contrast at two scales.
//!
//! STUB: identity until implemented. Interface contract (do not change without
//! updating pipeline.rs):
//! - runs on the display-linear ProPhoto image, after the base look and the
//!   Basic tone sliders, before colour;
//! - `texture` and `clarity` are Camera Raw slider values in -100..100;
//! - must be a no-op when both are 0, must not shift global brightness or hue,
//!   and must keep values finite (clamping to 0..1 is the caller's job).

use serde::{Deserialize, Serialize};

use crate::buffer::Image;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct LocalContrast {
    pub texture: f64,
    pub clarity: f64,
}

impl LocalContrast {
    pub fn is_neutral(&self) -> bool {
        self.texture == 0.0 && self.clarity == 0.0
    }
}

/// Tunable constants (fitted later against Lightroom sweeps).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LocalStrengths {}

pub fn apply(_img: &mut Image, _p: &LocalContrast, _k: &LocalStrengths) {}
