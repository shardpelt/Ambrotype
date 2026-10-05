//! Closed loop: synthetic DNG (amb-testkit) → rawler → our pipeline → sRGB.
//! The ColorChecker target is built from sRGB values, so a neutral develop must
//! reproduce them; Exposure +1 must double linear values.

use amb_engine::{output, pipeline, raw};
use amb_testkit::{dng, patterns};

fn write_target(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("amb-engine-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{name}.dng"));
    dng::write(
        &path,
        &patterns::generate(name).unwrap(),
        &Default::default(),
    )
    .unwrap();
    path
}

/// Centre of ColorChecker patch `i` in the synthetic target (see patterns::colorchecker).
fn patch_centre(i: usize) -> (usize, usize) {
    let (patch, gap) = (200, 24);
    (
        gap + (i % 6) * (patch + gap) + patch / 2,
        gap + (i / 6) * (patch + gap) + patch / 2,
    )
}

#[test]
fn neutral_develop_reproduces_colorchecker_srgb() {
    let path = write_target("colorchecker");
    let img = raw::load(&path).expect("decode synthetic DNG");
    assert_eq!((img.width, img.height), (1368, 920));
    for c in img.as_shot_neutral {
        assert!(
            (c - 1.0).abs() < 1e-3,
            "as-shot neutral {:?}",
            img.as_shot_neutral
        );
    }
    let linear = pipeline::DevelopSettings {
        base_look: pipeline::BaseLook::Linear,
        ..Default::default()
    };
    let developed = pipeline::render(&img, &linear);
    let srgb = output::to_srgb(&developed);
    let mut worst: f32 = 0.0;
    for (i, expected) in patterns::COLORCHECKER_SRGB8.iter().enumerate() {
        let (x, y) = patch_centre(i);
        let p = (y * developed.width + x) * 3;
        for c in 0..3 {
            let got = srgb[p + c] * 255.0;
            worst = worst.max((got - f32::from(expected[c])).abs());
        }
    }
    // 16-bit storage plus matrix round trips: allow about one 8-bit code value.
    assert!(worst < 1.5, "worst patch error {worst} code values");
}

#[test]
fn exposure_plus_one_doubles_linear_values() {
    let path = write_target("colorchecker");
    let img = raw::load(&path).unwrap();
    let linear = pipeline::DevelopSettings {
        base_look: pipeline::BaseLook::Linear,
        ..Default::default()
    };
    let base = pipeline::render(&img, &linear);
    let settings = pipeline::DevelopSettings {
        exposure_ev: 1.0,
        ..linear.clone()
    };
    let brighter = pipeline::render(&img, &settings);
    let (x, y) = patch_centre(21); // neutral 5 (mid grey)
    let p = (y * base.width + x) * 3;
    for c in 0..3 {
        let ratio = brighter.rgb[p + c] / base.rgb[p + c];
        assert!((ratio - 2.0).abs() < 1e-3, "channel {c}: ratio {ratio}");
    }
}

#[test]
fn as_shot_white_reads_as_d50() {
    let path = write_target("colorchecker");
    let img = raw::load(&path).unwrap();
    let (t, tint) = pipeline::as_shot_temp_tint(&img);
    // The synthetic camera's neutral is D50 (about 5003 K, slightly green).
    assert!((t - 5003.0).abs() < 20.0, "temperature {t}");
    assert!(tint > 0.0 && tint < 20.0, "tint {tint}");
}

#[test]
fn scanned_negative_converts_to_a_neutral_positive() {
    use std::collections::BTreeMap;
    let path = write_target("negative");
    let img = raw::load(&path).expect("decode synthetic negative");
    let settings: BTreeMap<String, amb_presets::Value> = [(
        "amb:NegativeConvert".to_string(),
        amb_presets::Value::Text("True".into()),
    )]
    .into_iter()
    .collect();
    let (develop, not_rendered) = pipeline::DevelopSettings::from_settings(&settings);
    assert!(not_rendered.is_empty(), "{not_rendered:?}");
    assert!(develop.negative.enabled && develop.negative.auto);
    let developed = pipeline::render(&img, &develop);
    let srgb = output::to_srgb(&developed);
    // The target puts a 64 px rebate above the ColorChecker.
    let at = |i: usize| {
        let (x, y) = patch_centre(i);
        let p = ((y + 64) * developed.width + x) * 3;
        [srgb[p], srgb[p + 1], srgb[p + 2]]
    };
    // Grey row (patches 18..23, white → black): neutral and getting darker.
    let mut last = f32::MAX;
    for i in 18..24 {
        let p = at(i);
        let mean = (p[0] + p[1] + p[2]) / 3.0;
        let spread =
            p.iter().cloned().fold(f32::MIN, f32::max) - p.iter().cloned().fold(f32::MAX, f32::min);
        assert!(spread < 0.04, "patch {i} not neutral: {p:?}");
        assert!(mean < last, "patch {i} not darker than the previous one");
        last = mean;
    }
    // Colour patches keep their hue: 12 blue, 14 red, 15 yellow (R and G above B).
    let red = at(14);
    assert!(
        red[0] > red[1] + 0.2 && red[0] > red[2] + 0.2,
        "red {red:?}"
    );
    let blue = at(12);
    assert!(blue[2] > blue[0] + 0.2, "blue {blue:?}");
    let yellow = at(15);
    assert!(
        yellow[0] > yellow[2] + 0.2 && yellow[1] > yellow[2] + 0.2,
        "yellow {yellow:?}"
    );
}
