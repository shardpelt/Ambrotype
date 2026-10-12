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

use crate::brush;
use crate::buffer::{Image, Tile, luminance_weights};
use crate::calibration::Calibration;
use crate::color::OkLab;
use crate::crop::CropGeometry;
use crate::detail::{NoiseReduction, Sharpening};
use crate::filters::{gaussian_blur, gaussian_reach};
use crate::lens::{self, Defringe};
use crate::local::LocalContrast;
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
    /// Something we can't draw (other AI masks, depth, Lightroom's own
    /// colour samples): it selects nothing.
    Unsupported,
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
        _ => Shape::Unsupported,
    }
}

// ---------------------------------------------------------------- sky

/// How sky-like a colour (OkLab of the tone-encoded light) is: bright, and
/// blue, cyan or near neutral. Foliage, earth and warm colours have a
/// positive b (yellow) and dark ones a low L, so they aren't sky.
fn sky_colour(lab: [f32; 3]) -> f32 {
    let [l, _, b] = lab.map(f64::from);
    let bright = smoothstep(0.38, 0.62, l);
    let not_yellow = 1.0 - smoothstep(0.0, 0.05, b);
    (bright * not_yellow) as f32
}

/// **Ours, not Adobe's:** the sky as smooth, bright, blue or neutral areas
/// joined to the top edge, on a grid of at most 128 cells a side. Each cell
/// scores by [`sky_colour`] and how little its brightness changes against
/// its neighbours; cells above 0.5 in the top row start a fill that spreads
/// through cells above 0.35, then grows into sky-coloured textured cells.
pub fn sky_map(lab: &[[f32; 3]], w: usize, h: usize) -> SkyMap {
    let scale = (w.max(h) as f64 / 128.0).max(1.0);
    let (sw, sh) = (
        ((w as f64 / scale).ceil() as usize).max(1),
        ((h as f64 / scale).ceil() as usize).max(1),
    );
    let mut cells = vec![[0.0f32; 3]; sw * sh];
    let mut count = vec![0u32; sw * sh];
    for y in 0..h {
        let cy = ((y as f64 / scale) as usize).min(sh - 1);
        for x in 0..w {
            let i = cy * sw + ((x as f64 / scale) as usize).min(sw - 1);
            for c in 0..3 {
                cells[i][c] += lab[y * w + x][c];
            }
            count[i] += 1;
        }
    }
    for (c, n) in cells.iter_mut().zip(&count) {
        *c = c.map(|v| v / (*n).max(1) as f32);
    }
    let score: Vec<f32> = (0..sw * sh)
        .map(|i| {
            let (x, y) = (i % sw, i / sw);
            let l = cells[i][0];
            let mut change = 0.0f32;
            for (dx, dy) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)] {
                let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                if nx >= 0 && ny >= 0 && (nx as usize) < sw && (ny as usize) < sh {
                    change = change.max((cells[ny as usize * sw + nx as usize][0] - l).abs());
                }
            }
            let smooth = 1.0 - smoothstep(0.02, 0.07, f64::from(change)) as f32;
            sky_colour(cells[i]) * smooth
        })
        .collect();
    let mut sky = vec![false; sw * sh];
    let mut stack: Vec<usize> = (0..sw).filter(|&x| score[x] > 0.5).collect();
    while let Some(i) = stack.pop() {
        if sky[i] {
            continue;
        }
        sky[i] = true;
        let (x, y) = (i % sw, i / sw);
        let mut push = |nx: usize, ny: usize| {
            let j = ny * sw + nx;
            if !sky[j] && score[j] > 0.35 {
                stack.push(j);
            }
        };
        if x > 0 {
            push(x - 1, y);
        }
        if x + 1 < sw {
            push(x + 1, y);
        }
        if y > 0 {
            push(x, y - 1);
        }
        if y + 1 < sh {
            push(x, y + 1);
        }
    }
    // Grow two cells into neighbours whose colour is sky-like however
    // textured (sky between branches): the per-pixel colour test then
    // leaves the branches out.
    for _ in 0..2 {
        let grown: Vec<bool> = (0..sw * sh)
            .map(|i| {
                if sky[i] {
                    return true;
                }
                let (x, y) = (i % sw, i / sw);
                let near = (x > 0 && sky[i - 1])
                    || (x + 1 < sw && sky[i + 1])
                    || (y > 0 && sky[i - sw])
                    || (y + 1 < sh && sky[i + sw]);
                near && sky_colour(cells[i]) > 0.3
            })
            .collect();
        sky = grown;
    }
    SkyMap {
        width: sw,
        height: sh,
        data: sky.iter().map(|&s| if s { 1.0 } else { 0.0 }).collect(),
    }
}

impl SkyMap {
    /// Bilinear value at output fractions (u, v).
    fn at(&self, u: f64, v: f64) -> f32 {
        let x = (u * self.width as f64 - 0.5).clamp(0.0, (self.width - 1) as f64);
        let y = (v * self.height as f64 - 0.5).clamp(0.0, (self.height - 1) as f64);
        let (x0, y0) = (x.floor() as usize, y.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(self.width - 1), (y0 + 1).min(self.height - 1));
        let (fx, fy) = ((x - x0 as f64) as f32, (y - y0 as f64) as f32);
        let p = |xx: usize, yy: usize| self.data[yy * self.width + xx];
        let top = p(x0, y0) + (p(x1, y0) - p(x0, y0)) * fx;
        let bottom = p(x0, y1) + (p(x1, y1) - p(x0, y1)) * fx;
        top + (bottom - top) * fy
    }
}

// ---------------------------------------------------------------- geometry

/// Output pixel ↔ uncropped frame pixel (pixel centres at integers).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Frame {
    pub(crate) width: f64,
    pub(crate) height: f64,
    /// frame = o + x·ex + y·ey
    o: (f64, f64),
    ex: (f64, f64),
    ey: (f64, f64),
}

impl Frame {
    pub(crate) fn new(frame: (usize, usize), crop: Option<&CropGeometry>) -> Frame {
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

    /// The same map for a buffer holding `tile` of the output: its pixel
    /// (0, 0) is output pixel (`tile.x`, `tile.y`).
    pub(crate) fn in_tile(self, tile: &Tile) -> Frame {
        let (tx, ty) = (tile.x as f64, tile.y as f64);
        Frame {
            o: (
                self.o.0 + tx * self.ex.0 + ty * self.ey.0,
                self.o.1 + tx * self.ex.1 + ty * self.ey.1,
            ),
            ..self
        }
    }

    /// Frame pixel coordinates (centres at integers) of output pixel (x, y).
    #[inline]
    pub(crate) fn source(&self, x: f64, y: f64) -> (f64, f64) {
        (
            self.o.0 + x * self.ex.0 + y * self.ey.0,
            self.o.1 + x * self.ex.1 + y * self.ey.1,
        )
    }

    /// Output pixel of a frame position.
    pub(crate) fn target(&self, fx: f64, fy: f64) -> (f64, f64) {
        let det = self.ex.0 * self.ey.1 - self.ey.0 * self.ex.1;
        let (dx, dy) = (fx - self.o.0, fy - self.o.1);
        (
            (dx * self.ey.1 - dy * self.ey.0) / det,
            (self.ex.0 * dy - self.ex.1 * dx) / det,
        )
    }

    /// Image fraction → frame pixel coordinates.
    pub(crate) fn pixel(&self, u: f64, v: f64) -> (f64, f64) {
        (u * self.width - 0.5, v * self.height - 0.5)
    }

    /// Output pixels per frame pixel (crops never scale much).
    pub(crate) fn scale(&self) -> f64 {
        1.0 / self.ex.0.hypot(self.ex.1).max(1e-9)
    }
}

pub(crate) fn smoothstep(e0: f64, e1: f64, x: f64) -> f64 {
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
    /// The buffer's place in the full output.
    tile: Tile,
}

/// The colours colour-range masks select, one list per colour shape in
/// drawing order: measured on the image, or given (a region render takes
/// them from the whole photo, since its samples may lie outside the region).
#[derive(Default)]
struct Targets<'a> {
    given: Option<&'a [Vec<[f32; 3]>]>,
    measured: Vec<Vec<[f32; 3]>>,
    /// Sky maps, as colour targets: given to a region render, measured on
    /// a whole one.
    sky_given: Option<&'a [SkyMap]>,
    sky_measured: Vec<SkyMap>,
}

/// Draw one shape into a plane of the output's size.
fn draw_shape(shape: &Shape, frame: &Frame, r: &Reference, targets: &mut Targets) -> Vec<f32> {
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
            // Dabs are circles in pixels, sized in frame widths.
            let centre = |u: f64, v: f64| {
                let (fx, fy) = frame.pixel(u, v);
                frame.target(fx, fy)
            };
            return brush::draw(w, h, strokes, centre, frame.width * frame.scale());
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
                let i = targets.measured.len();
                let colours = match targets.given.and_then(|g| g.get(i)) {
                    Some(t) => t.clone(),
                    None => sample_colours(lab, w, h, frame, samples),
                };
                draw_color_range(&mut plane, lab, &colours, *amount, &r.strengths);
                targets.measured.push(colours);
            }
        }
        Shape::Sky => {
            if let Some(lab) = r.lab {
                let i = targets.sky_measured.len();
                let map = match targets.sky_given.and_then(|g| g.get(i)) {
                    Some(m) => m.clone(),
                    None => sky_map(lab, w, h),
                };
                let t = &r.tile;
                let (fw, fh) = (t.full_width.max(1) as f64, t.full_height.max(1) as f64);
                plane.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
                    let v = (t.y + y) as f64 / fh;
                    for (x, out) in row.iter_mut().enumerate() {
                        let u = (t.x + x) as f64 / fw;
                        // The grid says where; each pixel's colour keeps
                        // branches and edges against the sky sharp.
                        *out = map.at(u, v) * sky_colour(lab[y * w + x]);
                    }
                });
                targets.sky_measured.push(map);
            }
        }
        Shape::Unsupported => {}
    }
    plane
}

/// The mean colour around each sample (5×5 pixels).
fn sample_colours(
    lab: &[[f32; 3]],
    w: usize,
    h: usize,
    frame: &Frame,
    samples: &[(f64, f64)],
) -> Vec<[f32; 3]> {
    if w == 0 || h == 0 {
        return Vec::new();
    }
    samples
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
        .collect()
}

/// A colour range mask selecting `targets`.
fn draw_color_range(
    plane: &mut [f32],
    lab: &[[f32; 3]],
    targets: &[[f32; 3]],
    amount: f64,
    k: &MaskStrengths,
) {
    let tol =
        (k.color_tolerance_min + (k.color_tolerance_max - k.color_tolerance_min) * amount) as f32;
    plane.par_iter_mut().zip(lab.par_iter()).for_each(|(v, p)| {
        let mut best = 0.0f32;
        for t in targets {
            let d = ((0.5 * (p[0] - t[0])).powi(2) + (p[1] - t[1]).powi(2) + (p[2] - t[2]).powi(2))
                .sqrt();
            let m = 1.0 - smoothstep(0.5 * tol as f64, tol as f64, d as f64) as f32;
            best = best.max(m);
        }
        *v = best;
    });
}

/// A correction's final alpha plane.
fn draw_correction(
    c: &Correction,
    frame: &Frame,
    r: &Reference,
    targets: &mut Targets,
) -> Vec<f32> {
    let mut alpha = vec![0.0f32; r.width * r.height];
    for part in &c.parts {
        let mut p = draw_shape(&part.shape, frame, r, targets);
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
        let p = draw_shape(shape, frame, r, targets);
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
    /// The masks with point curves, and where they apply.
    curves: Vec<(std::sync::Arc<LocalCurves>, Vec<f32>)>,
    overlay: Option<Vec<f32>>,
    /// The part of the output the planes cover.
    tile: Tile,
    /// The colours the colour range masks selected (see [`draw_in`]).
    pub colour_targets: Vec<Vec<[f32; 3]>>,
    /// Where Select Sky found the sky.
    pub sky_maps: Vec<SkyMap>,
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
    draw_in(
        masks,
        img,
        frame,
        crop,
        encode,
        strengths,
        &Tile::of(img),
        None,
        None,
    )
}

/// [`draw`] for `img` holding `tile` of the output. Colour range masks
/// select `colour_targets` when given (as measured by a render of the whole
/// output, [`Drawn::colour_targets`]); otherwise the colours under their
/// samples here.
#[allow(clippy::too_many_arguments)]
pub fn draw_in(
    masks: &Masks,
    img: &Image,
    frame: (usize, usize),
    crop: Option<&CropGeometry>,
    encode: impl Fn(f32) -> f32 + Sync,
    strengths: &MaskStrengths,
    tile: &Tile,
    colour_targets: Option<&[Vec<[f32; 3]>]>,
    sky_maps: Option<&[SkyMap]>,
) -> Drawn {
    let frame = Frame::new(frame, crop).in_tile(tile);
    let lw = luminance_weights();
    let luminance: Option<Vec<f32>> =
        masks.needs(|s| matches!(s, Shape::Luminance(_))).then(|| {
            img.rgb
                .par_chunks(3)
                .map(|p| encode((lw[0] * p[0] + lw[1] * p[1] + lw[2] * p[2]).max(0.0)))
                .collect()
        });
    let lab: Option<Vec<[f32; 3]>> = masks
        .needs(|s| matches!(s, Shape::Color { .. } | Shape::Sky))
        .then(|| {
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
        tile: *tile,
    };
    let mut planes = Vec::new();
    let mut curves = Vec::new();
    let mut overlay = None;
    let mut targets = Targets {
        given: colour_targets,
        measured: Vec::new(),
        sky_given: sky_maps,
        sky_measured: Vec::new(),
    };
    for c in &masks.corrections {
        let shown = masks.overlay.is_some() && masks.overlay == c.index;
        if !shown && c.adjust.is_neutral() && c.curves.is_none() {
            continue;
        }
        let alpha = draw_correction(c, &frame, &reference, &mut targets);
        if shown {
            overlay = Some(alpha.clone());
        }
        if let Some(k) = &c.curves {
            curves.push((k.clone(), alpha.clone()));
        }
        if !c.adjust.is_neutral() {
            planes.push((c.adjust, alpha));
        }
    }
    Drawn {
        planes,
        curves,
        overlay,
        tile: *tile,
        colour_targets: targets.measured,
        sky_maps: targets.sky_measured,
    }
}

/// How far (pixels) the masks' local adjustments reach at `scale` (the
/// shapes themselves are drawn per pixel).
pub fn reach(masks: &Masks, cal: &Calibration, scale: f32) -> usize {
    let k = &cal.masks;
    masks
        .corrections
        .iter()
        .map(|c| {
            let a = &c.adjust;
            let contrast = LocalContrast {
                texture: a.texture,
                clarity: a.clarity,
            };
            let noise = (a.noise > 0.0).then_some(NoiseReduction {
                luminance: a.noise,
                luminance_detail: 50.0,
                luminance_contrast: 0.0,
                color: 0.0,
                color_detail: 50.0,
                color_smoothness: 50.0,
            });
            let sharpness = if a.sharpness > 0.0 {
                let s = Sharpening {
                    amount: k.sharpness * a.sharpness / 100.0,
                    radius: 1.0,
                    detail: 25.0,
                    masking: 0.0,
                };
                detail::sharpening_reach(&s, &cal.detail)
            } else if a.sharpness < 0.0 {
                gaussian_reach(((k.soften_sigma * -a.sharpness / 100.0) as f32 * scale).max(0.3))
            } else {
                0
            };
            local::reach(&contrast, &cal.local, scale)
                + noise.map_or(0, |n| detail::noise_reduction_reach(&n, &cal.detail))
                + sharpness
        })
        .sum()
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

    /// Global Defringe (`global`) held back where masks have a negative
    /// local Defringe, then each positive one: a Defringe of its own, with
    /// the global hue ranges, inside its mask.
    pub fn apply_defringe(&self, img: &mut Image, global: &Defringe, cal: &Calibration) {
        let mut protect: Option<Vec<f32>> = None;
        for (a, alpha) in self.each(|a| a.defringe < 0.0) {
            let s = (-a.defringe / 100.0).min(1.0) as f32;
            let w = protect.get_or_insert_with(|| vec![1.0; alpha.len()]);
            w.iter_mut()
                .zip(alpha)
                .for_each(|(w, &m)| *w *= 1.0 - s * m);
        }
        lens::apply_defringe_weighted(img, global, &cal.lens, &self.tile, protect.as_deref());
        for (a, alpha) in self.each(|a| a.defringe > 0.0) {
            let amount = cal.masks.defringe * a.defringe / 100.0;
            let d = Defringe {
                purple_amount: amount,
                green_amount: amount,
                ..*global
            };
            lens::apply_defringe_weighted(img, &d, &cal.lens, &self.tile, Some(alpha));
        }
    }

    /// Local Dehaze. Each mask's haze is measured on the image (a whole
    /// output; `full` is the full output's size), or taken from `given` (one
    /// per Dehaze mask, in order, as measured on the whole output). Returns
    /// the measured estimates.
    pub fn apply_dehaze(
        &self,
        img: &mut Image,
        cal: &Calibration,
        given: Option<&[dehaze::Estimate]>,
        full: (usize, usize),
    ) -> Vec<dehaze::Estimate> {
        let mut measured = Vec::new();
        for (i, (a, alpha)) in self.each(|a| a.dehaze != 0.0).enumerate() {
            let mut alt = img.clone();
            let haze = match given.and_then(|e| e.get(i)) {
                Some(e) => dehaze::Haze::Given(e),
                None => dehaze::Haze::Measure { full },
            };
            measured.extend(dehaze::apply_haze(
                &mut alt,
                a.dehaze,
                &cal.dehaze,
                &self.tile,
                haze,
            ));
            blend(img, &alt, alpha);
        }
        measured
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
            local::apply_scaled(&mut alt, &p, &cal.local, self.tile.scale());
            blend(img, &alt, alpha);
        }
    }

    /// Each mask's point curves, after the global curves, faded by the mask.
    pub fn apply_curves(&self, img: &mut Image) {
        const LUT: usize = 4096;
        for (k, alpha) in &self.curves {
            let main = k.main.as_ref().map(|s| s.lut(LUT));
            let channels: Vec<Option<Vec<f32>>> = k
                .channels
                .iter()
                .map(|c| c.as_ref().map(|s| s.lut(LUT)))
                .collect();
            img.rgb
                .par_chunks_mut(3)
                .zip(alpha.par_iter())
                .for_each(|(p, &m)| {
                    if m <= 0.0 {
                        return;
                    }
                    for (ch, v) in p.iter_mut().enumerate() {
                        let mut e = local::srgb_encode(v.clamp(0.0, 1.0));
                        if let Some(lut) = &main {
                            e = local::lookup(lut, e);
                        }
                        if let Some(lut) = &channels[ch] {
                            e = local::lookup(lut, e);
                        }
                        *v += (local::srgb_decode(e) - *v) * m;
                    }
                });
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
                let sigma = (k.soften_sigma * -a.sharpness / 100.0) as f32 * self.tile.scale();
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
            tile: Tile::whole(w, h),
        }
    }

    fn alpha_of(s: &BTreeMap<String, Value>, w: usize, h: usize) -> Vec<f32> {
        let (m, _) = Masks::from_settings(s);
        draw_correction(
            &m.corrections[0],
            &Frame::new((w, h), None),
            &reference(w, h),
            &mut Targets::default(),
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
    fn local_defringe_adds_to_and_holds_back_the_global_one() {
        let (w, h) = (80, 40);
        let ok = OkLab::new();
        let hue = 310f32.to_radians();
        let purple = ok.to_working([0.5, 0.1 * hue.cos(), 0.1 * hue.sin()]);
        // A dark/bright edge with a 2 px purple fringe, in every row.
        let mut img = Image::new(w, h);
        for y in 0..h {
            for x in 0..w {
                let v = match x {
                    0..20 => [0.02; 3],
                    20..22 => purple,
                    _ => [0.9; 3],
                };
                img.set(x, y, v);
            }
        }
        let chroma = |img: &Image, y: usize| {
            let l = ok.from_working(img.get(21, y));
            l[1].hypot(l[2])
        };
        // The mask covers the top quarter fully, nothing below the middle.
        let drawn = |defringe: &str| {
            let mut c = correction(vec![linear("0.5", "0.25")]);
            if let Value::Struct(m) = &mut c {
                m.remove("LocalExposure2012");
                m.insert("LocalDefringe".into(), text(defringe));
            }
            let (m, _) = Masks::from_settings(&settings(vec![c]));
            draw(&m, &img, (w, h), None, |y| y, &MaskStrengths::default())
        };
        let cal = Calibration::default();
        // +100 with no global Defringe: removed under the mask only.
        let mut a = img.clone();
        drawn("1").apply_defringe(&mut a, &Defringe::default(), &cal);
        assert!(chroma(&a, 2) < 0.03, "top {}", chroma(&a, 2));
        assert!(chroma(&a, 35) > 0.09, "bottom {}", chroma(&a, 35));
        // −100 holds back a global Defringe under the mask.
        let global = Defringe {
            purple_amount: 10.0,
            ..Default::default()
        };
        let mut b = img.clone();
        drawn("-1").apply_defringe(&mut b, &global, &cal);
        assert!(chroma(&b, 2) > 0.09, "top {}", chroma(&b, 2));
        assert!(chroma(&b, 35) < 0.03, "bottom {}", chroma(&b, 35));
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
    fn aggregate_brush_with_a_density_and_the_acr_note() {
        let stroke = |value: &str| {
            st(&[
                ("What", text("Mask/Paint")),
                ("MaskValue", text(value)),
                ("Radius", text("0.1")),
                ("Flow", text("1")),
                ("CenterWeight", text("1")),
                ("Dabs", Value::Seq(vec![text("d 0.5 0.5")])),
            ])
        };
        let agg = st(&[
            ("What", text("Mask/Aggregate")),
            ("MaskBlendMode", text("0")),
            ("MaskValue", text("1")),
            ("Masks", Value::Seq(vec![stroke("0.5")])),
        ]);
        let (w, h) = (100, 100);
        let a = alpha_of(&settings(vec![correction(vec![agg])]), w, h);
        assert!((a[50 * w + 50] - 0.5).abs() < 0.01, "{}", a[50 * w + 50]);
        assert_eq!(a[10 * w + 10], 0.0);
        // Strokes kept in the .acr file can't be drawn yet, and say so.
        let table = st(&[
            ("What", text("Mask/Aggregate")),
            ("MaskBrushTable", text("0123456789abcdef0123456789abcdef")),
        ]);
        let (_, notes) = Masks::from_settings(&settings(vec![correction(vec![table])]));
        assert!(notes[0].contains(".acr"), "{notes:?}");
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
        let a = draw_correction(
            &m.corrections[0],
            &Frame::new((100, 10), Some(&g)),
            &r,
            &mut Targets::default(),
        );
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
            tile: Tile::whole(10, 1),
        };
        let a = draw_correction(
            &m.corrections[0],
            &Frame::new((10, 1), None),
            &r,
            &mut Targets::default(),
        );
        assert_eq!(a[0], 0.0);
        assert_eq!(a[9], 1.0);
        assert!(a[4] < 1e-6 && a[6] > 0.99, "{a:?}");
    }

    #[test]
    fn ai_masks_select_nothing_and_are_reported() {
        let ai = st(&[("What", text("Mask/Image")), ("MaskSubType", text("1"))]);
        let s = settings(vec![correction(vec![ai])]);
        let (m, notes) = Masks::from_settings(&s);
        assert_eq!(notes.len(), 1, "{notes:?}");
        assert!(notes[0].contains("Select Subject"));
        let a = draw_correction(
            &m.corrections[0],
            &Frame::new((4, 4), None),
            &reference(4, 4),
            &mut Targets::default(),
        );
        assert!(a.iter().all(|&v| v == 0.0));
    }

    #[test]
    fn select_sky_finds_the_sky_joined_to_the_top() {
        // A blue sky over dark, textured trees, with a blue lake below them.
        let (w, h) = (120, 90);
        let ok = OkLab::new();
        let sky = ok.from_working([0.45, 0.6, 0.9]);
        let lab: Vec<[f32; 3]> = (0..w * h)
            .map(|i| {
                let (x, y) = (i % w, i / w);
                // The sky, and the lake.
                if y < 40 || ((65..80).contains(&y) && (30..90).contains(&x)) {
                    sky
                } else {
                    // Trees: dark green with a checker texture.
                    let t = if (x / 2 + y / 2) % 2 == 0 { 0.05 } else { 0.12 };
                    ok.from_working([t * 0.6, t, t * 0.5])
                }
            })
            .collect();
        let map = sky_map(&lab, w, h);
        let at =
            |x: usize, y: usize| map.at((x as f64 + 0.5) / w as f64, (y as f64 + 0.5) / h as f64);
        assert!(at(60, 10) > 0.99 && at(5, 30) > 0.99);
        assert!(at(60, 55) < 0.01, "trees");
        assert!(at(60, 72) < 0.01, "the lake isn't joined to the top");
        // Drawn through the mask: the sky's own pixels, nothing else.
        let ai = st(&[("What", text("Mask/Image")), ("MaskSubType", text("2"))]);
        let (m, notes) = Masks::from_settings(&settings(vec![correction(vec![ai])]));
        assert!(notes.is_empty(), "{notes:?}");
        let r = Reference {
            luminance: None,
            lab: Some(&lab),
            width: w,
            height: h,
            strengths: MaskStrengths::default(),
            tile: Tile::whole(w, h),
        };
        let mut targets = Targets::default();
        let a = draw_correction(
            &m.corrections[0],
            &Frame::new((w, h), None),
            &r,
            &mut targets,
        );
        assert!(a[10 * w + 60] > 0.9 && a[55 * w + 60] < 0.01 && a[72 * w + 60] < 0.01);
        assert_eq!(targets.sky_measured.len(), 1);
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
    fn a_masks_curve_applies_only_under_it() {
        // A curve that darkens the midtones, and no sliders.
        let mut c = correction(vec![linear("0.55", "0.45")]);
        if let Value::Struct(m) = &mut c {
            m.remove("LocalExposure2012");
            m.insert(
                "MainCurve".into(),
                Value::Seq(["0, 0", "128, 64", "255, 255"].map(text).to_vec()),
            );
        }
        let (m, notes) = Masks::from_settings(&settings(vec![c]));
        assert!(notes.is_empty(), "{notes:?}");
        assert!(m.corrections[0].adjust.is_neutral() && m.corrections[0].curves.is_some());
        let mut img = Image::from_fn(4, 20, |_, _| [0.2, 0.2, 0.2]);
        let drawn = draw(&m, &img, (4, 20), None, |v| v, &MaskStrengths::default());
        drawn.apply_curves(&mut img);
        assert!(img.get(1, 1)[0] < 0.1, "{:?}", img.get(1, 1));
        assert!((img.get(1, 18)[0] - 0.2).abs() < 1e-6);
        // An identity curve is no curve.
        let mut id = correction(vec![linear("1", "0")]);
        if let Value::Struct(m) = &mut id {
            m.insert(
                "MainCurve".into(),
                Value::Seq(["0, 0", "255, 255"].map(text).to_vec()),
            );
        }
        assert!(
            Masks::from_settings(&settings(vec![id])).0.corrections[0]
                .curves
                .is_none()
        );
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
