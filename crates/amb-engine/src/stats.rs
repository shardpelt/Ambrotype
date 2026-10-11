// SPDX-License-Identifier: GPL-3.0-or-later
//! A developed photo as a few numbers (`amb stats`): lightness percentiles,
//! clipping and colour cast. An AI client reads these in a few dozen tokens
//! where a picture costs a thousand or more.

use amb_color::matrix::{PROPHOTO_TO_XYZ_D50, mul3, xyz_d50_to_srgb};
use serde::Serialize;

use crate::pipeline::ProPhotoImage;

/// The percentiles of CIE L* that [`Stats::lightness`] holds.
pub const PERCENTILES: [u8; 7] = [1, 5, 25, 50, 75, 95, 99];

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Stats {
    pub width: usize,
    pub height: usize,
    /// CIE L* (0-100) at [`PERCENTILES`].
    pub lightness: [f32; 7],
    pub mean_lightness: f32,
    /// Share (%) of pixels black in sRGB (every channel at most 1/255).
    pub clipped_shadows: f32,
    /// Share (%) of pixels with a channel white in sRGB (at least 254/255),
    /// and per channel.
    pub clipped_highlights: f32,
    pub clipped_rgb: [f32; 3],
    /// Mean a* and b* of the midtones (L* 20-80): the colour cast.
    pub cast: [f32; 2],
    /// Mean chroma C*ab over the photo.
    pub chroma: f32,
}

const XN: f64 = 0.9642;
const ZN: f64 = 0.8249;

fn lab_f(t: f64) -> f64 {
    const EPS: f64 = 216.0 / 24389.0;
    const KAPPA: f64 = 24389.0 / 27.0;
    if t > EPS {
        t.cbrt()
    } else {
        (KAPPA * t + 16.0) / 116.0
    }
}

fn srgb_oetf(v: f64) -> f64 {
    let v = v.clamp(0.0, 1.0);
    if v <= 0.003_130_8 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    }
}

/// Measure a developed (linear ProPhoto) image.
pub fn measure(img: &ProPhotoImage) -> Stats {
    let to_srgb = mul3(&xyz_d50_to_srgb(), &PROPHOTO_TO_XYZ_D50);
    let n = img.rgb.len() / 3;
    let mut l_values = Vec::with_capacity(n);
    let (mut l_sum, mut chroma_sum) = (0.0f64, 0.0f64);
    let (mut a_mid, mut b_mid, mut n_mid) = (0.0f64, 0.0f64, 0usize);
    let (mut dark, mut bright, mut per) = (0usize, 0usize, [0usize; 3]);
    for p in img.rgb.chunks_exact(3) {
        let p = [p[0] as f64, p[1] as f64, p[2] as f64];
        let dot = |m: &[f64; 3]| m[0] * p[0] + m[1] * p[1] + m[2] * p[2];
        let xyz = PROPHOTO_TO_XYZ_D50.map(|row| dot(&row));
        let (fx, fy, fz) = (
            lab_f(xyz[0].max(0.0) / XN),
            lab_f(xyz[1].max(0.0)),
            lab_f(xyz[2].max(0.0) / ZN),
        );
        let l = 116.0 * fy - 16.0;
        let (a, b) = (500.0 * (fx - fy), 200.0 * (fy - fz));
        l_values.push(l.clamp(0.0, 100.0) as f32);
        l_sum += l.clamp(0.0, 100.0);
        chroma_sum += a.hypot(b);
        if (20.0..=80.0).contains(&l) {
            a_mid += a;
            b_mid += b;
            n_mid += 1;
        }
        let enc = to_srgb.map(|row| srgb_oetf(dot(&row)));
        if enc.iter().all(|&v| v <= 1.0 / 255.0) {
            dark += 1;
        }
        if enc.iter().any(|&v| v >= 254.0 / 255.0) {
            bright += 1;
        }
        for (count, v) in per.iter_mut().zip(enc) {
            if v >= 254.0 / 255.0 {
                *count += 1;
            }
        }
    }
    l_values.sort_by(f32::total_cmp);
    let pct = |count: usize| 100.0 * count as f32 / n.max(1) as f32;
    let lightness = PERCENTILES.map(|q| {
        if l_values.is_empty() {
            0.0
        } else {
            l_values[((l_values.len() - 1) as f32 * q as f32 / 100.0).round() as usize]
        }
    });
    let mid = n_mid.max(1) as f64;
    // One decimal is all a reader needs.
    let r = |v: f32| (v * 10.0).round() / 10.0;
    Stats {
        width: img.width,
        height: img.height,
        lightness: lightness.map(r),
        mean_lightness: r((l_sum / n.max(1) as f64) as f32),
        clipped_shadows: r(pct(dark)),
        clipped_highlights: r(pct(bright)),
        clipped_rgb: per.map(|c| r(pct(c))),
        cast: [r((a_mid / mid) as f32), r((b_mid / mid) as f32)],
        chroma: r((chroma_sum / n.max(1) as f64) as f32),
    }
}

impl Stats {
    /// A few short lines.
    pub fn to_brief(&self) -> String {
        let l: Vec<String> = PERCENTILES
            .iter()
            .zip(self.lightness)
            .map(|(q, v)| format!("p{q} {v:.0}"))
            .collect();
        let [a, b] = self.cast;
        let mut words = Vec::new();
        if b > 3.0 {
            words.push("warm");
        } else if b < -3.0 {
            words.push("cool");
        }
        if a > 3.0 {
            words.push("magenta");
        } else if a < -3.0 {
            words.push("green");
        }
        let cast = if words.is_empty() {
            "neutral".to_string()
        } else {
            words.join(", ")
        };
        let [r, g, bl] = self.clipped_rgb;
        format!(
            "{}x{}\nL* {}, mean {:.0}\nclipped: shadows {:.1}%, highlights {:.1}% (R {r:.1} G {g:.1} B {bl:.1})\nmidtone cast a* {a:+.1} b* {b:+.1} ({cast}); chroma {:.0}",
            self.width,
            self.height,
            l.join(" "),
            self.mean_lightness,
            self.clipped_shadows,
            self.clipped_highlights,
            self.chroma,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(pixels: &[[f32; 3]]) -> ProPhotoImage {
        ProPhotoImage {
            width: pixels.len(),
            height: 1,
            rgb: pixels.iter().flatten().copied().collect(),
        }
    }

    #[test]
    fn grey_ramp_black_and_white() {
        let mut px: Vec<[f32; 3]> = (0..98).map(|i| [i as f32 / 98.0; 3]).collect();
        px.push([0.0; 3]);
        px.push([2.0, 2.0, 2.0]);
        let s = measure(&image(&px));
        assert_eq!(s.lightness[0], 0.0);
        assert!(s.lightness[6] > 99.0, "{s:?}");
        assert!((s.clipped_shadows - 2.0).abs() < 0.06, "{s:?}");
        assert!((1.0..3.0).contains(&s.clipped_highlights), "{s:?}");
        // Neutral greys: no cast, no chroma.
        assert!(s.cast[0].abs() < 0.5 && s.cast[1].abs() < 0.5, "{s:?}");
        assert!(s.chroma < 0.5, "{s:?}");
        assert!(s.to_brief().contains("(neutral)"), "{}", s.to_brief());
    }

    #[test]
    fn warm_cast_reads_warm() {
        let s = measure(&image(&[[0.3, 0.2, 0.1]; 10]));
        assert!(s.cast[1] > 10.0, "{s:?}");
        assert!(s.to_brief().contains("warm"), "{}", s.to_brief());
        assert_eq!(s.clipped_highlights, 0.0);
    }
}
