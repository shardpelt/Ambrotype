// SPDX-License-Identifier: GPL-3.0-or-later
//! Tests for masks.

use super::*;

fn text(s: &str) -> Value {
    Value::Text(s.into())
}

fn st(pairs: &[(&str, Value)]) -> Value {
    Value::Struct(
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
    )
}

fn settings(corrections: Vec<Value>) -> BTreeMap<String, Value> {
    [(MASK_TAG.to_string(), Value::Seq(corrections))]
        .into_iter()
        .collect()
}

fn reference(w: usize, h: usize) -> Reference<'static> {
    Reference {
        luminance: None,
        lab: None,
        srgb: None,
        width: w,
        height: h,
        strengths: MaskStrengths::default(),
        tile: Tile::whole(w, h),
    }
}

fn alpha_of(s: &BTreeMap<String, Value>, w: usize, h: usize) -> Vec<f32> {
    let (m, _) = Masks::from_settings(s);
    draw_correction(
        &m.corrections[0],
        &Frame::new((w, h), None),
        &reference(w, h),
        &mut Targets::default(),
    )
}

fn linear(zy: &str, fy: &str) -> Value {
    st(&[
        ("What", text("Mask/Gradient")),
        ("MaskBlendMode", text("0")),
        ("MaskValue", text("1")),
        ("ZeroX", text("0.5")),
        ("ZeroY", text(zy)),
        ("FullX", text("0.5")),
        ("FullY", text(fy)),
    ])
}

fn correction(parts: Vec<Value>) -> Value {
    st(&[
        ("What", text("Correction")),
        ("CorrectionAmount", text("1")),
        ("LocalExposure2012", text("0.25")),
        ("CorrectionMasks", Value::Seq(parts)),
    ])
}

#[test]
fn local_defringe_adds_to_and_holds_back_the_global_one() {
    let (w, h) = (80, 40);
    let ok = OkLab::new();
    let hue = 310f32.to_radians();
    let purple = ok.to_working([0.5, 0.1 * hue.cos(), 0.1 * hue.sin()]);
    // A dark/bright edge with a 2 px purple fringe, in every row.
    let mut img = Image::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let v = match x {
                0..20 => [0.02; 3],
                20..22 => purple,
                _ => [0.9; 3],
            };
            img.set(x, y, v);
        }
    }
    let chroma = |img: &Image, y: usize| {
        let l = ok.from_working(img.get(21, y));
        l[1].hypot(l[2])
    };
    // The mask covers the top quarter fully, nothing below the middle.
    let drawn = |defringe: &str| {
        let mut c = correction(vec![linear("0.5", "0.25")]);
        if let Value::Struct(m) = &mut c {
            m.remove("LocalExposure2012");
            m.insert("LocalDefringe".into(), text(defringe));
        }
        let (m, _) = Masks::from_settings(&settings(vec![c]));
        draw(&m, &img, (w, h), None, |y| y, &MaskStrengths::default())
    };
    let cal = Calibration::default();
    // +100 with no global Defringe: removed under the mask only.
    let mut a = img.clone();
    drawn("1").apply_defringe(&mut a, &Defringe::default(), &cal);
    assert!(chroma(&a, 2) < 0.03, "top {}", chroma(&a, 2));
    assert!(chroma(&a, 35) > 0.09, "bottom {}", chroma(&a, 35));
    // −100 holds back a global Defringe under the mask.
    let global = Defringe {
        purple_amount: 10.0,
        ..Default::default()
    };
    let mut b = img.clone();
    drawn("-1").apply_defringe(&mut b, &global, &cal);
    assert!(chroma(&b, 2) > 0.09, "top {}", chroma(&b, 2));
    assert!(chroma(&b, 35) < 0.03, "bottom {}", chroma(&b, 35));
}

#[test]
fn linear_gradient_runs_from_zero_to_full() {
    // Full at the top, nothing below the middle.
    let a = alpha_of(
        &settings(vec![correction(vec![linear("0.5", "0.1")])]),
        10,
        100,
    );
    assert!(a[5 * 10 + 5] > 0.99, "top {}", a[55]);
    assert!(a[60 * 10 + 5] < 1e-6);
    let mid = a[30 * 10 + 5];
    assert!(mid > 0.3 && mid < 0.7, "mid {mid}");
}

#[test]
fn radial_inside_and_inverted() {
    let radial = |inverted: &str, flipped: &str| {
        st(&[
            ("What", text("Mask/CircularGradient")),
            ("MaskBlendMode", text("0")),
            ("MaskValue", text("1")),
            ("MaskInverted", text(inverted)),
            ("Flipped", text(flipped)),
            ("Top", text("0.25")),
            ("Left", text("0.25")),
            ("Bottom", text("0.75")),
            ("Right", text("0.75")),
            ("Feather", text("0")),
        ])
    };
    let a = alpha_of(
        &settings(vec![correction(vec![radial("false", "true")])]),
        40,
        40,
    );
    assert_eq!(a[20 * 40 + 20], 1.0);
    assert_eq!(a[2 * 40 + 2], 0.0);
    let a = alpha_of(
        &settings(vec![correction(vec![radial("true", "false")])]),
        40,
        40,
    );
    assert_eq!(a[20 * 40 + 20], 0.0);
    assert_eq!(a[2 * 40 + 2], 1.0);
}

#[test]
fn subtract_and_intersect() {
    // A: the top half (hard-ish), B: the left half.
    let left = st(&[
        ("What", text("Mask/Gradient")),
        ("ZeroX", text("0.55")),
        ("ZeroY", text("0.5")),
        ("FullX", text("0.45")),
        ("FullY", text("0.5")),
    ]);
    let with = |mode: &str, value: &str, inverted: &str| {
        let mut b = left.clone();
        if let Value::Struct(m) = &mut b {
            m.insert("MaskBlendMode".into(), text(mode));
            m.insert("MaskValue".into(), text(value));
            m.insert("MaskInverted".into(), text(inverted));
        }
        settings(vec![correction(vec![linear("0.55", "0.45"), b])])
    };
    let (w, h) = (40, 40);
    let at = |a: &[f32], x: usize, y: usize| a[y * w + x];
    let sub = alpha_of(&with("1", "0", "false"), w, h);
    assert!(at(&sub, 35, 5) > 0.99, "top right kept");
    assert!(at(&sub, 5, 5) < 0.01, "top left removed");
    let inter = alpha_of(&with("1", "0", "true"), w, h);
    assert!(at(&inter, 5, 5) > 0.99, "top left kept");
    assert!(at(&inter, 35, 5) < 0.01, "top right removed");
    assert!(at(&inter, 5, 35) < 0.01, "bottom never in");
}

#[test]
fn brush_paints_and_erases() {
    let stroke = |value: &str, dabs: &[&str]| {
        st(&[
            ("What", text("Mask/Paint")),
            ("MaskSyncID", text("S")),
            ("MaskValue", text(value)),
            ("Radius", text("0.1")),
            ("Flow", text("1")),
            ("CenterWeight", text("0.5")),
            ("Dabs", Value::Seq(dabs.iter().map(|d| text(d)).collect())),
        ])
    };
    let s = settings(vec![correction(vec![
        stroke("1", &["d 0.25 0.5", "d 0.5 0.5", "d 0.75 0.5"]),
        stroke("0", &["d 0.75 0.5"]),
    ])]);
    let (w, h) = (100, 100);
    let a = alpha_of(&s, w, h);
    assert!(a[50 * w + 50] > 0.99, "painted");
    assert!(a[50 * w + 75] < 0.01, "erased");
    assert!(a[10 * w + 50] < 1e-6, "untouched");
}

#[test]
fn aggregate_brush_with_a_density_and_the_acr_note() {
    let stroke = |value: &str| {
        st(&[
            ("What", text("Mask/Paint")),
            ("MaskValue", text(value)),
            ("Radius", text("0.1")),
            ("Flow", text("1")),
            ("CenterWeight", text("1")),
            ("Dabs", Value::Seq(vec![text("d 0.5 0.5")])),
        ])
    };
    let agg = st(&[
        ("What", text("Mask/Aggregate")),
        ("MaskBlendMode", text("0")),
        ("MaskValue", text("1")),
        ("Masks", Value::Seq(vec![stroke("0.5")])),
    ]);
    let (w, h) = (100, 100);
    let a = alpha_of(&settings(vec![correction(vec![agg])]), w, h);
    assert!((a[50 * w + 50] - 0.5).abs() < 0.01, "{}", a[50 * w + 50]);
    assert_eq!(a[10 * w + 10], 0.0);
    // Strokes kept in the .acr file can't be drawn yet, and say so.
    let table = st(&[
        ("What", text("Mask/Aggregate")),
        ("MaskBrushTable", text("0123456789abcdef0123456789abcdef")),
    ]);
    let (_, notes) = Masks::from_settings(&settings(vec![correction(vec![table])]));
    assert!(notes[0].contains(".acr"), "{notes:?}");
}

#[test]
fn crop_moves_the_mask_with_the_photo() {
    // The right half of a 100×10 frame, as a crop: the gradient is
    // defined on the whole frame, so the crop sees only its full half.
    let crop = crate::crop::Crop {
        enabled: true,
        left: 0.5,
        right: 1.0,
        ..Default::default()
    };
    let g = crop.geometry(100, 10).unwrap();
    let s = settings(vec![correction(vec![st(&[
        ("What", text("Mask/Gradient")),
        ("ZeroX", text("0.4")),
        ("ZeroY", text("0.5")),
        ("FullX", text("0.5")),
        ("FullY", text("0.5")),
    ])])]);
    let (m, _) = Masks::from_settings(&s);
    let r = reference(g.width, g.height);
    let a = draw_correction(
        &m.corrections[0],
        &Frame::new((100, 10), Some(&g)),
        &r,
        &mut Targets::default(),
    );
    assert!(a.iter().all(|&v| v > 0.99));
}

#[test]
fn luminance_range_selects_bright_pixels() {
    let range = st(&[
        ("What", text("Mask/RangeMask")),
        (
            "CorrectionRangeMask",
            st(&[
                ("Version", text("3")),
                ("Type", text("2")),
                ("LumRange", text("0.5 0.6 1 1")),
            ]),
        ),
    ]);
    let (m, notes) = Masks::from_settings(&settings(vec![correction(vec![range])]));
    assert!(notes.is_empty(), "{notes:?}");
    let lum: Vec<f32> = (0..10).map(|i| i as f32 / 9.0).collect();
    let r = Reference {
        luminance: Some(&lum),
        lab: None,
        srgb: None,
        width: 10,
        height: 1,
        strengths: MaskStrengths::default(),
        tile: Tile::whole(10, 1),
    };
    let a = draw_correction(
        &m.corrections[0],
        &Frame::new((10, 1), None),
        &r,
        &mut Targets::default(),
    );
    assert_eq!(a[0], 0.0);
    assert_eq!(a[9], 1.0);
    assert!(a[4] < 1e-6 && a[6] > 0.99, "{a:?}");
}

/// The registered finder is one per process, and tests run side by side:
/// a test that needs it set, or unset, holds this for its whole run.
static FINDER_TEST: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Holds [`FINDER_TEST`]; on drop (even in a failed test) the finder is removed.
struct FinderTest {
    _lock: std::sync::MutexGuard<'static, ()>,
}

impl FinderTest {
    fn hold() -> Self {
        FinderTest {
            _lock: FINDER_TEST.lock().unwrap_or_else(|e| e.into_inner()),
        }
    }
}

impl Drop for FinderTest {
    fn drop(&mut self) {
        crate::subject::clear_finder();
    }
}

#[test]
fn ai_masks_select_nothing_and_are_reported() {
    let _no_finder = FinderTest::hold();
    let ai = st(&[("What", text("Mask/Image")), ("MaskSubType", text("1"))]);
    let s = settings(vec![correction(vec![ai])]);
    let (m, notes) = Masks::from_settings(&s);
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert!(notes[0].contains("Select Subject"));
    let a = draw_correction(
        &m.corrections[0],
        &Frame::new((4, 4), None),
        &reference(4, 4),
        &mut Targets::default(),
    );
    assert!(a.iter().all(|&v| v == 0.0));
}

#[test]
fn select_sky_finds_the_sky_joined_to_the_top() {
    // A blue sky over dark, textured trees, with a blue lake below them.
    let (w, h) = (120, 90);
    let ok = OkLab::new();
    let sky = ok.from_working([0.45, 0.6, 0.9]);
    let lab: Vec<[f32; 3]> = (0..w * h)
        .map(|i| {
            let (x, y) = (i % w, i / w);
            // The sky, and the lake.
            if y < 40 || ((65..80).contains(&y) && (30..90).contains(&x)) {
                sky
            } else {
                // Trees: dark green with a checker texture.
                let t = if (x / 2 + y / 2) % 2 == 0 { 0.05 } else { 0.12 };
                ok.from_working([t * 0.6, t, t * 0.5])
            }
        })
        .collect();
    let map = sky_map(&lab, w, h);
    let at = |x: usize, y: usize| map.at((x as f64 + 0.5) / w as f64, (y as f64 + 0.5) / h as f64);
    assert!(at(60, 10) > 0.99 && at(5, 30) > 0.99);
    assert!(at(60, 55) < 0.01, "trees");
    assert!(at(60, 72) < 0.01, "the lake isn't joined to the top");
    // Drawn through the mask: the sky's own pixels, nothing else.
    let ai = st(&[("What", text("Mask/Image")), ("MaskSubType", text("2"))]);
    let (m, notes) = Masks::from_settings(&settings(vec![correction(vec![ai])]));
    assert!(notes.is_empty(), "{notes:?}");
    let r = Reference {
        luminance: None,
        lab: Some(&lab),
        srgb: None,
        width: w,
        height: h,
        strengths: MaskStrengths::default(),
        tile: Tile::whole(w, h),
    };
    let mut targets = Targets::default();
    let a = draw_correction(
        &m.corrections[0],
        &Frame::new((w, h), None),
        &r,
        &mut targets,
    );
    assert!(a[10 * w + 60] > 0.9 && a[55 * w + 60] < 0.01 && a[72 * w + 60] < 0.01);
    assert_eq!(targets.sky_measured.len(), 1);
}

#[test]
fn model_masks_ask_for_their_kind_and_background_is_the_subject_inverted() {
    use crate::subject::{Map, MapKind};
    let _finder = FinderTest::hold();
    // A stand-in model: the subject is the left half, the people the top.
    crate::subject::set_finder(std::sync::Arc::new(
        |kind: MapKind, _: &[f32], _: usize, _: usize| {
            let n = 32;
            let data = (0..n * n)
                .map(|i| {
                    let (x, y) = (i % n, i / n);
                    let inside = match kind {
                        MapKind::Subject => x < n / 2,
                        MapKind::People => y < n / 2,
                    };
                    if inside { 1.0 } else { 0.0 }
                })
                .collect();
            Ok(Map {
                width: n,
                height: n,
                data,
            })
        },
    ));
    let (w, h) = (64, 64);
    let srgb = vec![0.5f32; w * h * 3];
    let draw_one = |part: Value| {
        let (m, notes) = Masks::from_settings(&settings(vec![correction(vec![part])]));
        assert!(notes.is_empty(), "{notes:?}");
        let r = Reference {
            luminance: None,
            lab: None,
            srgb: Some(&srgb),
            width: w,
            height: h,
            strengths: MaskStrengths::default(),
            tile: Tile::whole(w, h),
        };
        draw_correction(
            &m.corrections[0],
            &Frame::new((w, h), None),
            &r,
            &mut Targets::default(),
        )
    };
    let subject = st(&[("What", text("Mask/Image")), ("MaskSubType", text("1"))]);
    let background = st(&[
        ("What", text("Mask/Image")),
        ("MaskSubType", text("1")),
        ("MaskInverted", Value::Bool(true)),
    ]);
    let people = st(&[("What", text("Mask/Image")), ("MaskSubType", text("3"))]);
    let (s, b, p) = (draw_one(subject), draw_one(background), draw_one(people));
    let at = |a: &[f32], x: usize, y: usize| a[y * w + x];
    assert!(at(&s, 8, 40) > 0.9 && at(&s, 56, 40) < 0.1);
    assert!(at(&b, 8, 40) < 0.1 && at(&b, 56, 40) > 0.9);
    assert!(at(&p, 40, 8) > 0.9 && at(&p, 40, 56) < 0.1);
}

#[test]
fn local_exposure_is_stored_in_quarter_stops() {
    let (m, _) = Masks::from_settings(&settings(vec![correction(vec![linear("1", "0")])]));
    assert_eq!(m.corrections[0].adjust.exposure, 1.0);
}

#[test]
fn exposure_brightens_only_under_the_mask() {
    let s = settings(vec![correction(vec![linear("0.55", "0.45")])]);
    let (m, _) = Masks::from_settings(&s);
    let mut img = Image::from_fn(4, 20, |_, _| [0.2, 0.2, 0.2]);
    let drawn = draw(&m, &img, (4, 20), None, |v| v, &MaskStrengths::default());
    drawn.apply_light(&mut img, &MaskStrengths::default());
    assert!((img.get(1, 1)[0] - 0.4).abs() < 1e-4, "{:?}", img.get(1, 1));
    assert!((img.get(1, 18)[0] - 0.2).abs() < 1e-6);
}

#[test]
fn a_masks_curve_applies_only_under_it() {
    // A curve that darkens the midtones, and no sliders.
    let mut c = correction(vec![linear("0.55", "0.45")]);
    if let Value::Struct(m) = &mut c {
        m.remove("LocalExposure2012");
        m.insert(
            "MainCurve".into(),
            Value::Seq(["0, 0", "128, 64", "255, 255"].map(text).to_vec()),
        );
    }
    let (m, notes) = Masks::from_settings(&settings(vec![c]));
    assert!(notes.is_empty(), "{notes:?}");
    assert!(m.corrections[0].adjust.is_neutral() && m.corrections[0].curves.is_some());
    let mut img = Image::from_fn(4, 20, |_, _| [0.2, 0.2, 0.2]);
    let drawn = draw(&m, &img, (4, 20), None, |v| v, &MaskStrengths::default());
    drawn.apply_curves(&mut img);
    assert!(img.get(1, 1)[0] < 0.1, "{:?}", img.get(1, 1));
    assert!((img.get(1, 18)[0] - 0.2).abs() < 1e-6);
    // An identity curve is no curve.
    let mut id = correction(vec![linear("1", "0")]);
    if let Value::Struct(m) = &mut id {
        m.insert(
            "MainCurve".into(),
            Value::Seq(["0, 0", "255, 255"].map(text).to_vec()),
        );
    }
    assert!(
        Masks::from_settings(&settings(vec![id])).0.corrections[0]
            .curves
            .is_none()
    );
}

#[test]
fn inactive_corrections_and_disabled_panels_are_skipped() {
    let mut c = correction(vec![linear("1", "0")]);
    if let Value::Struct(m) = &mut c {
        m.insert("CorrectionActive".into(), text("false"));
    }
    assert!(Masks::from_settings(&settings(vec![c])).0.is_empty());
    let mut s = settings(vec![correction(vec![linear("1", "0")])]);
    s.insert(MASK_ENABLE_TAG.into(), Value::Bool(false));
    assert!(Masks::from_settings(&s).0.is_empty());
}
