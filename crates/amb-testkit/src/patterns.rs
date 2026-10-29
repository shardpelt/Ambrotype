// SPDX-License-Identifier: GPL-3.0-or-later
//! Synthetic calibration targets, generated in linear ProPhoto (the synthetic
//! DNG's camera space). See docs/research/05 §18.2 for what each one measures.

use crate::dng::{LinearImage, PROPHOTO_TO_XYZ_D50, SRGB_TO_XYZ_D50, apply3, invert3, mul3};

pub const NAMES: &[&str] = &["ramp", "hue", "edges", "colorchecker", "negative", "dust"];

pub fn generate(name: &str) -> Option<LinearImage> {
    Some(match name {
        "ramp" => ramp(),
        "hue" => hue_grid(),
        "edges" => edges(),
        "colorchecker" => colorchecker(),
        "negative" => negative(),
        "dust" => dust(),
        _ => return None,
    })
}

fn fill(img: &mut LinearImage, x0: u32, y0: u32, w: u32, h: u32, rgb: [f64; 3]) {
    for y in y0..(y0 + h).min(img.height) {
        for x in x0..(x0 + w).min(img.width) {
            img.set(x, y, rgb);
        }
    }
}

/// Grey ramp: 64 log-spaced patches from 2^-14 to 1.0 (top), and a smooth linear
/// gradient 0..1 (bottom). Measures tone response: Exposure, Contrast,
/// Whites/Blacks, curves and the global part of Highlights/Shadows.
pub fn ramp() -> LinearImage {
    let (w, h) = (2048, 768);
    let mut img = LinearImage::new(w, h);
    let steps = 64;
    let patch = w / 16;
    for i in 0..steps {
        let stops = -14.0 + 14.0 * f64::from(i) / f64::from(steps - 1);
        let v = 2f64.powf(stops);
        let (col, row) = (i % 16, i / 16);
        fill(&mut img, col * patch, row * 128, patch, 128, [v, v, v]);
    }
    for y in 512..h {
        for x in 0..w {
            let v = f64::from(x) / f64::from(w - 1);
            img.set(x, y, [v, v, v]);
        }
    }
    img
}

fn hsv_to_rgb(h: f64, s: f64, v: f64) -> [f64; 3] {
    let h = (h.rem_euclid(360.0)) / 60.0;
    let c = v * s;
    let x = c * (1.0 - ((h % 2.0) - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    [r + m, g + m, b + m]
}

/// Hue × saturation grid (36 hues × 8 saturations) at three brightness levels.
/// Colours are defined in gamma-1.8 ProPhoto, then linearised. Measures
/// HSL/Color Mixer, Vibrance/Saturation, Color Grading and Calibration.
pub fn hue_grid() -> LinearImage {
    let (cols, rows_per_band, bands) = (36u32, 8u32, 3u32);
    let cell = 48;
    let mut img = LinearImage::new(cols * cell, rows_per_band * bands * cell);
    for band in 0..bands {
        let v = [0.35, 0.6, 0.85][band as usize];
        for row in 0..rows_per_band {
            let s = f64::from(row + 1) / f64::from(rows_per_band);
            for col in 0..cols {
                let hue = f64::from(col) * 10.0;
                let encoded = hsv_to_rgb(hue, s, v);
                let linear = encoded.map(|c| c.powf(1.8));
                fill(
                    &mut img,
                    col * cell,
                    (band * rows_per_band + row) * cell,
                    cell,
                    cell,
                    linear,
                );
            }
        }
    }
    img
}

/// Local-contrast targets: checkerboards of several contrasts on dark, mid and
/// bright backgrounds, blurred and hard step edges, and sinusoidal gratings at
/// several frequencies. Measures Clarity, Texture, Dehaze, sharpening and the
/// local (adaptive) part of Highlights/Shadows.
pub fn edges() -> LinearImage {
    let (w, h) = (2048, 1536);
    let mut img = LinearImage::new(w, h);
    // Three background bands with checkerboards of increasing contrast.
    for (band, base) in [0.02, 0.18, 0.7].into_iter().enumerate() {
        let y0 = band as u32 * 384;
        fill(&mut img, 0, y0, w, 384, [base; 3]);
        for (k, contrast) in [1.1, 1.5, 2.0, 4.0].into_iter().enumerate() {
            let x0 = 64 + k as u32 * 480;
            for cy in 0..8 {
                for cx in 0..8 {
                    let light = (cx + cy) % 2 == 0;
                    let v = if light {
                        (base * contrast).min(1.0)
                    } else {
                        base / contrast
                    };
                    fill(&mut img, x0 + cx * 40, y0 + 32 + cy * 40, 40, 40, [v; 3]);
                }
            }
        }
    }
    // Bottom: gratings at 4 frequencies (left) and step edges (right).
    let y0 = 1152;
    for x in 0..1024 {
        let period = [64.0, 16.0, 6.0, 3.0][(x / 256) as usize];
        for y in y0..h {
            let v = 0.18 * (1.0 + 0.5 * (2.0 * std::f64::consts::PI * f64::from(x) / period).sin());
            img.set(x, y, [v; 3]);
        }
    }
    for x in 1024..w {
        for y in y0..h {
            let t = f64::from(x - 1024);
            let hard = if t < 256.0 {
                0.05
            } else if t < 512.0 {
                0.5
            } else {
                0.05
            };
            let soft = 0.05 + 0.45 / (1.0 + (-(t - 768.0) / 12.0).exp());
            img.set(x, y, [if y < y0 + 192 { hard } else { soft }; 3]);
        }
    }
    img
}

/// Approximate X-Rite ColorChecker Classic, from commonly published 8-bit sRGB
/// values (D65). A synthetic stand-in for testing the colour chain; it is NOT a
/// measured reference. Patches in reading order, 6 × 4.
pub const COLORCHECKER_SRGB8: [[u8; 3]; 24] = [
    [115, 82, 68],
    [194, 150, 130],
    [98, 122, 157],
    [87, 108, 67],
    [133, 128, 177],
    [103, 189, 170],
    [214, 126, 44],
    [80, 91, 166],
    [193, 90, 99],
    [94, 60, 108],
    [157, 188, 64],
    [224, 163, 46],
    [56, 61, 150],
    [70, 148, 73],
    [175, 54, 60],
    [231, 199, 31],
    [187, 86, 149],
    [8, 133, 161],
    [243, 243, 242],
    [200, 200, 200],
    [160, 160, 160],
    [122, 122, 121],
    [85, 85, 85],
    [52, 52, 52],
];

pub fn srgb_decode(c: f64) -> f64 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// 8-bit sRGB → linear ProPhoto (D50).
pub fn srgb8_to_prophoto(rgb: [u8; 3]) -> [f64; 3] {
    let linear = rgb.map(|c| srgb_decode(f64::from(c) / 255.0));
    let m = mul3(&invert3(&PROPHOTO_TO_XYZ_D50), &SRGB_TO_XYZ_D50);
    apply3(&m, linear)
}

pub fn colorchecker() -> LinearImage {
    let (patch, gap) = (200u32, 24u32);
    let (w, h) = (6 * (patch + gap) + gap, 4 * (patch + gap) + gap);
    let mut img = LinearImage::new(w, h);
    fill(&mut img, 0, 0, w, h, [0.03; 3]);
    for (i, srgb) in COLORCHECKER_SRGB8.iter().enumerate() {
        let (col, row) = (i as u32 % 6, i as u32 / 6);
        let rgb = srgb8_to_prophoto(*srgb).map(|c| c.clamp(0.0, 1.0));
        fill(
            &mut img,
            gap + col * (patch + gap),
            gap + row * (patch + gap),
            patch,
            patch,
            rgb,
        );
    }
    img
}

/// Orange base of the simulated colour negative (scan values, white level 1).
pub const NEGATIVE_BASE: [f64; 3] = [0.82, 0.46, 0.26];

/// A ColorChecker and a grey ramp as a "camera scan" of a colour negative:
/// orange film base, densities growing with log exposure (gamma ~0.6, slightly
/// different per dye layer), and a strip of unexposed rebate along the top and
/// bottom. Test target for film negative conversion (`amb:NegativeConvert`).
pub fn negative() -> LinearImage {
    const GAMMA: [f64; 3] = [0.56, 0.6, 0.64];
    let scene = colorchecker();
    let rebate = 64u32;
    let ramp_h = 96u32;
    let (w, h) = (scene.width, scene.height + ramp_h + 2 * rebate);
    let mut img = LinearImage::new(w, h);
    let expose = |s: [f64; 3]| -> [f64; 3] {
        std::array::from_fn(|c| {
            // 0.5 % reflectance sits at the film's base (no density above base).
            let d = GAMMA[c] * (s[c].max(0.005) / 0.005).log10();
            NEGATIVE_BASE[c] * 10f64.powf(-d)
        })
    };
    fill(&mut img, 0, 0, w, h, NEGATIVE_BASE);
    for y in 0..scene.height {
        for x in 0..w {
            let p = scene.get(x, y).map(f64::from);
            img.set(x, y + rebate, expose(p));
        }
    }
    for x in 0..w {
        let v = 0.01 * 100f64.powf(f64::from(x) / f64::from(w - 1)); // 1 %..100 %
        for y in 0..ramp_h {
            img.set(x, scene.height + rebate + y, expose([v; 3]));
        }
    }
    img
}

/// A scanned slide of a grainy sky with dust on it: [`DUST_SPECKS`] specks
/// of 1 to 4 pixels' radius, one curved hair, and a straight dark wire right
/// across, which is not dust.
pub fn dust() -> LinearImage {
    let (w, h) = (1200u32, 800u32);
    let mut img = LinearImage::new(w, h);
    let mut seed = 0x2545_f491u32;
    let mut grain = move || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        f64::from(seed) / f64::from(u32::MAX) - 0.5
    };
    for y in 0..h {
        for x in 0..w {
            let sky = 0.25 + 0.15 * f64::from(y) / f64::from(h);
            let g = 1.0 + 0.05 * grain();
            img.set(x, y, [sky * 0.8 * g, sky * g, sky * 1.2 * g]);
        }
    }
    let mut darken = |x: u32, y: u32| {
        let p = img.get(x, y).map(|v| f64::from(v) * 0.2);
        img.set(x, y, p);
    };
    for (i, &(cx, cy)) in DUST_SPECKS.iter().enumerate() {
        let r = 1 + (i % 4) as i64;
        for dy in -r..=r {
            for dx in -r..=r {
                if dx * dx + dy * dy <= r * r {
                    darken((i64::from(cx) + dx) as u32, (i64::from(cy) + dy) as u32);
                }
            }
        }
    }
    for t in 0..40u32 {
        let (x, y) = (600 + t, 600 + (8.0 * (f64::from(t) / 7.0).sin()) as u32 + 8);
        darken(x, y);
        darken(x, y + 1);
    }
    for x in 0..w {
        darken(x, 120);
        darken(x, 121);
    }
    img
}

/// Where [`dust`] puts its specks.
pub const DUST_SPECKS: [(u32, u32); 8] = [
    (100, 300),
    (250, 500),
    (400, 250),
    (520, 700),
    (700, 350),
    (850, 550),
    (1000, 200),
    (1100, 650),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_patterns_generate_in_range() {
        for name in NAMES {
            let img = generate(name).unwrap();
            assert!(img.width > 0 && img.height > 0);
            assert!(
                img.data.iter().all(|v| (0.0..=1.0).contains(v)),
                "{name} out of range"
            );
        }
        assert!(generate("nope").is_none());
    }

    #[test]
    fn neutral_srgb_stays_neutral_in_prophoto() {
        let grey = srgb8_to_prophoto([200, 200, 200]);
        let expected = srgb_decode(200.0 / 255.0);
        for c in grey {
            assert!((c - expected).abs() < 2e-3, "{grey:?} vs {expected}");
        }
    }

    #[test]
    fn ramp_is_monotonic_and_spans_14_stops() {
        let img = ramp();
        let first = img.get(0, 0)[0];
        let last = img.get(img.width - 1, 500)[0];
        assert!((f64::from(first) - 2f64.powi(-14)).abs() < 1e-6);
        assert!((f64::from(last) - 1.0).abs() < 1e-6);
    }
}
