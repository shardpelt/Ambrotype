// SPDX-License-Identifier: GPL-3.0-or-later
//! Darkroom aids that print the photo several ways on one sheet: a test
//! strip (one setting stepped across the photo, as a darkroom strip of paper
//! is exposed in bands) and a ring-around (the photo nine times, each a step
//! away in colour or lightness from the middle one). Each cell says what it
//! changes, so the editor can apply the one the user picks.
//!
//! Renders go through the same path as Edit Replay: a copy of the raw at
//! about the sheet's size, the edit's settings with no lens or camera
//! profiles, the GPU when there is one.

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

/// Test strip: how many bands, how far apart (stops, as exposure is
/// stepped by default), and the sheet's long side in pixels.
const STRIP_STEPS: usize = 7;
const STRIP_STEP: f64 = 1.0 / 3.0;
const STRIP_SIZE: u32 = 1200;
/// Ring-around: the colour step in slider units (of -100..100), the
/// lightness step in stops, and each cell's long side in pixels.
const RING_STEP: f64 = 15.0;
const RING_EXPOSURE_STEP: f64 = 1.0 / 3.0;
const RING_SIZE: u32 = 400;
/// The gap between ring-around cells, in pixels.
const RING_GAP: u32 = 4;
/// The gap shows this grey, 8-bit.
const RING_GAP_GREY: u8 = 24;
/// The bounds on a sheet's or a cell's long side, in pixels.
const SIZE_RANGE: (u32, u32) = (64, 4096);
/// The most bands in a strip.
const STRIP_MAX_STEPS: usize = 15;

/// The setting a strip steps when none is named, by how the photo prints.
const EXPOSURE_PAPER: &str = "amb:PaperSoftExposure";
const EXPOSURE_NEGATIVE: &str = "amb:NegativeExposure";
const EXPOSURE_PLAIN: &str = "Exposure2012";
const FLAG_PAPER: &str = "amb:PaperCurve";
/// The paper's Soft exposure only prints with split grade on.
const FLAG_SPLIT: &str = "amb:PaperSplitGrade";
const FLAG_NEGATIVE: &str = "amb:NegativeConvert";

/// The ring's colour moves: the cell's place (column, row; 0..3, from the
/// top left), its label, the setting and the direction it moves in. A
/// positive value of each is more red, green or blue (`negative/mod.rs`).
const RING_COLOURS: [((usize, usize), &str, &str, f64); 6] = [
    ((0, 0), "More red", "amb:NegativeCyanRed", 1.0),
    ((1, 0), "More yellow", "amb:NegativeYellowBlue", -1.0),
    ((2, 0), "More green", "amb:NegativeMagentaGreen", 1.0),
    ((2, 2), "More cyan", "amb:NegativeCyanRed", -1.0),
    ((1, 2), "More blue", "amb:NegativeYellowBlue", 1.0),
    ((0, 2), "More magenta", "amb:NegativeMagentaGreen", -1.0),
];
/// The ring's lightness moves: place, label, and which way the print goes
/// (`1.0` lighter, `-1.0` darker), as [`lighter_sign`] turns into a direction
/// of the exposure setting.
const RING_LIGHT: [((usize, usize), &str, f64); 2] =
    [((2, 1), "Darker", -1.0), ((0, 1), "Lighter", 1.0)];
/// Settings where more exposure prints darker, as on paper (the others, in
/// the negative's own exposure, print lighter).
const MORE_IS_DARKER: [&str; 1] = [EXPOSURE_PAPER];
/// The ring's middle cell.
const RING_MIDDLE: (usize, usize) = (1, 1);

#[derive(Debug, Clone)]
pub struct StripOptions {
    /// How many bands (7).
    pub steps: usize,
    /// How far apart, in the setting's own units (stops for exposure).
    pub step: f64,
    /// The setting to step; `None` is [`exposure_setting`].
    pub setting: Option<String>,
    /// The sheet's long side in pixels.
    pub size: u32,
}

impl Default for StripOptions {
    fn default() -> Self {
        StripOptions {
            steps: STRIP_STEPS,
            step: STRIP_STEP,
            setting: None,
            size: STRIP_SIZE,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RingOptions {
    /// The colour step, in slider units.
    pub step: f64,
    /// The lightness step, in stops.
    pub exposure_step: f64,
    /// Each cell's long side in pixels.
    pub size: u32,
}

impl Default for RingOptions {
    fn default() -> Self {
        RingOptions {
            step: RING_STEP,
            exposure_step: RING_EXPOSURE_STEP,
            size: RING_SIZE,
        }
    }
}

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

/// The photo as vertical bands, left to right, each rendered with the
/// setting at another value (a darkroom test strip on one sheet): the values
/// centre on the setting's current one, `step` apart, held to its range.
pub fn test_strip(
    photo: &Path,
    settings: &BTreeMap<String, Value>,
    o: &StripOptions,
    progress: &dyn Fn(usize, usize),
) -> Result<Aid, String> {
    let name = o
        .setting
        .clone()
        .unwrap_or_else(|| exposure_setting(settings).to_string());
    if !(1..=STRIP_MAX_STEPS).contains(&o.steps) {
        return Err(format!("a strip has 1 to {STRIP_MAX_STEPS} steps"));
    }
    if !o.step.is_finite() || o.step <= 0.0 {
        return Err("a strip's step must be more than 0".into());
    }
    let now = current(settings, &name);
    clamp(&name, now)?;
    let centre = (o.steps / 2) as f64;
    let mut values: Vec<f64> = Vec::new();
    for i in 0..o.steps {
        let v = clamp(&name, now + (i as f64 - centre) * o.step)?;
        if !values.iter().any(|x| (x - v).abs() < 1e-9) {
            values.push(v);
        }
    }
    let size = o.size.clamp(SIZE_RANGE.0, SIZE_RANGE.1);
    let developer = Developer::load(photo, settings, size)?;
    let stops = name == EXPOSURE_PAPER || name == EXPOSURE_NEGATIVE || name == EXPOSURE_PLAIN;
    let n = values.len();
    let renders: Vec<RgbImage> = values
        .iter()
        .enumerate()
        .map(|(i, v)| {
            progress(i, n);
            developer.render(&with(settings, &name, *v), size)
        })
        .collect();
    progress(n, n);
    let (width, height) = (renders[0].width(), renders[0].height());
    let mut image = RgbImage::new(width, height);
    let mut cells = Vec::new();
    for (i, (v, render)) in values.iter().zip(&renders).enumerate() {
        let x0 = width * i as u32 / n as u32;
        let x1 = width * (i as u32 + 1) / n as u32;
        for y in 0..height {
            for x in x0..x1 {
                image.put_pixel(x, y, *render.get_pixel(x, y));
            }
        }
        let delta = v - now;
        cells.push(Cell {
            x: x0,
            y: 0,
            w: x1 - x0,
            h: height,
            label: delta_label(delta, stops),
            changes: if delta.abs() < 1e-9 {
                BTreeMap::new()
            } else {
                BTreeMap::from([(name.clone(), *v)])
            },
        });
    }
    Ok(Aid {
        width,
        height,
        cells,
        image,
    })
}

/// A ring-around cell before it is drawn: place, label, changes.
type RingCell = ((usize, usize), String, BTreeMap<String, f64>);

/// The photo nine times in a 3 x 3 grid: the middle as it is now, the
/// others a step more red, yellow, green, darker, cyan, blue, magenta and
/// lighter, going round from the top left. Only for a converted negative.
pub fn ring_around(
    photo: &Path,
    settings: &BTreeMap<String, Value>,
    o: &RingOptions,
    progress: &dyn Fn(usize, usize),
) -> Result<Aid, String> {
    if !flag(settings, FLAG_NEGATIVE) {
        return Err(
            "Ring-around works on a converted negative: switch on Film Negative first.".into(),
        );
    }
    if !o.step.is_finite()
        || o.step <= 0.0
        || !o.exposure_step.is_finite()
        || o.exposure_step <= 0.0
    {
        return Err("a ring-around's steps must be more than 0".into());
    }
    let size = o.size.clamp(SIZE_RANGE.0, SIZE_RANGE.1);
    let exposure = exposure_setting(settings);
    // Every cell: place, label, the settings it changes (absolute values).
    let mut plan: Vec<RingCell> = vec![(RING_MIDDLE, "As now".into(), BTreeMap::new())];
    for (at, label, setting, sign) in RING_COLOURS {
        let v = clamp(setting, current(settings, setting) + sign * o.step)?;
        plan.push((at, label.into(), BTreeMap::from([(setting.to_string(), v)])));
    }
    for (at, label, sign) in RING_LIGHT {
        let v = clamp(
            exposure,
            current(settings, exposure) + sign * lighter_sign(exposure) * o.exposure_step,
        )?;
        plan.push((
            at,
            label.into(),
            BTreeMap::from([(exposure.to_string(), v)]),
        ));
    }
    let developer = Developer::load(photo, settings, size)?;
    let total = plan.len();
    let renders: Vec<RgbImage> = plan
        .iter()
        .enumerate()
        .map(|(i, (_, _, changes))| {
            progress(i, total);
            let mut s = settings.clone();
            for (k, v) in changes {
                s.insert(k.clone(), Value::Number(*v));
            }
            developer.render(&s, size)
        })
        .collect();
    progress(total, total);
    let (cw, ch) = (renders[0].width(), renders[0].height());
    let (width, height) = (3 * cw + 2 * RING_GAP, 3 * ch + 2 * RING_GAP);
    let mut image = RgbImage::from_pixel(width, height, image::Rgb([RING_GAP_GREY; 3]));
    let mut cells = Vec::new();
    for ((at, label, changes), render) in plan.into_iter().zip(&renders) {
        let (x0, y0) = (at.0 as u32 * (cw + RING_GAP), at.1 as u32 * (ch + RING_GAP));
        image::imageops::replace(&mut image, render, i64::from(x0), i64::from(y0));
        cells.push(Cell {
            x: x0,
            y: y0,
            w: cw,
            h: ch,
            label,
            changes,
        });
    }
    Ok(Aid {
        width,
        height,
        cells,
        image,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn negative_photo(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("amb-darkroom-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("neg.dng");
        amb_testkit::dng::write(
            &path,
            &amb_testkit::patterns::negative(),
            &Default::default(),
        )
        .unwrap();
        path
    }

    fn on() -> BTreeMap<String, Value> {
        BTreeMap::from([(FLAG_NEGATIVE.to_string(), Value::Bool(true))])
    }

    fn mean(img: &RgbImage, c: &Cell, channel: usize) -> f64 {
        let mut sum = 0.0;
        for y in c.y..c.y + c.h {
            for x in c.x..c.x + c.w {
                sum += f64::from(img.get_pixel(x, y)[channel]);
            }
        }
        sum / f64::from(c.w * c.h)
    }

    #[test]
    fn exposure_follows_how_the_photo_prints() {
        let mut s = BTreeMap::new();
        assert_eq!(exposure_setting(&s), "Exposure2012");
        s.insert(FLAG_NEGATIVE.into(), Value::Bool(true));
        assert_eq!(exposure_setting(&s), "amb:NegativeExposure");
        s.insert(FLAG_PAPER.into(), Value::Bool(true));
        assert_eq!(exposure_setting(&s), "amb:NegativeExposure");
        s.insert(FLAG_SPLIT.into(), Value::Bool(true));
        assert_eq!(exposure_setting(&s), "amb:PaperSoftExposure");
    }

    #[test]
    fn labels_are_thirds_of_a_stop() {
        assert_eq!(delta_label(1.0 / 3.0, true), "+1/3");
        assert_eq!(delta_label(-2.0 / 3.0, true), "-2/3");
        assert_eq!(delta_label(4.0 / 3.0, true), "+1 1/3");
        assert_eq!(delta_label(0.0, true), "As now");
        assert_eq!(delta_label(15.0, false), "+15");
    }

    #[test]
    fn a_strip_has_a_band_per_step_and_stops_at_the_range() {
        let photo = negative_photo("strip");
        let o = StripOptions {
            size: 160,
            ..Default::default()
        };
        let aid = test_strip(&photo, &on(), &o, &|_, _| {}).unwrap();
        assert_eq!(aid.cells.len(), 7);
        assert_eq!(aid.cells[3].label, "As now");
        assert_eq!(aid.cells[0].label, "-1");
        assert_eq!(aid.cells[6].changes["amb:NegativeExposure"], 1.0);
        let covered: u32 = aid.cells.iter().map(|c| c.w).sum();
        assert_eq!(covered, aid.width);
        // At the top of the range the steps beyond it are dropped.
        let mut s = on();
        s.insert("amb:NegativeExposure".into(), Value::Number(3.9));
        let aid = test_strip(&photo, &s, &o, &|_, _| {}).unwrap();
        assert!(aid.cells.len() < 7);
        assert!(
            aid.cells
                .iter()
                .all(|c| c.changes.values().all(|v| *v <= 4.0))
        );
    }

    #[test]
    fn a_ring_moves_each_colour_the_way_it_says() {
        let photo = negative_photo("ring");
        let o = RingOptions {
            size: 120,
            ..Default::default()
        };
        let aid = ring_around(&photo, &on(), &o, &|_, _| {}).unwrap();
        assert_eq!(aid.cells.len(), 9);
        let by = |label: &str| aid.cells.iter().find(|c| c.label == label).unwrap();
        let mid = by("As now");
        let m = |label: &str, ch: usize| mean(&aid.image, by(label), ch);
        let now = |ch: usize| mean(&aid.image, mid, ch);
        assert!(m("More red", 0) > now(0), "{} {}", m("More red", 0), now(0));
        assert!(m("More green", 1) > now(1));
        assert!(m("More blue", 2) > now(2));
        assert!(m("More cyan", 0) < now(0));
        assert!(m("More magenta", 1) < now(1));
        assert!(m("More yellow", 2) < now(2));
        let luma = |label: &str| (0..3).map(|c| m(label, c)).sum::<f64>();
        let centre: f64 = (0..3).map(now).sum();
        assert!(luma("Lighter") > centre && luma("Darker") < centre);
        assert!(by("Lighter").changes["amb:NegativeExposure"] > 0.0);
    }

    #[test]
    fn lighter_is_lighter_on_paper_too() {
        let photo = negative_photo("paper");
        let mut s = on();
        s.insert(FLAG_PAPER.into(), Value::Bool(true));
        s.insert(FLAG_SPLIT.into(), Value::Bool(true));
        let o = RingOptions {
            size: 120,
            ..Default::default()
        };
        let aid = ring_around(&photo, &s, &o, &|_, _| {}).unwrap();
        let luma = |label: &str| {
            let c = aid.cells.iter().find(|c| c.label == label).unwrap();
            (0..3).map(|ch| mean(&aid.image, c, ch)).sum::<f64>()
        };
        assert!(
            luma("Lighter") > luma("As now"),
            "{} {}",
            luma("Lighter"),
            luma("As now")
        );
        assert!(luma("Darker") < luma("As now"));
        let lighter = aid.cells.iter().find(|c| c.label == "Lighter").unwrap();
        assert!(lighter.changes["amb:PaperSoftExposure"] < 0.0);
    }

    #[test]
    fn a_ring_needs_a_converted_negative() {
        let err = ring_around(
            Path::new("x.dng"),
            &BTreeMap::new(),
            &RingOptions::default(),
            &|_, _| {},
        )
        .unwrap_err();
        assert!(err.contains("switch on Film Negative"), "{err}");
    }
}
