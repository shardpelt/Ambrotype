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

mod apply;
mod geometry;
mod sky;

pub use apply::{Drawn, draw, draw_in, reach};
pub(crate) use geometry::Frame;
use geometry::{Reference, Targets, draw_correction};
use sky::{sky_colour, sky_map};

use std::collections::BTreeMap;

use amb_presets::Value;
use amb_presets::masks::{
    self, Combination, LEGACY_TAGS, MASK_ENABLE_TAG, MASK_TAG, MaskKind, Mode, flag, num,
};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::brush;
use crate::buffer::{Image, Tile, luminance_weights};
use crate::calibration::Calibration;
use crate::color::OkLab;
use crate::crop::CropGeometry;
use crate::detail::{NoiseReduction, Sharpening};
use crate::filters::{gaussian_blur, gaussian_reach};
use crate::lens::{self, Defringe};
use crate::local::LocalContrast;
use crate::math::smoothstep_f64 as smoothstep;
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
    /// Defringe Purple and Green Amount (0..20) at local Defringe +100.
    pub defringe: f64,
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
            defringe: 10.0,
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
    pub defringe: f64,
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
            defringe: scaled("LocalDefringe"),
        }
    }

    pub(crate) fn tone(&self) -> ToneControls {
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
    Brush(Vec<brush::Stroke>),
    /// `LumRange`: none below [0], full from [1] to [2], none above [3].
    Luminance([f64; 4]),
    /// Sampled colours (image fractions) and Refine 0..1.
    Color {
        samples: Vec<(f64, f64)>,
        amount: f64,
    },
    /// Select Sky (`Mask/Image` subtype 2), found by our own colour and
    /// texture rules ([`sky_map`]), not Adobe's model.
    Sky,
    /// Select Subject (`Mask/Image` subtype 1), from the app's subject
    /// model ([`crate::subject`]).
    /// From a model (`subject::Finder`): the main subject or the people.
    Model(crate::subject::MapKind),
    /// Something we can't draw (other AI masks, depth, Lightroom's own
    /// colour samples): it selects nothing.
    Unsupported,
}

/// A coarse mask (`w`×`h`) snapped to the edges of the photo under it
/// (`rgb`, tone-encoded sRGB): a guided filter on its brightness, with a
/// radius of about 1.5 cells of the subject model's grid, then a gentle
/// tightening so the edge is crisp but not stepped. The filter's
/// coefficients are worked out on a copy shrunk by `step` and applied at
/// full size (He & Sun's fast guided filter), so it costs little at 24 MP.
fn refine_edges(plane: &[f32], rgb: &[f32], w: usize, h: usize, t: &Tile, grid: usize) -> Vec<f32> {
    use crate::filters::box_blur;
    let cell = t.full_width.max(t.full_height) as f32 / grid.max(1) as f32;
    let radius = (1.5 * cell).round().clamp(1.0, 64.0) as usize;
    if w < 2 * radius + 1 || h < 2 * radius + 1 {
        return plane.to_vec();
    }
    let guide: Vec<f32> = rgb
        .par_chunks(3)
        .map(|p| 0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2])
        .collect();
    let step = (radius / 4).max(1);
    let (sw, sh) = (w.div_ceil(step), h.div_ceil(step));
    let shrink = |src: &[f32]| -> Vec<f32> {
        (0..sw * sh)
            .into_par_iter()
            .map(|k| {
                let (sx, sy) = (k % sw, k / sw);
                let (x1, y1) = (((sx + 1) * step).min(w), ((sy + 1) * step).min(h));
                let mut sum = 0.0;
                for y in sy * step..y1 {
                    sum += src[y * w + sx * step..y * w + x1].iter().sum::<f32>();
                }
                sum / ((x1 - sx * step) * (y1 - sy * step)) as f32
            })
            .collect()
    };
    let (gi, gp) = (shrink(&guide), shrink(plane));
    let r = (radius / step).max(1);
    let mean = |p: &[f32]| box_blur(p, sw, sh, r);
    let (mi, mp) = (mean(&gi), mean(&gp));
    let ii: Vec<f32> = gi.iter().map(|v| v * v).collect();
    let ip: Vec<f32> = gi.iter().zip(&gp).map(|(a, b)| a * b).collect();
    let (cii, cip) = (mean(&ii), mean(&ip));
    let (a, b): (Vec<f32>, Vec<f32>) = (0..sw * sh)
        .map(|k| {
            let a = (cip[k] - mi[k] * mp[k]) / (cii[k] - mi[k] * mi[k] + 1e-3);
            (a, mp[k] - a * mi[k])
        })
        .unzip();
    let (a, b) = (mean(&a), mean(&b));
    // Bilinear on the small grid, at each full-size pixel.
    let at = |g: &[f32], x: usize, y: usize| -> f32 {
        let fx = ((x as f32 + 0.5) / step as f32 - 0.5).clamp(0.0, (sw - 1) as f32);
        let fy = ((y as f32 + 0.5) / step as f32 - 0.5).clamp(0.0, (sh - 1) as f32);
        let (x0, y0) = (fx as usize, fy as usize);
        let (x1, y1) = ((x0 + 1).min(sw - 1), (y0 + 1).min(sh - 1));
        let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
        let top = g[y0 * sw + x0] + (g[y0 * sw + x1] - g[y0 * sw + x0]) * tx;
        let bottom = g[y1 * sw + x0] + (g[y1 * sw + x1] - g[y1 * sw + x0]) * tx;
        top + (bottom - top) * ty
    };
    let mut out = vec![0.0f32; w * h];
    out.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
        for (x, o) in row.iter_mut().enumerate() {
            let v = at(&a, x, y) * guide[y * w + x] + at(&b, x, y);
            // A soft step around the middle: firmer inside and outside.
            let s = ((v - 0.5) * 1.6 + 0.5).clamp(0.0, 1.0);
            *o = s * s * (3.0 - 2.0 * s);
        }
    });
    out
}

/// Where the sky is, in fractions of the output (a small grid, 0..1).
#[derive(Debug, Clone, PartialEq)]
pub struct SkyMap {
    pub width: usize,
    pub height: usize,
    pub data: Vec<f32>,
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
    /// The mask's own point curves (`MainCurve`, `RedCurve`, ...).
    pub curves: Option<std::sync::Arc<LocalCurves>>,
}

/// A mask's point curves: the RGB curve, then per channel, on encoded values
/// like the global point curve.
#[derive(Debug, Clone)]
pub struct LocalCurves {
    pub main: Option<crate::curve::Spline>,
    pub channels: [Option<crate::curve::Spline>; 3],
}

impl PartialEq for LocalCurves {
    /// Curves that give the same values (sampled) are equal.
    fn eq(&self, other: &Self) -> bool {
        let same = |a: &Option<crate::curve::Spline>, b: &Option<crate::curve::Spline>| match (a, b)
        {
            (Some(a), Some(b)) => a.lut(33) == b.lut(33),
            (None, None) => true,
            _ => false,
        };
        same(&self.main, &other.main)
            && self
                .channels
                .iter()
                .zip(&other.channels)
                .all(|(a, b)| same(a, b))
    }
}

impl LocalCurves {
    fn read(c: &BTreeMap<String, Value>) -> Option<LocalCurves> {
        let spline = |tag: &str| {
            let pts = amb_presets::curve_points(c.get(tag)?)?;
            // Points in 0..255 as the global curve; a curve given in 0..1 is scaled.
            let unit = pts.iter().all(|(x, y)| *x <= 1.0 && *y <= 1.0);
            let pts: Vec<(f64, f64)> = if unit {
                pts.iter().map(|(x, y)| (x * 255.0, y * 255.0)).collect()
            } else {
                pts
            };
            crate::curve::Spline::from_points_255(&pts).filter(|s| !s.is_identity())
        };
        let curves = LocalCurves {
            main: spline("MainCurve"),
            channels: [
                spline("RedCurve"),
                spline("GreenCurve"),
                spline("BlueCurve"),
            ],
        };
        (curves.main.is_some() || curves.channels.iter().any(Option::is_some)).then_some(curves)
    }
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

    /// The output pixels colour range masks sample, per colour shape in the
    /// order [`draw_in`] measures them. `frame` is the uncropped frame's
    /// size and `crop` the output → frame map.
    pub fn colour_sample_pixels(
        &self,
        frame: (usize, usize),
        crop: Option<&CropGeometry>,
    ) -> Vec<Vec<(f64, f64)>> {
        let frame = Frame::new(frame, crop);
        let mut out = Vec::new();
        for c in &self.corrections {
            let shown = self.overlay.is_some() && self.overlay == c.index;
            if !shown && c.adjust.is_neutral() && c.curves.is_none() {
                continue;
            }
            let shapes = c.parts.iter().map(|p| &p.shape);
            for shape in shapes.chain(c.range.iter().map(|(s, _)| s)) {
                if let Shape::Color { samples, .. } = shape {
                    out.push(
                        samples
                            .iter()
                            .map(|&(u, v)| {
                                let (fx, fy) = frame.pixel(u, v);
                                frame.target(fx, fy)
                            })
                            .collect(),
                    );
                }
            }
        }
        out
    }

    /// Whether a colour range mask selects colours (sampled over the photo).
    pub fn has_colour_range(&self) -> bool {
        self.needs(|s| matches!(s, Shape::Color { .. }))
    }

    /// Sky or subject masks: found on the whole photo, so a region render
    /// needs the maps from a whole-photo pass.
    pub fn has_photo_maps(&self) -> bool {
        self.needs(|s| matches!(s, Shape::Sky | Shape::Model(_)))
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
            notes.push(if kind == MaskKind::Brush && masks::uses_brush_table(first) {
                format!("{tag}: brush in \"{name}\" (its strokes are in Lightroom's .acr file, not read yet)")
            } else {
                format!("{tag}: {} mask in \"{name}\"", kind.label())
            });
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
        curves: LocalCurves::read(c).map(std::sync::Arc::new),
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
            let strokes = masks::brush_strokes(entries);
            if strokes.is_empty() {
                return Shape::Unsupported;
            }
            // In an aggregate each stroke's MaskValue is its density, and a
            // zero one erases. Old files keep the combination on the strokes,
            // so a stroke whose zero-ness differs from the first one's erases.
            let aggregate = masks::text(e, "What").as_deref() == Some("Mask/Aggregate");
            let first_zero = !aggregate && f("MaskValue", 1.0) == 0.0;
            Shape::Brush(
                strokes
                    .iter()
                    .map(|s| {
                        let value = num(s, "MaskValue").unwrap_or(1.0).clamp(0.0, 1.0);
                        let erase = (value == 0.0) != first_zero;
                        let density = if erase || first_zero { 1.0 } else { value };
                        brush::stroke(s, density, erase)
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
        MaskKind::Ai(sub) if sub == "2" => Shape::Sky,
        MaskKind::Ai(sub) if sub == "1" && crate::subject::available() => {
            Shape::Model(crate::subject::MapKind::Subject)
        }
        // Lightroom keeps one part per person (and per face or body part);
        // a part without those details is everyone, which is what we draw.
        MaskKind::Ai(sub) if sub == "3" && crate::subject::available() => {
            Shape::Model(crate::subject::MapKind::People)
        }
        _ => Shape::Unsupported,
    }
}

#[cfg(test)]
mod tests;
