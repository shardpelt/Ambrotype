// SPDX-License-Identifier: GPL-3.0-or-later
//! The Calibration panel: `RedHue`, `RedSaturation`, `GreenHue`,
//! `GreenSaturation`, `BlueHue`, `BlueSaturation` and `ShadowTint`
//! (docs/design/calibration.md).
//!
//! **Model v1 (ours, not Adobe's).** Camera Raw describes these as moving
//! the camera's red, green and blue primaries. We move the primaries of the
//! linear ProPhoto working space, right after the camera matrix:
//!
//! - **Hue** turns a primary about the grey axis (R = G = B): positive moves
//!   red toward yellow, green toward cyan and blue toward magenta, the order
//!   of the colour wheel.
//! - **Saturation** moves it away from grey (positive) or toward it.
//! - The new primaries are rescaled so white stays white, so the sliders
//!   change colours, never the white balance.
//! - **Shadow Tint** shifts the shadows toward magenta (positive) or green,
//!   fading out by [`PrimariesStrengths::shadow_end`].
//!
//! The matrix folds into the camera matrix, so it costs nothing per pixel;
//! the shadow tint is one gain per pixel.

/// Strength constants, fitted later against Lightroom sweeps.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct PrimariesStrengths {
    /// Hue ±100: the turn about the grey axis, in degrees.
    pub max_hue_deg: f64,
    /// Saturation ±100: the change in the primary's distance from grey
    /// (+0.6 is 1.6 times as far).
    pub max_saturation: f64,
    /// Shadow Tint ±100: the green channel's change in the deepest shadows,
    /// in stops.
    pub shadow_tint_stops: f64,
    /// Scene-linear luminance where the shadow tint has faded out.
    pub shadow_end: f64,
}

impl Default for PrimariesStrengths {
    fn default() -> Self {
        DEFAULT_STRENGTHS
    }
}

pub const DEFAULT_STRENGTHS: PrimariesStrengths = PrimariesStrengths {
    max_hue_deg: 30.0,
    max_saturation: 0.6,
    shadow_tint_stops: 0.35,
    shadow_end: 0.18,
};

/// The Calibration panel's sliders, −100..100 each.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CameraCalibration {
    pub shadow_tint: f64,
    /// Hue and saturation of the red, green and blue primaries.
    pub hue: [f64; 3],
    pub saturation: [f64; 3],
}

impl CameraCalibration {
    pub fn is_neutral(&self) -> bool {
        *self == CameraCalibration::default()
    }

    /// The linear ProPhoto → linear ProPhoto matrix that moves the
    /// primaries; `None` when the sliders leave them alone.
    pub fn matrix(&self, k: &PrimariesStrengths) -> Option<[[f64; 3]; 3]> {
        if self.hue == [0.0; 3] && self.saturation == [0.0; 3] {
            return None;
        }
        let grey = [1.0 / 3.0; 3];
        let mut cols = [[0.0f64; 3]; 3];
        for (i, col) in cols.iter_mut().enumerate() {
            let theta = (self.hue[i].clamp(-100.0, 100.0) / 100.0 * k.max_hue_deg).to_radians();
            let s =
                (1.0 + self.saturation[i].clamp(-100.0, 100.0) / 100.0 * k.max_saturation).max(0.0);
            let mut d = grey.map(|g| -g);
            d[i] += 1.0;
            let turned = rotate_about_grey(d, theta);
            *col = [0, 1, 2].map(|c| grey[c] + s * turned[c]);
        }
        let a = [0, 1, 2].map(|r| [cols[0][r], cols[1][r], cols[2][r]]);
        // Rescale the primaries so (1, 1, 1) stays (1, 1, 1).
        let k = solve3(&a, [1.0; 3])?;
        Some(a.map(|row| [row[0] * k[0], row[1] * k[1], row[2] * k[2]]))
    }

    /// The per-pixel shadow tint, if any: (strength in stops, fade end).
    pub fn shadow_tint(&self, k: &PrimariesStrengths) -> Option<(f32, f32)> {
        (self.shadow_tint != 0.0).then(|| {
            (
                (self.shadow_tint.clamp(-100.0, 100.0) / 100.0 * k.shadow_tint_stops) as f32,
                k.shadow_end.max(1e-4) as f32,
            )
        })
    }
}

/// Turn `v` by `theta` about the grey axis (1, 1, 1): red → green → blue for
/// a positive angle (Rodrigues' formula).
fn rotate_about_grey(v: [f64; 3], theta: f64) -> [f64; 3] {
    let n = 1.0 / 3f64.sqrt();
    let (s, c) = theta.sin_cos();
    let dot = n * (v[0] + v[1] + v[2]);
    let cross = [n * (v[2] - v[1]), n * (v[0] - v[2]), n * (v[1] - v[0])];
    [0, 1, 2].map(|i| v[i] * c + cross[i] * s + n * dot * (1.0 - c))
}

/// x with a·x = b, by Cramer's rule.
fn solve3(a: &[[f64; 3]; 3], b: [f64; 3]) -> Option<[f64; 3]> {
    let det = |m: &[[f64; 3]; 3]| {
        m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
            - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
            + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
    };
    let d = det(a);
    if !d.is_finite() || d.abs() < 1e-12 {
        return None;
    }
    let mut x = [0.0; 3];
    for (i, xi) in x.iter_mut().enumerate() {
        let mut m = *a;
        for r in 0..3 {
            m[r][i] = b[r];
        }
        *xi = det(&m) / d;
    }
    Some(x)
}

/// Apply the shadow tint to linear ProPhoto `rgb` (one pixel): the green
/// channel goes down (magenta) or up (green), red and blue the other way
/// by half as much, so brightness stays about the same.
#[inline]
pub fn tint_shadows(rgb: &mut [f32], stops: f32, end: f32) {
    let y = (0.288 * rgb[0] + 0.712 * rgb[1]).max(0.0);
    let t = (y / end).min(1.0);
    let w = 1.0 - t * t * (3.0 - 2.0 * t);
    if w <= 0.0 {
        return;
    }
    let g = (-stops * w).exp2();
    let rb = 1.0 / g.sqrt();
    rgb[0] *= rb;
    rgb[1] *= g;
    rgb[2] *= rb;
}

#[cfg(test)]
mod tests {
    use super::*;

    const K: PrimariesStrengths = DEFAULT_STRENGTHS;

    fn apply(m: &[[f64; 3]; 3], v: [f64; 3]) -> [f64; 3] {
        m.map(|r| r[0] * v[0] + r[1] * v[1] + r[2] * v[2])
    }

    #[test]
    fn neutral_sliders_do_nothing() {
        assert!(CameraCalibration::default().matrix(&K).is_none());
        assert!(CameraCalibration::default().shadow_tint(&K).is_none());
    }

    #[test]
    fn a_third_of_a_turn_takes_red_to_green() {
        let g = rotate_about_grey([2.0 / 3.0, -1.0 / 3.0, -1.0 / 3.0], 120f64.to_radians());
        assert!(
            (g[1] - 2.0 / 3.0).abs() < 1e-12 && (g[0] + 1.0 / 3.0).abs() < 1e-12,
            "{g:?}"
        );
    }

    #[test]
    fn white_stays_white_and_primaries_move() {
        let c = CameraCalibration {
            hue: [50.0, -30.0, 80.0],
            saturation: [40.0, -60.0, 20.0],
            ..Default::default()
        };
        let m = c.matrix(&K).unwrap();
        for grey in [1.0, 0.18] {
            let w = apply(&m, [grey; 3]);
            assert!(w.iter().all(|v| (v - grey).abs() < 1e-9), "{w:?}");
        }
        // Red hue +: red leans toward yellow (more green than blue).
        let red = CameraCalibration {
            hue: [60.0, 0.0, 0.0],
            ..Default::default()
        };
        let r = apply(&red.matrix(&K).unwrap(), [1.0, 0.0, 0.0]);
        assert!(r[1] > r[2], "{r:?}");
        // Green saturation −100: green moves toward grey.
        let dull = CameraCalibration {
            saturation: [0.0, -100.0, 0.0],
            ..Default::default()
        };
        let g = apply(&dull.matrix(&K).unwrap(), [0.2, 0.6, 0.2]);
        let spread = |v: [f64; 3]| {
            v.iter().cloned().fold(f64::MIN, f64::max) - v.iter().cloned().fold(f64::MAX, f64::min)
        };
        assert!(spread(g) < 0.4 * 0.8, "{g:?}");
    }

    #[test]
    fn shadow_tint_touches_only_the_shadows() {
        let c = CameraCalibration {
            shadow_tint: 100.0,
            ..Default::default()
        };
        let (stops, end) = c.shadow_tint(&K).unwrap();
        let mut dark = [0.01f32; 3];
        tint_shadows(&mut dark, stops, end);
        assert!(dark[1] < dark[0] && dark[1] < dark[2], "magenta: {dark:?}");
        let mut bright = [0.5f32; 3];
        tint_shadows(&mut bright, stops, end);
        assert_eq!(bright, [0.5; 3]);
    }
}
