// SPDX-License-Identifier: GPL-3.0-or-later
//! The donor search for new spots, and the footprints of a region render.

use super::heal::search_offset;
use super::inpaint::{FILL_CONTEXT, FILL_MIN_HALF};
use super::{DEFAULT_FEATHER, Spot, SpotShape, Spots, place};
use crate::buffer::Image;
use crate::crop::CropGeometry;
use crate::mask::Frame;
use amb_presets::retouch::SpotKind;

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
            // A Remove spot's fill sees a square box that can slide by up
            // to its own half at the photo's edges.
            let donor = if spot.kind.synthesised() {
                let half = (r * FILL_CONTEXT).max(FILL_MIN_HALF);
                let fill = around(centre, 2.0 * half + 2.0);
                [
                    fill[0].min(donor[0]),
                    fill[1].min(donor[1]),
                    fill[2].max(donor[2]),
                    fill[3].max(donor[3]),
                ]
            } else {
                donor
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
