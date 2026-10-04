//! Closed loop: synthetic DNG (flb-testkit) → rawler → our pipeline → sRGB.
//! The ColorChecker target is built from sRGB values, so a neutral develop must
//! reproduce them; Exposure +1 must double linear values.

use flb_engine::{output, pipeline, raw};
use flb_testkit::{dng, patterns};

fn write_target(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("flb-engine-test-{}", std::process::id()));
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
