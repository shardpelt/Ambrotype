// SPDX-License-Identifier: GPL-3.0-or-later
//! Spot removal: heal and clone (docs/design/spot-removal.md).
//!
//! Each spot copies pixels from a donor area onto its own area, through a
//! feathered alpha. A clone copies them as they are. A heal also matches them
//! to the spot's surroundings: in log space, the difference between the
//! surroundings and the donor's surroundings is fitted with a plane per
//! channel (from a ring just outside the spot) and added, so brightness,
//! colour and gentle gradients carry over while the donor's texture stays.
//! Spots without a donor (content-aware and generative fills, whose pixels
//! Lightroom does not store, and spots made without one) heal from the donor
//! [`find_source`] picks: the nearby area whose surroundings best match.
//!
//! With an inpainter registered ([`set_inpainter`]: AI Remove, a model the
//! user installs), content-aware and generative spots are filled by it
//! instead ([`fill_one`]), with the photo's grain put back over the fill.
//!
//! Spots run in order on the scene-linear image, before masks are drawn, so
//! range masks see the cleaned photo. They work on the cropped image, so a
//! donor outside the crop is clamped to its edge.

use std::collections::BTreeMap;

use amb_presets::Value;
use amb_presets::masks::{self, flag, num};
use amb_presets::retouch::{self, ENABLE_TAG, LEGACY_TAG, RETOUCH_TAG, SpotKind};

use crate::brush;
use crate::buffer::{Image, Tile};
use crate::crop::CropGeometry;
use crate::mask::Frame;
use crate::math::smoothstep_f64 as smoothstep;

mod heal;
mod inpaint;
mod source;
#[cfg(test)]
mod tests;

use heal::{heal_one, search_offset};
pub use inpaint::{INPAINT_SIZE, Inpainter, set_inpainter};
use inpaint::{fill_one, inpainter};
pub use source::{find_source, find_spot_source, footprints};

#[derive(Debug, Clone, PartialEq)]
pub enum SpotShape {
    /// Centre (fractions) and half-extents (fractions of the frame width).
    Ellipse {
        centre: (f64, f64),
        size: (f64, f64),
    },
    Brush(Vec<brush::Stroke>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Spot {
    pub kind: SpotKind,
    pub shape: SpotShape,
    /// Donor centre (fractions); `None` to search for one.
    pub source: Option<(f64, f64)>,
    /// 0..1: the share of the radius that fades out.
    pub feather: f64,
    pub opacity: f64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Spots {
    pub spots: Vec<Spot>,
}

/// Feather Lightroom uses when an area doesn't state one.
pub(super) const DEFAULT_FEATHER: f64 = 0.5;

impl Spots {
    /// Read `RetouchAreas` and the older `RetouchInfo`. Notes name what is
    /// only approximated.
    pub fn from_settings(settings: &BTreeMap<String, Value>) -> (Spots, Vec<String>) {
        let mut out = Spots::default();
        let mut notes = Vec::new();
        if settings
            .get(ENABLE_TAG)
            .and_then(Value::as_bool)
            .is_some_and(|on| !on)
        {
            return (out, notes);
        }
        let mut synthesised = 0;
        for item in retouch::areas(settings) {
            let Value::Struct(a) = item else { continue };
            let Some(kind) = SpotKind::of(a) else {
                continue;
            };
            let comps = retouch::shape_components(a);
            if comps.iter().all(|c| flag(c, "MaskActive") == Some(false)) && !comps.is_empty() {
                continue;
            }
            let shape = match retouch::ellipse(a) {
                Some((centre, size)) => SpotShape::Ellipse { centre, size },
                None => {
                    let strokes: Vec<brush::Stroke> = comps
                        .iter()
                        .filter(|c| masks::text(c, "What").as_deref() == Some("Mask/Paint"))
                        .map(|c| brush::stroke(c, 1.0, false))
                        .filter(|s| !s.dabs.is_empty())
                        .collect();
                    if strokes.is_empty() {
                        continue;
                    }
                    SpotShape::Brush(strokes)
                }
            };
            if kind.synthesised() {
                synthesised += 1;
            }
            out.spots.push(Spot {
                kind,
                shape,
                source: if kind.synthesised() {
                    None
                } else {
                    retouch::source(a)
                },
                feather: num(a, "Feather").unwrap_or(DEFAULT_FEATHER).clamp(0.0, 1.0),
                opacity: num(a, "Opacity").unwrap_or(1.0).clamp(0.0, 1.0),
            });
        }
        if let Some(Value::Seq(items)) = settings.get(LEGACY_TAG) {
            for s in items
                .iter()
                .filter_map(Value::as_str)
                .filter_map(retouch::parse_legacy)
            {
                out.spots.push(Spot {
                    kind: s.kind,
                    shape: SpotShape::Ellipse {
                        centre: s.centre,
                        size: (s.radius, s.radius),
                    },
                    source: s.source,
                    feather: DEFAULT_FEATHER,
                    opacity: 1.0,
                });
            }
        }
        if synthesised > 0 {
            notes.push(format!(
                "{RETOUCH_TAG}: {synthesised} content-aware or generative spot(s) are healed from a nearby area unless AI Remove's model is installed (Lightroom doesn't store their pixels)"
            ));
        }
        (out, notes)
    }

    pub fn is_empty(&self) -> bool {
        self.spots.is_empty()
    }
}

/// A spot laid out on the output: its alpha over a box, and its centre.
pub(super) struct Placed {
    x0: usize,
    y0: usize,
    w: usize,
    h: usize,
    alpha: Vec<f32>,
    /// Centre and radius in output pixels.
    centre: (f64, f64),
    radius: f64,
}

pub(super) fn place(spot: &Spot, frame: &Frame, w: usize, h: usize) -> Option<Placed> {
    let px_per_width = frame.width * frame.scale();
    match &spot.shape {
        SpotShape::Ellipse { centre, size } => {
            let (cfx, cfy) = frame.pixel(centre.0, centre.1);
            let (cx, cy) = frame.target(cfx, cfy);
            let (ax, ay) = (size.0 * frame.width, size.1 * frame.width);
            if !(ax > 0.0 && ay > 0.0) {
                return None;
            }
            let r = ax.max(ay) * frame.scale();
            let x0 = (cx - r - 1.0).floor().max(0.0) as usize;
            let y0 = (cy - r - 1.0).floor().max(0.0) as usize;
            let x1 = ((cx + r + 2.0).ceil() as usize).min(w);
            let y1 = ((cy + r + 2.0).ceil() as usize).min(h);
            if x0 >= x1 || y0 >= y1 {
                return None;
            }
            let (bw, bh) = (x1 - x0, y1 - y0);
            let inner = 1.0 - spot.feather;
            let mut alpha = vec![0.0f32; bw * bh];
            for j in 0..bh {
                for i in 0..bw {
                    let (fx, fy) = frame.source((x0 + i) as f64, (y0 + j) as f64);
                    let rho = (((fx - cfx) / ax).powi(2) + ((fy - cfy) / ay).powi(2)).sqrt();
                    alpha[j * bw + i] = ((1.0 - smoothstep(inner, 1.0, rho)) * spot.opacity) as f32;
                }
            }
            Some(Placed {
                x0,
                y0,
                w: bw,
                h: bh,
                alpha,
                centre: (cx, cy),
                radius: r,
            })
        }
        SpotShape::Brush(strokes) => {
            let centre = |u: f64, v: f64| {
                let (fx, fy) = frame.pixel(u, v);
                frame.target(fx, fy)
            };
            let cover = brush::draw(w, h, strokes, centre, px_per_width);
            let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
            for (k, &a) in cover.iter().enumerate() {
                if a > 0.0 {
                    let (x, y) = (k % w, k / w);
                    x0 = x0.min(x);
                    y0 = y0.min(y);
                    x1 = x1.max(x + 1);
                    y1 = y1.max(y + 1);
                }
            }
            if x0 >= x1 || y0 >= y1 {
                return None;
            }
            let (bw, bh) = (x1 - x0, y1 - y0);
            let mut alpha = vec![0.0f32; bw * bh];
            for j in 0..bh {
                let src = (y0 + j) * w + x0;
                for i in 0..bw {
                    alpha[j * bw + i] = cover[src + i] * spot.opacity as f32;
                }
            }
            Some(Placed {
                x0,
                y0,
                w: bw,
                h: bh,
                alpha,
                centre: ((x0 + x1) as f64 / 2.0, (y0 + y1) as f64 / 2.0),
                radius: (bw.max(bh) as f64) / 2.0,
            })
        }
    }
}

/// Bilinear sample of `img` at output position (x, y), clamped to the edges.
pub(super) fn sample(img: &Image, x: f64, y: f64) -> [f32; 3] {
    let (w, h) = (img.width, img.height);
    let x = x.clamp(0.0, (w - 1) as f64);
    let y = y.clamp(0.0, (h - 1) as f64);
    let (x0, y0) = (x.floor() as usize, y.floor() as usize);
    let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
    let (fx, fy) = ((x - x0 as f64) as f32, (y - y0 as f64) as f32);
    let p = |xx: usize, yy: usize| img.get(xx, yy);
    let (a, b, c, d) = (p(x0, y0), p(x1, y0), p(x0, y1), p(x1, y1));
    let mut out = [0.0f32; 3];
    for k in 0..3 {
        let top = a[k] + (b[k] - a[k]) * fx;
        let bot = c[k] + (d[k] - c[k]) * fx;
        out[k] = top + (bot - top) * fy;
    }
    out
}

/// Apply every spot, in order. `frame` is the uncropped frame's size and
/// `crop` the output → frame map, as for masks.
pub fn apply(img: &mut Image, spots: &Spots, frame: (usize, usize), crop: Option<&CropGeometry>) {
    let tile = Tile::of(img);
    apply_in(img, spots, frame, crop, &tile);
}

/// [`apply`] on `img` holding `tile` of the output. Spots whose donors lie
/// outside it cannot be healed right here, so a region render includes them
/// ([`footprint`]).
pub fn apply_in(
    img: &mut Image,
    spots: &Spots,
    frame: (usize, usize),
    crop: Option<&CropGeometry>,
    tile: &Tile,
) {
    if spots.is_empty() || img.pixel_count() == 0 {
        return;
    }
    let frame = Frame::new(frame, crop).in_tile(tile);
    // The whole output, in this buffer's pixels.
    let bounds = (
        -(tile.x as f64),
        -(tile.y as f64),
        (tile.full_width - tile.x) as f64,
        (tile.full_height - tile.y) as f64,
    );
    for spot in &spots.spots {
        let Some(p) = place(spot, &frame, img.width, img.height) else {
            continue;
        };
        if spot.kind.synthesised() && spot.source.is_none() {
            // AI-free: an AI fill is left out, not healed from a donor (a
            // different picture than the one the edit asks for).
            if amb_presets::ai_free::on() {
                continue;
            }
            if let Some(fill) = inpainter() {
                if fill_one(img, &p, &*fill).is_ok() {
                    continue;
                }
            }
        }
        let offset = match spot.source {
            Some((sx, sy)) => {
                let (fx, fy) = frame.pixel(sx, sy);
                let (tx, ty) = frame.target(fx, fy);
                (tx - p.centre.0, ty - p.centre.1)
            }
            None => search_offset(img, &p, spot.kind == SpotKind::Clone, bounds),
        };
        heal_one(img, &p, offset, spot.kind == SpotKind::Clone);
    }
}
