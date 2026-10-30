// SPDX-License-Identifier: GPL-3.0-or-later
//! From the edit's settings (`crs:` and `amb:` tags) to [`DevelopSettings`]:
//! which tags render, their defaults, panel switches and legacy tone.

use super::*;

#[derive(Debug, Clone, PartialEq)]
pub enum WhiteBalance {
    AsShot,
    /// A Kelvin temperature and a tint; with no tint set, the photo's
    /// as-shot tint (a script that only sets Temperature keeps it).
    Custom {
        temperature: f64,
        tint: Option<f64>,
    },
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
    pub(crate) fn apply(self, x: f32) -> f32 {
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
pub(super) enum Base<'a> {
    Look(BaseLook),
    Curve(&'a crate::profile::ToneCurve),
}

impl Base<'_> {
    pub(super) fn apply(&self, x: f32) -> f32 {
        match self {
            Base::Look(l) => l.apply(x),
            Base::Curve(c) => c.eval(x),
        }
    }

    pub(super) fn apply_rgb(&self, c: [f32; 3]) -> [f32; 3] {
        match self {
            Base::Look(l) => c.map(|v| l.apply(v)),
            // The DNG way: hue-preserving.
            Base::Curve(curve) => curve.apply_rgb(c),
        }
    }
}

/// A `.cube` LUT and how much of it (`amb:LUTAmount`, 0..1).
#[derive(Debug, Clone)]
pub struct LookLut {
    pub table: std::sync::Arc<crate::lut::CubeLut>,
    pub amount: f64,
}

#[derive(Debug, Clone)]
pub struct DevelopSettings {
    pub base_look: BaseLook,
    pub white_balance: WhiteBalance,
    pub incremental_wb: IncrementalWhiteBalance,
    /// A camera-style white balance shift (red, blue; -9..9 steps) on top of
    /// the white balance: `amb:WBShiftRed`/`Blue`.
    pub wb_shift: [f64; 2],
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
    /// A `.cube` look (`amb:LUT`), after the colour and curves.
    pub lut: Option<LookLut>,
    /// Effect plugins (`amb:Effects`), in order, after the LUT.
    pub effects: Vec<crate::plugin_effects::ActiveEffect>,
    /// Model constants; [`Calibration::default`] until fitted against Lightroom.
    pub calibration: Calibration,
}

impl Default for DevelopSettings {
    fn default() -> Self {
        DevelopSettings {
            base_look: BaseLook::Default,
            white_balance: WhiteBalance::AsShot,
            incremental_wb: IncrementalWhiteBalance::default(),
            wb_shift: [0.0; 2],
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
            lut: None,
            effects: Vec::new(),
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

pub(super) fn num(settings: &BTreeMap<String, Value>, key: &str) -> Option<f64> {
    settings.get(key).and_then(Value::as_f64)
}

pub(super) fn wheel(settings: &BTreeMap<String, Value>, region: &str) -> Wheel {
    Wheel {
        hue: num(settings, &format!("ColorGrade{region}Hue")).unwrap_or(0.0),
        sat: num(settings, &format!("ColorGrade{region}Sat")).unwrap_or(0.0),
        lum: num(settings, &format!("ColorGrade{region}Lum")).unwrap_or(0.0),
    }
}

pub(super) fn spline_of(settings: &BTreeMap<String, Value>, key: &str) -> Option<Spline> {
    let points = curve_points(settings.get(key)?)?;
    Spline::from_points_255(&points).filter(|s| !s.is_identity())
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

impl DevelopSettings {
    /// Build from Camera Raw settings. Returns the settings plus the names of
    /// present tags this pipeline does not render yet.
    pub fn from_settings(settings: &BTreeMap<String, Value>) -> (DevelopSettings, Vec<String>) {
        let switched = without_switched_off(settings);
        let settings = switched.as_ref().unwrap_or(settings);
        let legacy = with_legacy_tone(settings, &Calibration::default().legacy_tone);
        // A current edit doesn't use the old sliders (nor does Lightroom).
        let unused_legacy: Vec<String> = match legacy {
            Some(_) => Vec::new(),
            None => LEGACY_TONE
                .iter()
                .filter(|t| {
                    let d = amb_presets::registry::Registry::builtin().numeric_default(t);
                    matches!((num(settings, t), d), (Some(v), Some(d)) if (v - d).abs() > 1e-9)
                })
                .map(|t| format!("{t} (an old-process slider; this edit is process 2012 or later, which doesn't use it)"))
                .collect(),
        };
        let settings = legacy.as_ref().unwrap_or(settings);
        let mut s = DevelopSettings::default();
        let wb_mode = settings
            .get("WhiteBalance")
            .and_then(Value::as_str)
            .unwrap_or("As Shot");
        let temperature = settings.get("Temperature").and_then(Value::as_f64);
        if wb_mode != "As Shot" {
            if let Some(temperature) = temperature {
                let tint = settings.get("Tint").and_then(Value::as_f64);
                s.white_balance = WhiteBalance::Custom { temperature, tint };
            }
        }
        s.wb_shift = [
            num(settings, "amb:WBShiftRed").unwrap_or(0.0),
            num(settings, "amb:WBShiftBlue").unwrap_or(0.0),
        ];
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
        not_rendered.extend(unused_legacy);
        if let Some(path) = text(settings, "amb:LUT") {
            let amount = num(settings, "amb:LUTAmount")
                .unwrap_or(100.0)
                .clamp(0.0, 100.0)
                / 100.0;
            match crate::lut::load(std::path::Path::new(&path)) {
                Ok(table) if amount > 0.0 => s.lut = Some(LookLut { table, amount }),
                Ok(_) => {}
                Err(e) => not_rendered.push(format!("amb:LUT ({e})")),
            }
        }
        if let Some(effects) = text(settings, "amb:Effects") {
            let (active, notes) = crate::plugin_effects::active(&effects);
            s.effects = active;
            not_rendered.extend(notes);
        }
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

pub(super) fn flag(settings: &BTreeMap<String, Value>, key: &str) -> bool {
    settings.get(key).and_then(Value::as_bool).unwrap_or(false)
}

pub(super) fn text(settings: &BTreeMap<String, Value>, key: &str) -> Option<String> {
    settings
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|t| !t.is_empty())
}

pub(super) fn lens_corrections(settings: &BTreeMap<String, Value>) -> LensCorrections {
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
        embedded: None,
    }
}

pub(super) fn transform_settings(settings: &BTreeMap<String, Value>) -> Transform {
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

pub(super) fn calibration_settings(settings: &BTreeMap<String, Value>) -> CameraCalibration {
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

pub(super) fn crop_settings(settings: &BTreeMap<String, Value>) -> Crop {
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

pub(super) fn rgb_triple(settings: &BTreeMap<String, Value>, prefix: &str) -> Option<[f64; 3]> {
    let r = num(settings, &format!("{prefix}Red"))?;
    let g = num(settings, &format!("{prefix}Green"))?;
    let b = num(settings, &format!("{prefix}Blue"))?;
    Some([r, g, b])
}

/// Expired film's stored crossover: `amb:NegativeCrossoverRed` and `Blue`,
/// each three numbers (at a shadow, a midtone and a highlight level of green).
fn crossover_tags(settings: &BTreeMap<String, Value>) -> Option<negative::Crossover> {
    let row = |name: &str| -> Option<[f64; 3]> {
        let t = text(settings, name)?;
        let v: Vec<f64> = t
            .split([' ', ','])
            .filter(|x| !x.is_empty())
            .filter_map(|x| x.parse().ok())
            .collect();
        (v.len() == 3).then(|| [v[0], v[1], v[2]])
    };
    let (red, blue) = (
        row("amb:NegativeCrossoverRed"),
        row("amb:NegativeCrossoverBlue"),
    );
    (red.is_some() || blue.is_some())
        .then(|| [red.unwrap_or([0.0; 3]), [0.0; 3], blue.unwrap_or([0.0; 3])])
}

pub(super) fn negative_settings(settings: &BTreeMap<String, Value>) -> Negative {
    let d = Negative::default();
    Negative {
        enabled: flag(settings, "amb:NegativeConvert"),
        kind: match settings.get("amb:NegativeType").and_then(Value::as_str) {
            Some(t)
                if t.eq_ignore_ascii_case("bw") || t.eq_ignore_ascii_case("black-and-white") =>
            {
                NegativeType::BlackAndWhite
            }
            Some(t) if t.eq_ignore_ascii_case("positive") || t.eq_ignore_ascii_case("slide") => {
                NegativeType::Positive
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
        range: num(settings, "amb:NegativeRange").unwrap_or(d.range),
        expired: flag(settings, "amb:NegativeExpired"),
        crossover: crossover_tags(settings),
        cast_removal: num(settings, "amb:NegativeCastRemoval").unwrap_or(d.cast_removal),
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
