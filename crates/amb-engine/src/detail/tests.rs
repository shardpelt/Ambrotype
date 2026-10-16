// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::filters::gaussian_blur;

const OFF: Sharpening = Sharpening {
    amount: 0.0,
    radius: 1.0,
    detail: 25.0,
    masking: 0.0,
};

fn nr_off() -> NoiseReduction {
    NoiseReduction {
        color: 0.0,
        ..NoiseReduction::default()
    }
}

/// Deterministic approximately standard-normal noise (Irwin–Hall of 4 uniforms).
fn gauss(i: usize, seed: u64) -> f32 {
    let mut x = (i as u64)
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add(seed.wrapping_mul(0xD1B5_4A32_D192_ED03));
    let mut sum = 0.0f32;
    for _ in 0..4 {
        x ^= x >> 30;
        x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
        x ^= x >> 27;
        x = x.wrapping_mul(0x94D0_49BB_1331_11EB);
        x ^= x >> 31;
        sum += (x >> 40) as f32 / (1u64 << 24) as f32;
    }
    (sum - 2.0) * 3.0f32.sqrt()
}

fn grey(v: f32) -> [f32; 3] {
    [v; 3]
}

fn lum(p: [f32; 3]) -> f32 {
    dot(luminance_weights(), &p)
}

fn enc(v: f32) -> f32 {
    srgb_encode(v)
}

/// Encoded-luminance row profile of an image.
fn row_profile(img: &Image, y: usize) -> Vec<f32> {
    (0..img.width).map(|x| enc(lum(img.get(x, y)))).collect()
}

/// Vertical step edge from `a` to `b` (linear grey), blurred with a Gaussian of `blur` px.
fn blurred_step(w: usize, h: usize, a: f32, b: f32, blur: f32) -> Image {
    let plane: Vec<f32> = (0..w * h)
        .map(|i| if i % w < w / 2 { a } else { b })
        .collect();
    let plane = gaussian_blur(&plane, w, h, blur);
    Image::from_fn(w, h, |x, y| grey(plane[y * w + x]))
}

fn std_dev(v: &[f32]) -> f32 {
    let n = v.len() as f64;
    let mean = v.iter().map(|&x| x as f64).sum::<f64>() / n;
    (v.iter().map(|&x| (x as f64 - mean).powi(2)).sum::<f64>() / n).sqrt() as f32
}

fn assert_finite(img: &Image, what: &str) {
    assert!(img.rgb.iter().all(|v| v.is_finite()), "{what}: non-finite");
}

fn sharpen(img: &Image, p: Sharpening) -> Image {
    let mut out = img.clone();
    apply_sharpening(&mut out, &p, &DetailStrengths::default());
    out
}

fn denoise(img: &Image, p: NoiseReduction) -> Image {
    let mut out = img.clone();
    apply_noise_reduction(&mut out, &p, &DetailStrengths::default());
    out
}

#[test]
fn strengths_serde_round_trip_and_partial_json() {
    let k = DetailStrengths::default();
    let json = serde_json::to_string(&k).unwrap();
    assert_eq!(serde_json::from_str::<DetailStrengths>(&json).unwrap(), k);
    let partial: DetailStrengths = serde_json::from_str(r#"{"sharpen_gain": 1.3}"#).unwrap();
    assert_eq!(partial.sharpen_gain, 1.3);
    assert_eq!(partial.color_guide_eps, DEFAULT_STRENGTHS.color_guide_eps);
}

#[test]
fn transfer_tables_match_the_formula_and_invert() {
    let t = transfer();
    for i in 0..=2000 {
        let v = i as f32 / 1000.0 - 0.5; // -0.5..1.5
        let exact = v.signum() * srgb_encode(v.abs());
        assert!((t.encode(v) - exact).abs() < 2e-5, "encode {v}");
        assert!((t.decode(t.encode(v)) - v).abs() < 2e-5, "round trip {v}");
    }
    assert_eq!(t.encode(f32::NAN), 0.0);
    let sum: f32 = luminance_weights().iter().sum();
    assert!(
        (sum - 1.0).abs() < 1e-5,
        "neutral add needs weights summing to 1"
    );
}

fn textured(w: usize, h: usize) -> Image {
    Image::from_fn(w, h, |x, y| {
        let i = y * w + x;
        let base = 0.05 + 0.6 * (x as f32 / w as f32);
        [
            base * (1.0 + 0.1 * gauss(i, 1)),
            base * (0.8 + 0.1 * gauss(i, 2)),
            base * (0.6 + 0.1 * gauss(i, 3)),
        ]
    })
}

#[test]
fn neutral_settings_are_exact_no_ops() {
    let img = textured(37, 23);
    assert_eq!(sharpen(&img, OFF).rgb, img.rgb);
    for amount in [-10.0, f64::NAN, f64::INFINITY] {
        assert_eq!(sharpen(&img, Sharpening { amount, ..OFF }).rgb, img.rgb);
    }
    // Zero strengths with every other slider moved.
    let p = NoiseReduction {
        luminance: 0.0,
        luminance_detail: 90.0,
        luminance_contrast: 80.0,
        color: 0.0,
        color_detail: 10.0,
        color_smoothness: 100.0,
    };
    assert_eq!(denoise(&img, p).rgb, img.rgb);
    assert_eq!(denoise(&img, nr_off()).rgb, img.rgb);
}

#[test]
fn sharpening_increases_acutance_and_leaves_flats_alone() {
    let (w, h) = (64, 8);
    let img = blurred_step(w, h, 0.05, 0.4, 1.0);
    let out = sharpen(
        &img,
        Sharpening {
            amount: 100.0,
            ..Sharpening::default()
        },
    );
    let max_slope = |p: &[f32]| p.windows(2).map(|s| s[1] - s[0]).fold(0.0f32, f32::max);
    let (before, after) = (row_profile(&img, 4), row_profile(&out, 4));
    assert!(
        max_slope(&after) > 1.2 * max_slope(&before),
        "slope {} → {}",
        max_slope(&before),
        max_slope(&after)
    );
    // Far from the edge nothing changes.
    for y in 0..h {
        for x in (0..12).chain(w - 12..w) {
            let (a, b) = (img.get(x, y), out.get(x, y));
            for c in 0..3 {
                assert!((a[c] - b[c]).abs() < 1e-6, "({x},{y}) {a:?} → {b:?}");
            }
        }
    }
    // Default settings sharpen too, more gently.
    let gentle = row_profile(&sharpen(&img, Sharpening::default()), 4);
    assert!(max_slope(&gentle) > max_slope(&before));
    assert!(max_slope(&gentle) < max_slope(&after));
}

#[test]
fn sharpening_keeps_chromaticity() {
    let (w, h) = (48, 6);
    let img = Image::from_fn(w, h, |x, _| {
        let v = if x < w / 2 { 0.1 } else { 0.5 };
        [v * 1.2, v * 0.9, v * 0.4]
    });
    let soft = {
        let mut s = img.clone();
        for c in 0..3 {
            let plane: Vec<f32> = s.rgb.iter().skip(c).step_by(3).copied().collect();
            let b = gaussian_blur(&plane, w, h, 1.2);
            for (i, v) in b.into_iter().enumerate() {
                s.rgb[i * 3 + c] = v;
            }
        }
        s
    };
    let out = sharpen(
        &soft,
        Sharpening {
            amount: 150.0,
            detail: 100.0,
            ..Sharpening::default()
        },
    );
    assert_ne!(out.rgb, soft.rgb);
    for (a, b) in soft.rgb.chunks(3).zip(out.rgb.chunks(3)) {
        assert!((a[0] / a[1] - b[0] / b[1]).abs() < 1e-4);
        assert!((a[2] / a[1] - b[2] / b[1]).abs() < 1e-4);
    }
}

/// Low-contrast texture (left half) next to a strong blurred edge (right half).
fn texture_and_edge(w: usize, h: usize) -> Image {
    let edge: Vec<f32> = (0..w * h)
        .map(|i| if i % w < 3 * w / 4 { 0.12 } else { 0.6 })
        .collect();
    let edge = gaussian_blur(&edge, w, h, 1.0);
    Image::from_fn(w, h, |x, y| {
        let v = if x < w / 2 {
            // ±~2% encoded sinusoidal texture around mid-grey.
            0.18 * (1.0 + 0.08 * ((x as f32 * 1.7).sin() * (y as f32 * 1.3).cos()))
        } else {
            edge[y * w + x]
        };
        grey(v)
    })
}

fn mean_abs_change(a: &Image, b: &Image, xs: std::ops::Range<usize>) -> f32 {
    let mut sum = 0.0;
    let mut n = 0;
    for y in 0..a.height {
        for x in xs.clone() {
            sum += (enc(lum(a.get(x, y))) - enc(lum(b.get(x, y)))).abs();
            n += 1;
        }
    }
    sum / n as f32
}

#[test]
fn masking_spares_low_contrast_texture_but_not_strong_edges() {
    let (w, h) = (96, 24);
    let img = texture_and_edge(w, h);
    let p = Sharpening {
        amount: 120.0,
        detail: 100.0,
        ..Sharpening::default()
    };
    let open = sharpen(&img, p);
    let masked = sharpen(
        &img,
        Sharpening {
            masking: 100.0,
            ..p
        },
    );
    let texture = 4..w / 2 - 4;
    let edge = 3 * w / 4 - 4..3 * w / 4 + 4;
    let (t_open, t_masked) = (
        mean_abs_change(&img, &open, texture.clone()),
        mean_abs_change(&img, &masked, texture),
    );
    let (e_open, e_masked) = (
        mean_abs_change(&img, &open, edge.clone()),
        mean_abs_change(&img, &masked, edge),
    );
    assert!(
        t_open > 1e-3,
        "texture is sharpened without a mask: {t_open}"
    );
    assert!(t_masked < 0.05 * t_open, "texture {t_open} → {t_masked}");
    assert!(e_masked > 0.6 * e_open, "edge {e_open} → {e_masked}");
    // Intermediate masking sits in between on the texture.
    let half = sharpen(&img, Sharpening { masking: 30.0, ..p });
    let t_half = mean_abs_change(&img, &half, 4..w / 2 - 4);
    assert!(
        t_masked <= t_half && t_half <= t_open,
        "{t_masked} {t_half} {t_open}"
    );
}

#[test]
fn low_detail_limits_overshoot() {
    let (w, h) = (64, 6);
    let img = blurred_step(w, h, 0.05, 0.4, 1.5);
    let overshoot = |detail: f64| {
        let out = sharpen(
            &img,
            Sharpening {
                amount: 150.0,
                radius: 2.0,
                detail,
                masking: 0.0,
            },
        );
        let p = row_profile(&out, 3);
        let (lo, hi) = (enc(0.05), enc(0.4));
        let under = p.iter().map(|v| lo - v).fold(0.0f32, f32::max);
        let over = p.iter().map(|v| v - hi).fold(0.0f32, f32::max);
        (under.max(over), p)
    };
    let (low, p_low) = overshoot(0.0);
    let (mid, _) = overshoot(50.0);
    let (high, _) = overshoot(100.0);
    assert!(high > 0.03, "plain unsharp mask overshoots: {high}");
    assert!(low < 0.006, "Detail 0 overshoot {low}");
    assert!(low < mid && mid < high, "{low} {mid} {high}");
    // Detail 0 still steepens the edge.
    let max_slope = |p: &[f32]| p.windows(2).map(|s| s[1] - s[0]).fold(0.0f32, f32::max);
    assert!(max_slope(&p_low) > 1.1 * max_slope(&row_profile(&img, 3)));
}

#[test]
fn high_detail_brings_out_texture_more() {
    let img = texture_and_edge(96, 24);
    let change = |detail: f64| {
        let out = sharpen(
            &img,
            Sharpening {
                amount: 100.0,
                detail,
                ..Sharpening::default()
            },
        );
        mean_abs_change(&img, &out, 4..44)
    };
    assert!(change(100.0) > 1.5 * change(0.0));
}

/// Flat grey patch with luminance noise (left) and a sharp step (right half).
fn noisy_patch_and_step(w: usize, h: usize, sigma: f32) -> Image {
    Image::from_fn(w, h, |x, y| {
        let i = y * w + x;
        let base = if x < 3 * w / 4 { 0.18 } else { 0.6 };
        // Noise in encoded units, applied as a luminance change (neutral).
        let e = (enc(base) + sigma * gauss(i, 7)).clamp(0.0, 1.0);
        grey(srgb_decode(e))
    })
}

#[test]
fn luminance_nr_flattens_noise_and_keeps_edges() {
    let (w, h) = (128, 64);
    let sigma = 0.02;
    let img = noisy_patch_and_step(w, h, sigma);
    let out = denoise(
        &img,
        NoiseReduction {
            luminance: 60.0,
            ..nr_off()
        },
    );
    assert_finite(&out, "luminance NR");
    let patch = |im: &Image| -> Vec<f32> {
        let mut v = Vec::new();
        for y in 12..h - 12 {
            for x in 12..3 * w / 4 - 16 {
                v.push(enc(lum(im.get(x, y))));
            }
        }
        v
    };
    let (before, after) = (std_dev(&patch(&img)), std_dev(&patch(&out)));
    assert!(before > 0.8 * sigma);
    assert!(after < before / 6.0, "noise {before} → {after}");
    // The step stays sharp: 2 px either side keep (almost) the full contrast.
    let edge = 3 * w / 4;
    for y in 20..h - 20 {
        let (a, b) = (
            enc(lum(out.get(edge - 3, y))),
            enc(lum(out.get(edge + 2, y))),
        );
        let full = enc(0.6) - enc(0.18);
        assert!(b - a > 0.85 * full, "row {y}: {a} {b}");
    }
    // Colour NR is off, so chromaticity of every pixel is kept (it is grey).
    for p in out.rgb.chunks(3) {
        assert!((p[0] - p[1]).abs() < 1e-5 && (p[2] - p[1]).abs() < 1e-5);
    }
}

#[test]
fn luminance_nr_detail_and_contrast_restore_texture() {
    let (w, h) = (96, 48);
    // Texture of ~3x the noise amplitude plus noise.
    let img = Image::from_fn(w, h, |x, y| {
        let i = y * w + x;
        let t = 0.04 * ((x as f32 * 0.9).sin() * (y as f32 * 0.7).sin());
        grey(srgb_decode(
            (0.45 + t + 0.012 * gauss(i, 3)).clamp(0.0, 1.0),
        ))
    });
    let texture_std = |p: NoiseReduction| {
        let out = denoise(&img, p);
        let v: Vec<f32> = (8..h - 8)
            .flat_map(|y| (8..w - 8).map(move |x| (x, y)))
            .map(|(x, y)| enc(lum(out.get(x, y))))
            .collect();
        std_dev(&v)
    };
    let base = NoiseReduction {
        luminance: 70.0,
        luminance_detail: 0.0,
        ..nr_off()
    };
    let flat = texture_std(base);
    let detailed = texture_std(NoiseReduction {
        luminance_detail: 100.0,
        ..base
    });
    let contrasty = texture_std(NoiseReduction {
        luminance_contrast: 100.0,
        ..base
    });
    assert!(detailed > 1.1 * flat, "detail {flat} → {detailed}");
    assert!(contrasty > 1.1 * flat, "contrast {flat} → {contrasty}");
}

/// Grey patch with independent per-channel (chroma) noise on the left, a
/// red/green colour edge on the right.
fn chroma_noise_and_colour_edge(w: usize, h: usize, sigma: f32) -> Image {
    Image::from_fn(w, h, |x, y| {
        let i = y * w + x;
        if x < w / 2 {
            let n = [gauss(i, 11), gauss(i, 12), gauss(i, 13)];
            [0, 1, 2].map(|c| srgb_decode((enc(0.2) + sigma * n[c]).clamp(0.0, 1.0)))
        } else if x < 3 * w / 4 {
            [0.5, 0.12, 0.08]
        } else {
            [0.12, 0.35, 0.08]
        }
    })
}

/// RMS of chroma (encoded channel minus encoded luminance) around its mean.
fn chroma_noise(img: &Image, xs: std::ops::Range<usize>, ys: std::ops::Range<usize>) -> f32 {
    let mut total = 0.0;
    for c in 0..3 {
        let v: Vec<f32> = ys
            .clone()
            .flat_map(|y| xs.clone().map(move |x| (x, y)))
            .map(|(x, y)| {
                let p = img.get(x, y);
                enc(p[c]) - enc(lum(p))
            })
            .collect();
        total += std_dev(&v).powi(2);
    }
    total.sqrt()
}

#[test]
fn colour_nr_removes_chroma_noise_keeps_colour_edges_and_luminance() {
    let (w, h) = (160, 64);
    let img = chroma_noise_and_colour_edge(w, h, 0.03);
    let out = denoise(
        &img,
        NoiseReduction {
            color: 50.0,
            ..nr_off()
        },
    );
    assert_finite(&out, "colour NR");
    let (xs, ys) = (8..w / 2 - 24, 8..h - 8);
    let (before, after) = (
        chroma_noise(&img, xs.clone(), ys.clone()),
        chroma_noise(&out, xs, ys),
    );
    assert!(after < before / 3.0, "chroma noise {before} → {after}");
    // Luminance is unchanged everywhere.
    for (a, b) in img.rgb.chunks(3).zip(out.rgb.chunks(3)) {
        let (ya, yb) = (lum([a[0], a[1], a[2]]), lum([b[0], b[1], b[2]]));
        assert!((ya - yb).abs() <= 1e-5 * ya.max(1e-3), "{ya} → {yb}");
    }
    // The red/green edge keeps its colours two pixels either side.
    let edge = 3 * w / 4;
    for y in 8..h - 8 {
        for (x, want) in [
            (edge - 2, [0.5, 0.12, 0.08]),
            (edge + 1, [0.12, 0.35, 0.08]),
        ] {
            let p = out.get(x, y);
            for c in 0..3 {
                assert!(
                    (enc(p[c]) - enc(want[c])).abs() < 0.03,
                    "({x},{y}) {p:?} vs {want:?}"
                );
            }
        }
    }
}

#[test]
fn colour_nr_detail_protects_isoluminant_edges() {
    // Red/green pair of (nearly) equal luminance: the guide cannot see the edge.
    let (w, h) = (96, 32);
    let lw = luminance_weights();
    let red = [0.4f32, 0.0, 0.1];
    let y_red = dot(lw, &red);
    let g = (y_red - lw[0] * 0.05 - lw[2] * 0.1) / lw[1];
    let green = [0.05, g, 0.1];
    let img = Image::from_fn(w, h, |x, _| if x < w / 2 { red } else { green });
    let bleed = |detail: f64| {
        let out = denoise(
            &img,
            NoiseReduction {
                color: 100.0,
                color_detail: detail,
                ..nr_off()
            },
        );
        let p = out.get(w / 2 - 2, h / 2);
        (enc(p[0]) - enc(red[0])).abs()
    };
    let (none, full) = (bleed(0.0), bleed(100.0));
    assert!(none > 0.02, "Detail 0 smooths across the edge: {none}");
    assert!(
        full < 0.25 * none,
        "Detail 100 protects it: {none} → {full}"
    );
}

#[test]
fn colour_nr_smoothness_removes_larger_blotches() {
    // Chroma blotches of ~6 px: low-frequency colour noise.
    let (w, h) = (128, 64);
    let img = Image::from_fn(w, h, |x, y| {
        let cell = (y / 6) * 64 + x / 6;
        let n = [gauss(cell, 21), gauss(cell, 22), gauss(cell, 23)];
        [0, 1, 2].map(|c| srgb_decode(enc(0.2) + 0.03 * n[c]))
    });
    let residual = |smoothness: f64| {
        let out = denoise(
            &img,
            NoiseReduction {
                color: 40.0,
                color_smoothness: smoothness,
                ..nr_off()
            },
        );
        chroma_noise(&out, 16..w - 16, 16..h - 16)
    };
    assert!(residual(100.0) < 0.8 * residual(0.0));
}

#[test]
fn operators_are_deterministic() {
    let img = textured(203, 77);
    let nr = NoiseReduction {
        luminance: 40.0,
        luminance_contrast: 30.0,
        color: 60.0,
        ..NoiseReduction::default()
    };
    let sh = Sharpening {
        amount: 90.0,
        masking: 40.0,
        ..Sharpening::default()
    };
    let run = || sharpen(&denoise(&img, nr), sh);
    let (a, b) = (run(), run());
    assert_eq!(a.rgb, b.rgb);
    assert_finite(&a, "textured");
}

#[test]
fn extreme_settings_keep_tiny_odd_black_and_white_images_finite() {
    let strong_nr = NoiseReduction {
        luminance: 100.0,
        luminance_detail: 100.0,
        luminance_contrast: 100.0,
        color: 100.0,
        color_detail: 0.0,
        color_smoothness: 100.0,
    };
    let strong_sharp = Sharpening {
        amount: 150.0,
        radius: 3.0,
        detail: 100.0,
        masking: 50.0,
    };
    let cases: Vec<(&str, Image)> = vec![
        ("1x1", Image::from_fn(1, 1, |_, _| [0.3, 0.2, 0.1])),
        (
            "2x3",
            Image::from_fn(2, 3, |x, y| [0.1 * x as f32, 0.3, 0.2 * y as f32]),
        ),
        ("1x9", Image::from_fn(1, 9, |_, y| grey(y as f32 / 8.0))),
        ("17x13", textured(17, 13)),
        ("black", Image::new(31, 19)),
        ("white", Image::from_fn(31, 19, |_, _| grey(1.0))),
        (
            "out of range",
            Image::from_fn(23, 11, |x, y| {
                [
                    -0.05 * x as f32,
                    1.5 + y as f32 * 0.1,
                    (x * y) as f32 * 0.01,
                ]
            }),
        ),
        ("empty", Image::new(0, 0)),
    ];
    for (name, img) in cases {
        for sh in [strong_sharp, Sharpening::default()] {
            let out = sharpen(&denoise(&img, strong_nr), sh);
            assert_finite(&out, name);
            let out = sharpen(&denoise(&img, NoiseReduction::default()), sh);
            assert_finite(&out, name);
        }
    }
    // Uniform frames stay (almost exactly) as they are.
    for v in [0.0, 1.0, 0.18] {
        let img = Image::from_fn(31, 19, |_, _| grey(v));
        let out = sharpen(&denoise(&img, strong_nr), strong_sharp);
        assert!(out.rgb.iter().all(|o| (o - v).abs() < 1e-5), "flat {v}");
    }
}

#[test]
fn non_finite_sliders_fall_back_to_defaults() {
    let img = textured(40, 20);
    let p = Sharpening {
        amount: 60.0,
        radius: f64::NAN,
        detail: f64::INFINITY,
        masking: f64::NAN,
    };
    assert_finite(&sharpen(&img, p), "sharpen");
    let q = NoiseReduction {
        luminance: 30.0,
        luminance_detail: f64::NAN,
        luminance_contrast: f64::NEG_INFINITY,
        color: 30.0,
        color_detail: f64::NAN,
        color_smoothness: f64::NAN,
    };
    assert_finite(&denoise(&img, q), "denoise");
}

#[test]
fn streaming_blur_matches_the_shared_gaussian() {
    let (w, h) = (53, 31);
    let plane: Vec<f32> = (0..w * h).map(|i| 0.5 + 0.2 * gauss(i, 5)).collect();
    for sigma in [0.2, 0.5, 1.0, 1.7, 3.0] {
        let want = gaussian_blur(&plane, w, h, sigma);
        let kernel = gaussian_kernel(sigma);
        let r = kernel.len() / 2;
        let (mut tmp, mut pad, mut out) = (vec![0.0; w], vec![0.0; w + 2 * r], vec![0.0; w]);
        for y in 0..h {
            convolve_row(&plane, w, h, y, &kernel, &mut tmp, &mut pad, &mut out);
            for x in 0..w {
                let d = (out[x] - want[y * w + x]).abs();
                assert!(d < 1e-5, "sigma {sigma} ({x},{y}): {d}");
            }
        }
    }
    // Narrow images (narrower than the kernel) clamp the same way.
    let (w, h) = (2, 9);
    let plane: Vec<f32> = (0..w * h).map(|i| i as f32).collect();
    let want = gaussian_blur(&plane, w, h, 2.0);
    let kernel = gaussian_kernel(2.0);
    let r = kernel.len() / 2;
    let (mut tmp, mut pad, mut out) = (vec![0.0; w], vec![0.0; w + 2 * r], vec![0.0; w]);
    for y in 0..h {
        convolve_row(&plane, w, h, y, &kernel, &mut tmp, &mut pad, &mut out);
        for x in 0..w {
            assert!((out[x] - want[y * w + x]).abs() < 1e-4);
        }
    }
}

#[test]
fn noise_estimate_tracks_noise_not_edges_or_gradients() {
    // Left half: encoded luminance noise of known sigma at two levels (dark
    // band noisier), at the centres of bands 1 and 4; right half: clean
    // step edges and a gradient (bands 2-3 and 6-7), which must not pull
    // the estimate up.
    let (w, h) = (256, 192);
    let (dark, light) = (1.5 / 8.0, 4.5 / 8.0);
    let truth = |l: f32| if l < 0.3 { 0.02 } else { 0.008 };
    let l: Vec<f32> = (0..w * h)
        .map(|i| {
            let (x, y) = (i % w, i / w);
            if x < w / 2 {
                let base = if y < h / 2 { dark } else { light };
                base + truth(base) * gauss(i, 9)
            } else {
                let step = if ((x + 1) / 24) % 2 == 0 { 0.3 } else { 0.76 };
                step + 0.001 * y as f32
            }
        })
        .map(|v| v - CENTRE)
        .collect();
    let tiles = luminance_tiles(&l, w, h);
    let noise = NoiseProfile::from_tiles(&tiles, DEFAULT_STRENGTHS.noise_percentile);
    for (lum, want) in [(dark, 0.02), (light, 0.008)] {
        let got = noise.at(lum);
        assert!(
            got > 0.8 * want && got < 1.1 * want,
            "at {lum}: {got} vs {want}"
        );
    }
    // Clean frames read zero; clipped tiles are skipped; empty input is zero.
    let flat = vec![0.1f32; w * h];
    let zero = NoiseProfile::from_tiles(&luminance_tiles(&flat, w, h), 0.15);
    assert!(zero.sigma.iter().all(|&s| s == 0.0));
    let clipped = vec![1.0 - CENTRE; w * h];
    assert!(luminance_tiles(&clipped, w, h).is_empty());
    assert_eq!(
        NoiseProfile::from_tiles(&[], 0.15).sigma,
        [0.0; NOISE_BANDS]
    );
}

#[test]
fn noise_profile_fills_empty_bands() {
    let tiles: Vec<TileStat> = (0..20)
        .flat_map(|_| {
            [
                TileStat {
                    lum: 0.2,
                    power: 0.01f32.powi(2),
                },
                TileStat {
                    lum: 0.7,
                    power: 0.03f32.powi(2),
                },
            ]
        })
        .collect();
    let p = NoiseProfile::from_tiles(&tiles, 0.5);
    // Band 1 holds 0.2, band 5 holds 0.7; between them linear, outside constant.
    assert!((p.sigma[0] - 0.01).abs() < 1e-6);
    assert!((p.sigma[1] - 0.01).abs() < 1e-6);
    assert!((p.sigma[3] - 0.02).abs() < 1e-6);
    assert!((p.sigma[7] - 0.03).abs() < 1e-6);
    assert!((p.at(0.0) - 0.01).abs() < 1e-6 && (p.at(1.0) - 0.03).abs() < 1e-6);
    // Too few tiles for any band: all pooled.
    let few = [TileStat {
        lum: 0.5,
        power: 4e-4,
    }];
    assert!((NoiseProfile::from_tiles(&few, 0.15).at(0.9) - 0.02).abs() < 1e-6);
}

/// Colour texture (isoluminant green/tan stripes, 3 px wide) on the left,
/// flat grey on the right, with per-channel encoded noise of `sigma` everywhere.
fn colour_texture(w: usize, h: usize, amplitude: f32, sigma: f32) -> Image {
    let lw = luminance_weights();
    Image::from_fn(w, h, |x, y| {
        let i = y * w + x;
        let tint = if x < w / 2 {
            if (x / 3) % 2 == 0 { 1.0 } else { -1.0 }
        } else {
            0.0
        };
        // ±amplitude on R−B (encoded), luminance restored through G.
        let e = [0.5 + tint * amplitude, 0.5, 0.5 - tint * amplitude];
        let mut p = e.map(srgb_decode);
        let target = srgb_decode(0.5);
        p[1] += (target - dot(lw, &p)) / lw[1];
        let n = [gauss(i, 31), gauss(i, 32), gauss(i, 33)];
        [0, 1, 2].map(|c| srgb_decode(srgb_encode(p[c]) + sigma * n[c]))
    })
}

#[test]
fn colour_nr_adapts_to_the_noise_level() {
    let (w, h) = (128, 96);
    let texture = (8..w / 2 - 8, 8..h - 8);
    let flat = (w / 2 + 8..w - 8, 8..h - 8);
    // Clean frame: the default keeps most of the colour texture.
    let clean = colour_texture(w, h, 0.03, 0.002);
    let out = denoise(&clean, NoiseReduction::default());
    let (before, after) = (
        chroma_noise(&clean, texture.0.clone(), texture.1.clone()),
        chroma_noise(&out, texture.0.clone(), texture.1.clone()),
    );
    assert!(after > 0.7 * before, "clean texture {before} → {after}");
    // Noisy frame (noise as strong as the texture): the noise goes.
    let noisy = colour_texture(w, h, 0.03, 0.03);
    let out = denoise(&noisy, NoiseReduction::default());
    let (before, after) = (
        chroma_noise(&noisy, flat.0.clone(), flat.1.clone()),
        chroma_noise(&out, flat.0, flat.1),
    );
    assert!(after < before / 3.0, "noise {before} → {after}");
    // Detail 0 smooths the clean texture away regardless.
    let bled = denoise(
        &clean,
        NoiseReduction {
            color_detail: 0.0,
            ..NoiseReduction::default()
        },
    );
    let left = chroma_noise(&bled, texture.0, texture.1);
    assert!(left < 0.5 * before, "Detail 0 keeps {left} of {before}");
}

#[test]
fn colour_nr_removes_isolated_speckles_but_keeps_thin_lines() {
    let (w, h) = (96, 64);
    let magenta = |p: [f32; 3]| [p[0] * 1.6, p[1] * 0.75, p[2] * 1.6];
    let img = Image::from_fn(w, h, |x, y| {
        let i = y * w + x;
        let n = [gauss(i, 41), gauss(i, 42), gauss(i, 43)];
        let p = [0, 1, 2].map(|c| srgb_decode(0.45 + 0.004 * n[c]));
        let speckle = (x, y) == (20, 20) || (x, y) == (30, 40);
        let line = (60..62).contains(&x);
        if speckle || line { magenta(p) } else { p }
    });
    let out = denoise(&img, NoiseReduction::default());
    let chroma = |im: &Image, x: usize, y: usize| {
        let p = im.get(x, y);
        let le = enc(lum(p));
        (0..3).map(|c| (enc(p[c]) - le).powi(2)).sum::<f32>().sqrt()
    };
    for (x, y) in [(20, 20), (30, 40)] {
        let (a, b) = (chroma(&img, x, y), chroma(&out, x, y));
        assert!(b < 0.3 * a, "speckle ({x},{y}) {a} → {b}");
    }
    for y in 10..h - 10 {
        let (a, b) = (chroma(&img, 60, y), chroma(&out, 60, y));
        assert!(b > 0.7 * a, "line row {y}: {a} → {b}");
    }
}

#[test]
fn luminance_nr_adapts_to_the_noise_level() {
    let (w, h) = (128, 96);
    // Fine texture (encoded ±0.02) on the left, flat on the right.
    let make = |sigma: f32| {
        Image::from_fn(w, h, |x, y| {
            let i = y * w + x;
            let t = if x < w / 2 {
                0.02 * ((x as f32 * 1.1).sin() * (y as f32 * 0.8).sin())
            } else {
                0.0
            };
            grey(srgb_decode(0.5 + t + sigma * gauss(i, 51)))
        })
    };
    let std_in = |im: &Image, xs: std::ops::Range<usize>| {
        let v: Vec<f32> = (8..h - 8)
            .flat_map(|y| xs.clone().map(move |x| (x, y)))
            .map(|(x, y)| enc(lum(im.get(x, y))))
            .collect();
        std_dev(&v)
    };
    let p = NoiseReduction {
        luminance: 30.0,
        ..nr_off()
    };
    // Clean: the texture survives almost intact.
    let clean = make(0.001);
    let out = denoise(&clean, p);
    let (a, b) = (std_in(&clean, 8..w / 2 - 8), std_in(&out, 8..w / 2 - 8));
    assert!(b > 0.85 * a, "clean texture {a} → {b}");
    // Noisy: the same slider removes most of the noise in the flat half.
    let noisy = make(0.02);
    let out = denoise(&noisy, p);
    let (a, b) = (
        std_in(&noisy, w / 2 + 8..w - 8),
        std_in(&out, w / 2 + 8..w - 8),
    );
    assert!(b < 0.5 * a, "noise {a} → {b}");
}

#[test]
fn fractional_box_blur_matches_integer_box_and_brute_force() {
    let (w, h) = (37, 29);
    let plane: Vec<f32> = (0..w * h).map(|i| 0.5 + 0.2 * gauss(i, 81)).collect();
    for r in [1usize, 2, 5, 30] {
        let (want, got) = (
            box_blur(&plane, w, h, r),
            box_blur_frac(&plane, w, h, r as f32),
        );
        assert!(
            want.iter().zip(&got).all(|(a, b)| (a - b).abs() < 1e-5),
            "r {r}"
        );
    }
    for r in [0.3f32, 1.4, 2.75] {
        let got = box_blur_frac(&plane, w, h, r);
        let ri = r.floor() as isize;
        let weight = |d: isize| match d.abs() {
            d if d <= ri => 1.0,
            d if d == ri + 1 => r - ri as f32,
            _ => 0.0,
        };
        let at = |x: isize, y: isize| {
            plane[y.clamp(0, h as isize - 1) as usize * w + x.clamp(0, w as isize - 1) as usize]
        };
        for y in 0..h as isize {
            for x in 0..w as isize {
                let (mut acc, mut norm) = (0.0, 0.0);
                for dy in -ri - 1..=ri + 1 {
                    for dx in -ri - 1..=ri + 1 {
                        let wt = weight(dx) * weight(dy);
                        acc += wt * at(x + dx, y + dy);
                        norm += wt;
                    }
                }
                let d = (got[y as usize * w + x as usize] - acc / norm).abs();
                assert!(d < 1e-5, "r {r} ({x},{y}): {d}");
            }
        }
    }
}

type Tint<'a> = dyn Fn(usize, usize) -> f32 + Sync + 'a;

/// Isoluminant colour texture `tint(x, y)` (in units of `amp`) over the
/// whole frame (no flat region for the noise estimate to fall back on),
/// with per-channel encoded noise.
fn full_frame_colour_texture(w: usize, h: usize, tint: &Tint<'_>, amp: f32, sigma: f32) -> Image {
    let lw = luminance_weights();
    Image::from_fn(w, h, |x, y| {
        let i = y * w + x;
        let tint = tint(x, y);
        let e = [0.5 + tint * amp, 0.5, 0.5 - tint * amp];
        let mut p = e.map(srgb_decode);
        p[1] += (srgb_decode(0.5) - dot(lw, &p)) / lw[1];
        let n = [gauss(i, 62), gauss(i, 63), gauss(i, 64)];
        [0, 1, 2].map(|c| srgb_decode(srgb_encode(p[c]) + sigma * n[c]))
    })
}

#[test]
fn colour_nr_keeps_clean_colour_texture_that_fills_the_frame() {
    let (w, h) = (128, 96);
    let (xs, ys) = (8..w - 8, 8..h - 8);
    let stripe = |v: usize| if (v / 3) % 2 == 0 { 1.0 } else { -1.0 };
    let patterns: [(&str, &Tint<'_>); 5] = [
        ("vertical", &|x, _| stripe(x)),
        ("horizontal", &|_, y| stripe(y)),
        ("diagonal", &|x, y| stripe(x + y)),
        ("anti-diagonal", &|x, y| stripe(x + 2 * h - y)),
        ("plaid", &|x, y| 0.5 * (stripe(x) + stripe(y / 2 + 1))),
    ];
    for (name, tint) in patterns {
        for sigma in [0.0, 0.002] {
            let img = full_frame_colour_texture(w, h, tint, 0.03, sigma);
            let out = denoise(&img, NoiseReduction::default());
            let (before, after) = (
                chroma_noise(&img, xs.clone(), ys.clone()),
                chroma_noise(&out, xs.clone(), ys.clone()),
            );
            assert!(
                after > 0.7 * before,
                "{name}, sigma {sigma}: {before} → {after}"
            );
        }
    }
    // Real colour noise over the whole frame still goes.
    let noisy = full_frame_colour_texture(w, h, &|_, _| 0.0, 0.0, 0.03);
    let out = denoise(&noisy, NoiseReduction::default());
    let (before, after) = (
        chroma_noise(&noisy, xs.clone(), ys.clone()),
        chroma_noise(&out, xs, ys),
    );
    assert!(after < before / 3.0, "noise {before} → {after}");
}

/// Largest per-pixel change of encoded luminance or chroma between two images.
fn max_change(a: &Image, b: &Image) -> f32 {
    a.rgb
        .chunks(3)
        .zip(b.rgb.chunks(3))
        .map(|(p, q)| {
            let (lp, lq) = (enc(lum([p[0], p[1], p[2]])), enc(lum([q[0], q[1], q[2]])));
            (0..3)
                .map(|c| ((enc(p[c]) - lp) - (enc(q[c]) - lq)).abs())
                .fold((lp - lq).abs(), f32::max)
        })
        .fold(0.0, f32::max)
}

/// Steps of 0.1 across `at` change the output by no more than a small
/// multiple of the steps on either side (no jump at `at`).
fn assert_continuous(what: &str, at: f64, f: &dyn Fn(f64) -> Image) {
    let v = [at - 0.15, at - 0.05, at + 0.05, at + 0.15].map(f);
    let d = [0, 1, 2].map(|i| max_change(&v[i], &v[i + 1]));
    assert!(
        d[1] <= 3.0 * d[0].max(d[2]) + 2e-4,
        "{what} jumps at {at}: steps {d:?}"
    );
}

#[test]
fn outputs_change_continuously_with_the_sliders() {
    let chroma = chroma_noise_and_colour_edge(160, 64, 0.03);
    for at in [6.25, 18.75, 31.25] {
        assert_continuous("Color", at, &|v| {
            denoise(
                &chroma,
                NoiseReduction {
                    color: v,
                    ..NoiseReduction::default()
                },
            )
        });
    }
    assert_continuous("Smoothness", 62.5, &|v| {
        denoise(
            &chroma,
            NoiseReduction {
                color_smoothness: v,
                ..NoiseReduction::default()
            },
        )
    });
    let luma = noisy_patch_and_step(96, 64, 0.03);
    for at in [16.67, 50.0, 83.33] {
        assert_continuous("Luminance", at, &|v| {
            denoise(
                &luma,
                NoiseReduction {
                    luminance: v,
                    ..nr_off()
                },
            )
        });
    }
    let step = blurred_step(64, 8, 0.05, 0.4, 1.0);
    for at in [1.5, 2.5] {
        // Radius steps of 0.01 (the slider's range is 0.5..3).
        assert_continuous("Radius", 100.0 * at, &|v| {
            sharpen(
                &step,
                Sharpening {
                    amount: 150.0,
                    radius: v / 100.0,
                    detail: 0.0,
                    masking: 0.0,
                },
            )
        });
    }
}

#[test]
fn a_non_finite_pixel_does_not_spread() {
    let (w, h) = (128, 128);
    let img = Image::from_fn(w, h, |x, y| {
        let i = y * w + x;
        let n = [gauss(i, 71), gauss(i, 72), gauss(i, 73)];
        [0, 1, 2].map(|c| srgb_decode(0.45 + 0.03 * n[c]))
    });
    let clean = denoise(&img, NoiseReduction::default());
    let (xs, ys) = (64..120, 64..120);
    let want = chroma_noise(&clean, xs.clone(), ys.clone());
    assert!(want < 0.2 * chroma_noise(&img, xs.clone(), ys.clone()));
    for bad in [f32::INFINITY, f32::NAN, f32::NEG_INFINITY] {
        let mut dirty = img.clone();
        dirty.set(10, 10, [bad; 3]);
        dirty.set(110, 20, [0.2, bad, 0.2]);
        let out = denoise(&dirty, NoiseReduction::default());
        let got = chroma_noise(&out, xs.clone(), ys.clone());
        assert!(got < 1.5 * want, "{bad}: chroma {want} → {got}");
        // Pixels away from the bad ones are NR'd as usual.
        for y in 30..70 {
            for x in 30..70 {
                let (a, b) = (out.get(x, y), clean.get(x, y));
                assert!(
                    (0..3).all(|c| (a[c] - b[c]).abs() < 0.02),
                    "{bad} ({x},{y}): {a:?} vs {b:?}"
                );
            }
        }
        // Sharpening: neighbours of a bad pixel are not blacked out, and
        // pixels outside the kernel are exactly as without it.
        let flat = Image::from_fn(32, 32, |_, _| grey(0.2));
        let mut dirty = flat.clone();
        dirty.set(16, 16, [bad; 3]);
        let out = sharpen(&dirty, Sharpening::default());
        let want = sharpen(&flat, Sharpening::default());
        for y in 0..32 {
            for x in 0..32 {
                if (x, y) == (16, 16) {
                    continue;
                }
                let p = out.get(x, y);
                assert!(
                    p.iter().all(|v| (v - 0.2).abs() < 0.05),
                    "{bad} ({x},{y}): {p:?}"
                );
                if x.abs_diff(16) > 4 || y.abs_diff(16) > 4 {
                    assert_eq!(p, want.get(x, y), "{bad} ({x},{y})");
                }
            }
        }
        // Luminance NR likewise.
        let out = denoise(
            &dirty,
            NoiseReduction {
                luminance: 50.0,
                luminance_contrast: 50.0,
                ..NoiseReduction::default()
            },
        );
        for (i, p) in out.rgb.chunks(3).enumerate() {
            if i != 16 * 32 + 16 {
                assert!(p.iter().all(|v| (v - 0.2).abs() < 0.05), "{bad} {i}: {p:?}");
            }
        }
    }
}

#[test]
fn huge_calibration_constants_neither_panic_nor_hang() {
    let img = textured(8, 8);
    let strong_nr = NoiseReduction {
        luminance: 100.0,
        luminance_contrast: 100.0,
        color: 100.0,
        color_smoothness: 100.0,
        ..NoiseReduction::default()
    };
    let strong_sharp = Sharpening {
        amount: 150.0,
        radius: 3.0,
        detail: 50.0,
        masking: 50.0,
    };
    for json in [
        r#"{"sharpen_sigma_per_radius": 1e300}"#,
        r#"{"halo_window_per_radius": 1e300}"#,
        r#"{"mask_blur_sigma": 1e300, "mask_smooth_sigma": 1e300}"#,
        r#"{"color_radius": 1e13}"#,
        r#"{"color_radius_per_strength": 1e300, "color_radius_per_smoothness": 1e300}"#,
        r#"{"color_max_downscale": 1e300, "color_low_res_radius": 1e-300}"#,
        r#"{"luminance_radius": 1e13, "luminance_coarse_radius": 1e300}"#,
        r#"{"luminance_contrast_sigma": 1e300}"#,
    ] {
        let k: DetailStrengths = serde_json::from_str(json).unwrap();
        let mut out = img.clone();
        apply_noise_reduction(&mut out, &strong_nr, &k);
        apply_sharpening(&mut out, &strong_sharp, &k);
        assert_finite(&out, json);
    }
}

/// Timing on a 24 MP frame (run with `--release -- --ignored --nocapture`):
/// the first run of each case (cold allocations, as in a one-shot render)
/// and the best of three.
#[test]
#[ignore]
fn timing_24mp_defaults() {
    let (w, h) = (6000, 4000);
    let img = Image::from_fn(w, h, |x, y| {
        let i = y * w + x;
        let v = 0.05 + 0.5 * ((x as f32 * 0.01).sin() * (y as f32 * 0.013).cos()).abs();
        [
            v * (1.0 + 0.05 * gauss(i, 1)),
            v * (1.0 + 0.05 * gauss(i, 2)),
            v * (1.0 + 0.05 * gauss(i, 3)),
        ]
    });
    let k = DetailStrengths::default();
    let time = |label: &str, f: &dyn Fn(&mut Image)| {
        let runs: Vec<f64> = (0..3)
            .map(|_| {
                let mut im = img.clone();
                let start = std::time::Instant::now();
                f(&mut im);
                start.elapsed().as_secs_f64()
            })
            .collect();
        let best = runs.iter().copied().fold(f64::INFINITY, f64::min);
        eprintln!("{label}: first {:.3} s, best {best:.3} s", runs[0]);
    };
    time("defaults (colour NR 25 + sharpening 40)", &|im| {
        apply_noise_reduction(im, &NoiseReduction::default(), &k);
        apply_sharpening(im, &Sharpening::default(), &k);
    });
    time("colour NR 25", &|im| {
        apply_noise_reduction(im, &NoiseReduction::default(), &k)
    });
    time("sharpening 40", &|im| {
        apply_sharpening(im, &Sharpening::default(), &k)
    });
    time("sharpening 100, radius 3, masking 50", &|im| {
        let p = Sharpening {
            amount: 100.0,
            radius: 3.0,
            detail: 50.0,
            masking: 50.0,
        };
        apply_sharpening(im, &p, &k)
    });
    time("luminance NR 50 + contrast 50", &|im| {
        let p = NoiseReduction {
            luminance: 50.0,
            luminance_contrast: 50.0,
            color: 0.0,
            ..NoiseReduction::default()
        };
        apply_noise_reduction(im, &p, &k)
    });
    time("colour NR 100, smoothness 100", &|im| {
        let p = NoiseReduction {
            color: 100.0,
            color_smoothness: 100.0,
            ..NoiseReduction::default()
        };
        apply_noise_reduction(im, &p, &k)
    });
}
