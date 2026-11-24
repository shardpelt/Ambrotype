// SPDX-License-Identifier: GPL-3.0-or-later
//! The ring-around: the photo nine times, a step away in colour or lightness.

use super::*;

/// Ring-around: the colour step in slider units (of -100..100), the
/// lightness step in stops, and each cell's long side in pixels.
const RING_STEP: f64 = 15.0;
const RING_EXPOSURE_STEP: f64 = 1.0 / 3.0;
const RING_SIZE: u32 = 400;
/// The gap between ring-around cells, in pixels.
const RING_GAP: u32 = 4;
/// The gap shows this grey, 8-bit.
const RING_GAP_GREY: u8 = 24;

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
/// The ring's middle cell.
const RING_MIDDLE: (usize, usize) = (1, 1);

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
