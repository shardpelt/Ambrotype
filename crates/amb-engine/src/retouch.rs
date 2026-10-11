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

fn log_luma(p: [f32; 3]) -> f32 {
    let lw = luminance_weights();
    (lw[0] * p[0] + lw[1] * p[1] + lw[2] * p[2]).max(1e-5).ln()
}

/// The donor offset (output pixels) whose surroundings best match the
/// spot's: candidates around the spot at 1.6 to 4 radii in 16 directions,
/// scored on log luminance along two rings, plus how much busier the
/// candidate's inside is than its own surroundings (a donor holding an object
/// is; a stretch of the same edge or texture isn't).
fn search_offset(
    img: &Image,
    centre: (f64, f64),
    radius: f64,
    (x0, y0, x1, y1): (f64, f64, f64, f64),
) -> (f64, f64) {
    let r = radius.max(1.0);
    let ring: Vec<(f64, f64)> = (0..32)
        .flat_map(|i| {
            let t = i as f64 / 32.0 * std::f64::consts::TAU;
            [1.25, 1.6].map(|k| (t.cos() * r * k, t.sin() * r * k))
        })
        .collect();
    let inside: Vec<(f64, f64)> = (0..16)
        .flat_map(|i| {
            let t = i as f64 / 16.0 * std::f64::consts::TAU;
            [0.0, 0.45, 0.85].map(|k| (t.cos() * r * k, t.sin() * r * k))
        })
        .collect();
    let target: Vec<f32> = ring
        .iter()
        .map(|&(dx, dy)| log_luma(sample(img, centre.0 + dx, centre.1 + dy)))
        .collect();
    let mut best = ((r * 2.2, 0.0), f64::INFINITY);
    for dist in [1.6, 2.2, 3.0, 4.0] {
        for i in 0..16 {
            let t = (i as f64 + 0.5 * (dist as i32 % 2) as f64) / 16.0 * std::f64::consts::TAU;
            let off = (t.cos() * r * dist, t.sin() * r * dist);
            let (cx, cy) = (centre.0 + off.0, centre.1 + off.1);
            // The donor and its ring must be in the picture.
            let reach = r * 1.6;
            if cx - reach < x0 || cy - reach < y0 || cx + reach > x1 || cy + reach > y1 {
                continue;
            }
            let around: Vec<f64> = ring
                .iter()
                .map(|&(dx, dy)| log_luma(sample(img, cx + dx, cy + dy)) as f64)
                .collect();
            let ssd: f64 = around
                .iter()
                .zip(&target)
                .map(|(a, &t)| (a - t as f64).powi(2))
                .sum::<f64>()
                / ring.len() as f64;
            let lum: Vec<f64> = inside
                .iter()
                .map(|&(dx, dy)| log_luma(sample(img, cx + dx, cy + dy)) as f64)
                .collect();
            let var = |v: &[f64]| {
                let mean = v.iter().sum::<f64>() / v.len() as f64;
                v.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / v.len() as f64
            };
            let busier = (var(&lum) - var(&around)).max(0.0);
            // Prefer near donors a little: the scene changes with distance.
            let score = ssd + busier + 0.002 * dist;
            if score < best.1 {
                best = (off, score);
            }
        }
    }
    best.0
}

/// Fit `a + b·x + c·y` to (x, y, v) by least squares; constant when the
/// points can't carry a plane.
fn fit_plane(pts: &[(f64, f64, f64)]) -> [f64; 3] {
    let n = pts.len() as f64;
    if pts.is_empty() {
        return [0.0; 3];
    }
    let mean = pts.iter().map(|p| p.2).sum::<f64>() / n;
    if pts.len() < 12 {
        return [mean, 0.0, 0.0];
    }
    let (mut sx, mut sy, mut sxx, mut syy, mut sxy, mut sv, mut sxv, mut syv) =
        (0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    for &(x, y, v) in pts {
        sx += x;
        sy += y;
        sxx += x * x;
        syy += y * y;
        sxy += x * y;
        sv += v;
        sxv += x * v;
        syv += y * v;
    }
    // Normal equations [n sx sy; sx sxx sxy; sy sxy syy] · [a b c] = [sv sxv syv].
    let m = [[n, sx, sy], [sx, sxx, sxy], [sy, sxy, syy]];
    let rhs = [sv, sxv, syv];
    let det = |m: [[f64; 3]; 3]| {
        m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
            - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
            + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
    };
    let d = det(m);
    if d.abs() < 1e-9 {
        return [mean, 0.0, 0.0];
    }
    let mut out = [0.0; 3];
    for (k, o) in out.iter_mut().enumerate() {
        let mut mk = m;
        for row in 0..3 {
            mk[row][k] = rhs[row];
        }
        *o = det(mk) / d;
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
        let offset = match spot.source {
            Some((sx, sy)) => {
                let (fx, fy) = frame.pixel(sx, sy);
                let (tx, ty) = frame.target(fx, fy);
                (tx - p.centre.0, ty - p.centre.1)
            }
            None => search_offset(img, p.centre, p.radius, bounds),
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
                // Candidates up to 4 radii away, with their rings.
                None => around(centre, 5.6 * r.max(1.0) + 3.0),
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
    let snapshot = img.clone();
    // The plane: log(target) − log(donor) over a ring just outside the spot.
    let plane = if clone {
        None
    } else {
        let margin = (p.radius * 0.35).clamp(2.0, 40.0) as usize;
        let (ex0, ey0) = (p.x0.saturating_sub(margin), p.y0.saturating_sub(margin));
        let ex1 = (p.x0 + p.w + margin).min(img.width);
        let ey1 = (p.y0 + p.h + margin).min(img.height);
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
        let mut pts: [Vec<(f64, f64, f64)>; 3] = [Vec::new(), Vec::new(), Vec::new()];
        let norm = p.radius.max(1.0);
        for j in 0..eh {
            for i in 0..ew {
                let k = j * ew + i;
                if inside[k] > 0.0 || near[k] <= 0.0 {
                    continue;
                }
                let (x, y) = ((ex0 + i) as f64, (ey0 + j) as f64);
                let t = snapshot.get(ex0 + i, ey0 + j);
                let d = sample(&snapshot, x + offset.0, y + offset.1);
                let (rx, ry) = ((x - p.centre.0) / norm, (y - p.centre.1) / norm);
                for c in 0..3 {
                    let v = (t[c].max(1e-5).ln() - d[c].max(1e-5).ln()) as f64;
                    pts[c].push((rx, ry, v));
                }
            }
        }
        Some([fit_plane(&pts[0]), fit_plane(&pts[1]), fit_plane(&pts[2])])
    };
    let norm = p.radius.max(1.0);
    for j in 0..p.h {
        for i in 0..p.w {
            let a = p.alpha[j * p.w + i];
            if a <= 0.0 {
                continue;
            }
            let (x, y) = ((p.x0 + i) as f64, (p.y0 + j) as f64);
            let mut d = sample(&snapshot, x + offset.0, y + offset.1);
            if let Some(pl) = &plane {
                let (rx, ry) = ((x - p.centre.0) / norm, (y - p.centre.1) / norm);
                for c in 0..3 {
                    let g = (pl[c][0] + pl[c][1] * rx + pl[c][2] * ry).clamp(-6.0, 6.0);
                    d[c] *= g.exp() as f32;
                }
            }
            let t = snapshot.get(p.x0 + i, p.y0 + j);
            let out = [0, 1, 2].map(|c| t[c] + (d[c] - t[c]) * a);
            img.set(p.x0 + i, p.y0 + j, out);
        }
    }
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
    let f = Frame::new(frame, crop);
    let (cfx, cfy) = f.pixel(centre.0, centre.1);
    let (cx, cy) = f.target(cfx, cfy);
    let r = radius * f.width * f.scale();
    let bounds = (0.0, 0.0, img.width as f64, img.height as f64);
    let (ox, oy) = search_offset(img, (cx, cy), r, bounds);
    let (sx, sy) = f.source(cx + ox, cy + oy);
    ((sx + 0.5) / f.width, (sy + 0.5) / f.height)
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn plane_fit() {
        let pts: Vec<(f64, f64, f64)> = (0..50)
            .map(|i| {
                let (x, y) = ((i % 7) as f64, (i / 7) as f64);
                (x, y, 1.0 + 0.5 * x - 0.25 * y)
            })
            .collect();
        let p = fit_plane(&pts);
        assert!(
            (p[0] - 1.0).abs() < 1e-9 && (p[1] - 0.5).abs() < 1e-9 && (p[2] + 0.25).abs() < 1e-9
        );
    }
}
