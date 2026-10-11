// SPDX-License-Identifier: GPL-3.0-or-later
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
//! Spot removal ([`crate::retouch`]) runs at the end of step 1. Masks
//! ([`crate::mask`]) are drawn then, and each local slider acts through them
//! right after its global counterpart.
//!
//! The exact order Camera Raw uses is still to be confirmed by sweeps. Each
//! stage is a pass over one shared [`Image`] buffer, so spatial operations can
//! sit between the per-pixel ones.
//!
//! Only a few settings render so far; [`DevelopSettings::from_settings`]
//! reports everything else as not rendered yet.

use std::collections::BTreeMap;

use amb_color::matrix::{D50_XYZ, Mat3, PROPHOTO_TO_XYZ_D50, apply3, bradford, invert3, mul3};
use amb_color::temperature::{TempTint, temp_tint_to_xy, xy_to_temp_tint};
use amb_presets::{Value, curve_points};
use rayon::prelude::*;

pub use crate::buffer::Image as ProPhotoImage;
use crate::buffer::{Image, Rect, Tile, luminance_weights};
use crate::calibration::Calibration;
use crate::color::{self, ColorControls, ColorOp, Wheel};
use crate::crop::Crop;
use crate::curve::Spline;
use crate::detail::{NoiseReduction, Sharpening};
use crate::effects::{Grain, Vignette};
use crate::lens::{Defringe, LensCorrections};
use crate::local::LocalContrast;
use crate::mask::Masks;
use crate::negative::{Negative, NegativeType};
use crate::primaries::CameraCalibration;
use crate::raw::{CameraImage, neutral_for_xy, xy_for_neutral};
use crate::retouch::Spots;
use crate::tone::{self, ParametricCurve, ToneControls};
use crate::transform::{Transform, UprightMode};
use crate::{
    dehaze, detail, effects, lens, local, local_tone, mask, negative, primaries, retouch, upright,
};

#[derive(Debug, Clone, PartialEq)]
pub enum WhiteBalance {
    AsShot,
    Custom { temperature: f64, tint: f64 },
}

/// Which raw-default detail settings an edit leaves unset.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UnsetDetail {
    pub sharpness: bool,
    pub color_noise: bool,
}

impl Default for UnsetDetail {
    fn default() -> Self {
        Self {
            sharpness: true,
            color_noise: true,
        }
    }
}

/// Settings whose default differs for rendered images (Lightroom sharpens
/// and denoises raws by default, but not JPEGs).
pub const RENDERED_DEFAULTS: &[(&str, f64)] = &[("Sharpness", 0.0), ("ColorNoiseReduction", 0.0)];

/// Relative white balance of rendered images (JPEG and the like):
/// Lightroom's `IncrementalTemperature` and `IncrementalTint`, -100..100.
/// Raws ignore it, and rendered images ignore Kelvin.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct IncrementalWhiteBalance {
    pub temperature: f64,
    pub tint: f64,
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

/// The base look in use: ours, or a camera profile's tone curve.
enum Base<'a> {
    Look(BaseLook),
    Curve(&'a crate::profile::ToneCurve),
}

impl Base<'_> {
    fn apply(&self, x: f32) -> f32 {
        match self {
            Base::Look(l) => l.apply(x),
            Base::Curve(c) => c.eval(x),
        }
    }

    fn apply_rgb(&self, c: [f32; 3]) -> [f32; 3] {
        match self {
            Base::Look(l) => c.map(|v| l.apply(v)),
            // The DNG way: hue-preserving.
            Base::Curve(curve) => curve.apply_rgb(c),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DevelopSettings {
    pub base_look: BaseLook,
    pub white_balance: WhiteBalance,
    pub incremental_wb: IncrementalWhiteBalance,
    /// Settings left at Camera Raw's raw defaults because the edit doesn't
    /// set them; rendered images default them to 0 instead ([`RENDERED_DEFAULTS`]).
    pub unset_detail: UnsetDetail,
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
    /// The Transform sliders (after the lens corrections, before the crop).
    pub transform: Transform,
    /// The Calibration panel.
    pub camera_calibration: CameraCalibration,
    /// `CameraProfile`, as named in the settings.
    pub profile_name: Option<String>,
    /// The camera profile resolved for this photo by the caller
    /// ([`crate::profile::ProfileLibrary`]); `None` uses our own colour and
    /// base look.
    pub camera_profile: Option<std::sync::Arc<crate::profile::Dcp>>,
    /// Crop and straighten (after the lens corrections, in stored orientation).
    pub crop: Crop,
    pub negative: Negative,
    /// Masks and their local adjustments.
    pub masks: Masks,
    /// Spot removal (heal and clone).
    pub spots: Spots,
    /// Model constants; [`Calibration::default`] until fitted against Lightroom.
    pub calibration: Calibration,
}

impl Default for DevelopSettings {
    fn default() -> Self {
        DevelopSettings {
            base_look: BaseLook::Default,
            white_balance: WhiteBalance::AsShot,
            incremental_wb: IncrementalWhiteBalance::default(),
            unset_detail: UnsetDetail::default(),
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
            transform: Transform::default(),
            camera_calibration: CameraCalibration::default(),
            profile_name: None,
            camera_profile: None,
            crop: Crop::default(),
            negative: Negative::default(),
            masks: Masks::default(),
            spots: Spots::default(),
            calibration: Calibration::default(),
        }
    }
}

/// Settings that this pipeline version renders.
pub const RENDERED_TAGS: &[&str] = &[
    "WhiteBalance",
    "Temperature",
    "Tint",
    "IncrementalTemperature",
    "IncrementalTint",
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
    "PerspectiveVertical",
    "PerspectiveHorizontal",
    "PerspectiveRotate",
    "PerspectiveAspect",
    "PerspectiveScale",
    "PerspectiveX",
    "PerspectiveY",
    "PerspectiveUpright",
    "EnableTransform",
    "ShadowTint",
    "RedHue",
    "RedSaturation",
    "GreenHue",
    "GreenSaturation",
    "BlueHue",
    "BlueSaturation",
    "EnableCalibration",
    "CameraProfile",
    "CameraProfileDigest",
    "HasCrop",
    "CropTop",
    "CropLeft",
    "CropBottom",
    "CropRight",
    "CropAngle",
    "MaskGroupBasedCorrections",
    "RetouchAreas",
    "RetouchInfo",
    "EnableRetouch",
    "EnableMaskGroupBasedCorrections",
    "GradientBasedCorrections",
    "EnableGradientBasedCorrections",
    "CircularGradientBasedCorrections",
    "EnableCircularGradientBasedCorrections",
    "PaintBasedCorrections",
    "EnablePaintBasedCorrections",
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
        // As Lightroom: the relative sliders count only under a custom white balance.
        if wb_mode != "As Shot" {
            s.incremental_wb = IncrementalWhiteBalance {
                temperature: num(settings, "IncrementalTemperature").unwrap_or(0.0),
                tint: num(settings, "IncrementalTint").unwrap_or(0.0),
            };
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
        s.unset_detail = UnsetDetail {
            sharpness: !settings.contains_key("Sharpness"),
            color_noise: !settings.contains_key("ColorNoiseReduction"),
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
        s.transform = transform_settings(settings);
        s.camera_calibration = calibration_settings(settings);
        s.profile_name = text(settings, "CameraProfile");
        s.crop = crop_settings(settings);
        s.negative = negative_settings(settings);
        s.curve_rgb = spline_of(settings, "ToneCurvePV2012");
        s.curve_channels = [
            spline_of(settings, "ToneCurvePV2012Red"),
            spline_of(settings, "ToneCurvePV2012Green"),
            spline_of(settings, "ToneCurvePV2012Blue"),
        ];
        let (masks, mask_notes) = Masks::from_settings(settings);
        s.masks = masks;
        let (spots, spot_notes) = Spots::from_settings(settings);
        s.spots = spots;
        let registry = amb_presets::registry::Registry::builtin();
        let mut not_rendered: Vec<String> = settings
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
        not_rendered.extend(mask_notes);
        not_rendered.extend(spot_notes);
        match s.transform.upright_mode {
            UprightMode::Off => {}
            UprightMode::Guided => not_rendered.push(
                "PerspectiveUpright (Upright Guided: the guides aren't read yet, so it is left off)"
                    .to_string(),
            ),
            // Lightroom's own Upright analysis: we find the lines again.
            _ => not_rendered.retain(|k| !k.starts_with("Upright")),
        }
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

fn transform_settings(settings: &BTreeMap<String, Value>) -> Transform {
    let d = Transform::default();
    if settings.get("EnableTransform").and_then(Value::as_bool) == Some(false) {
        return d;
    }
    Transform {
        vertical: num(settings, "PerspectiveVertical").unwrap_or(d.vertical),
        horizontal: num(settings, "PerspectiveHorizontal").unwrap_or(d.horizontal),
        rotate: num(settings, "PerspectiveRotate").unwrap_or(d.rotate),
        aspect: num(settings, "PerspectiveAspect").unwrap_or(d.aspect),
        scale: num(settings, "PerspectiveScale").unwrap_or(d.scale),
        offset_x: num(settings, "PerspectiveX").unwrap_or(d.offset_x),
        offset_y: num(settings, "PerspectiveY").unwrap_or(d.offset_y),
        upright_mode: num(settings, "PerspectiveUpright").map_or(d.upright_mode, UprightMode::of),
        upright: None,
    }
}

impl DevelopSettings {
    /// What to tell the user when `CameraProfile` names a profile that
    /// wasn't found for this photo (the caller resolves profiles).
    pub fn profile_note(&self) -> Option<String> {
        let name = self.profile_name.as_deref()?;
        (self.camera_profile.is_none()).then(|| {
            format!(
                "CameraProfile (\"{name}\" is not among the camera profiles on this computer; our default look is used)"
            )
        })
    }
}

fn calibration_settings(settings: &BTreeMap<String, Value>) -> CameraCalibration {
    if settings.get("EnableCalibration").and_then(Value::as_bool) == Some(false) {
        return CameraCalibration::default();
    }
    let n = |k: &str| num(settings, k).unwrap_or(0.0);
    CameraCalibration {
        shadow_tint: n("ShadowTint"),
        hue: [n("RedHue"), n("GreenHue"), n("BlueHue")],
        saturation: [
            n("RedSaturation"),
            n("GreenSaturation"),
            n("BlueSaturation"),
        ],
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

/// Clipped highlights: where a channel reached its clip level its real value
/// is unknown, and white balance would scale the clipped channels unequally
/// (white turns pink or cyan). Clipped channels are raised towards the
/// brightest white-balanced channel, so blown areas go neutral and keep
/// their brightness for the tone curve to roll off. A channel also counts as
/// clipped next to where it clips: demosaicing mixes the clipped values into
/// the edge, which would otherwise leave a coloured fringe.
struct Unclip {
    clip: [f32; 3],
    /// White-balance gains (1 / neutral).
    gains: [f32; 3],
}

impl Unclip {
    fn new(clip: [f32; 3], neutral: [f64; 3]) -> Self {
        Self {
            clip,
            gains: neutral.map(|n| if n > 1e-9 { (1.0 / n) as f32 } else { 1.0 }),
        }
    }

    /// How clipped each channel is: 0 below 90% of its clip level, 1 from 98%.
    fn weights(&self, c: &[f32]) -> [f32; 3] {
        std::array::from_fn(|i| {
            let k = self.clip[i];
            if !k.is_finite() {
                return 0.0;
            }
            let t = ((c[i] - 0.9 * k) / (0.08 * k)).clamp(0.0, 1.0);
            t * t * (3.0 - 2.0 * t)
        })
    }

    /// The weights of row `y`, each the most clipped within `r` pixels.
    fn row_weights(
        &self,
        rgb: &[f32],
        w: usize,
        h: usize,
        y: usize,
        r: usize,
        out: &mut Vec<[f32; 3]>,
    ) {
        out.clear();
        out.resize(w, [0.0; 3]);
        for sy in y.saturating_sub(r)..(y + r + 1).min(h) {
            let row = &rgb[sy * w * 3..(sy + 1) * w * 3];
            for (x, p) in row.chunks(3).enumerate() {
                let k = self.weights(p);
                if k == [0.0; 3] {
                    continue;
                }
                for slot in &mut out[x.saturating_sub(r)..(x + r + 1).min(w)] {
                    for i in 0..3 {
                        slot[i] = slot[i].max(k[i]);
                    }
                }
            }
        }
    }

    fn apply(&self, c: [f32; 3], w: [f32; 3]) -> [f32; 3] {
        if w == [0.0; 3] {
            return c;
        }
        let balanced: [f32; 3] = std::array::from_fn(|i| c[i] * self.gains[i]);
        let top = balanced[0].max(balanced[1]).max(balanced[2]);
        std::array::from_fn(|i| (balanced[i] + w[i] * (top - balanced[i])) / self.gains[i])
    }
}

/// Relative white balance: mireds per step of `IncrementalTemperature`, and
/// Tint units per step of `IncrementalTint`. Adobe doesn't document its
/// scale; these are ours, unfitted.
const INCREMENTAL_MIRED_PER_STEP: f64 = 1.0;
const INCREMENTAL_TINT_PER_STEP: f64 = 1.0;

/// Temperature and Tint of a rendered image's own white (equal RGB).
fn rendered_white(img: &CameraImage) -> TempTint {
    let (x, y) = xy_for_neutral(&img.xyz_to_cam, [1.0; 3]);
    xy_to_temp_tint(x, y)
}

/// Camera-space white for relative white balance: the image's own white,
/// moved in mireds (+ warmer) and tint (+ magenta), as Kelvin and Tint do on raws.
pub fn incremental_neutral(img: &CameraImage, wb: IncrementalWhiteBalance) -> [f64; 3] {
    if wb == IncrementalWhiteBalance::default() {
        return [1.0; 3];
    }
    let base = rendered_white(img);
    let mired = (1e6 / base.temperature - INCREMENTAL_MIRED_PER_STEP * wb.temperature).max(20.0);
    let (x, y) = temp_tint_to_xy(1e6 / mired, base.tint + INCREMENTAL_TINT_PER_STEP * wb.tint);
    let (bx, by) = temp_tint_to_xy(base.temperature, base.tint);
    let n = neutral_for_xy(&img.xyz_to_cam, x, y);
    let r = neutral_for_xy(&img.xyz_to_cam, bx, by);
    [n[0] / r[0], 1.0, n[2] / r[2]]
}

/// The relative white balance that makes the white (x, y) neutral on a
/// rendered image: the inverse of [`incremental_neutral`].
pub fn incremental_for_xy(img: &CameraImage, x: f64, y: f64) -> IncrementalWhiteBalance {
    let base = rendered_white(img);
    let t = xy_to_temp_tint(x, y);
    IncrementalWhiteBalance {
        temperature: (1e6 / base.temperature - 1e6 / t.temperature) / INCREMENTAL_MIRED_PER_STEP,
        tint: (t.tint - base.tint) / INCREMENTAL_TINT_PER_STEP,
    }
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

/// Stage times for `AMB_TIMING=1`, printed to stderr when a render ends.
struct Stages {
    on: bool,
    start: std::time::Instant,
    last: std::time::Instant,
    marks: Vec<(&'static str, f64)>,
}

impl Stages {
    fn new() -> Stages {
        static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        let on = *ON.get_or_init(|| std::env::var_os("AMB_TIMING").is_some_and(|v| v != "0"));
        let now = std::time::Instant::now();
        Stages {
            on,
            start: now,
            last: now,
            marks: Vec::new(),
        }
    }

    fn mark(&mut self, stage: &'static str) {
        if self.on {
            let now = std::time::Instant::now();
            self.marks
                .push((stage, (now - self.last).as_secs_f64() * 1000.0));
            self.last = now;
        }
    }
}

impl Drop for Stages {
    fn drop(&mut self) {
        if self.on {
            let total = self.start.elapsed().as_secs_f64() * 1000.0;
            let parts: Vec<String> = self
                .marks
                .iter()
                .filter(|(_, ms)| *ms >= 0.5)
                .map(|(n, ms)| format!("{n} {ms:.0}"))
                .collect();
            eprintln!("render {total:.0} ms: {}", parts.join(", "));
        }
    }
}

/// What a render measures over the whole photo: a film negative's levels,
/// the haze, and the colours colour range masks select. A region render
/// can't measure these on its part of the photo, so it takes them from a
/// small render of all of it ([`measure_globals`]).
#[derive(Debug, Clone, Default)]
pub struct Globals {
    negative: Option<negative::Analysis>,
    dehaze: Option<dehaze::Estimate>,
    mask_dehaze: Vec<dehaze::Estimate>,
    colour_targets: Vec<Vec<[f32; 3]>>,
}

/// How one run of [`develop`] goes.
#[derive(Default)]
struct Pass<'a> {
    /// The rectangle of the output to render; all of it when `None`.
    window: Option<Rect>,
    /// The full-size output's size, when this run renders a small copy of
    /// it (estimates are laid out for the full size).
    full: Option<(usize, usize)>,
    /// Measurements to use instead of measuring.
    given: Option<&'a Globals>,
    /// Stop once the last measurement is taken.
    measure_only: bool,
}

pub fn render(img: &CameraImage, s: &DevelopSettings) -> ProPhotoImage {
    develop(img, s, Pass::default()).0
}

/// Long side of the small copy [`measure_globals`] renders.
pub const GLOBALS_LONG_SIDE: usize = 1600;

/// Whether a region render of `s` needs [`Globals`]: a film negative, Dehaze
/// (global or in a mask) or a colour range mask.
pub fn needs_globals(s: &DevelopSettings) -> bool {
    s.negative.enabled
        || s.dehaze != 0.0
        || s.masks.corrections.iter().any(|c| c.adjust.dehaze != 0.0)
        || s.masks.has_colour_range()
}

/// Measure [`Globals`] for region renders of `img` with `s`, on `small`: a
/// copy of `img` downscaled to about [`GLOBALS_LONG_SIDE`] ([`crate::raw::downscale`]).
pub fn measure_globals(img: &CameraImage, small: &CameraImage, s: &DevelopSettings) -> Globals {
    if !needs_globals(s) {
        return Globals::default();
    }
    let s = solve_upright(img, s);
    let full = output_size(img, &s);
    let pass = Pass {
        full: Some(full),
        measure_only: true,
        ..Pass::default()
    };
    let mut globals = develop(small, &s, pass).1;
    if s.masks.has_colour_range() {
        // A colour range selects the colours under its samples. What comes
        // before the masks is local, so each sample is measured exactly on a
        // small full-size render around it.
        let probe_given = Globals {
            colour_targets: Vec::new(),
            ..globals.clone()
        };
        let crop = s.crop.geometry(img.width, img.height);
        let points = s
            .masks
            .colour_sample_pixels((img.width, img.height), crop.as_ref());
        for (i, shape) in points.iter().enumerate() {
            for (j, &(x, y)) in shape.iter().enumerate() {
                let x = x.round().clamp(0.0, (full.0 - 1) as f64) as usize;
                let y = y.round().clamp(0.0, (full.1 - 1) as f64) as usize;
                let (x0, y0) = (x.saturating_sub(2), y.saturating_sub(2));
                let window = Rect {
                    x: x0,
                    y: y0,
                    width: (x + 3).min(full.0) - x0,
                    height: (y + 3).min(full.1) - y0,
                };
                let pass = Pass {
                    window: Some(plan(img, &s, window, early_reach(&s, full))),
                    given: Some(&probe_given),
                    measure_only: true,
                    ..Pass::default()
                };
                let probe = develop(img, &s, pass).1;
                let measured = probe.colour_targets.get(i).and_then(|t| t.get(j));
                if let (Some(slot), Some(t)) = (
                    globals.colour_targets.get_mut(i).and_then(|t| t.get_mut(j)),
                    measured,
                ) {
                    *slot = *t;
                }
            }
        }
    }
    globals
}

/// Render `window` of the output (in pixels of the full, unoriented render)
/// as [`render`] would, without rendering the rest: only the window and a
/// margin as wide as the edit's filters reach go through the pipeline.
/// `globals` come from [`measure_globals`] (unused when
/// [`needs_globals`] is false). Noise reduction measures the noise on the
/// region, and the globals come from a smaller copy, so these can differ
/// slightly from the full render; everything else matches it.
pub fn render_region(
    img: &CameraImage,
    s: &DevelopSettings,
    window: Rect,
    globals: &Globals,
) -> ProPhotoImage {
    let s = solve_upright(img, s);
    let (w, h) = output_size(img, &s);
    let x = window.x.min(w.saturating_sub(1));
    let y = window.y.min(h.saturating_sub(1));
    let window = Rect {
        x,
        y,
        width: window.width.clamp(1, w - x),
        height: window.height.clamp(1, h - y),
    };
    let work = region_plan(img, &s, window);
    let pass = Pass {
        window: Some(work),
        given: Some(globals),
        ..Pass::default()
    };
    let (out, _) = develop(img, &s, pass);
    let mut rgb = Vec::with_capacity(window.width * window.height * 3);
    for row in window.y - work.y..window.bottom() - work.y {
        let start = (row * work.width + window.x - work.x) * 3;
        rgb.extend_from_slice(&out.rgb[start..start + window.width * 3]);
    }
    Image {
        width: window.width,
        height: window.height,
        rgb,
    }
}

/// `s` with Upright's correction found on the full-size `img`, so a region
/// and a small copy use the same one.
fn solve_upright(img: &CameraImage, s: &DevelopSettings) -> DevelopSettings {
    let mut s = s.clone();
    let t = &mut s.transform;
    if t.upright.is_none() && t.upright_mode.is_solved() {
        let cal = &s.calibration;
        t.upright = upright::analyse(img, &s.lens, &cal.lens, t.upright_mode, &cal.upright);
        if t.upright.is_none() {
            // Too few lines: leave the photo uncorrected, without searching
            // again on the small copy.
            t.upright_mode = UprightMode::Off;
        }
    }
    s
}

/// The full render's size (the crop's), before orientation.
pub fn output_size(img: &CameraImage, s: &DevelopSettings) -> (usize, usize) {
    s.crop
        .geometry(img.width, img.height)
        .map_or((img.width, img.height), |c| (c.width, c.height))
}

/// Region renders start on multiples of this, so the subsampled filters
/// (box-downscaled copies up to 64×) see the same blocks as the full render.
const REGION_ALIGN: usize = 64;

/// The rectangle a region render develops for `window`: the window with a
/// margin as wide as the filters reach, grown to hold whatever spots
/// writing into it read.
fn region_plan(img: &CameraImage, s: &DevelopSettings, window: Rect) -> Rect {
    let full = output_size(img, s);
    let cal = &s.calibration;
    let scale = Tile::whole(full.0, full.1).scale();
    let reach = early_reach(s, full)
        + 2
        + local_tone::reach(&s.tone, &cal.tone, scale)
        + local::reach(&s.local, &cal.local, scale)
        + detail::noise_reduction_reach(&s.noise_reduction, &cal.detail)
        + detail::sharpening_reach(&s.sharpening, &cal.detail)
        + mask::reach(&s.masks, cal, scale);
    plan(img, s, window, reach)
}

/// How far the stages before spot removal reach (pixels): the
/// clipped-highlight window and Defringe.
fn early_reach(s: &DevelopSettings, full: (usize, usize)) -> usize {
    let tile = Tile::whole(full.0, full.1);
    let cal = &s.calibration;
    3 + if s.lens.defringe.is_neutral() {
        0
    } else {
        lens::defringe_radius(&s.lens.defringe, &cal.lens, &tile) + 2
    }
}

/// `window` grown by `reach`, then by whatever spots writing into it read,
/// clamped to the output and aligned.
fn plan(img: &CameraImage, s: &DevelopSettings, window: Rect, reach: usize) -> Rect {
    let (w, h) = output_size(img, s);
    let early = early_reach(s, (w, h));
    let grow = |r: [f64; 4], by: f64| -> [f64; 4] { [r[0] - by, r[1] - by, r[2] + by, r[3] + by] };
    let mut area = grow(
        [
            window.x as f64,
            window.y as f64,
            window.right() as f64,
            window.bottom() as f64,
        ],
        reach as f64,
    );
    let spots = retouch::footprints(
        &s.spots,
        (img.width, img.height),
        s.crop.geometry(img.width, img.height).as_ref(),
    );
    // A spot writing into the area reads its surroundings and its donor;
    // those may hold other spots in turn.
    for _ in 0..=spots.len() {
        let mut grown = false;
        for (writes, reads) in &spots {
            let hits = writes[0] < area[2]
                && writes[2] > area[0]
                && writes[1] < area[3]
                && writes[3] > area[1];
            let reads = grow(*reads, early as f64);
            if hits
                && (reads[0] < area[0]
                    || reads[1] < area[1]
                    || reads[2] > area[2]
                    || reads[3] > area[3])
            {
                area = [
                    area[0].min(reads[0]),
                    area[1].min(reads[1]),
                    area[2].max(reads[2]),
                    area[3].max(reads[3]),
                ];
                grown = true;
            }
        }
        if !grown {
            break;
        }
    }
    let clamp = |v: f64, max: usize| (v.max(0.0).ceil() as usize).min(max);
    let x0 = clamp(area[0].floor(), w) / REGION_ALIGN * REGION_ALIGN;
    let y0 = clamp(area[1].floor(), h) / REGION_ALIGN * REGION_ALIGN;
    let x1 = clamp(area[2], w).max(x0 + 1);
    let y1 = clamp(area[3], h).max(y0 + 1);
    Rect {
        x: x0,
        y: y0,
        width: x1 - x0,
        height: y1 - y0,
    }
}

/// The render: all of the output, or a rectangle of it, or (`measure_only`)
/// just the measurements of a small copy.
fn develop(img: &CameraImage, s: &DevelopSettings, pass: Pass) -> (Image, Globals) {
    let cal = &s.calibration;
    let mut stages = Stages::new();
    let mut globals = Globals::default();

    // 0. Lens Corrections on camera RGB, then Transform, crop and straighten
    // (one resample).
    let crop = s.crop.geometry(img.width, img.height);
    let mut transform = s.transform;
    if transform.upright.is_none() && transform.upright_mode.is_solved() {
        transform.upright = upright::analyse(
            img,
            &s.lens,
            &cal.lens,
            transform.upright_mode,
            &cal.upright,
        );
    }
    let warp = transform.sampling_map(img.width, img.height, &cal.transform);
    let (out_w, out_h) = crop
        .as_ref()
        .map_or((img.width, img.height), |c| (c.width, c.height));
    let tile = match pass.window {
        Some(r) => Tile {
            x: r.x,
            y: r.y,
            full_width: out_w,
            full_height: out_h,
        },
        None => Tile::whole(out_w, out_h),
    };
    let full = pass.full.unwrap_or((out_w, out_h));
    let lens_pass =
        (s.lens.needs_camera_pass() || crop.is_some() || warp.is_some() || pass.window.is_some())
            .then(|| {
                lens::apply_camera_window(
                    &img.rgb,
                    img.width,
                    img.height,
                    img.shot.crop_factor,
                    &s.lens,
                    &cal.lens,
                    crop.as_ref(),
                    warp.as_ref(),
                    pass.window,
                )
            });
    stages.mark("upright+lens+crop");
    let (width, height) = lens_pass
        .as_ref()
        .map_or((img.width, img.height), |p| (p.width, p.height));
    let mut camera_rgb = lens_pass.as_ref().map_or(&img.rgb, |p| &p.rgb);
    // 0b. Film negative → positive (on camera RGB, before white balance).
    let negative_rgb = s.negative.enabled.then(|| {
        let a = match pass.given.and_then(|g| g.negative) {
            Some(a) => a,
            None => negative::resolve(&s.negative, camera_rgb, width, height, &cal.negative),
        };
        globals.negative = Some(a);
        let conversion = negative::Conversion::new(&s.negative, a, &cal.negative);
        let mut rgb = camera_rgb.clone();
        negative::apply(&mut rgb, &conversion);
        rgb
    });
    if let Some(rgb) = &negative_rgb {
        camera_rgb = rgb;
    }

    // 1. Camera RGB → scene-linear ProPhoto with white balance and Exposure.
    // A camera profile brings its own matrices, tables and tone curve (not
    // for rendered images or converted negatives).
    let profile = s
        .camera_profile
        .as_deref()
        .filter(|_| !img.rendered && !s.negative.enabled);
    let neutral = match (s.white_balance.clone(), s.negative.enabled) {
        // Rendered images are balanced already: white is equal RGB, moved by
        // the relative sliders.
        _ if img.rendered => incremental_neutral(img, s.incremental_wb),
        (WhiteBalance::AsShot, false) => img.as_shot_neutral,
        (WhiteBalance::Custom { temperature, tint }, false) => {
            let (x, y) = temp_tint_to_xy(temperature, tint);
            match &profile {
                Some(p) => p.neutral_for_xy(x, y),
                None => neutral_for_xy(&img.xyz_to_cam, x, y),
            }
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
    // The Calibration panel's primaries fold into the camera matrix.
    let mut m = match profile {
        Some(p) => p.camera_to_prophoto(neutral),
        None => camera_to_prophoto(&img.xyz_to_cam, neutral),
    };
    if let Some(p) = s.camera_calibration.matrix(&cal.primaries) {
        m = mul3(&p, &m);
    }
    let m = m.map(|row| row.map(|v| v as f32));
    let shadow_tint = s.camera_calibration.shadow_tint(&cal.primaries);
    let gain = 2f32.powf(s.exposure_ev as f32);
    let hue_sat_map = profile.and_then(|p| p.hue_sat_map(p.white_temperature(neutral)));
    let look_table = profile.and_then(|p| p.look_table.as_ref());
    let profile_gain = profile.map_or(1.0, |p| 2f32.powf(p.exposure_offset as f32));
    // As in the DNG reference render, a brightening Exposure goes in before
    // the look table and a darkening one after it (DNG spec, look table).
    let (gain_before_look, gain_after_look) = if gain >= 1.0 {
        (profile_gain * gain, 1.0)
    } else {
        (profile_gain, gain)
    };
    let clipped = (!s.negative.enabled && img.clip.iter().any(|c| c.is_finite()))
        .then(|| Unclip::new(img.clip, neutral));
    // The window of most-clipped neighbours: wider on large photos.
    let unclip_reach = if out_w.max(out_h) >= 2500 { 2 } else { 1 };
    let mut out = Image {
        width,
        height,
        rgb: vec![0.0f32; camera_rgb.len()],
    };
    out.rgb
        .par_chunks_mut(width * 3)
        .enumerate()
        .for_each_init(Vec::new, |weights, (y, orow)| {
            let crow = &camera_rgb[y * width * 3..(y + 1) * width * 3];
            if let Some(u) = &clipped {
                u.row_weights(camera_rgb, width, height, y, unclip_reach, weights);
            }
            for (x, (o, c)) in orow.chunks_mut(3).zip(crow.chunks(3)).enumerate() {
                let c = match &clipped {
                    Some(u) => u.apply([c[0], c[1], c[2]], weights[x]),
                    None => [c[0], c[1], c[2]],
                };
                if profile.is_some() {
                    // The profile's map, its exposure offset, then the look
                    // table with Exposure on the side given above.
                    for (row, slot) in m.iter().zip(o.iter_mut()) {
                        *slot = row[0] * c[0] + row[1] * c[1] + row[2] * c[2];
                    }
                    if let Some(t) = &hue_sat_map {
                        t.apply(o);
                    }
                    for v in o.iter_mut() {
                        *v *= gain_before_look;
                    }
                    if let Some(t) = look_table {
                        t.apply(o);
                    }
                    for v in o.iter_mut() {
                        *v *= gain_after_look;
                    }
                } else {
                    for (row, slot) in m.iter().zip(o.iter_mut()) {
                        *slot = (row[0] * c[0] + row[1] * c[1] + row[2] * c[2]) * gain;
                    }
                }
                if let Some((stops, end)) = shadow_tint {
                    primaries::tint_shadows(o, stops, end);
                }
            }
        });

    stages.mark("negative+matrix");
    if !s.lens.defringe.is_neutral() {
        lens::apply_defringe_in(&mut out, &s.lens.defringe, &cal.lens, &tile);
        stages.mark("defringe");
    }

    // Spot removal, then masks drawn on the photo as it is now, then the
    // local light sliders.
    retouch::apply_in(
        &mut out,
        &s.spots,
        (img.width, img.height),
        crop.as_ref(),
        &tile,
    );
    let base = match (
        img.rendered,
        s.base_look,
        profile.and_then(|p| p.tone_curve.as_ref()),
    ) {
        (true, _, _) => Base::Look(BaseLook::Linear),
        // A profile's tone curve stands in for our default base look.
        (false, BaseLook::Default, Some(curve)) => Base::Curve(curve),
        (false, look, _) => Base::Look(look),
    };
    let masks = (!s.masks.is_empty()).then(|| {
        mask::draw_in(
            &s.masks,
            &out,
            (img.width, img.height),
            crop.as_ref(),
            |y| srgb_encode(base.apply(y)),
            &cal.masks,
            &tile,
            pass.given.map(|g| g.colour_targets.as_slice()),
        )
    });
    if let Some(m) = &masks {
        globals.colour_targets = m.colour_targets.clone();
    }
    if let Some(m) = &masks {
        m.apply_light(&mut out, &cal.masks);
    }
    stages.mark("spots+masks");

    // 2. Dehaze (scene-linear).
    let haze = match pass.given.and_then(|g| g.dehaze.as_ref()) {
        Some(e) => dehaze::Haze::Given(e),
        None => dehaze::Haze::Measure { full },
    };
    globals.dehaze = dehaze::apply_haze(&mut out, s.dehaze, &cal.dehaze, &tile, haze);
    if let Some(m) = &masks {
        let given = pass.given.map(|g| g.mask_dehaze.as_slice());
        globals.mask_dehaze = m.apply_dehaze(&mut out, cal, given, full);
    }
    stages.mark("dehaze");
    if pass.measure_only {
        return (out, globals);
    }

    // 3. Base look, local Highlights/Shadows, then the global tone controls → display-linear.
    let global = ToneControls {
        highlights: 0.0,
        shadows: 0.0,
        ..s.tone
    };
    let tone_lut = (!global.is_neutral()).then(|| tone::tone_lut(&global, &cal.tone, LUT));
    let lum_weights = luminance_weights();
    // A rendered image already has its tone curve (`base` is linear then).
    out.map_pixels(|c| base.apply_rgb(c));
    stages.mark("base look");
    local_tone::apply_scaled(&mut out, &s.tone, &cal.tone, tile.scale());
    stages.mark("highlights/shadows");
    out.map_pixels(|c| {
        let mut p = c;
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
    if let Some(m) = &masks {
        m.apply_tone(&mut out, cal, LUT);
    }
    stages.mark("tone");

    // 4. Texture / Clarity.
    if !s.local.is_neutral() {
        local::apply_scaled(&mut out, &s.local, &cal.local, tile.scale());
    }
    if let Some(m) = &masks {
        m.apply_local_contrast(&mut out, cal);
        m.apply_color(&mut out, &cal.masks);
    }
    stages.mark("texture/clarity");

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

    stages.mark("colour+curves");

    // 6. Detail: noise reduction, then sharpening.
    let (mut nr, mut sharpening) = (s.noise_reduction, s.sharpening);
    if img.rendered {
        if s.unset_detail.color_noise {
            nr.color = 0.0;
        }
        if s.unset_detail.sharpness {
            sharpening.amount = 0.0;
        }
    }
    detail::apply_noise_reduction(&mut out, &nr, &cal.detail);
    stages.mark("noise reduction");
    detail::apply_sharpening(&mut out, &sharpening, &cal.detail);
    if let Some(m) = &masks {
        m.apply_detail(&mut out, cal);
    }
    stages.mark("sharpening");

    // 7. Effects: post-crop vignette, then grain.
    if s.vignette.amount != 0.0 {
        effects::apply_vignette_in(&mut out, &s.vignette, &cal.effects, &tile);
    }
    if s.grain.amount != 0.0 {
        effects::apply_grain_in(&mut out, &s.grain, &cal.effects, &tile);
    }
    if let Some(m) = &masks {
        m.apply_overlay(&mut out);
    }
    if let Some(blank) = lens_pass.as_ref().and_then(|p| p.blank.as_deref()) {
        lens::fill_blank(&mut out, blank, &cal.lens);
    }
    stages.mark("effects");
    (out, globals)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_white_balance_on_rendered_images() {
        let mut img = camera(1, 1, |_, _| [0.5, 0.5, 0.5]);
        img.xyz_to_cam = invert3(&amb_color::matrix::SRGB_TO_XYZ_D50);
        img.rendered = true;
        let none = IncrementalWhiteBalance::default();
        assert_eq!(incremental_neutral(&img, none), [1.0; 3]);
        // + is warmer: the white is assumed bluer, so blue's neutral rises.
        let warm = incremental_neutral(
            &img,
            IncrementalWhiteBalance {
                temperature: 40.0,
                tint: 0.0,
            },
        );
        assert!(warm[2] > 1.05 && warm[0] < 0.97, "{warm:?}");
        // + tint is magenta: green is assumed stronger in the light.
        let magenta = incremental_neutral(
            &img,
            IncrementalWhiteBalance {
                temperature: 0.0,
                tint: 30.0,
            },
        );
        assert!(magenta[0] < 1.0 && magenta[2] < 1.0, "{magenta:?}");
        // The picker's inverse gives back the same setting.
        let wb = IncrementalWhiteBalance {
            temperature: -25.0,
            tint: 12.0,
        };
        let (x, y) = xy_for_neutral(&img.xyz_to_cam, incremental_neutral(&img, wb));
        let back = incremental_for_xy(&img, x, y);
        assert!(
            (back.temperature - wb.temperature).abs() < 0.5 && (back.tint - wb.tint).abs() < 0.5,
            "{back:?}"
        );
        // A rendered image ignores Kelvin and gets no base tone curve.
        let mut s = DevelopSettings {
            white_balance: WhiteBalance::Custom {
                temperature: 3000.0,
                tint: 0.0,
            },
            ..Default::default()
        };
        let out = render(&img, &s);
        s.white_balance = WhiteBalance::AsShot;
        assert_eq!(out.rgb, render(&img, &s).rgb);
        let v = crate::output::to_srgb(&out);
        assert!((v[0] - 0.7354).abs() < 0.01, "{v:?}");
    }

    #[test]
    fn clipped_highlights_go_neutral() {
        let neutral = [0.5, 1.0, 0.6];
        let u = Unclip::new([1.0, 0.98, 0.95], neutral);
        let apply = |c: [f32; 3]| u.apply(c, u.weights(&c));
        // Every channel clipped: the result is the neutral, at the brightest
        // white-balanced level (here red: 1 / 0.5 = 2).
        let c = apply([1.0, 0.98, 0.95]);
        for (v, n) in c.iter().zip(neutral) {
            assert!((v / n as f32 - 2.0).abs() < 1e-5, "{c:?}");
        }
        // Only green clipped: it rises to match the brightest channel.
        let c = apply([0.7, 0.98, 0.3]);
        assert_eq!((c[0], c[2]), (0.7, 0.3));
        assert!((c[1] - 1.4).abs() < 1e-5, "{c:?}");
        // Nothing near clipping: untouched.
        assert_eq!(apply([0.5, 0.8, 0.4]), [0.5, 0.8, 0.4]);

        // Next to a clipped pixel, the edge pixel counts as clipped too.
        let rgb = [
            [0.6, 0.9, 0.5],
            [1.0, 0.98, 0.95],
            [0.2, 0.3, 0.2],
            [0.2, 0.3, 0.2],
        ]
        .concat();
        let mut w = Vec::new();
        u.row_weights(&rgb, 4, 1, 0, 1, &mut w);
        assert_eq!(w[0], [1.0; 3]);
        assert_eq!(w[3], [0.0; 3]);
    }

    #[test]
    fn clip_levels_find_the_pile_up() {
        // A smooth ramp in every channel, plus a blown area where red and
        // green pile up at 1.0 and blue a little lower.
        let mut rgb = Vec::new();
        for i in 0..20_000 {
            let v = i as f32 / 20_000.0 * 0.9;
            rgb.extend_from_slice(&[v, v, v]);
        }
        for _ in 0..2_000 {
            rgb.extend_from_slice(&[1.0, 1.0, 0.95]);
        }
        let k = crate::raw::clip_levels(&rgb);
        assert!(
            (k[0] - 1.0).abs() < 0.01 && (k[1] - 1.0).abs() < 0.01,
            "{k:?}"
        );
        assert!((k[2] - 0.95).abs() < 0.01, "{k:?}");
        // No pile-up: no clip.
        let ramp: Vec<f32> = rgb[..60_000].to_vec();
        assert_eq!(crate::raw::clip_levels(&ramp), [f32::INFINITY; 3]);
    }

    #[test]
    fn camera_profile_colours_and_tone_curve() {
        use crate::profile::{Dcp, tests::sample};
        // The profile's curve lifts scene 0.18 to display 0.5 and stands in
        // for our base look; a grey stays grey.
        let dcp = Dcp::parse(&sample(vec![(
            50940,
            11,
            vec![0.0, 0.0, 0.18, 0.5, 1.0, 1.0],
            None,
        )]))
        .unwrap();
        let img = camera(8, 8, |_, _| [0.18; 3]);
        let mut s = linear(&[("CameraProfile", "Test Standard")]);
        s.base_look = BaseLook::Default;
        assert!(s.profile_note().unwrap().contains("Test Standard"));
        s.camera_profile = Some(std::sync::Arc::new(dcp));
        assert!(s.profile_note().is_none());
        let out = render(&img, &s);
        let p = &out.rgb[..3];
        assert!(p.iter().all(|v| (v - 0.5).abs() < 0.01), "{p:?}");
        // Without it, our base look (scene 0.18 → about 0.27).
        s.camera_profile = None;
        let ours = render(&img, &s).rgb[0];
        assert!((ours - 0.5).abs() > 0.1, "{ours}");
    }

    #[test]
    fn brightening_exposure_goes_before_the_look_table() {
        use crate::profile::{Dcp, tests::sample};
        // A look table that darkens bright values: value scale 1 at V = 0,
        // 0.5 at V = 1.
        let rows = |scale: f64| (0..4).flat_map(move |_| [0.0, 1.0, scale]);
        let table: Vec<f64> = rows(1.0).chain(rows(0.5)).collect();
        let dcp = Dcp::parse(&sample(vec![
            (50981, 4, vec![2.0, 2.0, 2.0], None),
            (50982, 11, table, None),
        ]))
        .unwrap();
        let img = camera(4, 4, |_, _| [0.1, 0.05, 0.02]);
        let mut s = linear(&[("CameraProfile", "Test Standard")]);
        s.camera_profile = Some(std::sync::Arc::new(dcp));
        let at = |s: &mut DevelopSettings, ev: f64| {
            s.exposure_ev = ev;
            render(&img, s).rgb[0]
        };
        let base = at(&mut s, 0.0);
        // +1 EV: the look table sees the brighter value and darkens it more.
        let up = at(&mut s, 1.0);
        assert!(up < 2.0 * base * 0.99, "{up} vs {base}");
        // −1 EV comes after the table: exactly half.
        let down = at(&mut s, -1.0);
        assert!((down - 0.5 * base).abs() < 1e-5, "{down} vs {base}");
    }

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
            clip: [f32::INFINITY; 3],
            rendered: false,
            make: "Test".into(),
            model: "Synthetic".into(),
            shot: Default::default(),
            metadata: Default::default(),
        }
    }

    /// A busy photo: gradients, hard edges, fine texture and a bright patch.
    fn busy(w: usize, h: usize) -> CameraImage {
        textured(w, h, 0.05)
    }

    /// [`busy`] with pixel-level texture of `fine` amplitude.
    fn textured(w: usize, h: usize, fine: f32) -> CameraImage {
        let amplitude = fine;
        camera(w, h, |x, y| {
            let (fx, fy) = (x as f32 / w as f32, y as f32 / h as f32);
            let edge = if (x / 37 + y / 23) % 2 == 0 {
                0.15
            } else {
                0.0
            };
            let fine = ((x * 7 + y * 13) % 11) as f32 / 11.0 * amplitude;
            let sun = if (fx - 0.7).hypot(fy - 0.3) < 0.06 {
                2.0
            } else {
                0.0
            };
            [
                0.05 + 0.3 * fx + edge + fine + sun,
                0.08 + 0.25 * fy + edge * 0.5 + fine + sun,
                0.12 + 0.2 * (1.0 - fx) + fine + sun,
            ]
        })
    }

    /// Settings from `pairs`, with the default colour noise reduction off
    /// unless given (it measures the noise on what it renders).
    fn edit(pairs: &[(&str, &str)], extra: &[(&str, Value)]) -> DevelopSettings {
        let mut m = map(&[("ColorNoiseReduction", "0")]);
        m.extend(map(pairs));
        for (k, v) in extra {
            m.insert(k.to_string(), v.clone());
        }
        let (s, missing) = DevelopSettings::from_settings(&m);
        assert!(missing.is_empty(), "{missing:?}");
        s
    }

    /// Largest channel difference between a region render and the same
    /// rectangle of the full render, and the mean one.
    fn region_error(img: &CameraImage, s: &DevelopSettings, window: Rect) -> (f32, f32) {
        let full = render(img, s);
        // Globals from a copy a quarter the size (the editor's is about
        // 1600 px on a side, against 6000 or so).
        let small = crate::raw::downscale(img, img.width.max(img.height) / 4);
        let globals = measure_globals(img, &small, s);
        let part = render_region(img, s, window, &globals);
        assert_eq!((part.width, part.height), (window.width, window.height));
        let (mut max, mut sum) = (0.0f32, 0.0f64);
        for y in 0..window.height {
            for x in 0..window.width {
                let (a, b) = (part.get(x, y), full.get(window.x + x, window.y + y));
                for c in 0..3 {
                    let d = (a[c] - b[c]).abs();
                    max = max.max(d);
                    sum += d as f64;
                }
            }
        }
        (
            max,
            (sum / (window.width * window.height * 3) as f64) as f32,
        )
    }

    const WINDOW: Rect = Rect {
        x: 230,
        y: 150,
        width: 180,
        height: 120,
    };

    #[test]
    fn region_matches_the_full_render() {
        let img = busy(640, 480);
        let cases: Vec<(&str, DevelopSettings)> = vec![
            ("plain", edit(&[], &[])),
            (
                "geometry",
                edit(
                    &[
                        ("HasCrop", "True"),
                        ("CropLeft", "0.07"),
                        ("CropTop", "0.05"),
                        ("CropRight", "0.95"),
                        ("CropBottom", "0.93"),
                        ("CropAngle", "3.5"),
                        ("PerspectiveVertical", "-20"),
                        ("PerspectiveRotate", "1.5"),
                        ("LensManualDistortionAmount", "30"),
                        ("VignetteAmount", "-40"),
                    ],
                    &[],
                ),
            ),
            (
                "local filters",
                edit(
                    &[
                        ("Texture", "40"),
                        ("Clarity2012", "-35"),
                        ("Highlights2012", "-60"),
                        ("Shadows2012", "70"),
                        ("Sharpness", "80"),
                        ("SharpenRadius", "2"),
                        ("SharpenEdgeMasking", "30"),
                        ("DefringePurpleAmount", "10"),
                    ],
                    &[],
                ),
            ),
            (
                "effects",
                edit(
                    &[
                        ("PostCropVignetteAmount", "-50"),
                        ("PostCropVignetteFeather", "40"),
                        ("GrainAmount", "60"),
                        ("GrainSize", "40"),
                        ("HasCrop", "True"),
                        ("CropLeft", "0.1"),
                        ("CropTop", "0.1"),
                        ("CropRight", "0.9"),
                        ("CropBottom", "0.9"),
                    ],
                    &[],
                ),
            ),
        ];
        for (name, s) in cases {
            let (max, _) = region_error(&img, &s, WINDOW);
            assert!(max < 1e-4, "{name}: {max}");
        }
    }

    #[test]
    fn region_matches_with_masks_and_spots() {
        let img = busy(640, 480);
        let xmp = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
          <rdf:Description rdf:about="" xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/">
          <crs:MaskGroupBasedCorrections><rdf:Seq><rdf:li><rdf:Description
            crs:What="Correction" crs:CorrectionAmount="1" crs:CorrectionActive="true"
            crs:LocalExposure2012="0.25" crs:LocalClarity2012="0.5" crs:LocalSharpness="-0.6">
            <crs:CorrectionMasks><rdf:Seq>
              <rdf:li crs:What="Mask/CircularGradient" crs:MaskBlendMode="0" crs:MaskValue="1"
                crs:Top="0.2" crs:Left="0.3" crs:Bottom="0.7" crs:Right="0.65" crs:Angle="20"
                crs:Feather="50" crs:Flipped="true"/>
            </rdf:Seq></crs:CorrectionMasks>
          </rdf:Description></rdf:li></rdf:Seq></crs:MaskGroupBasedCorrections>
          </rdf:Description></rdf:RDF></x:xmpmeta>"#;
        let mut settings = amb_presets::xmp::parse_crs(xmp).unwrap();
        let area = |centre, r, source| {
            Value::Struct(amb_presets::retouch::new_area(
                amb_presets::retouch::SpotKind::Heal,
                centre,
                r,
                source,
                source.is_none(),
                0.3,
                1.0,
            ))
        };
        settings.insert(
            "RetouchAreas".into(),
            Value::Seq(vec![
                // Inside the window, its donor far outside it.
                area((0.5, 0.42), 0.02, Some((0.12, 0.85))),
                // In the margin, with a searched donor.
                area((0.33, 0.3), 0.015, None),
                // Far away: no effect on the window.
                area((0.9, 0.9), 0.02, None),
            ]),
        );
        settings.extend(map(&[("ColorNoiseReduction", "0")]));
        let (s, missing) = DevelopSettings::from_settings(&settings);
        assert!(missing.is_empty(), "{missing:?}");
        let (max, _) = region_error(&img, &s, WINDOW);
        assert!(max < 1e-4, "{max}");
    }

    #[test]
    fn region_with_measured_globals_is_close() {
        // Dehaze, a film negative, colour range masks and noise reduction
        // measure the whole photo; a region takes them from a small copy or
        // measures its part. A full-size photo is measured over blocks of
        // about 10 pixels, which the small copy's own averaging matches;
        // this photo is too small for that, so it has no pixel-level texture.
        let img = textured(640, 480, 0.0);
        let st = |pairs: &[(&str, Value)]| {
            Value::Struct(
                pairs
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.clone()))
                    .collect(),
            )
        };
        let text = |v: &str| Value::Text(v.into());
        // A colour range sampled outside the window, brightened and dehazed.
        let colour_mask = st(&[
            ("What", text("Correction")),
            ("CorrectionAmount", text("1")),
            ("CorrectionActive", text("true")),
            ("LocalExposure2012", text("0.3")),
            ("LocalDehaze", text("0.4")),
            (
                "CorrectionMasks",
                Value::Seq(vec![st(&[
                    ("What", text("Mask/RangeMask")),
                    ("MaskBlendMode", text("0")),
                    ("MaskValue", text("1")),
                    (
                        "CorrectionRangeMask",
                        st(&[
                            ("Version", text("3")),
                            ("Type", text("1")),
                            ("ColorAmount", text("0.6")),
                            ("amb:ColorSamples", Value::Seq(vec![text("0.2 0.8")])),
                        ]),
                    ),
                ])]),
            ),
        ]);
        let cases = [
            ("dehaze", edit(&[("Dehaze", "40")], &[])),
            (
                "negative",
                edit(
                    &[("amb:NegativeConvert", "True"), ("Exposure2012", "0.5")],
                    &[],
                ),
            ),
            (
                "colour range and local dehaze",
                edit(
                    &[],
                    &[("MaskGroupBasedCorrections", Value::Seq(vec![colour_mask]))],
                ),
            ),
            (
                "noise reduction",
                edit(
                    &[("LuminanceSmoothing", "40"), ("ColorNoiseReduction", "50")],
                    &[],
                ),
            ),
        ];
        for (name, mut s) in cases {
            assert!(needs_globals(&s) || name == "noise reduction", "{name}");
            // As in the editor, the haze is estimated on a grid about the
            // small copy's size (1536 px against 1600 there).
            s.calibration.dehaze.estimate_long_side = 150.0;
            let (max, mean) = region_error(&img, &s, WINDOW);
            assert!(mean < 3e-3 && max < 0.05, "{name}: max {max}, mean {mean}");
        }
    }

    #[test]
    fn region_plan_holds_the_window_and_aligns() {
        let img = busy(640, 480);
        let s = edit(&[("Clarity2012", "50")], &[]);
        let work = region_plan(&img, &s, WINDOW);
        assert!(work.contains(&WINDOW), "{work:?}");
        assert_eq!(work.x % REGION_ALIGN, 0);
        assert_eq!(work.y % REGION_ALIGN, 0);
        assert!(work.right() <= 640 && work.bottom() <= 480);
        assert!(work.width < 640 || work.height < 480, "{work:?}");
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

    #[test]
    fn local_adjustments_render_through_their_masks() {
        // The top of the frame (rows < 10 of 40) gets local Exposure +1 EV,
        // Saturation −100 and Clarity; the bottom stays as the global edit.
        let xmp = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
          <rdf:Description rdf:about="" xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/">
          <crs:MaskGroupBasedCorrections><rdf:Seq><rdf:li><rdf:Description
            crs:What="Correction" crs:CorrectionAmount="1" crs:CorrectionActive="true"
            crs:LocalExposure2012="0.25" crs:LocalSaturation="-1" crs:LocalClarity2012="0.5">
            <crs:CorrectionMasks><rdf:Seq>
              <rdf:li crs:What="Mask/Gradient" crs:MaskBlendMode="0" crs:MaskValue="1"
                crs:ZeroX="0.5" crs:ZeroY="0.3" crs:FullX="0.5" crs:FullY="0.25"/>
            </rdf:Seq></crs:CorrectionMasks>
          </rdf:Description></rdf:li></rdf:Seq></crs:MaskGroupBasedCorrections>
          </rdf:Description></rdf:RDF></x:xmpmeta>"#;
        let settings = amb_presets::xmp::parse_crs(xmp).unwrap();
        let (s, missing) = DevelopSettings::from_settings(&settings);
        assert!(missing.is_empty(), "{missing:?}");
        let s = DevelopSettings {
            base_look: BaseLook::Linear,
            sharpening: Sharpening {
                amount: 0.0,
                ..s.sharpening
            },
            noise_reduction: NoiseReduction {
                color: 0.0,
                ..s.noise_reduction
            },
            ..s
        };
        let img = camera(20, 40, |_, _| [0.10, 0.05, 0.02]);
        let out = render(&img, &s);
        let plain = render(&img, &linear(&[]));
        let (top, bottom) = (out.get(10, 2), out.get(10, 37));
        assert_eq!(bottom, plain.get(10, 37), "outside the mask is untouched");
        let y = |p: [f32; 3]| p.iter().sum::<f32>();
        assert!(y(top) > 1.6 * y(plain.get(10, 2)), "brighter: {top:?}");
        let spread = |p: [f32; 3]| {
            p.iter().fold(0f32, |a, &v| a.max(v)) - p.iter().fold(1f32, |a, &v| a.min(v))
        };
        assert!(spread(top) < 0.02 * y(top), "grey: {top:?}");

        // The overlay tints the mask red and leaves the rest alone.
        let mut shown = s.clone();
        shown.masks.overlay = Some(0);
        let o = render(&img, &shown);
        assert!(o.get(10, 2)[0] > 3.0 * o.get(10, 2)[2]);
        assert_eq!(o.get(10, 37), out.get(10, 37));
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
    fn transform_warps_before_the_crop_and_constrain_crop_fills() {
        // A dark photo: blank areas (filled light) stand out.
        let img = camera(240, 160, |_, _| [0.02; 3]);
        let bright =
            |out: &ProPhotoImage, x: usize, y: usize| out.rgb[(y * out.width + x) * 3] > 0.5;
        let out = render(&img, &linear(&[("PerspectiveScale", "50")]));
        assert_eq!((out.width, out.height), (240, 160));
        assert!(bright(&out, 2, 2) && !bright(&out, 120, 80));
        // Negative Vertical widens the top: blank wedges at the bottom corners.
        let keystone = [("PerspectiveVertical", "-60")];
        let out = render(&img, &linear(&keystone));
        assert!(bright(&out, 1, 158) && !bright(&out, 1, 1));
        let mut constrained = keystone.to_vec();
        constrained.push(("CropConstrainToWarp", "True"));
        let out = render(&img, &linear(&constrained));
        assert!(out.rgb.iter().all(|v| *v < 0.5));
        // A crop is cut from the transformed photo.
        let mut cropped = keystone.to_vec();
        cropped.extend([
            ("HasCrop", "True"),
            ("CropLeft", "0"),
            ("CropTop", "0.5"),
            ("CropRight", "0.5"),
            ("CropBottom", "1"),
        ]);
        let out = render(&img, &linear(&cropped));
        assert_eq!((out.width, out.height), (120, 80));
        assert!(bright(&out, 1, 78));
        // Transform off: nothing happens, Upright is reported as not rendered.
        let (s, missing) = DevelopSettings::from_settings(&map(&[
            ("PerspectiveVertical", "-60"),
            ("EnableTransform", "False"),
        ]));
        assert!(s.transform.is_identity() && missing.is_empty());
        // Upright: Lightroom's analysis tags are not reported (we solve
        // again); Guided is.
        let (s, missing) = DevelopSettings::from_settings(&map(&[
            ("PerspectiveUpright", "3"),
            ("UprightTransform_3", "0.5,0.5"),
        ]));
        assert_eq!(s.transform.upright_mode, UprightMode::Vertical);
        assert!(missing.is_empty(), "{missing:?}");
        let (_, missing) = DevelopSettings::from_settings(&map(&[("PerspectiveUpright", "5")]));
        assert_eq!(missing.len(), 1);
        assert!(missing[0].contains("Upright Guided"), "{missing:?}");
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
        map.insert("HighlightRecovery".to_string(), Value::Text("10".into()));
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
        assert_eq!(missing, vec!["HighlightRecovery".to_string()]);
    }
}
