// SPDX-License-Identifier: GPL-3.0-or-later
//! Glitch styles that work along a line in a chosen direction: Pixel sort
//! and Streaks.

use super::{Look, Src, luma, map_pixels};
use amb_plugin_sdk::{Effect, Pixels};

// The ranges repeat those in plugin.json.
const ANGLE: (f64, f64) = (0.0, 180.0);
const LEVEL: (f64, f64) = (0.0, 100.0);
/// Salts for the noise, so the choices below don't follow one another.
const SALT_LINE: u32 = 0x51;
const SALT_LENGTH: u32 = 0x52;
const SALT_JITTER_R: u32 = 0x53;
const SALT_JITTER_B: u32 = 0x54;
/// A streak's length, as a share of the longest, runs from this to 1.
const STREAK_SHORTEST: f32 = 0.3;
/// How far a streak's red and blue lines wander sideways, in pixels.
const STREAK_JITTER_PX: f32 = 2.0;
/// Most samples taken along a streak; a longer one takes them further apart.
const STREAK_TAPS: usize = 48;
/// Keeps the line number positive for the noise.
const LINE_BIAS: i32 = 1 << 20;

/// Pixel sort: along parallel lines, each stretch of pixels whose
/// brightness is between `low` and `high` is sorted dark to bright.
pub(crate) fn pixel_sort(e: &Effect, px: &mut Pixels, look: &Look) {
    let angle = e.number("angle").clamp(ANGLE.0, ANGLE.1) as f32;
    let low = (e.number("low").clamp(LEVEL.0, LEVEL.1) / 100.0) as f32;
    let high = (e.number("high").clamp(LEVEL.0, LEVEL.1) / 100.0) as f32;
    let (w, h) = (px.width as i32, px.height as i32);
    let (ds, dc) = angle.to_radians().sin_cos();
    let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
    let reach = (w as f32).hypot(h as f32).ceil() as i32 / 2 + 1;
    for k in -reach..=reach {
        // The share of lines sorted: the lines picked by noise.
        if e.noise((k + reach) as u32, SALT_LINE) >= look.s {
            continue;
        }
        let mut line: Vec<usize> = Vec::new();
        for t in -reach..=reach {
            let x = (cx - ds * k as f32 + dc * t as f32).round() as i32;
            let y = (cy + dc * k as f32 + ds * t as f32).round() as i32;
            if (0..w).contains(&x) && (0..h).contains(&y) {
                let i = (y * w + x) as usize;
                if line.last() != Some(&i) {
                    line.push(i);
                }
            }
        }
        sort_line(px.data, &line, low, high);
    }
}

/// Sort each run of `line`'s pixels whose brightness is in `low..=high`.
fn sort_line(data: &mut [f32], line: &[usize], low: f32, high: f32) {
    let colour = |data: &[f32], i: usize| [data[i * 3], data[i * 3 + 1], data[i * 3 + 2]];
    let mut run: Vec<usize> = Vec::new();
    for (n, &i) in line.iter().enumerate() {
        let inside = (low..=high).contains(&luma(colour(data, i)));
        if inside {
            run.push(i);
        }
        if (!inside || n + 1 == line.len()) && !run.is_empty() {
            let mut cols: Vec<[f32; 3]> = run.iter().map(|&j| colour(data, j)).collect();
            cols.sort_by(|a, b| luma(*a).total_cmp(&luma(*b)));
            for (&j, c) in run.iter().zip(cols) {
                data[j * 3..j * 3 + 3].copy_from_slice(&c);
            }
            run.clear();
        }
    }
}

/// Streaks: each pixel smeared along the direction, a different length on
/// every line, with red and blue wandering a little: fine lines, like
/// brushed metal.
pub(crate) fn streaks(e: &Effect, px: &mut Pixels, look: &Look) {
    let angle = e.number("angle").clamp(ANGLE.0, ANGLE.1) as f32;
    let (ds, dc) = angle.to_radians().sin_cos();
    let src = Src::new(px, e);
    let longest = look.scale * look.s;
    map_pixels(px, e, |x, y| {
        let (xf, yf) = (x as f32, y as f32);
        // Pixels on one line across the direction share one number.
        let key = ((-ds * xf + dc * yf).round() as i32 + LINE_BIAS) as u32;
        let share = STREAK_SHORTEST + (1.0 - STREAK_SHORTEST) * e.noise(key, SALT_LENGTH);
        let length = longest * share;
        let wander = |salt: u32| (e.noise(key, salt) - 0.5) * 2.0 * STREAK_JITTER_PX;
        let sideways = [wander(SALT_JITTER_R), 0.0, wander(SALT_JITTER_B)];
        let taps = (length.ceil() as usize).clamp(1, STREAK_TAPS);
        [0, 1, 2].map(|k| {
            let side = sideways[k];
            let sum: f32 = (0..taps)
                .map(|i| {
                    let t = length * i as f32 / taps as f32;
                    src.read_ch(xf - dc * t - ds * side, yf - ds * t + dc * side, k)
                })
                .sum();
            sum / taps as f32
        })
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::run;
    use serde_json::json;

    #[test]
    fn sorted_runs_never_get_darker_along_a_row() {
        let (w, h) = (64, 20);
        let (low, high) = (25.0f32, 85.0f32);
        let p = json!({ "style": "Pixel sort", "strength": 100, "angle": 0, "low": low,
            "high": high });
        let (before, out) = run(p, w, h);
        let l = |x: usize, y: usize| {
            let i = (y * w + x) * 3;
            luma([out[i], out[i + 1], out[i + 2]])
        };
        let inside = |v: f32| (low / 100.0..=high / 100.0).contains(&v);
        let mut sorted_something = false;
        for y in 0..h {
            for x in 1..w {
                if inside(l(x - 1, y)) && inside(l(x, y)) {
                    assert!(l(x, y) >= l(x - 1, y), "row {y} at {x}");
                    sorted_something = true;
                }
            }
        }
        assert!(sorted_something);
        assert_ne!(before, out);
    }

    #[test]
    fn streaks_change_the_photo() {
        let p = json!({ "style": "Streaks", "strength": 100, "size": 10, "angle": 0 });
        let (a, b) = run(p, 60, 40);
        assert_ne!(a, b);
    }
}
