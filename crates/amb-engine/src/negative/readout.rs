// SPDX-License-Identifier: GPL-3.0-or-later
//! Darkroom readouts on a scan: how wide the negative's range is, how much
//! of the scan clipped, and the density and zone at a point. Read only:
//! nothing here changes a conversion.

use super::*;

/// A scan sample counts as clipped from this share of the scan's white. The
/// scan's white is 1.0: camera RGB is normalised to the sensor's white level
/// (the base estimate treats 0.97 as clipped, this is the strict end).
pub const SCAN_CLIP: f64 = 0.995;

/// Samples the clipping share is counted on (the scan, block-averaged).
const CLIP_SAMPLES: usize = 250_000;

/// Side of the block of pixels a probe averages, in pixels (odd, so it is
/// centred on the point; a few pixels beat the noise of one).
pub const PROBE_BLOCK: usize = 5;

/// 18 % grey, the print's middle: zone V in Ansel Adams' zone system.
pub const ZONE_GREY: f64 = 0.18;
/// Zone V is the middle of the eleven zones, 0 to X.
const ZONE_MIDDLE: i32 = 5;
const ZONE_MAX: i32 = 10;
/// The zones' names, 0 to X, as the zone system writes them.
pub const ZONE_NAMES: [&str; 11] = [
    "0", "I", "II", "III", "IV", "V", "VI", "VII", "VIII", "IX", "X",
];

/// What a scan says about the negative on it.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct ScanStats {
    /// Per channel, the levels' span in density (decades): the range of the
    /// negative that the print spans.
    pub range: [f64; 3],
    /// Share (0..1) of the metered scan with any channel at the scan's white.
    pub clipped: f64,
}

/// What is under one point of the scan.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct Probe {
    /// Density above the film base, per channel.
    pub density: [f64; 3],
    /// The zone (0 to X) of the printed value there.
    pub zone: u8,
}

/// The range of `a`'s levels and the clipped share of `metered`.
pub fn scan_stats(metered: Scan, a: &Analysis) -> ScanStats {
    let range = std::array::from_fn(|c| a.density_high[c] - a.density_low[c]);
    let (blocks, ..) =
        crate::math::downsample(metered.rgb, metered.width, metered.height, CLIP_SAMPLES);
    let clipped = blocks
        .iter()
        .filter(|p| p.iter().any(|v| *v >= SCAN_CLIP))
        .count();
    ScanStats {
        range,
        clipped: clipped as f64 / blocks.len().max(1) as f64,
    }
}

/// The zone of a printed value: one stop per zone, V is 18 % grey.
pub fn zone_of(linear: f64) -> u8 {
    (zone_exact(linear).round() as i32).clamp(0, ZONE_MAX) as u8
}

/// The zone of a printed value without rounding or clamping: a stop is one
/// zone, so 18 % grey is exactly 5.
pub fn zone_exact(linear: f64) -> f64 {
    (linear.max(1e-9) / ZONE_GREY).log2() + f64::from(ZONE_MIDDLE)
}

/// The linear-light value of an 8-bit sRGB pixel as the zone overlay reads
/// it: Rec. 709 luma of the encoded values, then the inverse sRGB curve.
pub fn srgb8_linear_luma(rgb: [u8; 3]) -> f64 {
    let v = (0.2126 * f64::from(rgb[0]) + 0.7152 * f64::from(rgb[1]) + 0.0722 * f64::from(rgb[2]))
        / 255.0;
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

/// The density and zone at `at` (fractions of the scan, left-top). The
/// density is the mean over a [`PROBE_BLOCK`] block of −log10(T / base); the
/// zone is that of the block's mean converted to the print (the mean of its
/// layers, which are equal on a neutral).
pub fn probe(scan: Scan, a: &Analysis, conversion: &Conversion, at: [f64; 2]) -> Probe {
    let (w, h) = (scan.width.max(1), scan.height.max(1));
    let centre = |f: f64, n: usize| ((f.clamp(0.0, 1.0) * n as f64) as usize).min(n - 1);
    let (cx, cy) = (centre(at[0], w), centre(at[1], h));
    let half = PROBE_BLOCK / 2;
    let mut density = [0.0f64; 3];
    let mut mean = [0.0f64; 3];
    let mut n = 0.0;
    for y in cy.saturating_sub(half)..(cy + half + 1).min(h) {
        for x in cx.saturating_sub(half)..(cx + half + 1).min(w) {
            let i = (y * scan.width + x) * 3;
            let Some(p) = scan.rgb.get(i..i + 3) else {
                continue;
            };
            for c in 0..3 {
                let t = f64::from(p[c]);
                density[c] += -(t.max(1e-6) / a.base[c].max(1e-6)).log10();
                mean[c] += t;
            }
            n += 1.0;
        }
    }
    let n = f64::max(n, 1.0);
    let printed = conversion.apply(mean.map(|m| (m / n) as f32));
    let l = printed.iter().map(|v| f64::from(*v)).sum::<f64>() / 3.0;
    Probe {
        density: density.map(|d| d / n),
        zone: zone_of(l),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: [f64; 3] = [0.8, 0.45, 0.25];

    fn analysis() -> Analysis {
        Analysis {
            base: BASE,
            density_low: [0.1, 0.05, 0.02],
            density_high: [1.2, 1.1, 0.64],
            midtone: 0.5,
            crossover: NO_CROSSOVER,
        }
    }

    #[test]
    fn range_is_high_minus_low() {
        let rgb = vec![0.5f32; 30 * 3];
        let s = scan_stats(
            Scan {
                rgb: &rgb,
                width: 10,
                height: 3,
            },
            &analysis(),
        );
        assert!((s.range[0] - 1.1).abs() < 1e-12);
        assert!((s.range[1] - 1.05).abs() < 1e-12);
        assert!((s.range[2] - 0.62).abs() < 1e-12);
        assert_eq!(s.clipped, 0.0);
    }

    #[test]
    fn two_percent_white_is_two_percent_clipped() {
        let mut rgb = vec![0.5f32; 100 * 100 * 3];
        for p in rgb.chunks_mut(3).take(200) {
            p.copy_from_slice(&[1.0, 1.0, 1.0]);
        }
        let s = scan_stats(
            Scan {
                rgb: &rgb,
                width: 100,
                height: 100,
            },
            &analysis(),
        );
        assert!((s.clipped - 0.02).abs() < 1e-9, "{}", s.clipped);
    }

    #[test]
    fn probe_reads_the_density_of_a_flat_patch() {
        let t = [0.4f32, 0.3, 0.1];
        let rgb: Vec<f32> = (0..20 * 20).flat_map(|_| t).collect();
        let scan = Scan {
            rgb: &rgb,
            width: 20,
            height: 20,
        };
        let a = analysis();
        let n = Negative::default();
        let conv = Conversion::new(&n, a, &DEFAULT_STRENGTHS);
        let p = probe(scan, &a, &conv, [0.5, 0.5]);
        for c in 0..3 {
            let want = -(f64::from(t[c]) / BASE[c]).log10();
            assert!((p.density[c] - want).abs() < 1e-3, "{c}: {:?}", p.density);
        }
        // Corners are clamped inside the scan.
        let _ = probe(scan, &a, &conv, [1.0, 1.0]);
    }

    #[test]
    fn zones_are_stops_from_grey() {
        assert_eq!(zone_of(0.18), 5);
        assert_eq!(zone_of(0.18 * 8.0), 8);
        assert_eq!(zone_of(0.18 / 4.0), 3);
        assert_eq!(zone_of(1e-9), 0);
        assert_eq!(zone_of(0.0), 0);
        assert_eq!(zone_of(100.0), 10);
    }
}
