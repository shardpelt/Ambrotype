//! Calibration: every tunable model constant in one serialisable place, so the
//! Lightroom calibration harness can fit them and ship them as data
//! (`amb render --calibration fitted.json`) instead of code changes.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::color::ColorStrengths;
use crate::dehaze::DehazeStrengths;
use crate::detail::DetailStrengths;
use crate::effects::EffectsStrengths;
use crate::lens::LensStrengths;
use crate::local::LocalStrengths;
use crate::negative::NegativeStrengths;
use crate::tone::ToneStrengths;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Calibration {
    pub tone: ToneStrengths,
    pub color: ColorStrengths,
    pub local: LocalStrengths,
    pub dehaze: DehazeStrengths,
    pub detail: DetailStrengths,
    pub effects: EffectsStrengths,
    pub lens: LensStrengths,
    pub negative: NegativeStrengths,
}

impl Calibration {
    /// Load from JSON. Missing sections or fields keep their defaults.
    pub fn from_json_file(path: &Path) -> Result<Calibration, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("calibration serialises")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_json_keeps_defaults() {
        let c: Calibration = serde_json::from_str(r#"{"tone": {"contrast": 0.9}}"#).unwrap();
        assert_eq!(c.tone.contrast, 0.9);
        assert_eq!(c.tone.shadows, crate::tone::DEFAULT_STRENGTHS.shadows);
        assert_eq!(c.color, ColorStrengths::default());
        let round: Calibration = serde_json::from_str(&c.to_json()).unwrap();
        assert_eq!(round, c);
    }
}
