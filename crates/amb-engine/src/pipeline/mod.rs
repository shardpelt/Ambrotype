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

mod camera;
mod settings;

pub use camera::*;
pub use settings::*;

pub(crate) fn srgb_encode(v: f32) -> f32 {
    if v <= 0.003_130_8 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    }
}

pub(crate) fn srgb_decode(v: f32) -> f32 {
    if v <= 0.040_45 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

pub(crate) const LUT: usize = 4096;

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
    sky_maps: Vec<mask::SkyMap>,
}

/// How one run of [`develop`] goes.
#[derive(Default)]
pub(crate) struct Pass<'a> {
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
        || s.masks.has_photo_maps()
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

/// The film negative analysis a render of `img` with `s` makes (film base
/// from the whole scan, levels inside the crop), with the conversion on.
/// `fresh`: what Auto would measure now, keeping a stored film base (for
/// Sync negative roll). Pass a small copy ([`crate::raw::downscale`]): this
/// converts the whole of `img`.
pub fn negative_analysis(
    img: &CameraImage,
    s: &DevelopSettings,
    fresh: bool,
) -> negative::Analysis {
    let mut s = s.clone();
    s.negative.enabled = true;
    if fresh {
        s.negative.auto = true;
    }
    camera_stage(img, &s, &Pass::default())
        .negative
        .expect("the conversion is on")
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
        let lens = s.lens.for_image(&img.shot);
        t.upright = upright::analyse(img, &lens, &cal.lens, t.upright_mode, &cal.upright);
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
/// Noise reduction and sharpening as they apply to `img`: a rendered image
/// gets neither unless the edit sets them (it was processed already).
pub(crate) fn effective_detail(
    img: &CameraImage,
    s: &DevelopSettings,
) -> (NoiseReduction, Sharpening) {
    let (mut nr, mut sharpening) = (s.noise_reduction, s.sharpening);
    if img.rendered {
        if s.unset_detail.color_noise {
            nr.color = 0.0;
        }
        if s.unset_detail.sharpness {
            sharpening.amount = 0.0;
        }
    }
    (nr, sharpening)
}

fn develop(img: &CameraImage, s: &DevelopSettings, pass: Pass) -> (Image, Globals) {
    let cal = &s.calibration;
    let mut stages = Stages::new();
    let mut globals = Globals::default();

    // 0. Lens Corrections on camera RGB, then Transform, crop and straighten
    // (one resample), then a film negative turned positive.
    let stage0 = camera_stage(img, s, &pass);
    stages.mark("upright+lens+crop");
    let CameraStage {
        crop,
        out: (out_w, out_h),
        tile,
        full,
        width,
        height,
        ref lens_pass,
        negative: negative_analysis,
        ..
    } = stage0;
    if negative_analysis.is_some() {
        globals.negative = negative_analysis;
    }
    let camera_rgb = stage0.rgb(img);
    // 1. Camera RGB → scene-linear ProPhoto with white balance and Exposure.
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
    } = camera_to_working(img, s, (out_w, out_h));
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

    // Spot removal, then masks drawn on the photo as it is now, Defringe
    // (global and local), then the local light sliders.
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
            pass.given.map(|g| g.sky_maps.as_slice()),
        )
    });
    if let Some(m) = &masks {
        globals.colour_targets = m.colour_targets.clone();
        globals.sky_maps = m.sky_maps.clone();
    }
    match &masks {
        Some(m) => m.apply_defringe(&mut out, &s.lens.defringe, cal),
        None => lens::apply_defringe_in(&mut out, &s.lens.defringe, &cal.lens, &tile),
    }
    if let Some(m) = &masks {
        m.apply_light(&mut out, &cal.masks);
    }
    stages.mark("spots+masks+defringe");

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

    if let Some(m) = &masks {
        m.apply_curves(&mut out);
    }
    if let Some(l) = &s.lut {
        apply_lut(&mut out, l);
    }
    for e in &s.effects {
        crate::plugin_effects::apply(&mut out, e, &tile);
    }
    stages.mark("colour+curves");

    // 6. Detail: noise reduction, then sharpening.
    let (nr, sharpening) = effective_detail(img, s);
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
mod tests;
