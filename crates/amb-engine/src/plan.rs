// SPDX-License-Identifier: GPL-3.0-or-later
//! The develop pipeline as numbers another backend can run: the GPU preview
//! (`amb-gpu`) takes a [`Plan`] and runs each stage's kernel with these
//! parameters. They are the same values the CPU stages use, computed by the
//! same code, so the two backends can only differ in arithmetic.
//!
//! Stage 0 (Lens Corrections, Transform, crop, a film negative) runs here on
//! the CPU: it comes first, so its result is simply the GPU's input. A later
//! stage without a GPU version yet is a [`Step::Cpu`]: the backend brings the
//! image back, runs the engine's own code on it and carries on. Masks, spots,
//! defringe and camera profiles make [`plan`] return a reason instead, and
//! the caller renders on the CPU.

use crate::buffer::{Image, luminance_weights};
use crate::color::{ColorOp, ColorPlan};
use crate::pipeline::{
    BaseLook, CameraToWorking, DevelopSettings, LUT, Pass, camera_stage, camera_to_working,
    effective_detail,
};
use crate::raw::CameraImage;
use crate::tone::{self, ToneControls};
use crate::{dehaze, detail, effects, local, local_tone};

/// Highlight recovery's numbers (see `Unclip` in the pipeline).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UnclipPlan {
    pub clip: [f32; 3],
    pub gains: [f32; 3],
    /// Neighbourhood radius in pixels.
    pub reach: usize,
}

/// Lookup tables (size [`Plan::lut_size`]) on encoded values.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Curves {
    pub parametric: Option<Vec<f32>>,
    pub rgb: Option<Vec<f32>>,
    pub channels: [Option<Vec<f32>>; 3],
}

impl Curves {
    pub fn is_empty(&self) -> bool {
        self.parametric.is_none() && self.rgb.is_none() && self.channels.iter().all(Option::is_none)
    }
}

/// One step of a develop, in the CPU pipeline's order.
pub enum Step {
    /// Stage 1: camera RGB to scene-linear ProPhoto (white balance, camera
    /// matrix, Calibration panel) and the Exposure gain; always first.
    Matrix {
        m: [[f32; 3]; 3],
        gain: f32,
        unclip: Option<UnclipPlan>,
        /// The Calibration panel's shadow tint: (stops, end).
        shadow_tint: Option<(f32, f32)>,
    },
    /// Scene-linear to display-linear.
    BaseLook(BaseLook),
    /// Highlights and Shadows on the luminance level's guided base.
    HighlightsShadows(Box<local_tone::TonePlan>),
    /// Texture and Clarity.
    Local(Box<local::LocalPlan>),
    Sharpen(Box<detail::SharpenPlan>),
    /// Colour noise reduction: the GPU downscales the image and gathers tile
    /// sums, the CPU runs the low-resolution guided filter
    /// ([`detail::color_nr_low`]) and measures the noise
    /// ([`detail::color_nr_tau`]), the GPU applies it.
    ColorNr(Box<detail::ColorNrPlan>),
    /// Dehaze: the GPU downscales the image by `factor`, the CPU measures
    /// the haze on that copy ([`dehaze::Estimate::from_small`], then
    /// [`dehaze::apply_numbers`]) and the GPU applies it.
    Dehaze {
        amount: f64,
        k: dehaze::DehazeStrengths,
        factor: usize,
    },
    Vignette(Box<effects::VignettePlan>),
    Grain(Box<effects::GrainPlan>),
    /// The global tone curve on luminance (table of [`LUT`] entries).
    ToneCurve {
        lut: Vec<f32>,
        lum_weights: [f32; 3],
    },
    Color(Box<ColorPlan>),
    Curves(Curves),
    /// A stage without a GPU version yet: a backend runs it on the CPU,
    /// with the engine's own code.
    Cpu {
        name: &'static str,
        run: Box<dyn Fn(&mut Image) + Send + Sync>,
    },
}

impl std::fmt::Debug for Step {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Step::Matrix { .. } => "matrix",
            Step::BaseLook(_) => "base look",
            Step::HighlightsShadows(_) => "highlights/shadows",
            Step::Local(_) => "texture/clarity",
            Step::Sharpen(_) => "sharpening",
            Step::ColorNr(_) => "colour noise reduction",
            Step::Dehaze { .. } => "dehaze",
            Step::Vignette(_) => "vignette",
            Step::Grain(_) => "grain",
            Step::ToneCurve { .. } => "tone curve",
            Step::Color(_) => "colour",
            Step::Curves(_) => "curves",
            Step::Cpu { name, .. } => name,
        };
        f.write_str(name)
    }
}

/// Everything a backend needs to develop one image.
#[derive(Debug)]
pub struct Plan {
    pub width: usize,
    pub height: usize,
    /// Camera RGB after stage 0, interleaved.
    pub camera_rgb: Vec<f32>,
    pub steps: Vec<Step>,
    /// Entries in every lookup table.
    pub lut_size: usize,
}

/// The plan for developing `img` with `s`, or why it has to be developed
/// on the CPU (`pipeline::render`) instead.
pub fn plan(img: &CameraImage, s: &DevelopSettings) -> Result<Plan, String> {
    let cpu_only = |what: &str| Err(format!("{what} runs on the CPU only"));
    // These keep state across several stages (masks are drawn once and
    // used in five places), so they take the whole render to the CPU.
    if s.camera_profile.is_some() && !img.rendered && !s.negative.enabled {
        return cpu_only("a camera profile");
    }
    if !s.masks.is_empty() {
        return cpu_only("masks");
    }
    if !s.spots.is_empty() {
        return cpu_only("spot removal");
    }
    if !s.lens.defringe.is_neutral() {
        return cpu_only("defringe");
    }

    let stage0 = camera_stage(img, s, &Pass::default());
    if stage0.lens_pass.as_ref().is_some_and(|p| p.blank.is_some()) {
        return cpu_only("filling the edges a lens correction leaves");
    }
    let camera_rgb = stage0.rgb(img).to_vec();
    let tile = stage0.tile;
    let full = stage0.full;
    let CameraToWorking {
        m,
        gain,
        shadow_tint,
        clipped,
        unclip_reach,
        ..
    } = camera_to_working(img, s, stage0.out);
    let cal = std::sync::Arc::new(s.calibration.clone());
    let mut steps = vec![Step::Matrix {
        m,
        gain,
        unclip: clipped.map(|u| UnclipPlan {
            clip: u.clip,
            gains: u.gains,
            reach: unclip_reach,
        }),
        shadow_tint,
    }];
    let cpu =
        |name: &'static str, run: Box<dyn Fn(&mut Image) + Send + Sync>| Step::Cpu { name, run };

    // 2. Dehaze (scene-linear).
    // The whole output is rendered here, so the haze is measured on it.
    if s.dehaze != 0.0 && s.dehaze.is_finite() {
        steps.push(Step::Dehaze {
            amount: s.dehaze,
            k: cal.dehaze,
            factor: dehaze::estimate_factor(full, &cal.dehaze),
        });
    }
    // 3. Base look, local Highlights/Shadows, the global tone curve.
    // A rendered image already has its tone curve.
    steps.push(Step::BaseLook(if img.rendered {
        BaseLook::Linear
    } else {
        s.base_look
    }));
    let (w, h) = (stage0.width, stage0.height);
    if let Some(p) = local_tone::plan(&s.tone, &cal.tone, tile.scale(), w, h) {
        steps.push(Step::HighlightsShadows(Box::new(p)));
    }
    let global = ToneControls {
        highlights: 0.0,
        shadows: 0.0,
        ..s.tone
    };
    if !global.is_neutral() {
        steps.push(Step::ToneCurve {
            lut: tone::tone_lut(&global, &cal.tone, LUT),
            lum_weights: luminance_weights(),
        });
    }
    // 4. Texture / Clarity.
    if let Some(p) = local::plan(&s.local, &cal.local, tile.scale(), w, h) {
        steps.push(Step::Local(Box::new(p)));
    }
    // 5. Colour, then the curves.
    if !s.color.is_neutral() {
        steps.push(Step::Color(Box::new(
            ColorOp::new(&s.color, cal.color).plan(),
        )));
    }
    let curves = Curves {
        parametric: (!s.parametric.is_neutral())
            .then(|| tone::parametric_lut(&s.parametric, &cal.tone, LUT)),
        rgb: s.curve_rgb.as_ref().map(|c| c.lut(LUT)),
        channels: std::array::from_fn(|i| s.curve_channels[i].as_ref().map(|c| c.lut(LUT))),
    };
    if !curves.is_empty() {
        steps.push(Step::Curves(curves));
    }
    // 6. Detail: noise reduction, then sharpening.
    let (nr, sharpening) = effective_detail(img, s);
    if nr.luminance > 0.0 && nr.luminance.is_finite() {
        let k = cal.clone();
        let luminance_only = detail::NoiseReduction { color: 0.0, ..nr };
        steps.push(cpu(
            "luminance noise reduction",
            Box::new(move |img| detail::apply_noise_reduction(img, &luminance_only, &k.detail)),
        ));
    }
    if let Some(p) = detail::color_nr_plan(&nr, &cal.detail, w, h) {
        steps.push(Step::ColorNr(Box::new(p)));
    }
    if let Some(p) = detail::sharpen_plan(&sharpening, &cal.detail) {
        steps.push(Step::Sharpen(Box::new(p)));
    }
    // 7. Effects: post-crop vignette, then grain.
    if let Some(p) = effects::vignette_plan(
        &s.vignette,
        &cal.effects,
        (tile.full_width, tile.full_height),
    ) {
        steps.push(Step::Vignette(Box::new(p)));
    }
    if let Some(p) =
        effects::grain_plan(&s.grain, &cal.effects, (tile.full_width, tile.full_height))
    {
        steps.push(Step::Grain(Box::new(p)));
    }
    Ok(Plan {
        width: stage0.width,
        height: stage0.height,
        camera_rgb,
        steps,
        lut_size: LUT,
    })
}
