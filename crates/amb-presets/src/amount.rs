// SPDX-License-Identifier: GPL-3.0-or-later
//! Preset "Amount" (intensity) scaling.
//!
//! Adobe does not document how the Amount slider scales a preset. Until it is
//! measured (docs/lightroom-trial-plan.md, week 2), this implements the natural
//! model: numeric settings interpolate linearly from the base value towards the
//! preset value, and may extrapolate past it (Amount above 100%). Non-numeric
//! settings (text, enums, curves, masks) are taken from the preset whenever the
//! amount is above zero. Swap this out once real measurements exist.

use std::collections::BTreeMap;

use crate::registry::Registry;
use crate::value::Value;

/// Apply `preset` on top of `base` at `amount` (1.0 = 100%).
///
/// `base` holds the image's current settings. Keys missing from `base` fall back
/// to the registry default. Numeric tags without any known base are copied unscaled.
pub fn apply(
    base: &BTreeMap<String, Value>,
    preset: &BTreeMap<String, Value>,
    amount: f64,
    registry: &Registry,
) -> BTreeMap<String, Value> {
    let mut out = base.clone();
    if amount <= 0.0 {
        return out;
    }
    for (name, target) in preset {
        let info = registry.lookup(name);
        let numeric = info.as_ref().is_some_and(|i| i.kind == "number");
        let scaled = if numeric {
            let from = base
                .get(name)
                .and_then(Value::as_f64)
                .or_else(|| registry.numeric_default(name));
            match (from, target.as_f64()) {
                (Some(from), Some(to)) => {
                    let mut v = from + (to - from) * amount;
                    if let Some(info) = &info {
                        if let (Some(lo), Some(hi)) = (info.min, info.max) {
                            v = v.clamp(lo, hi);
                        }
                    }
                    Value::Number(v)
                }
                _ => target.clone(),
            }
        } else {
            target.clone()
        };
        out.insert(name.clone(), scaled);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(pairs: &[(&str, Value)]) -> BTreeMap<String, Value> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect()
    }

    #[test]
    fn interpolates_from_defaults_and_base() {
        let reg = Registry::builtin();
        let preset = map(&[
            ("Exposure2012", Value::Text("+1.0".into())),
            ("ColorGradeBlending", Value::Number(100.0)),
            ("ToneCurveName2012", Value::Text("Custom".into())),
        ]);
        let base = map(&[("Exposure2012", Value::Number(0.5))]);
        let out = apply(&base, &preset, 0.5, reg);
        assert_eq!(out["Exposure2012"].as_f64(), Some(0.75)); // 0.5 → 1.0 at 50%
        assert_eq!(out["ColorGradeBlending"].as_f64(), Some(75.0)); // default 50 → 100
        assert_eq!(out["ToneCurveName2012"].as_str(), Some("Custom"));
    }

    #[test]
    fn zero_amount_is_identity_and_extrapolation_is_clamped() {
        let reg = Registry::builtin();
        let preset = map(&[("Vibrance", Value::Number(80.0))]);
        let base = BTreeMap::new();
        assert!(apply(&base, &preset, 0.0, reg).is_empty());
        assert_eq!(
            apply(&base, &preset, 2.0, reg)["Vibrance"].as_f64(),
            Some(100.0)
        );
    }
}
