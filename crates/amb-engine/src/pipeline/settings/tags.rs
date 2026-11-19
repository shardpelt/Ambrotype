// SPDX-License-Identifier: GPL-3.0-or-later
//! Which settings tags the pipeline renders, and panel switches that turn groups off.

use super::num;
use super::*;

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
    "amb:DefringeAuto",
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
    "EnableToneCurve",
    "EnableColorAdjustments",
    "EnableGrayscaleMix",
    "EnableSplitToning",
    "EnableDetail",
    "EnableLensCorrections",
    "EnableEffects",
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
    "Exposure",
    "Brightness",
    "Contrast",
    "Shadows",
    "HighlightRecovery",
    "FillLight",
    "Clarity",
    "amb:NegativeConvert",
    "amb:WBShiftRed",
    "amb:WBShiftBlue",
    "amb:LUT",
    "amb:LUTAmount",
    "amb:GrainFilm",
    "amb:HighlightRolloff",
    "amb:Halation",
    "amb:Bloom",
    "amb:GlowSize",
    "amb:Effects",
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
    "amb:NegativeRange",
    "amb:NegativeExpired",
    "amb:NegativeCastRemoval",
    "amb:NegativeAutoDensity",
    "amb:NegativeAutoGrade",
    "amb:NegativeMeterLeft",
    "amb:NegativeMeterTop",
    "amb:NegativeMeterRight",
    "amb:NegativeMeterBottom",
    "amb:NegativeCrossoverRed",
    "amb:NegativeCrossoverBlue",
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

/// Lightroom's panel switches (`crs:Enable*`) and the registry groups each
/// one turns off. The names come from preset corpora; which settings each
/// covers is our reading. Transform, Calibration, masks and spots read their
/// own switches where they are read.
pub(super) const PANEL_SWITCHES: &[(&str, &[&str])] = &[
    ("EnableToneCurve", &["tone_curve"]),
    ("EnableColorAdjustments", &["hsl", "point_color"]),
    ("EnableGrayscaleMix", &["bw_mixer"]),
    ("EnableSplitToning", &["split_toning", "color_grading"]),
    ("EnableDetail", &["detail"]),
    ("EnableLensCorrections", &["lens"]),
    ("EnableEffects", &["effects"]),
];

/// `settings` without the panels that are switched off, or `None` when all
/// are on. A panel off renders as if at its defaults, except Detail, whose
/// sharpening and noise reduction go to 0, and the black and white treatment
/// (`ConvertToGrayscale`, a Basic panel choice), which stays.
pub(super) fn without_switched_off(
    settings: &BTreeMap<String, Value>,
) -> Option<BTreeMap<String, Value>> {
    let off: Vec<&str> = PANEL_SWITCHES
        .iter()
        .filter(|(switch, _)| settings.get(*switch).and_then(Value::as_bool) == Some(false))
        .flat_map(|(_, groups)| groups.iter().copied())
        .collect();
    if off.is_empty() {
        return None;
    }
    let registry = amb_presets::registry::Registry::builtin();
    let mut kept: BTreeMap<String, Value> = settings
        .iter()
        .filter(|(k, _)| {
            k.as_str() == "ConvertToGrayscale"
                || !registry
                    .lookup(k)
                    .is_some_and(|t| off.contains(&t.group.as_str()))
        })
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    if off.contains(&"detail") {
        for k in ["Sharpness", "LuminanceSmoothing", "ColorNoiseReduction"] {
            kept.insert(k.into(), Value::Number(0.0));
        }
    }
    Some(kept)
}

/// The old process versions' Basic sliders.
pub(super) const LEGACY_TONE: [&str; 7] = [
    "Exposure",
    "Brightness",
    "Contrast",
    "Shadows",
    "HighlightRecovery",
    "FillLight",
    "Clarity",
];

/// An edit from process version 2010 or 2003 (`ProcessVersion` below 6.7,
/// or old tone sliders with no 2012 ones): its Basic sliders read as
/// today's, where the 2012 setting isn't there ([`LegacyToneStrengths`]).
/// `None` for a current edit.
pub(super) fn with_legacy_tone(
    settings: &BTreeMap<String, Value>,
    k: &crate::tone::LegacyToneStrengths,
) -> Option<BTreeMap<String, Value>> {
    let has = |key: &str| settings.contains_key(key);
    if !LEGACY_TONE.iter().any(|t| has(t)) {
        return None;
    }
    let old_process = match settings.get("ProcessVersion").and_then(Value::as_f64) {
        Some(pv) => pv < 6.7,
        None => !settings.keys().any(|k| k.ends_with("2012")),
    };
    if !old_process {
        return None;
    }
    let n = |key: &str, default: f64| num(settings, key).unwrap_or(default);
    let mut out = settings.clone();
    let mut put = |key: &str, v: f64, lo: f64, hi: f64| {
        if !has(key) && v != 0.0 {
            out.insert(key.into(), Value::Number(v.clamp(lo, hi)));
        }
    };
    let ev = n("Exposure", 0.0) + (n("Brightness", 50.0) - 50.0) * k.brightness_ev;
    put("Exposure2012", ev, -5.0, 5.0);
    put(
        "Contrast2012",
        (n("Contrast", 25.0) - 25.0) * k.contrast,
        -100.0,
        100.0,
    );
    put(
        "Blacks2012",
        (n("Shadows", 5.0) - 5.0) * k.blacks,
        -100.0,
        100.0,
    );
    put(
        "Highlights2012",
        n("HighlightRecovery", 0.0) * k.recovery,
        -100.0,
        100.0,
    );
    put("Shadows2012", n("FillLight", 0.0) * k.fill, -100.0, 100.0);
    put("Clarity2012", n("Clarity", 0.0) * k.clarity, -100.0, 100.0);
    Some(out)
}
