// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

#[test]
fn temperature_alone_keeps_the_as_shot_tint() {
    let mut img = busy(64, 48);
    // A camera white with a clear tint, so 0 would show.
    img.as_shot_neutral = [0.5, 1.0, 0.62];
    let (temperature, tint) = as_shot_temp_tint(&img);
    assert!(tint.abs() > 3.0, "{tint}");
    let as_shot = render(&img, &DevelopSettings::default());
    let custom = |tint| DevelopSettings {
        white_balance: WhiteBalance::Custom { temperature, tint },
        ..Default::default()
    };
    let max_diff = |a: &Image, b: &Image| {
        a.rgb
            .iter()
            .zip(&b.rgb)
            .map(|(x, y)| (x - y).abs())
            .fold(0.0, f32::max)
    };
    assert!(max_diff(&render(&img, &custom(None)), &as_shot) < 2e-3);
    assert!(max_diff(&render(&img, &custom(Some(0.0))), &as_shot) > 1e-2);
}

#[test]
fn a_white_balance_shift_moves_as_shot_too() {
    let img = busy(64, 48);
    let mean = |s: &DevelopSettings| {
        let out = render(&img, s);
        let n = (out.rgb.len() / 3) as f32;
        let sum = |c: usize| out.rgb.iter().skip(c).step_by(3).sum::<f32>() / n;
        (sum(0), sum(2))
    };
    let (r0, b0) = mean(&DevelopSettings::default());
    // Red minus blue warms.
    let (r1, b1) = mean(&DevelopSettings {
        wb_shift: [2.0, -4.0],
        ..Default::default()
    });
    assert!(r1 > r0 && b1 < b0, "{r0} {b0} -> {r1} {b1}");
    // On the white (x, y): 4 mireds a step and 3 Tint a step.
    let (x, y) = temp_tint_to_xy(5000.0, 0.0);
    let t = xy_to_temp_tint(
        wb_shifted(x, y, [1.0, 1.0]).0,
        wb_shifted(x, y, [1.0, 1.0]).1,
    );
    assert!(
        (t.temperature - 5000.0).abs() < 5.0 && (t.tint - 6.0).abs() < 0.2,
        "{t:?}"
    );
}

#[test]
fn relative_white_balance_on_rendered_images() {
    let mut img = camera(1, 1, |_, _| [0.5, 0.5, 0.5]);
    img.xyz_to_cam = invert3(&amb_color::matrix::SRGB_TO_XYZ_D50);
    img.rendered = true;
    let none = IncrementalWhiteBalance::default();
    assert_eq!(incremental_neutral(&img, none), [1.0; 3]);
    // + is warmer: the white is assumed bluer, so blue's neutral rises.
    let warm = incremental_neutral(
        &img,
        IncrementalWhiteBalance {
            temperature: 40.0,
            tint: 0.0,
        },
    );
    assert!(warm[2] > 1.05 && warm[0] < 0.97, "{warm:?}");
    // + tint is magenta: green is assumed stronger in the light.
    let magenta = incremental_neutral(
        &img,
        IncrementalWhiteBalance {
            temperature: 0.0,
            tint: 30.0,
        },
    );
    assert!(magenta[0] < 1.0 && magenta[2] < 1.0, "{magenta:?}");
    // The picker's inverse gives back the same setting.
    let wb = IncrementalWhiteBalance {
        temperature: -25.0,
        tint: 12.0,
    };
    let (x, y) = xy_for_neutral(&img.xyz_to_cam, incremental_neutral(&img, wb));
    let back = incremental_for_xy(&img, x, y);
    assert!(
        (back.temperature - wb.temperature).abs() < 0.5 && (back.tint - wb.tint).abs() < 0.5,
        "{back:?}"
    );
    // A rendered image ignores Kelvin and gets no base tone curve.
    let mut s = DevelopSettings {
        white_balance: WhiteBalance::Custom {
            temperature: 3000.0,
            tint: Some(0.0),
        },
        ..Default::default()
    };
    let out = render(&img, &s);
    s.white_balance = WhiteBalance::AsShot;
    assert_eq!(out.rgb, render(&img, &s).rgb);
    let v = crate::output::to_srgb(&out);
    assert!((v[0] - 0.7354).abs() < 0.01, "{v:?}");
}

#[test]
fn clipped_highlights_go_neutral() {
    let neutral = [0.5, 1.0, 0.6];
    let u = Unclip::new([1.0, 0.98, 0.95], neutral, false);
    let apply = |c: [f32; 3]| u.apply(c, u.weights(&c));
    // Every channel clipped: the result is the neutral, at the brightest
    // white-balanced level (here red: 1 / 0.5 = 2).
    let c = apply([1.0, 0.98, 0.95]);
    for (v, n) in c.iter().zip(neutral) {
        assert!((v / n as f32 - 2.0).abs() < 1e-5, "{c:?}");
    }
    // Only green clipped: it rises to match the brightest channel.
    let c = apply([0.7, 0.98, 0.3]);
    assert_eq!((c[0], c[2]), (0.7, 0.3));
    assert!((c[1] - 1.4).abs() < 1e-5, "{c:?}");
    // Nothing near clipping: untouched.
    assert_eq!(apply([0.5, 0.8, 0.4]), [0.5, 0.8, 0.4]);

    // Next to a clipped pixel, the edge pixel counts as clipped too.
    let rgb = [
        [0.6, 0.9, 0.5],
        [1.0, 0.98, 0.95],
        [0.2, 0.3, 0.2],
        [0.2, 0.3, 0.2],
    ]
    .concat();
    let mut w = Vec::new();
    u.row_weights(&rgb, 4, 1, 0, 1, &mut w);
    assert_eq!(w[0], [1.0; 3]);
    assert_eq!(w[3], [0.0; 3]);
}

#[test]
fn clip_levels_find_the_pile_up() {
    // A smooth ramp in every channel, plus a blown area where red and
    // green pile up at 1.0 and blue a little lower.
    let mut rgb = Vec::new();
    for i in 0..20_000 {
        let v = i as f32 / 20_000.0 * 0.9;
        rgb.extend_from_slice(&[v, v, v]);
    }
    for _ in 0..2_000 {
        rgb.extend_from_slice(&[1.0, 1.0, 0.95]);
    }
    let k = crate::raw::clip_levels(&rgb);
    assert!(
        (k[0] - 1.0).abs() < 0.01 && (k[1] - 1.0).abs() < 0.01,
        "{k:?}"
    );
    assert!((k[2] - 0.95).abs() < 0.01, "{k:?}");
    // No pile-up: no clip.
    let ramp: Vec<f32> = rgb[..60_000].to_vec();
    assert_eq!(crate::raw::clip_levels(&ramp), [f32::INFINITY; 3]);
}

#[test]
fn camera_profile_colours_and_tone_curve() {
    use crate::profile::{Dcp, tests::sample};
    // The profile's curve lifts scene 0.18 to display 0.5 and stands in
    // for our base look; a grey stays grey.
    let dcp = Dcp::parse(&sample(vec![(
        50940,
        11,
        vec![0.0, 0.0, 0.18, 0.5, 1.0, 1.0],
        None,
    )]))
    .unwrap();
    let img = camera(8, 8, |_, _| [0.18; 3]);
    let mut s = linear(&[("CameraProfile", "Test Standard")]);
    s.base_look = BaseLook::Default;
    assert!(s.profile_note().unwrap().contains("Test Standard"));
    s.camera_profile = Some(std::sync::Arc::new(dcp));
    assert!(s.profile_note().is_none());
    let out = render(&img, &s);
    let p = &out.rgb[..3];
    assert!(p.iter().all(|v| (v - 0.5).abs() < 0.01), "{p:?}");
    // Without it, our base look (scene 0.18 → about 0.27).
    s.camera_profile = None;
    let ours = render(&img, &s).rgb[0];
    assert!((ours - 0.5).abs() > 0.1, "{ours}");
}

#[test]
fn brightening_exposure_goes_before_the_look_table() {
    use crate::profile::{Dcp, tests::sample};
    // A look table that darkens bright values: value scale 1 at V = 0,
    // 0.5 at V = 1.
    let rows = |scale: f64| (0..4).flat_map(move |_| [0.0, 1.0, scale]);
    let table: Vec<f64> = rows(1.0).chain(rows(0.5)).collect();
    let dcp = Dcp::parse(&sample(vec![
        (50981, 4, vec![2.0, 2.0, 2.0], None),
        (50982, 11, table, None),
    ]))
    .unwrap();
    let img = camera(4, 4, |_, _| [0.1, 0.05, 0.02]);
    let mut s = linear(&[("CameraProfile", "Test Standard")]);
    s.camera_profile = Some(std::sync::Arc::new(dcp));
    let at = |s: &mut DevelopSettings, ev: f64| {
        s.exposure_ev = ev;
        render(&img, s).rgb[0]
    };
    let base = at(&mut s, 0.0);
    // +1 EV: the look table sees the brighter value and darkens it more.
    let up = at(&mut s, 1.0);
    assert!(up < 2.0 * base * 0.99, "{up} vs {base}");
    // −1 EV comes after the table: exactly half.
    let down = at(&mut s, -1.0);
    assert!((down - 0.5 * base).abs() < 1e-5, "{down} vs {base}");
}

#[test]
fn neutral_maps_to_prophoto_white() {
    // A plausible camera matrix (XYZ → camera) for a generic Bayer sensor.
    let xyz_to_cam = [[0.7, 0.1, -0.1], [-0.4, 1.2, 0.2], [-0.05, 0.15, 0.6]];
    let neutral = neutral_for_xy(&xyz_to_cam, 0.3127, 0.3290);
    let m = camera_to_prophoto(&xyz_to_cam, neutral);
    let w = apply3(&m, neutral);
    for c in w {
        assert!((c - 1.0).abs() < 1e-9, "{w:?}");
    }
    // A darker grey of the same chromaticity stays neutral and scales linearly.
    let g = apply3(&m, neutral.map(|v| v * 0.18));
    for c in g {
        assert!((c - 0.18).abs() < 1e-9, "{g:?}");
    }
}

#[test]
fn base_look_is_monotonic_and_bounded() {
    let mut last = -1.0;
    for i in 0..=1000 {
        let x = i as f32 / 100.0;
        let y = BaseLook::Default.apply(x);
        assert!(y >= last && (0.0..=1.0).contains(&y), "x={x} y={y}");
        last = y;
    }
    assert_eq!(BaseLook::Default.apply(0.0), 0.0);
    assert_eq!(BaseLook::Linear.apply(0.3), 0.3);
}

fn map(pairs: &[(&str, &str)]) -> BTreeMap<String, Value> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), Value::Text(v.to_string())))
        .collect()
}

#[test]
fn old_process_sliders_are_read_as_todays() {
    let old = [
        ("ProcessVersion", "5.7"),
        ("Exposure", "+0.50"),
        ("Brightness", "+50"),
        ("Contrast", "+75"),
        ("Shadows", "5"),
        ("FillLight", "20"),
        ("HighlightRecovery", "30"),
    ];
    let (s, missing) = DevelopSettings::from_settings(&map(&old));
    assert!(missing.is_empty(), "{missing:?}");
    assert!((s.exposure_ev - 0.5).abs() < 1e-9);
    assert!(s.tone.contrast > 0.0 && s.tone.shadows > 0.0 && s.tone.highlights < 0.0);
    assert_eq!(s.tone.blacks, 0.0, "Shadows at its default of 5");
    // Defaults alone change nothing.
    let (s, _) = DevelopSettings::from_settings(&map(&[
        ("ProcessVersion", "5.7"),
        ("Brightness", "+50"),
        ("Contrast", "+25"),
    ]));
    assert!(s.tone.is_neutral() && s.exposure_ev == 0.0);
    // A 2012 setting wins, and a current edit ignores the old sliders.
    let (s, _) = DevelopSettings::from_settings(&map(&[
        ("ProcessVersion", "5.7"),
        ("Exposure", "+1"),
        ("Exposure2012", "-1"),
    ]));
    assert_eq!(s.exposure_ev, -1.0);
    let (s, _) =
        DevelopSettings::from_settings(&map(&[("ProcessVersion", "11.0"), ("Contrast", "+100")]));
    assert!(s.tone.is_neutral());
}

#[test]
fn panels_switched_off_are_left_out() {
    let curve = Value::Seq(
        ["0, 0", "128, 160", "255, 255"]
            .map(|p| Value::Text(p.into()))
            .to_vec(),
    );
    let with_curve = |pairs: &[(&str, &str)]| {
        let mut m = map(pairs);
        m.insert("ToneCurvePV2012".into(), curve.clone());
        m
    };
    let edit = [
        ("SaturationAdjustmentRed", "-50"),
        ("ConvertToGrayscale", "True"),
        ("GrayMixerRed", "40"),
        ("ColorGradeShadowSat", "30"),
        ("Sharpness", "60"),
        ("GrainAmount", "40"),
        ("AutoLateralCA", "1"),
    ];
    let (on, missing) = DevelopSettings::from_settings(&with_curve(&edit));
    assert!(on.curve_rgb.is_some() && on.grain.amount > 0.0 && on.lens.auto_ca);
    assert!(on.sharpening.amount > 0.0 && on.color.gray_mix[0] != 0.0);
    let mut off = edit.to_vec();
    for switch in [
        "EnableToneCurve",
        "EnableColorAdjustments",
        "EnableGrayscaleMix",
        "EnableSplitToning",
        "EnableDetail",
        "EnableLensCorrections",
        "EnableEffects",
    ] {
        off.push((switch, "False"));
    }
    let (s, missing_off) = DevelopSettings::from_settings(&with_curve(&off));
    assert!(s.curve_rgb.is_none() && s.grain.amount == 0.0 && !s.lens.auto_ca);
    assert_eq!(s.sharpening.amount, 0.0);
    assert!(s.color.grayscale, "the treatment stays");
    assert_eq!(s.color.gray_mix[0], 0.0);
    assert!(s.color.hsl.iter().flatten().all(|v| *v == 0.0));
    assert!(s.color.grading.is_neutral());
    assert_eq!(missing, missing_off, "the switches themselves are read");
    // Explicitly on is the same as unset.
    let mut on_again = edit.to_vec();
    on_again.push(("EnableToneCurve", "True"));
    assert!(
        DevelopSettings::from_settings(&with_curve(&on_again))
            .0
            .curve_rgb
            .is_some()
    );
}

#[test]
fn parses_tone_colour_and_grading() {
    let m = map(&[
        ("Contrast2012", "+40"),
        ("Highlights2012", "-60"),
        ("Vibrance", "+30"),
        ("SaturationAdjustmentGreen", "-40"),
        ("HueAdjustmentRed", "+15"),
        ("ColorGradeShadowHue", "200"),
        ("ColorGradeShadowSat", "25"),
        ("ColorGradeBlending", "70"),
        ("SplitToningBalance", "-20"),
        ("ParametricLights", "+10"),
    ]);
    let (s, missing) = DevelopSettings::from_settings(&m);
    assert!(missing.is_empty(), "{missing:?}");
    assert_eq!(s.tone.contrast, 40.0);
    assert_eq!(s.tone.highlights, -60.0);
    assert_eq!(s.color.vibrance, 30.0);
    assert_eq!(s.color.hsl[3][1], -40.0); // Green saturation
    assert_eq!(s.color.hsl[0][0], 15.0); // Red hue
    assert_eq!(s.color.grading.shadows.hue, 200.0);
    assert_eq!(s.color.grading.blending, 70.0);
    assert_eq!(s.color.grading.balance, -20.0);
    assert_eq!(s.parametric.lights, 10.0);
}

#[test]
fn legacy_split_toning_maps_to_grading_wheels() {
    let m = map(&[
        ("SplitToningShadowHue", "210"),
        ("SplitToningShadowSaturation", "18"),
        ("SplitToningHighlightHue", "45"),
        ("SplitToningHighlightSaturation", "12"),
    ]);
    let (s, _) = DevelopSettings::from_settings(&m);
    assert_eq!(
        s.color.grading.shadows,
        Wheel {
            hue: 210.0,
            sat: 18.0,
            lum: 0.0
        }
    );
    assert_eq!(s.color.grading.highlights.sat, 12.0);
    // When Color Grading tags are present they win over split toning.
    let mut m2 = m.clone();
    m2.insert("ColorGradeShadowSat".into(), Value::Text("5".into()));
    let (s2, _) = DevelopSettings::from_settings(&m2);
    assert_eq!(s2.color.grading.shadows.sat, 5.0);
}

/// A camera whose RGB is linear ProPhoto (white balance and matrix are
/// then the identity), with `f` giving each pixel.
#[test]
fn a_cube_lut_is_applied_by_its_amount() {
    let dir = std::env::temp_dir().join(format!("amb-pipeline-lut-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // Every colour to a mid grey (sRGB 0.5), so the result is known.
    let path = dir.join("grey.cube");
    std::fs::write(
        &path,
        format!("LUT_3D_SIZE 2\n{}", "0.5 0.5 0.5\n".repeat(8)),
    )
    .unwrap();
    let img = camera(2, 1, |x, _| {
        if x == 0 {
            [0.05, 0.05, 0.05]
        } else {
            [0.6, 0.3, 0.1]
        }
    });
    let render_with = |amount: &str| {
        let (s, missing) = DevelopSettings::from_settings(&map(&[
            ("amb:LUT", path.to_str().unwrap()),
            ("amb:LUTAmount", amount),
        ]));
        assert!(missing.is_empty(), "{missing:?}");
        render(&img, &s)
    };
    let plain = render(&img, &DevelopSettings::default());
    let full = render_with("100");
    let grey = srgb_decode(0.5);
    for i in 0..2 {
        let p = [0, 1, 2].map(|c| full.rgb[i * 3 + c]);
        assert!(p.iter().all(|v| (v - grey).abs() < 0.02), "{p:?}");
    }
    // Half way: between the plain render and the grey.
    let half = render_with("50");
    let (a, b, c) = (plain.rgb[0], half.rgb[0], full.rgb[0]);
    assert!(a.min(c) < b && b < a.max(c), "{a} {b} {c}");
    // A missing file is reported, not rendered.
    let (s, missing) = DevelopSettings::from_settings(&map(&[("amb:LUT", "/nowhere/x.cube")]));
    assert!(s.lut.is_none() && missing.len() == 1 && missing[0].starts_with("amb:LUT"));
    let _ = std::fs::remove_dir_all(&dir);
}

fn camera(w: usize, h: usize, f: impl Fn(usize, usize) -> [f32; 3]) -> CameraImage {
    let mut rgb = Vec::with_capacity(w * h * 3);
    for y in 0..h {
        for x in 0..w {
            rgb.extend_from_slice(&f(x, y));
        }
    }
    CameraImage {
        width: w,
        height: h,
        rgb,
        xyz_to_cam: invert3(&PROPHOTO_TO_XYZ_D50),
        matrices: Vec::new(),
        highlights_rebuilt: false,
        as_shot_neutral: [1.0; 3],
        clip: [f32::INFINITY; 3],
        rendered: false,
        make: "Test".into(),
        model: "Synthetic".into(),
        shot: Default::default(),
        metadata: Default::default(),
    }
}

/// A busy photo: gradients, hard edges, fine texture and a bright patch.
fn busy(w: usize, h: usize) -> CameraImage {
    textured(w, h, 0.05)
}

/// [`busy`] with pixel-level texture of `fine` amplitude.
fn textured(w: usize, h: usize, fine: f32) -> CameraImage {
    let amplitude = fine;
    camera(w, h, |x, y| {
        let (fx, fy) = (x as f32 / w as f32, y as f32 / h as f32);
        let edge = if (x / 37 + y / 23) % 2 == 0 {
            0.15
        } else {
            0.0
        };
        let fine = ((x * 7 + y * 13) % 11) as f32 / 11.0 * amplitude;
        let sun = if (fx - 0.7).hypot(fy - 0.3) < 0.06 {
            2.0
        } else {
            0.0
        };
        [
            0.05 + 0.3 * fx + edge + fine + sun,
            0.08 + 0.25 * fy + edge * 0.5 + fine + sun,
            0.12 + 0.2 * (1.0 - fx) + fine + sun,
        ]
    })
}

/// Settings from `pairs`, with the default colour noise reduction off
/// unless given (it measures the noise on what it renders).
fn edit(pairs: &[(&str, &str)], extra: &[(&str, Value)]) -> DevelopSettings {
    let mut m = map(&[("ColorNoiseReduction", "0")]);
    m.extend(map(pairs));
    for (k, v) in extra {
        m.insert(k.to_string(), v.clone());
    }
    let (s, missing) = DevelopSettings::from_settings(&m);
    assert!(missing.is_empty(), "{missing:?}");
    s
}

/// Largest channel difference between a region render and the same
/// rectangle of the full render, and the mean one.
fn region_error(img: &CameraImage, s: &DevelopSettings, window: Rect) -> (f32, f32) {
    let full = render(img, s);
    // Globals from a copy a quarter the size (the editor's is about
    // 1600 px on a side, against 6000 or so).
    let small = crate::raw::downscale(img, img.width.max(img.height) / 4);
    let globals = measure_globals(img, &small, s);
    let part = render_region(img, s, window, &globals);
    assert_eq!((part.width, part.height), (window.width, window.height));
    let (mut max, mut sum) = (0.0f32, 0.0f64);
    for y in 0..window.height {
        for x in 0..window.width {
            let (a, b) = (part.get(x, y), full.get(window.x + x, window.y + y));
            for c in 0..3 {
                let d = (a[c] - b[c]).abs();
                max = max.max(d);
                sum += d as f64;
            }
        }
    }
    (
        max,
        (sum / (window.width * window.height * 3) as f64) as f32,
    )
}

const WINDOW: Rect = Rect {
    x: 230,
    y: 150,
    width: 180,
    height: 120,
};

#[test]
fn region_matches_the_full_render() {
    let img = busy(640, 480);
    let cases: Vec<(&str, DevelopSettings)> = vec![
        ("plain", edit(&[], &[])),
        (
            "geometry",
            edit(
                &[
                    ("HasCrop", "True"),
                    ("CropLeft", "0.07"),
                    ("CropTop", "0.05"),
                    ("CropRight", "0.95"),
                    ("CropBottom", "0.93"),
                    ("CropAngle", "3.5"),
                    ("PerspectiveVertical", "-20"),
                    ("PerspectiveRotate", "1.5"),
                    ("LensManualDistortionAmount", "30"),
                    ("VignetteAmount", "-40"),
                ],
                &[],
            ),
        ),
        (
            "local filters",
            edit(
                &[
                    ("Texture", "40"),
                    ("Clarity2012", "-35"),
                    ("Highlights2012", "-60"),
                    ("Shadows2012", "70"),
                    ("Sharpness", "80"),
                    ("SharpenRadius", "2"),
                    ("SharpenEdgeMasking", "30"),
                    ("DefringePurpleAmount", "10"),
                ],
                &[],
            ),
        ),
        (
            "effects",
            edit(
                &[
                    ("PostCropVignetteAmount", "-50"),
                    ("PostCropVignetteFeather", "40"),
                    ("GrainAmount", "60"),
                    ("GrainSize", "40"),
                    ("HasCrop", "True"),
                    ("CropLeft", "0.1"),
                    ("CropTop", "0.1"),
                    ("CropRight", "0.9"),
                    ("CropBottom", "0.9"),
                ],
                &[],
            ),
        ),
    ];
    for (name, s) in cases {
        let (max, _) = region_error(&img, &s, WINDOW);
        assert!(max < 1e-4, "{name}: {max}");
    }
}

#[test]
fn region_matches_with_masks_and_spots() {
    let img = busy(640, 480);
    let xmp = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
      <rdf:Description rdf:about="" xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/">
      <crs:MaskGroupBasedCorrections><rdf:Seq><rdf:li><rdf:Description
        crs:What="Correction" crs:CorrectionAmount="1" crs:CorrectionActive="true"
        crs:LocalExposure2012="0.25" crs:LocalClarity2012="0.5" crs:LocalSharpness="-0.6">
        <crs:CorrectionMasks><rdf:Seq>
          <rdf:li crs:What="Mask/CircularGradient" crs:MaskBlendMode="0" crs:MaskValue="1"
            crs:Top="0.2" crs:Left="0.3" crs:Bottom="0.7" crs:Right="0.65" crs:Angle="20"
            crs:Feather="50" crs:Flipped="true"/>
        </rdf:Seq></crs:CorrectionMasks>
      </rdf:Description></rdf:li></rdf:Seq></crs:MaskGroupBasedCorrections>
      </rdf:Description></rdf:RDF></x:xmpmeta>"#;
    let mut settings = amb_presets::xmp::parse_crs(xmp).unwrap();
    let area = |centre, r, source| {
        Value::Struct(amb_presets::retouch::new_area(
            amb_presets::retouch::SpotKind::Heal,
            centre,
            r,
            source,
            source.is_none(),
            0.3,
            1.0,
        ))
    };
    settings.insert(
        "RetouchAreas".into(),
        Value::Seq(vec![
            // Inside the window, its donor far outside it.
            area((0.5, 0.42), 0.02, Some((0.12, 0.85))),
            // In the margin, with a searched donor.
            area((0.33, 0.3), 0.015, None),
            // Far away: no effect on the window.
            area((0.9, 0.9), 0.02, None),
        ]),
    );
    settings.extend(map(&[("ColorNoiseReduction", "0")]));
    let (s, missing) = DevelopSettings::from_settings(&settings);
    assert!(missing.is_empty(), "{missing:?}");
    let (max, _) = region_error(&img, &s, WINDOW);
    assert!(max < 1e-4, "{max}");
}

#[test]
fn region_with_measured_globals_is_close() {
    // Dehaze, a film negative, colour range masks and noise reduction
    // measure the whole photo; a region takes them from a small copy or
    // measures its part. A full-size photo is measured over blocks of
    // about 10 pixels, which the small copy's own averaging matches;
    // this photo is too small for that, so it has no pixel-level texture.
    let img = textured(640, 480, 0.0);
    let st = |pairs: &[(&str, Value)]| {
        Value::Struct(
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect(),
        )
    };
    let text = |v: &str| Value::Text(v.into());
    // A colour range sampled outside the window, brightened and dehazed.
    let colour_mask = st(&[
        ("What", text("Correction")),
        ("CorrectionAmount", text("1")),
        ("CorrectionActive", text("true")),
        ("LocalExposure2012", text("0.3")),
        ("LocalDehaze", text("0.4")),
        (
            "CorrectionMasks",
            Value::Seq(vec![st(&[
                ("What", text("Mask/RangeMask")),
                ("MaskBlendMode", text("0")),
                ("MaskValue", text("1")),
                (
                    "CorrectionRangeMask",
                    st(&[
                        ("Version", text("3")),
                        ("Type", text("1")),
                        ("ColorAmount", text("0.6")),
                        ("amb:ColorSamples", Value::Seq(vec![text("0.2 0.8")])),
                    ]),
                ),
            ])]),
        ),
    ]);
    let cases = [
        ("dehaze", edit(&[("Dehaze", "40")], &[])),
        (
            "negative",
            edit(
                &[("amb:NegativeConvert", "True"), ("Exposure2012", "0.5")],
                &[],
            ),
        ),
        (
            "colour range and local dehaze",
            edit(
                &[],
                &[("MaskGroupBasedCorrections", Value::Seq(vec![colour_mask]))],
            ),
        ),
        (
            "noise reduction",
            edit(
                &[("LuminanceSmoothing", "40"), ("ColorNoiseReduction", "50")],
                &[],
            ),
        ),
    ];
    for (name, mut s) in cases {
        assert!(needs_globals(&s) || name == "noise reduction", "{name}");
        // As in the editor, the haze is estimated on a grid about the
        // small copy's size (1536 px against 1600 there).
        s.calibration.dehaze.estimate_long_side = 150.0;
        let (max, mean) = region_error(&img, &s, WINDOW);
        assert!(mean < 3e-3 && max < 0.05, "{name}: max {max}, mean {mean}");
    }
}

#[test]
fn region_plan_holds_the_window_and_aligns() {
    let img = busy(640, 480);
    let s = edit(&[("Clarity2012", "50")], &[]);
    let work = region_plan(&img, &s, WINDOW);
    assert!(work.contains(&WINDOW), "{work:?}");
    assert_eq!(work.x % REGION_ALIGN, 0);
    assert_eq!(work.y % REGION_ALIGN, 0);
    assert!(work.right() <= 640 && work.bottom() <= 480);
    assert!(work.width < 640 || work.height < 480, "{work:?}");
}

fn linear(settings: &[(&str, &str)]) -> DevelopSettings {
    let (s, missing) = DevelopSettings::from_settings(&map(settings));
    assert!(missing.is_empty(), "{missing:?}");
    // No detail defaults: they treat the image border differently from
    // the interior, which would blur the comparisons below.
    let mut s = DevelopSettings {
        base_look: BaseLook::Linear,
        ..s
    };
    s.sharpening.amount = 0.0;
    s.noise_reduction.color = 0.0;
    s
}

#[test]
fn local_adjustments_render_through_their_masks() {
    // The top of the frame (rows < 10 of 40) gets local Exposure +1 EV,
    // Saturation −100 and Clarity; the bottom stays as the global edit.
    let xmp = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
      <rdf:Description rdf:about="" xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/">
      <crs:MaskGroupBasedCorrections><rdf:Seq><rdf:li><rdf:Description
        crs:What="Correction" crs:CorrectionAmount="1" crs:CorrectionActive="true"
        crs:LocalExposure2012="0.25" crs:LocalSaturation="-1" crs:LocalClarity2012="0.5">
        <crs:CorrectionMasks><rdf:Seq>
          <rdf:li crs:What="Mask/Gradient" crs:MaskBlendMode="0" crs:MaskValue="1"
            crs:ZeroX="0.5" crs:ZeroY="0.3" crs:FullX="0.5" crs:FullY="0.25"/>
        </rdf:Seq></crs:CorrectionMasks>
      </rdf:Description></rdf:li></rdf:Seq></crs:MaskGroupBasedCorrections>
      </rdf:Description></rdf:RDF></x:xmpmeta>"#;
    let settings = amb_presets::xmp::parse_crs(xmp).unwrap();
    let (s, missing) = DevelopSettings::from_settings(&settings);
    assert!(missing.is_empty(), "{missing:?}");
    let s = DevelopSettings {
        base_look: BaseLook::Linear,
        sharpening: Sharpening {
            amount: 0.0,
            ..s.sharpening
        },
        noise_reduction: NoiseReduction {
            color: 0.0,
            ..s.noise_reduction
        },
        ..s
    };
    let img = camera(20, 40, |_, _| [0.10, 0.05, 0.02]);
    let out = render(&img, &s);
    let plain = render(&img, &linear(&[]));
    let (top, bottom) = (out.get(10, 2), out.get(10, 37));
    assert_eq!(bottom, plain.get(10, 37), "outside the mask is untouched");
    let y = |p: [f32; 3]| p.iter().sum::<f32>();
    assert!(y(top) > 1.6 * y(plain.get(10, 2)), "brighter: {top:?}");
    let spread =
        |p: [f32; 3]| p.iter().fold(0f32, |a, &v| a.max(v)) - p.iter().fold(1f32, |a, &v| a.min(v));
    assert!(spread(top) < 0.02 * y(top), "grey: {top:?}");

    // The overlay tints the mask red and leaves the rest alone.
    let mut shown = s.clone();
    shown.masks.overlay = Some(0);
    let o = render(&img, &shown);
    assert!(o.get(10, 2)[0] > 3.0 * o.get(10, 2)[2]);
    assert_eq!(o.get(10, 37), out.get(10, 37));
}

fn ramp(x: usize, y: usize) -> [f32; 3] {
    [
        0.1 + 0.002 * x as f32,
        0.2 + 0.003 * y as f32,
        0.05 + 0.001 * (x + y) as f32,
    ]
}

#[test]
fn crop_tags_parse_and_render() {
    let m = map(&[
        ("HasCrop", "True"),
        ("CropLeft", "0.25"),
        ("CropTop", "0.1"),
        ("CropRight", "0.75"),
        ("CropBottom", "0.6"),
        ("CropAngle", "0"),
        ("CropConstrainToWarp", "0"),
    ]);
    let (s, missing) = DevelopSettings::from_settings(&m);
    assert!(missing.is_empty(), "{missing:?}");
    assert_eq!(
        s.crop,
        Crop {
            enabled: true,
            left: 0.25,
            top: 0.1,
            right: 0.75,
            bottom: 0.6,
            angle: 0.0
        }
    );
    // Without HasCrop the edges are kept but nothing is cropped.
    let (s, _) = DevelopSettings::from_settings(&map(&[("CropLeft", "0.25")]));
    assert!(!s.crop.enabled);
    let img = camera(200, 100, ramp);
    assert_eq!(render(&img, &s).width, 200);
}

#[test]
fn crop_renders_the_sub_rectangle() {
    let img = camera(200, 100, ramp);
    let full = render(&img, &linear(&[]));
    assert_eq!((full.width, full.height), (200, 100));
    let s = linear(&[
        ("HasCrop", "True"),
        ("CropLeft", "0.25"),
        ("CropTop", "0.1"),
        ("CropRight", "0.75"),
        ("CropBottom", "0.6"),
    ]);
    let out = render(&img, &s);
    assert_eq!((out.width, out.height), (100, 50));
    for y in 0..50 {
        for x in 0..100 {
            let a = &out.rgb[(y * 100 + x) * 3..][..3];
            let b = &full.rgb[((y + 10) * 200 + x + 50) * 3..][..3];
            for c in 0..3 {
                assert!((a[c] - b[c]).abs() < 1e-5, "({x},{y}) {a:?} vs {b:?}");
            }
        }
    }
}

#[test]
fn transform_warps_before_the_crop_and_constrain_crop_fills() {
    // A dark photo: blank areas (filled light) stand out.
    let img = camera(240, 160, |_, _| [0.02; 3]);
    let bright = |out: &ProPhotoImage, x: usize, y: usize| out.rgb[(y * out.width + x) * 3] > 0.5;
    let out = render(&img, &linear(&[("PerspectiveScale", "50")]));
    assert_eq!((out.width, out.height), (240, 160));
    assert!(bright(&out, 2, 2) && !bright(&out, 120, 80));
    // Negative Vertical widens the top: blank wedges at the bottom corners.
    let keystone = [("PerspectiveVertical", "-60")];
    let out = render(&img, &linear(&keystone));
    assert!(bright(&out, 1, 158) && !bright(&out, 1, 1));
    let mut constrained = keystone.to_vec();
    constrained.push(("CropConstrainToWarp", "True"));
    let out = render(&img, &linear(&constrained));
    assert!(out.rgb.iter().all(|v| *v < 0.5));
    // A crop is cut from the transformed photo.
    let mut cropped = keystone.to_vec();
    cropped.extend([
        ("HasCrop", "True"),
        ("CropLeft", "0"),
        ("CropTop", "0.5"),
        ("CropRight", "0.5"),
        ("CropBottom", "1"),
    ]);
    let out = render(&img, &linear(&cropped));
    assert_eq!((out.width, out.height), (120, 80));
    assert!(bright(&out, 1, 78));
    // Transform off: nothing happens, Upright is reported as not rendered.
    let (s, missing) = DevelopSettings::from_settings(&map(&[
        ("PerspectiveVertical", "-60"),
        ("EnableTransform", "False"),
    ]));
    assert!(s.transform.is_identity() && missing.is_empty());
    // Upright: Lightroom's analysis tags are not reported (we solve
    // again); Guided is.
    let (s, missing) = DevelopSettings::from_settings(&map(&[
        ("PerspectiveUpright", "3"),
        ("UprightTransform_3", "0.5,0.5"),
    ]));
    assert_eq!(s.transform.upright_mode, UprightMode::Vertical);
    assert!(missing.is_empty(), "{missing:?}");
    let (_, missing) = DevelopSettings::from_settings(&map(&[("PerspectiveUpright", "5")]));
    assert_eq!(missing.len(), 1);
    assert!(missing[0].contains("Upright Guided"), "{missing:?}");
}

#[test]
fn crop_composes_with_lens_geometry() {
    let img = camera(240, 160, ramp);
    let lens = [("LensManualDistortionAmount", "40")];
    let full = render(&img, &linear(&lens));
    let mut cropped = lens.to_vec();
    cropped.extend([
        ("HasCrop", "True"),
        ("CropLeft", "0.5"),
        ("CropTop", "0.25"),
        ("CropRight", "1"),
        ("CropBottom", "0.75"),
    ]);
    let out = render(&img, &linear(&cropped));
    assert_eq!((out.width, out.height), (120, 80));
    // The same lens-corrected pixels, with the same blank fill.
    for y in 0..80 {
        for x in 0..120 {
            let a = &out.rgb[(y * 120 + x) * 3..][..3];
            let b = &full.rgb[((y + 40) * 240 + x + 120) * 3..][..3];
            assert_eq!(a, b, "({x},{y})");
        }
    }
}

#[test]
fn straighten_levels_a_tilted_horizon() {
    // A horizon tilted by `tilt` degrees: dark above, light below, along
    // the line through the centre. Straightening by the matching angle
    // makes it level: every output row is (nearly) uniform.
    let (w, h) = (300, 200);
    let tilt: f64 = 6.0;
    let (sin, cos) = (crate::crop::ANGLE_SIGN * tilt).to_radians().sin_cos();
    let (cx, cy) = ((w as f64 - 1.0) / 2.0, (h as f64 - 1.0) / 2.0);
    let img = camera(w, h, |x, y| {
        // Distance below the horizon line, which runs along (cos, sin).
        let (dx, dy) = (x as f64 - cx, y as f64 - cy);
        let d = -dx * sin + dy * cos;
        let v = (0.5 + 0.4 * (d / 6.0).tanh()) as f32;
        [v; 3]
    });
    let row_spread = |img: &Image| -> f32 {
        (img.height / 4..img.height * 3 / 4)
            .map(|y| {
                let row: Vec<f32> = (img.width / 8..img.width * 7 / 8)
                    .map(|x| img.rgb[(y * img.width + x) * 3 + 1])
                    .collect();
                let lo = row.iter().cloned().fold(f32::MAX, f32::min);
                let hi = row.iter().cloned().fold(f32::MIN, f32::max);
                hi - lo
            })
            .fold(0.0, f32::max)
    };
    let tilted = render(&img, &linear(&[]));
    assert!(row_spread(&tilted) > 0.5, "the horizon is tilted");
    let angle = format!("{tilt}");
    let s = linear(&[
        ("HasCrop", "True"),
        ("CropLeft", "0.2"),
        ("CropTop", "0.2"),
        ("CropRight", "0.8"),
        ("CropBottom", "0.8"),
        ("CropAngle", angle.as_str()),
    ]);
    let level = render(&img, &s);
    assert!(row_spread(&level) < 0.02, "{}", row_spread(&level));
    // The opposite angle doubles the tilt instead.
    let angle = format!("{}", -tilt);
    let s = linear(&[
        ("HasCrop", "True"),
        ("CropLeft", "0.2"),
        ("CropTop", "0.2"),
        ("CropRight", "0.8"),
        ("CropBottom", "0.8"),
        ("CropAngle", angle.as_str()),
    ]);
    assert!(row_spread(&render(&img, &s)) > 0.5);
}

#[test]
fn post_crop_vignette_is_centred_on_the_crop() {
    let img = camera(300, 200, |_, _| [0.5; 3]);
    // An off-centre crop: the left third of the frame.
    let s = linear(&[
        ("HasCrop", "True"),
        ("CropLeft", "0"),
        ("CropTop", "0.25"),
        ("CropRight", "0.4"),
        ("CropBottom", "0.75"),
        ("PostCropVignetteAmount", "-80"),
    ]);
    let out = render(&img, &s);
    assert_eq!((out.width, out.height), (120, 100));
    let at = |x: usize, y: usize| out.rgb[(y * out.width + x) * 3 + 1];
    let (w, h) = (out.width, out.height);
    let centre = at(w / 2, h / 2);
    assert!(
        (centre - 0.5).abs() < 1e-3,
        "crop centre untouched: {centre}"
    );
    // All four corners of the crop are darkened alike.
    let corners = [at(0, 0), at(w - 1, 0), at(0, h - 1), at(w - 1, h - 1)];
    for c in corners {
        assert!(c < 0.4, "{corners:?}");
        assert!((c - corners[0]).abs() < 1e-3, "{corners:?}");
    }
    // Left/right symmetric about the crop's centre, not the frame's.
    for x in 0..w / 2 {
        let (l, r) = (at(x, h / 2), at(w - 1 - x, h / 2));
        assert!((l - r).abs() < 1e-3, "x={x}: {l} vs {r}");
    }
}

#[test]
fn settings_parse_and_report() {
    let mut map = BTreeMap::new();
    map.insert("WhiteBalance".to_string(), Value::Text("Custom".into()));
    map.insert("Temperature".to_string(), Value::Text("5500".into()));
    map.insert("Tint".to_string(), Value::Text("+10".into()));
    map.insert("Exposure2012".to_string(), Value::Text("+0.5".into()));
    map.insert("HighlightRecovery".to_string(), Value::Text("10".into()));
    map.insert("Name".to_string(), Value::Text("x".into()));
    let (s, missing) = DevelopSettings::from_settings(&map);
    assert_eq!(
        s.white_balance,
        WhiteBalance::Custom {
            temperature: 5500.0,
            tint: Some(10.0)
        }
    );
    assert_eq!(s.exposure_ev, 0.5);
    assert_eq!(missing.len(), 1);
    assert!(missing[0].starts_with("HighlightRecovery ("), "{missing:?}");
}
