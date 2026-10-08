// SPDX-License-Identifier: GPL-3.0-or-later
//! Random look: a new set of tone and colour settings each run, from a style.
//!
//! Each run starts from the seed the app passes, so it needs no clock or
//! randomness of its own. Everything is set with ordinary settings.

use amb_plugin_sdk::{Input, Output, Rng};

struct Style {
    contrast: (f64, f64),
    highlights: (f64, f64),
    shadows: (f64, f64),
    blacks: (f64, f64),
    vibrance: (f64, f64),
    saturation: (f64, f64),
    clarity: (f64, f64),
    /// Shadow tint hues to pick from (degrees), and the highlight tint is
    /// roughly opposite.
    shadow_hues: &'static [f64],
    tint: (f64, f64),
    vignette: (f64, f64),
    grain: (f64, f64),
}

const ANYTHING: Style = Style {
    contrast: (-30.0, 40.0),
    highlights: (-60.0, 10.0),
    shadows: (-20.0, 50.0),
    blacks: (-30.0, 30.0),
    vibrance: (-30.0, 40.0),
    saturation: (-30.0, 20.0),
    clarity: (-20.0, 30.0),
    shadow_hues: &[200.0, 220.0, 180.0, 260.0, 30.0, 140.0, 320.0],
    tint: (0.0, 30.0),
    vignette: (-35.0, 0.0),
    grain: (0.0, 0.0),
};
const FILM: Style = Style {
    contrast: (-10.0, 25.0),
    highlights: (-50.0, -15.0),
    shadows: (10.0, 40.0),
    blacks: (10.0, 40.0),
    vibrance: (-15.0, 15.0),
    saturation: (-25.0, 0.0),
    clarity: (-15.0, 5.0),
    shadow_hues: &[190.0, 210.0, 160.0],
    tint: (8.0, 25.0),
    vignette: (-25.0, -8.0),
    grain: (15.0, 40.0),
};
const MOODY: Style = Style {
    contrast: (15.0, 45.0),
    highlights: (-70.0, -30.0),
    shadows: (-30.0, 10.0),
    blacks: (-30.0, 0.0),
    vibrance: (-20.0, 10.0),
    saturation: (-40.0, -10.0),
    clarity: (5.0, 35.0),
    shadow_hues: &[200.0, 220.0, 240.0],
    tint: (10.0, 35.0),
    vignette: (-45.0, -20.0),
    grain: (0.0, 10.0),
};
const BRIGHT: Style = Style {
    contrast: (-10.0, 20.0),
    highlights: (-30.0, 0.0),
    shadows: (20.0, 60.0),
    blacks: (0.0, 25.0),
    vibrance: (10.0, 40.0),
    saturation: (0.0, 15.0),
    clarity: (-10.0, 15.0),
    shadow_hues: &[40.0, 30.0, 200.0],
    tint: (0.0, 15.0),
    vignette: (-10.0, 0.0),
    grain: (0.0, 0.0),
};
const MUTED: Style = Style {
    contrast: (-35.0, -5.0),
    highlights: (-40.0, -10.0),
    shadows: (10.0, 40.0),
    blacks: (15.0, 45.0),
    vibrance: (-40.0, -10.0),
    saturation: (-50.0, -20.0),
    clarity: (-20.0, 0.0),
    shadow_hues: &[180.0, 200.0, 30.0],
    tint: (5.0, 20.0),
    vignette: (-15.0, 0.0),
    grain: (0.0, 15.0),
};

fn pick(rng: &mut Rng, (lo, hi): (f64, f64), strength: f64) -> f64 {
    (rng.range(lo, hi) * strength).round()
}

pub fn run(input: Input) -> Result<Output, String> {
    let (name, style) = match input.choice("style") {
        "Film" => ("Film", FILM),
        "Moody" => ("Moody", MOODY),
        "Bright" => ("Bright", BRIGHT),
        "Muted" => ("Muted", MUTED),
        _ => ("Anything", ANYTHING),
    };
    let strength = input.number("strength") / 100.0;
    let mut rng = input.rng();
    let mut out = Output::new()
        .set("Contrast2012", pick(&mut rng, style.contrast, strength))
        .set("Highlights2012", pick(&mut rng, style.highlights, strength))
        .set("Shadows2012", pick(&mut rng, style.shadows, strength))
        .set("Blacks2012", pick(&mut rng, style.blacks, strength))
        .set("Clarity2012", pick(&mut rng, style.clarity, strength))
        .set(
            "PostCropVignetteAmount",
            pick(&mut rng, style.vignette, strength),
        )
        .set("GrainAmount", pick(&mut rng, style.grain, strength));
    let mut note = format!("{name} look");
    if input.flag("colour") {
        let hue = style.shadow_hues[(rng.next_u64() % style.shadow_hues.len() as u64) as usize];
        let shadow_hue = (hue + rng.range(-12.0, 12.0)).round().rem_euclid(360.0);
        let highlight_hue = (shadow_hue + 180.0 + rng.range(-25.0, 25.0))
            .round()
            .rem_euclid(360.0);
        out = out
            .set("Vibrance", pick(&mut rng, style.vibrance, strength))
            .set("Saturation", pick(&mut rng, style.saturation, strength))
            .set("ColorGradeShadowHue", shadow_hue)
            .set("ColorGradeShadowSat", pick(&mut rng, style.tint, strength))
            .set("ColorGradeHighlightHue", highlight_hue)
            .set(
                "ColorGradeHighlightSat",
                pick(&mut rng, style.tint, strength * 0.7),
            );
        note.push_str(&format!(
            ": shadows toward {shadow_hue}°, highlights toward {highlight_hue}°"
        ));
    }
    Ok(out.message(note))
}

amb_plugin_sdk::export_plugin!(run);

#[cfg(test)]
mod tests {
    use super::*;

    fn input(seed: u64, style: &str) -> Input {
        let mut i =
            Input::for_test(serde_json::json!({ "style": style, "strength": 60, "colour": true }));
        i.seed = seed;
        i
    }

    #[test]
    fn same_seed_same_look_new_seed_new_look() {
        let a = run(input(1, "Film")).unwrap();
        assert_eq!(a.commands, run(input(1, "Film")).unwrap().commands);
        assert_ne!(a.commands, run(input(2, "Film")).unwrap().commands);
        assert!(
            a.commands
                .iter()
                .any(|c| c["name"] == "GrainAmount" && c["value"].as_f64().unwrap() > 0.0)
        );
    }

    #[test]
    fn values_stay_in_range() {
        for seed in 0..200 {
            for style in ["Anything", "Film", "Moody", "Bright", "Muted"] {
                for c in run(input(seed, style)).unwrap().commands {
                    let v = c["value"].as_f64().unwrap();
                    let max = if c["name"].as_str().unwrap().ends_with("Hue") {
                        359.0
                    } else {
                        100.0
                    };
                    assert!((-100.0..=max).contains(&v), "{c}");
                }
            }
        }
    }
}
