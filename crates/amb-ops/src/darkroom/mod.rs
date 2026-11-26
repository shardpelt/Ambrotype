// SPDX-License-Identifier: GPL-3.0-or-later
//! Darkroom aids that print the photo several ways on one sheet: a test
//! strip (one setting stepped across the photo, as a darkroom strip of paper
//! is exposed in bands), a ring-around (the photo nine times, each a step
//! away in colour or lightness from the middle one) and Pick Better's pair
//! (a step down and a step up, side by side). Each cell says what it
//! changes, so the editor can apply the one the user picks.
//!
//! Renders go through the same path as Edit Replay: a copy of the raw at
//! about the sheet's size, the edit's settings with no lens or camera
//! profiles, the GPU when there is one.

mod pick;
mod ring;
mod strip;
#[cfg(test)]
mod tests;
mod zone;

pub use pick::{PickOptions, pick_pair, pick_settings, pick_step};
pub use ring::{RingOptions, ring_around};
pub use strip::{StripOptions, test_strip};
pub use zone::{PlaceOptions, Placed, place_in_zone};

use std::collections::BTreeMap;
use std::path::Path;

use amb_engine::buffer::Image;
use amb_engine::matching;
use amb_engine::raw::CameraImage;
use amb_presets::Value;
use amb_presets::registry::Registry;
use amb_protocol::DevelopState;
use image::RgbImage;
use serde::Serialize;

/// The bounds on a sheet's or a cell's long side, in pixels.
const SIZE_RANGE: (u32, u32) = (64, 4096);
/// The setting a strip steps when none is named, by how the photo prints.
const EXPOSURE_PAPER: &str = "amb:PaperSoftExposure";
const EXPOSURE_NEGATIVE: &str = "amb:NegativeExposure";
const EXPOSURE_PLAIN: &str = "Exposure2012";
const FLAG_PAPER: &str = "amb:PaperCurve";
/// The paper's Soft exposure only prints with split grade on.
const FLAG_SPLIT: &str = "amb:PaperSplitGrade";
const FLAG_NEGATIVE: &str = "amb:NegativeConvert";

/// Settings where more exposure prints darker, as on paper (the others, in
/// the negative's own exposure, print lighter).
const MORE_IS_DARKER: [&str; 1] = [EXPOSURE_PAPER];

/// One cell of an aid: where it is in the image, what it is called, and the
/// settings to apply (absolute values) to make the photo as it shows. A
/// cell that is "as now" changes nothing.
#[derive(Debug, Clone, Serialize)]
pub struct Cell {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    pub label: String,
    pub changes: BTreeMap<String, f64>,
}

/// A sheet and its cells. The pixels stay out of the JSON.
#[derive(Debug, Clone, Serialize)]
pub struct Aid {
    pub width: u32,
    pub height: u32,
    pub cells: Vec<Cell>,
    #[serde(skip)]
    pub image: RgbImage,
}

impl Aid {
    /// The sheet as a PNG.
    pub fn png(&self) -> Result<Vec<u8>, String> {
        let mut out = Vec::new();
        self.image
            .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
            .map_err(|e| e.to_string())?;
        Ok(out)
    }
}

fn flag(settings: &BTreeMap<String, Value>, name: &str) -> bool {
    settings.get(name).and_then(Value::as_bool).unwrap_or(false)
}

/// The setting that makes the print lighter or darker: the paper's Soft
/// exposure with the paper curve and split grade on (without split grade it
/// does nothing), the negative's exposure with the conversion on, else the
/// photo's Exposure.
pub fn exposure_setting(settings: &BTreeMap<String, Value>) -> &'static str {
    if flag(settings, FLAG_PAPER) && flag(settings, FLAG_SPLIT) {
        EXPOSURE_PAPER
    } else if flag(settings, FLAG_NEGATIVE) {
        EXPOSURE_NEGATIVE
    } else {
        EXPOSURE_PLAIN
    }
}

/// Which way `setting` moves to print lighter: `1.0` up, `-1.0` down.
fn lighter_sign(setting: &str) -> f64 {
    if MORE_IS_DARKER.contains(&setting) {
        -1.0
    } else {
        1.0
    }
}

/// A setting's value now: set, else its default, else 0.
fn current(settings: &BTreeMap<String, Value>, name: &str) -> f64 {
    settings
        .get(name)
        .and_then(Value::as_f64)
        .or_else(|| {
            Registry::builtin()
                .lookup(name)
                .and_then(|i| i.default)
                .and_then(|d| d.as_f64())
        })
        .unwrap_or(0.0)
}

/// `value` held to the setting's range in the registry.
fn clamp(name: &str, value: f64) -> Result<f64, String> {
    let info = Registry::builtin()
        .lookup(name)
        .ok_or_else(|| format!("{name} is not a setting"))?;
    if info.kind != "number" {
        return Err(format!("{name} is not a number: a strip steps numbers"));
    }
    Ok(value.clamp(info.min.unwrap_or(f64::MIN), info.max.unwrap_or(f64::MAX)))
}

/// The settings of a photo's sidecar (none without one).
pub fn sidecar_settings(photo: &Path) -> Result<BTreeMap<String, Value>, String> {
    let sidecar = amb_engine::sidecar_path(photo);
    if !sidecar.is_file() {
        return Ok(BTreeMap::new());
    }
    DevelopState::load(&sidecar)
        .map(|s| s.settings().clone())
        .map_err(|e| format!("{}: {e}", sidecar.display()))
}

/// The photo loaded once, at about the size the sheet needs.
struct Developer {
    img: CameraImage,
    orientation: u16,
}

impl Developer {
    /// `edge`: the longest side a render will be asked for. A crop in
    /// `settings` makes the copy larger, so renders have the pixels.
    fn load(photo: &Path, settings: &BTreeMap<String, Value>, edge: u32) -> Result<Self, String> {
        let source = amb_engine::source_path(photo);
        let full =
            amb_engine::raw::load(&source).map_err(|e| format!("{}: {e}", source.display()))?;
        let (cw, ch) = amb_engine::pipeline::output_size(&full, &matching::develop_plain(settings));
        let crop = full.width.max(full.height) as f64 / cw.max(ch).max(1) as f64;
        let img = amb_engine::raw::downscale(&full, (f64::from(edge) * crop.max(1.0)) as usize);
        Ok(Developer {
            orientation: img.metadata.orientation,
            img,
        })
    }

    /// The photo with `settings`, as 8-bit sRGB with `edge` on its long side.
    fn render(&self, settings: &BTreeMap<String, Value>, edge: u32) -> RgbImage {
        let develop = matching::develop_plain(settings);
        let (out, _) = amb_gpu::full::render(amb_gpu::full::shared(), &self.img, &develop);
        let out: Image = amb_export::geometry::orient(&out, self.orientation);
        let s = f64::from(edge) / out.width.max(out.height).max(1) as f64;
        let (w, h) = (
            ((out.width as f64 * s).round() as usize).max(1),
            ((out.height as f64 * s).round() as usize).max(1),
        );
        let small = amb_export::geometry::resize(&out, w, h);
        let srgb = amb_engine::output::to_srgb(&small);
        let bytes = srgb
            .iter()
            .map(|v| (v * 255.0).round().clamp(0.0, 255.0) as u8)
            .collect();
        RgbImage::from_raw(w as u32, h as u32, bytes).expect("w * h * 3 bytes were made")
    }
}

/// `settings` with `name` set to `value`.
fn with(settings: &BTreeMap<String, Value>, name: &str, value: f64) -> BTreeMap<String, Value> {
    let mut s = settings.clone();
    s.insert(name.to_string(), Value::Number(value));
    s
}

/// "+1/3", "-2/3", "+1 1/3" for a move in stops that is a whole number of
/// thirds; "+0.25" otherwise. `unit_stops` is false for other settings.
fn delta_label(delta: f64, stops: bool) -> String {
    if delta.abs() < 1e-9 {
        return "As now".into();
    }
    let sign = if delta < 0.0 { '-' } else { '+' };
    let thirds = delta.abs() * 3.0;
    if stops && (thirds - thirds.round()).abs() < 1e-6 {
        let t = thirds.round() as u32;
        let (whole, rem) = (t / 3, t % 3);
        return match (whole, rem) {
            (w, 0) => format!("{sign}{w}"),
            (0, r) => format!("{sign}{r}/3"),
            (w, r) => format!("{sign}{w} {r}/3"),
        };
    }
    let text = format!("{:.2}", delta.abs());
    format!("{sign}{}", text.trim_end_matches('0').trim_end_matches('.'))
}
