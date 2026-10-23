// SPDX-License-Identifier: GPL-3.0-or-later
//! Showing sRGB pixels on a monitor that has its own ICC profile.
//!
//! The editor's preview is sRGB. A wide-gamut or calibrated monitor shows
//! those numbers in its own colours unless they are converted first: sRGB
//! decoded to linear, through XYZ (D50) into the monitor's primaries, then
//! encoded with the monitor's tone curves. Colours the monitor can't show
//! are clipped. Only matrix/TRC profiles are read (`icc::parse`); for any
//! other the caller shows plain sRGB.

use crate::icc::{Curve, RgbProfile};
use crate::matrix::{Mat3, SRGB_TO_XYZ_D50, invert3, mul3};

/// Steps in the table from linear light back to the monitor's encoding.
const STEPS: usize = 4096;

/// sRGB 8-bit → the monitor's 8-bit RGB, ready to apply to whole images.
#[derive(Debug, Clone)]
pub struct ToDisplay {
    /// sRGB code value → linear light.
    decode: [f32; 256],
    /// Linear sRGB → the monitor's linear RGB.
    matrix: [[f32; 3]; 3],
    /// Per channel: linear light (0..1 in `STEPS`) → monitor code value.
    encode: [Vec<u8>; 3],
}

impl ToDisplay {
    pub fn new(monitor: &RgbProfile) -> ToDisplay {
        let srgb = crate::icc::srgb();
        let to_monitor: Mat3 = mul3(&invert3(&monitor.to_xyz_d50), &SRGB_TO_XYZ_D50);
        ToDisplay {
            decode: std::array::from_fn(|i| srgb.curves[0].to_linear(i as f64 / 255.0) as f32),
            matrix: to_monitor.map(|row| row.map(|v| v as f32)),
            encode: std::array::from_fn(|c| encode_table(&monitor.curves[c])),
        }
    }

    /// Convert packed RGB in place.
    pub fn apply(&self, rgb: &mut [u8]) {
        let m = &self.matrix;
        let top = (STEPS - 1) as f32;
        for p in rgb.chunks_exact_mut(3) {
            let v = [p[0], p[1], p[2]].map(|c| self.decode[c as usize]);
            for (c, out) in p.iter_mut().enumerate() {
                let lin = m[c][0] * v[0] + m[c][1] * v[1] + m[c][2] * v[2];
                *out = self.encode[c][(lin.clamp(0.0, 1.0) * top + 0.5) as usize];
            }
        }
    }
}

/// The curve's inverse, sampled: each linear step's code value. The curve
/// is found by bisection, so tables and parametric curves work alike.
fn encode_table(curve: &Curve) -> Vec<u8> {
    (0..STEPS)
        .map(|i| {
            let want = i as f64 / (STEPS - 1) as f64;
            let (mut lo, mut hi) = (0.0f64, 1.0f64);
            for _ in 0..24 {
                let mid = (lo + hi) / 2.0;
                if curve.to_linear(mid) < want {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            ((lo + hi) / 2.0 * 255.0).round() as u8
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::space::OutputSpace;

    fn through(space: OutputSpace, rgb: [u8; 3]) -> [u8; 3] {
        let t = ToDisplay::new(&crate::icc::parse(&space.icc_profile()).unwrap());
        let mut p = rgb;
        t.apply(&mut p);
        p
    }

    #[test]
    fn an_srgb_monitor_changes_nothing() {
        for rgb in [
            [0, 0, 0],
            [255, 255, 255],
            [12, 128, 240],
            [200, 30, 90],
            [3, 3, 3],
        ] {
            let out = through(OutputSpace::Srgb, rgb);
            for c in 0..3 {
                assert!(out[c].abs_diff(rgb[c]) <= 1, "{rgb:?} → {out:?}");
            }
        }
    }

    #[test]
    fn a_wide_gamut_monitor_gets_less_saturated_numbers() {
        // Pure sRGB red lies inside Display P3: it needs less than full P3 red.
        let red = through(OutputSpace::DisplayP3, [255, 0, 0]);
        assert!(red[0] < 245 && red[1] > 40 && red[2] > 20, "{red:?}");
        // Greys stay grey (both are D65 white, adapted alike).
        let grey = through(OutputSpace::DisplayP3, [128, 128, 128]);
        assert!(grey.iter().all(|c| c.abs_diff(128) <= 1), "{grey:?}");
        // Adobe RGB's gamma 2.2 curve differs from sRGB's in the shadows.
        let dark = through(OutputSpace::AdobeRgbCompatible, [10, 10, 10]);
        assert!(dark[0] > 10, "{dark:?}");
    }
}
