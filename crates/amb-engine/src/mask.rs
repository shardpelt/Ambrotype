// SPDX-License-Identifier: GPL-3.0-or-later
//! Masks and local adjustments (docs/design/masks.md).
//!
//! Each mask (a Camera Raw *correction*) is drawn into an alpha plane the size
//! of the output, from its components in order: linear and radial gradients,
//! brush strokes, and luminance and colour ranges, added, subtracted or
//! intersected. The local sliders then act through that plane at the matching
//! pipeline stage, as if the global slider were moved by that much under the
//! mask: Exposure, Temp and Tint on scene-linear light; Dehaze after the
//! global one; the tone sliders after the global tone curve; Texture and
//! Clarity, colour, then sharpening and noise reduction after theirs.
//!
//! The geometry follows Lightroom's files exactly; fall-off shapes and slider
//! strengths are ours, to be fitted ([`MaskStrengths`]).

use std::collections::BTreeMap;

use amb_presets::Value;
use amb_presets::masks::{
    self, Combination, LEGACY_TAGS, MASK_ENABLE_TAG, MASK_TAG, MaskKind, Mode, flag, num,
};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::buffer::{Image, luminance_weights};
use crate::calibration::Calibration;
use crate::color::OkLab;
use crate::crop::CropGeometry;
use crate::detail::{NoiseReduction, Sharpening};
use crate::filters::gaussian_blur;
use crate::local::{LocalContrast, reference_scale};
use crate::tone::{self, ToneControls};
use crate::{dehaze, detail, local};

/// Model constants for local adjustments, to be fitted against Lightroom.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MaskStrengths {
    /// Red/blue gain exponent at Temp ±100 (scene-linear).
    pub temperature: f64,
    /// Green gain exponent at Tint ±100.
    pub tint: f64,
    /// Global Sharpening amount added at local Sharpness +100.
    pub sharpness: f64,
    /// Blur sigma (pixels at the 24 MP reference) at local Sharpness −100.
    pub soften_sigma: f64,
    /// OkLab chroma of the Color tint at amount 100.
    pub toning: f64,
    /// Colour range: OkLab distance accepted at Refine 0 and at Refine 100.
    pub color_tolerance_min: f64,
    pub color_tolerance_max: f64,
}

impl Default for MaskStrengths {
    fn default() -> Self {
        MaskStrengths {
            temperature: 0.35,
            tint: 0.25,
            sharpness: 60.0,
            soften_sigma: 4.0,
            toning: 0.08,
            color_tolerance_min: 0.02,
            color_tolerance_max: 0.2,
        }
    }
}

/// A mask's local sliders in UI units (Exposure in EV, the rest −100..100,
/// toning hue in degrees), already scaled by the mask's Amount.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct LocalAdjustments {
    pub exposure: f64,
    pub temperature: f64,
    pub tint: f64,
    pub contrast: f64,
    pub highlights: f64,
    pub shadows: f64,
    pub whites: f64,
    pub blacks: f64,
    pub texture: f64,
    pub clarity: f64,
    pub dehaze: f64,
    pub saturation: f64,
    pub toning_hue: f64,
    pub toning_saturation: f64,
    pub sharpness: f64,
    pub noise: f64,
}

impl LocalAdjustments {
    fn from_correction(c: &BTreeMap<String, Value>) -> LocalAdjustments {
        let amount = num(c, "CorrectionAmount").unwrap_or(1.0).clamp(0.0, 1.0);
        let ui = |tag: &str| -> f64 {
            let s = masks::local_slider(tag).expect("known slider");
            let v = num(c, tag).unwrap_or(0.0) * s.scale;
            v.clamp(s.min, s.max)
        };
        let scaled = |tag: &str| ui(tag) * amount;
        LocalAdjustments {
            exposure: scaled("LocalExposure2012"),
            temperature: scaled("LocalTemperature"),
            tint: scaled("LocalTint"),
            contrast: scaled("LocalContrast2012"),
            highlights: scaled("LocalHighlights2012"),
            shadows: scaled("LocalShadows2012"),
            whites: scaled("LocalWhites2012"),
            blacks: scaled("LocalBlacks2012"),
            texture: scaled("LocalTexture"),
            clarity: scaled("LocalClarity2012"),
            dehaze: scaled("LocalDehaze"),
            saturation: scaled("LocalSaturation"),
            toning_hue: ui("LocalToningHue"),
            toning_saturation: scaled("LocalToningSaturation"),
            sharpness: scaled("LocalSharpness"),
            noise: scaled("LocalLuminanceNoise"),
        }
    }

    fn tone(&self) -> ToneControls {
        ToneControls {
            contrast: self.contrast,
            highlights: self.highlights,
            shadows: self.shadows,
            whites: self.whites,
            blacks: self.blacks,
        }
    }

    pub fn is_neutral(&self) -> bool {
        LocalAdjustments {
            toning_hue: 0.0,
            ..*self
        } == LocalAdjustments::default()
    }
}

/// One brush stroke.
#[derive(Debug, Clone, PartialEq)]
pub struct Stroke {
    /// Dab centres, image fractions.
    pub dabs: Vec<(f64, f64)>,
    /// Radius as a fraction of the image's longer side.
    pub radius: f64,
    pub flow: f64,
    /// Fraction of the radius at full strength (`CenterWeight`).
    pub hardness: f64,
    /// Density reached by painting (`MaskValue`), 0..1.
    pub density: f64,
    pub erase: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    /// No effect at `zero`, full effect at `full` (image fractions).
    Linear {
        zero: (f64, f64),
        full: (f64, f64),
    },
    /// An ellipse in its unrotated bounding box (image fractions), turned by
    /// `angle` degrees; `feather` 0..100.
    Radial {
        top: f64,
        left: f64,
        bottom: f64,
        right: f64,
        angle: f64,
        feather: f64,
    },
    Brush(Vec<Stroke>),
    /// `LumRange`: none below [0], full from [1] to [2], none above [3].
    Luminance([f64; 4]),
    /// Sampled colours (image fractions) and Refine 0..1.
    Color {
        samples: Vec<(f64, f64)>,
        amount: f64,
    },
    /// Something we can't draw (AI, depth, Lightroom's own colour samples):
    /// it selects nothing.
    Unsupported,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Part {
    pub shape: Shape,
    pub combination: Combination,
    /// `MaskValue` of an added shape (its opacity).
    pub opacity: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Correction {
    pub name: String,
    /// Position in `MaskGroupBasedCorrections` (None for the legacy tags).
    pub index: Option<usize>,
    pub parts: Vec<Part>,
    /// A pre-2021 range mask on the whole correction: intersected.
    pub range: Option<(Shape, bool)>,
    pub adjust: LocalAdjustments,
}

/// Every active mask of an edit, plus which one to show as an overlay.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Masks {
    pub corrections: Vec<Correction>,
    /// Draw this correction's mask (its index in `MaskGroupBasedCorrections`)
    /// over the photo in red, as well as applying its adjustments.
    pub overlay: Option<usize>,
}

impl Masks {
    /// Read the current and the legacy mask tags. Notes name what can't be
    /// rendered yet.
    pub fn from_settings(settings: &BTreeMap<String, Value>) -> (Masks, Vec<String>) {
        let mut out = Masks::default();
        let mut notes = Vec::new();
        let enabled = |tag: &str| settings.get(tag).and_then(Value::as_bool).unwrap_or(true);
        let lists = std::iter::once((MASK_TAG, MASK_ENABLE_TAG))
            .chain(LEGACY_TAGS)
            .filter(|(tag, enable)| enabled(enable) && settings.contains_key(*tag));
        for (tag, _) in lists {
            let Some(Value::Seq(items)) = settings.get(tag) else {
                continue;
            };
            for (i, item) in items.iter().enumerate() {
                let Value::Struct(c) = item else { continue };
                if flag(c, "CorrectionActive") == Some(false) {
                    continue;
                }
                let mut correction = read_correction(tag, c, &mut notes);
                correction.index = (tag == MASK_TAG).then_some(i);
                out.corrections.push(correction);
            }
        }
        notes.sort();
        notes.dedup();
        (out, notes)
    }

    pub fn is_empty(&self) -> bool {
        self.corrections.is_empty()
    }

    fn needs(&self, f: impl Fn(&Shape) -> bool) -> bool {
        self.corrections.iter().any(|c| {
            c.parts.iter().any(|p| f(&p.shape)) || c.range.as_ref().is_some_and(|(s, _)| f(s))
        })
    }
}

fn read_correction(tag: &str, c: &BTreeMap<String, Value>, notes: &mut Vec<String>) -> Correction {
    let name = masks::text(c, "CorrectionName").unwrap_or_else(|| "Mask".into());
    let components = masks::components(c);
    let mut parts = Vec::new();
    for range in masks::parts(components) {
        let entries: Vec<&BTreeMap<String, Value>> = components[range]
            .iter()
            .filter_map(|v| match v {
                Value::Struct(m) => Some(m),
                _ => None,
            })
            .collect();
        let Some(first) = entries.first() else {
            continue;
        };
        if flag(first, "MaskActive") == Some(false) {
            continue;
        }
        let kind = MaskKind::of(first);
        let shape = read_shape(&kind, &entries);
        if shape == Shape::Unsupported {
            notes.push(format!("{tag}: {} mask in \"{name}\"", kind.label()));
        }
        parts.push(Part {
            shape,
            combination: Combination::of(first),
            opacity: num(first, "MaskValue").unwrap_or(1.0).clamp(0.0, 1.0),
        });
    }
    let range = masks::range_mask(c).map(|r| {
        let entry: BTreeMap<String, Value> = [
            ("What".to_string(), Value::Text("Mask/RangeMask".into())),
            ("CorrectionRangeMask".to_string(), Value::Struct(r.clone())),
        ]
        .into_iter()
        .collect();
        let kind = MaskKind::of(&entry);
        let shape = read_shape(&kind, &[&entry]);
        if shape == Shape::Unsupported {
            notes.push(format!("{tag}: {} on \"{name}\"", kind.label()));
        }
        (shape, flag(r, "Invert").unwrap_or(false))
    });
    for field in masks::UNRENDERED_FIELDS {
        if c.get(*field).is_some_and(|v| !is_neutral_field(field, v)) {
            notes.push(format!("{tag}: {field} in \"{name}\""));
        }
    }
    for s in masks::LOCAL_SLIDERS.iter().filter(|s| !s.rendered) {
        if num(c, s.tag).is_some_and(|v| v != 0.0) {
            notes.push(format!("{tag}: {} in \"{name}\"", s.tag));
        }
    }
    Correction {
        name,
        index: None,
        parts,
        range,
        adjust: LocalAdjustments::from_correction(c),
    }
}

/// Curves at the identity and zeroed sliders change nothing.
fn is_neutral_field(field: &str, v: &Value) -> bool {
    if field.ends_with("Curve") {
        return amb_presets::curve_points(v)
            .is_none_or(|pts| pts.iter().all(|(x, y)| (x - y).abs() < 1e-6));
    }
    v.as_f64() == Some(0.0)
}

fn read_shape(kind: &MaskKind, entries: &[&BTreeMap<String, Value>]) -> Shape {
    let e = entries[0];
    let f = |k: &str, d: f64| num(e, k).unwrap_or(d);
    match kind {
        MaskKind::Linear => Shape::Linear {
            zero: (f("ZeroX", 0.5), f("ZeroY", 1.0)),
            full: (f("FullX", 0.5), f("FullY", 0.0)),
        },
        MaskKind::Radial => Shape::Radial {
            top: f("Top", 0.25),
            left: f("Left", 0.25),
            bottom: f("Bottom", 0.75),
            right: f("Right", 0.75),
            angle: f("Angle", 0.0),
            feather: f("Feather", 50.0).clamp(0.0, 100.0),
        },
        MaskKind::Brush => {
            // Painting raises the part towards its density; a stroke whose
            // value differs from the first one's in being zero erases.
            let first_zero = f("MaskValue", 1.0) == 0.0;
            Shape::Brush(
                entries
                    .iter()
                    .map(|s| {
                        let value = num(s, "MaskValue").unwrap_or(1.0).clamp(0.0, 1.0);
                        let dabs = match s.get("Dabs") {
                            Some(Value::Seq(d)) => d
                                .iter()
                                .filter_map(Value::as_str)
                                .filter_map(masks::parse_dab)
                                .collect(),
                            _ => Vec::new(),
                        };
                        let erase = (value == 0.0) != first_zero;
                        Stroke {
                            dabs,
                            radius: num(s, "Radius").unwrap_or(0.05).max(1e-4),
                            flow: num(s, "Flow").unwrap_or(1.0).clamp(0.0, 1.0),
                            hardness: num(s, "CenterWeight").unwrap_or(0.0).clamp(0.0, 1.0),
                            density: if first_zero { 1.0 } else { value },
                            erase,
                        }
                    })
                    .collect(),
            )
        }
        MaskKind::Luminance => {
            masks::range_mask(e)
                .and_then(masks::lum_range)
                .map_or(Shape::Unsupported, |mut r| {
                    for i in 1..4 {
                        r[i] = r[i].max(r[i - 1]);
                    }
                    Shape::Luminance(r)
                })
        }
        MaskKind::Color => {
            let r = masks::range_mask(e);
            let samples = r.map(masks::color_samples).unwrap_or_default();
            if samples.is_empty() {
                Shape::Unsupported
            } else {
                Shape::Color {
                    samples,
                    amount: r
                        .and_then(|r| num(r, "ColorAmount"))
                        .unwrap_or(0.5)
                        .clamp(0.0, 1.0),
                }
            }
        }
        _ => Shape::Unsupported,
    }
}

// ---------------------------------------------------------------- geometry

/// Output pixel ↔ uncropped frame pixel (pixel centres at integers).
#[derive(Debug, Clone, Copy)]
struct Frame {
    width: f64,
    height: f64,
    /// frame = o + x·ex + y·ey
    o: (f64, f64),
    ex: (f64, f64),
    ey: (f64, f64),
}

impl Frame {
    fn new(frame: (usize, usize), crop: Option<&CropGeometry>) -> Frame {
        let (o, ex, ey) = match crop {
            Some(g) => {
                let o = g.source(0.0, 0.0);
                let a = g.source(1.0, 0.0);
                let b = g.source(0.0, 1.0);
                (o, (a.0 - o.0, a.1 - o.1), (b.0 - o.0, b.1 - o.1))
            }
            None => ((0.0, 0.0), (1.0, 0.0), (0.0, 1.0)),
        };
        Frame {
            width: frame.0 as f64,
            height: frame.1 as f64,
            o,
            ex,
            ey,
        }
    }

    /// Frame pixel coordinates (centres at integers) of output pixel (x, y).
    #[inline]
    fn source(&self, x: f64, y: f64) -> (f64, f64) {
        (
            self.o.0 + x * self.ex.0 + y * self.ey.0,
            self.o.1 + x * self.ex.1 + y * self.ey.1,
        )
    }

    /// Output pixel of a frame position.
    fn target(&self, fx: f64, fy: f64) -> (f64, f64) {
        let det = self.ex.0 * self.ey.1 - self.ey.0 * self.ex.1;
        let (dx, dy) = (fx - self.o.0, fy - self.o.1);
        (
            (dx * self.ey.1 - dy * self.ey.0) / det,
            (self.ex.0 * dy - self.ex.1 * dx) / det,
        )
    }

    /// Image fraction → frame pixel coordinates.
    fn pixel(&self, u: f64, v: f64) -> (f64, f64) {
        (u * self.width - 0.5, v * self.height - 0.5)
    }

    /// Output pixels per frame pixel (crops never scale much).
    fn scale(&self) -> f64 {
        1.0 / self.ex.0.hypot(self.ex.1).max(1e-9)
    }
}

fn smoothstep(e0: f64, e1: f64, x: f64) -> f64 {
    if e1 <= e0 {
        return if x < e0 { 0.0 } else { 1.0 };
    }
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// What a correction's mask needs from the image.
struct Reference<'a> {
    /// Perceptual luminance 0..1 per output pixel.
    luminance: Option<&'a [f32]>,
    /// OkLab per output pixel.
    lab: Option<&'a [[f32; 3]]>,
    width: usize,
    height: usize,
    strengths: MaskStrengths,
}

/// Draw one shape into a plane of the output's size.
fn draw_shape(shape: &Shape, frame: &Frame, r: &Reference) -> Vec<f32> {
    let (w, h) = (r.width, r.height);
    let mut plane = vec![0.0f32; w * h];
    let per_pixel = |plane: &mut Vec<f32>, f: &(dyn Fn(f64, f64) -> f64 + Sync)| {
        plane.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
            for (x, v) in row.iter_mut().enumerate() {
                let (fx, fy) = frame.source(x as f64, y as f64);
                *v = f(fx, fy) as f32;
            }
        });
    };
    match shape {
        Shape::Linear { zero, full } => {
            let z = frame.pixel(zero.0, zero.1);
            let f = frame.pixel(full.0, full.1);
            let d = (f.0 - z.0, f.1 - z.1);
            let len2 = d.0 * d.0 + d.1 * d.1;
            if len2 < 1e-12 {
                return plane;
            }
            per_pixel(&mut plane, &|fx, fy| {
                let t = ((fx - z.0) * d.0 + (fy - z.1) * d.1) / len2;
                smoothstep(0.0, 1.0, t)
            });
        }
        Shape::Radial {
            top,
            left,
            bottom,
            right,
            angle,
            feather,
        } => {
            let c = frame.pixel((left + right) / 2.0, (top + bottom) / 2.0);
            let a = ((right - left).abs() / 2.0 * frame.width).max(1e-6);
            let b = ((bottom - top).abs() / 2.0 * frame.height).max(1e-6);
            // Positive angles turn the ellipse counter-clockwise on screen (y down).
            let t = angle.to_radians();
            let (cos, sin) = (t.cos(), t.sin());
            let inner = 1.0 - feather / 100.0;
            per_pixel(&mut plane, &|fx, fy| {
                let (dx, dy) = (fx - c.0, fy - c.1);
                let u = dx * cos - dy * sin;
                let v = dx * sin + dy * cos;
                let d = ((u / a).powi(2) + (v / b).powi(2)).sqrt();
                1.0 - smoothstep(inner, 1.0, d)
            });
        }
        Shape::Brush(strokes) => {
            let mut stroke_plane = vec![0.0f32; w * h];
            for s in strokes {
                stroke_plane.iter_mut().for_each(|v| *v = 0.0);
                let bounds = paint_stroke(&mut stroke_plane, w, h, frame, s);
                let Some((x0, y0, x1, y1)) = bounds else {
                    continue;
                };
                for y in y0..y1 {
                    for x in x0..x1 {
                        let i = y * w + x;
                        let c = stroke_plane[i];
                        if s.erase {
                            plane[i] *= 1.0 - c;
                        } else {
                            plane[i] = plane[i].max(s.density as f32 * c);
                        }
                    }
                }
            }
        }
        Shape::Luminance(range) => {
            if let Some(l) = r.luminance {
                let [a, b, c, d] = *range;
                plane.par_iter_mut().zip(l.par_iter()).for_each(|(v, &y)| {
                    let y = y as f64;
                    // Equal edges are hard: in from `a`, out after `d`.
                    let rise = if b <= a {
                        f64::from(u8::from(y >= a))
                    } else {
                        smoothstep(a, b, y)
                    };
                    let fall = if d <= c {
                        f64::from(u8::from(y <= d))
                    } else {
                        1.0 - smoothstep(c, d, y)
                    };
                    *v = (rise * fall) as f32;
                });
            }
        }
        Shape::Color { samples, amount } => {
            if let Some(lab) = r.lab {
                draw_color_range(&mut plane, lab, w, h, frame, samples, *amount, &r.strengths);
            }
        }
        Shape::Unsupported => {}
    }
    plane
}

/// Paint one stroke's coverage; returns the touched output rectangle.
fn paint_stroke(
    plane: &mut [f32],
    w: usize,
    h: usize,
    frame: &Frame,
    s: &Stroke,
) -> Option<(usize, usize, usize, usize)> {
    let long = frame.width.max(frame.height);
    let r = s.radius * long * frame.scale();
    if r.is_nan() || r <= 0.0 || s.flow <= 0.0 {
        return None;
    }
    let flow = s.flow as f32;
    let inner = s.hardness.clamp(0.0, 0.999);
    let mut bounds: Option<(usize, usize, usize, usize)> = None;
    for &(u, v) in &s.dabs {
        let (fx, fy) = frame.pixel(u, v);
        let (cx, cy) = frame.target(fx, fy);
        let x0 = (cx - r).floor().max(0.0) as usize;
        let y0 = (cy - r).floor().max(0.0) as usize;
        let x1 = ((cx + r).ceil() + 1.0).clamp(0.0, w as f64) as usize;
        let y1 = ((cy + r).ceil() + 1.0).clamp(0.0, h as f64) as usize;
        if x0 >= x1 || y0 >= y1 {
            continue;
        }
        bounds = Some(match bounds {
            Some((a, b, c, d)) => (a.min(x0), b.min(y0), c.max(x1), d.max(y1)),
            None => (x0, y0, x1, y1),
        });
        plane[y0 * w..y1 * w]
            .par_chunks_mut(w)
            .enumerate()
            .for_each(|(j, row)| {
                let y = (y0 + j) as f64;
                for (x, c) in row.iter_mut().enumerate().take(x1).skip(x0) {
                    let d = ((x as f64 - cx).hypot(y - cy)) / r;
                    if d >= 1.0 {
                        continue;
                    }
                    let a = flow * (1.0 - smoothstep(inner, 1.0, d)) as f32;
                    *c += (1.0 - *c) * a;
                }
            });
    }
    bounds
}

#[allow(clippy::too_many_arguments)]
fn draw_color_range(
    plane: &mut [f32],
    lab: &[[f32; 3]],
    w: usize,
    h: usize,
    frame: &Frame,
    samples: &[(f64, f64)],
    amount: f64,
    k: &MaskStrengths,
) {
    if w == 0 || h == 0 {
        return;
    }
    // The mean colour around each sample (5×5 pixels).
    let targets: Vec<[f32; 3]> = samples
        .iter()
        .map(|&(u, v)| {
            let (fx, fy) = frame.pixel(u, v);
            let (x, y) = frame.target(fx, fy);
            let x = x.round().clamp(0.0, (w - 1) as f64) as usize;
            let y = y.round().clamp(0.0, (h - 1) as f64) as usize;
            let mut sum = [0.0f32; 3];
            let mut n = 0.0f32;
            for yy in y.saturating_sub(2)..(y + 3).min(h) {
                for xx in x.saturating_sub(2)..(x + 3).min(w) {
                    let p = lab[yy * w + xx];
                    for c in 0..3 {
                        sum[c] += p[c];
                    }
                    n += 1.0;
                }
            }
            sum.map(|s| s / n)
        })
        .collect();
    let tol =
        (k.color_tolerance_min + (k.color_tolerance_max - k.color_tolerance_min) * amount) as f32;
    plane.par_iter_mut().zip(lab.par_iter()).for_each(|(v, p)| {
        let mut best = 0.0f32;
        for t in &targets {
            let d = ((0.5 * (p[0] - t[0])).powi(2) + (p[1] - t[1]).powi(2) + (p[2] - t[2]).powi(2))
                .sqrt();
            let m = 1.0 - smoothstep(0.5 * tol as f64, tol as f64, d as f64) as f32;
            best = best.max(m);
        }
        *v = best;
    });
}

/// A correction's final alpha plane.
fn draw_correction(c: &Correction, frame: &Frame, r: &Reference) -> Vec<f32> {
    let mut alpha = vec![0.0f32; r.width * r.height];
    for part in &c.parts {
        let mut p = draw_shape(&part.shape, frame, r);
        let (_, _, inverted) = part.combination.stored();
        if inverted {
            p.par_iter_mut().for_each(|v| *v = 1.0 - *v);
        }
        match part.combination.mode {
            Mode::Add => {
                let o = part.opacity as f32;
                alpha
                    .par_iter_mut()
                    .zip(p.par_iter())
                    .for_each(|(a, &v)| *a = a.max(v * o));
            }
            Mode::Subtract | Mode::Intersect => {
                alpha
                    .par_iter_mut()
                    .zip(p.par_iter())
                    .for_each(|(a, &v)| *a *= 1.0 - v);
            }
        }
    }
    if let Some((shape, invert)) = &c.range {
        let p = draw_shape(shape, frame, r);
        alpha.par_iter_mut().zip(p.par_iter()).for_each(|(a, &v)| {
            *a *= if *invert { 1.0 - v } else { v };
        });
    }
    alpha
}

// ---------------------------------------------------------------- applying

/// Masks drawn for one render, with the adjustments they carry.
pub struct Drawn {
    planes: Vec<(LocalAdjustments, Vec<f32>)>,
    overlay: Option<Vec<f32>>,
}

/// Draw every mask for an image at the end of the scene-linear stage.
/// `frame` is the uncropped frame's size, `crop` the output → frame map,
/// `encode` maps scene-linear luminance to the perceptual 0..1 scale range
/// masks select on.
pub fn draw(
    masks: &Masks,
    img: &Image,
    frame: (usize, usize),
    crop: Option<&CropGeometry>,
    encode: impl Fn(f32) -> f32 + Sync,
    strengths: &MaskStrengths,
) -> Drawn {
    let frame = Frame::new(frame, crop);
    let lw = luminance_weights();
    let luminance: Option<Vec<f32>> =
        masks.needs(|s| matches!(s, Shape::Luminance(_))).then(|| {
            img.rgb
                .par_chunks(3)
                .map(|p| encode((lw[0] * p[0] + lw[1] * p[1] + lw[2] * p[2]).max(0.0)))
                .collect()
        });
    let lab: Option<Vec<[f32; 3]>> = masks.needs(|s| matches!(s, Shape::Color { .. })).then(|| {
        let ok = OkLab::new();
        img.rgb
            .par_chunks(3)
            .map(|p| {
                // Compare colours as they look: tone-encode the light first.
                let y = (lw[0] * p[0] + lw[1] * p[1] + lw[2] * p[2]).max(1e-6);
                let s = encode(y) / y;
                ok.from_working([p[0] * s, p[1] * s, p[2] * s])
            })
            .collect()
    });
    let reference = Reference {
        luminance: luminance.as_deref(),
        lab: lab.as_deref(),
        width: img.width,
        height: img.height,
        strengths: *strengths,
    };
    let mut planes = Vec::new();
    let mut overlay = None;
    for c in &masks.corrections {
        let shown = masks.overlay.is_some() && masks.overlay == c.index;
        if !shown && c.adjust.is_neutral() {
            continue;
        }
        let alpha = draw_correction(c, &frame, &reference);
        if shown {
            overlay = Some(alpha.clone());
        }
        if !c.adjust.is_neutral() {
            planes.push((c.adjust, alpha));
        }
    }
    Drawn { planes, overlay }
}

/// Blend `alt` into `img` through `alpha`.
fn blend(img: &mut Image, alt: &Image, alpha: &[f32]) {
    img.rgb
        .par_chunks_mut(3)
        .zip(alt.rgb.par_chunks(3))
        .zip(alpha.par_iter())
        .for_each(|((p, q), &a)| {
            if a > 0.0 {
                for c in 0..3 {
                    p[c] += (q[c] - p[c]) * a;
                }
            }
        });
}

impl Drawn {
    fn each(
        &self,
        active: impl Fn(&LocalAdjustments) -> bool,
    ) -> impl Iterator<Item = &(LocalAdjustments, Vec<f32>)> {
        self.planes.iter().filter(move |(a, _)| active(a))
    }

    /// Exposure, Temp and Tint on scene-linear ProPhoto.
    pub fn apply_light(&self, img: &mut Image, k: &MaskStrengths) {
        let lw = luminance_weights();
        for (a, alpha) in self.each(|a| a.exposure != 0.0 || a.temperature != 0.0 || a.tint != 0.0)
        {
            let t = a.temperature / 100.0;
            let g = [
                (k.temperature * t).exp(),
                (-k.tint * a.tint / 100.0).exp(),
                (-k.temperature * t).exp(),
            ];
            // Keep luminance: a colour shift, not a brightness one.
            let norm = lw[0] as f64 * g[0] + lw[1] as f64 * g[1] + lw[2] as f64 * g[2];
            let log_gain =
                g.map(|v| ((v / norm).ln() + a.exposure * std::f64::consts::LN_2) as f32);
            img.rgb
                .par_chunks_mut(3)
                .zip(alpha.par_iter())
                .for_each(|(p, &m)| {
                    if m > 0.0 {
                        for c in 0..3 {
                            p[c] *= (log_gain[c] * m).exp();
                        }
                    }
                });
        }
    }

    pub fn apply_dehaze(&self, img: &mut Image, cal: &Calibration) {
        for (a, alpha) in self.each(|a| a.dehaze != 0.0) {
            let mut alt = img.clone();
            dehaze::apply(&mut alt, a.dehaze, &cal.dehaze);
            blend(img, &alt, alpha);
        }
    }

    /// Contrast, Highlights, Shadows, Whites, Blacks on display-linear light,
    /// hue-preserving like the global ones.
    pub fn apply_tone(&self, img: &mut Image, cal: &Calibration, lut_size: usize) {
        let lw = luminance_weights();
        for (a, alpha) in self.each(|a| !a.tone().is_neutral()) {
            let lut = tone::tone_lut(&a.tone(), &cal.tone, lut_size);
            img.rgb
                .par_chunks_mut(3)
                .zip(alpha.par_iter())
                .for_each(|(p, &m)| {
                    if m <= 0.0 {
                        return;
                    }
                    let y = lw[0] * p[0] + lw[1] * p[1] + lw[2] * p[2];
                    if y > 1e-6 {
                        let yc = y.min(1.0);
                        let target =
                            local::srgb_decode(local::lookup(&lut, local::srgb_encode(yc)));
                        let scale = (target / yc).max(0.0).powf(m);
                        for v in p.iter_mut() {
                            *v *= scale;
                        }
                    }
                });
        }
    }

    pub fn apply_local_contrast(&self, img: &mut Image, cal: &Calibration) {
        for (a, alpha) in self.each(|a| a.texture != 0.0 || a.clarity != 0.0) {
            let mut alt = img.clone();
            let p = LocalContrast {
                texture: a.texture,
                clarity: a.clarity,
            };
            local::apply(&mut alt, &p, &cal.local);
            blend(img, &alt, alpha);
        }
    }

    /// Saturation and the Color tint, in OkLab.
    pub fn apply_color(&self, img: &mut Image, k: &MaskStrengths) {
        let ok = OkLab::new();
        for (a, alpha) in self.each(|a| a.saturation != 0.0 || a.toning_saturation != 0.0) {
            let sat = (a.saturation / 100.0) as f32;
            // Lightroom's hue 0 is red, about 29° in OkLab.
            let h = (a.toning_hue + 29.0).to_radians();
            let tint = (k.toning * a.toning_saturation / 100.0) as f32;
            let (ta, tb) = (tint * h.cos() as f32, tint * h.sin() as f32);
            img.rgb
                .par_chunks_mut(3)
                .zip(alpha.par_iter())
                .for_each(|(p, &m)| {
                    if m <= 0.0 {
                        return;
                    }
                    let mut lab = ok.from_working([p[0], p[1], p[2]]);
                    let s = (1.0 + sat * m).max(0.0);
                    lab[1] = lab[1] * s + ta * m;
                    lab[2] = lab[2] * s + tb * m;
                    let rgb = ok.to_working(lab);
                    p.copy_from_slice(&rgb);
                });
        }
    }

    /// Local Sharpness (added sharpening, or a soft blur below 0) and Noise.
    pub fn apply_detail(&self, img: &mut Image, cal: &Calibration) {
        let k = &cal.masks;
        for (a, alpha) in self.each(|a| a.noise > 0.0) {
            let mut alt = img.clone();
            let nr = NoiseReduction {
                luminance: a.noise,
                luminance_detail: 50.0,
                luminance_contrast: 0.0,
                color: 0.0,
                color_detail: 50.0,
                color_smoothness: 50.0,
            };
            detail::apply_noise_reduction(&mut alt, &nr, &cal.detail);
            blend(img, &alt, alpha);
        }
        for (a, alpha) in self.each(|a| a.sharpness != 0.0) {
            let mut alt = img.clone();
            if a.sharpness > 0.0 {
                let s = Sharpening {
                    amount: k.sharpness * a.sharpness / 100.0,
                    radius: 1.0,
                    detail: 25.0,
                    masking: 0.0,
                };
                detail::apply_sharpening(&mut alt, &s, &cal.detail);
            } else {
                let sigma = (k.soften_sigma * -a.sharpness / 100.0) as f32
                    * reference_scale(img.width, img.height);
                let (w, h) = (img.width, img.height);
                for c in 0..3 {
                    let plane: Vec<f32> = img.rgb.iter().skip(c).step_by(3).copied().collect();
                    let blurred = gaussian_blur(&plane, w, h, sigma.max(0.3));
                    for (i, v) in blurred.into_iter().enumerate() {
                        alt.rgb[i * 3 + c] = v;
                    }
                }
            }
            blend(img, &alt, alpha);
        }
    }

    /// Tint the shown mask red, as Lightroom's overlay does.
    pub fn apply_overlay(&self, img: &mut Image) {
        let Some(alpha) = &self.overlay else { return };
        let red = [0.55f32, 0.04, 0.04];
        img.rgb
            .par_chunks_mut(3)
            .zip(alpha.par_iter())
            .for_each(|(p, &m)| {
                let a = 0.6 * m;
                for c in 0..3 {
                    p[c] += (red[c] - p[c]) * a;
                }
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(s: &str) -> Value {
        Value::Text(s.into())
    }

    fn st(pairs: &[(&str, Value)]) -> Value {
        Value::Struct(
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect(),
        )
    }

    fn settings(corrections: Vec<Value>) -> BTreeMap<String, Value> {
        [(MASK_TAG.to_string(), Value::Seq(corrections))]
            .into_iter()
            .collect()
    }

    fn reference(w: usize, h: usize) -> Reference<'static> {
        Reference {
            luminance: None,
            lab: None,
            width: w,
            height: h,
            strengths: MaskStrengths::default(),
        }
    }

    fn alpha_of(s: &BTreeMap<String, Value>, w: usize, h: usize) -> Vec<f32> {
        let (m, _) = Masks::from_settings(s);
        draw_correction(
            &m.corrections[0],
            &Frame::new((w, h), None),
            &reference(w, h),
        )
    }

    fn linear(zy: &str, fy: &str) -> Value {
        st(&[
            ("What", text("Mask/Gradient")),
            ("MaskBlendMode", text("0")),
            ("MaskValue", text("1")),
            ("ZeroX", text("0.5")),
            ("ZeroY", text(zy)),
            ("FullX", text("0.5")),
            ("FullY", text(fy)),
        ])
    }

    fn correction(parts: Vec<Value>) -> Value {
        st(&[
            ("What", text("Correction")),
            ("CorrectionAmount", text("1")),
            ("LocalExposure2012", text("0.25")),
            ("CorrectionMasks", Value::Seq(parts)),
        ])
    }

    #[test]
    fn linear_gradient_runs_from_zero_to_full() {
        // Full at the top, nothing below the middle.
        let a = alpha_of(
            &settings(vec![correction(vec![linear("0.5", "0.1")])]),
            10,
            100,
        );
        assert!(a[5 * 10 + 5] > 0.99, "top {}", a[55]);
        assert!(a[60 * 10 + 5] < 1e-6);
        let mid = a[30 * 10 + 5];
        assert!(mid > 0.3 && mid < 0.7, "mid {mid}");
    }

    #[test]
    fn radial_inside_and_inverted() {
        let radial = |inverted: &str, flipped: &str| {
            st(&[
                ("What", text("Mask/CircularGradient")),
                ("MaskBlendMode", text("0")),
                ("MaskValue", text("1")),
                ("MaskInverted", text(inverted)),
                ("Flipped", text(flipped)),
                ("Top", text("0.25")),
                ("Left", text("0.25")),
                ("Bottom", text("0.75")),
                ("Right", text("0.75")),
                ("Feather", text("0")),
            ])
        };
        let a = alpha_of(
            &settings(vec![correction(vec![radial("false", "true")])]),
            40,
            40,
        );
        assert_eq!(a[20 * 40 + 20], 1.0);
        assert_eq!(a[2 * 40 + 2], 0.0);
        let a = alpha_of(
            &settings(vec![correction(vec![radial("true", "false")])]),
            40,
            40,
        );
        assert_eq!(a[20 * 40 + 20], 0.0);
        assert_eq!(a[2 * 40 + 2], 1.0);
    }

    #[test]
    fn subtract_and_intersect() {
        // A: the top half (hard-ish), B: the left half.
        let left = st(&[
            ("What", text("Mask/Gradient")),
            ("ZeroX", text("0.55")),
            ("ZeroY", text("0.5")),
            ("FullX", text("0.45")),
            ("FullY", text("0.5")),
        ]);
        let with = |mode: &str, value: &str, inverted: &str| {
            let mut b = left.clone();
            if let Value::Struct(m) = &mut b {
                m.insert("MaskBlendMode".into(), text(mode));
                m.insert("MaskValue".into(), text(value));
                m.insert("MaskInverted".into(), text(inverted));
            }
            settings(vec![correction(vec![linear("0.55", "0.45"), b])])
        };
        let (w, h) = (40, 40);
        let at = |a: &[f32], x: usize, y: usize| a[y * w + x];
        let sub = alpha_of(&with("1", "0", "false"), w, h);
        assert!(at(&sub, 35, 5) > 0.99, "top right kept");
        assert!(at(&sub, 5, 5) < 0.01, "top left removed");
        let inter = alpha_of(&with("1", "0", "true"), w, h);
        assert!(at(&inter, 5, 5) > 0.99, "top left kept");
        assert!(at(&inter, 35, 5) < 0.01, "top right removed");
        assert!(at(&inter, 5, 35) < 0.01, "bottom never in");
    }

    #[test]
    fn brush_paints_and_erases() {
        let stroke = |value: &str, dabs: &[&str]| {
            st(&[
                ("What", text("Mask/Paint")),
                ("MaskSyncID", text("S")),
                ("MaskValue", text(value)),
                ("Radius", text("0.1")),
                ("Flow", text("1")),
                ("CenterWeight", text("0.5")),
                ("Dabs", Value::Seq(dabs.iter().map(|d| text(d)).collect())),
            ])
        };
        let s = settings(vec![correction(vec![
            stroke("1", &["d 0.25 0.5", "d 0.5 0.5", "d 0.75 0.5"]),
            stroke("0", &["d 0.75 0.5"]),
        ])]);
        let (w, h) = (100, 100);
        let a = alpha_of(&s, w, h);
        assert!(a[50 * w + 50] > 0.99, "painted");
        assert!(a[50 * w + 75] < 0.01, "erased");
        assert!(a[10 * w + 50] < 1e-6, "untouched");
    }

    #[test]
    fn crop_moves_the_mask_with_the_photo() {
        // The right half of a 100×10 frame, as a crop: the gradient is
        // defined on the whole frame, so the crop sees only its full half.
        let crop = crate::crop::Crop {
            enabled: true,
            left: 0.5,
            right: 1.0,
            ..Default::default()
        };
        let g = crop.geometry(100, 10).unwrap();
        let s = settings(vec![correction(vec![st(&[
            ("What", text("Mask/Gradient")),
            ("ZeroX", text("0.4")),
            ("ZeroY", text("0.5")),
            ("FullX", text("0.5")),
            ("FullY", text("0.5")),
        ])])]);
        let (m, _) = Masks::from_settings(&s);
        let r = reference(g.width, g.height);
        let a = draw_correction(&m.corrections[0], &Frame::new((100, 10), Some(&g)), &r);
        assert!(a.iter().all(|&v| v > 0.99));
    }

    #[test]
    fn luminance_range_selects_bright_pixels() {
        let range = st(&[
            ("What", text("Mask/RangeMask")),
            (
                "CorrectionRangeMask",
                st(&[
                    ("Version", text("3")),
                    ("Type", text("2")),
                    ("LumRange", text("0.5 0.6 1 1")),
                ]),
            ),
        ]);
        let (m, notes) = Masks::from_settings(&settings(vec![correction(vec![range])]));
        assert!(notes.is_empty(), "{notes:?}");
        let lum: Vec<f32> = (0..10).map(|i| i as f32 / 9.0).collect();
        let r = Reference {
            luminance: Some(&lum),
            lab: None,
            width: 10,
            height: 1,
            strengths: MaskStrengths::default(),
        };
        let a = draw_correction(&m.corrections[0], &Frame::new((10, 1), None), &r);
        assert_eq!(a[0], 0.0);
        assert_eq!(a[9], 1.0);
        assert!(a[4] < 1e-6 && a[6] > 0.99, "{a:?}");
    }

    #[test]
    fn ai_masks_select_nothing_and_are_reported() {
        let ai = st(&[("What", text("Mask/Image")), ("MaskSubType", text("2"))]);
        let s = settings(vec![correction(vec![ai])]);
        let (m, notes) = Masks::from_settings(&s);
        assert_eq!(notes.len(), 1, "{notes:?}");
        assert!(notes[0].contains("Select Sky"));
        let a = draw_correction(
            &m.corrections[0],
            &Frame::new((4, 4), None),
            &reference(4, 4),
        );
        assert!(a.iter().all(|&v| v == 0.0));
    }

    #[test]
    fn local_exposure_is_stored_in_quarter_stops() {
        let (m, _) = Masks::from_settings(&settings(vec![correction(vec![linear("1", "0")])]));
        assert_eq!(m.corrections[0].adjust.exposure, 1.0);
    }

    #[test]
    fn exposure_brightens_only_under_the_mask() {
        let s = settings(vec![correction(vec![linear("0.55", "0.45")])]);
        let (m, _) = Masks::from_settings(&s);
        let mut img = Image::from_fn(4, 20, |_, _| [0.2, 0.2, 0.2]);
        let drawn = draw(&m, &img, (4, 20), None, |v| v, &MaskStrengths::default());
        drawn.apply_light(&mut img, &MaskStrengths::default());
        assert!((img.get(1, 1)[0] - 0.4).abs() < 1e-4, "{:?}", img.get(1, 1));
        assert!((img.get(1, 18)[0] - 0.2).abs() < 1e-6);
    }

    #[test]
    fn inactive_corrections_and_disabled_panels_are_skipped() {
        let mut c = correction(vec![linear("1", "0")]);
        if let Value::Struct(m) = &mut c {
            m.insert("CorrectionActive".into(), text("false"));
        }
        assert!(Masks::from_settings(&settings(vec![c])).0.is_empty());
        let mut s = settings(vec![correction(vec![linear("1", "0")])]);
        s.insert(MASK_ENABLE_TAG.into(), Value::Bool(false));
        assert!(Masks::from_settings(&s).0.is_empty());
    }
}
