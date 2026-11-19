// SPDX-License-Identifier: GPL-3.0-or-later
//! A look's statistics, and how far two looks are apart.

use super::Weights;
use crate::pipeline::ProPhotoImage;
use crate::stats::lab;
use serde::Serialize;

/// The percentiles of L* in [`LookStats::lightness`].
pub const L_PERCENTILES: [f32; 9] = [1.0, 5.0, 10.0, 25.0, 50.0, 75.0, 90.0, 95.0, 99.0];

/// A look as numbers.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LookStats {
    /// CIE L* at [`L_PERCENTILES`].
    pub lightness: [f32; 9],
    /// Mean (a*, b*) of the shadows, midtones and highlights (soft zones of L*).
    pub zones: [[f32; 2]; 3],
    /// Chroma C*ab at the 50th and 90th percentiles.
    pub chroma: [f32; 2],
    /// Spread of (a*, b*) about its mean: the standard deviations of a* and
    /// b*, and the signed root of their covariance. Saturation scales it,
    /// white balance barely moves it, so the two can be told apart.
    pub spread: [f32; 3],
    /// Per HSL band (in [`crate::color::BANDS`] order, weighed as the Color
    /// Mixer weighs colours): its share of the photo, and the mean OkLCh
    /// chroma, lightness and hue offset from the band's centre (radians) of
    /// its colours.
    pub bands: [[f32; 4]; 8],
    /// Mean (a*, b*) of the near-grey midtones (chroma under
    /// [`NEUTRAL_CHROMA`]) and their share of the photo: the white balance,
    /// read where the scene is probably grey.
    pub neutral: [f32; 3],
}

/// Chroma (C*ab) under which a midtone counts as near grey for [`LookStats::neutral`].
pub const NEUTRAL_CHROMA: f64 = 8.0;

/// Least share of near-grey midtones for the white balance to be read from
/// them. Below it the few near-grey pixels are as likely a coloured object
/// (a golden ball in red cloth reads as 3.8% warm "grey") as the light.
const MIN_NEUTRAL_SHARE: f32 = 0.05;

/// Weights of a pixel of lightness `l` in the shadow, midtone and highlight zones.
fn zone_weights(l: f64) -> [f64; 3] {
    let shadow = ((55.0 - l) / 40.0).clamp(0.0, 1.0);
    let highlight = ((l - 45.0) / 40.0).clamp(0.0, 1.0);
    let mid = (1.0 - (l - 50.0).abs() / 35.0).clamp(0.0, 1.0);
    [shadow, mid, highlight]
}

fn percentile(sorted: &[f32], q: f32) -> f32 {
    if sorted.is_empty() {
        return 0.0;
    }
    let pos = (sorted.len() - 1) as f32 * q / 100.0;
    let i = pos.floor() as usize;
    let j = (i + 1).min(sorted.len() - 1);
    sorted[i] + (sorted[j] - sorted[i]) * (pos - i as f32)
}

/// Measure a developed (linear ProPhoto) image.
pub fn look_stats(img: &ProPhotoImage) -> LookStats {
    let n = img.rgb.len() / 3;
    let mut ls = Vec::with_capacity(n);
    let mut cs = Vec::with_capacity(n);
    let mut sums = [[0.0f64; 3]; 3];
    let mut moments = [0.0f64; 5];
    let mut neutral = [0.0f64; 3];
    let ok = crate::color::OkLab::new();
    let centres = crate::color::hsl_band_hues();
    let mut band_sums = [[0.0f64; 4]; 8];
    for p in img.rgb.chunks_exact(3) {
        let [ol, oa, ob] = ok.from_working([p[0], p[1], p[2]]);
        if ol.is_finite() && oa.is_finite() && ob.is_finite() {
            let (chroma, hue) = (oa.hypot(ob), ob.atan2(oa));
            for (i, w) in crate::color::hsl_band_membership(hue, chroma) {
                if w > 0.0 {
                    let d = (hue - centres[i] + std::f32::consts::PI)
                        .rem_euclid(std::f32::consts::TAU)
                        - std::f32::consts::PI;
                    let w = w as f64;
                    let sum = &mut band_sums[i];
                    sum[0] += w;
                    sum[1] += w * chroma as f64;
                    sum[2] += w * ol as f64;
                    sum[3] += w * d as f64;
                }
            }
        }
        let [l, a, b] = lab(p);
        for (m, v) in moments.iter_mut().zip([a, b, a * a, b * b, a * b]) {
            *m += v;
        }
        if (25.0..=85.0).contains(&l) && a.hypot(b) < NEUTRAL_CHROMA {
            neutral[0] += a;
            neutral[1] += b;
            neutral[2] += 1.0;
        }
        let l = l.clamp(0.0, 100.0);
        ls.push(l as f32);
        cs.push(a.hypot(b) as f32);
        for (sum, w) in sums.iter_mut().zip(zone_weights(l)) {
            sum[0] += w * a;
            sum[1] += w * b;
            sum[2] += w;
        }
    }
    ls.sort_by(f32::total_cmp);
    cs.sort_by(f32::total_cmp);
    let k = n.max(1) as f64;
    let [ma, mb, aa, bb, ab] = moments.map(|m| m / k);
    let cov = ab - ma * mb;
    let spread = [
        (aa - ma * ma).max(0.0).sqrt() as f32,
        (bb - mb * mb).max(0.0).sqrt() as f32,
        (cov.signum() * cov.abs().sqrt()) as f32,
    ];
    LookStats {
        lightness: L_PERCENTILES.map(|q| percentile(&ls, q)),
        zones: sums.map(|[a, b, w]| {
            if w > 1e-9 {
                [(a / w) as f32, (b / w) as f32]
            } else {
                [0.0, 0.0]
            }
        }),
        chroma: [percentile(&cs, 50.0), percentile(&cs, 90.0)],
        spread,
        neutral: if neutral[2] > 0.0 {
            [
                (neutral[0] / neutral[2]) as f32,
                (neutral[1] / neutral[2]) as f32,
                (neutral[2] / k) as f32,
            ]
        } else {
            [0.0; 3]
        },
        bands: band_sums.map(|[w, c, l, d]| {
            if w > 1e-9 {
                [
                    (w / k) as f32,
                    (c / w) as f32,
                    (l / w) as f32,
                    (d / w) as f32,
                ]
            } else {
                [0.0; 4]
            }
        }),
    }
}

/// The mean of several looks (several references make one target).
pub fn average(looks: &[LookStats]) -> Option<LookStats> {
    let n = looks.len() as f32;
    let first = looks.first()?;
    let mut out = first.clone();
    for (i, v) in out.lightness.iter_mut().enumerate() {
        *v = looks.iter().map(|s| s.lightness[i]).sum::<f32>() / n;
    }
    for z in 0..3 {
        for c in 0..2 {
            out.zones[z][c] = looks.iter().map(|s| s.zones[z][c]).sum::<f32>() / n;
        }
    }
    for c in 0..2 {
        out.chroma[c] = looks.iter().map(|s| s.chroma[c]).sum::<f32>() / n;
    }
    for c in 0..3 {
        out.spread[c] = looks.iter().map(|s| s.spread[c]).sum::<f32>() / n;
    }
    for b in 0..8 {
        for c in 0..4 {
            out.bands[b][c] = looks.iter().map(|s| s.bands[b][c]).sum::<f32>() / n;
        }
    }
    for c in 0..3 {
        out.neutral[c] = looks.iter().map(|s| s.neutral[c]).sum::<f32>() / n;
    }
    Some(out)
}

/// Least share of the photo, in both looks, for a band's colours to count.
pub(super) const MIN_BAND_SHARE: f32 = 0.01;

/// The differences between two looks, weighted so their squares add up to
/// [`distance`]; with `bands`, also each HSL band's colours where both
/// looks have them (in OkLCh, times 100 to be near Lab units, weighted by
/// how much of the photos they cover).
pub(super) fn residuals(a: &LookStats, b: &LookStats, k: &Weights) -> Vec<f64> {
    let mut r = Vec::with_capacity(9 + 6 + 2 + 3 + 2);
    let tone = k.tone * (1.0 / a.lightness.len() as f64).sqrt();
    r.extend(
        a.lightness
            .iter()
            .zip(&b.lightness)
            .map(|(x, y)| tone * (x - y) as f64),
    );
    for (w, (x, y)) in [0.5f64, 1.0, 0.5].iter().zip(a.zones.iter().zip(&b.zones)) {
        r.push(k.cast * w.sqrt() * (x[0] - y[0]) as f64);
        r.push(k.cast * w.sqrt() * (x[1] - y[1]) as f64);
    }
    r.extend(
        a.chroma
            .iter()
            .zip(&b.chroma)
            .map(|(x, y)| k.colour * (x - y) as f64 / 2f64.sqrt()),
    );
    r.extend(
        a.spread
            .iter()
            .zip(&b.spread)
            .map(|(x, y)| k.colour * (x - y) as f64),
    );
    // The near-grey cast, where both looks have enough near-grey to read it.
    let w = if a.neutral[2].min(b.neutral[2]) >= MIN_NEUTRAL_SHARE {
        k.neutral
    } else {
        0.0
    };
    r.push(w * (a.neutral[0] - b.neutral[0]) as f64);
    r.push(w * (a.neutral[1] - b.neutral[1]) as f64);
    if k.bands {
        for (x, y) in a.bands.iter().zip(&b.bands) {
            let share = x[0].min(y[0]);
            let w = if share < MIN_BAND_SHARE {
                0.0
            } else {
                (8.0 * share as f64).min(2.0).sqrt() * 100.0
            };
            r.push(w * (x[1] - y[1]) as f64);
            r.push(w * (x[2] - y[2]) as f64);
            r.push(w * (x[3] - y[3]) as f64 * y[1] as f64);
        }
    }
    r
}

/// How far apart two looks are, in squared Lab units: the tone (L*
/// percentiles), the zone casts, chroma and spread.
pub fn distance(a: &LookStats, b: &LookStats) -> f64 {
    distance_with(a, b, false)
}

/// [`distance`], with the HSL bands' colours too when `bands`.
pub fn distance_with(a: &LookStats, b: &LookStats, bands: bool) -> f64 {
    weighted_distance(
        a,
        b,
        &Weights {
            bands,
            ..Weights::default()
        },
    )
}

pub(super) fn weighted_distance(a: &LookStats, b: &LookStats, k: &Weights) -> f64 {
    residuals(a, b, k).iter().map(|r| r * r).sum()
}
