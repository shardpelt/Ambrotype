// SPDX-License-Identifier: GPL-3.0-or-later
//! Film negative rolls: the automatic analysis (film base, levels, midtone)
//! frozen into settings, for one frame or as the median of a roll, so later
//! edits or other frames don't shift the conversion
//! (docs/design/film-negatives.md).

use std::collections::BTreeMap;
use std::path::Path;

use amb_engine::negative::{Analysis, NO_CROSSOVER, NegativeType};
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
    // Expired film's crossover, when there is one: three numbers per layer.
    if frames.iter().any(|a| a.crossover != NO_CROSSOVER) {
        for (c, name) in [(0, "Red"), (2, "Blue")] {
            let row: Vec<String> = (0..3)
                .map(|j| format!("{}", round(median(&|a| a.crossover[c][j]))))
                .collect();
            put(
                &format!("NegativeCrossover{name}"),
                Value::Text(row.join(" ")),
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
                Value::Text(t) => serde_json::json!(t),
                other => serde_json::json!(other.as_f64()),
            },
        })
        .collect()
}

/// How a roll is given one conversion ([`roll`]).
#[derive(Debug, Clone, Copy, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct RollOptions {
    /// Frames shot in different light (daylight, tungsten) are grouped, and
    /// each group gets its own median.
    pub scenes: bool,
    /// A frame far from its group's median keeps its own analysis.
    pub odd: bool,
    /// Every frame keeps its own midtone (its own brightness); the roll
    /// shares the film base and the levels ("lock bounds").
    pub own_midtone: bool,
    /// The baseline is this frame's analysis instead of the median.
    pub baseline: Option<usize>,
}

impl Default for RollOptions {
    fn default() -> Self {
        RollOptions {
            scenes: true,
            odd: true,
            own_midtone: false,
            baseline: None,
        }
    }
}

/// Frames whose light differs by more than this (the log ratio of red's or
/// blue's highlight density to green's) are different scenes. **Ours:** on
/// two BlueNeg rolls it keeps a daylight roll together and puts its night
/// frames apart.
pub const SCENE_GAP: f64 = 0.2;
/// A frame whose levels sit further than this (density) from its group's
/// baseline is odd: about two stops on colour negative film. **Ours.**
pub const ODD_GAP: f64 = 0.35;

/// One frame's place in the roll.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct RollFrame {
    /// Its scene (0 when scenes aren't grouped).
    pub scene: usize,
    /// It keeps its own analysis.
    pub odd: bool,
    /// Its settings ([`frozen`] form).
    #[serde(skip)]
    pub settings: BTreeMap<String, Value>,
}

/// The light's colour as the levels show it: red's and blue's highlight
/// density against green's, as log ratios (a ratio, so exposure and
/// contrast, which scale the layers together, don't count).
fn light(a: &Analysis) -> [f64; 2] {
    let g = a.density_high[1].max(1e-3);
    [
        (a.density_high[0].max(1e-3) / g).ln(),
        (a.density_high[2].max(1e-3) / g).ln(),
    ]
}

/// One conversion for a roll, with roll baseline. Frames are grouped by
/// their light (each new frame joins the first group whose median light is
/// within [`SCENE_GAP`]); each group's baseline is its median (or the
/// chosen frame's analysis, for that frame's group); a frame whose levels
/// are more than [`ODD_GAP`] from its baseline keeps its own; and with
/// `own_midtone` every frame keeps its own midtone.
pub fn roll(frames: &[Analysis], o: &RollOptions) -> Vec<RollFrame> {
    let mut scene = vec![0usize; frames.len()];
    if o.scenes {
        let mut groups: Vec<Vec<usize>> = Vec::new();
        for (i, a) in frames.iter().enumerate() {
            let l = light(a);
            let found = groups.iter().position(|g| {
                let mid = |c: usize| {
                    let mut v: Vec<f64> = g.iter().map(|&j| light(&frames[j])[c]).collect();
                    v.sort_by(f64::total_cmp);
                    v[v.len() / 2]
                };
                (l[0] - mid(0)).abs().max((l[1] - mid(1)).abs()) <= SCENE_GAP
            });
            match found {
                Some(g) => groups[g].push(i),
                None => groups.push(vec![i]),
            }
        }
        for (g, members) in groups.iter().enumerate() {
            for &i in members {
                scene[i] = g;
            }
        }
    }
    let scenes = scene.iter().max().map_or(0, |m| m + 1);
    let mut base: Vec<BTreeMap<String, Value>> = Vec::new();
    let mut base_a: Vec<Analysis> = Vec::new();
    for g in 0..scenes {
        let members: Vec<Analysis> = (0..frames.len())
            .filter(|&i| scene[i] == g)
            .map(|i| frames[i])
            .collect();
        let chosen = o.baseline.filter(|&b| b < frames.len() && scene[b] == g);
        let pick = match chosen {
            Some(b) => vec![frames[b]],
            None => members,
        };
        base.push(frozen(&pick));
        // The baseline's levels, for telling odd frames.
        let get = |k: &str| base[g].get(k).and_then(Value::as_f64).unwrap_or(0.0);
        let mut a = pick[0];
        for (c, name) in ["Red", "Green", "Blue"].iter().enumerate() {
            a.density_low[c] = get(&format!("amb:NegativeDensityLow{name}"));
            a.density_high[c] = get(&format!("amb:NegativeDensityHigh{name}"));
        }
        base_a.push(a);
    }
    frames
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let b = &base_a[scene[i]];
            let gap = (0..3)
                .map(|c| {
                    (a.density_low[c] - b.density_low[c])
                        .abs()
                        .max((a.density_high[c] - b.density_high[c]).abs())
                })
                .fold(0.0, f64::max);
            let odd = o.odd && o.baseline != Some(i) && gap > ODD_GAP;
            let mut settings = if odd {
                frozen(std::slice::from_ref(a))
            } else {
                base[scene[i]].clone()
            };
            if o.own_midtone {
                settings.insert(
                    "amb:NegativeMidtone".into(),
                    Value::Number((a.midtone * 10_000.0).round() / 10_000.0),
                );
            }
            RollFrame {
                scene: scene[i],
                odd,
                settings,
            }
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
            crossover: NO_CROSSOVER,
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

    /// A frame of a light (red and blue against green at the highlights),
    /// exposed `stops` more than usual.
    fn shot(light: [f64; 2], stops: f64, midtone: f64) -> Analysis {
        let d = 0.18 * stops;
        Analysis {
            base: [0.8, 0.48, 0.32],
            density_low: [0.1 + d; 3],
            density_high: [1.5 + d + light[0], 1.5 + d, 1.5 + d + light[1]],
            midtone,
            crossover: NO_CROSSOVER,
        }
    }

    fn green_high(s: &BTreeMap<String, Value>) -> f64 {
        s["amb:NegativeDensityHighGreen"].as_f64().unwrap()
    }

    #[test]
    fn roll_baseline_groups_scenes_and_keeps_odd_frames() {
        let day = [0.0, 0.0];
        let tungsten = [0.25, -0.3];
        let frames = [
            shot(day, 0.0, 0.5),
            shot(day, 0.5, 0.45),
            shot(day, -0.5, 0.55),
            shot(tungsten, 0.0, 0.5),
            shot(tungsten, 0.3, 0.5),
            shot(day, 3.0, 0.4), // badly overexposed
        ];
        let r = roll(&frames, &RollOptions::default());
        assert_eq!(
            r.iter().map(|f| f.scene).collect::<Vec<_>>(),
            [0, 0, 0, 1, 1, 0]
        );
        assert_eq!(
            r.iter().map(|f| f.odd).collect::<Vec<_>>(),
            [false, false, false, false, false, true]
        );
        // The daylight frames share one conversion, the tungsten ones another.
        assert_eq!(r[0].settings, r[1].settings);
        assert_eq!(r[3].settings, r[4].settings);
        assert_ne!(r[0].settings, r[3].settings);
        // The odd frame keeps its own levels.
        assert!((green_high(&r[5].settings) - (1.5 + 0.54)).abs() < 1e-6);

        // Everything as one roll: one conversion for all.
        let flat = roll(
            &frames,
            &RollOptions {
                scenes: false,
                odd: false,
                ..Default::default()
            },
        );
        assert!(flat.windows(2).all(|w| w[0].settings == w[1].settings));

        // This frame as the baseline; each keeps its own midtone.
        let chosen = roll(
            &frames,
            &RollOptions {
                baseline: Some(1),
                own_midtone: true,
                ..Default::default()
            },
        );
        assert!((green_high(&chosen[0].settings) - (1.5 + 0.09)).abs() < 1e-6);
        assert_eq!(
            chosen[2].settings["amb:NegativeMidtone"].as_f64(),
            Some(0.55)
        );
        assert_eq!(chosen[0].settings["amb:NegativeAuto"], Value::Bool(false));
    }
}
