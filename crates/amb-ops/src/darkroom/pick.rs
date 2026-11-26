// SPDX-License-Identifier: GPL-3.0-or-later
//! Pick Better: two prints of the photo side by side, one setting a step
//! down (A) and a step up (B). The editor runs the rounds (each pick halves
//! the step, then the next setting); this makes one pair.

use super::*;

/// The default step of an exposure setting, in stops.
const PICK_EXPOSURE_STEP: f64 = 2.0 / 3.0;
/// The default step of any other setting: this share of its registry range.
const PICK_RANGE_SHARE: f64 = 0.1;
/// Each print's long side in pixels.
const PICK_SIZE: u32 = 600;
/// The gap between the two prints, in pixels, and its grey (8-bit).
const PICK_GAP: u32 = 4;
const PICK_GAP_GREY: u8 = 24;

/// The settings a round of Pick Better goes through, in order, after the
/// exposure: for a converted negative, its own contrast and colour; for
/// any other photo, the Basic panel's tone and colour.
const PICK_NEGATIVE: [&str; 4] = [
    "amb:NegativeContrast",
    "amb:NegativeCyanRed",
    "amb:NegativeMagentaGreen",
    "amb:NegativeYellowBlue",
];
const PICK_PLAIN: [&str; 5] = [
    EXPOSURE_PLAIN,
    "Contrast2012",
    "Highlights2012",
    "Shadows2012",
    "Vibrance",
];

#[derive(Debug, Clone)]
pub struct PickOptions {
    /// The setting to move; `None` is [`exposure_setting`].
    pub setting: Option<String>,
    /// The value to centre on; `None` is the setting's value now.
    pub value: Option<f64>,
    /// How far A and B are from the centre; `None` is [`pick_step`].
    pub step: Option<f64>,
    /// Each print's long side in pixels.
    pub size: u32,
}

impl Default for PickOptions {
    fn default() -> Self {
        PickOptions {
            setting: None,
            value: None,
            step: None,
            size: PICK_SIZE,
        }
    }
}

/// The settings a round of Pick Better goes through, in order.
pub fn pick_settings(settings: &BTreeMap<String, Value>) -> Vec<&'static str> {
    if flag(settings, FLAG_NEGATIVE) {
        std::iter::once(exposure_setting(settings))
            .chain(PICK_NEGATIVE)
            .collect()
    } else {
        PICK_PLAIN.to_vec()
    }
}

fn is_exposure(name: &str) -> bool {
    name == EXPOSURE_PAPER || name == EXPOSURE_NEGATIVE || name == EXPOSURE_PLAIN
}

/// A round's first step for `name`: 2/3 stop for exposure, else a tenth of
/// the setting's range.
pub fn pick_step(name: &str) -> f64 {
    if is_exposure(name) {
        return PICK_EXPOSURE_STEP;
    }
    Registry::builtin()
        .lookup(name)
        .and_then(|i| Some((i.max? - i.min?) * PICK_RANGE_SHARE))
        .filter(|s| *s > 0.0)
        .unwrap_or(1.0)
}

/// Two prints side by side: A with the setting `step` below `value`, B
/// `step` above, each held to the setting's range. Each cell's `changes`
/// set the setting to its value.
pub fn pick_pair(
    photo: &Path,
    settings: &BTreeMap<String, Value>,
    o: &PickOptions,
    progress: &dyn Fn(usize, usize),
) -> Result<Aid, String> {
    let name = o
        .setting
        .clone()
        .unwrap_or_else(|| exposure_setting(settings).to_string());
    let step = o.step.unwrap_or_else(|| pick_step(&name));
    if !step.is_finite() || step <= 0.0 {
        return Err("Pick Better's step must be more than 0".into());
    }
    let value = clamp(&name, o.value.unwrap_or_else(|| current(settings, &name)))?;
    let pair = [
        ("A", clamp(&name, value - step)?),
        ("B", clamp(&name, value + step)?),
    ];
    let size = o.size.clamp(SIZE_RANGE.0, SIZE_RANGE.1);
    let developer = Developer::load(photo, settings, size)?;
    let renders: Vec<RgbImage> = pair
        .iter()
        .enumerate()
        .map(|(i, (_, v))| {
            progress(i, pair.len());
            developer.render(&with(settings, &name, *v), size)
        })
        .collect();
    progress(pair.len(), pair.len());
    let (cw, ch) = (renders[0].width(), renders[0].height());
    let (width, height) = (2 * cw + PICK_GAP, ch);
    let mut image = RgbImage::from_pixel(width, height, image::Rgb([PICK_GAP_GREY; 3]));
    let mut cells = Vec::new();
    for (i, ((label, v), render)) in pair.iter().zip(&renders).enumerate() {
        let x0 = i as u32 * (cw + PICK_GAP);
        image::imageops::replace(&mut image, render, i64::from(x0), 0);
        cells.push(Cell {
            x: x0,
            y: 0,
            w: cw,
            h: ch,
            label: format!("{label} {}", delta_label(v - value, is_exposure(&name))),
            changes: BTreeMap::from([(name.clone(), *v)]),
        });
    }
    Ok(Aid {
        width,
        height,
        cells,
        image,
    })
}
