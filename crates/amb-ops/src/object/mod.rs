// SPDX-License-Identifier: GPL-3.0-or-later
//! Remove object: a click on something (or a box round it), its outline
//! found by the object selection model (SAM, when installed; for a box
//! without it, the Subject model on just the box), and one AI Remove spot
//! over all of it.
//!
//! Removing an object in several spots goes wrong: each fill sees the rest
//! of the object and brings it back. One spot over the whole outline,
//! widened a little, fills well. The spot is a brushed Remove spot whose
//! dabs cover the outline (`add_spot` with `separate`), so it is an
//! ordinary Lightroom spot: it undoes, moves and shows like one by hand.

use std::collections::BTreeMap;

use amb_engine::pipeline::DevelopSettings;
use amb_engine::raw::{self, CameraImage};
use amb_presets::Value;
use amb_protocol::Command;

mod geometry;
mod remove;
mod select;
#[cfg(test)]
mod tests;
mod words;

pub use geometry::cover;
pub use select::select;
pub use words::select_words;

/// The long edge the photo is looked at, in pixels.
const EDGE: usize = 2048;
/// Context round the box the model sees, as a share of the box's size.
pub(super) const CONTEXT: f64 = 0.15;
/// The outline is widened by this share of the box's longer side (and at
/// least `MIN_WIDEN` pixels at `EDGE`): shadows, soft edges and the model's
/// blur round the object go with it.
pub(super) const WIDEN: f64 = 0.05;
pub(super) const MIN_WIDEN: f64 = 4.0;
/// The outline snaps to the photo's edges within this many pixels (at
/// `EDGE`; the model's map has a cell every 8), following colour edges at
/// least this strong (`snap::guided`).
pub(super) const SNAP_RADIUS: usize = 8;
pub(super) const SNAP_EPS: f32 = 1e-3;

/// What to remove: a box round it, or a click on it (fractions of the
/// uncropped photo as stored, as spots are).
#[derive(Debug, Clone)]
pub enum Target {
    Box([f64; 4]),
    Click([f64; 2]),
    /// A click on a part of something: only that part.
    Part([f64; 2]),
    /// Several clicks: on it (true) and on what is not it (false).
    Clicks(Vec<([f64; 2], bool)>),
}

/// An object selected on the photo: where it is, at `EDGE` pixels on the
/// long side (the whole frame, as spots and masks are placed on it).
#[derive(Debug, Clone)]
pub struct Selection {
    pub mask: Vec<bool>,
    pub width: usize,
    pub height: usize,
    /// The share of the box (or, for clicks, the photo) it fills (0-1).
    pub share: f64,
    /// Found by the object selection model, not the Subject model on a box.
    pub selected: bool,
    lo: (usize, usize),
    hi: (usize, usize),
    /// The object alone, and its shadow (the darker patch joined to it;
    /// empty when there's none), which `set_shadow` adds to `mask`.
    object: Vec<bool>,
    shadow: Vec<bool>,
    with_shadow: bool,
}

/// What was found: the spot to add, how much of the box (or, for a click,
/// the photo) the object fills, and which model found it.
#[derive(Debug)]
pub struct Object {
    pub command: Command,
    /// 0-1.
    pub share: f64,
    /// The object selection model (a click, or a box when it's installed)
    /// rather than the Subject model on the box.
    pub selected: bool,
}

/// The Remove spot for the object at `target`, with the edit's `settings`.
/// A click needs the object selection model (`subject::set_selector`); a
/// box uses it when it's there, else the Subject model on just the box.
pub fn find(
    img: &CameraImage,
    settings: &BTreeMap<String, Value>,
    target: Target,
) -> Result<Object, String> {
    Ok(select(img, settings, &target)?.remove())
}

/// The whole frame (as spots and masks are placed on it), developed with
/// `settings`, `EDGE` pixels on the long side: sRGB 0–1, interleaved.
pub(super) fn frame(
    img: &CameraImage,
    settings: &BTreeMap<String, Value>,
) -> (Vec<f32>, usize, usize) {
    let (mut develop, _) = DevelopSettings::from_settings(settings);
    develop.crop.enabled = false;
    let small = raw::downscale(img, EDGE);
    let out = amb_engine::pipeline::render(&small, &develop);
    let (w, h) = (out.width, out.height);
    (amb_engine::output::to_srgb(&out), w, h)
}

/// How like some words a picture is (sRGB 0–1, interleaved, `w`×`h`).
pub type Like<'a> = dyn Fn(&[f32], usize, usize) -> Result<f32, String> + Sync + 'a;
