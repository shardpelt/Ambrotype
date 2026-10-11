// SPDX-License-Identifier: GPL-3.0-or-later
//! The develop pipeline as numbers another backend can run: the GPU preview
//! (`amb-gpu`) takes a [`Plan`] and runs each stage's kernel with these
//! parameters. They are the same values the CPU stages use, computed by the
//! same code, so the two backends can only differ in arithmetic.
//!
//! Stage 0 (Lens Corrections, Transform, crop, a film negative) runs here on
//! the CPU: it comes first, so its result is simply the GPU's input. A stage
//! without a GPU version yet makes [`plan`] return the reason instead, and
//! the caller renders on the CPU.

use crate::buffer::luminance_weights;
use crate::color::{ColorOp, ColorPlan};
use crate::pipeline::{
    BaseLook, CameraToWorking, DevelopSettings, LUT, Pass, camera_stage, camera_to_working,
    effective_detail,
};
use crate::raw::CameraImage;
use crate::tone::{self, ToneControls};

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

/// Everything a backend needs to develop one image.
#[derive(Debug, Clone)]
pub struct Plan {
    pub width: usize,
    pub height: usize,
    /// Camera RGB after stage 0, interleaved.
    pub camera_rgb: Vec<f32>,
    /// Stage 1: camera RGB to scene-linear ProPhoto (white balance, camera
    /// matrix, Calibration panel) and the Exposure gain.
    pub matrix: [[f32; 3]; 3],
    pub gain: f32,
    pub unclip: Option<UnclipPlan>,
    /// The Calibration panel's shadow tint: (stops, end).
    pub shadow_tint: Option<(f32, f32)>,
    /// Stage 3: the base look, then the global tone curve on luminance.
    pub base_look: BaseLook,
    pub lum_weights: [f32; 3],
    pub tone_lut: Option<Vec<f32>>,
    /// Stage 5: colour, then the curves.
    pub color: Option<ColorPlan>,
    pub curves: Curves,
    pub lut_size: usize,
}

/// The plan for developing `img` with `s`, or why the GPU can't run it yet.
pub fn plan(img: &CameraImage, s: &DevelopSettings) -> Result<Plan, String> {
    let unsupported = |what: &str| Err(format!("{what} has no GPU version yet"));
    if s.camera_profile.is_some() && !img.rendered && !s.negative.enabled {
        return unsupported("a camera profile");
    }
    if !s.masks.is_empty() {
        return unsupported("masks");
    }
    if !s.spots.is_empty() {
        return unsupported("spot removal");
    }
    if !s.lens.defringe.is_neutral() {
        return unsupported("defringe");
    }
    if s.dehaze != 0.0 {
        return unsupported("Dehaze");
    }
    if s.tone.highlights != 0.0 || s.tone.shadows != 0.0 {
        return unsupported("Highlights and Shadows");
    }
    if !s.local.is_neutral() {
        return unsupported("Texture and Clarity");
    }
    let (nr, sharpening) = effective_detail(img, s);
    if (nr.luminance > 0.0 && nr.luminance.is_finite()) || (nr.color > 0.0 && nr.color.is_finite())
    {
        return unsupported("noise reduction");
    }
    if sharpening.amount > 0.0 && sharpening.amount.is_finite() {
        return unsupported("sharpening");
    }
    if s.vignette.amount != 0.0 {
        return unsupported("post-crop vignetting");
    }
    if s.grain.amount != 0.0 {
        return unsupported("grain");
    }

    let stage0 = camera_stage(img, s, &Pass::default());
    if stage0.lens_pass.as_ref().is_some_and(|p| p.blank.is_some()) {
        return unsupported("filling the edges a lens correction leaves");
    }
    let camera_rgb = stage0.rgb(img).to_vec();
    let CameraToWorking {
        m,
        gain,
        shadow_tint,
        clipped,
        unclip_reach,
        ..
    } = camera_to_working(img, s, stage0.out);

    let cal = &s.calibration;
    let global = ToneControls {
        highlights: 0.0,
        shadows: 0.0,
        ..s.tone
    };
    let tone_lut = (!global.is_neutral()).then(|| tone::tone_lut(&global, &cal.tone, LUT));
    let color = (!s.color.is_neutral()).then(|| ColorOp::new(&s.color, cal.color).plan());
    let curves = Curves {
        parametric: (!s.parametric.is_neutral())
            .then(|| tone::parametric_lut(&s.parametric, &cal.tone, LUT)),
        rgb: s.curve_rgb.as_ref().map(|c| c.lut(LUT)),
        channels: std::array::from_fn(|i| s.curve_channels[i].as_ref().map(|c| c.lut(LUT))),
    };
    // A rendered image already has its tone curve.
    let base_look = if img.rendered {
        BaseLook::Linear
    } else {
        s.base_look
    };
    Ok(Plan {
        width: stage0.width,
        height: stage0.height,
        camera_rgb,
        matrix: m,
        gain,
        unclip: clipped.map(|u| UnclipPlan {
            clip: u.clip,
            gains: u.gains,
            reach: unclip_reach,
        }),
        shadow_tint,
        base_look,
        lum_weights: luminance_weights(),
        tone_lut,
        color,
        curves,
        lut_size: LUT,
    })
}
