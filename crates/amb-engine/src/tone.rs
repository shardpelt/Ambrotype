// SPDX-License-Identifier: GPL-3.0-or-later
//! Tone controls: Contrast, Highlights, Shadows, Whites, Blacks, and the
//! parametric tone curve.
//!
//! **Model v2 (ours, not Adobe's).** Every control is a smooth, bounded
//! deformation of a luminance tone curve on perceptually encoded values (sRGB
//! transfer, 0..1). Contrast, Whites and Blacks act globally ([`tone_lut`]).
//! Highlights and Shadows are local, like Lightroom's PV2012 versions
//! (docs/research/05 §4): their curve ([`region_lut`]) is evaluated on an
//! edge-aware smooth base of the photo ([`crate::local_tone`]). All strength
//! constants live in [`ToneStrengths`] so the calibration harness can fit them
//! against Lightroom sweeps instead of us guessing.

/// Strength constants, at slider = ±100. Fitted later; these are starting guesses.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ToneStrengths {
    /// Contrast S-curve: mid-tone slope change at ±100 (slope 1 ± this).
    pub contrast: f64,
    /// Peak lift/drop of the shadows (centred at 0.25) and highlights (0.75) bumps.
    pub shadows: f64,
    pub highlights: f64,
    /// Endpoint push for Whites/Blacks: added at the extreme, fading towards mid-grey.
    pub whites: f64,
    pub blacks: f64,
    /// Peak height of a parametric-curve region bump.
    pub parametric: f64,
    /// Highlights/Shadows: radius of the edge-aware base, as a fraction of the
    /// image diagonal (larger = moves bigger areas as a whole).
    pub local_radius_frac: f64,
    /// Highlights/Shadows: how strong an edge must be (encoded luminance,
    /// squared) for the base to stop at it. Smaller = sharper, fewer halos.
    pub local_edge_eps: f64,
}

impl Default for ToneStrengths {
    fn default() -> Self {
        DEFAULT_STRENGTHS
    }
}

pub const DEFAULT_STRENGTHS: ToneStrengths = ToneStrengths {
    contrast: 0.6,
    shadows: 0.25,
    highlights: 0.25,
    whites: 0.2,
    blacks: 0.15,
    parametric: 0.12,
    local_radius_frac: 0.012,
    local_edge_eps: 0.01,
};

/// Basic-panel tone sliders, each in -100..100.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ToneControls {
    pub contrast: f64,
    pub highlights: f64,
    pub shadows: f64,
    pub whites: f64,
    pub blacks: f64,
}

impl ToneControls {
    pub fn is_neutral(&self) -> bool {
        *self == ToneControls::default()
    }
}

/// Parametric curve: four region sliders (-100..100) and three split points (0..100).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParametricCurve {
    pub shadows: f64,
    pub darks: f64,
    pub lights: f64,
    pub highlights: f64,
    pub shadow_split: f64,
    pub midtone_split: f64,
    pub highlight_split: f64,
}

impl Default for ParametricCurve {
    fn default() -> Self {
        ParametricCurve {
            shadows: 0.0,
            darks: 0.0,
            lights: 0.0,
            highlights: 0.0,
            shadow_split: 25.0,
            midtone_split: 50.0,
            highlight_split: 75.0,
        }
    }
}

impl ParametricCurve {
    pub fn is_neutral(&self) -> bool {
        self.shadows == 0.0 && self.darks == 0.0 && self.lights == 0.0 && self.highlights == 0.0
    }
}

fn gaussian(x: f64, centre: f64, width: f64) -> f64 {
    let d = (x - centre) / width;
    (-d * d).exp()
}

fn smoothstep(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Smooth bump that is 1 in the middle of [lo, hi] and fades to 0 just outside it.
fn region_bump(x: f64, lo: f64, hi: f64) -> f64 {
    let centre = 0.5 * (lo + hi);
    let width = (0.5 * (hi - lo)).max(0.02);
    gaussian(x, centre, width * 1.2)
}

/// Make a table non-decreasing and clamp to 0..1 (sliders must never invert tones).
fn monotone(lut: &mut [f32]) {
    let mut last = 0.0f32;
    for v in lut.iter_mut() {
        *v = v.clamp(0.0, 1.0).max(last);
        last = *v;
    }
}

/// Encoded-luminance → encoded-luminance table for the global tone sliders:
/// Contrast, Whites and Blacks (Highlights and Shadows are [`region_lut`]).
pub fn tone_lut(c: &ToneControls, k: &ToneStrengths, size: usize) -> Vec<f32> {
    let mut lut: Vec<f32> = (0..size)
        .map(|i| {
            let x = i as f64 / (size - 1) as f64;
            // Contrast: S-curve pivoting at 0.5. Slope is 1 + a at mid-grey and 1 - a at
            // the ends, so it stays monotone for |a| <= 1.
            let a = k.contrast * c.contrast / 100.0;
            let tau = 2.0 * std::f64::consts::PI;
            let mut y = x + a * (tau * (x - 0.5)).sin() / tau;
            y += k.blacks * c.blacks / 100.0 * (1.0 - y).powi(4);
            y += k.whites * c.whites / 100.0 * y.powi(4);
            y as f32
        })
        .collect();
    monotone(&mut lut);
    lut
}

/// Encoded → encoded table for Highlights and Shadows: smooth bumps centred
/// in the darks and the brights. Applied to the local base of a photo, not
/// to single pixels ([`crate::local_tone`]).
pub fn region_lut(c: &ToneControls, k: &ToneStrengths, size: usize) -> Vec<f32> {
    let mut lut: Vec<f32> = (0..size)
        .map(|i| {
            let x = i as f64 / (size - 1) as f64;
            // Pure black and white stay put (Blacks and Whites move those).
            let ends = smoothstep(x / 0.04) * smoothstep((1.0 - x) / 0.04);
            let y = x + ends
                * (k.shadows * c.shadows / 100.0 * gaussian(x, 0.25, 0.17) * (1.0 - x)
                    + k.highlights * c.highlights / 100.0 * gaussian(x, 0.75, 0.17) * x);
            y as f32
        })
        .collect();
    monotone(&mut lut);
    lut
}

/// Encoded → encoded table for the parametric curve (applied per channel, before the point curve).
pub fn parametric_lut(p: &ParametricCurve, k: &ToneStrengths, size: usize) -> Vec<f32> {
    let s1 = (p.shadow_split / 100.0).clamp(0.0, 1.0);
    let s2 = (p.midtone_split / 100.0).clamp(s1, 1.0);
    let s3 = (p.highlight_split / 100.0).clamp(s2, 1.0);
    let mut lut: Vec<f32> = (0..size)
        .map(|i| {
            let x = i as f64 / (size - 1) as f64;
            // Endpoint taper keeps black and white fixed (x·(1−x) factor, normalised).
            let taper = (4.0 * x * (1.0 - x)).min(1.0).sqrt();
            let delta = p.shadows * region_bump(x, 0.0, s1)
                + p.darks * region_bump(x, s1, s2)
                + p.lights * region_bump(x, s2, s3)
                + p.highlights * region_bump(x, s3, 1.0);
            (x + k.parametric * delta / 100.0 * taper) as f32
        })
        .collect();
    monotone(&mut lut);
    lut
}

#[cfg(test)]
mod tests {
    use super::*;

    const N: usize = 1024;

    fn at(lut: &[f32], x: f64) -> f64 {
        f64::from(lut[(x * (N - 1) as f64).round() as usize])
    }

    #[test]
    fn neutral_is_identity() {
        let lut = tone_lut(&ToneControls::default(), &DEFAULT_STRENGTHS, N);
        for (i, v) in lut.iter().enumerate() {
            assert!((f64::from(*v) - i as f64 / (N - 1) as f64).abs() < 1e-6);
        }
        let p = parametric_lut(&ParametricCurve::default(), &DEFAULT_STRENGTHS, N);
        assert!((at(&p, 0.3) - 0.3).abs() < 1e-3);
    }

    #[test]
    fn extremes_stay_monotone_and_bounded() {
        for v in [-100.0, 100.0] {
            let c = ToneControls {
                contrast: v,
                highlights: v,
                shadows: v,
                whites: v,
                blacks: v,
            };
            for lut in [
                tone_lut(&c, &DEFAULT_STRENGTHS, N),
                region_lut(&c, &DEFAULT_STRENGTHS, N),
            ] {
                assert!(lut.windows(2).all(|w| w[1] >= w[0]));
                assert!(lut.iter().all(|x| (0.0..=1.0).contains(x)));
            }
        }
    }

    #[test]
    fn controls_move_the_right_tones() {
        let k = &DEFAULT_STRENGTHS;
        let contrast = tone_lut(
            &ToneControls {
                contrast: 100.0,
                ..Default::default()
            },
            k,
            N,
        );
        assert!(
            at(&contrast, 0.25) < 0.25 && at(&contrast, 0.75) > 0.75,
            "contrast is an S-curve"
        );
        assert!((at(&contrast, 0.5) - 0.5).abs() < 1e-3, "pivot at mid-grey");

        let shadows = region_lut(
            &ToneControls {
                shadows: 100.0,
                ..Default::default()
            },
            k,
            N,
        );
        assert!(
            at(&shadows, 0.25) - 0.25 > 2.0 * (at(&shadows, 0.75) - 0.75).abs(),
            "shadows lifts darks most"
        );

        let highlights = region_lut(
            &ToneControls {
                highlights: -100.0,
                ..Default::default()
            },
            k,
            N,
        );
        assert!(
            0.75 - at(&highlights, 0.75) > 2.0 * (0.25 - at(&highlights, 0.25)).abs(),
            "highlights recovers brights most"
        );

        let whites = tone_lut(
            &ToneControls {
                whites: 100.0,
                ..Default::default()
            },
            k,
            N,
        );
        assert!(at(&whites, 0.95) > 0.95 && (at(&whites, 0.2) - 0.2).abs() < 0.01);
        let blacks = tone_lut(
            &ToneControls {
                blacks: -100.0,
                ..Default::default()
            },
            k,
            N,
        );
        assert!(at(&blacks, 0.05) < 0.05 && (at(&blacks, 0.8) - 0.8).abs() < 0.01);
    }

    #[test]
    fn parametric_regions_follow_splits() {
        let k = &DEFAULT_STRENGTHS;
        let lights = parametric_lut(
            &ParametricCurve {
                lights: 100.0,
                ..Default::default()
            },
            k,
            N,
        );
        assert!(
            at(&lights, 0.625) - 0.625 > 0.05,
            "lights region centre lifted"
        );
        assert!((at(&lights, 0.1) - 0.1).abs() < 0.01, "shadows untouched");
        assert!(
            at(&lights, 0.0) < 1e-3 && at(&lights, 1.0) > 0.999,
            "endpoints fixed"
        );
    }
}

/// How the old process versions' Basic sliders (PV2010/PV2003: `Exposure`,
/// `Brightness`, `Contrast`, `Shadows`, `HighlightRecovery`, `FillLight`,
/// `Clarity`) are read as today's (PV2012) ones. **Ours, a first guess**:
/// the scale of each slider against its 2012 counterpart, to be fitted from
/// Lightroom renders of old-process photos.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct LegacyToneStrengths {
    /// EV per Brightness step from its default of 50.
    pub brightness_ev: f64,
    /// `Contrast2012` per Contrast step from its default of 25.
    pub contrast: f64,
    /// `Blacks2012` per Shadows step from its default of 5 (negative: darker).
    pub blacks: f64,
    /// `Highlights2012` per HighlightRecovery step (negative: darker).
    pub recovery: f64,
    /// `Shadows2012` per Fill Light step.
    pub fill: f64,
    /// `Clarity2012` per old Clarity step.
    pub clarity: f64,
}

impl Default for LegacyToneStrengths {
    fn default() -> Self {
        DEFAULT_LEGACY
    }
}

pub const DEFAULT_LEGACY: LegacyToneStrengths = LegacyToneStrengths {
    brightness_ev: 0.01,
    contrast: 0.8,
    blacks: -1.5,
    recovery: -0.8,
    fill: 0.8,
    clarity: 0.7,
};
