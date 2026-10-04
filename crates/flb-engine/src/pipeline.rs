//! Develop pipeline v0.
//!
//! Order (subset of the Camera Raw model, docs/research/05 §1):
//! 1. camera RGB → white balance + camera matrix → XYZ → Bradford to D50 →
//!    linear ProPhoto → Exposure (2^EV)                      [scene-linear]
//! 2. Dehaze                                                 [scene-linear]
//! 3. base look → Basic tone controls (luminance, hue-preserving) [display-linear]
//! 4. Texture / Clarity (local contrast)
//! 5. colour (Vibrance, Saturation, HSL, Color Grading in OkLab) →
//!    parametric + point curves (per channel, encoded)
//! 6. noise reduction → sharpening
//! 7. post-crop vignette → grain → output
//!
//! The exact order Camera Raw uses is still to be confirmed by sweeps. Each
//! stage is a pass over one shared [`Image`] buffer, so spatial operations can
//! sit between the per-pixel ones.
//!
//! Only a few settings render so far; [`DevelopSettings::from_settings`]
//! reports everything else as not rendered yet.

use std::collections::BTreeMap;

use flb_color::matrix::{D50_XYZ, Mat3, PROPHOTO_TO_XYZ_D50, apply3, bradford, invert3, mul3};
use flb_color::temperature::{temp_tint_to_xy, xy_to_temp_tint};
use flb_presets::{Value, curve_points};
use rayon::prelude::*;

pub use crate::buffer::Image as ProPhotoImage;
use crate::buffer::{Image, luminance_weights};
use crate::calibration::Calibration;
use crate::color::{self, ColorControls, ColorOp, Wheel};
use crate::curve::Spline;
use crate::detail::{NoiseReduction, Sharpening};
use crate::effects::{Grain, Vignette};
use crate::local::LocalContrast;
use crate::raw::{CameraImage, neutral_for_xy, xy_for_neutral};
use crate::tone::{self, ParametricCurve, ToneControls};
use crate::{dehaze, detail, effects, local};

#[derive(Debug, Clone, PartialEq)]
pub enum WhiteBalance {
    AsShot,
    Custom { temperature: f64, tint: f64 },
}

/// The base rendering applied before user tone edits.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BaseLook {
    /// Scene-linear straight to output (for measurements).
    Linear,
    /// Placeholder "normal raw look": a filmic toe and shoulder (ACES fit).
    /// NOT Adobe's curve; to be replaced by a base curve fitted to
    /// Lightroom renders (docs/lightroom-trial-plan.md, week 3).
    Default,
}

impl BaseLook {
    /// Scene-linear → display-linear, per channel.
    fn apply(self, x: f32) -> f32 {
        match self {
            BaseLook::Linear => x,
            BaseLook::Default => {
                // Narkowicz's fit of the ACES filmic curve: toe for contrast, smooth
                // shoulder for highlights; scene 0.18 lands near display 0.27 (sRGB ~0.55).
                let x = x.max(0.0);
                ((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14)).min(1.0)
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct DevelopSettings {
    pub base_look: BaseLook,
    pub white_balance: WhiteBalance,
    pub exposure_ev: f64,
    pub tone: ToneControls,
    pub color: ColorControls,
    pub parametric: ParametricCurve,
    pub curve_rgb: Option<Spline>,
    pub curve_channels: [Option<Spline>; 3],
    pub dehaze: f64,
    pub local: LocalContrast,
    pub noise_reduction: NoiseReduction,
    pub sharpening: Sharpening,
    pub vignette: Vignette,
    pub grain: Grain,
    /// Model constants; [`Calibration::default`] until fitted against Lightroom.
    pub calibration: Calibration,
}

impl Default for DevelopSettings {
    fn default() -> Self {
        DevelopSettings {
            base_look: BaseLook::Default,
            white_balance: WhiteBalance::AsShot,
            exposure_ev: 0.0,
            tone: ToneControls::default(),
            color: ColorControls::default(),
            parametric: ParametricCurve::default(),
            curve_rgb: None,
            curve_channels: [None, None, None],
            dehaze: 0.0,
            local: LocalContrast::default(),
            noise_reduction: NoiseReduction::default(),
            sharpening: Sharpening::default(),
            vignette: Vignette::default(),
            grain: Grain::default(),
            calibration: Calibration::default(),
        }
    }
}

/// Settings that this pipeline version renders.
pub const RENDERED_TAGS: &[&str] = &[
    "WhiteBalance",
    "Temperature",
    "Tint",
    "Exposure2012",
    "Contrast2012",
    "Highlights2012",
    "Shadows2012",
    "Whites2012",
    "Blacks2012",
    "Vibrance",
    "Saturation",
    "ParametricShadows",
    "ParametricDarks",
    "ParametricLights",
    "ParametricHighlights",
    "ParametricShadowSplit",
    "ParametricMidtoneSplit",
    "ParametricHighlightSplit",
    "ColorGradeShadowHue",
    "ColorGradeShadowSat",
    "ColorGradeShadowLum",
    "ColorGradeMidtoneHue",
    "ColorGradeMidtoneSat",
    "ColorGradeMidtoneLum",
    "ColorGradeHighlightHue",
    "ColorGradeHighlightSat",
    "ColorGradeHighlightLum",
    "ColorGradeGlobalHue",
    "ColorGradeGlobalSat",
    "ColorGradeGlobalLum",
    "ColorGradeBlending",
    "SplitToningShadowHue",
    "SplitToningShadowSaturation",
    "SplitToningHighlightHue",
    "SplitToningHighlightSaturation",
    "SplitToningBalance",
    "Dehaze",
    "Texture",
    "Clarity2012",
    "Sharpness",
    "SharpenRadius",
    "SharpenDetail",
    "SharpenEdgeMasking",
    "LuminanceSmoothing",
    "LuminanceNoiseReductionDetail",
    "LuminanceNoiseReductionContrast",
    "ColorNoiseReduction",
    "ColorNoiseReductionDetail",
    "ColorNoiseReductionSmoothness",
    "PostCropVignetteAmount",
    "PostCropVignetteMidpoint",
    "PostCropVignetteRoundness",
    "PostCropVignetteFeather",
    "PostCropVignetteHighlightContrast",
    "PostCropVignetteStyle",
    "GrainAmount",
    "GrainSize",
    "GrainFrequency",
    "GrainSeed",
    "ToneCurvePV2012",
    "ToneCurvePV2012Red",
    "ToneCurvePV2012Green",
    "ToneCurvePV2012Blue",
];

/// Whether `tag` is rendered by this pipeline version (fixed list plus the 24 HSL tags).
pub fn is_rendered(tag: &str) -> bool {
    RENDERED_TAGS.contains(&tag)
        || color::BANDS.iter().any(|band| {
            ["Hue", "Saturation", "Luminance"]
                .iter()
                .any(|kind| tag == format!("{kind}Adjustment{band}"))
        })
}

fn num(settings: &BTreeMap<String, Value>, key: &str) -> Option<f64> {
    settings.get(key).and_then(Value::as_f64)
}

fn wheel(settings: &BTreeMap<String, Value>, region: &str) -> Wheel {
    Wheel {
        hue: num(settings, &format!("ColorGrade{region}Hue")).unwrap_or(0.0),
        sat: num(settings, &format!("ColorGrade{region}Sat")).unwrap_or(0.0),
        lum: num(settings, &format!("ColorGrade{region}Lum")).unwrap_or(0.0),
    }
}

fn spline_of(settings: &BTreeMap<String, Value>, key: &str) -> Option<Spline> {
    let points = curve_points(settings.get(key)?)?;
    Spline::from_points_255(&points).filter(|s| !s.is_identity())
}

impl DevelopSettings {
    /// Build from Camera Raw settings. Returns the settings plus the names of
    /// present tags this pipeline does not render yet.
    pub fn from_settings(settings: &BTreeMap<String, Value>) -> (DevelopSettings, Vec<String>) {
        let mut s = DevelopSettings::default();
        let wb_mode = settings
            .get("WhiteBalance")
            .and_then(Value::as_str)
            .unwrap_or("As Shot");
        let temperature = settings.get("Temperature").and_then(Value::as_f64);
        if wb_mode != "As Shot" {
            if let Some(temperature) = temperature {
                let tint = settings.get("Tint").and_then(Value::as_f64).unwrap_or(0.0);
                s.white_balance = WhiteBalance::Custom { temperature, tint };
            }
        }
        if let Some(ev) = num(settings, "Exposure2012") {
            s.exposure_ev = ev;
        }
        s.tone = ToneControls {
            contrast: num(settings, "Contrast2012").unwrap_or(0.0),
            highlights: num(settings, "Highlights2012").unwrap_or(0.0),
            shadows: num(settings, "Shadows2012").unwrap_or(0.0),
            whites: num(settings, "Whites2012").unwrap_or(0.0),
            blacks: num(settings, "Blacks2012").unwrap_or(0.0),
        };
        let pd = ParametricCurve::default();
        s.parametric = ParametricCurve {
            shadows: num(settings, "ParametricShadows").unwrap_or(0.0),
            darks: num(settings, "ParametricDarks").unwrap_or(0.0),
            lights: num(settings, "ParametricLights").unwrap_or(0.0),
            highlights: num(settings, "ParametricHighlights").unwrap_or(0.0),
            shadow_split: num(settings, "ParametricShadowSplit").unwrap_or(pd.shadow_split),
            midtone_split: num(settings, "ParametricMidtoneSplit").unwrap_or(pd.midtone_split),
            highlight_split: num(settings, "ParametricHighlightSplit")
                .unwrap_or(pd.highlight_split),
        };
        let mut c = ColorControls {
            vibrance: num(settings, "Vibrance").unwrap_or(0.0),
            saturation: num(settings, "Saturation").unwrap_or(0.0),
            ..Default::default()
        };
        for (i, band) in color::BANDS.iter().enumerate() {
            for (j, kind) in ["Hue", "Saturation", "Luminance"].iter().enumerate() {
                c.hsl[i][j] = num(settings, &format!("{kind}Adjustment{band}")).unwrap_or(0.0);
            }
        }
        let g = &mut c.grading;
        g.shadows = wheel(settings, "Shadow");
        g.midtones = wheel(settings, "Midtone");
        g.highlights = wheel(settings, "Highlight");
        g.global = wheel(settings, "Global");
        g.blending = num(settings, "ColorGradeBlending").unwrap_or(50.0);
        // Camera Raw reuses SplitToningBalance as the Color Grading balance.
        g.balance = num(settings, "SplitToningBalance").unwrap_or(0.0);
        // Legacy split toning (pre-Color-Grading presets): map onto the shadow/highlight wheels.
        let has_grade = settings
            .keys()
            .any(|k| k.starts_with("ColorGrade") && k != "ColorGradeBlending");
        if !has_grade {
            g.shadows.hue = num(settings, "SplitToningShadowHue").unwrap_or(0.0);
            g.shadows.sat = num(settings, "SplitToningShadowSaturation").unwrap_or(0.0);
            g.highlights.hue = num(settings, "SplitToningHighlightHue").unwrap_or(0.0);
            g.highlights.sat = num(settings, "SplitToningHighlightSaturation").unwrap_or(0.0);
        }
        s.color = c;
        s.dehaze = num(settings, "Dehaze").unwrap_or(0.0);
        s.local = LocalContrast {
            texture: num(settings, "Texture").unwrap_or(0.0),
            clarity: num(settings, "Clarity2012").unwrap_or(0.0),
        };
        let nd = NoiseReduction::default();
        s.noise_reduction = NoiseReduction {
            luminance: num(settings, "LuminanceSmoothing").unwrap_or(nd.luminance),
            luminance_detail: num(settings, "LuminanceNoiseReductionDetail")
                .unwrap_or(nd.luminance_detail),
            luminance_contrast: num(settings, "LuminanceNoiseReductionContrast")
                .unwrap_or(nd.luminance_contrast),
            color: num(settings, "ColorNoiseReduction").unwrap_or(nd.color),
            color_detail: num(settings, "ColorNoiseReductionDetail").unwrap_or(nd.color_detail),
            color_smoothness: num(settings, "ColorNoiseReductionSmoothness")
                .unwrap_or(nd.color_smoothness),
        };
        let sd = Sharpening::default();
        s.sharpening = Sharpening {
            amount: num(settings, "Sharpness").unwrap_or(sd.amount),
            radius: num(settings, "SharpenRadius").unwrap_or(sd.radius),
            detail: num(settings, "SharpenDetail").unwrap_or(sd.detail),
            masking: num(settings, "SharpenEdgeMasking").unwrap_or(sd.masking),
        };
        let vd = Vignette::default();
        s.vignette = Vignette {
            amount: num(settings, "PostCropVignetteAmount").unwrap_or(vd.amount),
            midpoint: num(settings, "PostCropVignetteMidpoint").unwrap_or(vd.midpoint),
            roundness: num(settings, "PostCropVignetteRoundness").unwrap_or(vd.roundness),
            feather: num(settings, "PostCropVignetteFeather").unwrap_or(vd.feather),
            highlights: num(settings, "PostCropVignetteHighlightContrast").unwrap_or(vd.highlights),
            style: num(settings, "PostCropVignetteStyle")
                .map_or(vd.style, |v| v.clamp(1.0, 3.0) as u8),
        };
        let gd = Grain::default();
        s.grain = Grain {
            amount: num(settings, "GrainAmount").unwrap_or(gd.amount),
            size: num(settings, "GrainSize").unwrap_or(gd.size),
            roughness: num(settings, "GrainFrequency").unwrap_or(gd.roughness),
            seed: num(settings, "GrainSeed").map_or(gd.seed, |v| v as u64),
        };
        s.curve_rgb = spline_of(settings, "ToneCurvePV2012");
        s.curve_channels = [
            spline_of(settings, "ToneCurvePV2012Red"),
            spline_of(settings, "ToneCurvePV2012Green"),
            spline_of(settings, "ToneCurvePV2012Blue"),
        ];
        let registry = flb_presets::registry::Registry::builtin();
        let not_rendered = settings
            .keys()
            .filter(|k| !is_rendered(k))
            .filter(|k| {
                // Metadata tags are not "missing features".
                !matches!(
                    registry.lookup(k).map(|i| i.status),
                    Some(flb_presets::registry::Status::Metadata)
                )
            })
            .filter(|k| {
                // A setting at its default changes nothing, so it is not missing either.
                let is_default = match (settings[k.as_str()].as_f64(), registry.numeric_default(k))
                {
                    (Some(v), Some(d)) => (v - d).abs() < 1e-9,
                    _ => false,
                };
                !is_default && k.as_str() != "ToneCurveName2012"
            })
            .cloned()
            .collect();
        (s, not_rendered)
    }
}

/// Camera RGB → linear ProPhoto matrix for a given white (camera-space neutral).
/// The neutral maps to ProPhoto (1, 1, 1) (i.e. D50 white).
pub fn camera_to_prophoto(xyz_to_cam: &Mat3, neutral: [f64; 3]) -> Mat3 {
    let cam_to_xyz = invert3(xyz_to_cam);
    let white = apply3(&cam_to_xyz, neutral);
    let white = [white[0] / white[1], 1.0, white[2] / white[1]];
    let adapt = bradford(white, D50_XYZ);
    let m = mul3(&invert3(&PROPHOTO_TO_XYZ_D50), &mul3(&adapt, &cam_to_xyz));
    // Scale so the neutral lands exactly on 1.0 (removes the neutral's own magnitude).
    let n = apply3(&m, neutral);
    let k = 1.0 / n[1];
    m.map(|row| row.map(|v| v * k))
}

/// Temperature/Tint that Lightroom would show for this image's as-shot white.
pub fn as_shot_temp_tint(img: &CameraImage) -> (f64, f64) {
    let (x, y) = xy_for_neutral(&img.xyz_to_cam, img.as_shot_neutral);
    let tt = xy_to_temp_tint(x, y);
    (tt.temperature, tt.tint)
}

fn srgb_encode(v: f32) -> f32 {
    if v <= 0.003_130_8 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    }
}

fn srgb_decode(v: f32) -> f32 {
    if v <= 0.040_45 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

const LUT: usize = 4096;

fn lookup(lut: &[f32], v: f32) -> f32 {
    let pos = v.clamp(0.0, 1.0) * (LUT - 1) as f32;
    let i = (pos as usize).min(LUT - 2);
    let f = pos - i as f32;
    lut[i] * (1.0 - f) + lut[i + 1] * f
}

pub fn render(img: &CameraImage, s: &DevelopSettings) -> ProPhotoImage {
    let cal = &s.calibration;

    // 1. Camera RGB → scene-linear ProPhoto with white balance and Exposure.
    let neutral = match s.white_balance {
        WhiteBalance::AsShot => img.as_shot_neutral,
        WhiteBalance::Custom { temperature, tint } => {
            let (x, y) = temp_tint_to_xy(temperature, tint);
            neutral_for_xy(&img.xyz_to_cam, x, y)
        }
    };
    let m = camera_to_prophoto(&img.xyz_to_cam, neutral).map(|row| row.map(|v| v as f32));
    let gain = 2f32.powf(s.exposure_ev as f32);
    let mut out = Image {
        width: img.width,
        height: img.height,
        rgb: vec![0.0f32; img.rgb.len()],
    };
    out.rgb
        .par_chunks_mut(3)
        .zip(img.rgb.par_chunks(3))
        .for_each(|(o, c)| {
            for (row, slot) in m.iter().zip(o.iter_mut()) {
                *slot = (row[0] * c[0] + row[1] * c[1] + row[2] * c[2]) * gain;
            }
        });

    // 2. Dehaze (scene-linear).
    if s.dehaze != 0.0 {
        dehaze::apply(&mut out, s.dehaze, &cal.dehaze);
    }

    // 3. Base look and Basic tone controls → display-linear.
    let tone_lut = (!s.tone.is_neutral()).then(|| tone::tone_lut(&s.tone, &cal.tone, LUT));
    let lum_weights = luminance_weights();
    let base = s.base_look;
    out.map_pixels(|c| {
        let mut p = c.map(|v| base.apply(v));
        if let Some(lut) = &tone_lut {
            // Hue-preserving: scale RGB by the luminance curve's ratio.
            let y = lum_weights[0] * p[0] + lum_weights[1] * p[1] + lum_weights[2] * p[2];
            if y > 1e-6 {
                let target = srgb_decode(lookup(lut, srgb_encode(y.min(1.0))));
                let scale = target / y.min(1.0);
                p = p.map(|v| v * scale);
            }
        }
        p
    });

    // 4. Texture / Clarity.
    if !s.local.is_neutral() {
        local::apply(&mut out, &s.local, &cal.local);
    }

    // 5. Colour, then parametric and point curves.
    let rgb_lut = s.curve_rgb.as_ref().map(|c| c.lut(LUT));
    let channel_luts: Vec<Option<Vec<f32>>> = s
        .curve_channels
        .iter()
        .map(|c| c.as_ref().map(|c| c.lut(LUT)))
        .collect();
    let parametric_lut =
        (!s.parametric.is_neutral()).then(|| tone::parametric_lut(&s.parametric, &cal.tone, LUT));
    let has_curves =
        parametric_lut.is_some() || rgb_lut.is_some() || channel_luts.iter().any(Option::is_some);
    let color_op = (!s.color.is_neutral()).then(|| ColorOp::new(&s.color, cal.color));
    if color_op.is_some() || has_curves {
        out.map_pixels(|c| {
            let mut p = c;
            if let Some(op) = &color_op {
                p = op.apply(p);
            }
            if has_curves {
                // Curves act on encoded values (sRGB transfer for now; to be calibrated, see docs/research/05 §8).
                for (ch, v) in p.iter_mut().enumerate() {
                    let mut e = srgb_encode(v.clamp(0.0, 1.0));
                    if let Some(lut) = &parametric_lut {
                        e = lookup(lut, e);
                    }
                    if let Some(lut) = &rgb_lut {
                        e = lookup(lut, e);
                    }
                    if let Some(lut) = &channel_luts[ch] {
                        e = lookup(lut, e);
                    }
                    *v = srgb_decode(e);
                }
            }
            p
        });
    }

    // 6. Detail: noise reduction, then sharpening.
    detail::apply_noise_reduction(&mut out, &s.noise_reduction, &cal.detail);
    detail::apply_sharpening(&mut out, &s.sharpening, &cal.detail);

    // 7. Effects: post-crop vignette, then grain.
    if s.vignette.amount != 0.0 {
        effects::apply_vignette(&mut out, &s.vignette, &cal.effects);
    }
    if s.grain.amount != 0.0 {
        effects::apply_grain(&mut out, &s.grain, &cal.effects);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neutral_maps_to_prophoto_white() {
        // A plausible camera matrix (XYZ → camera) for a generic Bayer sensor.
        let xyz_to_cam = [[0.7, 0.1, -0.1], [-0.4, 1.2, 0.2], [-0.05, 0.15, 0.6]];
        let neutral = neutral_for_xy(&xyz_to_cam, 0.3127, 0.3290);
        let m = camera_to_prophoto(&xyz_to_cam, neutral);
        let w = apply3(&m, neutral);
        for c in w {
            assert!((c - 1.0).abs() < 1e-9, "{w:?}");
        }
        // A darker grey of the same chromaticity stays neutral and scales linearly.
        let g = apply3(&m, neutral.map(|v| v * 0.18));
        for c in g {
            assert!((c - 0.18).abs() < 1e-9, "{g:?}");
        }
    }

    #[test]
    fn base_look_is_monotonic_and_bounded() {
        let mut last = -1.0;
        for i in 0..=1000 {
            let x = i as f32 / 100.0;
            let y = BaseLook::Default.apply(x);
            assert!(y >= last && (0.0..=1.0).contains(&y), "x={x} y={y}");
            last = y;
        }
        assert_eq!(BaseLook::Default.apply(0.0), 0.0);
        assert_eq!(BaseLook::Linear.apply(0.3), 0.3);
    }

    fn map(pairs: &[(&str, &str)]) -> BTreeMap<String, Value> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), Value::Text(v.to_string())))
            .collect()
    }

    #[test]
    fn parses_tone_colour_and_grading() {
        let m = map(&[
            ("Contrast2012", "+40"),
            ("Highlights2012", "-60"),
            ("Vibrance", "+30"),
            ("SaturationAdjustmentGreen", "-40"),
            ("HueAdjustmentRed", "+15"),
            ("ColorGradeShadowHue", "200"),
            ("ColorGradeShadowSat", "25"),
            ("ColorGradeBlending", "70"),
            ("SplitToningBalance", "-20"),
            ("ParametricLights", "+10"),
        ]);
        let (s, missing) = DevelopSettings::from_settings(&m);
        assert!(missing.is_empty(), "{missing:?}");
        assert_eq!(s.tone.contrast, 40.0);
        assert_eq!(s.tone.highlights, -60.0);
        assert_eq!(s.color.vibrance, 30.0);
        assert_eq!(s.color.hsl[3][1], -40.0); // Green saturation
        assert_eq!(s.color.hsl[0][0], 15.0); // Red hue
        assert_eq!(s.color.grading.shadows.hue, 200.0);
        assert_eq!(s.color.grading.blending, 70.0);
        assert_eq!(s.color.grading.balance, -20.0);
        assert_eq!(s.parametric.lights, 10.0);
    }

    #[test]
    fn legacy_split_toning_maps_to_grading_wheels() {
        let m = map(&[
            ("SplitToningShadowHue", "210"),
            ("SplitToningShadowSaturation", "18"),
            ("SplitToningHighlightHue", "45"),
            ("SplitToningHighlightSaturation", "12"),
        ]);
        let (s, _) = DevelopSettings::from_settings(&m);
        assert_eq!(
            s.color.grading.shadows,
            Wheel {
                hue: 210.0,
                sat: 18.0,
                lum: 0.0
            }
        );
        assert_eq!(s.color.grading.highlights.sat, 12.0);
        // When Color Grading tags are present they win over split toning.
        let mut m2 = m.clone();
        m2.insert("ColorGradeShadowSat".into(), Value::Text("5".into()));
        let (s2, _) = DevelopSettings::from_settings(&m2);
        assert_eq!(s2.color.grading.shadows.sat, 5.0);
    }

    #[test]
    fn settings_parse_and_report() {
        let mut map = BTreeMap::new();
        map.insert("WhiteBalance".to_string(), Value::Text("Custom".into()));
        map.insert("Temperature".to_string(), Value::Text("5500".into()));
        map.insert("Tint".to_string(), Value::Text("+10".into()));
        map.insert("Exposure2012".to_string(), Value::Text("+0.5".into()));
        map.insert("LensProfileEnable".to_string(), Value::Text("1".into()));
        map.insert("Name".to_string(), Value::Text("x".into()));
        let (s, missing) = DevelopSettings::from_settings(&map);
        assert_eq!(
            s.white_balance,
            WhiteBalance::Custom {
                temperature: 5500.0,
                tint: 10.0
            }
        );
        assert_eq!(s.exposure_ev, 0.5);
        assert_eq!(missing, vec!["LensProfileEnable".to_string()]);
    }
}
