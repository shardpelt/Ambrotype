use super::filters::{downscale, min_filter};
use super::*;
use crate::buffer::{Image, luminance_weights};

const AIR: [f32; 3] = [0.85, 0.9, 0.95];

/// Haze-free scene radiance: saturated colours whose darkest channel is near
/// zero in every small patch (the dark channel prior holds), with texture.
fn scene(x: usize, y: usize) -> [f32; 3] {
    const PALETTE: [[f32; 3]; 5] = [
        [0.55, 0.05, 0.008],
        [0.01, 0.35, 0.06],
        [0.006, 0.08, 0.45],
        [0.40, 0.30, 0.005],
        [0.012, 0.012, 0.012],
    ];
    let c = PALETTE[(x / 2 + 3 * (y / 2)) % 5];
    let shade = 0.7 + 0.3 * ((x as f32 * 0.21).sin() * (y as f32 * 0.17).cos()).abs();
    c.map(|v| v * shade)
}

/// Transmission of the synthetic hazy image: a pure-haze "sky" band on top,
/// then haze thinning from left (t = 0.35) to right (t = 0.9).
fn transmission(x: usize, y: usize, w: usize, h: usize) -> f32 {
    if y < h / 6 {
        0.0
    } else {
        0.35 + 0.55 * x as f32 / (w - 1) as f32
    }
}

fn hazy(w: usize, h: usize) -> Image {
    Image::from_fn(w, h, |x, y| {
        let t = transmission(x, y, w, h);
        let j = scene(x, y);
        [0, 1, 2].map(|c| j[c] * t + AIR[c] * (1.0 - t))
    })
}

/// Pixels below the sky band (where a ground truth radiance exists).
fn ground(img: &Image) -> Vec<[f32; 3]> {
    (img.height / 6 + 2..img.height)
        .flat_map(|y| (0..img.width).map(move |x| (x, y)))
        .map(|(x, y)| img.get(x, y))
        .collect()
}

fn mean(px: &[[f32; 3]]) -> [f32; 3] {
    let mut s = [0.0f64; 3];
    for p in px {
        for c in 0..3 {
            s[c] += p[c] as f64;
        }
    }
    s.map(|v| (v / px.len() as f64) as f32)
}

/// Standard deviation over all channels (a contrast measure).
fn spread(px: &[[f32; 3]]) -> f32 {
    let m = mean(px);
    let var: f64 = px
        .iter()
        .map(|p| (0..3).map(|c| ((p[c] - m[c]) as f64).powi(2)).sum::<f64>())
        .sum::<f64>()
        / (3 * px.len()) as f64;
    var.sqrt() as f32
}

/// Mean absolute difference between horizontal neighbours below the sky
/// band (local contrast, insensitive to the left-to-right haze gradient).
fn local_contrast(img: &Image) -> f32 {
    let mut s = 0.0f64;
    let mut n = 0;
    for y in img.height / 6 + 2..img.height {
        for x in 0..img.width - 1 {
            s += distance(img.get(x, y), img.get(x + 1, y)) as f64;
            n += 1;
        }
    }
    (s / n as f64) as f32
}

fn rms_error(a: &[[f32; 3]], b: &[[f32; 3]]) -> f32 {
    let s: f64 = a
        .iter()
        .zip(b)
        .map(|(p, q)| (0..3).map(|c| ((p[c] - q[c]) as f64).powi(2)).sum::<f64>())
        .sum();
    (s / (3 * a.len()) as f64).sqrt() as f32
}

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    (0..3).map(|c| (a[c] - b[c]).powi(2)).sum::<f32>().sqrt()
}

fn dehazed(img: &Image, amount: f64) -> Image {
    let mut out = img.clone();
    apply(&mut out, amount, &DehazeStrengths::default());
    out
}

fn assert_valid(img: &Image, what: &str) {
    assert!(
        img.rgb.iter().all(|v| v.is_finite() && *v >= 0.0),
        "{what}: non-finite or negative value"
    );
}

#[test]
fn zero_amount_is_an_exact_no_op() {
    let img = hazy(97, 61);
    let mut out = img.clone();
    apply(&mut out, 0.0, &DehazeStrengths::default());
    assert_eq!(out.rgb, img.rgb);
    apply(&mut out, f64::NAN, &DehazeStrengths::default());
    assert_eq!(out.rgb, img.rgb);
}

#[test]
fn positive_amount_recovers_the_hazy_scene() {
    let (w, h) = (180, 120);
    let img = hazy(w, h);
    let truth = ground(&Image::from_fn(w, h, scene));
    let out = dehazed(&img, 100.0);
    let (before, after) = (ground(&img), ground(&out));
    let (err_before, err_after) = (rms_error(&before, &truth), rms_error(&after, &truth));
    assert!(
        err_after < 0.5 * err_before,
        "error to J: {err_before} → {err_after}"
    );
    let (c_before, c_after) = (local_contrast(&img), local_contrast(&out));
    let c_truth = local_contrast(&Image::from_fn(w, h, scene));
    assert!(
        c_after > 1.5 * c_before && c_after < 1.3 * c_truth,
        "contrast: {c_before} → {c_after} (haze-free {c_truth})"
    );
    // Haze removal darkens (the airlight veil is taken away).
    let lum = luminance_weights();
    assert!(dot(lum, mean(&after)) < dot(lum, mean(&before)));
}

#[test]
fn removal_is_stronger_where_the_haze_is_thicker() {
    let (w, h) = (180, 120);
    let img = hazy(w, h);
    let out = dehazed(&img, 100.0);
    // Mean absolute change in a thick-haze column band vs a thin-haze one.
    let change = |x0: usize, x1: usize| {
        let mut s = 0.0;
        let mut n = 0;
        for y in h / 6 + 2..h {
            for x in x0..x1 {
                let (a, b) = (img.get(x, y), out.get(x, y));
                s += distance(a, b);
                n += 1;
            }
        }
        s / n as f32
    };
    let thick = change(5, 35);
    let thin = change(w - 35, w - 5);
    assert!(thick > 2.0 * thin, "thick {thick} thin {thin}");
}

#[test]
fn effect_grows_with_the_amount() {
    let img = hazy(120, 90);
    let base = local_contrast(&img);
    let c50 = local_contrast(&dehazed(&img, 50.0));
    let c100 = local_contrast(&dehazed(&img, 100.0));
    assert!(base < c50 && c50 < c100, "{base} {c50} {c100}");
    // And a tiny amount changes the image only a little (no jump at 0).
    let tiny = dehazed(&img, 0.5);
    let max_change = img
        .rgb
        .iter()
        .zip(&tiny.rgb)
        .map(|(a, b)| (a - b).abs())
        .fold(0.0f32, f32::max);
    assert!(max_change < 0.02, "max change at +0.5: {max_change}");
}

#[test]
fn negative_amount_adds_haze() {
    let (w, h) = (150, 100);
    // A clear scene with a bright region so the airlight is a light grey.
    let img = Image::from_fn(w, h, |x, y| {
        if x < 20 && y < 20 {
            [0.9, 0.9, 0.9]
        } else {
            scene(x, y)
        }
    });
    let all =
        |i: &Image| -> Vec<[f32; 3]> { i.rgb.chunks(3).map(|p| [p[0], p[1], p[2]]).collect() };
    let before = all(&img);
    let fog50 = all(&dehazed(&img, -50.0));
    let fog100 = all(&dehazed(&img, -100.0));
    let grey = [0.9f32; 3];
    let (d0, d50, d100) = (
        distance(mean(&before), grey),
        distance(mean(&fog50), grey),
        distance(mean(&fog100), grey),
    );
    assert!(
        d100 < d50 && d50 < d0,
        "mean → fog colour: {d0} {d50} {d100}"
    );
    let (c0, c50, c100) = (spread(&before), spread(&fog50), spread(&fog100));
    assert!(c100 < c50 && c50 < c0, "contrast: {c0} {c50} {c100}");
    assert!(c100 < 0.7 * c0, "contrast at -100: {c0} → {c100}");
}

#[test]
fn haze_free_regions_barely_change() {
    let (w, h) = (150, 100);
    // Every patch holds a near-black channel: no haze to remove, except the
    // white block, which defines the airlight.
    let img = Image::from_fn(w, h, |x, y| {
        if x < 20 && y < 20 {
            [0.9, 0.9, 0.9]
        } else {
            scene(x, y)
        }
    });
    let out = dehazed(&img, 100.0);
    let mut worst = 0.0f32;
    let mut total = 0.0f32;
    let mut n = 0;
    for y in 0..h {
        for x in 0..w {
            if x < 40 && y < 40 {
                continue;
            }
            let (a, b) = (img.get(x, y), out.get(x, y));
            let rel = distance(a, b) / (distance(a, [0.0; 3]) + 0.02);
            worst = worst.max(rel);
            total += rel;
            n += 1;
        }
    }
    let avg = total / n as f32;
    assert!(avg < 0.12, "mean relative change {avg} (worst {worst})");
}

#[test]
fn neutral_input_stays_neutral() {
    let img = Image::from_fn(80, 60, |x, y| {
        let v = 0.05 + 0.8 * ((x * 7 + y * 3) % 23) as f32 / 23.0;
        let v = if y < 10 { 0.9 } else { v * 0.5 };
        [v, v, v]
    });
    for amount in [100.0, -100.0] {
        let out = dehazed(&img, amount);
        for p in out.rgb.chunks(3) {
            let spread = p[0].max(p[1]).max(p[2]) - p[0].min(p[1]).min(p[2]);
            assert!(spread < 1e-4 * (1.0 + p[1]), "{amount}: {p:?}");
        }
    }
}

#[test]
fn scale_equivariant_in_scene_linear() {
    // Exposure then Dehaze equals Dehaze then Exposure (values above 1 included).
    let img = hazy(90, 60);
    let mut bright = img.clone();
    bright.rgb.iter_mut().for_each(|v| *v *= 4.0);
    for amount in [70.0, -70.0] {
        let a = dehazed(&img, amount);
        let b = dehazed(&bright, amount);
        for (x, y) in a.rgb.iter().zip(&b.rgb) {
            assert!(
                (x * 4.0 - y).abs() <= 1e-3 * (1.0 + y),
                "{amount}: {x} vs {y}"
            );
        }
    }
}

#[test]
fn resolution_independent() {
    // The same continuous scene at two resolutions gives the same result.
    let field = |u: f32, v: f32| -> [f32; 3] {
        let t = if v < 0.15 { 0.05 } else { 0.3 + 0.6 * u };
        let j = [
            0.5 * (6.0 * u).sin().abs(),
            0.3 * (5.0 * v).cos().abs(),
            0.02 + 0.4 * u * v,
        ];
        [0, 1, 2].map(|c| j[c] * t + AIR[c] * (1.0 - t))
    };
    let render = |w: usize, h: usize| {
        let img = Image::from_fn(w, h, |x, y| {
            field((x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32)
        });
        dehazed(&img, 80.0)
    };
    let small = render(150, 100);
    let large = render(300, 200);
    let mut diff = 0.0f32;
    for y in 0..100 {
        for x in 0..150 {
            // 2×2 average of the large render vs the small render.
            let mut avg = [0.0f32; 3];
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let p = large.get(2 * x + dx, 2 * y + dy);
                (0..3).for_each(|c| avg[c] += p[c] / 4.0);
            }
            diff += distance(avg, small.get(x, y));
        }
    }
    let diff = diff / (150.0 * 100.0);
    assert!(diff < 0.02, "mean difference between resolutions {diff}");
}

#[test]
fn downscaled_estimate_matches_full_resolution() {
    // Forcing a 4× downscale for the estimate changes the result only a little.
    let img = hazy(240, 160);
    let full = dehazed(&img, 100.0);
    let k = DehazeStrengths {
        estimate_long_side: 60.0,
        ..DehazeStrengths::default()
    };
    let mut coarse = img.clone();
    apply(&mut coarse, 100.0, &k);
    let truth = ground(&Image::from_fn(240, 160, scene));
    let (e_full, e_coarse) = (
        rms_error(&ground(&full), &truth),
        rms_error(&ground(&coarse), &truth),
    );
    let e_hazy = rms_error(&ground(&img), &truth);
    assert!(e_coarse < 0.7 * e_hazy, "{e_hazy} {e_full} {e_coarse}");
    assert_valid(&coarse, "coarse estimate");
}

#[test]
fn extremes_and_tiny_images_stay_finite() {
    let k = DehazeStrengths::default();
    let cases = [
        ("black", Image::new(64, 48)),
        ("white", Image::from_fn(64, 48, |_, _| [1.0, 1.0, 1.0])),
        (
            "very bright",
            Image::from_fn(33, 17, |_, _| [40.0, 35.0, 50.0]),
        ),
        ("1x1", Image::from_fn(1, 1, |_, _| [0.3, 0.2, 0.1])),
        ("1x1 black", Image::new(1, 1)),
        (
            "2x3",
            Image::from_fn(2, 3, |x, y| [0.1 * x as f32, 0.2 * y as f32, 0.5]),
        ),
        ("odd", hazy(37, 23)),
        (
            "row",
            Image::from_fn(57, 1, |x, _| [x as f32 / 57.0, 0.4, 0.2]),
        ),
        (
            "column",
            Image::from_fn(1, 41, |_, y| [0.6, y as f32 / 41.0, 0.2]),
        ),
        (
            "inf pixel",
            Image::from_fn(16, 16, |x, y| {
                if (x, y) == (5, 7) {
                    [f32::INFINITY, 0.3, f32::NAN]
                } else {
                    [0.4, 0.5, 0.6]
                }
            }),
        ),
        (
            "negative input",
            Image::from_fn(16, 16, |x, y| {
                [x as f32 * 0.05 - 0.2, 0.3, y as f32 * 0.04 - 0.1]
            }),
        ),
    ];
    for (name, img) in cases {
        for amount in [100.0, 37.0, -37.0, -100.0, 250.0, -250.0] {
            let mut out = img.clone();
            apply(&mut out, amount, &k);
            assert_valid(&out, &format!("{name} at {amount}"));
        }
    }
    // Black stays black when removing haze. A uniform white frame is all
    // haze: it stays uniform and neutral and darkens moderately.
    let black = dehazed(&Image::new(20, 20), 100.0);
    assert!(black.rgb.iter().all(|&v| v == 0.0));
    let white = dehazed(&Image::from_fn(20, 20, |_, _| [1.0; 3]), 100.0);
    let v0 = white.rgb[0];
    assert!(
        white.rgb.iter().all(|&v| (v - v0).abs() < 1e-6) && v0 > 0.6 && v0 < 1.0,
        "{:?}",
        &white.rgb[..3]
    );
}

#[test]
fn positive_amount_darkens_the_sky() {
    // The pure-haze sky band is the haziest region: removing haze darkens
    // it, more so as the amount grows, and never brightens anything there.
    let img = hazy(180, 120);
    let lum = luminance_weights();
    let sky = |i: &Image| dot(lum, i.get(90, 5));
    let mut last = sky(&img);
    for amount in [25.0, 50.0, 100.0] {
        let y = sky(&dehazed(&img, amount));
        assert!(
            y < last,
            "sky luminance at +{amount}: {y} (previous {last})"
        );
        last = y;
    }
    let out = dehazed(&img, 100.0);
    for y in 0..img.height / 6 {
        for x in 0..img.width {
            let (a, b) = (img.get(x, y), out.get(x, y));
            assert!(
                dot(lum, b) <= dot(lum, a) * 1.001,
                "({x},{y}): {a:?} → {b:?}"
            );
        }
    }
}

#[test]
fn highlights_brighter_than_the_airlight_are_not_boosted() {
    // Small speculars and a "sun" far above the haze level: haze removal
    // must not multiply them.
    let mut img = hazy(180, 120);
    let mut spots = Vec::new();
    for (x0, y0) in [(30, 60), (100, 80), (150, 40)] {
        for y in y0..y0 + 4 {
            for x in x0..x0 + 4 {
                img.set(x, y, [5.0; 3]);
                spots.push((x, y));
            }
        }
    }
    for y in 8..14 {
        for x in 60..66 {
            img.set(x, y, [20.0, 19.0, 17.0]);
            spots.push((x, y));
        }
    }
    // Haze removal alone leaves them as they are; the exposure compensation
    // may then lift them like their surroundings, no more.
    let no_comp = DehazeStrengths {
        exposure_compensation: 0.0,
        ..DehazeStrengths::default()
    };
    for amount in [30.0, 60.0, 100.0] {
        for (k, bound) in [(no_comp, 1.0), (DehazeStrengths::default(), 1.3)] {
            let mut out = img.clone();
            apply(&mut out, amount, &k);
            assert_valid(&out, "speculars");
            for &(x, y) in &spots {
                let (a, b) = (img.get(x, y), out.get(x, y));
                for c in 0..3 {
                    assert!(
                        b[c] <= a[c] * bound + 1e-5,
                        "+{amount} ({x},{y}): {a:?} → {b:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn bright_object_does_not_take_over_the_airlight() {
    // A neutral highlight 1.5–8× brighter than the haze (a sunlit wall, a
    // lamp, a clipped sun) is not haze: the result elsewhere stays close to
    // the result without it.
    let (w, h) = (300, 200);
    let clean = hazy(w, h);
    let reference = dehazed(&clean, 100.0);
    let lum = luminance_weights();
    let (bx, by, size) = (200usize, 120usize, 15usize);
    let outside = |x: usize, y: usize| {
        !(bx.saturating_sub(10)..bx + size + 10).contains(&x)
            || !(by.saturating_sub(10)..by + size + 10).contains(&y)
    };
    for level in [1.5f32, 2.0, 8.0] {
        let mut img = clean.clone();
        for y in by..by + size {
            for x in bx..bx + size {
                img.set(x, y, [level; 3]);
            }
        }
        let out = dehazed(&img, 100.0);
        let mut diff = 0.0f32;
        let mut n = 0;
        for y in 0..h {
            for x in 0..w {
                if outside(x, y) {
                    diff += distance(out.get(x, y), reference.get(x, y));
                    n += 1;
                }
            }
        }
        let diff = diff / n as f32;
        assert!(
            diff < 0.03,
            "block at {level}: mean change elsewhere {diff}"
        );
        let (sky_ref, sky) = (dot(lum, reference.get(50, 12)), dot(lum, out.get(50, 12)));
        assert!(
            sky > 0.9 * sky_ref,
            "block at {level}: sky {sky} vs {sky_ref} without it"
        );
    }
}

#[test]
fn non_finite_input_pixels_stay_local() {
    // One garbage pixel (inf or NaN) must not disable anything frame-wide
    // or leak non-finite values.
    let img = hazy(300, 200);
    for amount in [60.0, -60.0] {
        let reference = dehazed(&img, amount);
        for bad in [f32::INFINITY, f32::NAN, f32::NEG_INFINITY] {
            let mut garbage = img.clone();
            garbage.set(150, 100, [bad; 3]);
            let out = dehazed(&garbage, amount);
            assert_valid(&out, &format!("{bad} at {amount}"));
            let mut worst = 0.0f32;
            for y in 0..200usize {
                for x in 0..300usize {
                    if x.abs_diff(150) > 20 || y.abs_diff(100) > 20 {
                        worst = worst.max(distance(out.get(x, y), reference.get(x, y)));
                    }
                }
            }
            assert!(
                worst < 1e-3,
                "{bad} at {amount}: far pixels moved by {worst}"
            );
        }
    }
}

#[test]
fn deterministic() {
    let img = hazy(131, 77);
    for amount in [65.0, -65.0] {
        assert_eq!(dehazed(&img, amount).rgb, dehazed(&img, amount).rgb);
    }
}

#[test]
fn min_filter_matches_brute_force() {
    let (w, h) = (23, 17);
    let p: Vec<f32> = (0..w * h)
        .map(|i| ((i * 7919) % 101) as f32 / 101.0)
        .collect();
    for r in [0usize, 1, 2, 5, 30] {
        let fast = min_filter(&p, w, h, r);
        for y in 0..h {
            for x in 0..w {
                let mut m = f32::INFINITY;
                for yy in y.saturating_sub(r)..(y + r + 1).min(h) {
                    for xx in x.saturating_sub(r)..(x + r + 1).min(w) {
                        m = m.min(p[yy * w + xx]);
                    }
                }
                assert_eq!(fast[y * w + x], m, "r={r} ({x},{y})");
            }
        }
    }
}

#[test]
fn downscale_averages_blocks() {
    let img = Image::from_fn(5, 3, |x, y| [x as f32, y as f32, 1.0]);
    let (small, sw, sh) = downscale(&img, 2);
    assert_eq!((sw, sh), (3, 2));
    // Top-left block: x ∈ {0,1}, y ∈ {0,1}; bottom-right partial block: x = 4, y = 2.
    assert_eq!(&small[..3], &[0.5, 0.5, 1.0]);
    assert_eq!(&small[small.len() - 3..], &[4.0, 2.0, 1.0]);
}

#[test]
fn strengths_serde_round_trip_and_defaults() {
    let k: DehazeStrengths = serde_json::from_str(r#"{"omega": 0.7}"#).unwrap();
    assert_eq!(k.omega, 0.7);
    assert_eq!(k.min_transmission, DEFAULT_STRENGTHS.min_transmission);
    let back: DehazeStrengths = serde_json::from_str(&serde_json::to_string(&k).unwrap()).unwrap();
    assert_eq!(back, k);
    assert_eq!(DehazeStrengths::default(), DEFAULT_STRENGTHS);
}

/// Timing on a 24 MP frame: `cargo test --release -p amb-engine dehaze_speed -- --ignored --nocapture`.
#[test]
#[ignore]
fn dehaze_speed_24mp() {
    let img = hazy(6000, 4000);
    for amount in [60.0, -60.0] {
        let mut out = img.clone();
        let start = std::time::Instant::now();
        apply(&mut out, amount, &DehazeStrengths::default());
        println!(
            "dehaze {amount:+} on 6000x4000: {:.3}s",
            start.elapsed().as_secs_f64()
        );
        assert_valid(&out, "24 MP");
    }
}
