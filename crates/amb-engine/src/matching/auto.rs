// SPDX-License-Identifier: GPL-3.0-or-later
//! Auto: a target look for the photo itself, and the settings that reach it.

use super::solve::solve;
use super::stats::{LookStats, look_stats};
use super::{MatchOptions, MatchResult, Part, Settings, Weights};
use crate::pipeline::{self, DevelopSettings};
use crate::raw::CameraImage;
use amb_presets::Value;

/// Where Auto puts the darkest tones, the middle and the brightest (L* at
/// the 1st, 50th and 99th percentiles), and how far it moves each there:
/// not all the way, so a dark, low-key photo stays darker than a bright one
/// and a hazy one isn't forced to full contrast.
const AUTO_ANCHORS: [f32; 3] = [2.0, 46.0, 95.0];

const AUTO_TONE_PULL: [f32; 3] = [0.5, 0.7, 0.5];

/// Share of the near-grey cast Auto white balance takes out: near-grey
/// pixels may be a beige wall or dry grass, not grey, so not all of it.
const AUTO_WHITE_PULL: f32 = 0.6;

/// Least share of near-grey midtones for Auto to change the white balance
/// at all. In a scene with a little near-grey (water, dry grass, a metal
/// ball) its colour says more about the object than the light, and the
/// camera's own white balance is the better guess.
const AUTO_WHITE_SHARE: f32 = 0.15;

/// What Auto aims for, from the photo's own look. `tone` stretches the
/// photo's own tones so its black point, middle and white point move
/// towards [`AUTO_ANCHORS`] (the shape in between is kept, so sliders can
/// reach it). `white` takes the cast out of the
/// near-grey midtones, shifting every zone's colour by the same amount; a
/// photo with too little near-grey keeps its white balance.
pub fn auto_target(current: &LookStats, tone: bool, white: bool) -> LookStats {
    let mut t = current.clone();
    if tone {
        let l = &current.lightness;
        let (lo, mid, hi) = (l[0], l[4], l[8]);
        let pull = |v: f32, i: usize| v + AUTO_TONE_PULL[i] * (AUTO_ANCHORS[i] - v);
        // The black and white points only move outwards: Auto never
        // flattens a photo that already reaches black or white.
        let lo2 = pull(lo, 0).min(lo);
        let hi2 = pull(hi, 2).max(hi);
        let mid2 = pull(mid, 1);
        for v in t.lightness.iter_mut() {
            *v = if *v <= mid {
                lo2 + (*v - lo) * (mid2 - lo2) / (mid - lo).max(1e-3)
            } else {
                mid2 + (*v - mid) * (hi2 - mid2) / (hi - mid).max(1e-3)
            };
        }
    }
    if white && current.neutral[2] >= AUTO_WHITE_SHARE {
        let [da, db, _] = current.neutral.map(|v| v * AUTO_WHITE_PULL);
        for z in t.zones.iter_mut() {
            z[0] -= da;
            z[1] -= db;
        }
        t.neutral[0] -= da;
        t.neutral[1] -= db;
    }
    t
}

/// Lightroom-style Auto: the tone sliders with `tone`, the white balance
/// with `white`, found by the matching solver towards [`auto_target`].
/// Only the tones and the near-grey cast count: brightening a photo makes
/// its colours stronger in Lab, and that is not something to undo.
/// Deterministic.
pub fn auto(
    img: &CameraImage,
    base: &Settings,
    tone: bool,
    white: bool,
    develop: &dyn Fn(&Settings) -> DevelopSettings,
) -> MatchResult {
    // As Lightroom's Auto, the tone sliders start from zero, so the result
    // doesn't depend on where they were.
    const TONE: [&str; 6] = [
        "Exposure2012",
        "Contrast2012",
        "Highlights2012",
        "Shadows2012",
        "Whites2012",
        "Blacks2012",
    ];
    let original = base;
    let mut cleared = base.clone();
    if tone {
        cleared.retain(|k, _| !TONE.contains(&k.as_str()));
    }
    let base = &cleared;
    let current = look_stats(&pipeline::render(img, &develop(base)));
    let target = auto_target(&current, tone, white);
    let white = white && current.neutral[2] >= AUTO_WHITE_SHARE;
    let mut parts = Vec::new();
    if white {
        parts.push(Part::WhiteBalance);
    }
    if tone {
        parts.push(Part::Tone);
    }
    let opts = MatchOptions {
        parts,
        max_evaluations: 250,
        weights: Weights {
            tone: 1.0,
            cast: 0.0,
            colour: 0.0,
            neutral: if white { 1.0 } else { 0.0 },
            detail: 0.0,
            bands: false,
        },
    };
    let mut r = solve(img, base, &target, &opts, develop);
    r.evaluations += 1;
    // Sliders Auto left at zero that the photo had set.
    for k in TONE {
        let had = original
            .get(k)
            .and_then(Value::as_f64)
            .is_some_and(|v| v != 0.0);
        if tone && had && !r.changes.iter().any(|(c, _)| c == k) {
            r.changes.push((k.to_string(), Value::Number(0.0)));
        }
    }
    r.changes.retain(|(k, v)| original.get(k) != Some(v));
    // Lightroom marks an automatic white balance as Auto, not Custom: set
    // last, once Temperature and Tint are in (as commands, in this order).
    if white && r.changes.iter().any(|(k, _)| k == "WhiteBalance") {
        r.changes
            .push(("WhiteBalance".into(), Value::Text("Auto".into())));
    }
    r
}
