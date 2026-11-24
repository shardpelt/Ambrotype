// SPDX-License-Identifier: GPL-3.0-or-later
//! Place a spot in a zone: find the exposure that prints the spot the photo
//! shows at a point as a chosen zone (0 to X, one stop each, V is 18 % grey).

use super::*;
use amb_engine::negative::{PROBE_BLOCK, srgb8_linear_luma, zone_exact};

/// The search stops when the spot is this close to the target, in zones.
const ZONE_TOLERANCE: f64 = 0.1;
/// The most renders a search makes (the first, at the setting as it is, counts).
const MAX_RENDERS: usize = 10;
/// The render's long side in pixels.
const PLACE_SIZE: u32 = 512;
/// The zones there are: 0 to X.
const ZONE_COUNT: u8 = 11;
/// A step of the search stays this far (a share of the bracket) from its ends,
/// so it always makes headway.
const BRACKET_MARGIN: f64 = 0.1;

#[derive(Debug, Clone)]
pub struct PlaceOptions {
    /// The spot: fractions (0..1) of the cropped photo, from the top left.
    pub at: [f64; 2],
    /// The zone to put it in, 0 to 10.
    pub zone: u8,
    /// The setting to move; `None` is [`exposure_setting`].
    pub setting: Option<String>,
    /// The render's long side in pixels.
    pub size: u32,
}

impl Default for PlaceOptions {
    fn default() -> Self {
        PlaceOptions {
            at: [0.5, 0.5],
            zone: 5,
            setting: None,
            size: PLACE_SIZE,
        }
    }
}

/// What a placement found. Nothing is saved: `changes` is for the caller to apply.
#[derive(Debug, Clone, Serialize)]
pub struct Placed {
    /// The setting that moved.
    pub setting: String,
    pub from: f64,
    pub to: f64,
    /// The spot's zone (not rounded) before and after.
    pub zone_before: f64,
    pub zone_after: f64,
    /// False when the setting's range ends short of the zone: `to` is the end.
    pub reached: bool,
    /// The setting's new value, or nothing when it does not move.
    pub changes: BTreeMap<String, f64>,
}

/// The mean over a [`PROBE_BLOCK`] block at `at`, in zones (not rounded).
fn spot_zone(img: &RgbImage, at: [f64; 2]) -> f64 {
    let (w, h) = (img.width() as usize, img.height() as usize);
    let centre = |f: f64, n: usize| ((f.clamp(0.0, 1.0) * n as f64) as usize).min(n - 1);
    let (cx, cy) = (centre(at[0], w), centre(at[1], h));
    let half = PROBE_BLOCK / 2;
    let (mut sum, mut n) = (0.0, 0.0);
    for y in cy.saturating_sub(half)..=(cy + half).min(h - 1) {
        for x in cx.saturating_sub(half)..=(cx + half).min(w - 1) {
            sum += srgb8_linear_luma(img.get_pixel(x as u32, y as u32).0);
            n += 1.0;
        }
    }
    zone_exact(sum / n)
}

/// Move the exposure until the spot at `o.at` prints in zone `o.zone`: the
/// search brackets the zone between the setting now and the end of its range
/// that moves the print the right way, then closes in (false position, kept
/// inside the bracket) until it is within [`ZONE_TOLERANCE`] or out of renders.
pub fn place_in_zone(
    photo: &Path,
    settings: &BTreeMap<String, Value>,
    o: &PlaceOptions,
    progress: &dyn Fn(usize, usize),
) -> Result<Placed, String> {
    if o.zone >= ZONE_COUNT {
        return Err("a zone is 0 to 10 (0 to X)".into());
    }
    if !o.at.iter().all(|v| v.is_finite()) {
        return Err("the spot must be two numbers".into());
    }
    let name = o
        .setting
        .clone()
        .unwrap_or_else(|| exposure_setting(settings).to_string());
    let (lo, hi) = (clamp(&name, f64::MIN)?, clamp(&name, f64::MAX)?);
    if !lo.is_finite() || !hi.is_finite() {
        return Err(format!("{name} has no range to search"));
    }
    let size = o.size.clamp(SIZE_RANGE.0, SIZE_RANGE.1);
    let developer = Developer::load(photo, settings, size)?;
    let target = f64::from(o.zone);
    let renders = std::cell::Cell::new(0usize);
    let measure = |v: f64| {
        progress(renders.get(), MAX_RENDERS);
        renders.set(renders.get() + 1);
        spot_zone(&developer.render(&with(settings, &name, v), size), o.at)
    };
    let from = clamp(&name, current(settings, &name))?;
    let before = measure(from);
    let mut best = (from, before);
    let close = |z: f64| (z - target).abs() < ZONE_TOLERANCE;
    if !close(before) {
        // The end of the range that moves the print towards the target.
        let up = (target > before) == (lighter_sign(&name) > 0.0);
        let end = if up { hi } else { lo };
        let at_end = measure(end);
        best = (end, at_end);
        // Bracket: `a` and `b` have the target between their zones.
        if (at_end - target) * (before - target) <= 0.0 {
            let (mut a, mut b) = ((from, before), (end, at_end));
            while !close(best.1) && renders.get() < MAX_RENDERS {
                let t = ((target - a.1) / (b.1 - a.1)).clamp(BRACKET_MARGIN, 1.0 - BRACKET_MARGIN);
                let v = a.0 + t * (b.0 - a.0);
                let z = measure(v);
                best = (v, z);
                if (z - target) * (a.1 - target) <= 0.0 {
                    b = (v, z);
                } else {
                    a = (v, z);
                }
            }
        }
    }
    progress(MAX_RENDERS, MAX_RENDERS);
    let (to, zone_after) = best;
    Ok(Placed {
        reached: close(zone_after),
        changes: if (to - from).abs() < 1e-9 {
            BTreeMap::new()
        } else {
            BTreeMap::from([(name.clone(), to)])
        },
        setting: name,
        from,
        to,
        zone_before: before,
        zone_after,
    })
}
