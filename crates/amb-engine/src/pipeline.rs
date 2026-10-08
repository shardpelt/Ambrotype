//! Develop pipeline v0.
//!
//! Order (subset of the Camera Raw model, docs/research/05 §1):
//! 0. Lens Corrections on camera RGB: vignetting, Remove CA, distortion
//!    (profile + manual), then crop and straighten, in one resampling pass
//!    (the image is the crop's size from here on)            [sensor-linear]
//!    → film negative conversion (optional, `amb:` settings) [sensor-linear]
//! 1. camera RGB → white balance + camera matrix → XYZ → Bradford to D50 →
//!    linear ProPhoto → Exposure (2^EV) → Defringe           [scene-linear]
//! 2. Dehaze                                                 [scene-linear]
//! 3. base look → Basic tone controls (luminance, hue-preserving) [display-linear]
//! 4. Texture / Clarity (local contrast)
//! 5. colour (Vibrance, Saturation, HSL, Color Grading in OkLab) →
//!    parametric + point curves (per channel, encoded)
//! 6. noise reduction → sharpening
//! 7. post-crop vignette (relative to the crop) → grain → output
//!
//! The exact order Camera Raw uses is still to be confirmed by sweeps. Each
//! stage is a pass over one shared [`Image`] buffer, so spatial operations can
//! sit between the per-pixel ones.
//!
//! Only a few settings render so far; [`DevelopSettings::from_settings`]
//! reports everything else as not rendered yet.

use std::collections::BTreeMap;

use amb_color::matrix::{D50_XYZ, Mat3, PROPHOTO_TO_XYZ_D50, apply3, bradford, invert3, mul3};
use amb_color::temperature::{temp_tint_to_xy, xy_to_temp_tint};
use amb_presets::{Value, curve_points};
use rayon::prelude::*;

pub use crate::buffer::Image as ProPhotoImage;
use crate::buffer::{Image, luminance_weights};
use crate::calibration::Calibration;
use crate::color::{self, ColorControls, ColorOp, Wheel};
use crate::crop::Crop;
use crate::curve::Spline;
use crate::detail::{NoiseReduction, Sharpening};
use crate::effects::{Grain, Vignette};
use crate::lens::{Defringe, LensCorrections};
use crate::local::LocalContrast;
use crate::negative::{Negative, NegativeType};
use crate::raw::{CameraImage, neutral_for_xy, xy_for_neutral};
use crate::tone::{self, ParametricCurve, ToneControls};
use crate::{dehaze, detail, effects, lens, local, negative};

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
    pub lens: LensCorrections,
    /// Crop and straighten (after the lens corrections, in stored orientation).
    pub crop: Crop,
    pub negative: Negative,
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
            lens: LensCorrections::default(),
            crop: Crop::default(),
            negative: Negative::default(),
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
    "ConvertToGrayscale",
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
    "LensProfileEnable",
    "LensProfileSetup",
    "LensProfileName",
    "LensProfileFilename",
    "LensProfileDigest",
    "LensProfileDistortionScale",
    "LensProfileVignettingScale",
    "LensProfileChromaticAberrationScale",
    "AutoLateralCA",
    "LensManualDistortionAmount",
    "VignetteAmount",
    "VignetteMidpoint",
    "DefringePurpleAmount",
    "DefringePurpleHueLo",
    "DefringePurpleHueHi",
    "DefringeGreenAmount",
    "DefringeGreenHueLo",
    "DefringeGreenHueHi",
    "CropConstrainToWarp",
    "HasCrop",
    "CropTop",
    "CropLeft",
    "CropBottom",
    "CropRight",
    "CropAngle",
    "amb:NegativeConvert",
    "amb:NegativeType",
    "amb:NegativeAuto",
    "amb:FilmStock",
    "amb:FilmStockAmount",
    "amb:FilmBaseRed",
    "amb:FilmBaseGreen",
    "amb:FilmBaseBlue",
    "amb:NegativeDensityLowRed",
    "amb:NegativeDensityLowGreen",
    "amb:NegativeDensityLowBlue",
    "amb:NegativeDensityHighRed",
    "amb:NegativeDensityHighGreen",
    "amb:NegativeDensityHighBlue",
    "amb:NegativeMidtone",
    "amb:NegativeExposure",
    "amb:NegativeContrast",
    "amb:NegativeCyanRed",
    "amb:NegativeMagentaGreen",
    "amb:NegativeYellowBlue",
    "amb:NegativeToe",
    "amb:NegativeShoulder",
    "amb:NegativeSaturation",
];

/// Whether `tag` is rendered by this pipeline version (fixed list plus the
/// 24 HSL tags and the 8 black & white mix tags).
pub fn is_rendered(tag: &str) -> bool {
    RENDERED_TAGS.contains(&tag)
        || color::BANDS.iter().any(|band| {
            ["Hue", "Saturation", "Luminance"]
                .iter()
                .any(|kind| tag == format!("{kind}Adjustment{band}"))
                || tag == format!("GrayMixer{band}")
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
            c.gray_mix[i] = num(settings, &format!("GrayMixer{band}")).unwrap_or(0.0);
        }
        c.grayscale = settings
            .get("ConvertToGrayscale")
            .and_then(Value::as_bool)
            .unwrap_or(false);
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
        s.lens = lens_corrections(settings);
        s.crop = crop_settings(settings);
        s.negative = negative_settings(settings);
        s.curve_rgb = spline_of(settings, "ToneCurvePV2012");
        s.curve_channels = [
            spline_of(settings, "ToneCurvePV2012Red"),
            spline_of(settings, "ToneCurvePV2012Green"),
            spline_of(settings, "ToneCurvePV2012Blue"),
        ];
        let registry = amb_presets::registry::Registry::builtin();
        let not_rendered = settings
            .keys()
            .filter(|k| !is_rendered(k))
            .filter(|k| {
                // Metadata tags are not "missing features".
                !matches!(
                    registry.lookup(k).map(|i| i.status),
                    Some(amb_presets::registry::Status::Metadata)
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

fn flag(settings: &BTreeMap<String, Value>, key: &str) -> bool {
    settings.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn text(settings: &BTreeMap<String, Value>, key: &str) -> Option<String> {
    settings
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|t| !t.is_empty())
}

fn lens_corrections(settings: &BTreeMap<String, Value>) -> LensCorrections {
    let ld = LensCorrections::default();
    let dd = Defringe::default();
    LensCorrections {
        profile_enable: flag(settings, "LensProfileEnable"),
        distortion_scale: num(settings, "LensProfileDistortionScale")
            .unwrap_or(ld.distortion_scale),
        vignetting_scale: num(settings, "LensProfileVignettingScale")
            .unwrap_or(ld.vignetting_scale),
        ca_scale: num(settings, "LensProfileChromaticAberrationScale").unwrap_or(ld.ca_scale),
        auto_ca: flag(settings, "AutoLateralCA"),
        manual_distortion: num(settings, "LensManualDistortionAmount").unwrap_or(0.0),
        vignette_amount: num(settings, "VignetteAmount").unwrap_or(0.0),
        vignette_midpoint: num(settings, "VignetteMidpoint").unwrap_or(ld.vignette_midpoint),
        defringe: Defringe {
            purple_amount: num(settings, "DefringePurpleAmount").unwrap_or(0.0),
            purple_hue_lo: num(settings, "DefringePurpleHueLo").unwrap_or(dd.purple_hue_lo),
            purple_hue_hi: num(settings, "DefringePurpleHueHi").unwrap_or(dd.purple_hue_hi),
            green_amount: num(settings, "DefringeGreenAmount").unwrap_or(0.0),
            green_hue_lo: num(settings, "DefringeGreenHueLo").unwrap_or(dd.green_hue_lo),
            green_hue_hi: num(settings, "DefringeGreenHueHi").unwrap_or(dd.green_hue_hi),
        },
        constrain_crop: flag(settings, "CropConstrainToWarp"),
        profile_name: text(settings, "LensProfileName"),
        profile_filename: text(settings, "LensProfileFilename"),
        profile: None,
    }
}

fn crop_settings(settings: &BTreeMap<String, Value>) -> Crop {
    let d = Crop::default();
    Crop {
        enabled: flag(settings, "HasCrop"),
        left: num(settings, "CropLeft").unwrap_or(d.left),
        top: num(settings, "CropTop").unwrap_or(d.top),
        right: num(settings, "CropRight").unwrap_or(d.right),
        bottom: num(settings, "CropBottom").unwrap_or(d.bottom),
        angle: num(settings, "CropAngle").unwrap_or(d.angle),
    }
}

fn rgb_triple(settings: &BTreeMap<String, Value>, prefix: &str) -> Option<[f64; 3]> {
    let r = num(settings, &format!("{prefix}Red"))?;
    let g = num(settings, &format!("{prefix}Green"))?;
    let b = num(settings, &format!("{prefix}Blue"))?;
    Some([r, g, b])
}

fn negative_settings(settings: &BTreeMap<String, Value>) -> Negative {
    let d = Negative::default();
    Negative {
        enabled: flag(settings, "amb:NegativeConvert"),
        kind: match settings.get("amb:NegativeType").and_then(Value::as_str) {
            Some(t)
                if t.eq_ignore_ascii_case("bw") || t.eq_ignore_ascii_case("black-and-white") =>
            {
                NegativeType::BlackAndWhite
            }
            _ => NegativeType::Color,
        },
        auto: settings
            .get("amb:NegativeAuto")
            .and_then(Value::as_bool)
            .unwrap_or(d.auto),
        stock: text(settings, "amb:FilmStock").unwrap_or(d.stock),
        stock_amount: num(settings, "amb:FilmStockAmount").unwrap_or(d.stock_amount),
        base: rgb_triple(settings, "amb:FilmBase"),
        density_low: rgb_triple(settings, "amb:NegativeDensityLow"),
        density_high: rgb_triple(settings, "amb:NegativeDensityHigh"),
        midtone: num(settings, "amb:NegativeMidtone"),
        exposure: num(settings, "amb:NegativeExposure").unwrap_or(0.0),
        contrast: num(settings, "amb:NegativeContrast").unwrap_or(0.0),
        balance: [
            num(settings, "amb:NegativeCyanRed").unwrap_or(0.0),
            num(settings, "amb:NegativeMagentaGreen").unwrap_or(0.0),
            num(settings, "amb:NegativeYellowBlue").unwrap_or(0.0),
        ],
        toe: num(settings, "amb:NegativeToe").unwrap_or(0.0),
        shoulder: num(settings, "amb:NegativeShoulder").unwrap_or(0.0),
        saturation: num(settings, "amb:NegativeSaturation").unwrap_or(0.0),
    }
}

/// White against which Temperature/Tint are read on a converted negative
/// (the conversion itself already balances the image).
pub const NEGATIVE_REFERENCE_WB: (f64, f64) = (5500.0, 0.0);

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

    // 0. Lens Corrections on camera RGB, then crop and straighten (one resample).
    let crop = s.crop.geometry(img.width, img.height);
    let lens_pass = (s.lens.needs_camera_pass() || crop.is_some()).then(|| {
        lens::apply_camera_cropped(
            &img.rgb,
            img.width,
            img.height,
            img.shot.crop_factor,
            &s.lens,
            &cal.lens,
            crop.as_ref(),
        )
    });
    let (width, height) = lens_pass
        .as_ref()
        .map_or((img.width, img.height), |p| (p.width, p.height));
    let mut camera_rgb = lens_pass.as_ref().map_or(&img.rgb, |p| &p.rgb);
    // 0b. Film negative → positive (on camera RGB, before white balance).
    let negative_rgb = s.negative.enabled.then(|| {
        let a = negative::resolve(&s.negative, camera_rgb, width, height, &cal.negative);
        let conversion = negative::Conversion::new(&s.negative, a, &cal.negative);
        let mut rgb = camera_rgb.clone();
        negative::apply(&mut rgb, &conversion);
        rgb
    });
    if let Some(rgb) = &negative_rgb {
        camera_rgb = rgb;
    }

    // 1. Camera RGB → scene-linear ProPhoto with white balance and Exposure.
    let neutral = match (s.white_balance.clone(), s.negative.enabled) {
        (WhiteBalance::AsShot, false) => img.as_shot_neutral,
        (WhiteBalance::Custom { temperature, tint }, false) => {
            let (x, y) = temp_tint_to_xy(temperature, tint);
            neutral_for_xy(&img.xyz_to_cam, x, y)
        }
        // A converted negative is already balanced: equal channels are white,
        // and Temperature/Tint move relative to the reference white.
        (WhiteBalance::AsShot, true) => [1.0; 3],
        (WhiteBalance::Custom { temperature, tint }, true) => {
            let (x, y) = temp_tint_to_xy(temperature, tint);
            let (rx, ry) = temp_tint_to_xy(NEGATIVE_REFERENCE_WB.0, NEGATIVE_REFERENCE_WB.1);
            let n = neutral_for_xy(&img.xyz_to_cam, x, y);
            let r = neutral_for_xy(&img.xyz_to_cam, rx, ry);
            [n[0] / r[0], 1.0, n[2] / r[2]]
        }
    };
    let m = camera_to_prophoto(&img.xyz_to_cam, neutral).map(|row| row.map(|v| v as f32));
    let gain = 2f32.powf(s.exposure_ev as f32);
    let mut out = Image {
        width,
        height,
        rgb: vec![0.0f32; camera_rgb.len()],
    };
    out.rgb
        .par_chunks_mut(3)
        .zip(camera_rgb.par_chunks(3))
        .for_each(|(o, c)| {
            for (row, slot) in m.iter().zip(o.iter_mut()) {
                *slot = (row[0] * c[0] + row[1] * c[1] + row[2] * c[2]) * gain;
            }
        });

    if !s.lens.defringe.is_neutral() {
        lens::apply_defringe(&mut out, &s.lens.defringe, &cal.lens);
    }

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
    if let Some(blank) = lens_pass.as_ref().and_then(|p| p.blank.as_deref()) {
        lens::fill_blank(&mut out, blank, &cal.lens);
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

    /// A camera whose RGB is linear ProPhoto (white balance and matrix are
    /// then the identity), with `f` giving each pixel.
    fn camera(w: usize, h: usize, f: impl Fn(usize, usize) -> [f32; 3]) -> CameraImage {
        let mut rgb = Vec::with_capacity(w * h * 3);
        for y in 0..h {
            for x in 0..w {
                rgb.extend_from_slice(&f(x, y));
            }
        }
        CameraImage {
            width: w,
            height: h,
            rgb,
            xyz_to_cam: invert3(&PROPHOTO_TO_XYZ_D50),
            as_shot_neutral: [1.0; 3],
            make: "Test".into(),
            model: "Synthetic".into(),
            shot: Default::default(),
            metadata: Default::default(),
        }
    }

    fn linear(settings: &[(&str, &str)]) -> DevelopSettings {
        let (s, missing) = DevelopSettings::from_settings(&map(settings));
        assert!(missing.is_empty(), "{missing:?}");
        // No detail defaults: they treat the image border differently from
        // the interior, which would blur the comparisons below.
        let mut s = DevelopSettings {
            base_look: BaseLook::Linear,
            ..s
        };
        s.sharpening.amount = 0.0;
        s.noise_reduction.color = 0.0;
        s
    }

    fn ramp(x: usize, y: usize) -> [f32; 3] {
        [
            0.1 + 0.002 * x as f32,
            0.2 + 0.003 * y as f32,
            0.05 + 0.001 * (x + y) as f32,
        ]
    }

    #[test]
    fn crop_tags_parse_and_render() {
        let m = map(&[
            ("HasCrop", "True"),
            ("CropLeft", "0.25"),
            ("CropTop", "0.1"),
            ("CropRight", "0.75"),
            ("CropBottom", "0.6"),
            ("CropAngle", "0"),
            ("CropConstrainToWarp", "0"),
        ]);
        let (s, missing) = DevelopSettings::from_settings(&m);
        assert!(missing.is_empty(), "{missing:?}");
        assert_eq!(
            s.crop,
            Crop {
                enabled: true,
                left: 0.25,
                top: 0.1,
                right: 0.75,
                bottom: 0.6,
                angle: 0.0
            }
        );
        // Without HasCrop the edges are kept but nothing is cropped.
        let (s, _) = DevelopSettings::from_settings(&map(&[("CropLeft", "0.25")]));
        assert!(!s.crop.enabled);
        let img = camera(200, 100, ramp);
        assert_eq!(render(&img, &s).width, 200);
    }

    #[test]
    fn crop_renders_the_sub_rectangle() {
        let img = camera(200, 100, ramp);
        let full = render(&img, &linear(&[]));
        assert_eq!((full.width, full.height), (200, 100));
        let s = linear(&[
            ("HasCrop", "True"),
            ("CropLeft", "0.25"),
            ("CropTop", "0.1"),
            ("CropRight", "0.75"),
            ("CropBottom", "0.6"),
        ]);
        let out = render(&img, &s);
        assert_eq!((out.width, out.height), (100, 50));
        for y in 0..50 {
            for x in 0..100 {
                let a = &out.rgb[(y * 100 + x) * 3..][..3];
                let b = &full.rgb[((y + 10) * 200 + x + 50) * 3..][..3];
                for c in 0..3 {
                    assert!((a[c] - b[c]).abs() < 1e-5, "({x},{y}) {a:?} vs {b:?}");
                }
            }
        }
    }

    #[test]
    fn crop_composes_with_lens_geometry() {
        let img = camera(240, 160, ramp);
        let lens = [("LensManualDistortionAmount", "40")];
        let full = render(&img, &linear(&lens));
        let mut cropped = lens.to_vec();
        cropped.extend([
            ("HasCrop", "True"),
            ("CropLeft", "0.5"),
            ("CropTop", "0.25"),
            ("CropRight", "1"),
            ("CropBottom", "0.75"),
        ]);
        let out = render(&img, &linear(&cropped));
        assert_eq!((out.width, out.height), (120, 80));
        // The same lens-corrected pixels, with the same blank fill.
        for y in 0..80 {
            for x in 0..120 {
                let a = &out.rgb[(y * 120 + x) * 3..][..3];
                let b = &full.rgb[((y + 40) * 240 + x + 120) * 3..][..3];
                assert_eq!(a, b, "({x},{y})");
            }
        }
    }

    #[test]
    fn straighten_levels_a_tilted_horizon() {
        // A horizon tilted by `tilt` degrees: dark above, light below, along
        // the line through the centre. Straightening by the matching angle
        // makes it level: every output row is (nearly) uniform.
        let (w, h) = (300, 200);
        let tilt: f64 = 6.0;
        let (sin, cos) = (crate::crop::ANGLE_SIGN * tilt).to_radians().sin_cos();
        let (cx, cy) = ((w as f64 - 1.0) / 2.0, (h as f64 - 1.0) / 2.0);
        let img = camera(w, h, |x, y| {
            // Distance below the horizon line, which runs along (cos, sin).
            let (dx, dy) = (x as f64 - cx, y as f64 - cy);
            let d = -dx * sin + dy * cos;
            let v = (0.5 + 0.4 * (d / 6.0).tanh()) as f32;
            [v; 3]
        });
        let row_spread = |img: &Image| -> f32 {
            (img.height / 4..img.height * 3 / 4)
                .map(|y| {
                    let row: Vec<f32> = (img.width / 8..img.width * 7 / 8)
                        .map(|x| img.rgb[(y * img.width + x) * 3 + 1])
                        .collect();
                    let lo = row.iter().cloned().fold(f32::MAX, f32::min);
                    let hi = row.iter().cloned().fold(f32::MIN, f32::max);
                    hi - lo
                })
                .fold(0.0, f32::max)
        };
        let tilted = render(&img, &linear(&[]));
        assert!(row_spread(&tilted) > 0.5, "the horizon is tilted");
        let angle = format!("{tilt}");
        let s = linear(&[
            ("HasCrop", "True"),
            ("CropLeft", "0.2"),
            ("CropTop", "0.2"),
            ("CropRight", "0.8"),
            ("CropBottom", "0.8"),
            ("CropAngle", angle.as_str()),
        ]);
        let level = render(&img, &s);
        assert!(row_spread(&level) < 0.02, "{}", row_spread(&level));
        // The opposite angle doubles the tilt instead.
        let angle = format!("{}", -tilt);
        let s = linear(&[
            ("HasCrop", "True"),
            ("CropLeft", "0.2"),
            ("CropTop", "0.2"),
            ("CropRight", "0.8"),
            ("CropBottom", "0.8"),
            ("CropAngle", angle.as_str()),
        ]);
        assert!(row_spread(&render(&img, &s)) > 0.5);
    }

    #[test]
    fn post_crop_vignette_is_centred_on_the_crop() {
        let img = camera(300, 200, |_, _| [0.5; 3]);
        // An off-centre crop: the left third of the frame.
        let s = linear(&[
            ("HasCrop", "True"),
            ("CropLeft", "0"),
            ("CropTop", "0.25"),
            ("CropRight", "0.4"),
            ("CropBottom", "0.75"),
            ("PostCropVignetteAmount", "-80"),
        ]);
        let out = render(&img, &s);
        assert_eq!((out.width, out.height), (120, 100));
        let at = |x: usize, y: usize| out.rgb[(y * out.width + x) * 3 + 1];
        let (w, h) = (out.width, out.height);
        let centre = at(w / 2, h / 2);
        assert!(
            (centre - 0.5).abs() < 1e-3,
            "crop centre untouched: {centre}"
        );
        // All four corners of the crop are darkened alike.
        let corners = [at(0, 0), at(w - 1, 0), at(0, h - 1), at(w - 1, h - 1)];
        for c in corners {
            assert!(c < 0.4, "{corners:?}");
            assert!((c - corners[0]).abs() < 1e-3, "{corners:?}");
        }
        // Left/right symmetric about the crop's centre, not the frame's.
        for x in 0..w / 2 {
            let (l, r) = (at(x, h / 2), at(w - 1 - x, h / 2));
            assert!((l - r).abs() < 1e-3, "x={x}: {l} vs {r}");
        }
    }

    #[test]
    fn settings_parse_and_report() {
        let mut map = BTreeMap::new();
        map.insert("WhiteBalance".to_string(), Value::Text("Custom".into()));
        map.insert("Temperature".to_string(), Value::Text("5500".into()));
        map.insert("Tint".to_string(), Value::Text("+10".into()));
        map.insert("Exposure2012".to_string(), Value::Text("+0.5".into()));
        map.insert("PerspectiveVertical".to_string(), Value::Text("10".into()));
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
        assert_eq!(missing, vec!["PerspectiveVertical".to_string()]);
    }
}
