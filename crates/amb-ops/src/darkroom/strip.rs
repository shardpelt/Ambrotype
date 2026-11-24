// SPDX-License-Identifier: GPL-3.0-or-later
//! The test strip: one setting stepped across the photo in bands.

use super::*;

/// Test strip: how many bands, how far apart (stops, as exposure is
/// stepped by default), and the sheet's long side in pixels.
const STRIP_STEPS: usize = 7;
const STRIP_STEP: f64 = 1.0 / 3.0;
const STRIP_SIZE: u32 = 1200;
/// The most bands in a strip.
const STRIP_MAX_STEPS: usize = 15;

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
