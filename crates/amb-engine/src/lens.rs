// SPDX-License-Identifier: GPL-3.0-or-later
//! Lens Corrections: profile corrections (distortion, vignetting, lateral
//! chromatic aberration), Remove Chromatic Aberration, manual Distortion and
//! Lens Vignetting, Defringe, and Constrain Crop.
//!
//! **Model v1 (ours, not Adobe's).** Profile data is a [`amb_lens::Correction`]
//! resolved by the caller (lensfun database or the user's own Adobe LCP files).
//! The geometric part works on camera RGB before the colour matrix, in the
//! sensor's orientation:
//!
//! 1. vignetting gains (profile, then manual) on the source pixels;
//! 2. Remove CA: a radial red/blue shift model, estimated from the image's
//!    edges (or the profile's model if estimation finds too few edges);
//! 3. one resampling pass that composes crop and straighten
//!    ([`crate::crop`]) → Transform ([`crate::transform`]) → zoom → manual
//!    distortion → profile distortion → per-channel CA, with Catmull-Rom
//!    interpolation.
//!
//! Profile corrections scale to fill the frame (Lightroom auto-scales profile
//! corrections); manual distortion leaves blank areas unless Constrain Crop is
//! on. Defringe runs later, on the working-space image ([`apply_defringe`]).
//! All guessed constants live in [`LensStrengths`].

use std::borrow::Cow;

use amb_lens::{Correction, Geometry, ImageFrame};
use rayon::prelude::*;

use crate::buffer::{Image, Rect, Tile};
use crate::color::OkLab;
use crate::crop::CropGeometry;
use crate::filters::transpose;
use crate::transform::Homography;

/// Strength and shape constants, fitted later against Lightroom sweeps.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct LensStrengths {
    /// Manual Distortion ±100: radial coefficient k of r' = r·(1 + k r²)/(1 + k),
    /// r = 1 at the corner (corners stay put, edge midpoints move).
    pub manual_distortion: f64,
    /// Manual Lens Vignetting ±100: corner brightening/darkening in stops.
    pub manual_vignette_stops: f64,
    /// Radius (fraction of the half diagonal) where the manual vignetting
    /// starts, at Midpoint 0 and at Midpoint 100.
    pub manual_vignette_start_min: f64,
    pub manual_vignette_start_max: f64,
    /// Apply a profile's lateral-CA model even without Remove Chromatic Aberration.
    pub profile_tca_always: bool,
    /// Display-linear grey level for areas the corrected image no longer covers.
    pub blank_fill: f64,
    /// Remove CA estimation: largest red/blue shift searched, in pixels.
    pub ca_max_shift: f64,
    /// Defringe search radius in pixels per Amount unit, at 24 MP (scales with resolution).
    pub defringe_radius: f64,
    /// Defringe edge detection: OkLab lightness range (max − min in the window)
    /// where the edge weight goes from 0 to 1.
    pub defringe_contrast_lo: f64,
    pub defringe_contrast_hi: f64,
    /// Hue slider 0..100 → OkLab hue degrees: start + slider/100 · span.
    pub purple_hue_start: f64,
    pub purple_hue_span: f64,
    pub green_hue_start: f64,
    pub green_hue_span: f64,
}

impl Default for LensStrengths {
    fn default() -> Self {
        DEFAULT_STRENGTHS
    }
}

pub const DEFAULT_STRENGTHS: LensStrengths = LensStrengths {
    manual_distortion: 0.15,
    manual_vignette_stops: 1.5,
    manual_vignette_start_min: 0.0,
    manual_vignette_start_max: 0.85,
    profile_tca_always: false,
    blank_fill: 1.0,
    ca_max_shift: 4.0,
    defringe_radius: 0.35,
    defringe_contrast_lo: 0.08,
    defringe_contrast_hi: 0.25,
    purple_hue_start: 230.0,
    purple_hue_span: 160.0,
    green_hue_start: 70.0,
    green_hue_span: 140.0,
};

/// Defringe sliders: Amount 0..20, hue ranges 0..100.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Defringe {
    pub purple_amount: f64,
    pub purple_hue_lo: f64,
    pub purple_hue_hi: f64,
    pub green_amount: f64,
    pub green_hue_lo: f64,
    pub green_hue_hi: f64,
}

impl Default for Defringe {
    fn default() -> Self {
        Defringe {
            purple_amount: 0.0,
            purple_hue_lo: 30.0,
            purple_hue_hi: 70.0,
            green_amount: 0.0,
            green_hue_lo: 40.0,
            green_hue_hi: 60.0,
        }
    }
}

impl Defringe {
    pub fn is_neutral(&self) -> bool {
        self.purple_amount <= 0.0 && self.green_amount <= 0.0
    }
}

/// The Lens Corrections panel.
#[derive(Debug, Clone, PartialEq)]
pub struct LensCorrections {
    /// Enable Profile Corrections.
    pub profile_enable: bool,
    /// Profile Distortion / Vignetting / CA amounts, 0..200 (100 = as profiled).
    pub distortion_scale: f64,
    pub vignetting_scale: f64,
    pub ca_scale: f64,
    /// Remove Chromatic Aberration.
    pub auto_ca: bool,
    /// Manual Distortion, −100..100 (positive corrects barrel distortion).
    pub manual_distortion: f64,
    /// Manual Lens Vignetting Amount −100..100 (positive lightens corners) and Midpoint 0..100.
    pub vignette_amount: f64,
    pub vignette_midpoint: f64,
    pub defringe: Defringe,
    /// Constrain Crop: scale up so no blank areas remain.
    pub constrain_crop: bool,
    /// The profile chosen in Lightroom (`LensProfileName` / `LensProfileFilename`), if any.
    pub profile_name: Option<String>,
    pub profile_filename: Option<String>,
    /// The profile resolved for this image by the caller. `None` with
    /// `profile_enable` means no profile was found: nothing profile-based is applied.
    pub profile: Option<Correction>,
}

impl Default for LensCorrections {
    fn default() -> Self {
        LensCorrections {
            profile_enable: false,
            distortion_scale: 100.0,
            vignetting_scale: 100.0,
            ca_scale: 100.0,
            auto_ca: false,
            manual_distortion: 0.0,
            vignette_amount: 0.0,
            vignette_midpoint: 50.0,
            defringe: Defringe::default(),
            constrain_crop: false,
            profile_name: None,
            profile_filename: None,
            profile: None,
        }
    }
}

impl LensCorrections {
    fn active_profile(&self) -> Option<&Correction> {
        self.profile.as_ref().filter(|_| self.profile_enable)
    }

    /// Whether anything happens before the colour matrix.
    pub fn needs_camera_pass(&self) -> bool {
        self.active_profile().is_some()
            || self.auto_ca
            || self.manual_distortion != 0.0
            || self.vignette_amount != 0.0
    }
}

/// Result of the camera-space pass.
pub struct CameraPass {
    /// Output size: the input's, or the crop's.
    pub width: usize,
    pub height: usize,
    /// Corrected interleaved camera RGB, `width × height`.
    pub rgb: Vec<f32>,
    /// Pixels that the corrected image no longer covers (to fill with
    /// [`LensStrengths::blank_fill`] at the end of the pipeline).
    pub blank: Option<Vec<bool>>,
}

/// Radial red/blue displacement in pixels, outward positive:
/// δ(r) = a·r + b·r³ with r = 1 at the half diagonal.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RadialShift {
    pub a: f64,
    pub b: f64,
}

impl RadialShift {
    fn at(&self, r: f64) -> f64 {
        self.a * r + self.b * r * r * r
    }
}

/// Image centre and half diagonal in pixels.
fn centre(w: usize, h: usize) -> (f64, f64, f64) {
    let (cx, cy) = ((w as f64 - 1.0) / 2.0, (h as f64 - 1.0) / 2.0);
    (cx, cy, cx.hypot(cy).max(1.0))
}

fn smoothstep(e0: f64, e1: f64, x: f64) -> f64 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Apply the camera-space part of the lens corrections. `crop_factor` is the
/// camera's, when known.
pub fn apply_camera(
    rgb: &[f32],
    width: usize,
    height: usize,
    crop_factor: Option<f64>,
    lc: &LensCorrections,
    k: &LensStrengths,
) -> CameraPass {
    apply_camera_cropped(rgb, width, height, crop_factor, lc, k, None, None)
}

/// [`apply_camera`], then Transform, crop and straighten in the same
/// resampling pass. `warp` maps the transformed frame into the lens-corrected
/// one ([`crate::transform::Transform::sampling_map`]); the output is
/// `crop`'s size, and `crop` maps it into the transformed frame. Parts
/// outside the source are blank. With Constrain Crop, the transformed frame
/// is zoomed until no blank areas remain.
#[allow(clippy::too_many_arguments)]
pub fn apply_camera_cropped(
    rgb: &[f32],
    width: usize,
    height: usize,
    crop_factor: Option<f64>,
    lc: &LensCorrections,
    k: &LensStrengths,
    crop: Option<&CropGeometry>,
    warp: Option<&Homography>,
) -> CameraPass {
    apply_camera_window(rgb, width, height, crop_factor, lc, k, crop, warp, None)
}

/// [`apply_camera_cropped`] for `window` of the output only (all of it when
/// `None`): the pass returns that rectangle, sampled exactly as the full
/// output would be there.
#[allow(clippy::too_many_arguments)]
pub fn apply_camera_window(
    rgb: &[f32],
    width: usize,
    height: usize,
    crop_factor: Option<f64>,
    lc: &LensCorrections,
    k: &LensStrengths,
    crop: Option<&CropGeometry>,
    warp: Option<&Homography>,
    window: Option<Rect>,
) -> CameraPass {
    let frame = ImageFrame {
        width,
        height,
        crop_factor,
    };
    let profile = lc.active_profile().map(|c| Geometry::new(c, &frame));
    let (full_w, full_h) = crop.map_or((width, height), |c| (c.width, c.height));
    let win = window.unwrap_or(Rect {
        x: 0,
        y: 0,
        width: full_w,
        height: full_h,
    });
    let mut src = Cow::Borrowed(rgb);

    // 1. Vignetting gains on the source pixels.
    let profile_vignette = profile
        .as_ref()
        .filter(|g| g.has_vignetting() && lc.vignetting_scale > 0.0);
    if profile_vignette.is_some() || lc.vignette_amount != 0.0 {
        let (cx, cy, half_diag) = centre(width, height);
        let vs = lc.vignetting_scale / 100.0;
        let start = k.manual_vignette_start_min
            + (k.manual_vignette_start_max - k.manual_vignette_start_min)
                * (lc.vignette_midpoint / 100.0).clamp(0.0, 1.0);
        let manual_stops = k.manual_vignette_stops * lc.vignette_amount / 100.0;
        src.to_mut()
            .par_chunks_mut(width * 3)
            .enumerate()
            .for_each(|(y, row)| {
                for x in 0..width {
                    let (fx, fy) = (x as f64, y as f64);
                    let mut gain = 1.0;
                    if let Some(g) = profile_vignette {
                        gain *= g.vignette_gain(fx, fy).powf(vs);
                    }
                    if manual_stops != 0.0 {
                        let r = (fx - cx).hypot(fy - cy) / half_diag;
                        gain *= (manual_stops * smoothstep(start, 1.0, r)).exp2();
                    }
                    let gain = gain as f32;
                    for v in &mut row[x * 3..x * 3 + 3] {
                        *v *= gain;
                    }
                }
            });
    }

    // 2. Lateral chromatic aberration model.
    let auto_ca = if lc.auto_ca {
        estimate_ca(&src, width, height, k.ca_max_shift)
    } else {
        None
    };
    let profile_tca = profile
        .as_ref()
        .filter(|g| g.has_tca() && (lc.auto_ca || k.profile_tca_always) && auto_ca.is_none());

    // 3. Geometry.
    let profile_distortion = profile
        .as_ref()
        .filter(|g| g.has_distortion() && lc.distortion_scale != 0.0);
    let manual_k = -k.manual_distortion * lc.manual_distortion / 100.0;
    if profile_distortion.is_none()
        && manual_k == 0.0
        && auto_ca.is_none()
        && profile_tca.is_none()
        && crop.is_none()
        && warp.is_none()
    {
        let rgb = if win.width == width && win.height == height {
            src.into_owned()
        } else {
            let mut out = Vec::with_capacity(win.width * win.height * 3);
            for y in win.y..win.bottom() {
                out.extend_from_slice(&src[(y * width + win.x) * 3..(y * width + win.right()) * 3]);
            }
            out
        };
        return CameraPass {
            width: win.width,
            height: win.height,
            rgb,
            blank: None,
        };
    }
    let (cx, cy, half_diag) = centre(width, height);
    let ds = lc.distortion_scale / 100.0;
    let cs = lc.ca_scale / 100.0;
    // Output pixel → green source position, before zoom.
    let manual = move |x: f64, y: f64| -> (f64, f64) {
        if manual_k == 0.0 {
            return (x, y);
        }
        let (dx, dy) = (x - cx, y - cy);
        let r2 = (dx * dx + dy * dy) / (half_diag * half_diag);
        let m = (1.0 + manual_k * r2) / (1.0 + manual_k);
        (cx + dx * m, cy + dy * m)
    };
    let prof = |x: f64, y: f64| -> (f64, f64) {
        match profile_distortion {
            Some(g) => {
                let (px, py) = g.distort(x, y);
                (x + ds * (px - x), y + ds * (py - y))
            }
            None => (x, y),
        }
    };
    let zoomed = |z: f64, x: f64, y: f64| (cx + (x - cx) / z, cy + (y - cy) / z);
    let zoom = if lc.constrain_crop && (manual_k != 0.0 || profile_distortion.is_some()) {
        fill_zoom(width, height, |x, y| {
            let (mx, my) = manual(x, y);
            prof(mx, my)
        })
    } else if profile_distortion.is_some() {
        fill_zoom(width, height, prof)
    } else {
        1.0
    };
    let lens_map = |x: f64, y: f64| {
        let (zx, zy) = zoomed(zoom, x, y);
        let (mx, my) = manual(zx, zy);
        prof(mx, my)
    };
    // Transform, before the lens maps; Constrain Crop zooms the transformed
    // frame to fill.
    let warp_zoom = match warp {
        Some(h) if lc.constrain_crop => fill_zoom(width, height, |x, y| {
            let (tx, ty) = h.apply(x, y);
            lens_map(tx, ty)
        }),
        _ => 1.0,
    };
    let map_green = |x: f64, y: f64| match warp {
        Some(h) => {
            let (zx, zy) = zoomed(warp_zoom, x, y);
            let (tx, ty) = h.apply(zx, zy);
            lens_map(tx, ty)
        }
        None => lens_map(x, y),
    };
    let channel_positions = |gx: f64, gy: f64| -> [(f64, f64); 3] {
        if let Some([red, blue]) = &auto_ca {
            let (dx, dy) = (gx - cx, gy - cy);
            let rp = dx.hypot(dy);
            if rp < 1e-9 {
                return [(gx, gy); 3];
            }
            let r = rp / half_diag;
            let shift = |s: &RadialShift| {
                let f = 1.0 + s.at(r) / rp;
                (cx + dx * f, cy + dy * f)
            };
            [shift(red), (gx, gy), shift(blue)]
        } else if let Some(g) = profile_tca {
            let [r, b] = g.tca(gx, gy);
            let blend = |p: (f64, f64)| (gx + cs * (p.0 - gx), gy + cs * (p.1 - gy));
            [blend(r), (gx, gy), blend(b)]
        } else {
            [(gx, gy); 3]
        }
    };

    let (wf, hf) = (width as f64, height as f64);
    let inside = |x: f64, y: f64| x >= -0.5 && x <= wf - 0.5 && y >= -0.5 && y <= hf - 0.5;
    let (out_w, out_h) = (win.width, win.height);
    let mut out = vec![0.0f32; out_w * out_h * 3];
    let mut blank = vec![false; out_w * out_h];
    out.par_chunks_mut(out_w * 3)
        .zip(blank.par_chunks_mut(out_w))
        .enumerate()
        .for_each(|(y, (row, blank_row))| {
            for x in 0..out_w {
                // Output pixel → lens-corrected frame (crop) → source (lens).
                let (ox, oy) = ((win.x + x) as f64, (win.y + y) as f64);
                let (fx, fy) = crop.map_or((ox, oy), |c| c.source(ox, oy));
                let (gx, gy) = map_green(fx, fy);
                if !inside(fx, fy) || !inside(gx, gy) {
                    blank_row[x] = true;
                }
                if !(gx.is_finite() && gy.is_finite()) {
                    // Beyond the transformed photo's horizon.
                    continue;
                }
                let pos = channel_positions(gx, gy);
                if pos[0] == pos[1] && pos[2] == pos[1] {
                    let v = sample_bicubic_rgb(&src, width, height, gx, gy);
                    row[x * 3..x * 3 + 3].copy_from_slice(&v);
                } else {
                    for (c, (sx, sy)) in pos.iter().enumerate() {
                        row[x * 3 + c] = sample_bicubic(&src, width, height, c, *sx, *sy);
                    }
                }
            }
        });
    let any_blank = blank.iter().any(|b| *b);
    CameraPass {
        width: out_w,
        height: out_h,
        rgb: out,
        blank: any_blank.then_some(blank),
    }
}

/// Smallest zoom (≥ 0.5, ≤ 4) at which every border pixel of the output maps
/// inside the source image, for a mapping `f` from output to source pixels
/// (before zoom). Zooms out when the correction leaves room to spare.
fn fill_zoom(width: usize, height: usize, f: impl Fn(f64, f64) -> (f64, f64)) -> f64 {
    let (w, h) = (width as f64, height as f64);
    let (cx, cy) = ((w - 1.0) / 2.0, (h - 1.0) / 2.0);
    let n = 96;
    let border: Vec<(f64, f64)> = (0..=n)
        .flat_map(|i| {
            let t = i as f64 / n as f64;
            [
                (t * (w - 1.0), 0.0),
                (t * (w - 1.0), h - 1.0),
                (0.0, t * (h - 1.0)),
                (w - 1.0, t * (h - 1.0)),
            ]
        })
        .collect();
    let valid = |z: f64| {
        border.iter().all(|&(x, y)| {
            let (sx, sy) = f(cx + (x - cx) / z, cy + (y - cy) / z);
            sx >= -0.5 && sx <= w - 0.5 && sy >= -0.5 && sy <= h - 0.5
        })
    };
    let (mut lo, mut hi) = (0.5, 4.0);
    if valid(lo) {
        return lo;
    }
    if !valid(hi) {
        return hi;
    }
    for _ in 0..40 {
        let mid = 0.5 * (lo + hi);
        if valid(mid) {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    hi
}

fn catmull_rom(t: f32) -> [f32; 4] {
    let t2 = t * t;
    let t3 = t2 * t;
    [
        0.5 * (-t3 + 2.0 * t2 - t),
        0.5 * (3.0 * t3 - 5.0 * t2 + 2.0),
        0.5 * (-3.0 * t3 + 4.0 * t2 + t),
        0.5 * (t3 - t2),
    ]
}

/// Catmull-Rom sample of channel `c` at (x, y), clamping at the edges.
fn sample_bicubic(rgb: &[f32], w: usize, h: usize, c: usize, x: f64, y: f64) -> f32 {
    let x = x.clamp(0.0, (w - 1) as f64);
    let y = y.clamp(0.0, (h - 1) as f64);
    let (x0, y0) = (x.floor() as isize, y.floor() as isize);
    let (fx, fy) = ((x - x0 as f64) as f32, (y - y0 as f64) as f32);
    let (wx, wy) = (catmull_rom(fx), catmull_rom(fy));
    let mut acc = 0.0f32;
    for (j, wyj) in wy.iter().enumerate() {
        let yy = (y0 + j as isize - 1).clamp(0, h as isize - 1) as usize;
        let row = &rgb[yy * w * 3..(yy + 1) * w * 3];
        let mut racc = 0.0f32;
        for (i, wxi) in wx.iter().enumerate() {
            let xx = (x0 + i as isize - 1).clamp(0, w as isize - 1) as usize;
            racc += wxi * row[xx * 3 + c];
        }
        acc += wyj * racc;
    }
    acc
}

/// Catmull-Rom sample of all three channels at (x, y).
pub(crate) fn sample_bicubic_rgb(rgb: &[f32], w: usize, h: usize, x: f64, y: f64) -> [f32; 3] {
    let x = x.clamp(0.0, (w - 1) as f64);
    let y = y.clamp(0.0, (h - 1) as f64);
    let (x0, y0) = (x.floor() as isize, y.floor() as isize);
    let (wx, wy) = (
        catmull_rom((x - x0 as f64) as f32),
        catmull_rom((y - y0 as f64) as f32),
    );
    let xs: [usize; 4] =
        std::array::from_fn(|i| (x0 + i as isize - 1).clamp(0, w as isize - 1) as usize);
    let mut acc = [0.0f32; 3];
    for (j, wyj) in wy.iter().enumerate() {
        let yy = (y0 + j as isize - 1).clamp(0, h as isize - 1) as usize;
        let row = &rgb[yy * w * 3..(yy + 1) * w * 3];
        let mut racc = [0.0f32; 3];
        for (xx, wxi) in xs.iter().zip(wx) {
            let p = &row[xx * 3..xx * 3 + 3];
            racc[0] += wxi * p[0];
            racc[1] += wxi * p[1];
            racc[2] += wxi * p[2];
        }
        for c in 0..3 {
            acc[c] += wyj * racc[c];
        }
    }
    acc
}

/// Bilinear sample of channel `c` (clamped).
fn sample_bilinear(rgb: &[f32], w: usize, h: usize, c: usize, x: f64, y: f64) -> f64 {
    let x = x.clamp(0.0, (w - 1) as f64);
    let y = y.clamp(0.0, (h - 1) as f64);
    let (x0, y0) = (x.floor() as usize, y.floor() as usize);
    let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
    let (fx, fy) = (x - x0 as f64, y - y0 as f64);
    let at = |xx: usize, yy: usize| f64::from(rgb[(yy * w + xx) * 3 + c]);
    let top = at(x0, y0) * (1.0 - fx) + at(x1, y0) * fx;
    let bottom = at(x0, y1) * (1.0 - fx) + at(x1, y1) * fx;
    top * (1.0 - fy) + bottom * fy
}

/// Estimate lateral CA from the image: at strong edges that run across the
/// radial direction, find the radial offset that best aligns red (and blue)
/// with green, then fit δ(r) = a·r + b·r³ robustly. `None` when there are too
/// few usable edges.
pub fn estimate_ca(rgb: &[f32], w: usize, h: usize, max_shift: f64) -> Option<[RadialShift; 2]> {
    if w < 64 || h < 64 {
        return None;
    }
    let (cx, cy, half_diag) = centre(w, h);
    // Candidate edge points on a coarse grid, strongest per radial bin.
    const BINS: usize = 12;
    const PER_BIN: usize = 300;
    let stride = ((w * h) as f64 / 400_000.0).sqrt().ceil().max(2.0) as usize;
    let green = |x: usize, y: usize| f64::from(rgb[(y * w + x) * 3 + 1]);
    let mut bins: Vec<Vec<(f64, usize, usize)>> = vec![Vec::new(); BINS];
    let margin = (max_shift.ceil() as usize + 10).min(w / 4).min(h / 4);
    for y in (margin..h - margin).step_by(stride) {
        for x in (margin..w - margin).step_by(stride) {
            let (dx, dy) = (x as f64 - cx, y as f64 - cy);
            let rp = dx.hypot(dy);
            let r = rp / half_diag;
            if !(0.2..0.98).contains(&r) {
                continue;
            }
            let gx = green(x + 1, y) - green(x - 1, y);
            let gy = green(x, y + 1) - green(x, y - 1);
            let mag = gx.hypot(gy);
            let level = green(x, y).max(1e-4);
            if mag / level < 0.15 {
                continue;
            }
            let radial = (gx * dx + gy * dy).abs() / (rp * mag);
            if radial < 0.85 {
                continue;
            }
            let bin = ((r * BINS as f64) as usize).min(BINS - 1);
            bins[bin].push((mag / level, x, y));
        }
    }
    let points: Vec<(usize, usize)> = bins
        .into_iter()
        .flat_map(|mut b| {
            b.sort_by(|a, b| b.0.total_cmp(&a.0));
            b.truncate(PER_BIN);
            b.into_iter().map(|(_, x, y)| (x, y))
        })
        .collect();
    if points.len() < 60 {
        return None;
    }
    // Radial shift of channel c against green at each point.
    let measure = |c: usize| -> Vec<(f64, f64)> {
        points
            .par_iter()
            .filter_map(|&(x, y)| {
                let (dx, dy) = (x as f64 - cx, y as f64 - cy);
                let rp = dx.hypot(dy);
                let (ex, ey) = (dx / rp, dy / rp);
                let shift = radial_shift(rgb, w, h, c, x as f64, y as f64, ex, ey, max_shift)?;
                Some((rp / half_diag, shift))
            })
            .collect()
    };
    let fit = |samples: Vec<(f64, f64)>| robust_fit(&samples, max_shift);
    let red = fit(measure(0))?;
    let blue = fit(measure(2))?;
    Some([red, blue])
}

/// Offset t (pixels along (ex, ey)) such that channel c at s + t matches green
/// at s, around the edge point (x, y). `None` if the match is ambiguous.
#[allow(clippy::too_many_arguments)]
fn radial_shift(
    rgb: &[f32],
    w: usize,
    h: usize,
    c: usize,
    x: f64,
    y: f64,
    ex: f64,
    ey: f64,
    max_shift: f64,
) -> Option<f64> {
    const HALF: i32 = 6;
    let at = |ch: usize, t: f64| sample_bilinear(rgb, w, h, ch, x + ex * t, y + ey * t);
    let standardise = |v: &mut Vec<f64>| -> bool {
        let n = v.len() as f64;
        let mean = v.iter().sum::<f64>() / n;
        let var = v.iter().map(|a| (a - mean) * (a - mean)).sum::<f64>() / n;
        if var <= 1e-12 {
            return false;
        }
        let sd = var.sqrt();
        v.iter_mut().for_each(|a| *a = (*a - mean) / sd);
        true
    };
    let ts: Vec<f64> = (-HALF..=HALF).map(|i| i as f64 * 0.5).collect();
    let mut g: Vec<f64> = ts.iter().map(|t| at(1, *t)).collect();
    if !standardise(&mut g) {
        return None;
    }
    let cost = |d: f64| -> f64 {
        let mut v: Vec<f64> = ts.iter().map(|t| at(c, t + d)).collect();
        if !standardise(&mut v) {
            return f64::INFINITY;
        }
        v.iter().zip(&g).map(|(a, b)| (a - b) * (a - b)).sum()
    };
    let step = 0.1;
    let n = (max_shift / step).round() as i32;
    let costs: Vec<f64> = (-n..=n).map(|i| cost(i as f64 * step)).collect();
    let (best, &cmin) = costs.iter().enumerate().min_by(|a, b| a.1.total_cmp(b.1))?;
    if best == 0 || best == costs.len() - 1 || !cmin.is_finite() {
        return None;
    }
    // Parabolic refinement; reject flat minima.
    let (cl, cr) = (costs[best - 1], costs[best + 1]);
    let curvature = cl + cr - 2.0 * cmin;
    if curvature <= 1e-6 || cmin > 0.25 * ts.len() as f64 {
        return None;
    }
    let offset = 0.5 * (cl - cr) / curvature;
    Some((best as i32 - n) as f64 * step + offset * step)
}

/// Least-squares fit of δ = a·r + b·r³ with iterative outlier rejection.
fn robust_fit(samples: &[(f64, f64)], max_shift: f64) -> Option<RadialShift> {
    let mut pts: Vec<(f64, f64)> = samples.to_vec();
    let mut model = RadialShift { a: 0.0, b: 0.0 };
    for _ in 0..4 {
        if pts.len() < 40 {
            return None;
        }
        // Normal equations for basis (r, r³).
        let (mut s11, mut s13, mut s33, mut t1, mut t3) = (0.0, 0.0, 0.0, 0.0, 0.0);
        for &(r, d) in &pts {
            let r3 = r * r * r;
            s11 += r * r;
            s13 += r * r3;
            s33 += r3 * r3;
            t1 += r * d;
            t3 += r3 * d;
        }
        let det = s11 * s33 - s13 * s13;
        if det.abs() < 1e-12 {
            return None;
        }
        model = RadialShift {
            a: (t1 * s33 - t3 * s13) / det,
            b: (s11 * t3 - s13 * t1) / det,
        };
        let mut residuals: Vec<f64> = pts.iter().map(|&(r, d)| (d - model.at(r)).abs()).collect();
        residuals.sort_by(f64::total_cmp);
        let mad = residuals[residuals.len() / 2].max(0.02);
        pts.retain(|&(r, d)| (d - model.at(r)).abs() <= 2.5 * mad);
    }
    // Shifts larger than the search range are not trustworthy.
    if model.at(1.0).abs() > max_shift || !model.a.is_finite() || !model.b.is_finite() {
        return None;
    }
    Some(model)
}

/// Defringe's search radius in pixels on an output of `tile`'s full size.
pub fn defringe_radius(d: &Defringe, k: &LensStrengths, tile: &Tile) -> usize {
    let resolution = ((tile.full_width * tile.full_height) as f64 / 24e6).sqrt();
    let amount = d.purple_amount.max(d.green_amount);
    ((1.0 + amount * k.defringe_radius) * resolution)
        .round()
        .max(1.0) as usize
}

/// Defringe on the working-space image (scene-linear ProPhoto): desaturate
/// purple and green fringes next to high-contrast edges.
pub fn apply_defringe(img: &mut Image, d: &Defringe, k: &LensStrengths) {
    let tile = Tile::of(img);
    apply_defringe_in(img, d, k, &tile);
}

/// [`apply_defringe`] on a part of the output (`tile`), with the search
/// radius of the full output.
pub fn apply_defringe_in(img: &mut Image, d: &Defringe, k: &LensStrengths, tile: &Tile) {
    if d.is_neutral() || img.width < 3 || img.height < 3 {
        return;
    }
    let (w, h) = (img.width, img.height);
    let oklab = OkLab::new();
    let positive = |p: &[f32]| [p[0].max(0.0), p[1].max(0.0), p[2].max(0.0)];
    let lightness: Vec<f32> = img
        .rgb
        .par_chunks(3)
        .map(|p| oklab.from_working(positive(p))[0])
        .collect();
    let radius = defringe_radius(d, k, tile);
    let contrast = local_range(&lightness, w, h, radius);

    let band = |amount: f64, lo: f64, hi: f64, start: f64, span: f64| {
        let (lo, hi) = (lo.min(hi), lo.max(hi));
        (
            (amount / 4.0).clamp(0.0, 1.0),
            start + lo / 100.0 * span,
            start + hi / 100.0 * span,
        )
    };
    let purple = band(
        d.purple_amount,
        d.purple_hue_lo,
        d.purple_hue_hi,
        k.purple_hue_start,
        k.purple_hue_span,
    );
    let green = band(
        d.green_amount,
        d.green_hue_lo,
        d.green_hue_hi,
        k.green_hue_start,
        k.green_hue_span,
    );
    // Weight of hue (degrees) inside [lo, hi] with an 8° feather, on the circle.
    let hue_weight = |hue: f64, lo: f64, hi: f64| -> f64 {
        let centre = 0.5 * (lo + hi);
        let half = 0.5 * (hi - lo);
        let dist = ((hue - centre + 540.0).rem_euclid(360.0) - 180.0).abs();
        1.0 - smoothstep(half, half + 8.0, dist)
    };
    let (clo, chi) = (k.defringe_contrast_lo, k.defringe_contrast_hi);
    img.rgb
        .par_chunks_mut(3)
        .zip(contrast.par_iter())
        .for_each(|(p, c)| {
            let edge = smoothstep(clo, chi, f64::from(*c));
            if edge <= 0.0 {
                return;
            }
            let l = oklab.from_working(positive(p));
            let chroma = f64::from(l[1].hypot(l[2]));
            if chroma < 0.01 {
                return;
            }
            let hue = f64::from(l[2].atan2(l[1])).to_degrees().rem_euclid(360.0);
            let wp = purple.0 * hue_weight(hue, purple.1, purple.2);
            let wg = green.0 * hue_weight(hue, green.1, green.2);
            let wgt = (edge * wp.max(wg)).clamp(0.0, 1.0) as f32;
            if wgt <= 0.0 {
                return;
            }
            let out = oklab.to_working([l[0], l[1] * (1.0 - wgt), l[2] * (1.0 - wgt)]);
            p.copy_from_slice(&out);
        });
}

/// max − min of `plane` over a (2r+1)² window, separable.
fn local_range(plane: &[f32], w: usize, h: usize, r: usize) -> Vec<f32> {
    // One pass along rows of length `width`, producing (max, min) planes.
    let pass = |max_src: &[f32], min_src: &[f32], width: usize| -> (Vec<f32>, Vec<f32>) {
        let mut max_out = vec![0.0f32; max_src.len()];
        let mut min_out = vec![0.0f32; min_src.len()];
        max_out
            .par_chunks_mut(width)
            .zip(min_out.par_chunks_mut(width))
            .zip(max_src.par_chunks(width).zip(min_src.par_chunks(width)))
            .for_each(|((mo, mi), (ms, ns))| {
                for x in 0..width {
                    let lo = x.saturating_sub(r);
                    let hi = (x + r).min(width - 1);
                    mo[x] = ms[lo..=hi].iter().copied().fold(f32::MIN, f32::max);
                    mi[x] = ns[lo..=hi].iter().copied().fold(f32::MAX, f32::min);
                }
            });
        (max_out, min_out)
    };
    let (max_h, min_h) = pass(plane, plane, w);
    let (max_v, min_v) = pass(&transpose(&max_h, w, h), &transpose(&min_h, w, h), h);
    let range: Vec<f32> = max_v.par_iter().zip(&min_v).map(|(a, b)| a - b).collect();
    transpose(&range, h, w)
}

/// Paint the blank areas left by the geometric corrections.
pub fn fill_blank(img: &mut Image, blank: &[bool], k: &LensStrengths) {
    let v = k.blank_fill as f32;
    img.rgb
        .par_chunks_mut(3)
        .zip(blank.par_iter())
        .for_each(|(p, b)| {
            if *b {
                p.copy_from_slice(&[v, v, v]);
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use amb_lens::{Distortion, Frame, Source, Tca, Vignetting};

    fn correction(
        distortion: Option<(Distortion, Frame)>,
        tca: Option<(Tca, Frame)>,
        vignetting: Option<(Vignetting, Frame)>,
    ) -> Correction {
        Correction {
            source: Source::Lensfun,
            description: "synthetic".into(),
            file: None,
            distortion,
            tca,
            vignetting,
        }
    }

    const HUGIN: Frame = Frame::HuginShortSide {
        cal_crop: 1.0,
        cal_aspect: 1.5,
    };

    fn lens(profile: Correction) -> LensCorrections {
        LensCorrections {
            profile_enable: true,
            profile: Some(profile),
            ..Default::default()
        }
    }

    #[test]
    fn neutral_settings_do_nothing() {
        let lc = LensCorrections::default();
        assert!(!lc.needs_camera_pass());
        let rgb = vec![0.5f32; 30 * 20 * 3];
        let out = apply_camera(&rgb, 30, 20, None, &lc, &DEFAULT_STRENGTHS);
        assert_eq!(out.rgb, rgb);
        assert!(out.blank.is_none());
    }

    #[test]
    fn profile_vignetting_flattens_a_vignetted_field() {
        let (w, h) = (300, 200);
        let v = Vignetting::Pa {
            k1: -0.4,
            k2: 0.1,
            k3: 0.0,
        };
        let c = correction(None, None, Some((v, Frame::HalfDiagonal { cal_crop: 1.0 })));
        let geo = Geometry::new(
            &c,
            &ImageFrame {
                width: w,
                height: h,
                crop_factor: Some(1.0),
            },
        );
        // Flat 0.5 field, darkened by the lens.
        let mut rgb = vec![0.0f32; w * h * 3];
        for y in 0..h {
            for x in 0..w {
                let g = 0.5 / geo.vignette_gain(x as f64, y as f64);
                rgb[(y * w + x) * 3..(y * w + x) * 3 + 3].copy_from_slice(&[g as f32; 3]);
            }
        }
        let out = apply_camera(&rgb, w, h, Some(1.0), &lens(c.clone()), &DEFAULT_STRENGTHS);
        assert!(out.rgb.iter().all(|v| (v - 0.5).abs() < 1e-4));
        // Vignetting amount 0 leaves the falloff in.
        let half = LensCorrections {
            vignetting_scale: 0.0,
            ..lens(c)
        };
        let out = apply_camera(&rgb, w, h, Some(1.0), &half, &DEFAULT_STRENGTHS);
        assert_eq!(out.rgb, rgb);
    }

    /// A smooth test scene (no aliasing), as an RGB image seen through `distort`
    /// (output pixel → scene position).
    fn scene(x: f64, y: f64) -> f32 {
        (0.5 + 0.2 * (x / 9.0).sin() + 0.2 * (y / 7.0).cos()) as f32
    }

    fn render_scene(w: usize, h: usize, distort: impl Fn(f64, f64) -> (f64, f64)) -> Vec<f32> {
        let mut rgb = vec![0.0f32; w * h * 3];
        for y in 0..h {
            for x in 0..w {
                let (sx, sy) = distort(x as f64, y as f64);
                rgb[(y * w + x) * 3..(y * w + x) * 3 + 3].copy_from_slice(&[scene(sx, sy); 3]);
            }
        }
        rgb
    }

    /// Smallest mean error between `img` and the scene at any zoom about the
    /// centre: 0 when `img` is an undistorted (scaled) view of the scene.
    fn best_zoom_error(img: &[f32], w: usize, h: usize) -> f64 {
        let (cx, cy, _) = centre(w, h);
        let err = |z: f64| -> f64 {
            let (mut sum, mut n) = (0.0, 0);
            for y in (h / 10..h - h / 10).step_by(3) {
                for x in (w / 10..w - w / 10).step_by(3) {
                    let expect = scene(cx + (x as f64 - cx) / z, cy + (y as f64 - cy) / z);
                    sum += f64::from((img[(y * w + x) * 3 + 1] - expect).abs());
                    n += 1;
                }
            }
            sum / n as f64
        };
        (0..=1200)
            .map(|i| err(0.7 + i as f64 * 0.0005))
            .fold(f64::MAX, f64::min)
    }

    #[test]
    fn profile_distortion_undoes_the_lens() {
        let (w, h) = (360, 240);
        let d = Distortion::Poly5 { k1: -0.06, k2: 0.0 };
        let c = correction(Some((d, HUGIN)), None, None);
        let frame = ImageFrame {
            width: w,
            height: h,
            crop_factor: Some(1.0),
        };
        let geo = Geometry::new(&c, &frame);
        // The camera sees the scene through a barrel lens: the observed pixel at
        // source position s shows the scene point u with distort(u) = s.
        let invert = |sx: f64, sy: f64| {
            let (mut ux, mut uy) = (sx, sy);
            for _ in 0..40 {
                let (dx, dy) = geo.distort(ux, uy);
                ux += sx - dx;
                uy += sy - dy;
            }
            (ux, uy)
        };
        let observed = render_scene(w, h, invert);
        let before = best_zoom_error(&observed, w, h);
        let out = apply_camera(&observed, w, h, Some(1.0), &lens(c), &DEFAULT_STRENGTHS);
        assert!(out.blank.is_none(), "profile corrections fill the frame");
        let after = best_zoom_error(&out.rgb, w, h);
        assert!(before > 0.03, "test image is visibly distorted ({before})");
        assert!(
            after < 0.005,
            "undistorted after correction: {after} (before {before})"
        );
    }

    #[test]
    fn manual_distortion_leaves_blanks_unless_constrained() {
        let (w, h) = (120, 80);
        let rgb = vec![0.5f32; w * h * 3];
        let lc = LensCorrections {
            manual_distortion: 60.0,
            ..Default::default()
        };
        let out = apply_camera(&rgb, w, h, None, &lc, &DEFAULT_STRENGTHS);
        let blank = out
            .blank
            .expect("correcting barrel pulls the edge midpoints in");
        assert!(blank[(h / 2) * w], "left edge midpoint is blank");
        assert!(!blank[0], "corners stay put");
        assert!(!blank[(h / 2) * w + w / 2], "centre is covered");
        let constrained = LensCorrections {
            constrain_crop: true,
            ..lc
        };
        let out = apply_camera(&rgb, w, h, None, &constrained, &DEFAULT_STRENGTHS);
        assert!(out.blank.is_none());
    }

    #[test]
    fn crop_resamples_the_rotated_rectangle_and_blanks_outside() {
        use crate::crop::Crop;
        let (w, h) = (240, 160);
        let observed = render_scene(w, h, |x, y| (x, y));
        let lc = LensCorrections::default();
        // A straightened crop inside the frame: every output pixel is the
        // scene at its mapped position, and nothing is blank.
        let inside = Crop {
            enabled: true,
            left: 0.2,
            top: 0.2,
            right: 0.8,
            bottom: 0.8,
            angle: 5.0,
        };
        let g = inside.geometry(w, h).expect("a crop");
        let out = apply_camera_cropped(
            &observed,
            w,
            h,
            None,
            &lc,
            &DEFAULT_STRENGTHS,
            Some(&g),
            None,
        );
        assert_eq!((out.width, out.height), (g.width, g.height));
        assert_eq!(out.rgb.len(), g.width * g.height * 3);
        assert!(out.blank.is_none());
        let mut worst = 0.0f32;
        for y in 0..g.height {
            for x in 0..g.width {
                let (sx, sy) = g.source(x as f64, y as f64);
                let v = out.rgb[(y * g.width + x) * 3 + 1];
                worst = worst.max((v - scene(sx, sy)).abs());
            }
        }
        assert!(worst < 2e-3, "{worst}");
        // The full-frame corners with an angle: the rotated rectangle pokes
        // out of the frame, and those parts are blank.
        let full = Crop {
            left: 0.0,
            top: 0.0,
            right: 1.0,
            bottom: 1.0,
            ..inside
        };
        let g = full.geometry(w, h).expect("a crop");
        let out = apply_camera_cropped(
            &observed,
            w,
            h,
            None,
            &lc,
            &DEFAULT_STRENGTHS,
            Some(&g),
            None,
        );
        let blank = out.blank.expect("blank corners");
        let (cw, ch) = (g.width, g.height);
        assert!(
            blank[cw - 1] && blank[(ch - 1) * cw],
            "the other two corners"
        );
        assert!(!blank[(ch / 2) * cw + cw / 2]);
    }

    #[test]
    fn manual_vignetting_lightens_corners_and_follows_midpoint() {
        let (w, h) = (90, 60);
        let rgb = vec![0.25f32; w * h * 3];
        let lc = LensCorrections {
            vignette_amount: 100.0,
            ..Default::default()
        };
        let out = apply_camera(&rgb, w, h, None, &lc, &DEFAULT_STRENGTHS);
        let at = |o: &CameraPass, x: usize, y: usize| o.rgb[(y * w + x) * 3];
        assert!(
            (at(&out, w / 2, h / 2) - 0.25).abs() < 1e-6,
            "centre unchanged"
        );
        assert!(at(&out, 0, 0) > 0.6, "corner lightened by ~1.5 stops");
        let late = LensCorrections {
            vignette_midpoint: 100.0,
            ..lc
        };
        let out_late = apply_camera(&rgb, w, h, None, &late, &DEFAULT_STRENGTHS);
        let mid = (w / 4, h / 4);
        assert!(at(&out_late, mid.0, mid.1) < at(&out, mid.0, mid.1));
    }

    /// Edges at many angles: a pattern of rings, with red scaled radially by
    /// `kr` and blue by `kb` (lateral CA).
    fn rings_with_ca(w: usize, h: usize, kr: f64, kb: f64) -> Vec<f32> {
        let (cx, cy, _) = centre(w, h);
        let pattern = |x: f64, y: f64| -> f32 {
            let r = (x - cx).hypot(y - cy);
            let angle = (y - cy).atan2(x - cx);
            let v = (r / 9.0 + 0.3 * (angle * 7.0).sin()).sin();
            if v > 0.0 { 0.85 } else { 0.08 }
        };
        let sample = |x: f64, y: f64, k: f64| {
            let mut acc = 0.0;
            for sy in 0..4 {
                for sx in 0..4 {
                    let px = x + (sx as f64 - 1.5) / 4.0;
                    let py = y + (sy as f64 - 1.5) / 4.0;
                    // Channel content at radius r comes from scene radius r / k.
                    acc += pattern(cx + (px - cx) / k, cy + (py - cy) / k);
                }
            }
            acc / 16.0
        };
        let mut rgb = vec![0.0f32; w * h * 3];
        rgb.par_chunks_mut(w * 3).enumerate().for_each(|(y, row)| {
            for x in 0..w {
                let (fx, fy) = (x as f64, y as f64);
                row[x * 3] = sample(fx, fy, kr);
                row[x * 3 + 1] = sample(fx, fy, 1.0);
                row[x * 3 + 2] = sample(fx, fy, kb);
            }
        });
        rgb
    }

    #[test]
    fn auto_ca_estimates_and_removes_radial_shift() {
        let (w, h) = (600, 400);
        let (kr, kb) = (1.004, 0.996);
        let rgb = rings_with_ca(w, h, kr, kb);
        let [red, blue] = estimate_ca(&rgb, w, h, 4.0).expect("enough edges");
        let half_diag = centre(w, h).2;
        // Red is displaced outward by (kr − 1)·r_px at radius r_px.
        let expect_red = (kr - 1.0) * half_diag;
        let expect_blue = (kb - 1.0) * half_diag;
        assert!(
            (red.at(1.0) - expect_red).abs() < 0.25,
            "red {red:?} vs {expect_red}"
        );
        assert!(
            (blue.at(1.0) - expect_blue).abs() < 0.25,
            "blue {blue:?} vs {expect_blue}"
        );
        let lc = LensCorrections {
            auto_ca: true,
            ..Default::default()
        };
        let out = apply_camera(&rgb, w, h, None, &lc, &DEFAULT_STRENGTHS);
        let misfit = |img: &[f32]| -> f64 {
            img.chunks(3)
                .map(|p| f64::from((p[0] - p[1]).abs() + (p[2] - p[1]).abs()))
                .sum::<f64>()
        };
        assert!(
            misfit(&out.rgb) < 0.5 * misfit(&rgb),
            "fringes reduced: {} → {}",
            misfit(&rgb),
            misfit(&out.rgb)
        );
    }

    #[test]
    fn auto_ca_finds_nothing_in_a_flat_image() {
        let rgb = vec![0.3f32; 200 * 150 * 3];
        assert!(estimate_ca(&rgb, 200, 150, 4.0).is_none());
    }

    #[test]
    fn profile_tca_applies_with_remove_ca_only() {
        let (w, h) = (200, 140);
        let rgb = rings_with_ca(w, h, 1.0, 1.0);
        let tca = Tca::Linear {
            kr: 1.002,
            kb: 0.998,
        };
        let c = correction(None, Some((tca, HUGIN)), None);
        // Without Remove CA, a profile's TCA is not applied (by default).
        let out = apply_camera(&rgb, w, h, Some(1.0), &lens(c.clone()), &DEFAULT_STRENGTHS);
        assert_eq!(out.rgb, rgb);
        // On a flat field (no edges for auto CA), Remove CA falls back to the profile.
        let flat = vec![0.4f32; w * h * 3];
        let lc = LensCorrections {
            auto_ca: true,
            ..lens(c)
        };
        let out = apply_camera(&flat, w, h, Some(1.0), &lc, &DEFAULT_STRENGTHS);
        assert!(out.rgb.iter().all(|v| (v - 0.4).abs() < 1e-5));
    }

    #[test]
    fn defringe_desaturates_purple_at_edges_only() {
        let (w, h) = (80, 40);
        let oklab = OkLab::new();
        // Purple (OkLab hue ~310°) at chroma 0.1, lightness 0.5.
        let hue = 310f32.to_radians();
        let purple = oklab.to_working([0.5, 0.1 * hue.cos(), 0.1 * hue.sin()]);
        let mut img = Image::new(w, h);
        for y in 0..h {
            for x in 0..w {
                let v = if x < 20 {
                    [0.02, 0.02, 0.02] // dark
                } else if x < 22 {
                    purple // a 2 px fringe at the edge
                } else if x < 50 {
                    [0.9, 0.9, 0.9] // bright
                } else {
                    purple // a purple subject far from any edge
                };
                img.set(x, y, v);
            }
        }
        let d = Defringe {
            purple_amount: 10.0,
            ..Default::default()
        };
        apply_defringe(&mut img, &d, &DEFAULT_STRENGTHS);
        let chroma = |p: [f32; 3]| {
            let l = oklab.from_working(p);
            l[1].hypot(l[2])
        };
        assert!(chroma(img.get(20, 20)) < 0.03, "fringe removed");
        assert!(chroma(img.get(21, 20)) < 0.03, "fringe removed");
        assert!(chroma(img.get(70, 20)) > 0.09, "flat purple subject kept");
        // Green amount alone doesn't touch purple.
        let mut img2 = Image::new(w, h);
        for y in 0..h {
            for x in 0..w {
                img2.set(x, y, if x < 20 { [0.02; 3] } else { purple });
            }
        }
        let green_only = Defringe {
            green_amount: 10.0,
            ..Default::default()
        };
        apply_defringe(&mut img2, &green_only, &DEFAULT_STRENGTHS);
        assert!(chroma(img2.get(21, 20)) > 0.09);
    }
}
