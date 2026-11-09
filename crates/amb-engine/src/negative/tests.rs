// SPDX-License-Identifier: GPL-3.0-or-later
//! Tests for negative conversion.

use super::*;

const K: NegativeStrengths = DEFAULT_STRENGTHS;
/// An orange film base as a camera might see it (raw-normalised).
const BASE: [f64; 3] = [0.8, 0.45, 0.25];
/// Dye densities per unit of log scene exposure (colour negative gamma ~0.6,
/// different per layer).
const GAMMA: [f64; 3] = [0.55, 0.6, 0.65];

fn scan_of(rgb: &[f32], width: usize, height: usize) -> Scan<'_> {
    Scan { rgb, width, height }
}

/// Simulate scanning a negative of a scene with linear values `scene` (RGB).
fn negative_of(scene: [f64; 3]) -> [f32; 3] {
    std::array::from_fn(|c| {
        // Density above base grows with log exposure (toe at 0.002 exposure).
        let d = GAMMA[c] * (scene[c].max(0.002) / 0.002).log10();
        (BASE[c] * 10f64.powf(-d)) as f32
    })
}

/// A test scan: a rebate strip of bare base on the left, then a scene of
/// grey ramps and colour patches.
fn scan(w: usize, h: usize) -> Vec<f32> {
    let mut rgb = vec![0.0f32; w * h * 3];
    for y in 0..h {
        for x in 0..w {
            let p = if x < w / 20 {
                BASE.map(|v| v as f32)
            } else {
                let t = x as f64 / w as f64;
                let v = 0.005 * (1.0f64 / 0.005).powf(t); // 0.005..1 log ramp
                // Colour patches stay inside the grey range, as in real scenes
                // (auto levels assume the deepest shadow and brightest highlight
                // are close to neutral).
                let m = 0.03 + 0.4 * t;
                let scene = match (y * 4) / h {
                    0 | 1 => [v, v, v],
                    2 => [m, m * 0.5, m * 0.3],
                    _ => [m * 0.3, m * 0.6, m],
                };
                negative_of(scene)
            };
            rgb[(y * w + x) * 3..(y * w + x) * 3 + 3].copy_from_slice(&p);
        }
    }
    rgb
}

/// A scene that is mostly flat bright sky (`sky` of the frame) over a
/// detailed, darker subject: a checker of textures around `subject`.
fn sky_scene(w: usize, h: usize, sky: f64, subject: f64) -> Vec<f32> {
    let mut rgb = vec![0.0f32; w * h * 3];
    for y in 0..h {
        for x in 0..w {
            let scene = if (y as f64) < sky * h as f64 {
                [0.6, 0.6, 0.6]
            } else {
                // Texture: small blocks a stop apart.
                let t = if (x / 6 + y / 6) % 2 == 0 { 1.4 } else { 0.7 };
                [subject * t; 3]
            };
            rgb[(y * w + x) * 3..][..3].copy_from_slice(&negative_of(scene));
        }
    }
    rgb
}

/// The print's value (mean of the layers) at a pixel after conversion.
fn printed(rgb: &[f32], w: usize, h: usize, n: &Negative, x: usize, y: usize) -> f32 {
    let a = resolve(n, scan_of(rgb, w, h), scan_of(rgb, w, h), &K);
    let mut out = rgb.to_vec();
    apply(&mut out, &Conversion::new(n, a, &K));
    let p = &out[(y * w + x) * 3..][..3];
    (p[0] + p[1] + p[2]) / 3.0
}

#[test]
fn auto_density_meters_the_subject_not_the_sky() {
    let (w, h) = (240, 240);
    // The same subject under a small and a large sky.
    let small = sky_scene(w, h, 0.2, 0.05);
    let large = sky_scene(w, h, 0.75, 0.05);
    let plain = Negative {
        enabled: true,
        ..Default::default()
    };
    let dense = Negative {
        auto_density: true,
        ..plain.clone()
    };
    // Sample a block of the subject's average (left-right averaged over texture).
    let subject = |rgb: &[f32], n: &Negative| {
        (0..12)
            .map(|i| printed(rgb, w, h, n, 100 + i, h - 20))
            .sum::<f32>()
            / 12.0
    };
    let plain_shift = (subject(&small, &plain) / subject(&large, &plain))
        .log2()
        .abs();
    let dense_shift = (subject(&small, &dense) / subject(&large, &dense))
        .log2()
        .abs();
    eprintln!(
        "subject moved by the sky: {plain_shift:.2} stops plain, {dense_shift:.2} with Auto Density"
    );
    assert!(
        dense_shift < plain_shift * 0.6,
        "the sky moves the subject less: {plain_shift:.2} stops plain, {dense_shift:.2} with Auto Density"
    );
}

#[test]
fn auto_grade_keeps_a_soft_frame_soft() {
    let (w, h) = (240, 240);
    // A flat (foggy) frame: little texture, a narrow range.
    let mut rgb = vec![0.0f32; w * h * 3];
    for y in 0..h {
        for x in 0..w {
            let t = if (x / 6 + y / 6) % 2 == 0 { 1.15 } else { 0.87 };
            let v = 0.12 * t * (1.0 + 0.3 * y as f64 / h as f64);
            rgb[(y * w + x) * 3..][..3].copy_from_slice(&negative_of([v; 3]));
        }
    }
    let plain = Negative {
        enabled: true,
        ..Default::default()
    };
    let graded = Negative {
        auto_grade: true,
        ..plain.clone()
    };
    let contrast = |n: &Negative| {
        let a = printed(&rgb, w, h, n, 30, 30);
        let b = printed(&rgb, w, h, n, 36, 30);
        (a.max(b) / a.min(b)).log2()
    };
    let (c0, c1) = (contrast(&plain), contrast(&graded));
    eprintln!("texture contrast: {c0:.2} stops plain, {c1:.2} with Auto Grade");
    assert!(
        c1 < c0 * 0.8,
        "softer with Auto Grade: {c0:.2} vs {c1:.2} stops"
    );
    assert!(c1 > 0.0, "but not grey");
}

#[test]
fn base_estimate_finds_the_rebate() {
    let (w, h) = (400, 200);
    let rgb = scan(w, h);
    let base = estimate_base(&rgb, w, h, NegativeType::Color, &K);
    for c in 0..3 {
        assert!((base[c] - BASE[c]).abs() < 0.01, "{base:?}");
    }
}

#[test]
fn auto_conversion_gives_a_neutral_monotone_positive() {
    let (w, h) = (400, 200);
    let mut rgb = scan(w, h);
    let n = Negative {
        enabled: true,
        ..Default::default()
    };
    let a = resolve(&n, scan_of(&rgb, w, h), scan_of(&rgb, w, h), &K);
    let conv = Conversion::new(&n, a, &K);
    apply(&mut rgb, &conv);
    // Grey rows stay neutral and get brighter left → right.
    let y = h / 8;
    let mut last = 0.0;
    for x in (w / 10..w - w / 20).step_by(10) {
        let p = &rgb[(y * w + x) * 3..(y * w + x) * 3 + 3];
        let mean = (p[0] + p[1] + p[2]) / 3.0;
        assert!(mean > last, "monotone at x={x}");
        last = mean;
        let spread =
            p.iter().cloned().fold(f32::MIN, f32::max) - p.iter().cloned().fold(f32::MAX, f32::min);
        assert!(spread < 0.12 * mean + 1e-3, "neutral at x={x}: {p:?}");
    }
    // The warm patch row is warm, the cool one cool.
    let at = |y: usize| {
        let x = w * 3 / 4;
        let i = (y * w + x) * 3;
        [rgb[i], rgb[i + 1], rgb[i + 2]]
    };
    let warm = at(h * 5 / 8);
    let cool = at(h * 7 / 8);
    assert!(warm[0] > warm[2], "{warm:?}");
    assert!(cool[2] > cool[0], "{cool:?}");
}

#[test]
fn a_slide_is_balanced_but_not_inverted() {
    // A slide of grey ramps and a warm row, lit by a bluish light, with
    // slide film's steep contrast (transmittance ∝ scene^1.5).
    let (w, h) = (300, 120);
    let light = [0.55, 0.75, 0.95];
    let mut rgb = vec![0.0f32; w * h * 3];
    for y in 0..h {
        for x in 0..w {
            let t = x as f64 / w as f64;
            let v = 0.01 * (1.0f64 / 0.01).powf(t);
            // The warm row stays inside the grey range, as auto levels assume.
            let m = 0.05 + 0.5 * t;
            let scene = if y < h * 2 / 3 {
                [v; 3]
            } else {
                [m, m * 0.6, m * 0.4]
            };
            for c in 0..3 {
                rgb[(y * w + x) * 3 + c] = (light[c] * scene[c].powf(1.5)) as f32;
            }
        }
    }
    let n = Negative {
        enabled: true,
        kind: NegativeType::Positive,
        ..Default::default()
    };
    let a = resolve(&n, scan_of(&rgb, w, h), scan_of(&rgb, w, h), &K);
    assert_eq!(a.base, [1.0; 3]);
    apply(&mut rgb, &Conversion::new(&n, a, &K));
    // Brighter on the slide stays brighter, and the grey rows are grey.
    let y = h / 4;
    let mut last = 0.0;
    for x in (w / 10..w - w / 10).step_by(10) {
        let p = &rgb[(y * w + x) * 3..(y * w + x) * 3 + 3];
        let mean = (p[0] + p[1] + p[2]) / 3.0;
        assert!(mean > last, "monotone at x={x}");
        last = mean;
        let spread =
            p.iter().cloned().fold(f32::MIN, f32::max) - p.iter().cloned().fold(f32::MAX, f32::min);
        assert!(spread < 0.12 * mean + 1e-3, "neutral at x={x}: {p:?}");
    }
    let i = ((h * 5 / 6) * w + w / 2) * 3;
    assert!(rgb[i] > rgb[i + 2], "warm stays warm: {:?}", &rgb[i..i + 3]);
}

#[test]
fn the_frame_is_found_inside_rebate_and_holder() {
    // The test scan (rebate strip on the left) inside a black holder edge.
    let (w, h) = (400, 200);
    let inner = scan(w, h);
    let (pw, ph) = (w + 40, h + 30);
    let mut rgb = vec![0.001f32; pw * ph * 3];
    for y in 0..h {
        let (o, i) = (((y + 10) * pw + 30) * 3, y * w * 3);
        rgb[o..o + w * 3].copy_from_slice(&inner[i..i + w * 3]);
    }
    let (l, t, r, b) = detect_frame(&rgb, pw, ph, NegativeType::Color, &K).unwrap();
    // Left: past the holder (30 px) and the rebate (w/20 = 20 px).
    let px = |f: f64, n: usize| f * n as f64;
    assert!((px(l, pw) - 50.0).abs() < 6.0, "left {}", px(l, pw));
    assert!((px(t, ph) - 10.0).abs() < 4.0, "top {}", px(t, ph));
    assert!((px(r, pw) - 430.0).abs() < 4.0, "right {}", px(r, pw));
    assert!((px(b, ph) - 210.0).abs() < 4.0, "bottom {}", px(b, ph));
    // A scan that is all photo has no frame to find.
    let photo: Vec<f32> = inner
        .chunks(3)
        .enumerate()
        .flat_map(|(i, p)| {
            if i % w < w / 20 {
                negative_of([0.1; 3])
            } else {
                [p[0], p[1], p[2]]
            }
        })
        .collect();
    assert_eq!(detect_frame(&photo, w, h, NegativeType::Color, &K), None);
}

#[test]
fn stored_analysis_is_used_when_auto_is_off() {
    let (w, h) = (200, 100);
    let rgb = scan(w, h);
    let n = Negative {
        enabled: true,
        auto: false,
        base: Some([0.5, 0.5, 0.5]),
        midtone: Some(0.4),
        range: 1.5,
        ..Default::default()
    };
    let a = resolve(&n, scan_of(&rgb, w, h), scan_of(&rgb, w, h), &K);
    assert_eq!(a.base, [0.5, 0.5, 0.5]);
    assert_eq!(a.midtone, 0.4);
    // Levels were missing, so they come from the fixed range: green
    // spans it, the other layers keep their measured share.
    assert_eq!(a.density_low, [K.fixed_floor; 3]);
    assert!((a.density_high[1] - (K.fixed_floor + 1.5)).abs() < 1e-9);
    let m = resolve(
        &Negative {
            auto: true,
            ..n.clone()
        },
        scan_of(&rgb, w, h),
        scan_of(&rgb, w, h),
        &K,
    );
    let share = |a: &Analysis, c: usize| {
        (a.density_high[c] - a.density_low[c]) / (a.density_high[1] - a.density_low[1])
    };
    for c in [0, 2] {
        assert!((share(&a, c) - share(&m, c)).abs() < 1e-9);
    }
    // Auto measures the levels, keeping the stored base.
    let auto = resolve(
        &Negative {
            auto: true,
            ..n.clone()
        },
        scan_of(&rgb, w, h),
        scan_of(&rgb, w, h),
        &K,
    );
    assert_eq!(auto.base, [0.5, 0.5, 0.5]);
    assert_ne!(auto.density_high, a.density_high);
    // Without a stored base, it is measured from the scan.
    let measured = resolve(
        &Negative { base: None, ..n },
        scan_of(&rgb, w, h),
        scan_of(&rgb, w, h),
        &K,
    );
    assert!(
        (measured.base[0] - BASE[0]).abs() < 0.01,
        "{:?}",
        measured.base
    );
}

#[test]
fn auto_off_prints_frames_alike() {
    // The same scene on two frames, the second exposed a stop more.
    let (w, h) = (300, 120);
    let frame = |gain: f64| -> Vec<f32> {
        let mut rgb = scan(w, h);
        for y in 0..h {
            for x in w / 20..w {
                let t = x as f64 / w as f64;
                let v = 0.005 * (1.0f64 / 0.005).powf(t) * gain;
                let p = negative_of([v; 3]);
                rgb[(y * w + x) * 3..(y * w + x) * 3 + 3].copy_from_slice(&p);
            }
        }
        rgb
    };
    let (one, two) = (frame(1.0), frame(2.0));
    let grey = negative_of([0.05; 3]);
    let render = |n: &Negative, rgb: &[f32]| {
        let a = resolve(n, scan_of(rgb, w, h), scan_of(rgb, w, h), &K);
        Conversion::new(n, a, &K).apply(grey)[1]
    };
    let off = Negative {
        enabled: true,
        auto: false,
        ..Default::default()
    };
    // Auto off: the same film prints the same on both frames.
    assert!((render(&off, &one) / render(&off, &two) - 1.0).abs() < 0.01);
    // ... so the frame exposed a stop more prints about a stop brighter
    // where the scene is the same (a grey patch shot at 0.05 and at 0.1).
    let brighter = negative_of([0.1; 3]);
    let a = resolve(&off, scan_of(&two, w, h), scan_of(&two, w, h), &K);
    let ratio = Conversion::new(&off, a, &K).apply(brighter)[1] / render(&off, &two);
    assert!((1.6..2.6).contains(&ratio), "{ratio}");
    // Auto evens out each frame's exposure instead.
    let auto = Negative {
        enabled: true,
        ..Default::default()
    };
    let a1 = resolve(&auto, scan_of(&one, w, h), scan_of(&one, w, h), &K);
    let a2 = resolve(&auto, scan_of(&two, w, h), scan_of(&two, w, h), &K);
    let p1 = Conversion::new(&auto, a1, &K).apply(grey)[1];
    let p2 = Conversion::new(&auto, a2, &K).apply(brighter)[1];
    assert!((p1 / p2 - 1.0).abs() < 0.2, "{p1} {p2}");
}

#[test]
fn the_base_comes_from_the_whole_scan_and_the_levels_from_the_crop() {
    // The test scan in a black holder: the holder is denser than any
    // film, so levels measured over it put the highlights too far.
    let (w, h) = (400, 200);
    let inner = scan(w, h);
    let (pw, ph) = (w + 80, h + 80);
    let mut rgb = vec![0.0005f32; pw * ph * 3];
    for y in 0..h {
        let (o, i) = (((y + 40) * pw + 40) * 3, y * w * 3);
        rgb[o..o + w * 3].copy_from_slice(&inner[i..i + w * 3]);
    }
    // The crop: the photo, without the rebate strip.
    let (cw, ch) = (w - w / 20, h);
    let mut crop = Vec::with_capacity(cw * ch * 3);
    for y in 0..h {
        let i = (y * w + w / 20) * 3;
        crop.extend_from_slice(&inner[i..i + cw * 3]);
    }
    let n = Negative {
        enabled: true,
        ..Default::default()
    };
    let whole = scan_of(&rgb, pw, ph);
    let cropped = resolve(&n, whole, scan_of(&crop, cw, ch), &K);
    let uncropped = resolve(&n, whole, whole, &K);
    for (c, base) in BASE.iter().enumerate() {
        assert!((cropped.base[c] - base).abs() < 0.01, "{:?}", cropped.base);
        assert!(cropped.density_high[c] < uncropped.density_high[c] - 0.3);
    }
    // Measured on the crop alone, the base would be the thinnest shadow.
    let alone = resolve(&n, scan_of(&crop, cw, ch), scan_of(&crop, cw, ch), &K);
    assert!(alone.base[2] < BASE[2] - 0.02, "{:?}", alone.base);
}

/// The test scan, exposed a stop darker, with sprocket holes in its
/// rebate strip that let the light through unclipped: the light is
/// `gap` density brighter than the film base.
fn holes(w: usize, h: usize, gap: [f64; 3]) -> (Vec<f32>, [f64; 3]) {
    let mut rgb: Vec<f32> = scan(w, h).iter().map(|v| v * 0.5).collect();
    let base = BASE.map(|b| b * 0.5);
    let rebate = w / 20;
    for y in 0..h {
        for x in rebate / 4..rebate * 3 / 4 {
            if y % 16 < 10 {
                for c in 0..3 {
                    rgb[(y * w + x) * 3 + c] = (base[c] * 10f64.powf(gap[c])) as f32;
                }
            }
        }
    }
    (rgb, base)
}

#[test]
fn the_base_is_not_the_light_through_the_perforations() {
    let (w, h) = (400, 200);
    let close = |found: [f64; 3], base: [f64; 3]| {
        (0..3).all(|c| (found[c] - base[c]).abs() < 0.005 * base[c].max(0.1) / 0.1)
    };
    // Colour film: the orange base is far denser in blue than in red.
    let (rgb, base) = holes(w, h, [0.25, 0.6, 0.85]);
    let found = estimate_base(&rgb, w, h, NegativeType::Color, &K);
    assert!(close(found, base), "{found:?} {base:?}");
    // Black-and-white film: a grey base, found by the holes it surrounds.
    let (rgb, base) = holes(w, h, [0.25; 3]);
    let found = estimate_base(&rgb, w, h, NegativeType::BlackAndWhite, &K);
    assert!(close(found, base), "{found:?} {base:?}");
    // Without holes the rebate is still the base, though the picture
    // beside it is dark: it is neither around it nor orange against it.
    let rgb = scan(w, h);
    let found = estimate_base(&rgb, w, h, NegativeType::Color, &K);
    assert!(close(found, BASE), "{found:?}");
    let found = estimate_base(&rgb, w, h, NegativeType::BlackAndWhite, &K);
    assert!(close(found, BASE), "{found:?}");
}

/// A grey ramp on film whose blue layer is off by `shift(l)` density at
/// log exposure `l` (0..2.6), its ends matching the others.
fn crossed(w: usize, h: usize, shift: impl Fn(f64) -> f64) -> Vec<f32> {
    let mut rgb = vec![0.0f32; w * h * 3];
    for y in 0..h {
        for x in 0..w {
            let l = 2.6 * x as f64 / w as f64;
            for c in 0..3 {
                let d = GAMMA[1] * l + if c == 2 { shift(l) } else { 0.0 };
                rgb[(y * w + x) * 3 + c] = (BASE[c] * 10f64.powf(-d)) as f32;
            }
        }
    }
    rgb
}

/// The blue cast (|ln b/g|) at a fraction `at` along the converted ramp.
fn cast(rgb: &[f32], w: usize, h: usize, expired: bool, at: f64) -> f64 {
    let n = Negative {
        enabled: true,
        base: Some(BASE),
        expired,
        ..Default::default()
    };
    let a = resolve(&n, scan_of(rgb, w, h), scan_of(rgb, w, h), &K);
    let c = Conversion::new(&n, a, &K);
    let mut out = rgb.to_vec();
    apply(&mut out, &c);
    let i = (h / 2 * w + (w as f64 * at) as usize) * 3;
    (out[i + 2] / out[i + 1]).ln().abs() as f64
}

#[test]
fn expired_film_matches_the_layers_from_shadows_to_highlights() {
    let (w, h) = (300, 120);
    // The blue layer bows: its midtones are too dense (a crossover).
    let bowed = crossed(w, h, |l| 0.12 * l * (2.6 - l));
    let (plain, mended) = (
        cast(&bowed, w, h, false, 0.5),
        cast(&bowed, w, h, true, 0.5),
    );
    assert!(mended < plain * 0.5, "plain {plain}, expired {mended}");
    // An S: blue too dense in the shadows, too thin in the highlights.
    let s = crossed(w, h, |l| 0.06 * (std::f64::consts::TAU * l / 2.6).sin());
    for at in [0.25, 0.75] {
        let (plain, mended) = (cast(&s, w, h, false, at), cast(&s, w, h, true, at));
        assert!(
            mended < plain * 0.5,
            "at {at}: plain {plain}, expired {mended}"
        );
    }
    // Black-and-white film has no layers to even out.
    let n = Negative {
        enabled: true,
        kind: NegativeType::BlackAndWhite,
        expired: true,
        ..Default::default()
    };
    let a = resolve(&n, scan_of(&bowed, w, h), scan_of(&bowed, w, h), &K);
    assert_eq!(a.crossover, NO_CROSSOVER);
}

/// A sunset-like scene: greys from shadow to midtone over half the
/// frame, a bright orange sky over a quarter, dark silhouettes below.
fn sunset(w: usize, h: usize, sky: bool) -> Vec<f32> {
    let mut rgb = vec![0.0f32; w * h * 3];
    for y in 0..h {
        for x in 0..w {
            let t = x as f64 / w as f64;
            let scene = match (y * 4) / h {
                0 | 1 => [0.02 + 0.3 * t; 3],
                2 if sky => {
                    let s = 0.4 + 0.6 * t;
                    [s, 0.55 * s, 0.25 * s]
                }
                2 => [0.05 + 0.25 * t; 3],
                _ => [0.006 + 0.004 * t; 3],
            };
            rgb[(y * w + x) * 3..(y * w + x) * 3 + 3].copy_from_slice(&negative_of(scene));
        }
    }
    rgb
}

/// How far a pixel is from grey (|ln r/g| + |ln b/g|) and its red to
/// blue ratio, after converting with Cast removal at `strength`.
fn after_cast_removal(
    rgb: &[f32],
    w: usize,
    h: usize,
    strength: f64,
    at: (f64, f64),
) -> (f64, f64) {
    let n = Negative {
        enabled: true,
        base: Some(BASE),
        cast_removal: strength,
        ..Default::default()
    };
    let a = resolve(&n, scan_of(rgb, w, h), scan_of(rgb, w, h), &K);
    let mut out = rgb.to_vec();
    apply(&mut out, &Conversion::new(&n, a, &K));
    let i = (((h as f64 * at.1) as usize) * w + (w as f64 * at.0) as usize) * 3;
    let (r, g, b) = (out[i] as f64, out[i + 1] as f64, out[i + 2] as f64);
    ((r / g).ln().abs() + (b / g).ln().abs(), r / b)
}

#[test]
fn cast_removal_finds_the_greys_under_an_orange_sky() {
    let (w, h) = (300, 200);
    let scan = sunset(w, h, true);
    for x in [0.3, 0.6, 0.9] {
        let (plain, _) = after_cast_removal(&scan, w, h, 0.0, (x, 0.25));
        let (mended, _) = after_cast_removal(&scan, w, h, 100.0, (x, 0.25));
        assert!(
            mended < plain * 0.4,
            "grey at {x}: plain {plain}, cast removal {mended}"
        );
        assert!(mended < 0.05, "grey at {x}: {mended}");
    }
    // The sky stays orange, more so than when the levels made it grey.
    let (_, plain) = after_cast_removal(&scan, w, h, 0.0, (0.5, 0.625));
    let (_, mended) = after_cast_removal(&scan, w, h, 100.0, (0.5, 0.625));
    assert!(
        mended > plain && mended > 1.5,
        "sky r/b: plain {plain}, cast removal {mended}"
    );
    // Half way is about half the change.
    let (half, _) = after_cast_removal(&scan, w, h, 50.0, (0.6, 0.25));
    let (plain, _) = after_cast_removal(&scan, w, h, 0.0, (0.6, 0.25));
    assert!(
        half < plain * 0.8 && half > plain * 0.2,
        "half {half}, plain {plain}"
    );
    // A frame that is grey already stays grey.
    let grey = sunset(w, h, false);
    let (plain, _) = after_cast_removal(&grey, w, h, 0.0, (0.6, 0.25));
    let (mended, _) = after_cast_removal(&grey, w, h, 100.0, (0.6, 0.25));
    assert!(
        mended < plain + 0.02,
        "grey frame: plain {plain}, cast removal {mended}"
    );
    // A frame with few greys in it is left alone.
    let mut colourful = sunset(w, h, true);
    for y in h / 20..h {
        for x in 0..w {
            let (t, u) = (x as f64 / w as f64, y as f64 / h as f64);
            let hue = std::f64::consts::TAU * (t + 0.37 * u);
            let v = 0.02 + 0.3 * u;
            let scene = [0.0, 2.1, 4.2].map(|o| v * (1.0 + 0.8 * (hue + o).cos()));
            colourful[(y * w + x) * 3..(y * w + x) * 3 + 3].copy_from_slice(&negative_of(scene));
        }
    }
    let n = Negative {
        enabled: true,
        base: Some(BASE),
        ..Default::default()
    };
    let removing = Negative {
        cast_removal: 100.0,
        ..n.clone()
    };
    let few = scan_of(&colourful, w, h);
    assert_eq!(resolve(&removing, few, few, &K), resolve(&n, few, few, &K));
    // Stored levels already hold it.
    let plain = Negative {
        enabled: true,
        base: Some(BASE),
        ..Default::default()
    };
    let a = resolve(&plain, scan_of(&scan, w, h), scan_of(&scan, w, h), &K);
    let n = Negative {
        enabled: true,
        auto: false,
        base: Some(BASE),
        density_low: Some(a.density_low),
        density_high: Some(a.density_high),
        midtone: Some(a.midtone),
        cast_removal: 100.0,
        ..Default::default()
    };
    let stored = resolve(&n, scan_of(&scan, w, h), scan_of(&scan, w, h), &K);
    assert_eq!(stored.density_high, a.density_high);
}

#[test]
fn controls_move_the_image_the_right_way() {
    let a = Analysis {
        base: BASE,
        density_low: [0.0; 3],
        density_high: GAMMA.map(|g| g * 2.7),
        midtone: 0.5,
        crossover: NO_CROSSOVER,
    };
    let grey = negative_of([0.05; 3]);
    let render = |n: Negative| Conversion::new(&n, a, &K).apply(grey);
    let base = render(Negative::default());
    let brighter = render(Negative {
        exposure: 1.0,
        ..Default::default()
    });
    assert!((brighter[1] / base[1] - 2.0).abs() < 1e-3, "exposure is EV");
    let redder = render(Negative {
        balance: [50.0, 0.0, 0.0],
        ..Default::default()
    });
    assert!(redder[0] > base[0] && (redder[1] - base[1]).abs() < 1e-6);
    // Contrast: a shadow gets darker, a highlight brighter.
    let shadow = negative_of([0.01; 3]);
    let high = negative_of([0.5; 3]);
    let plain = Conversion::new(&Negative::default(), a, &K);
    let punchy = Conversion::new(
        &Negative {
            contrast: 50.0,
            ..Default::default()
        },
        a,
        &K,
    );
    assert!(punchy.apply(shadow)[1] < plain.apply(shadow)[1]);
    assert!(punchy.apply(high)[1] > plain.apply(high)[1]);
    // Shoulder compresses highlights.
    let soft = Conversion::new(
        &Negative {
            shoulder: 100.0,
            ..Default::default()
        },
        a,
        &K,
    );
    assert!(soft.apply(high)[1] < plain.apply(high)[1]);
}

#[test]
fn stocks_are_valid_and_black_and_white_is_grey() {
    let mut ids = std::collections::BTreeSet::new();
    for s in STOCKS {
        assert!(ids.insert(s.id), "duplicate id {}", s.id);
        for row in s.crosstalk {
            assert!((row.iter().sum::<f64>() - 1.0).abs() < 1e-9, "{}", s.id);
        }
        assert!((s.mix.iter().sum::<f64>() - 1.0).abs() < 1e-9, "{}", s.id);
        assert!(s.contrast > 0.0 && s.saturation > 0.0);
    }
    assert_eq!(stock("VIVID-100").id, "vivid-100");
    assert_eq!(stock("no-such-film").id, "neutral");

    let a = Analysis {
        base: BASE,
        density_low: [0.0; 3],
        density_high: GAMMA.map(|g| g * 2.7),
        midtone: 0.5,
        crossover: NO_CROSSOVER,
    };
    let n = Negative {
        stock: "bw-classic-400".into(),
        ..Default::default()
    };
    let p = Conversion::new(&n, a, &K).apply(negative_of([0.1, 0.05, 0.02]));
    assert!(p[0] == p[1] && p[1] == p[2], "{p:?}");
    // Stock amount 0 = neutral conversion.
    let vivid = |amount: f64| {
        Conversion::new(
            &Negative {
                stock: "vivid-100".into(),
                stock_amount: amount,
                ..Default::default()
            },
            a,
            &K,
        )
        .apply(negative_of([0.1, 0.05, 0.02]))
    };
    let neutral =
        Conversion::new(&Negative::default(), a, &K).apply(negative_of([0.1, 0.05, 0.02]));
    assert_eq!(vivid(0.0), neutral);
    assert_ne!(vivid(100.0), neutral);
}
