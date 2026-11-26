// SPDX-License-Identifier: GPL-3.0-or-later
//! Glitch styles that break the picture the way a medium does: Blocks (a
//! torn compressed video or JPEG) and VHS (a worn tape).

use super::{Look, Src, map_pixels};
use amb_plugin_sdk::{Effect, Pixels};

const SALT_BLOCK_KIND: u32 = 0x71;
const SALT_BLOCK_DX: u32 = 0x72;
const SALT_BLOCK_DY: u32 = 0x73;
const SALT_JITTER: u32 = 0x74;
const SALT_LINE_ROW: u32 = 0x75;
/// Blocks are never smaller than this, in pixels (a codec's own size).
const BLOCK_MIN_PX: f32 = 4.0;
/// A copied block comes from at most this many blocks away.
const NEAR_BLOCKS: i32 = 3;
/// Levels per channel inside a torn block.
const LEVELS: f32 = 6.0;
/// Rows a tape's jitter is smoothed over, each side of a row.
const JITTER_SPREAD: i32 = 2;
/// Colour bleed of the tape, in sizes at full strength; red is pushed
/// twice as far as it is blurred.
const BLEED: f32 = 0.1;
/// How much darker every other row is at full strength.
const SCAN_DEPTH: f32 = 0.35;
/// Share of rows that are a bright noise line at full strength.
const NOISE_LINE_RATE: f32 = 0.01;
/// How bright a noise line's speckle gets.
const NOISE_LINE_LIFT: f32 = 0.7;

/// What a torn block does.
#[derive(Clone, Copy)]
enum Tear {
    /// Take the picture from a block nearby (a datamosh).
    Copy,
    /// One flat colour, the block's average.
    Flat,
    /// The block's first column dragged across it (a JPEG streak).
    Smear,
}

fn quantise(c: [f32; 3]) -> [f32; 3] {
    c.map(|v| (v * (LEVELS - 1.0)).round() / (LEVELS - 1.0))
}

/// Blocks: a grid of square blocks, some of them torn.
pub(crate) fn blocks(e: &Effect, px: &mut Pixels, look: &Look) {
    let src = Src::new(px, e);
    let b = look.scale.round().max(BLOCK_MIN_PX) as usize;
    let (ox, oy) = (e.x as usize, e.y as usize);
    let (w, h) = (px.width, px.height);
    for by in oy / b..=(oy + h - 1) / b {
        for bx in ox / b..=(ox + w - 1) / b {
            let (kx, ky) = (bx as u32, by as u32);
            if e.noise(kx, ky) >= look.s {
                continue;
            }
            let tear = match (e.noise(kx, ky ^ SALT_BLOCK_KIND) * 3.0) as u32 {
                0 => Tear::Copy,
                1 => Tear::Flat,
                _ => Tear::Smear,
            };
            // The part of the block that is in the buffer.
            let xs = (bx * b).max(ox)..((bx + 1) * b).min(ox + w);
            let ys = (by * b).max(oy)..((by + 1) * b).min(oy + h);
            let span = |salt: u32| {
                (e.noise(kx, ky ^ salt) * (2 * NEAR_BLOCKS + 1) as f32) as i32 - NEAR_BLOCKS
            };
            let (dx, dy) = (
                span(SALT_BLOCK_DX) * b as i32,
                span(SALT_BLOCK_DY) * b as i32,
            );
            let average = match tear {
                Tear::Flat => {
                    let n = (xs.len() * ys.len()) as f32;
                    let mut sum = [0.0f32; 3];
                    for y in ys.clone() {
                        for x in xs.clone() {
                            let c = src.read(x as f32, y as f32);
                            (0..3).for_each(|k| sum[k] += c[k] / n);
                        }
                    }
                    sum
                }
                _ => [0.0; 3],
            };
            for y in ys {
                for x in xs.clone() {
                    let (xf, yf) = (x as f32, y as f32);
                    let c = match tear {
                        Tear::Copy => src.read(xf + dx as f32, yf + dy as f32),
                        Tear::Flat => average,
                        Tear::Smear => src.read((bx * b) as f32, yf),
                    };
                    px.set(x - ox, y - oy, quantise(c));
                }
            }
        }
    }
}

/// VHS: rows wobbling sideways, colour bleeding to one side, darker
/// scanlines and a few bright noise lines.
pub(crate) fn vhs(e: &Effect, px: &mut Pixels, look: &Look) {
    let src = Src::new(px, e);
    let wobble = look.scale * look.s;
    let bleed = look.scale * BLEED * look.s;
    let jitter = |y: u32| -> f32 {
        let sum: f32 = (-JITTER_SPREAD..=JITTER_SPREAD)
            .map(|d| e.noise((y as i32 + d).max(0) as u32, SALT_JITTER) - 0.5)
            .sum();
        sum * 2.0 / (2 * JITTER_SPREAD + 1) as f32 * wobble
    };
    // Five taps spread over a bleed's width: a horizontal blur.
    let blur = |x: f32, y: f32, k: usize| -> f32 {
        (-2..=2)
            .map(|i| src.read_ch(x + i as f32 * bleed / 2.0, y, k))
            .sum::<f32>()
            / 5.0
    };
    map_pixels(px, e, |x, y| {
        let (xf, yf) = (x as f32 + jitter(y), y as f32);
        let mut c = [
            blur(xf - 2.0 * bleed, yf, 0),
            src.read_ch(xf, yf, 1),
            blur(xf + bleed, yf, 2),
        ];
        if y % 2 == 1 {
            c = c.map(|v| v * (1.0 - SCAN_DEPTH * look.s));
        }
        if e.noise(y, SALT_LINE_ROW) < NOISE_LINE_RATE * look.s {
            let lift = e.noise(x, y) * NOISE_LINE_LIFT;
            c = c.map(|v| v + lift);
        }
        c
    });
}

#[cfg(test)]
mod tests {
    use crate::tests::run;
    use serde_json::json;

    #[test]
    fn blocks_tear_some_blocks_and_leave_others() {
        let (w, h) = (64, 48);
        let p = json!({ "style": "Blocks", "strength": 50, "size": 12.5 });
        let (a, b) = run(p, w, h);
        let same = (0..w * h)
            .filter(|&i| a[i * 3..i * 3 + 3] == b[i * 3..i * 3 + 3])
            .count();
        assert!(same > 0 && same < w * h, "{same} of {} unchanged", w * h);
    }

    #[test]
    fn vhs_darkens_every_other_row() {
        let (w, h) = (32, 32);
        let p = json!({ "style": "VHS", "strength": 100, "size": 0.5 });
        let (_, out) = run(p, w, h);
        let row = |y: usize| out[y * w * 3..(y + 1) * w * 3].iter().sum::<f32>();
        let (even, odd): (f32, f32) = (
            (4..28).step_by(2).map(row).sum(),
            (5..28).step_by(2).map(row).sum(),
        );
        assert!(odd < even, "odd {odd} even {even}");
    }
}
