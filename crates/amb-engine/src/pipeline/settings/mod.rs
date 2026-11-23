// SPDX-License-Identifier: GPL-3.0-or-later
//! From the edit's settings (`crs:` and `amb:` tags) to [`DevelopSettings`]:
//! which tags render, their defaults, panel switches and legacy tone.

use super::*;

mod from_settings;
mod tags;

pub use tags::{RENDERED_TAGS, is_rendered};

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
    /// Halation and bloom (`amb:Halation`, `amb:Bloom`, `amb:GlowSize`).
    pub glow: crate::glow::Glow,
    /// `amb:HighlightRolloff`, 0..100 ([`crate::rolloff`]).
    pub rolloff: f64,
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
            glow: crate::glow::Glow::default(),
            rolloff: 0.0,
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
    /// Whether a film negative is printed on the paper curve: the result is
    /// then a print's reflectance (display-referred), so it gets no base look.
    pub fn paper_print(&self) -> bool {
        self.negative.enabled && self.negative.paper.enabled
    }

    /// The base look for a photo (`rendered`: a JPEG or the like, which has
    /// its tone curve already): linear for those and for paper prints.
    pub(crate) fn base_look_for(&self, rendered: bool) -> BaseLook {
        if rendered || self.paper_print() {
            BaseLook::Linear
        } else {
            self.base_look
        }
    }
}
