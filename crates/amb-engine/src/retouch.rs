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
//! Spots run in order on the scene-linear image, before masks are drawn, so
//! range masks see the cleaned photo. They work on the cropped image, so a
//! donor outside the crop is clamped to its edge.

use std::collections::BTreeMap;

use amb_presets::Value;
use amb_presets::masks::{self, flag, num};
use amb_presets::retouch::{self, ENABLE_TAG, LEGACY_TAG, RETOUCH_TAG, SpotKind};

use crate::brush;
use crate::buffer::{Image, Tile, luminance_weights};
use crate::crop::CropGeometry;
use crate::filters::box_blur;
use crate::mask::{Frame, smoothstep};

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
const DEFAULT_FEATHER: f64 = 0.5;

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
                "{RETOUCH_TAG}: {synthesised} content-aware or generative spot(s) are healed from a nearby area instead (Lightroom doesn't store their pixels)"
            ));
        }
        (out, notes)
    }

    pub fn is_empty(&self) -> bool {
        self.spots.is_empty()
    }
}

/// A spot laid out on the output: its alpha over a box, and its centre.
struct Placed {
    x0: usize,
    y0: usize,
    w: usize,
    h: usize,
    alpha: Vec<f32>,
    /// Centre and radius in output pixels.
    centre: (f64, f64),
    radius: f64,
}

fn place(spot: &Spot, frame: &Frame, w: usize, h: usize) -> Option<Placed> {
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
fn sample(img: &Image, x: f64, y: f64) -> [f32; 3] {
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

/// The donor offset (output pixels) whose surroundings best match the
/// spot's, for the spot's own shape (a round spot or a brushed stroke).
/// Candidates lie in 24 directions, just clear of the spot along each one and
/// further out; each is scored on the band around the spot: how far the
/// donor's band differs from the spot's (log luminance; for a heal, which
/// evens out brightness, mostly in shape, a little in level), plus how far
/// the donor's inside strays from what its own band predicts (a donor
/// holding an object or a line does), how different its fine texture is from
/// the band's, and its distance. A donor may not overlap the spot.
fn search_offset(
    img: &Image,
    p: &Placed,
    clone: bool,
    (x0, y0, x1, y1): (f64, f64, f64, f64),
) -> (f64, f64) {
    let margin = (p.radius * 0.35).clamp(3.0, 40.0) as usize;
    let (ex0, ey0) = (p.x0.saturating_sub(margin), p.y0.saturating_sub(margin));
    let ex1 = (p.x0 + p.w + margin).min(img.width);
    let ey1 = (p.y0 + p.h + margin).min(img.height);
    let (ew, eh) = (ex1 - ex0, ey1 - ey0);
    let mut mask = vec![0.0f32; ew * eh];
    for j in 0..p.h {
        for i in 0..p.w {
            if p.alpha[j * p.w + i] > 0.01 {
                mask[(p.y0 - ey0 + j) * ew + (p.x0 - ex0 + i)] = 1.0;
            }
        }
    }
    let near = box_blur(&mask, ew, eh, margin);
    let (mut band, mut inside) = (Vec::new(), Vec::new());
    for j in 0..eh {
        for i in 0..ew {
            let k = j * ew + i;
            let pt = ((ex0 + i) as f64, (ey0 + j) as f64);
            if mask[k] > 0.0 {
                inside.push(pt);
            } else if near[k] > 0.0 {
                band.push(pt);
            }
        }
    }
    // A few hundred points each is plenty to compare.
    let thin = |v: Vec<(f64, f64)>, n: usize| -> Vec<(f64, f64)> {
        let step = v.len().div_ceil(n).max(1);
        v.into_iter().step_by(step).collect()
    };
    let (band, inside) = (thin(band, 600), thin(inside, 400));
    if band.is_empty() || inside.is_empty() {
        return (p.radius.max(1.0) * 2.2, 0.0);
    }
    // Log luminance with a floor, as the eye sees it: plain logs blow the
    // noise of near-black areas up into differences that swamp the rest.
    let lw = luminance_weights();
    let luma = |pts: &[(f64, f64)], (dx, dy): (f64, f64)| -> Vec<f64> {
        pts.iter()
            .map(|&(x, y)| {
                let p = sample(img, x + dx, y + dy);
                ((lw[0] * p[0] + lw[1] * p[1] + lw[2] * p[2]).max(0.0) as f64 + 0.005).ln()
            })
            .collect()
    };
    let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len() as f64;
    let var = |v: &[f64]| {
        let m = mean(v);
        v.iter().map(|x| (x - m).powi(2)).sum::<f64>() / v.len() as f64
    };
    // Fine texture: the difference to the pixel two to the right.
    let fine = |pts: &[(f64, f64)], (dx, dy): (f64, f64)| -> f64 {
        let a = luma(pts, (dx, dy));
        let b = luma(pts, (dx + 2.0, dy));
        (a.iter().zip(&b).map(|(a, b)| (a - b).powi(2)).sum::<f64>() / a.len() as f64).sqrt()
    };
    let target = luma(&band, (0.0, 0.0));
    let target_fine = fine(&band, (0.0, 0.0));
    // How each inside point is guessed from the band (inverse squared
    // distance weights); the same for every candidate, as both move together.
    let guess: Vec<Vec<f32>> = inside
        .iter()
        .map(|&(x, y)| {
            let w: Vec<f64> = band
                .iter()
                .map(|&(bx, by)| 1.0 / ((bx - x).powi(2) + (by - y).powi(2)).max(1.0))
                .collect();
            let total: f64 = w.iter().sum();
            w.iter().map(|v| (v / total) as f32).collect()
        })
        .collect();
    let in_mask = |x: f64, y: f64| {
        let (i, j) = (x.round() - ex0 as f64, y.round() - ey0 as f64);
        i >= 0.0
            && j >= 0.0
            && (i as usize) < ew
            && (j as usize) < eh
            && mask[j as usize * ew + i as usize] > 0.0
    };
    // The spot's width along each direction, so a donor clears it.
    let clearance: Vec<(f64, f64, f64)> = (0..24)
        .map(|d| {
            let t = d as f64 / 24.0 * std::f64::consts::TAU;
            let (ux, uy) = (t.cos(), t.sin());
            let proj = inside.iter().map(|&(x, y)| x * ux + y * uy);
            let (lo, hi) = proj.fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), v| {
                (a.min(v), b.max(v))
            });
            (ux, uy, hi - lo + margin as f64)
        })
        .collect();
    // Distance counts in the spot's narrowest width: beside a long stroke
    // is near, along it is far.
    let thinnest = clearance
        .iter()
        .map(|c| c.2)
        .fold(f64::INFINITY, f64::min)
        .max(1.0);
    let mut best = ((p.radius.max(1.0) * 2.2, 0.0), f64::INFINITY);
    for &(ux, uy, clear) in &clearance {
        for k in [1.0, 1.35, 1.8, 2.5] {
            let off = ((ux * clear * k).round(), (uy * clear * k).round());
            // The donor and its band must be in the picture.
            let (bx0, by0, bx1, by1) = (
                ex0 as f64 + off.0,
                ey0 as f64 + off.1,
                ex1 as f64 + off.0,
                ey1 as f64 + off.1,
            );
            if bx0 < x0 || by0 < y0 || bx1 > x1 || by1 > y1 {
                continue;
            }
            let overlap = inside
                .iter()
                .filter(|&&(x, y)| in_mask(x + off.0, y + off.1))
                .count();
            if overlap * 50 > inside.len() {
                continue;
            }
            let around = luma(&band, off);
            let diff: Vec<f64> = around.iter().zip(&target).map(|(a, t)| a - t).collect();
            let shape = if clone {
                diff.iter().map(|e| e * e).sum::<f64>() / diff.len() as f64
            } else {
                // The heal evens out brightness, but brightening a much
                // darker donor brings up its noise and faint lines with it.
                var(&diff) + 0.5 * mean(&diff).powi(2)
            };
            // Something in the donor that its own band doesn't explain (an
            // object, a streak) would be carried into the spot: how far its
            // inside is from what its band predicts there.
            let inner = luma(&inside, off);
            let structure = inner
                .iter()
                .zip(&guess)
                .map(|(v, w)| {
                    let g: f64 = w.iter().zip(&around).map(|(&w, a)| w as f64 * a).sum();
                    (v - g).powi(2)
                })
                .sum::<f64>()
                / inner.len() as f64;
            let texture = (fine(&inside, off) - target_fine).powi(2);
            // Near donors are likelier to match: the scene changes with
            // distance, which the band alone may not show.
            let far = 0.004 * (off.0.hypot(off.1) / thinnest);
            let score = shape + structure + texture + far;
            if score < best.1 {
                best = (off, score);
            }
        }
    }
    best.0
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

/// Where each spot writes, and what it reads to do so (its own box and the
/// ring around it, and its donor or the area searched for one), in output
/// pixels as `[x0, y0, x1, y1]`. A region render that a spot writes into must
/// hold all it reads.
pub fn footprints(
    spots: &Spots,
    frame: (usize, usize),
    crop: Option<&CropGeometry>,
) -> Vec<([f64; 4], [f64; 4])> {
    let frame = Frame::new(frame, crop);
    let px_per_width = frame.width * frame.scale();
    let to_output = |u: f64, v: f64| {
        let (fx, fy) = frame.pixel(u, v);
        frame.target(fx, fy)
    };
    spots
        .spots
        .iter()
        .filter_map(|spot| {
            let (centre, r) = match &spot.shape {
                SpotShape::Ellipse { centre, size } => {
                    let r = size.0.max(size.1) * frame.width * frame.scale();
                    (to_output(centre.0, centre.1), r)
                }
                SpotShape::Brush(strokes) => {
                    let (mut x0, mut y0) = (f64::INFINITY, f64::INFINITY);
                    let (mut x1, mut y1) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
                    for d in strokes.iter().flat_map(|s| &s.dabs) {
                        let (x, y) = to_output(d.x, d.y);
                        let r = d.radius * px_per_width + 1.0;
                        (x0, y0) = (x0.min(x - r), y0.min(y - r));
                        (x1, y1) = (x1.max(x + r), y1.max(y + r));
                    }
                    if x0 > x1 {
                        return None;
                    }
                    (
                        ((x0 + x1) / 2.0, (y0 + y1) / 2.0),
                        (x1 - x0).max(y1 - y0) / 2.0,
                    )
                }
            };
            if !(r > 0.0 && centre.0.is_finite() && centre.1.is_finite()) {
                return None;
            }
            let ring = (r * 0.35).clamp(2.0, 40.0) + 3.0;
            let around =
                |(x, y): (f64, f64), reach: f64| [x - reach, y - reach, x + reach, y + reach];
            let writes = around(centre, r + 2.0);
            let donor = match spot.source {
                Some((u, v)) => around(to_output(u, v), r + ring + 2.0),
                // Candidates up to 2.5 × (the spot's width and a margin)
                // away, with their bands.
                None => around(centre, 6.0 * r.max(1.0) + 4.0 * ring + 3.0),
            };
            let own = around(centre, r + ring);
            let reads = [
                own[0].min(donor[0]),
                own[1].min(donor[1]),
                own[2].max(donor[2]),
                own[3].max(donor[3]),
            ];
            Some((writes, reads))
        })
        .collect()
}

fn heal_one(img: &mut Image, p: &Placed, offset: (f64, f64), clone: bool) {
    // Whole pixels: a fractional offset would blur the donor's texture.
    let offset = (offset.0.round(), offset.1.round());
    let snapshot = img.clone();
    let field = if clone {
        None
    } else {
        Some(correction(&snapshot, p, offset))
    };
    for j in 0..p.h {
        for i in 0..p.w {
            let a = p.alpha[j * p.w + i];
            if a <= 0.0 {
                continue;
            }
            let (x, y) = ((p.x0 + i) as f64, (p.y0 + j) as f64);
            let mut d = sample(&snapshot, x + offset.0, y + offset.1);
            if let Some((f, fw, (fx0, fy0))) = &field {
                let k = (p.y0 + j - fy0) * fw + (p.x0 + i - fx0);
                for c in 0..3 {
                    d[c] *= f[c][k].clamp(-6.0, 6.0).exp();
                }
            }
            let t = snapshot.get(p.x0 + i, p.y0 + j);
            let out = [0, 1, 2].map(|c| t[c] + (d[c] - t[c]) * a);
            img.set(p.x0 + i, p.y0 + j, out);
        }
    }
}

/// The heal's correction, log(target) − log(donor) per channel, over the
/// spot's box and a margin: measured in a band just outside the spot,
/// smoothed there, and carried smoothly across the inside (a membrane, by
/// push-pull interpolation). Brightness, colour and curved shading carry
/// over from the edge all round; the donor's texture stays. Returns the
/// three planes, their width and their origin.
#[allow(clippy::type_complexity)]
fn correction(
    snapshot: &Image,
    p: &Placed,
    offset: (f64, f64),
) -> ([Vec<f32>; 3], usize, (usize, usize)) {
    let margin = (p.radius * 0.35).clamp(3.0, 40.0) as usize;
    let (ex0, ey0) = (p.x0.saturating_sub(margin), p.y0.saturating_sub(margin));
    let ex1 = (p.x0 + p.w + margin).min(snapshot.width);
    let ey1 = (p.y0 + p.h + margin).min(snapshot.height);
    let (ew, eh) = (ex1 - ex0, ey1 - ey0);
    let mut inside = vec![0.0f32; ew * eh];
    for j in 0..p.h {
        for i in 0..p.w {
            if p.alpha[j * p.w + i] > 0.01 {
                inside[(p.y0 - ey0 + j) * ew + (p.x0 - ex0 + i)] = 1.0;
            }
        }
    }
    let near = box_blur(&inside, ew, eh, margin);
    // Known values: the band outside the spot.
    let known: Vec<f32> = (0..ew * eh)
        .map(|k| f32::from(inside[k] == 0.0 && near[k] > 0.0))
        .collect();
    let smooth = (p.radius * 0.15).clamp(1.0, 12.0) as usize;
    let weight = box_blur(&known, ew, eh, smooth);
    let planes = [0, 1, 2].map(|c| {
        let mut v = vec![0.0f32; ew * eh];
        for j in 0..eh {
            for i in 0..ew {
                let k = j * ew + i;
                if known[k] > 0.0 {
                    let (x, y) = ((ex0 + i) as f64, (ey0 + j) as f64);
                    let t = snapshot.get(ex0 + i, ey0 + j)[c];
                    let d = sample(snapshot, x + offset.0, y + offset.1)[c];
                    v[k] = t.max(1e-5).ln() - d.max(1e-5).ln();
                }
            }
        }
        // Smooth along the band so the texture of either side doesn't print
        // into the correction.
        let sum = box_blur(&v, ew, eh, smooth);
        let band: Vec<f32> = (0..ew * eh)
            .map(|k| {
                if known[k] > 0.0 && weight[k] > 0.0 {
                    sum[k] / weight[k]
                } else {
                    0.0
                }
            })
            .collect();
        let mut filled = push_pull(&band, &known, ew, eh);
        // A last smoothing inside only, so coarse levels leave no steps.
        let soft = box_blur(&filled, ew, eh, smooth.max(2));
        for k in 0..ew * eh {
            if known[k] == 0.0 {
                filled[k] = soft[k];
            }
        }
        filled
    });
    (planes, ew, (ex0, ey0))
}

/// Fill the unknown (`weight` 0) values of a plane smoothly from the known
/// ones: average down to coarser levels until everything is covered, then
/// fill each level's gaps from the one above (Gortler et al.'s push-pull).
fn push_pull(values: &[f32], weight: &[f32], w: usize, h: usize) -> Vec<f32> {
    if w <= 1 && h <= 1 || weight.iter().all(|&k| k >= 1.0) {
        return values.to_vec();
    }
    let (cw, ch) = (w.div_ceil(2), h.div_ceil(2));
    let mut cv = vec![0.0f32; cw * ch];
    let mut cwgt = vec![0.0f32; cw * ch];
    for j in 0..h {
        for i in 0..w {
            let (k, ck) = (j * w + i, (j / 2) * cw + i / 2);
            cv[ck] += values[k] * weight[k];
            cwgt[ck] += weight[k];
        }
    }
    for k in 0..cw * ch {
        if cwgt[k] > 0.0 {
            cv[k] /= cwgt[k];
        }
        cwgt[k] = cwgt[k].min(1.0);
    }
    if cwgt.iter().all(|&k| k == 0.0) {
        return values.to_vec();
    }
    let coarse = push_pull(&cv, &cwgt, cw, ch);
    // Bilinear up from the coarse level, for the parts this level lacks.
    let up = |x: f32, y: f32| {
        let x = (x / 2.0 - 0.25).clamp(0.0, (cw - 1) as f32);
        let y = (y / 2.0 - 0.25).clamp(0.0, (ch - 1) as f32);
        let (x0, y0) = (x.floor() as usize, y.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(cw - 1), (y0 + 1).min(ch - 1));
        let (fx, fy) = (x - x0 as f32, y - y0 as f32);
        let g = |xx: usize, yy: usize| coarse[yy * cw + xx];
        let top = g(x0, y0) + (g(x1, y0) - g(x0, y0)) * fx;
        let bot = g(x0, y1) + (g(x1, y1) - g(x0, y1)) * fx;
        top + (bot - top) * fy
    };
    (0..w * h)
        .map(|k| {
            let a = weight[k].min(1.0);
            values[k] * a + up((k % w) as f32, (k / w) as f32) * (1.0 - a)
        })
        .collect()
}

/// A donor for a new round spot at `centre` with `radius` (fractions; the
/// radius of the frame width), found on `img` (the developed output for
/// `frame`/`crop`). Returns the donor centre in fractions.
pub fn find_source(
    img: &Image,
    frame: (usize, usize),
    crop: Option<&CropGeometry>,
    centre: (f64, f64),
    radius: f64,
) -> (f64, f64) {
    let spot = Spot {
        kind: SpotKind::Heal,
        shape: SpotShape::Ellipse {
            centre,
            size: (radius, radius),
        },
        source: None,
        feather: DEFAULT_FEATHER,
        opacity: 1.0,
    };
    find_spot_source(img, frame, crop, &spot).unwrap_or(centre)
}

/// A donor for `spot`, round or brushed, searched for its own shape on
/// `img` (the developed output for `frame`/`crop`): the centre its centre
/// copies from, in fractions. `None` when the spot is off the picture.
pub fn find_spot_source(
    img: &Image,
    frame: (usize, usize),
    crop: Option<&CropGeometry>,
    spot: &Spot,
) -> Option<(f64, f64)> {
    let f = Frame::new(frame, crop);
    let p = place(spot, &f, img.width, img.height)?;
    let bounds = (0.0, 0.0, img.width as f64, img.height as f64);
    let (ox, oy) = search_offset(img, &p, spot.kind == SpotKind::Clone, bounds);
    let (sx, sy) = f.source(p.centre.0 + ox, p.centre.1 + oy);
    Some(((sx + 0.5) / f.width, (sy + 0.5) / f.height))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn log_luma(p: [f32; 3]) -> f32 {
        let lw = luminance_weights();
        (lw[0] * p[0] + lw[1] * p[1] + lw[2] * p[2]).max(1e-5).ln()
    }

    fn text(s: &str) -> Value {
        Value::Text(s.into())
    }

    fn area(spot_type: &str, x: f64, y: f64, r: f64, source: Option<(f64, f64)>) -> Value {
        let kind = match spot_type {
            "clone" => SpotKind::Clone,
            "heal_patchmatch" => SpotKind::ContentAware,
            _ => SpotKind::Heal,
        };
        Value::Struct(retouch::new_area(kind, (x, y), r, source, false, 0.2, 1.0))
    }

    fn settings(areas: Vec<Value>) -> BTreeMap<String, Value> {
        [(RETOUCH_TAG.to_string(), Value::Seq(areas))]
            .into_iter()
            .collect()
    }

    /// A smooth gradient with a dark blob at (30, 20) and a bright one at (70, 20).
    fn scene() -> Image {
        Image::from_fn(100, 40, |x, y| {
            let base = 0.2 + 0.002 * x as f32;
            let d = |cx: f32, cy: f32| ((x as f32 - cx).powi(2) + (y as f32 - cy).powi(2)).sqrt();
            let mut v = base;
            if d(30.0, 20.0) < 3.0 {
                v *= 0.1;
            }
            if d(70.0, 20.0) < 3.0 {
                v *= 3.0;
            }
            [v, v * 0.9, v * 0.8]
        })
    }

    #[test]
    fn heal_removes_a_blob_and_matches_the_surroundings() {
        // Donor 10 px to the right: brighter base there, which the heal corrects.
        let s = settings(vec![area("heal", 0.30, 0.5, 0.05, Some((0.40, 0.5)))]);
        let (spots, notes) = Spots::from_settings(&s);
        assert!(notes.is_empty());
        let mut img = scene();
        apply(&mut img, &spots, (100, 40), None);
        let healed = img.get(30, 20)[0];
        let expect = 0.2 + 0.002 * 30.0;
        assert!((healed - expect).abs() < 0.01, "{healed} vs {expect}");
        // Far away nothing changed.
        assert_eq!(img.get(90, 5), scene().get(90, 5));
    }

    #[test]
    fn clone_copies_the_donor_as_it_is() {
        let s = settings(vec![area("clone", 0.30, 0.5, 0.05, Some((0.40, 0.5)))]);
        let (spots, _) = Spots::from_settings(&s);
        let mut img = scene();
        apply(&mut img, &spots, (100, 40), None);
        // The donor's base is 0.002 × 10 brighter, and a clone keeps that.
        let got = img.get(30, 20)[0];
        assert!((got - (0.2 + 0.002 * 40.0)).abs() < 0.005, "{got}");
    }

    #[test]
    fn content_aware_finds_its_own_donor() {
        let s = settings(vec![area("heal_patchmatch", 0.70, 0.5, 0.05, None)]);
        let (spots, notes) = Spots::from_settings(&s);
        assert_eq!(notes.len(), 1, "{notes:?}");
        let mut img = scene();
        apply(&mut img, &spots, (100, 40), None);
        let healed = img.get(70, 20)[0];
        let expect = 0.2 + 0.002 * 70.0;
        assert!((healed - expect).abs() < 0.02, "{healed} vs {expect}");
        // The search avoids the dark blob as a donor.
        let (sx, _) = find_source(&scene(), (100, 40), None, (0.70, 0.5), 0.05);
        assert!((sx - 0.30).abs() > 0.05, "{sx}");
    }

    #[test]
    fn legacy_retouch_info_and_the_panel_switch() {
        let mut s: BTreeMap<String, Value> = [(
            LEGACY_TAG.to_string(),
            Value::Seq(vec![text(
                "centerX = 0.3, centerY = 0.5, radius = 0.05, sourceState = sourceSetExplicitly, sourceX = 0.4, sourceY = 0.5, spotType = heal",
            )]),
        )]
        .into_iter()
        .collect();
        let (spots, _) = Spots::from_settings(&s);
        assert_eq!(spots.spots.len(), 1);
        s.insert(ENABLE_TAG.into(), Value::Bool(false));
        assert!(Spots::from_settings(&s).0.is_empty());
    }

    /// Skin-like texture: a soft gradient with fine grain and pores, and a
    /// curved dark scar across it. Returns (with scar, clean).
    fn textured(w: usize, h: usize) -> (Image, Image) {
        let hash = |x: usize, y: usize| {
            let mut v = (x as u32).wrapping_mul(374761393) ^ (y as u32).wrapping_mul(668265263);
            v = (v ^ (v >> 13)).wrapping_mul(1274126177);
            (v ^ (v >> 16)) as f32 / u32::MAX as f32
        };
        let clean = Image::from_fn(w, h, |x, y| {
            let (fx, fy) = (x as f32 / w as f32, y as f32 / h as f32);
            // Rounded shading, as on a cheek: not a plane over a long stroke.
            let base = 0.25 + 0.15 * fx + 0.08 * (fx * 12.0).sin() * (fy * 7.0).cos();
            let grain = 1.0 + 0.10 * (hash(x, y) - 0.5) + 0.06 * (hash(x / 3, y / 3) - 0.5);
            let v = base * grain;
            [v, v * 0.78, v * 0.66]
        });
        let scar = Image::from_fn(w, h, |x, y| {
            let mut p = clean.get(x, y);
            // y = 60 + 12·sin(x / 18), 3 px wide, from x = 60 to 140.
            let cy = 60.0 + 12.0 * (x as f32 / 18.0).sin();
            if (60..140).contains(&x) && (y as f32 - cy).abs() < 2.0 {
                p = [p[0] * 0.45, p[1] * 0.4, p[2] * 0.45];
            }
            p
        });
        (scar, clean)
    }

    /// Leftover blemish, seams at the edge, and lost texture after healing a
    /// brushed stroke along the scar.
    fn brushed_heal_scores() -> (f64, f64, f64) {
        let (w, h) = (200, 120);
        let (mut img, clean) = textured(w, h);
        let (m, n) = brush::kernel_exponents(0.5);
        let dabs = (60..140)
            .step_by(2)
            .map(|x| brush::Dab {
                x: (x as f64 + 0.5) / w as f64,
                y: (60.0 + 12.0 * (x as f64 / 18.0).sin() + 0.5) / h as f64,
                radius: 6.0 / w as f64,
                deposit: 1.0,
                half_m: (m / 2.0) as f32,
                n: n as f32,
            })
            .collect();
        let spots = Spots {
            spots: vec![Spot {
                kind: SpotKind::Heal,
                shape: SpotShape::Brush(vec![brush::Stroke { dabs, erase: false }]),
                source: None,
                feather: 0.5,
                opacity: 1.0,
            }],
        };
        apply(&mut img, &spots, (w, h), None);
        let ll = |im: &Image| -> Vec<f32> {
            (0..w * h).map(|k| log_luma(im.get(k % w, k / w))).collect()
        };
        let (a, c) = (ll(&img), ll(&clean));
        let diff: Vec<f32> = a.iter().zip(&c).map(|(a, c)| a - c).collect();
        let low = box_blur(&diff, w, h, 3);
        let inside = |x: usize, y: usize| {
            let cy = 60.0 + 12.0 * (x as f32 / 18.0).sin();
            (62..138).contains(&x) && (y as f32 - cy).abs() < 4.0
        };
        let edge = |x: usize, y: usize| {
            let cy = 60.0 + 12.0 * (x as f32 / 18.0).sin();
            (62..138).contains(&x) && ((y as f32 - cy).abs() - 7.0).abs() < 2.0
        };
        let (mut res, mut nr, mut seam, mut ns) = (0.0, 0, 0.0, 0);
        for y in 1..h - 1 {
            for x in 1..w - 1 {
                if inside(x, y) {
                    res += low[y * w + x].abs() as f64;
                    nr += 1;
                }
                if edge(x, y) {
                    let gx = low[y * w + x + 1] - low[y * w + x - 1];
                    let gy = low[(y + 1) * w + x] - low[(y - 1) * w + x];
                    seam += (gx * gx + gy * gy).sqrt() as f64;
                    ns += 1;
                }
            }
        }
        // Texture: high-pass energy inside, healed against clean.
        let hp = |v: &[f32]| -> f64 {
            let b = box_blur(v, w, h, 2);
            let (mut e, mut k) = (0.0, 0);
            for y in 0..h {
                for x in 0..w {
                    if inside(x, y) {
                        e += ((v[y * w + x] - b[y * w + x]) as f64).powi(2);
                        k += 1;
                    }
                }
            }
            (e / k as f64).sqrt()
        };
        (res / nr as f64, seam / ns as f64, hp(&a) / hp(&c))
    }

    #[test]
    fn brushed_heal_blends_in() {
        // Before 2026-10-22 (a plane fitted to the edge, a donor for the
        // stroke's bounding circle): residual 0.087, seam 0.027, texture 0.84.
        let (residual, seam, texture) = brushed_heal_scores();
        eprintln!("residual {residual:.4} seam {seam:.4} texture {texture:.3}");
        assert!(residual < 0.03, "the scar shows through: {residual}");
        assert!(seam < 0.012, "a seam at the edge: {seam}");
        assert!(texture > 0.85, "the texture is smeared: {texture}");
    }
}
