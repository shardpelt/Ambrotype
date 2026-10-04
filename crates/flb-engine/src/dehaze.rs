//! Dehaze: remove (positive) or add (negative) atmospheric haze.
//!
//! STUB: identity until implemented. Interface contract:
//! - runs on the scene-linear ProPhoto image, after Exposure and before the base look;
//! - `amount` is the Camera Raw slider value in -100..100; no-op at 0;
//! - values may exceed 1.0 (scene-linear); keep them finite and non-negative.

use serde::{Deserialize, Serialize};

use crate::buffer::Image;

/// Tunable constants (fitted later against Lightroom sweeps).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DehazeStrengths {}

pub fn apply(_img: &mut Image, _amount: f64, _k: &DehazeStrengths) {}
