// SPDX-License-Identifier: GPL-3.0-or-later
//! Glitch styles that move pixels sideways: RGB split, Slice shift, Wave
//! and Tiles.

use super::{Look, Src, luma, map_pixels};
use amb_plugin_sdk::{Effect, Pixels};
use std::f32::consts::TAU;

const SALT_BAND_HEIGHT: u32 = 0x61;
const SALT_BAND_PICK: u32 = 0x62;
const SALT_BAND_SHIFT: u32 = 0x63;
const SALT_TILE_X: u32 = 0x64;
const SALT_TILE_Y: u32 = 0x65;
/// A band's height, in sizes, runs from the first to the second.
const BAND_HEIGHT: (f32, f32) = (0.3, 1.5);
/// The furthest a band slides, in sizes.
const BAND_SHIFT: f32 = 4.0;
/// The red and blue split inside a slid band, in sizes.
const BAND_SPLIT: f32 = 0.15;
/// How much the photo's own brightness pushes a wave, as a share of it.
const WAVE_LUMA_PUSH: f32 = 0.5;
/// A tile slides at most this share of its size at full strength.
const TILE_SLIDE: f32 = 0.5;
/// Tiles and their lines are never smaller than this, in pixels.
const TILE_MIN_PX: f32 = 4.0;
/// Colour of the dark lines between tiles (grey, sRGB).
const GRID_LINE: f32 = 0.1;
/// Colour of the "+" in each tile's centre (sRGB).
const CROSS: [f32; 3] = [0.4, 1.0, 0.4];
/// A cross's arms are this share of a tile long, and at least this many
/// pixels.
const CROSS_ARM_SHARE: f32 = 1.0 / 40.0;
const CROSS_ARM_MIN_PX: f32 = 3.0;
/// A cross's line is this thick, in pixels, across.
const CROSS_THICK_PX: f32 = 1.5;
/// Thickness of the grid lines, in pixels: from this, with the tile size.
const GRID_LINE_PX: (f32, f32) = (1.0, 2.0);

/// RGB split: red read from one side, blue from the other.
pub(crate) fn rgb_split(e: &Effect, px: &mut Pixels, look: &Look) {
    let src = Src::new(px, e);
    let off = look.scale * look.s;
    map_pixels(px, e, |x, y| {
        let (xf, yf) = (x as f32, y as f32);
        [
            src.read_ch(xf + off, yf, 0),
            src.read_ch(xf, yf, 1),
            src.read_ch(xf - off, yf, 2),
        ]
    });
}

/// A horizontal band of the photo, in whole-photo rows.
struct Band {
    top: f32,
    /// Sideways slide in pixels, 0 if the band stays.
    slide: f32,
}

/// The bands down a photo of `height` rows: random heights, each slid with
/// probability `s`. The same for any part of the photo.
fn bands(e: &Effect, height: f32, look: &Look) -> Vec<Band> {
    let mut out = Vec::new();
    let (mut top, mut n) = (0.0f32, 0u32);
    while top < height {
        let share = BAND_HEIGHT.0 + (BAND_HEIGHT.1 - BAND_HEIGHT.0) * e.noise(n, SALT_BAND_HEIGHT);
        let picked = e.noise(n, SALT_BAND_PICK) < look.s;
        let slide = (e.noise(n, SALT_BAND_SHIFT) - 0.5) * 2.0 * look.scale * BAND_SHIFT;
        out.push(Band {
            top,
            slide: if picked { slide } else { 0.0 },
        });
        top += (look.scale * share).max(1.0);
        n += 1;
    }
    out
}

/// Slice shift: bands slid sideways, each with a little colour split, like
/// a video signal losing its sync.
pub(crate) fn slice_shift(e: &Effect, px: &mut Pixels, look: &Look) {
    let src = Src::new(px, e);
    let bands = bands(e, e.full_height.max(1) as f32, look);
    let split = look.scale * BAND_SPLIT;
    map_pixels(px, e, |x, y| {
        let (xf, yf) = (x as f32, y as f32);
        let band = &bands[bands.partition_point(|b| b.top <= yf).max(1) - 1];
        if band.slide == 0.0 {
            return src.read(xf, yf);
        }
        let at = xf + band.slide;
        [
            src.read_ch(at + split, yf, 0),
            src.read_ch(at, yf, 1),
            src.read_ch(at - split, yf, 2),
        ]
    });
}

/// Wave: rows pushed sideways in a ripple, and a little by the photo's own
/// brightness, as a displacement map would.
pub(crate) fn wave(e: &Effect, px: &mut Pixels, look: &Look) {
    let src = Src::new(px, e);
    let amount = look.scale * look.s;
    let phase = (e.seed % 1000) as f32 / 1000.0 * TAU;
    map_pixels(px, e, |x, y| {
        let (xf, yf) = (x as f32, y as f32);
        let ripple = (yf / look.scale.max(1.0) * TAU + phase).sin();
        let push = luma(src.read(xf, yf)) - 0.5;
        src.read(xf + (ripple + push * WAVE_LUMA_PUSH) * amount, yf)
    });
}

/// Tiles: the photo cut in squares, each read from a slightly different
/// place, with grid lines and a green "+" in each tile's centre if asked.
pub(crate) fn tiles(e: &Effect, px: &mut Pixels, look: &Look) {
    let src = Src::new(px, e);
    let size = look.scale.max(TILE_MIN_PX);
    let slide = size * look.s * TILE_SLIDE;
    let marks = e.flag("marks");
    let line = (size / 100.0).clamp(GRID_LINE_PX.0, GRID_LINE_PX.1);
    let arm = (size * CROSS_ARM_SHARE).max(CROSS_ARM_MIN_PX);
    map_pixels(px, e, |x, y| {
        let (xf, yf) = (x as f32, y as f32);
        let (tx, ty) = ((xf / size).floor(), (yf / size).floor());
        let (kx, ky) = (tx as u32, ty as u32);
        let dx = (e.noise(kx, ky ^ SALT_TILE_X) - 0.5) * 2.0 * slide;
        let dy = (e.noise(kx, ky ^ SALT_TILE_Y) - 0.5) * 2.0 * slide;
        if marks {
            let (lx, ly) = (xf - tx * size, yf - ty * size);
            if lx < line || ly < line {
                return [GRID_LINE; 3];
            }
            // Pixel centre, measured from the tile's centre.
            let (cx, cy) = (lx + 0.5 - size / 2.0, ly + 0.5 - size / 2.0);
            let half = CROSS_THICK_PX / 2.0;
            let bar = |along: f32, across: f32| along.abs() <= arm && across.abs() <= half;
            if bar(cx, cy) || bar(cy, cx) {
                return CROSS;
            }
        }
        src.read(xf + dx, yf + dy)
    });
}

#[cfg(test)]
mod tests {
    use crate::tests::run;
    use serde_json::json;

    #[test]
    fn rgb_split_moves_red_and_not_green() {
        let (w, h) = (48, 32);
        let p = json!({ "style": "RGB split", "strength": 100, "size": 10 });
        let (before, out) = run(p, w, h);
        let (mut red, mut green) = (0.0f32, 0.0f32);
        for i in 0..w * h {
            red += (out[i * 3] - before[i * 3]).abs();
            green += (out[i * 3 + 1] - before[i * 3 + 1]).abs();
        }
        assert!(red > 1.0, "red moved: {red}");
        assert!(green < 1e-3, "green stayed: {green}");
    }

    #[test]
    fn tiles_draw_a_cross_in_each_centre() {
        let (w, h) = (160, 160);
        // Size 25 % of 160 px: tiles of 40 px, centres at 20.
        let p = json!({ "style": "Tiles", "strength": 40, "size": 25, "marks": true });
        let (_, out) = run(p, w, h);
        let i = (20 * w + 20) * 3;
        assert_eq!(&out[i..i + 3], &[0.4, 1.0, 0.4]);
        // And a dark line on a tile's edge.
        let j = (50 * w + 40) * 3;
        assert_eq!(&out[j..j + 3], &[0.1, 0.1, 0.1]);
        let p = json!({ "style": "Tiles", "strength": 40, "size": 25, "marks": false });
        let (_, plain) = run(p, w, h);
        assert_ne!(&plain[i..i + 3], &[0.4, 1.0, 0.4]);
    }

    #[test]
    fn slice_shift_and_wave_change_the_photo() {
        for style in ["Slice shift", "Wave"] {
            let p = json!({ "style": style, "strength": 100, "size": 8 });
            let (a, b) = run(p, 64, 48);
            assert_ne!(a, b, "{style}");
        }
    }
}
