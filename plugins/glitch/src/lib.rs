// SPDX-License-Identifier: GPL-3.0-or-later
//! Glitch: a photo broken the way machines break it. Pixel sorting, split
//! colour channels, slid bands, torn compression blocks, a bad video tape,
//! ripples, smeared streaks and nudged tiles.
//!
//! A pixel effect that needs the whole photo (`"whole": true`). Every
//! random choice comes from [`Effect::noise`], keyed by full-photo
//! coordinates or by a row, block or tile index in full-photo units, so a
//! preview and an export break alike. Strength 0 leaves the photo as it is.

mod blocks;
mod shift;
mod sort;

use amb_plugin_sdk::{Effect, Pixels};

// The ranges repeat those in plugin.json.
const STRENGTH: (f64, f64) = (0.0, 100.0);
const SIZE: (f64, f64) = (0.5, 25.0);
const GRAIN: (f64, f64) = (0.0, 100.0);
/// Most a tint pulls the colours toward its hue, at full strength.
const TINT_MIX: f32 = 0.8;
/// Spread of the grain at 100 %, as a share of the full range.
const GRAIN_SPREAD: f32 = 0.5;
/// Salt for the grain's noise, so it differs from a style's own noise.
const SALT_GRAIN: u32 = 0x6A41;
/// Tints: the hue each pulls toward (sRGB 0..1).
const TINTS: [(&str, [f32; 3]); 4] = [
    ("Violet", [0.6, 0.3, 1.0]),
    ("Cyan", [0.2, 0.9, 1.0]),
    ("Green", [0.3, 1.0, 0.4]),
    ("Red", [1.0, 0.25, 0.25]),
];

/// What every style needs: how much, and how big in pixels.
pub(crate) struct Look {
    /// Strength as a share, 0..1.
    pub s: f32,
    /// The glitch's size in pixels: a share of the photo's short side.
    pub scale: f32,
}

impl Look {
    fn new(e: &Effect) -> Look {
        let (fw, fh) = (e.full_width.max(1) as f32, e.full_height.max(1) as f32);
        let size = e.number("size").clamp(SIZE.0, SIZE.1) as f32;
        Look {
            s: (e.number("strength").clamp(STRENGTH.0, STRENGTH.1) / 100.0) as f32,
            scale: fw.min(fh) * size / 100.0,
        }
    }
}

/// A copy of the photo to read from while the buffer is overwritten.
pub(crate) struct Src {
    data: Vec<f32>,
    w: usize,
    h: usize,
    /// The buffer's top-left in the whole photo.
    ox: f32,
    oy: f32,
}

/// `v` reflected into 0..len, as a mirror at each edge would.
fn reflect(v: f32, len: f32) -> f32 {
    let m = v.rem_euclid(2.0 * len);
    if m >= len { 2.0 * len - m } else { m }
}

impl Src {
    pub(crate) fn new(px: &Pixels, e: &Effect) -> Src {
        Src {
            data: px.data.to_vec(),
            w: px.width,
            h: px.height,
            ox: e.x as f32,
            oy: e.y as f32,
        }
    }

    /// One channel at a place in the whole photo (pixels), smoothly;
    /// outside the photo it reflects back in.
    pub(crate) fn read_ch(&self, x: f32, y: f32, k: usize) -> f32 {
        let x = reflect(x - self.ox, self.w as f32 - 1.0);
        let y = reflect(y - self.oy, self.h as f32 - 1.0);
        let (x0, y0) = (x as usize, y as usize);
        let (x1, y1) = ((x0 + 1).min(self.w - 1), (y0 + 1).min(self.h - 1));
        let (tx, ty) = (x - x0 as f32, y - y0 as f32);
        let at = |x: usize, y: usize| self.data[(y * self.w + x) * 3 + k];
        let top = at(x0, y0) + (at(x1, y0) - at(x0, y0)) * tx;
        let bottom = at(x0, y1) + (at(x1, y1) - at(x0, y1)) * tx;
        top + (bottom - top) * ty
    }

    pub(crate) fn read(&self, x: f32, y: f32) -> [f32; 3] {
        [0, 1, 2].map(|k| self.read_ch(x, y, k))
    }
}

/// Brightness of an sRGB colour, 0..1.
pub(crate) fn luma(c: [f32; 3]) -> f32 {
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}

/// Set every pixel of the buffer from `f(x, y)`, the pixel's place in the
/// whole photo.
pub(crate) fn map_pixels(px: &mut Pixels, e: &Effect, f: impl Fn(u32, u32) -> [f32; 3]) {
    for y in 0..px.height {
        for x in 0..px.width {
            let c = f(e.x + x as u32, e.y + y as u32);
            px.set(x, y, c.map(|v| v.clamp(0.0, 1.0)));
        }
    }
}

/// The tint and the grain, after the style.
fn finish(e: &Effect, px: &mut Pixels, s: f32) {
    let tint = TINTS.iter().find(|t| t.0 == e.choice("tint")).map(|t| t.1);
    let grain = (e.number("grain").clamp(GRAIN.0, GRAIN.1) / 100.0) as f32;
    if tint.is_none() && grain <= 0.0 {
        return;
    }
    for y in 0..px.height {
        for x in 0..px.width {
            let mut c = px.get(x, y);
            if let Some(t) = tint {
                // Keep the brightness, move the colour toward the tint.
                let l = luma(c);
                let target = t.map(|v| v * l / luma(t));
                for k in 0..3 {
                    c[k] += (target[k] - c[k]) * TINT_MIX * s;
                }
            }
            let n = (e.noise(e.x + x as u32, (e.y + y as u32) ^ SALT_GRAIN) - 0.5)
                * GRAIN_SPREAD
                * grain;
            px.set(x, y, c.map(|v| (v + n).clamp(0.0, 1.0)));
        }
    }
}

pub fn glitch(e: &Effect, px: &mut Pixels) -> Result<(), String> {
    if px.width < 2 || px.height < 2 {
        return Ok(());
    }
    let style: fn(&Effect, &mut Pixels, &Look) = match e.choice("style") {
        "Pixel sort" => sort::pixel_sort,
        "RGB split" => shift::rgb_split,
        "Slice shift" => shift::slice_shift,
        "Blocks" => blocks::blocks,
        "VHS" => blocks::vhs,
        "Wave" => shift::wave,
        "Streaks" => sort::streaks,
        "Tiles" => shift::tiles,
        other => return Err(format!("Glitch has no style called \"{other}\"")),
    };
    let look = Look::new(e);
    if look.s > 0.0 {
        style(e, px, &look);
    }
    finish(e, px, look.s);
    Ok(())
}

amb_plugin_sdk::export_effect!(pixels: glitch);

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    pub(crate) const STYLES: [&str; 8] = [
        "Pixel sort",
        "RGB split",
        "Slice shift",
        "Blocks",
        "VHS",
        "Wave",
        "Streaks",
        "Tiles",
    ];

    /// A photo with something in every channel, in every direction.
    pub(crate) fn photo(w: usize, h: usize) -> Vec<f32> {
        (0..w * h)
            .flat_map(|i| {
                let (x, y) = ((i % w) as f32 / w as f32, (i / w) as f32 / h as f32);
                [x, y, ((i * 37) % 101) as f32 / 101.0]
            })
            .collect()
    }

    pub(crate) fn run(params: serde_json::Value, w: usize, h: usize) -> (Vec<f32>, Vec<f32>) {
        let before = photo(w, h);
        let mut data = before.clone();
        let e = Effect::for_test(params, w as u32, h as u32);
        let mut px = Pixels {
            data: &mut data,
            width: w,
            height: h,
        };
        glitch(&e, &mut px).unwrap();
        (before, data)
    }

    #[test]
    fn strength_zero_changes_nothing() {
        for style in STYLES {
            let p = json!({ "style": style, "strength": 0, "size": 5, "marks": true });
            let (a, b) = run(p, 40, 30);
            assert_eq!(a, b, "{style}");
        }
    }

    #[test]
    fn full_strength_stays_finite_and_in_range() {
        for style in STYLES {
            let p = json!({ "style": style, "strength": 100, "size": 25, "angle": 33, "low": 0,
                "high": 100, "tint": "Violet", "grain": 100, "marks": true });
            let (_, out) = run(p, 50, 37);
            assert!(
                out.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v)),
                "{style}"
            );
        }
    }

    #[test]
    fn the_same_seed_gives_the_same_picture() {
        for style in STYLES {
            let p = json!({ "style": style, "strength": 70, "size": 6, "angle": 20, "low": 10,
                "high": 90 });
            assert_eq!(run(p.clone(), 48, 36).1, run(p, 48, 36).1, "{style}");
        }
    }

    #[test]
    fn an_unknown_style_is_an_error() {
        let mut data = photo(8, 8);
        let e = Effect::for_test(json!({ "style": "Nope" }), 8, 8);
        let mut px = Pixels {
            data: &mut data,
            width: 8,
            height: 8,
        };
        assert!(glitch(&e, &mut px).is_err());
    }

    #[test]
    fn a_tint_changes_colour_and_grain_stays_in_range() {
        let p = json!({ "style": "RGB split", "strength": 100, "size": 0.5, "tint": "Cyan" });
        let (a, b) = run(p, 40, 30);
        assert_ne!(a, b);
        let (_, g) = run(
            json!({ "style": "RGB split", "strength": 0, "grain": 60 }),
            40,
            30,
        );
        assert!(g.iter().all(|v| (0.0..=1.0).contains(v)));
    }
}
