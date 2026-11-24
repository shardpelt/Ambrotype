// SPDX-License-Identifier: GPL-3.0-or-later
//! Fade Restoration (`amb:FadeRestore`): a faded print or slide given back
//! its blacks, its contrast and the colours that faded unevenly. Our own
//! model, not anyone else's: each channel's levels are measured in log
//! (density-like) units inside the crop and stretched to a full range, its
//! midtone matched to green's, and the colours given back a little of the
//! saturation that fading took. See docs/design/film-negatives.md.

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::math::{downsample, percentile};

/// Samples the photo is block-averaged to before it is measured (about a
/// 500 × 500 copy): enough for steady percentiles, fast on any photo.
pub const FADE_SAMPLES: usize = 250_000;
/// A channel that spans less than this, in decades, between its levels has
/// nothing to stretch (a flat or empty channel): it is left alone. Ours.
pub const FADE_MIN_SPAN: f64 = 0.05;
/// Fraction of each side not measured (a print's border, a slide's mount). Ours.
pub const FADE_BORDER: f64 = 0.03;
/// The percentiles taken as a channel's black and white (0..1). Ours.
pub const FADE_LOW_PCT: f64 = 0.002;
pub const FADE_HIGH_PCT: f64 = 0.998;
/// The most a channel's midtone gamma may bend to meet green's. Ours.
pub const FADE_GAMMA_MIN: f64 = 0.7;
pub const FADE_GAMMA_MAX: f64 = 1.4;
/// Linear values a channel's black and white levels are stretched to. Ours.
pub const FADE_BLACK: f64 = 0.004;
pub const FADE_WHITE: f64 = 0.90;
/// Saturation given back, as a share of each pixel's difference from its
/// grey (0.25 = a quarter more). Ours.
pub const FADE_SATURATION: f64 = 0.25;
/// Linear values below this are read as this before taking the log.
const FADE_FLOOR: f64 = 1e-5;

/// Model constants (`amb calibration defaults`, section `fade`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FadeStrengths {
    /// Fraction of each side not measured.
    pub border: f64,
    /// Percentiles taken as a channel's black and white.
    pub low_pct: f64,
    pub high_pct: f64,
    /// The clamp of a channel's midtone gamma.
    pub gamma_min: f64,
    pub gamma_max: f64,
    /// Linear black and white the levels are stretched to.
    pub black: f64,
    pub white: f64,
    /// Saturation given back (share of the difference from grey).
    pub saturation: f64,
}

pub const DEFAULT_STRENGTHS: FadeStrengths = FadeStrengths {
    border: FADE_BORDER,
    low_pct: FADE_LOW_PCT,
    high_pct: FADE_HIGH_PCT,
    gamma_min: FADE_GAMMA_MIN,
    gamma_max: FADE_GAMMA_MAX,
    black: FADE_BLACK,
    white: FADE_WHITE,
    saturation: FADE_SATURATION,
};

impl Default for FadeStrengths {
    fn default() -> Self {
        DEFAULT_STRENGTHS
    }
}

/// What a photo's fade measures: per channel, the black and white levels
/// (log10 of linear) and the gamma that brings its midtone to green's.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Analysis {
    pub low: [f64; 3],
    pub high: [f64; 3],
    pub gamma: [f64; 3],
}

impl Analysis {
    /// Whether channel `c` has a range to stretch.
    fn spans(&self, c: usize) -> bool {
        self.high[c] - self.low[c] >= FADE_MIN_SPAN
    }
}

fn log(v: f64) -> f64 {
    v.max(FADE_FLOOR).log10()
}

/// Measure a faded photo (interleaved linear RGB, `width` × `height`): the
/// whole of it but its border.
pub fn analyze(rgb: &[f32], width: usize, height: usize, k: &FadeStrengths) -> Analysis {
    let (blocks, bw, bh) = downsample(rgb, width, height, FADE_SAMPLES);
    let (mx, my) = (
        (bw as f64 * k.border) as usize,
        (bh as f64 * k.border) as usize,
    );
    let inside: Vec<[f64; 3]> = blocks
        .iter()
        .enumerate()
        .filter(|(i, _)| {
            let (x, y) = (i % bw, i / bw);
            x >= mx && x + mx < bw && y >= my && y + my < bh
        })
        .map(|(_, p)| p.map(log))
        .collect();
    let samples = if inside.is_empty() {
        blocks.iter().map(|p| p.map(log)).collect()
    } else {
        inside
    };
    let mut low = [0.0; 3];
    let mut high = [0.0; 3];
    let mut median = [0.0; 3];
    for c in 0..3 {
        let mut v: Vec<f64> = samples.iter().map(|p| p[c]).collect();
        low[c] = percentile(&mut v, k.low_pct);
        high[c] = percentile(&mut v, k.high_pct);
        let span = (high[c] - low[c]).max(FADE_MIN_SPAN);
        let mut t: Vec<f64> = v
            .iter()
            .map(|x| ((x - low[c]) / span).clamp(0.0, 1.0))
            .collect();
        median[c] = percentile(&mut t, 0.5);
    }
    let gamma = std::array::from_fn(|c| {
        let (m, target) = (median[c], median[1]);
        let usable = |x: f64| x > 1e-3 && x < 1.0 - 1e-3;
        if c == 1 || !usable(m) || !usable(target) || high[c] - low[c] < FADE_MIN_SPAN {
            return 1.0;
        }
        (target.ln() / m.ln()).clamp(k.gamma_min, k.gamma_max)
    });
    Analysis { low, high, gamma }
}

/// Restore `rgb` (interleaved linear RGB) by `amount` (0..100) with the
/// measured fade. 0 leaves every pixel as it is. A channel without a range
/// stays as it is, and the saturation is only given back when all three
/// have one (a toned or one-colour picture has no colours to bring back).
pub fn apply(rgb: &mut [f32], a: &Analysis, amount: f64, k: &FadeStrengths) {
    let mix = (amount / 100.0).clamp(0.0, 1.0);
    if mix <= 0.0 {
        return;
    }
    let (black, white) = (log(k.black), log(k.white));
    let spans: [bool; 3] = std::array::from_fn(|c| a.spans(c));
    let saturate = spans.iter().all(|&s| s);
    rgb.par_chunks_mut(3).for_each(|p| {
        let x: [f64; 3] = std::array::from_fn(|c| f64::from(p[c]));
        let mut r: [f64; 3] = std::array::from_fn(|c| {
            if !spans[c] {
                return x[c];
            }
            let t = ((log(x[c]) - a.low[c]) / (a.high[c] - a.low[c])).clamp(0.0, 1.0);
            10f64.powf(black + (white - black) * t.powf(a.gamma[c]))
        });
        if saturate {
            let grey = (r[0] + r[1] + r[2]) / 3.0;
            for v in &mut r {
                *v = (grey + (*v - grey) * (1.0 + k.saturation)).max(0.0);
            }
        }
        for c in 0..3 {
            p[c] = (x[c] + (r[c] - x[c]) * mix) as f32;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: usize = 240;
    const H: usize = 160;

    /// A test print: a grey ramp across the top half, colour patches below,
    /// each spanning the full range from black to white.
    fn scene() -> Vec<f32> {
        let (lo, hi) = (FADE_BLACK.log10(), FADE_WHITE.log10());
        let tints = [
            [1.0, 1.0, 1.0],
            [1.3, 0.9, 0.8],
            [0.8, 1.0, 1.3],
            [0.9, 1.2, 0.9],
        ];
        let mut rgb = Vec::with_capacity(W * H * 3);
        for y in 0..H {
            for x in 0..W {
                let t = x as f64 / (W - 1) as f64;
                let v = 10f64.powf(lo + (hi - lo) * t);
                let tint = if y < H / 2 {
                    tints[0]
                } else {
                    tints[(x * 4 / W + y) % 4]
                };
                for c in tint {
                    rgb.push((v * c).clamp(FADE_BLACK, FADE_WHITE) as f32);
                }
            }
        }
        rgb
    }

    /// Fade a print: the log range squeezed (blacks up to 0.08, white down
    /// to 0.6), red and blue fading unevenly (×1.15 and ×0.8 in log), then
    /// 40 % of the colour gone.
    fn fade(rgb: &mut [f32]) {
        let (lo, hi) = (FADE_BLACK.log10(), FADE_WHITE.log10());
        let (flo, fhi) = (0.08f64.log10(), 0.6f64.log10());
        let cast = [1.15, 1.0, 0.8];
        for p in rgb.chunks_mut(3) {
            let v: [f64; 3] = std::array::from_fn(|c| {
                let t = (log(f64::from(p[c])) - lo) / (hi - lo);
                10f64.powf((flo + (fhi - flo) * t) * cast[c])
            });
            let grey = (v[0] + v[1] + v[2]) / 3.0;
            for c in 0..3 {
                p[c] = (grey + (v[c] - grey) * 0.6) as f32;
            }
        }
    }

    fn log_error(a: &[f32], b: &[f32]) -> f64 {
        let sum: f64 = a
            .iter()
            .zip(b)
            .map(|(x, y)| (log(f64::from(*x)) - log(f64::from(*y))).abs())
            .sum();
        sum / a.len() as f64
    }

    #[test]
    fn a_faded_print_comes_back() {
        let original = scene();
        let mut faded = original.clone();
        fade(&mut faded);
        let a = analyze(&faded, W, H, &DEFAULT_STRENGTHS);
        let mut restored = faded.clone();
        apply(&mut restored, &a, 100.0, &DEFAULT_STRENGTHS);
        let (before, after) = (
            log_error(&faded, &original),
            log_error(&restored, &original),
        );
        // Mean absolute log10 error to the original: 0.60 faded, 0.024 restored.
        assert!(
            after < before / 2.0,
            "faded {before:.3}, restored {after:.3}"
        );
    }

    #[test]
    fn amount_zero_changes_nothing() {
        let mut rgb = scene();
        fade(&mut rgb);
        let a = analyze(&rgb, W, H, &DEFAULT_STRENGTHS);
        let before = rgb.clone();
        apply(&mut rgb, &a, 0.0, &DEFAULT_STRENGTHS);
        assert!(
            rgb.iter()
                .zip(&before)
                .all(|(x, y)| x.to_bits() == y.to_bits())
        );
    }

    #[test]
    fn a_flat_channel_is_left_alone() {
        let mut rgb = scene();
        fade(&mut rgb);
        for p in rgb.chunks_mut(3) {
            p[2] = 0.3;
        }
        let a = analyze(&rgb, W, H, &DEFAULT_STRENGTHS);
        assert_eq!(a.gamma[2], 1.0);
        let before = rgb.clone();
        apply(&mut rgb, &a, 100.0, &DEFAULT_STRENGTHS);
        assert!(
            rgb.iter()
                .zip(&before)
                .skip(2)
                .step_by(3)
                .all(|(x, y)| x == y)
        );
        assert_ne!(rgb, before, "red and green are restored");
    }

    #[test]
    fn gamma_stays_in_its_clamp() {
        // All red: green and blue are near black with a little noise.
        let rgb: Vec<f32> = (0..W * H)
            .flat_map(|i| {
                let t = (i % W) as f32 / W as f32;
                let n = ((i * 7919) % 97) as f32 / 97.0;
                [0.05 + 0.85 * t, 0.002 + 0.01 * n, 0.002 + 0.01 * n * n]
            })
            .collect();
        let a = analyze(&rgb, W, H, &DEFAULT_STRENGTHS);
        for g in a.gamma {
            assert!((FADE_GAMMA_MIN..=FADE_GAMMA_MAX).contains(&g), "{a:?}");
        }
    }
}
