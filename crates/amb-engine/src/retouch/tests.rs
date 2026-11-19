use super::inpaint::fill_one;
use super::source::find_source;
use super::*;
use crate::buffer::luminance_weights;
use crate::filters::box_blur;

fn log_luma(p: [f32; 3]) -> f32 {
    let lw = luminance_weights();
    (lw[0] * p[0] + lw[1] * p[1] + lw[2] * p[2]).max(1e-5).ln()
}

fn text(s: &str) -> Value {
    Value::Text(s.into())
}

fn area(spot_type: &str, x: f64, y: f64, r: f64, source: Option<(f64, f64)>) -> Value {
    let kind = match spot_type {
        "clone" => SpotKind::Clone,
        "heal_patchmatch" => SpotKind::ContentAware,
        _ => SpotKind::Heal,
    };
    Value::Struct(retouch::new_area(kind, (x, y), r, source, false, 0.2, 1.0))
}

fn settings(areas: Vec<Value>) -> BTreeMap<String, Value> {
    [(RETOUCH_TAG.to_string(), Value::Seq(areas))]
        .into_iter()
        .collect()
}

/// A smooth gradient with a dark blob at (30, 20) and a bright one at (70, 20).
fn scene() -> Image {
    Image::from_fn(100, 40, |x, y| {
        let base = 0.2 + 0.002 * x as f32;
        let d = |cx: f32, cy: f32| ((x as f32 - cx).powi(2) + (y as f32 - cy).powi(2)).sqrt();
        let mut v = base;
        if d(30.0, 20.0) < 3.0 {
            v *= 0.1;
        }
        if d(70.0, 20.0) < 3.0 {
            v *= 3.0;
        }
        [v, v * 0.9, v * 0.8]
    })
}

#[test]
fn heal_removes_a_blob_and_matches_the_surroundings() {
    // Donor 10 px to the right: brighter base there, which the heal corrects.
    let s = settings(vec![area("heal", 0.30, 0.5, 0.05, Some((0.40, 0.5)))]);
    let (spots, notes) = Spots::from_settings(&s);
    assert!(notes.is_empty());
    let mut img = scene();
    apply(&mut img, &spots, (100, 40), None);
    let healed = img.get(30, 20)[0];
    let expect = 0.2 + 0.002 * 30.0;
    assert!((healed - expect).abs() < 0.01, "{healed} vs {expect}");
    // Far away nothing changed.
    assert_eq!(img.get(90, 5), scene().get(90, 5));
}

#[test]
fn clone_copies_the_donor_as_it_is() {
    let s = settings(vec![area("clone", 0.30, 0.5, 0.05, Some((0.40, 0.5)))]);
    let (spots, _) = Spots::from_settings(&s);
    let mut img = scene();
    apply(&mut img, &spots, (100, 40), None);
    // The donor's base is 0.002 × 10 brighter, and a clone keeps that.
    let got = img.get(30, 20)[0];
    assert!((got - (0.2 + 0.002 * 40.0)).abs() < 0.005, "{got}");
}

#[test]
fn content_aware_finds_its_own_donor() {
    let s = settings(vec![area("heal_patchmatch", 0.70, 0.5, 0.05, None)]);
    let (spots, notes) = Spots::from_settings(&s);
    assert_eq!(notes.len(), 1, "{notes:?}");
    let mut img = scene();
    apply(&mut img, &spots, (100, 40), None);
    let healed = img.get(70, 20)[0];
    let expect = 0.2 + 0.002 * 70.0;
    assert!((healed - expect).abs() < 0.02, "{healed} vs {expect}");
    // The search avoids the dark blob as a donor.
    let (sx, _) = find_source(&scene(), (100, 40), None, (0.70, 0.5), 0.05);
    assert!((sx - 0.30).abs() > 0.05, "{sx}");
}

#[test]
fn legacy_retouch_info_and_the_panel_switch() {
    let mut s: BTreeMap<String, Value> = [(
        LEGACY_TAG.to_string(),
        Value::Seq(vec![text(
            "centerX = 0.3, centerY = 0.5, radius = 0.05, sourceState = sourceSetExplicitly, sourceX = 0.4, sourceY = 0.5, spotType = heal",
        )]),
    )]
    .into_iter()
    .collect();
    let (spots, _) = Spots::from_settings(&s);
    assert_eq!(spots.spots.len(), 1);
    s.insert(ENABLE_TAG.into(), Value::Bool(false));
    assert!(Spots::from_settings(&s).0.is_empty());
}

/// Skin-like texture: a soft gradient with fine grain and pores, and a
/// curved dark scar across it. Returns (with scar, clean).
#[test]
fn an_inpainter_fills_the_spot_and_the_grain_comes_back() {
    let (w, h) = (200, 120);
    let (mut img, clean) = textured(w, h);
    // A dark blot to remove.
    for y in 50..70 {
        for x in 90..110 {
            img.set(x, y, [0.02, 0.02, 0.02]);
        }
    }
    let before = img.clone();
    let spot = Spot {
        kind: SpotKind::ContentAware,
        shape: SpotShape::Ellipse {
            centre: (100.0 / w as f64, 60.0 / h as f64),
            size: (14.0 / w as f64, 14.0 / w as f64),
        },
        source: None,
        feather: 0.3,
        opacity: 1.0,
    };
    let frame = Frame::new((w, h), None);
    let p = place(&spot, &frame, w, h).unwrap();
    // A stand-in model: every hole pixel gets the mean of the rest.
    let fill = |rgb: &[f32], mask: &[f32]| -> Result<Vec<f32>, String> {
        let mut sum = [0.0f32; 3];
        let mut n = 0.0;
        for (k, m) in mask.iter().enumerate() {
            if *m == 0.0 {
                (0..3).for_each(|c| sum[c] += rgb[k * 3 + c]);
                n += 1.0;
            }
        }
        let mean = sum.map(|v| v / n);
        Ok((0..rgb.len())
            .map(|i| {
                if mask[i / 3] > 0.0 {
                    mean[i % 3]
                } else {
                    rgb[i]
                }
            })
            .collect())
    };
    fill_one(&mut img, &p, &fill).unwrap();
    // The blot is gone: near the photo's level, not black.
    let centre = img.get(100, 60);
    let around = clean.get(100, 60);
    assert!(
        (centre[0] / around[0] - 1.0).abs() < 0.35,
        "{centre:?} vs {around:?}"
    );
    // Grain: the filled area isn't flat.
    let v: Vec<f32> = (95..105).map(|x| img.get(x, 60)[0]).collect();
    let mean = v.iter().sum::<f32>() / v.len() as f32;
    let spread = (v.iter().map(|x| (x - mean).powi(2)).sum::<f32>() / v.len() as f32).sqrt();
    assert!(spread > 0.004, "grain {spread}");
    // Far from the spot, nothing changed.
    assert_eq!(img.get(10, 10), before.get(10, 10));
}

fn textured(w: usize, h: usize) -> (Image, Image) {
    let hash = |x: usize, y: usize| {
        let mut v = (x as u32).wrapping_mul(374761393) ^ (y as u32).wrapping_mul(668265263);
        v = (v ^ (v >> 13)).wrapping_mul(1274126177);
        (v ^ (v >> 16)) as f32 / u32::MAX as f32
    };
    let clean = Image::from_fn(w, h, |x, y| {
        let (fx, fy) = (x as f32 / w as f32, y as f32 / h as f32);
        // Rounded shading, as on a cheek: not a plane over a long stroke.
        let base = 0.25 + 0.15 * fx + 0.08 * (fx * 12.0).sin() * (fy * 7.0).cos();
        let grain = 1.0 + 0.10 * (hash(x, y) - 0.5) + 0.06 * (hash(x / 3, y / 3) - 0.5);
        let v = base * grain;
        [v, v * 0.78, v * 0.66]
    });
    let scar = Image::from_fn(w, h, |x, y| {
        let mut p = clean.get(x, y);
        // y = 60 + 12·sin(x / 18), 3 px wide, from x = 60 to 140.
        let cy = 60.0 + 12.0 * (x as f32 / 18.0).sin();
        if (60..140).contains(&x) && (y as f32 - cy).abs() < 2.0 {
            p = [p[0] * 0.45, p[1] * 0.4, p[2] * 0.45];
        }
        p
    });
    (scar, clean)
}

/// Leftover blemish, seams at the edge, and lost texture after healing a
/// brushed stroke along the scar.
fn brushed_heal_scores() -> (f64, f64, f64) {
    let (w, h) = (200, 120);
    let (mut img, clean) = textured(w, h);
    let (m, n) = brush::kernel_exponents(0.5);
    let dabs = (60..140)
        .step_by(2)
        .map(|x| brush::Dab {
            x: (x as f64 + 0.5) / w as f64,
            y: (60.0 + 12.0 * (x as f64 / 18.0).sin() + 0.5) / h as f64,
            radius: 6.0 / w as f64,
            deposit: 1.0,
            half_m: (m / 2.0) as f32,
            n: n as f32,
        })
        .collect();
    let spots = Spots {
        spots: vec![Spot {
            kind: SpotKind::Heal,
            shape: SpotShape::Brush(vec![brush::Stroke { dabs, erase: false }]),
            source: None,
            feather: 0.5,
            opacity: 1.0,
        }],
    };
    apply(&mut img, &spots, (w, h), None);
    let ll =
        |im: &Image| -> Vec<f32> { (0..w * h).map(|k| log_luma(im.get(k % w, k / w))).collect() };
    let (a, c) = (ll(&img), ll(&clean));
    let diff: Vec<f32> = a.iter().zip(&c).map(|(a, c)| a - c).collect();
    let low = box_blur(&diff, w, h, 3);
    let inside = |x: usize, y: usize| {
        let cy = 60.0 + 12.0 * (x as f32 / 18.0).sin();
        (62..138).contains(&x) && (y as f32 - cy).abs() < 4.0
    };
    let edge = |x: usize, y: usize| {
        let cy = 60.0 + 12.0 * (x as f32 / 18.0).sin();
        (62..138).contains(&x) && ((y as f32 - cy).abs() - 7.0).abs() < 2.0
    };
    let (mut res, mut nr, mut seam, mut ns) = (0.0, 0, 0.0, 0);
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            if inside(x, y) {
                res += low[y * w + x].abs() as f64;
                nr += 1;
            }
            if edge(x, y) {
                let gx = low[y * w + x + 1] - low[y * w + x - 1];
                let gy = low[(y + 1) * w + x] - low[(y - 1) * w + x];
                seam += (gx * gx + gy * gy).sqrt() as f64;
                ns += 1;
            }
        }
    }
    // Texture: high-pass energy inside, healed against clean.
    let hp = |v: &[f32]| -> f64 {
        let b = box_blur(v, w, h, 2);
        let (mut e, mut k) = (0.0, 0);
        for y in 0..h {
            for x in 0..w {
                if inside(x, y) {
                    e += ((v[y * w + x] - b[y * w + x]) as f64).powi(2);
                    k += 1;
                }
            }
        }
        (e / k as f64).sqrt()
    };
    (res / nr as f64, seam / ns as f64, hp(&a) / hp(&c))
}

#[test]
fn brushed_heal_blends_in() {
    // Before 2026-10-22 (a plane fitted to the edge, a donor for the
    // stroke's bounding circle): residual 0.087, seam 0.027, texture 0.84.
    let (residual, seam, texture) = brushed_heal_scores();
    eprintln!("residual {residual:.4} seam {seam:.4} texture {texture:.3}");
    assert!(residual < 0.03, "the scar shows through: {residual}");
    assert!(seam < 0.012, "a seam at the edge: {seam}");
    assert!(texture > 0.85, "the texture is smeared: {texture}");
}
