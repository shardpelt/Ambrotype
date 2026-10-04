//! Develop pipeline v0.
//!
//! Order (subset of the Camera Raw model, docs/research/05 §1):
//! camera RGB → white balance + camera matrix → XYZ → Bradford to D50 →
//! linear ProPhoto → Exposure (2^EV) → point curves (RGB, then R/G/B) on
//! encoded values → output.
//!
//! Only a few settings render so far; [`DevelopSettings::from_settings`]
//! reports everything else as not rendered yet.

use std::collections::BTreeMap;

use flb_color::matrix::{D50_XYZ, Mat3, PROPHOTO_TO_XYZ_D50, apply3, bradford, invert3, mul3};
use flb_color::temperature::{temp_tint_to_xy, xy_to_temp_tint};
use flb_presets::{Value, curve_points};
use rayon::prelude::*;

use crate::curve::Spline;
use crate::raw::{CameraImage, neutral_for_xy, xy_for_neutral};

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
    pub curve_rgb: Option<Spline>,
    pub curve_channels: [Option<Spline>; 3],
}

impl Default for DevelopSettings {
    fn default() -> Self {
        DevelopSettings {
            base_look: BaseLook::Default,
            white_balance: WhiteBalance::AsShot,
            exposure_ev: 0.0,
            curve_rgb: None,
            curve_channels: [None, None, None],
        }
    }
}

/// Settings that this pipeline version renders.
pub const RENDERED_TAGS: &[&str] = &[
    "WhiteBalance",
    "Temperature",
    "Tint",
    "Exposure2012",
    "ToneCurvePV2012",
    "ToneCurvePV2012Red",
    "ToneCurvePV2012Green",
    "ToneCurvePV2012Blue",
];

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
        if let Some(ev) = settings.get("Exposure2012").and_then(Value::as_f64) {
            s.exposure_ev = ev;
        }
        s.curve_rgb = spline_of(settings, "ToneCurvePV2012");
        s.curve_channels = [
            spline_of(settings, "ToneCurvePV2012Red"),
            spline_of(settings, "ToneCurvePV2012Green"),
            spline_of(settings, "ToneCurvePV2012Blue"),
        ];
        let registry = flb_presets::registry::Registry::builtin();
        let not_rendered = settings
            .keys()
            .filter(|k| !RENDERED_TAGS.contains(&k.as_str()))
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

/// Linear ProPhoto (D50) image produced by the pipeline.
pub struct ProPhotoImage {
    pub width: usize,
    pub height: usize,
    pub rgb: Vec<f32>,
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

pub fn render(img: &CameraImage, s: &DevelopSettings) -> ProPhotoImage {
    let neutral = match s.white_balance {
        WhiteBalance::AsShot => img.as_shot_neutral,
        WhiteBalance::Custom { temperature, tint } => {
            let (x, y) = temp_tint_to_xy(temperature, tint);
            neutral_for_xy(&img.xyz_to_cam, x, y)
        }
    };
    let m = camera_to_prophoto(&img.xyz_to_cam, neutral).map(|row| row.map(|v| v as f32));
    let gain = 2f32.powf(s.exposure_ev as f32);
    const LUT: usize = 4096;
    let rgb_lut = s.curve_rgb.as_ref().map(|c| c.lut(LUT));
    let channel_luts: Vec<Option<Vec<f32>>> = s
        .curve_channels
        .iter()
        .map(|c| c.as_ref().map(|c| c.lut(LUT)))
        .collect();
    let has_curves = rgb_lut.is_some() || channel_luts.iter().any(Option::is_some);
    let base = s.base_look;
    let lookup = |lut: &[f32], v: f32| {
        let pos = v.clamp(0.0, 1.0) * (LUT - 1) as f32;
        let i = (pos as usize).min(LUT - 2);
        let f = pos - i as f32;
        lut[i] * (1.0 - f) + lut[i + 1] * f
    };

    let mut out = vec![0.0f32; img.rgb.len()];
    out.par_chunks_mut(3)
        .zip(img.rgb.par_chunks(3))
        .for_each(|(o, c)| {
            let mut p = [0.0f32; 3];
            for (row, slot) in m.iter().zip(p.iter_mut()) {
                *slot = base.apply((row[0] * c[0] + row[1] * c[1] + row[2] * c[2]) * gain);
            }
            if has_curves {
                // Curves act on encoded values (sRGB transfer for now; to be calibrated, see docs/research/05 §8).
                for (ch, v) in p.iter_mut().enumerate() {
                    let mut e = srgb_encode(v.clamp(0.0, 1.0));
                    if let Some(lut) = &rgb_lut {
                        e = lookup(lut, e);
                    }
                    if let Some(lut) = &channel_luts[ch] {
                        e = lookup(lut, e);
                    }
                    *v = srgb_decode(e);
                }
            }
            o.copy_from_slice(&p);
        });
    ProPhotoImage {
        width: img.width,
        height: img.height,
        rgb: out,
    }
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

    #[test]
    fn settings_parse_and_report() {
        let mut map = BTreeMap::new();
        map.insert("WhiteBalance".to_string(), Value::Text("Custom".into()));
        map.insert("Temperature".to_string(), Value::Text("5500".into()));
        map.insert("Tint".to_string(), Value::Text("+10".into()));
        map.insert("Exposure2012".to_string(), Value::Text("+0.5".into()));
        map.insert("Clarity2012".to_string(), Value::Text("+20".into()));
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
        assert_eq!(missing, vec!["Clarity2012".to_string()]);
    }
}
