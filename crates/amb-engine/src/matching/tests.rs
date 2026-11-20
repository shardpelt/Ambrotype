use super::auto::auto;
use super::solve::{num, solve};
use super::stats::{LookStats, average, distance, look_stats};
use super::*;
use crate::pipeline::{self};
use crate::raw::CameraImage;

fn photo(w: usize, h: usize) -> CameraImage {
    let mut rgb = Vec::with_capacity(w * h * 3);
    for y in 0..h {
        for x in 0..w {
            let (u, v) = (x as f32 / w as f32, y as f32 / h as f32);
            let mut p = [
                0.02 + 0.5 * u * u,
                0.03 + 0.4 * v,
                0.04 + 0.3 * (1.0 - u) * v,
            ];
            if (x / 6 + y / 6) % 3 == 0 {
                p = [p[2] * 1.4, p[0] * 0.6, p[1]];
            }
            rgb.extend_from_slice(&p);
        }
    }
    CameraImage {
        width: w,
        height: h,
        rgb,
        xyz_to_cam: [
            [0.7186, -0.2108, -0.0606],
            [-0.4386, 1.2001, 0.2584],
            [-0.0577, 0.1271, 0.6585],
        ],
        matrices: Vec::new(),
        highlights_rebuilt: false,
        as_shot_neutral: [0.45, 1.0, 0.62],
        clip: [1.0, 1.0, 1.0],
        rendered: false,
        make: "Test".into(),
        model: "Synthetic".into(),
        shot: Default::default(),
        metadata: Default::default(),
    }
}

fn settings(tags: &[(&str, f64)]) -> Settings {
    tags.iter()
        .map(|(k, v)| (k.to_string(), Value::Number(*v)))
        .collect()
}

#[test]
fn stats_of_the_same_image_are_the_same_distance_zero() {
    let img = photo(48, 32);
    let s = look_stats(&pipeline::render(&img, &develop_plain(&Settings::new())));
    assert_eq!(distance(&s, &s), 0.0);
    assert_eq!(average(std::slice::from_ref(&s)).unwrap(), s);
    assert!(s.lightness.windows(2).all(|w| w[0] <= w[1]), "{s:?}");
}

#[test]
fn finds_a_known_edit_again() {
    let img = photo(48, 32);
    // The reference: the same photo, warmer, brighter, flatter, less saturated.
    let mut edit = settings(&[
        ("Exposure2012", 0.6),
        ("Contrast2012", -30.0),
        ("Saturation", -25.0),
        ("Tint", 0.0),
    ]);
    let (t0, _) = pipeline::as_shot_temp_tint(&img);
    edit.insert("WhiteBalance".into(), Value::Text("Custom".into()));
    edit.insert("Temperature".into(), Value::Number((t0 + 900.0).round()));
    let target = look_stats(&pipeline::render(&img, &develop_plain(&edit)));
    let opts = MatchOptions {
        parts: vec![Part::WhiteBalance, Part::Tone, Part::Color],
        max_evaluations: 400,
        ..Default::default()
    };
    let r = solve(&img, &Settings::new(), &target, &opts, &develop_plain);
    assert!(r.after < r.before / 20.0, "{r:?}");
    assert!(r.after < 2.0, "{r:?}");
    let got: Settings = r.changes.iter().cloned().collect();
    let n = |k: &str| num(&got, k).unwrap_or(0.0);
    assert!(n("Temperature") > t0 + 300.0, "{r:?}");
    assert!(n("Exposure2012") > 0.2, "{r:?}");
    assert!(n("Saturation") + n("Vibrance") < -5.0, "{r:?}");
    assert!(r.evaluations <= 401, "{r:?}");
}

#[test]
fn the_color_mixer_moves_one_band() {
    let img = photo(48, 32);
    let edit = settings(&[("SaturationAdjustmentRed", -70.0)]);
    let target = look_stats(&pipeline::render(&img, &develop_plain(&edit)));
    let opts = MatchOptions {
        parts: vec![Part::Hsl],
        max_evaluations: 300,
        ..Default::default()
    };
    let r = solve(&img, &Settings::new(), &target, &opts, &develop_plain);
    assert!(r.after < r.before / 4.0, "{r:?}");
    let got: Settings = r.changes.iter().cloned().collect();
    assert!(
        num(&got, "SaturationAdjustmentRed").unwrap_or(0.0) < -25.0,
        "{r:?}"
    );
}

#[test]
fn auto_brightens_a_dark_photo() {
    let mut img = photo(48, 32);
    img.rgb.iter_mut().for_each(|v| *v *= 0.15);
    let r = auto(&img, &Settings::new(), true, false, &develop_plain);
    let got: Settings = r.changes.iter().cloned().collect();
    assert!(num(&got, "Exposure2012").unwrap_or(0.0) > 0.8, "{r:?}");
    let after = look_stats(&pipeline::render(&img, &develop_plain(&got)));
    assert!(after.lightness[4] > 30.0, "{after:?}");
    assert!(!got.contains_key("WhiteBalance"), "{r:?}");
    // Deterministic, and the same whatever the tone sliders were.
    let again = auto(&img, &Settings::new(), true, false, &develop_plain);
    assert_eq!(again.changes, r.changes);
    let edited = settings(&[("Exposure2012", -1.0), ("Contrast2012", 50.0)]);
    let from_edit = auto(&img, &edited, true, false, &develop_plain);
    let a: Settings = from_edit.changes.iter().cloned().collect();
    assert_eq!(num(&a, "Exposure2012"), num(&got, "Exposure2012"));
    assert_eq!(
        num(&a, "Contrast2012").unwrap_or(0.0),
        num(&got, "Contrast2012").unwrap_or(0.0)
    );
}

#[test]
fn auto_white_balance_neutralises_greys() {
    // Mostly grey wall, lit warmer than the camera's white: red up.
    let mut img = photo(48, 32);
    let n = img.as_shot_neutral.map(|v| v as f32);
    for (i, px) in img.rgb.chunks_exact_mut(3).enumerate() {
        if i % 3 != 0 {
            let v = 0.25 + 0.2 * ((i % 48) as f32 / 48.0);
            px.copy_from_slice(&[v * n[0] * 1.3, v * n[1], v * n[2] * 0.85]);
        }
    }
    let before = look_stats(&pipeline::render(&img, &develop_plain(&Settings::new())));
    assert!(before.neutral[2] > 0.1, "{before:?}");
    let r = auto(&img, &Settings::new(), false, true, &develop_plain);
    assert_eq!(
        r.changes.last().unwrap(),
        &("WhiteBalance".to_string(), Value::Text("Auto".into()))
    );
    let got: Settings = r.changes.iter().cloned().collect();
    let after = look_stats(&pipeline::render(&img, &develop_plain(&got)));
    let cast = |s: &LookStats| s.neutral[0].hypot(s.neutral[1]);
    assert!(cast(&after) < cast(&before) / 1.8, "{before:?} {after:?}");
    assert!(
        got.contains_key("Temperature") && got.contains_key("Tint"),
        "{r:?}"
    );
}

#[test]
fn grading_tints_the_shadows() {
    let img = photo(48, 32);
    let edit = settings(&[
        ("ColorGradeShadowHue", 220.0),
        ("ColorGradeShadowSat", 40.0),
    ]);
    let target = look_stats(&pipeline::render(&img, &develop_plain(&edit)));
    let opts = MatchOptions {
        parts: vec![Part::Grading],
        max_evaluations: 200,
        ..Default::default()
    };
    let r = solve(&img, &Settings::new(), &target, &opts, &develop_plain);
    assert!(r.after < r.before / 5.0, "{r:?}");
    let got: Settings = r.changes.iter().cloned().collect();
    let hue = num(&got, "ColorGradeShadowHue").unwrap_or(0.0);
    assert!((hue - 220.0).abs() < 40.0, "{r:?}");
}

#[test]
fn finds_clarity_again() {
    // At the matcher's size, where Clarity's and Texture's scales differ.
    let img = photo(256, 171);
    let edit = settings(&[("Clarity2012", 50.0)]);
    let target = look_stats(&pipeline::render(&img, &develop_plain(&edit)));
    let opts = MatchOptions {
        parts: vec![Part::Detail],
        max_evaluations: 150,
        ..Default::default()
    };
    let r = solve(&img, &Settings::new(), &target, &opts, &develop_plain);
    assert!(r.after < r.before / 5.0, "{r:?}");
    let got: Settings = r.changes.iter().cloned().collect();
    let clarity = num(&got, "Clarity2012").unwrap_or(0.0);
    assert!((clarity - 50.0).abs() <= 15.0, "{r:?}");
}

#[test]
fn finds_a_parametric_curve_again() {
    let img = photo(48, 32);
    let edit = settings(&[("ParametricShadows", 40.0)]);
    let target = look_stats(&pipeline::render(&img, &develop_plain(&edit)));
    let opts = MatchOptions {
        parts: vec![Part::Curve],
        max_evaluations: 200,
        ..Default::default()
    };
    let r = solve(&img, &Settings::new(), &target, &opts, &develop_plain);
    assert!(r.after < r.before / 5.0, "{r:?}");
    let got: Settings = r.changes.iter().cloned().collect();
    let n = |k: &str| num(&got, k).unwrap_or(0.0);
    assert!(
        n("ParametricShadows").max(n("ParametricDarks")) > 10.0,
        "{r:?}"
    );
}

#[test]
fn detail_is_higher_for_a_sharper_photo() {
    let sharp = pipeline::render(&photo(256, 171), &develop_plain(&Settings::new()));
    let (w, h) = (sharp.width, sharp.height);
    let channels: Vec<Vec<f32>> = (0..3)
        .map(|c| {
            let plane: Vec<f32> = sharp.rgb.iter().skip(c).step_by(3).copied().collect();
            crate::filters::gaussian_blur(&plane, w, h, 1.5)
        })
        .collect();
    let mut soft = sharp.clone();
    for (i, px) in soft.rgb.chunks_exact_mut(3).enumerate() {
        for (c, v) in px.iter_mut().enumerate() {
            *v = channels[c][i];
        }
    }
    let (a, b) = (look_stats(&sharp), look_stats(&soft));
    assert!(
        a.detail[0] > b.detail[0] && a.detail[1] > b.detail[1],
        "{a:?} {b:?}"
    );
    // The medium scale sees more of the picture's structure than the fine one.
    assert!(a.detail[1] > a.detail[0], "{a:?}");
}

#[test]
fn focus_parses_and_picks_parts() {
    assert_eq!(Focus::parse("all"), Some(Focus::All));
    assert_eq!(Focus::parse("tone"), Some(Focus::Tone));
    assert_eq!(Focus::parse("colour"), Some(Focus::Colour));
    assert_eq!(Focus::parse("color"), Some(Focus::Colour));
    assert_eq!(Focus::parse("detail"), None);
    assert_eq!(Focus::All.parts(), Part::ALL.to_vec());
    assert_eq!(
        Focus::Tone.parts(),
        vec![Part::Tone, Part::Curve, Part::Detail]
    );
    assert_eq!(
        Focus::Colour.parts(),
        vec![Part::WhiteBalance, Part::Color, Part::Grading, Part::Hsl]
    );
    assert_eq!(Focus::All.weights(), Weights::default());
    let tone = Focus::Tone.weights();
    assert_eq!((tone.tone, tone.detail), (1.0, 1.0));
    assert_eq!((tone.cast, tone.colour, tone.neutral), (0.0, 0.0, 0.0));
    let colour = Focus::Colour.weights();
    assert_eq!((colour.tone, colour.detail), (0.0, 0.0));
    assert_eq!(
        (colour.cast, colour.colour, colour.neutral),
        (1.0, 1.0, 1.0)
    );
    assert_eq!(serde_json::to_string(&Focus::Colour).unwrap(), "\"colour\"");
    assert_eq!(Part::parse("tone_curve"), Some(Part::Curve));
    assert_eq!(Part::parse("clarity"), Some(Part::Detail));
    assert_eq!(Part::parse("texture"), Some(Part::Detail));
}

#[test]
fn look_stats_round_trip_through_json() {
    let s = look_stats(&pipeline::render(
        &photo(48, 32),
        &develop_plain(&Settings::new()),
    ));
    let text = serde_json::to_string(&s).unwrap();
    assert_eq!(serde_json::from_str::<LookStats>(&text).unwrap(), s);
    // Saved before local contrast was measured: it reads as none.
    let mut old = serde_json::to_value(&s).unwrap();
    old.as_object_mut().unwrap().remove("detail");
    let read: LookStats = serde_json::from_value(old).unwrap();
    assert_eq!(read.detail, [0.0; 2]);
    assert_eq!(read.lightness, s.lightness);
}
