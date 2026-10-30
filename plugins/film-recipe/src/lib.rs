// SPDX-License-Identifier: GPL-3.0-or-later
//! Film recipe: a film look plus the handful of picture settings a camera's
//! menu offers (grain, colour depth, white balance shift, highlight and
//! shadow tone, colour, sharpness, noise reduction, clarity), turned into
//! ordinary develop settings. Recipes people share for their cameras can be
//! typed in as they are (docs/ai/film-recipes.md has the mapping).
//!
//! The looks are ours, built from tone curve, HSL, colour grading and B&W
//! mix settings; they're not copies of any camera maker's processing.
//! Every setting the recipe touches is set outright, so switching recipes
//! replaces the last one instead of piling up.

use amb_plugin_sdk::{Input, Output};

struct Look {
    contrast: f64,
    saturation: f64,
    vibrance: f64,
    /// Parametric curve: shadows, darks, lights, highlights.
    curve: [f64; 4],
    /// (setting, value): HSL adjustments.
    hsl: &'static [(&'static str, f64)],
    /// (region, hue, saturation) colour grading.
    grade: &'static [(&'static str, f64, f64)],
    /// Black & white mix (Red … Magenta) for monochrome looks.
    mono: Option<[f64; 8]>,
    clarity: f64,
}

const COLOUR: Look = Look {
    contrast: 0.0,
    saturation: 0.0,
    vibrance: 0.0,
    curve: [0.0; 4],
    hsl: &[],
    grade: &[],
    mono: None,
    clarity: 0.0,
};

const MONO: [f64; 8] = [0.0; 8];

fn look(name: &str) -> Look {
    match name {
        "Vivid Slide" => Look {
            contrast: 20.0,
            saturation: 22.0,
            vibrance: 12.0,
            curve: [-8.0, -4.0, 4.0, 4.0],
            hsl: &[
                ("LuminanceAdjustmentBlue", -10.0),
                ("SaturationAdjustmentGreen", 10.0),
                ("LuminanceAdjustmentGreen", -6.0),
                ("SaturationAdjustmentRed", 6.0),
            ],
            ..COLOUR
        },
        "Soft Slide" => Look {
            contrast: -6.0,
            saturation: 8.0,
            curve: [6.0, 2.0, 0.0, -2.0],
            hsl: &[
                ("LuminanceAdjustmentOrange", 5.0),
                ("SaturationAdjustmentOrange", -4.0),
                ("SaturationAdjustmentBlue", 8.0),
            ],
            ..COLOUR
        },
        "Muted Chrome" => Look {
            contrast: 14.0,
            saturation: -18.0,
            curve: [-10.0, -4.0, 2.0, -4.0],
            hsl: &[
                ("HueAdjustmentBlue", -10.0),
                ("SaturationAdjustmentBlue", -14.0),
                ("LuminanceAdjustmentBlue", -6.0),
                ("HueAdjustmentGreen", 12.0),
                ("SaturationAdjustmentGreen", -24.0),
                ("SaturationAdjustmentRed", -8.0),
                ("SaturationAdjustmentOrange", -4.0),
            ],
            grade: &[("Shadow", 200.0, 6.0)],
            ..COLOUR
        },
        "Real Colour" => Look {
            contrast: 8.0,
            saturation: 3.0,
            curve: [-3.0, 0.0, 2.0, 0.0],
            ..COLOUR
        },
        "Studio Negative" => Look {
            contrast: 8.0,
            saturation: -2.0,
            curve: [-4.0, 0.0, 2.0, 0.0],
            hsl: &[
                ("LuminanceAdjustmentOrange", 4.0),
                ("SaturationAdjustmentOrange", -6.0),
            ],
            ..COLOUR
        },
        "Studio Negative Soft" => Look {
            contrast: -14.0,
            saturation: -6.0,
            curve: [6.0, 3.0, -2.0, -4.0],
            hsl: &[
                ("LuminanceAdjustmentOrange", 6.0),
                ("SaturationAdjustmentOrange", -8.0),
            ],
            ..COLOUR
        },
        "Street Negative" => Look {
            contrast: 24.0,
            saturation: -10.0,
            curve: [-6.0, -6.0, 6.0, -6.0],
            hsl: &[
                ("HueAdjustmentGreen", 22.0),
                ("SaturationAdjustmentGreen", -20.0),
                ("HueAdjustmentRed", 6.0),
                ("SaturationAdjustmentYellow", -12.0),
                ("HueAdjustmentYellow", -8.0),
            ],
            grade: &[("Shadow", 190.0, 14.0), ("Highlight", 35.0, 10.0)],
            ..COLOUR
        },
        "Amber Negative" => Look {
            contrast: -6.0,
            saturation: -4.0,
            curve: [8.0, 2.0, 2.0, -2.0],
            hsl: &[
                ("HueAdjustmentGreen", 10.0),
                ("SaturationAdjustmentBlue", -10.0),
            ],
            grade: &[("Highlight", 45.0, 18.0), ("Midtone", 40.0, 6.0)],
            ..COLOUR
        },
        "Cine Flat" => Look {
            contrast: -24.0,
            saturation: -22.0,
            curve: [10.0, 4.0, -4.0, -8.0],
            hsl: &[
                ("HueAdjustmentGreen", 8.0),
                ("SaturationAdjustmentOrange", -6.0),
            ],
            grade: &[("Shadow", 200.0, 8.0)],
            ..COLOUR
        },
        "Bleach Bypass" => Look {
            contrast: 34.0,
            saturation: -45.0,
            curve: [-8.0, -6.0, 6.0, 0.0],
            clarity: 10.0,
            ..COLOUR
        },
        "Mono" => Look {
            mono: Some(MONO),
            contrast: 6.0,
            ..COLOUR
        },
        "Mono Yellow" => Look {
            mono: Some([10.0, 10.0, 15.0, 0.0, -10.0, -25.0, -10.0, 0.0]),
            contrast: 8.0,
            ..COLOUR
        },
        "Mono Red" => Look {
            mono: Some([35.0, 25.0, 10.0, -15.0, -30.0, -45.0, -15.0, 15.0]),
            contrast: 12.0,
            ..COLOUR
        },
        "Mono Green" => Look {
            mono: Some([-20.0, -10.0, 10.0, 30.0, 10.0, -10.0, -15.0, -20.0]),
            contrast: 6.0,
            ..COLOUR
        },
        "Fine Mono" => Look {
            mono: Some([5.0, 10.0, 10.0, -5.0, -10.0, -15.0, -5.0, 0.0]),
            contrast: 16.0,
            curve: [-10.0, -4.0, 6.0, 4.0],
            clarity: 6.0,
            ..COLOUR
        },
        // Fine Mono through a coloured filter: the filter's mix on Fine Mono's tone.
        "Fine Mono Yellow" => Look {
            mono: Some([15.0, 15.0, 20.0, 0.0, -10.0, -25.0, -10.0, 0.0]),
            ..look("Fine Mono")
        },
        "Fine Mono Red" => Look {
            mono: Some([40.0, 30.0, 15.0, -15.0, -30.0, -45.0, -15.0, 15.0]),
            contrast: 20.0,
            ..look("Fine Mono")
        },
        "Fine Mono Green" => Look {
            mono: Some([-15.0, -5.0, 15.0, 30.0, 10.0, -10.0, -15.0, -20.0]),
            ..look("Fine Mono")
        },
        "Sepia" => Look {
            mono: Some(MONO),
            contrast: 4.0,
            grade: &[("Global", 38.0, 28.0)],
            ..COLOUR
        },
        // "Standard" and anything unknown.
        _ => Look {
            contrast: 5.0,
            saturation: 4.0,
            curve: [-2.0, 0.0, 2.0, 0.0],
            ..COLOUR
        },
    }
}

const BANDS: [&str; 8] = [
    "Red", "Orange", "Yellow", "Green", "Aqua", "Blue", "Purple", "Magenta",
];

/// Temperature (K) and Tint of the named white balances.
fn white_balance(name: &str) -> Option<(f64, f64)> {
    Some(match name {
        "Daylight" => (5500.0, 10.0),
        "Cloudy" => (6500.0, 10.0),
        "Shade" => (7500.0, 10.0),
        "Tungsten" => (2850.0, 0.0),
        "Fluorescent" => (3800.0, 21.0),
        // Daylight, warm white and cool white tubes.
        "Fluorescent 1" => (6300.0, 18.0),
        "Fluorescent 2" => (3000.0, 18.0),
        "Fluorescent 3" => (4100.0, 18.0),
        _ => return None,
    })
}

fn level(choice: &str) -> f64 {
    match choice {
        "Weak" => 1.0,
        "Strong" => 2.0,
        _ => 0.0,
    }
}

fn round(v: f64) -> f64 {
    v.round()
}

pub fn run(input: Input) -> Result<Output, String> {
    let name = input.choice("look").to_string();
    let lk = look(&name);
    let mut out = Output::new()
        .reset_group("hsl")
        .reset_group("bw_mixer")
        .reset_group("color_grading")
        .reset_group("split_toning");
    let mut hsl: Vec<(String, f64)> = lk.hsl.iter().map(|(k, v)| (k.to_string(), *v)).collect();
    let mut add_hsl = |key: &str, v: f64| match hsl.iter_mut().find(|(k, _)| k == key) {
        Some((_, x)) => *x += v,
        None => hsl.push((key.to_string(), v)),
    };

    // Colour depth: saturated colours deeper (darker, a touch richer); blue depth for skies.
    let depth = level(input.choice("colour_depth"));
    for (band, dl) in [
        ("Red", -6.0),
        ("Orange", -3.0),
        ("Yellow", -6.0),
        ("Green", -6.0),
    ] {
        if depth > 0.0 {
            add_hsl(&format!("LuminanceAdjustment{band}"), dl * depth);
        }
    }
    if depth > 0.0 {
        add_hsl("SaturationAdjustmentRed", 3.0 * depth);
        add_hsl("SaturationAdjustmentGreen", 3.0 * depth);
    }
    let blue = level(input.choice("blue_depth"));
    if blue > 0.0 {
        add_hsl("LuminanceAdjustmentBlue", -8.0 * blue);
        add_hsl("LuminanceAdjustmentAqua", -4.0 * blue);
        add_hsl("SaturationAdjustmentBlue", 4.0 * blue);
    }

    // Tone: dynamic range protects highlights; highlight / shadow tone bend the curve
    // (+ is harder: brighter highlights, deeper shadows), half steps allowed.
    // Dynamic range priority sets the range and both tones itself, softer
    // than DR400 alone (ours).
    let priority = match input.choice("dr_priority") {
        "Weak" | "Auto" => Some((-45.0, -10.0, 10.0, -1.0, -1.0)),
        "Strong" => Some((-60.0, -15.0, 20.0, -2.0, -2.0)),
        _ => None,
    };
    let (dr_highlights, dr_whites, dr_shadows) = match (priority, input.choice("dynamic_range")) {
        (Some((hl, wh, sh, _, _)), _) => (hl, wh, sh),
        (None, "DR200") => (-20.0, -5.0, 0.0),
        (None, "DR400") => (-40.0, -10.0, 5.0),
        _ => (0.0, 0.0, 0.0),
    };
    let (h, s) = match priority {
        Some((_, _, _, h, s)) => (h, s),
        None => (input.number("highlight"), input.number("shadow")),
    };
    let [cs, cd, cl, ch] = lk.curve;
    let colour = input.number("colour");
    out = out
        .set("Contrast2012", round(lk.contrast))
        .set("Highlights2012", round(dr_highlights))
        .set("Whites2012", round(dr_whites))
        .set("Shadows2012", round(dr_shadows))
        .set("ParametricHighlights", round(ch + 8.0 * h))
        .set("ParametricLights", round(cl + 3.0 * h))
        .set("ParametricDarks", round(cd - 3.0 * s))
        .set("ParametricShadows", round(cs - 8.0 * s))
        .set(
            "Clarity2012",
            round(lk.clarity + 6.0 * input.number("clarity")),
        )
        .set("Sharpness", round(40.0 + 10.0 * input.number("sharpness")))
        .set(
            "LuminanceSmoothing",
            round((10.0 + 2.5 * input.number("noise_reduction")).max(0.0)),
        );

    // Grain.
    let grain = level(input.choice("grain"));
    out = out
        .set("GrainAmount", [0.0, 25.0, 45.0][grain as usize])
        .set(
            "GrainSize",
            if input.choice("grain_size") == "Large" {
                35.0
            } else {
                15.0
            },
        )
        .set("GrainFrequency", 50.0);

    if let Some(mix) = lk.mono {
        // Monochrome: the B&W mix, and toning (warm–cool, magenta–green) as a tint.
        out = out.set("ConvertToGrayscale", true);
        for (band, v) in BANDS.iter().zip(mix) {
            if v != 0.0 {
                out = out.set(&format!("GrayMixer{band}"), v);
            }
        }
        out = out.set("Saturation", 0.0).set("Vibrance", 0.0);
        let (wc, mg) = (input.number("tone_warm"), input.number("tone_magenta"));
        if wc != 0.0 || mg != 0.0 {
            // Warm at 40°, magenta at 320°: add them as vectors.
            let (a, b) = (40f64.to_radians(), 320f64.to_radians());
            let (x, y) = (wc * a.cos() + mg * b.cos(), wc * a.sin() + mg * b.sin());
            let hue = y.atan2(x).to_degrees().rem_euclid(360.0).round() % 360.0;
            out = out
                .set("ColorGradeGlobalHue", hue)
                .set("ColorGradeGlobalSat", round(x.hypot(y) * 3.0).min(100.0));
        }
    } else {
        out = out
            .set("ConvertToGrayscale", false)
            .set(
                "Saturation",
                round(lk.saturation + 6.0 * colour).clamp(-100.0, 100.0),
            )
            .set("Vibrance", round(lk.vibrance + 2.0 * colour));
    }
    for (region, hue, sat) in lk.grade {
        out = out
            .set(&format!("ColorGrade{region}Hue"), *hue)
            .set(&format!("ColorGrade{region}Sat"), *sat);
    }
    for (k, v) in hsl {
        out = out.set(&k, round(v).clamp(-100.0, 100.0));
    }

    // White balance, and the shift on a red (+) / blue (+) grid as its own
    // settings, so it moves whatever white balance the photo gets (As Shot too).
    let (r, b) = (input.number("wb_red"), input.number("wb_blue"));
    let wb = input.choice("white_balance");
    let base = match wb {
        "Kelvin" => Some((input.number("kelvin"), 0.0)),
        other => white_balance(other),
    };
    let mut note = format!("{name} look");
    match base {
        Some((t, tint)) => {
            out = out
                .set("WhiteBalance", "Custom")
                .set("Temperature", t)
                .set("Tint", tint);
            note.push_str(&format!(", white balance {t} K"));
        }
        None => {
            out = out
                .set("WhiteBalance", "As Shot")
                .reset("Temperature")
                .reset("Tint");
        }
    }
    out = out.set("amb:WBShiftRed", r).set("amb:WBShiftBlue", b);
    if r != 0.0 || b != 0.0 {
        note.push_str(&format!(", shift red {r:+}, blue {b:+}"));
    }
    Ok(out.message(note))
}

amb_plugin_sdk::export_plugin!(run);

#[cfg(test)]
mod tests {
    use super::*;
    use amb_plugin_sdk::Photo;

    fn input(params: serde_json::Value) -> Input {
        let mut defaults = serde_json::json!({
            "look": "Standard", "grain": "Off", "grain_size": "Small", "colour_depth": "Off", "blue_depth": "Off",
            "white_balance": "As shot", "kelvin": 5500, "wb_red": 0, "wb_blue": 0, "dynamic_range": "DR100", "dr_priority": "Off",
            "highlight": 0, "shadow": 0, "colour": 0, "sharpness": 0, "noise_reduction": 0, "clarity": 0,
            "tone_warm": 0, "tone_magenta": 0
        });
        for (k, v) in params.as_object().unwrap() {
            defaults[k] = v.clone();
        }
        let mut i = Input::for_test(defaults);
        i.photo = Some(Photo {
            as_shot_temperature: Some(5000.0),
            as_shot_tint: Some(4.0),
            ..Photo::default()
        });
        i
    }

    fn value(out: &Output, name: &str) -> Option<serde_json::Value> {
        out.commands
            .iter()
            .rev()
            .find(|c| c["name"] == name)
            .map(|c| c["value"].clone())
    }

    #[test]
    fn a_recipe_becomes_settings() {
        let out = run(input(serde_json::json!({
            "look": "Muted Chrome", "grain": "Weak", "colour_depth": "Strong", "highlight": -1, "shadow": 1.5,
            "colour": -2, "white_balance": "Daylight", "wb_red": 2, "wb_blue": -4
        })))
        .unwrap();
        assert_eq!(value(&out, "GrainAmount"), Some(25.0.into()));
        assert_eq!(value(&out, "Saturation"), Some((-30.0).into()));
        assert_eq!(value(&out, "ParametricShadows"), Some((-22.0).into()));
        // Daylight, shifted red +2 / blue -4 by the engine.
        assert_eq!(value(&out, "Temperature"), Some(5500.0.into()));
        assert_eq!(value(&out, "amb:WBShiftRed"), Some(2.0.into()));
        assert_eq!(value(&out, "amb:WBShiftBlue"), Some((-4.0).into()));
        // Colour depth deepens reds on top of the look's own HSL.
        assert_eq!(value(&out, "LuminanceAdjustmentRed"), Some((-12.0).into()));
        assert!(out.message.unwrap().starts_with("Muted Chrome look"));
    }

    #[test]
    fn as_shot_shifts_from_the_camera_and_mono_tones() {
        let out = run(input(serde_json::json!({ "wb_red": 1, "wb_blue": 1 }))).unwrap();
        assert_eq!(value(&out, "WhiteBalance"), Some("As Shot".into()));
        assert_eq!(value(&out, "amb:WBShiftBlue"), Some(1.0.into()));
        let strong = run(input(
            serde_json::json!({ "dr_priority": "Strong", "highlight": 3 }),
        ))
        .unwrap();
        assert_eq!(value(&strong, "Highlights2012"), Some((-60.0).into()));
        let plain = run(input(serde_json::json!({}))).unwrap();
        assert_eq!(value(&plain, "WhiteBalance"), Some("As Shot".into()));

        let mono = run(input(
            serde_json::json!({ "look": "Mono Red", "tone_warm": 3 }),
        ))
        .unwrap();
        assert_eq!(value(&mono, "ConvertToGrayscale"), Some(true.into()));
        assert_eq!(value(&mono, "GrayMixerRed"), Some(35.0.into()));
        assert_eq!(value(&mono, "ColorGradeGlobalHue"), Some(40.0.into()));
        // Every look gives values in range.
        for name in [
            "Standard",
            "Vivid Slide",
            "Soft Slide",
            "Muted Chrome",
            "Real Colour",
            "Studio Negative",
            "Studio Negative Soft",
            "Street Negative",
            "Amber Negative",
            "Cine Flat",
            "Bleach Bypass",
            "Mono",
            "Mono Yellow",
            "Mono Red",
            "Mono Green",
            "Fine Mono",
            "Fine Mono Yellow",
            "Fine Mono Red",
            "Fine Mono Green",
            "Sepia",
        ] {
            for extreme in [-4, 4] {
                let o = run(input(serde_json::json!({ "look": name, "colour": extreme, "highlight": extreme, "shadow": extreme, "clarity": extreme }))).unwrap();
                for c in &o.commands {
                    if let Some(v) = c["value"].as_f64() {
                        let range = if c["name"].as_str().unwrap().ends_with("Hue") {
                            0.0..=359.0
                        } else {
                            -150.0..=50000.0
                        };
                        assert!(range.contains(&v), "{name}: {c}");
                    }
                }
            }
        }
    }
}
