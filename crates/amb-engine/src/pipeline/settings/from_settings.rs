// SPDX-License-Identifier: GPL-3.0-or-later
//! `DevelopSettings` read from a photo's settings map.

use super::tags::{LEGACY_TONE, is_rendered, with_legacy_tone, without_switched_off};
use super::*;
use super::{
    DevelopSettings, IncrementalWhiteBalance, LookLut, UnsetDetail, WhiteBalance, num, spline_of,
    wheel,
};

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
            film: flag(settings, "amb:GrainFilm"),
        };
        let gl = crate::glow::Glow::default();
        s.glow = crate::glow::Glow {
            halation: num(settings, "amb:Halation").unwrap_or(gl.halation),
            bloom: num(settings, "amb:Bloom").unwrap_or(gl.bloom),
            size: num(settings, "amb:GlowSize").unwrap_or(gl.size),
        };
        s.rolloff = num(settings, "amb:HighlightRolloff").unwrap_or(0.0);
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
            auto: flag(settings, "amb:DefringeAuto"),
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
        auto_density: flag(settings, "amb:NegativeAutoDensity"),
        auto_grade: flag(settings, "amb:NegativeAutoGrade"),
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
