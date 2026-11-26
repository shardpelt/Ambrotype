// SPDX-License-Identifier: GPL-3.0-or-later
//! The step wedge: a transmission wedge (the photo's film base under steps
//! of density) printed through the photo's own negative conversion and
//! paper, so the sheet shows which steps merge into paper white or black and
//! how the grade spreads the rest. Read, not clicked.

use super::*;

/// A Stouffer-style 21-step wedge: the first step's density over the film
/// base, then this much more per step (Stouffer T2115: 0.05, 0.15 a step).
const WEDGE_STEPS: usize = 21;
const WEDGE_FIRST: f64 = 0.05;
const WEDGE_STEP: f64 = 0.15;
/// The most steps a wedge has.
const WEDGE_MAX_STEPS: usize = 41;
/// The sheet's long side, in pixels.
const WEDGE_SIZE: u32 = 1260;
/// A patch is this many times as tall as it is wide.
const WEDGE_TALL: u32 = 3;
/// The photo is measured at this long side, in pixels: the editor's preview.
const WEDGE_MEASURE_EDGE: u32 = 1024;
/// A patch prints as paper white at or above this in every channel, black at
/// or below the other (8-bit sRGB).
const WEDGE_WHITE: u8 = 254;
const WEDGE_BLACK: u8 = 1;
/// The part of a patch read for white or black: the middle, away from the
/// edges a sharpening halo touches (a share of its width and height).
const WEDGE_READ: f64 = 0.5;

/// The settings that make the print from the negative, kept for the wedge:
/// the conversion, the paper, the film base and stock, and the white
/// balance. Crop, geometry, masks, grain and the rest belong to the photo.
const WEDGE_KEEP_PREFIX: [&str; 4] = ["amb:Negative", "amb:Paper", "amb:FilmBase", "amb:FilmStock"];
const WEDGE_KEEP: [&str; 3] = ["WhiteBalance", "Temperature", "Tint"];

#[derive(Debug, Clone)]
pub struct WedgeOptions {
    /// How many steps (21).
    pub steps: usize,
    /// The density added per step (0.15).
    pub step: f64,
    /// The sheet's long side in pixels.
    pub size: u32,
}

impl Default for WedgeOptions {
    fn default() -> Self {
        WedgeOptions {
            steps: WEDGE_STEPS,
            step: WEDGE_STEP,
            size: WEDGE_SIZE,
        }
    }
}

/// The settings that print the wedge: the photo's print settings, with the
/// film base, levels and midtone it measures stored and Auto off, so the
/// wedge is printed as the photo is, not measured itself.
fn wedge_settings(
    settings: &BTreeMap<String, Value>,
    analysis: &amb_engine::negative::Analysis,
) -> BTreeMap<String, Value> {
    let mut s: BTreeMap<String, Value> = settings
        .iter()
        .filter(|(k, _)| {
            WEDGE_KEEP.contains(&k.as_str()) || WEDGE_KEEP_PREFIX.iter().any(|p| k.starts_with(p))
        })
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    s.extend(crate::negative::frozen(std::slice::from_ref(analysis)));
    s
}

/// Whether every channel of the middle of `c` in `img` is at or past `limit`
/// (`above`: at or above it; else at or below).
fn patch_is(img: &RgbImage, c: &Cell, limit: u8, above: bool) -> bool {
    let (mx, my) = (
        (f64::from(c.w) * (1.0 - WEDGE_READ) / 2.0) as u32,
        (f64::from(c.h) * (1.0 - WEDGE_READ) / 2.0) as u32,
    );
    let mut sum = [0.0f64; 3];
    let mut n = 0.0f64;
    for y in c.y + my..c.y + c.h - my {
        for x in c.x + mx..c.x + c.w - mx {
            let p = img.get_pixel(x, y);
            for (s, v) in sum.iter_mut().zip(p.0) {
                *s += f64::from(v);
            }
            n += 1.0;
        }
    }
    sum.iter().all(|s| {
        let mean = (s / n.max(1.0)).round();
        if above {
            mean >= f64::from(limit)
        } else {
            mean <= f64::from(limit)
        }
    })
}

/// A step wedge printed through the photo's conversion and paper: one patch
/// per step, left to right, each `step` denser than the last (from 0.05 over
/// the film base). Only for a converted negative.
pub fn step_wedge(
    photo: &Path,
    settings: &BTreeMap<String, Value>,
    o: &WedgeOptions,
    progress: &dyn Fn(usize, usize),
) -> Result<Aid, String> {
    if !flag(settings, FLAG_NEGATIVE) {
        return Err(
            "The step wedge works on a converted negative: switch on Film Negative first.".into(),
        );
    }
    if !(2..=WEDGE_MAX_STEPS).contains(&o.steps) {
        return Err(format!("a wedge has 2 to {WEDGE_MAX_STEPS} steps"));
    }
    if !o.step.is_finite() || o.step <= 0.0 {
        return Err("a wedge's step must be more than 0".into());
    }
    progress(0, 2);
    let developer = Developer::load(photo, settings, WEDGE_MEASURE_EDGE)?;
    let analysis = amb_engine::pipeline::negative_analysis(
        &developer.img,
        &matching::develop_plain(settings),
        false,
    );
    progress(1, 2);
    let size = o.size.clamp(SIZE_RANGE.0, SIZE_RANGE.1);
    let pw = (size / o.steps as u32).max(1);
    let (width, height) = (pw * o.steps as u32, pw * WEDGE_TALL);
    let densities: Vec<f64> = (0..o.steps)
        .map(|i| WEDGE_FIRST + o.step * i as f64)
        .collect();
    let mut rgb = Vec::with_capacity((width * height * 3) as usize);
    for _ in 0..height {
        for x in 0..width {
            let d = densities[(x / pw) as usize];
            rgb.extend(analysis.base.map(|b| (b * 10f64.powf(-d)) as f32));
        }
    }
    let mut img = developer
        .img
        .with_pixels(rgb, width as usize, height as usize);
    img.metadata.orientation = 1;
    let wedge = Developer {
        img,
        orientation: 1,
    };
    let image = wedge.render(&wedge_settings(settings, &analysis), width);
    progress(2, 2);
    let cells = densities
        .iter()
        .enumerate()
        .map(|(i, d)| {
            let mut c = Cell {
                x: i as u32 * pw,
                y: 0,
                w: pw,
                h: height,
                label: format!("{} · {d:.2}", i + 1),
                changes: BTreeMap::new(),
            };
            if patch_is(&image, &c, WEDGE_WHITE, true) {
                c.label.push_str(" white");
            } else if patch_is(&image, &c, WEDGE_BLACK, false) {
                c.label.push_str(" black");
            }
            c
        })
        .collect();
    Ok(Aid {
        width: image.width(),
        height: image.height(),
        cells,
        image,
    })
}
