//! Colour controls: Vibrance, Saturation, the 8-band HSL / Color Mixer, and
//! Color Grading (three wheels + global, Blending, Balance; legacy Split Toning
//! maps onto it).
//!
//! **Model v1 (ours, not Adobe's).** Operations run in OkLab / OkLCh (Björn
//! Ottosson's perceptual space, public domain), computed from the display-linear
//! working values. HSL band centres and colour-wheel hue directions are derived
//! from the HSV hues Lightroom's UI uses (red 0°, orange 30°, yellow 60°, ...),
//! mapped into OkLab. Strengths live in [`ColorStrengths`] for later fitting
//! against Lightroom sweeps (docs/lightroom-trial-plan.md).

use std::f32::consts::TAU;

use amb_color::matrix::{Mat3, PROPHOTO_TO_XYZ_D50, invert3, mul3, xyz_d50_to_srgb};

/// Strength constants at slider = ±100. Starting guesses, to be fitted.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ColorStrengths {
    /// HSL hue shift at ±100, in OkLCh degrees.
    pub hsl_hue_deg: f32,
    /// HSL luminance change at ±100 (relative L change on saturated colours).
    pub hsl_lum: f32,
    /// Vibrance boost at +100 for the least saturated colours.
    pub vibrance: f32,
    /// Color-grading tint at saturation 100, in OkLab a/b units.
    pub grade_tint: f32,
    /// Color-grading luminance shift at ±100, in OkLab L units.
    pub grade_lum: f32,
}

impl Default for ColorStrengths {
    fn default() -> Self {
        DEFAULT_STRENGTHS
    }
}

pub const DEFAULT_STRENGTHS: ColorStrengths = ColorStrengths {
    hsl_hue_deg: 25.0,
    hsl_lum: 0.3,
    vibrance: 0.8,
    grade_tint: 0.06,
    grade_lum: 0.1,
};

/// HSL band order used by Camera Raw tags.
pub const BANDS: [&str; 8] = [
    "Red", "Orange", "Yellow", "Green", "Aqua", "Blue", "Purple", "Magenta",
];
const BAND_HSV_HUES: [f32; 8] = [0.0, 30.0, 60.0, 120.0, 180.0, 240.0, 270.0, 300.0];

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Wheel {
    pub hue: f64,
    pub sat: f64,
    pub lum: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Grading {
    pub shadows: Wheel,
    pub midtones: Wheel,
    pub highlights: Wheel,
    pub global: Wheel,
    /// 0..100, default 50: overlap between the tonal regions.
    pub blending: f64,
    /// -100..100, default 0: positive favours highlights.
    pub balance: f64,
}

impl Default for Grading {
    fn default() -> Self {
        Grading {
            shadows: Wheel::default(),
            midtones: Wheel::default(),
            highlights: Wheel::default(),
            global: Wheel::default(),
            blending: 50.0,
            balance: 0.0,
        }
    }
}

impl Grading {
    fn is_neutral(&self) -> bool {
        [self.shadows, self.midtones, self.highlights, self.global]
            .iter()
            .all(|w| w.sat == 0.0 && w.lum == 0.0)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ColorControls {
    pub vibrance: f64,
    pub saturation: f64,
    /// Per band: (hue, saturation, luminance), each -100..100.
    pub hsl: [[f64; 3]; 8],
    pub grading: Grading,
    /// Black & white: the photo becomes grey, each band's grey tone set by
    /// `gray_mix` (-100..100, darker..lighter), before colour grading (so
    /// grading can tone it, e.g. sepia). The HSL sliders don't apply then.
    pub grayscale: bool,
    pub gray_mix: [f64; 8],
}

impl ColorControls {
    pub fn is_neutral(&self) -> bool {
        !self.grayscale
            && self.vibrance == 0.0
            && self.saturation == 0.0
            && self.hsl.iter().all(|b| b.iter().all(|v| *v == 0.0))
            && self.grading.is_neutral()
    }
}

// --- OkLab -----------------------------------------------------------------

const M1: Mat3 = [
    [0.412_221_470_8, 0.536_332_536_3, 0.051_445_992_9],
    [0.211_903_498_2, 0.680_699_545_1, 0.107_396_956_6],
    [0.088_302_461_9, 0.281_718_837_6, 0.629_978_700_5],
];
const M2: Mat3 = [
    [0.210_454_255_3, 0.793_617_785_0, -0.004_072_046_8],
    [1.977_998_495_1, -2.428_592_205_0, 0.450_593_709_9],
    [0.025_904_037_1, 0.782_771_766_2, -0.808_675_766_0],
];
const M2_INV: Mat3 = [
    [1.0, 0.396_337_777_4, 0.215_803_757_3],
    [1.0, -0.105_561_345_8, -0.063_854_172_8],
    [1.0, -0.089_484_177_5, -1.291_485_548_0],
];

fn mat_f32(m: &Mat3) -> [[f32; 3]; 3] {
    m.map(|row| row.map(|v| v as f32))
}

fn apply(m: &[[f32; 3]; 3], v: [f32; 3]) -> [f32; 3] {
    [0, 1, 2].map(|i| m[i][0] * v[0] + m[i][1] * v[1] + m[i][2] * v[2])
}

/// Precomputed conversions between the working space (linear ProPhoto) and OkLab.
#[derive(Debug, Clone)]
pub struct OkLab {
    to_lms: [[f32; 3]; 3],
    from_lms: [[f32; 3]; 3],
    m2: [[f32; 3]; 3],
    m2_inv: [[f32; 3]; 3],
}

impl OkLab {
    pub fn new() -> Self {
        let prophoto_to_srgb = mul3(&xyz_d50_to_srgb(), &PROPHOTO_TO_XYZ_D50);
        let to_lms = mul3(&M1, &prophoto_to_srgb);
        OkLab {
            to_lms: mat_f32(&to_lms),
            from_lms: mat_f32(&invert3(&to_lms)),
            m2: mat_f32(&M2),
            m2_inv: mat_f32(&M2_INV),
        }
    }

    pub fn from_working(&self, rgb: [f32; 3]) -> [f32; 3] {
        let lms = apply(&self.to_lms, rgb).map(f32::cbrt);
        apply(&self.m2, lms)
    }

    pub fn to_working(&self, lab: [f32; 3]) -> [f32; 3] {
        let lms = apply(&self.m2_inv, lab).map(|v| v * v * v);
        apply(&self.from_lms, lms)
    }

    /// OkLab hue angle (radians) of the sRGB colour with HSV hue `deg` at full saturation.
    fn hue_of_hsv(deg: f32) -> f32 {
        let h = deg.rem_euclid(360.0) / 60.0;
        let x = 1.0 - ((h % 2.0) - 1.0).abs();
        let srgb = match h as u32 {
            0 => [1.0, x, 0.0],
            1 => [x, 1.0, 0.0],
            2 => [0.0, 1.0, x],
            3 => [0.0, x, 1.0],
            4 => [x, 0.0, 1.0],
            _ => [1.0, 0.0, x],
        };
        let lms = apply(&mat_f32(&M1), srgb).map(f32::cbrt);
        let lab = apply(&mat_f32(&M2), lms);
        lab[2].atan2(lab[1])
    }
}

impl Default for OkLab {
    fn default() -> Self {
        Self::new()
    }
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Prepared per-image colour operation.
pub struct ColorOp {
    lab: OkLab,
    k: ColorStrengths,
    vibrance: f32,
    saturation: f32,
    /// Band centres (OkLCh hue, radians, ascending) with their (hue, sat,
    /// lum, black & white mix) amounts.
    bands: Vec<(f32, [f32; 4])>,
    grayscale: bool,
    orange_index: usize,
    grade_dirs: [[f32; 2]; 4],
    grade_amounts: [[f32; 2]; 4],
    pivot: f32,
    width: f32,
}

fn wheel_dir(hue_deg: f64) -> [f32; 2] {
    let h = OkLab::hue_of_hsv(hue_deg as f32);
    [h.cos(), h.sin()]
}

impl ColorOp {
    pub fn new(c: &ColorControls, k: ColorStrengths) -> Self {
        let mut bands: Vec<(f32, [f32; 4])> = BAND_HSV_HUES
            .iter()
            .zip(c.hsl.iter().zip(c.gray_mix))
            .map(|(deg, ([h, s, l], mix))| {
                let hue = OkLab::hue_of_hsv(*deg).rem_euclid(TAU);
                (hue, [*h, *s, *l, mix].map(|v| v as f32 / 100.0))
            })
            .collect();
        let orange_hue = bands[1].0;
        bands.sort_by(|a, b| a.0.total_cmp(&b.0));
        let orange_index = bands.iter().position(|b| b.0 == orange_hue).unwrap_or(0);
        let g = &c.grading;
        let wheels = [g.shadows, g.midtones, g.highlights, g.global];
        ColorOp {
            lab: OkLab::new(),
            k,
            vibrance: c.vibrance as f32 / 100.0,
            saturation: c.saturation as f32 / 100.0,
            bands,
            grayscale: c.grayscale,
            orange_index,
            grade_dirs: wheels.map(|w| wheel_dir(w.hue)),
            grade_amounts: wheels.map(|w| [w.sat as f32 / 100.0, w.lum as f32 / 100.0]),
            pivot: 0.5 - 0.25 * g.balance as f32 / 100.0,
            width: 0.08 + 0.4 * g.blending as f32 / 100.0,
        }
    }

    /// Interpolation weights of the two bands around `hue` (radians): (index, weight) pairs.
    fn band_weights(&self, hue: f32) -> [(usize, f32); 2] {
        let h = hue.rem_euclid(TAU);
        let n = self.bands.len();
        for i in 0..n {
            let lo = self.bands[i].0;
            let hi = if i + 1 < n {
                self.bands[i + 1].0
            } else {
                self.bands[0].0 + TAU
            };
            let hh = if h < lo { h + TAU } else { h };
            if hh >= lo && hh < hi {
                let t = smoothstep(0.0, 1.0, (hh - lo) / (hi - lo));
                return [(i, 1.0 - t), ((i + 1) % n, t)];
            }
        }
        [(0, 1.0), (0, 0.0)]
    }

    /// Apply to one display-linear working-space pixel.
    pub fn apply(&self, rgb: [f32; 3]) -> [f32; 3] {
        let [mut l, a, b] = self.lab.from_working(rgb);
        let mut chroma = a.hypot(b);
        let mut hue = b.atan2(a);

        let weights = self.band_weights(hue);
        if self.grayscale {
            // Each colour's grey tone from the mix; neutrals stay as they are.
            let mix: f32 = weights.iter().map(|(i, w)| w * self.bands[*i].1[3]).sum();
            let colourfulness = smoothstep(0.0, 0.08, chroma);
            l *= (1.0 + mix * self.k.hsl_lum * 1.5 * colourfulness).max(0.0);
            return self.grade(l, 0.0, 0.0);
        }
        let mut dh = 0.0;
        let mut sat_gain = 1.0;
        let mut lum_gain = 0.0;
        for (i, w) in weights {
            let [bh, bs, bl, _] = self.bands[i].1;
            dh += w * bh;
            sat_gain += w * bs;
            lum_gain += w * bl;
        }
        // Neutrals have no hue, so the HSL luminance and hue sliders fade out near grey.
        let colourfulness = smoothstep(0.0, 0.08, chroma);
        hue += dh * self.k.hsl_hue_deg.to_radians() * colourfulness;
        chroma *= sat_gain.max(0.0);
        l *= 1.0 + lum_gain * self.k.hsl_lum * colourfulness;

        if self.vibrance != 0.0 {
            let skin = weights
                .iter()
                .filter(|(i, _)| *i == self.orange_index)
                .map(|(_, w)| *w)
                .sum::<f32>();
            let low_sat = 1.0 - smoothstep(0.0, 0.25, chroma);
            chroma *=
                (1.0 + self.vibrance * self.k.vibrance * low_sat * (1.0 - 0.5 * skin)).max(0.0);
        }
        chroma *= (1.0 + self.saturation).max(0.0);

        self.grade(l, chroma * hue.cos(), chroma * hue.sin())
    }

    /// Color grading, then back to the working space.
    fn grade(&self, mut l: f32, mut a: f32, mut b: f32) -> [f32; 3] {
        // Color grading: tonal regions from OkLab lightness.
        let t = smoothstep(self.pivot - self.width, self.pivot + self.width, l);
        let region = [(1.0 - t) * (1.0 - t), 2.0 * t * (1.0 - t), t * t, 1.0];
        for ((w, [sat, lum]), dir) in region.iter().zip(self.grade_amounts).zip(self.grade_dirs) {
            if sat == 0.0 && lum == 0.0 {
                continue;
            }
            a += w * sat * self.k.grade_tint * dir[0];
            b += w * sat * self.k.grade_tint * dir[1];
            l += w * lum * self.k.grade_lum;
        }
        self.lab.to_working([l.max(0.0), a, b])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chroma(op: &ColorOp, rgb: [f32; 3]) -> f32 {
        let [_, a, b] = op.lab.from_working(rgb);
        a.hypot(b)
    }

    fn hue_deg(op: &ColorOp, rgb: [f32; 3]) -> f32 {
        let [_, a, b] = op.lab.from_working(rgb);
        b.atan2(a).to_degrees()
    }

    const RED: [f32; 3] = [0.6, 0.12, 0.1];
    const GREY: [f32; 3] = [0.2, 0.2, 0.2];

    #[test]
    fn oklab_round_trip_and_neutral_axis() {
        let lab = OkLab::new();
        for rgb in [RED, GREY, [0.05, 0.4, 0.9]] {
            let back = lab.to_working(lab.from_working(rgb));
            for c in 0..3 {
                assert!((back[c] - rgb[c]).abs() < 1e-4, "{rgb:?} → {back:?}");
            }
        }
        let [_, a, b] = lab.from_working(GREY);
        assert!(
            a.abs() < 1e-3 && b.abs() < 1e-3,
            "D50 grey is neutral in OkLab: {a} {b}"
        );
    }

    #[test]
    fn neutral_controls_are_identity() {
        let op = ColorOp::new(&ColorControls::default(), DEFAULT_STRENGTHS);
        let out = op.apply(RED);
        for c in 0..3 {
            assert!((out[c] - RED[c]).abs() < 1e-4);
        }
    }

    #[test]
    fn saturation_minus_100_is_greyscale() {
        let op = ColorOp::new(
            &ColorControls {
                saturation: -100.0,
                ..Default::default()
            },
            DEFAULT_STRENGTHS,
        );
        let out = op.apply(RED);
        assert!(chroma(&op, out) < 1e-3);
    }

    #[test]
    fn vibrance_favours_muted_colours() {
        let k = DEFAULT_STRENGTHS;
        let op = ColorOp::new(
            &ColorControls {
                vibrance: 100.0,
                ..Default::default()
            },
            k,
        );
        let muted = [0.22, 0.2, 0.25];
        let vivid = [0.05, 0.2, 0.8];
        let gain = |c: [f32; 3]| chroma(&op, op.apply(c)) / chroma(&op, c);
        assert!(
            gain(muted) > gain(vivid) + 0.2,
            "muted {} vivid {}",
            gain(muted),
            gain(vivid)
        );
    }

    #[test]
    fn red_hue_slider_moves_red_towards_orange() {
        let mut c = ColorControls::default();
        c.hsl[0][0] = 100.0; // Red hue +100
        let op = ColorOp::new(&c, DEFAULT_STRENGTHS);
        let before = hue_deg(&op, RED);
        let after = hue_deg(&op, op.apply(RED));
        assert!(after > before + 10.0, "{before} → {after}");
        // Blue is far from the red band and must not move.
        let blue = [0.05, 0.1, 0.7];
        assert!((hue_deg(&op, op.apply(blue)) - hue_deg(&op, blue)).abs() < 0.5);
    }

    #[test]
    fn shadow_grading_tints_darks_more_than_brights() {
        let mut c = ColorControls::default();
        c.grading.shadows = Wheel {
            hue: 220.0,
            sat: 100.0,
            lum: 0.0,
        };
        let op = ColorOp::new(&c, DEFAULT_STRENGTHS);
        let dark = [0.01, 0.01, 0.01];
        let bright = [0.8, 0.8, 0.8];
        assert!(chroma(&op, op.apply(dark)) > 3.0 * chroma(&op, op.apply(bright)));
        // The tint points towards blue-ish (negative b in OkLab for hue 220).
        let [_, _, b] = op.lab.from_working(op.apply(dark));
        assert!(b < 0.0);
    }

    #[test]
    fn black_and_white_mix_and_toning() {
        let mono = |red_mix: f64| {
            let mut c = ColorControls {
                grayscale: true,
                ..Default::default()
            };
            c.gray_mix[0] = red_mix;
            ColorOp::new(&c, DEFAULT_STRENGTHS)
        };
        let plain = mono(0.0);
        assert!(chroma(&plain, plain.apply(RED)) < 1e-4, "grey");
        assert!(
            !ColorControls {
                grayscale: true,
                ..Default::default()
            }
            .is_neutral()
        );
        // Red mix lightens or darkens reds' grey; a neutral stays as it was.
        let l = |op: &ColorOp, rgb| op.lab.from_working(op.apply(rgb))[0];
        assert!(l(&mono(80.0), RED) > l(&plain, RED) + 0.02);
        assert!(l(&mono(-80.0), RED) < l(&plain, RED) - 0.02);
        assert!((l(&mono(80.0), GREY) - l(&plain, GREY)).abs() < 1e-4);
        // Grading still tones the grey (sepia).
        let mut c = ColorControls {
            grayscale: true,
            ..Default::default()
        };
        c.grading.global = Wheel {
            hue: 35.0,
            sat: 30.0,
            lum: 0.0,
        };
        let op = ColorOp::new(&c, DEFAULT_STRENGTHS);
        assert!(chroma(&op, op.apply(GREY)) > 0.005);
    }
}
