// SPDX-License-Identifier: GPL-3.0-or-later
//! The develop pipeline as numbers another backend can run: the GPU preview
//! (`amb-gpu`) takes a [`Plan`] and runs each stage's kernel with these
//! parameters. They are the same values the CPU stages use, computed by the
//! same code, so the two backends can only differ in arithmetic.
//!
//! Stage 0 (Lens Corrections, Transform, crop, a film negative) runs here on
//! the CPU: it comes first, so its result is simply the GPU's input. Every
//! later stage has a GPU version; one without would be a [`Step::Cpu`]: the
//! backend brings the image back, runs the engine's own code on it and
//! carries on (a camera profile's tables and tone curve, defringe, spot
//! removal, masks, a LUT and filling the edges a lens correction leaves do
//! that).

use crate::buffer::{Image, luminance_weights};
use crate::color::{ColorOp, ColorPlan};
use crate::mask::{self, LocalAdjustments};
use crate::pipeline::{
    BaseLook, CameraToWorking, DevelopSettings, LUT, Pass, camera_stage, camera_to_working,
    effective_detail,
};
use crate::raw::CameraImage;
use crate::tone::{self, ToneControls};
use crate::{dehaze, detail, effects, local, local_tone};
use rayon::prelude::*;

/// Highlight recovery's numbers (see `Unclip` in the pipeline).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UnclipPlan {
    pub clip: [f32; 3],
    pub gains: [f32; 3],
    /// Neighbourhood radius in pixels.
    pub reach: usize,
    /// Only where every channel clipped (`Unclip::full_only`).
    pub full_only: bool,
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
    /// Luminance noise reduction: the GPU gathers the noise tiles of the
    /// encoded luminance, the CPU turns them into tables
    /// (`detail::luminance_nr_tables`), and the GPU filters.
    LuminanceNr(Box<detail::LuminanceNrPlan>),
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
            Step::LuminanceNr(_) => "luminance noise reduction",
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
/// on the CPU (`pipeline::render`) instead (every edit has a plan today).
pub fn plan(img: &CameraImage, s: &DevelopSettings) -> Result<Plan, String> {
    let stage0 = camera_stage(img, s, &Pass::default());
    let camera_rgb = stage0.rgb(img).to_vec();
    let tile = stage0.tile;
    let full = stage0.full;
    let CameraToWorking {
        profile,
        m,
        gain,
        hue_sat_map,
        look_table,
        gain_before_look,
        gain_after_look,
        shadow_tint,
        clipped,
        unclip_reach,
    } = camera_to_working(img, s, stage0.out);
    let cal = std::sync::Arc::new(s.calibration.clone());
    let unclip = clipped.map(|u| UnclipPlan {
        clip: u.clip,
        gains: u.gains,
        reach: unclip_reach,
        full_only: u.full_only,
    });
    let mut steps = Vec::new();
    if profile.is_some() {
        // A camera profile: its matrix on the GPU, then its tables with the
        // Exposure gain around the look table, as the CPU does it.
        steps.push(Step::Matrix {
            m,
            gain: 1.0,
            unclip,
            shadow_tint: None,
        });
        let hue_sat_map = hue_sat_map.map(std::borrow::Cow::into_owned);
        let look_table = look_table.cloned();
        steps.push(Step::Cpu {
            name: "camera profile",
            run: Box::new(move |img| {
                img.rgb.par_chunks_mut(3).for_each(|o| {
                    if let Some(t) = &hue_sat_map {
                        t.apply(o);
                    }
                    o.iter_mut().for_each(|v| *v *= gain_before_look);
                    if let Some(t) = &look_table {
                        t.apply(o);
                    }
                    o.iter_mut().for_each(|v| *v *= gain_after_look);
                    if let Some((stops, end)) = shadow_tint {
                        crate::primaries::tint_shadows(o, stops, end);
                    }
                })
            }),
        });
    } else {
        steps.push(Step::Matrix {
            m,
            gain,
            unclip,
            shadow_tint,
        });
    }
    // The base look, or the profile's tone curve standing in for the
    // default one.
    let look = if img.rendered {
        BaseLook::Linear
    } else {
        s.base_look
    };
    let curve = profile
        .filter(|_| look == BaseLook::Default)
        .and_then(|p| p.tone_curve.clone())
        .map(std::sync::Arc::new);
    // Spot removal, masks and Defringe on the scene-linear photo as the CPU
    // does them, with the engine's own code.
    if !s.spots.is_empty() {
        let spots = s.spots.clone();
        let frame = (img.width, img.height);
        let crop = s.crop.geometry(img.width, img.height);
        steps.push(Step::Cpu {
            name: "spot removal",
            run: Box::new(move |out| {
                let tile = crate::buffer::Tile::whole(out.width, out.height);
                crate::retouch::apply_in(out, &spots, frame, crop.as_ref(), &tile);
            }),
        });
    }

    // Masks are drawn once, here, on the photo as it is after spot removal,
    // and applied with the engine's own code wherever the CPU applies them.
    // Defringe comes next (masks can hold it back or add to it).
    let drawn = Shared::default();
    let any = |f: fn(&LocalAdjustments) -> bool| s.masks.corrections.iter().any(|c| f(&c.adjust));
    if !s.masks.is_empty() {
        let masks = s.masks.clone();
        let frame = (img.width, img.height);
        let crop = s.crop.geometry(img.width, img.height);
        let (d, cal, curve) = (drawn.clone(), cal.clone(), curve.clone());
        let defringe = s.lens.defringe;
        steps.push(Step::Cpu {
            name: "masks",
            run: Box::new(move |out| {
                let base = |y| match &curve {
                    Some(c) => c.eval(y),
                    None => look.apply(y),
                };
                let encode = |y| local::srgb_encode(base(y));
                let m = mask::draw(&masks, out, frame, crop.as_ref(), encode, &cal.masks);
                m.apply_defringe(out, &defringe, &cal);
                m.apply_light(out, &cal.masks);
                *d.lock().expect("masks") = Some(m);
            }),
        });
    } else if !s.lens.defringe.is_neutral() {
        let (d, k) = (s.lens.defringe, cal.lens);
        steps.push(Step::Cpu {
            name: "defringe",
            run: Box::new(move |out| crate::lens::apply_defringe_in(out, &d, &k, &tile)),
        });
    }

    // 2. Dehaze (scene-linear).
    // The whole output is rendered here, so the haze is measured on it.
    if s.dehaze != 0.0 && s.dehaze.is_finite() {
        steps.push(Step::Dehaze {
            amount: s.dehaze,
            k: cal.dehaze,
            factor: dehaze::estimate_factor(full, &cal.dehaze),
        });
    }
    if any(|a| a.dehaze != 0.0) {
        let cal = cal.clone();
        steps.push(masked(&drawn, "mask dehaze", move |m, img| {
            m.apply_dehaze(img, &cal, None, full);
        }));
    }
    // 3. Base look, local Highlights/Shadows, the global tone curve.
    // A rendered image already has its tone curve.
    match curve {
        Some(c) => steps.push(Step::Cpu {
            name: "profile tone curve",
            run: Box::new(move |img| {
                img.rgb.par_chunks_mut(3).for_each(|p| {
                    let c = c.apply_rgb([p[0], p[1], p[2]]);
                    p.copy_from_slice(&c);
                })
            }),
        }),
        None => steps.push(Step::BaseLook(look)),
    }
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
    if any(|a| !a.tone().is_neutral()) {
        let cal = cal.clone();
        steps.push(masked(&drawn, "mask tone", move |m, img| {
            m.apply_tone(img, &cal, LUT)
        }));
    }
    // 4. Texture / Clarity.
    if let Some(p) = local::plan(&s.local, &cal.local, tile.scale(), w, h) {
        steps.push(Step::Local(Box::new(p)));
    }
    if any(|a| a.texture != 0.0 || a.clarity != 0.0)
        || any(|a| a.saturation != 0.0 || a.toning_saturation != 0.0)
    {
        let cal = cal.clone();
        steps.push(masked(&drawn, "mask texture/colour", move |m, img| {
            m.apply_local_contrast(img, &cal);
            m.apply_color(img, &cal.masks);
        }));
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
    if s.masks.corrections.iter().any(|c| c.curves.is_some()) {
        steps.push(masked(&drawn, "mask curves", |m, img| m.apply_curves(img)));
    }
    // The LUT, with the engine's own code (a 3D table read per pixel).
    if let Some(l) = s.lut.clone() {
        steps.push(Step::Cpu {
            name: "LUT",
            run: Box::new(move |img| crate::pipeline::apply_lut(img, &l)),
        });
    }
    // Effect plugins, with the engine's own code (and the plugin's).
    for e in &s.effects {
        let e = e.clone();
        steps.push(Step::Cpu {
            name: "effect",
            run: Box::new(move |img| crate::plugin_effects::apply(img, &e, &tile)),
        });
    }
    // 6. Detail: noise reduction, then sharpening.
    let (nr, sharpening) = effective_detail(img, s);
    if let Some(p) = detail::luminance_nr_plan(&nr, &cal.detail) {
        steps.push(Step::LuminanceNr(Box::new(p)));
    }
    if let Some(p) = detail::color_nr_plan(&nr, &cal.detail, w, h) {
        steps.push(Step::ColorNr(Box::new(p)));
    }
    if let Some(p) = detail::sharpen_plan(&sharpening, &cal.detail) {
        steps.push(Step::Sharpen(Box::new(p)));
    }
    if any(|a| a.noise > 0.0 || a.sharpness != 0.0) {
        let cal = cal.clone();
        steps.push(masked(&drawn, "mask detail", move |m, img| {
            m.apply_detail(img, &cal)
        }));
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
    if s.masks.overlay.is_some() {
        steps.push(masked(&drawn, "mask overlay", |m, img| {
            m.apply_overlay(img)
        }));
    }
    // The edges a lens correction leaves, filled from their neighbours.
    if let Some(blank) = stage0.lens_pass.as_ref().and_then(|p| p.blank.clone()) {
        let k = cal.lens;
        steps.push(Step::Cpu {
            name: "lens edges",
            run: Box::new(move |img| crate::lens::fill_blank(img, &blank, &k)),
        });
    }
    Ok(Plan {
        width: stage0.width,
        height: stage0.height,
        camera_rgb,
        steps,
        lut_size: LUT,
    })
}

/// The masks the plan's first mask step draws, for the later ones.
type Shared = std::sync::Arc<std::sync::Mutex<Option<mask::Drawn>>>;

/// A step applying part of the drawn masks.
fn masked(
    drawn: &Shared,
    name: &'static str,
    apply: impl Fn(&mask::Drawn, &mut Image) + Send + Sync + 'static,
) -> Step {
    let drawn = drawn.clone();
    Step::Cpu {
        name,
        run: Box::new(move |img| {
            if let Some(m) = drawn.lock().expect("masks").as_ref() {
                apply(m, img);
            }
        }),
    }
}
