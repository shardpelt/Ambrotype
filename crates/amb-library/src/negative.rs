// SPDX-License-Identifier: GPL-3.0-or-later
//! Film negative rolls: the automatic analysis (film base, levels, midtone)
//! frozen into settings, for one frame or as the median of a roll, so later
//! edits or other frames don't shift the conversion
//! (docs/design/film-negatives.md).

use std::collections::BTreeMap;
use std::path::Path;

use amb_engine::negative::{Analysis, NegativeType};
use amb_presets::Value;
use amb_protocol::Command;
use rayon::prelude::*;

/// Analyse one photo's raw as Auto would now, with its edit: the film type,
/// a picked film base and the crop (levels are measured inside it).
pub fn analyze_file(photo: &Path) -> Result<Analysis, String> {
    let source = amb_engine::source_path(photo);
    let img = amb_engine::raw::load(&source).map_err(|e| format!("{}: {e}", source.display()))?;
    let settings = std::fs::read_to_string(amb_engine::sidecar_path(photo))
        .ok()
        .and_then(|t| amb_presets::xmp::parse_crs(&t).ok())
        .unwrap_or_default();
    Ok(analyze_image(&img, &settings, true))
}

/// The film type an edit says (`amb:NegativeType`).
pub fn film_type(settings: &BTreeMap<String, Value>) -> NegativeType {
    amb_engine::pipeline::DevelopSettings::from_settings(settings)
        .0
        .negative
        .kind
}

/// The analysis a render of a decoded photo with `settings` makes, on a
/// small copy; `fresh`: what Auto would measure now (keeping a picked base).
pub fn analyze_image(
    img: &amb_engine::raw::CameraImage,
    settings: &BTreeMap<String, Value>,
    fresh: bool,
) -> Analysis {
    let small = amb_engine::raw::downscale(img, amb_engine::pipeline::GLOBALS_LONG_SIDE);
    let develop = amb_engine::pipeline::DevelopSettings::from_settings(settings).0;
    amb_engine::pipeline::negative_analysis(&small, &develop, fresh)
}

/// Analyse several photos in parallel; one that can't be read is an error.
pub fn analyze_files(photos: &[&Path]) -> Result<Vec<Analysis>, String> {
    photos.par_iter().map(|p| analyze_file(p)).collect()
}

/// The frames' median as `amb:` settings: conversion on, Auto off, and the
/// film base, densities and midtone stored.
pub fn frozen(frames: &[Analysis]) -> BTreeMap<String, Value> {
    let median = |f: &dyn Fn(&Analysis) -> f64| -> f64 {
        let mut v: Vec<f64> = frames.iter().map(f).collect();
        v.sort_by(f64::total_cmp);
        let n = v.len();
        match n {
            0 => 0.0,
            _ if n % 2 == 1 => v[n / 2],
            _ => 0.5 * (v[n / 2 - 1] + v[n / 2]),
        }
    };
    let round = |v: f64| (v * 10_000.0).round() / 10_000.0;
    let mut settings = BTreeMap::new();
    let mut put = |k: &str, v: Value| {
        settings.insert(format!("amb:{k}"), v);
    };
    put("NegativeConvert", Value::Bool(true));
    put("NegativeAuto", Value::Bool(false));
    for (c, name) in ["Red", "Green", "Blue"].iter().enumerate() {
        put(
            &format!("FilmBase{name}"),
            Value::Number(round(median(&|a| a.base[c]))),
        );
        put(
            &format!("NegativeDensityLow{name}"),
            Value::Number(round(median(&|a| a.density_low[c]))),
        );
        put(
            &format!("NegativeDensityHigh{name}"),
            Value::Number(round(median(&|a| a.density_high[c]))),
        );
    }
    put(
        "NegativeMidtone",
        Value::Number(round(median(&|a| a.midtone))),
    );
    // Expired film's crossover, when there is one.
    if frames.iter().any(|a| a.crossover != [0.0; 3]) {
        for (c, name) in ["Red", "Green", "Blue"].iter().enumerate() {
            put(
                &format!("NegativeCrossover{name}"),
                Value::Number(round(median(&|a| a.crossover[c]))),
            );
        }
    }
    settings
}

/// The commands that store `settings` (from [`frozen`]).
pub fn commands(settings: &BTreeMap<String, Value>) -> Vec<Command> {
    settings
        .iter()
        .map(|(name, v)| Command::SetParam {
            name: name.clone(),
            value: match v {
                Value::Bool(b) => serde_json::json!(b),
                other => serde_json::json!(other.as_f64()),
            },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(base: f64, midtone: f64) -> Analysis {
        Analysis {
            base: [base, base * 0.6, base * 0.4],
            density_low: [0.1; 3],
            density_high: [1.5; 3],
            midtone,
            crossover: [0.0; 3],
        }
    }

    #[test]
    fn the_roll_median_is_stored_with_auto_off() {
        let s = frozen(&[frame(0.8, 0.4), frame(0.6, 0.5), frame(0.9, 0.45)]);
        assert_eq!(s["amb:NegativeAuto"], Value::Bool(false));
        assert_eq!(s["amb:FilmBaseRed"].as_f64(), Some(0.8));
        assert_eq!(s["amb:NegativeMidtone"].as_f64(), Some(0.45));
        let even = frozen(&[frame(0.8, 0.4), frame(0.6, 0.5)]);
        assert_eq!(even["amb:FilmBaseRed"].as_f64(), Some(0.7));
        let cmds = commands(&s);
        assert_eq!(cmds.len(), s.len());
        assert!(cmds.iter().any(|c| matches!(c, Command::SetParam { name, value } if name == "amb:NegativeConvert" && *value == serde_json::json!(true))));
    }
}
