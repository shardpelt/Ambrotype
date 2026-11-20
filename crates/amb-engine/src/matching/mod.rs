// SPDX-License-Identifier: GPL-3.0-or-later
//! Look matching (`amb match`): find the settings that make a photo look
//! like reference images. Ours: statistics, not a learned model.
//!
//! The references and the photo are measured the same way ([`LookStats`]):
//! the spread of CIE L* (percentiles), the mean colour of the shadows,
//! midtones and highlights, how colourful they are (chroma percentiles),
//! and local contrast at Texture's and Clarity's scales. These describe a
//! look without depending much on what is in the picture, and the spread of
//! a* and b* tells saturation apart from a white balance shift. The photo is
//! then developed small again and again while a least-squares solver
//! (Levenberg–Marquardt) moves its settings together: white balance, tone
//! (Exposure, Contrast, Highlights, Shadows, Whites, Blacks), the tone
//! curve (the parametric curve's four regions, not free points, so the
//! answer stays unique), Clarity and Texture, colour (Vibrance,
//! Saturation), Color Grading's shadow and highlight tints and the Color
//! Mixer. Each try costs one small render. A small penalty on every change
//! keeps the answer to the smallest moves that reach the look.

use std::collections::BTreeMap;

use amb_presets::Value;
use serde::{Deserialize, Serialize};

use crate::pipeline::DevelopSettings;

mod auto;
mod solve;
mod stats;
#[cfg(test)]
mod tests;

pub use auto::{auto, auto_target};
pub use solve::solve;
pub use stats::{
    L_PERCENTILES, LookStats, NEUTRAL_CHROMA, average, distance, distance_with, look_stats,
};

pub type Settings = BTreeMap<String, Value>;

/// How much each part of a look counts in the distance (1 = as in
/// [`distance`], 0 = not at all).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Weights {
    /// The L* percentiles.
    pub tone: f64,
    /// The shadow, midtone and highlight casts.
    pub cast: f64,
    /// Chroma percentiles and the a*/b* spread.
    pub colour: f64,
    /// The near-grey midtones' cast.
    pub neutral: f64,
    /// Local contrast at Texture's and Clarity's scales.
    pub detail: f64,
    /// The HSL bands' colours.
    pub bands: bool,
}

impl Default for Weights {
    fn default() -> Self {
        Weights {
            tone: 1.0,
            cast: 1.0,
            colour: 1.0,
            neutral: 1.0,
            detail: 1.0,
            bands: false,
        }
    }
}

/// The groups of settings matching may change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Part {
    WhiteBalance,
    Tone,
    /// The parametric tone curve: Shadows, Darks, Lights and Highlights.
    Curve,
    /// Clarity and Texture.
    Detail,
    Color,
    Grading,
    /// The Color Mixer: hue, saturation and luminance per colour band.
    Hsl,
}

impl Part {
    pub const ALL: [Part; 7] = [
        Part::WhiteBalance,
        Part::Tone,
        Part::Curve,
        Part::Detail,
        Part::Color,
        Part::Grading,
        Part::Hsl,
    ];

    pub fn parse(s: &str) -> Option<Part> {
        match s {
            "wb" | "white_balance" => Some(Part::WhiteBalance),
            "tone" => Some(Part::Tone),
            "curve" | "tone_curve" => Some(Part::Curve),
            "detail" | "clarity" | "texture" => Some(Part::Detail),
            "color" | "colour" => Some(Part::Color),
            "grading" | "color_grading" => Some(Part::Grading),
            "hsl" | "mixer" | "color_mixer" => Some(Part::Hsl),
            _ => None,
        }
    }
}

/// What of a look to take: all of it, only its tones (light, curve and
/// local contrast) or only its colours. For saved looks ("Styles") that
/// carry one half of a look onto any photo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Focus {
    All,
    Tone,
    Colour,
}

impl Focus {
    pub fn parse(s: &str) -> Option<Focus> {
        match s {
            "all" => Some(Focus::All),
            "tone" => Some(Focus::Tone),
            "colour" | "color" => Some(Focus::Colour),
            _ => None,
        }
    }

    /// The parts it lets the solver change.
    pub fn parts(self) -> Vec<Part> {
        match self {
            Focus::All => Part::ALL.to_vec(),
            Focus::Tone => vec![Part::Tone, Part::Curve, Part::Detail],
            Focus::Colour => vec![Part::WhiteBalance, Part::Color, Part::Grading, Part::Hsl],
        }
    }

    /// What counts in the distance: only what its parts can change, so the
    /// other half of the photo's look stays as it is.
    pub fn weights(self) -> Weights {
        match self {
            Focus::All => Weights::default(),
            Focus::Tone => Weights {
                tone: 1.0,
                detail: 1.0,
                cast: 0.0,
                colour: 0.0,
                neutral: 0.0,
                bands: false,
            },
            Focus::Colour => Weights {
                tone: 0.0,
                detail: 0.0,
                ..Weights::default()
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct MatchOptions {
    pub parts: Vec<Part>,
    /// Most renders to try.
    pub max_evaluations: usize,
    /// What counts in the distance (the bands only with [`Part::Hsl`], local
    /// contrast only with [`Part::Detail`]).
    pub weights: Weights,
}

impl Default for MatchOptions {
    fn default() -> Self {
        MatchOptions {
            parts: Part::ALL.to_vec(),
            max_evaluations: 600,
            weights: Weights::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct MatchResult {
    /// Settings to set, in this order, with their new values (only those
    /// that change; a setting may come twice, the last value counts).
    pub changes: Vec<(String, Value)>,
    /// The distance to the references before and after.
    pub before: f64,
    pub after: f64,
    pub evaluations: usize,
}

/// Settings as the pipeline renders them, with no lens or camera profiles.
pub fn develop_plain(s: &Settings) -> DevelopSettings {
    DevelopSettings::from_settings(s).0
}
