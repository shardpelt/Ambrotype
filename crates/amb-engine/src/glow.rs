// SPDX-License-Identifier: GPL-3.0-or-later
//! Film glow on any photo: halation and bloom (`amb:Halation`, `amb:Bloom`,
//! `amb:GlowSize`), in scene-linear light after Dehaze. Our own model, from
//! how film behaves as its makers describe it:
//!
//! - **Halation:** light that passes through the emulsion reflects off the
//!   film base and comes back into the bottom, red-sensitive layer, so bright
//!   highlights get a red-orange halo. Film with an anti-halation layer shows
//!   little; motion-picture film shot without its rem-jet layer (sold for
//!   stills) shows a lot. Only light well above the midtones is strong
//!   enough to make one.
//! - **Bloom:** light scattered in the emulsion and the lens: highlights
//!   spread a soft glow of their own colour over their surroundings.
//!
//! Both act on light, so they come before the tone curve: a halo is as
//! bright as the light that made it, and a clipped lamp makes a wide one.
//! Sizes are relative to the photo's diagonal, so a preview, a 1:1 region
//! and the full render agree.

use rayon::prelude::*;

use crate::buffer::{Image, Tile};
use crate::filters::gaussian_blur;

/// `amb:Halation` and `amb:Bloom` 0..100 (0), `amb:GlowSize` 0..100 (50).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Glow {
    pub halation: f64,
    pub bloom: f64,
    pub size: f64,
}

impl Default for Glow {
    fn default() -> Self {
        Glow {
            halation: 0.0,
            bloom: 0.0,
            size: 50.0,
        }
    }
}

impl Glow {
    pub fn is_neutral(&self) -> bool {
        !(self.halation > 0.0 || self.bloom > 0.0)
    }
}

/// Scene-linear level (18 % grey = 0.18) above which light makes a halo:
/// about 2.5 stops over the midtones.
const HALATION_THRESHOLD: f32 = 1.0;
/// The halo's colour: mostly the red layer, some green, almost no blue.
const HALATION_TINT: [f32; 3] = [1.0, 0.32, 0.06];
/// Halation's strength at 100.
const HALATION_GAIN: f32 = 0.9;
/// Bloom starts lower: the scattered light of bright parts.
const BLOOM_THRESHOLD: f32 = 0.5;
/// Bloom's strength at 100.
const BLOOM_GAIN: f32 = 0.6;
/// Blur sigmas at GlowSize 50 on the reference diagonal (24 MP), pixels:
/// halation is a tight ring, bloom a wide haze.
const HALATION_SIGMA: f32 = 18.0;
const BLOOM_SIGMA: f32 = 60.0;

/// GlowSize 0..100 as a factor on the sigmas: 0.25× to 4×, 1× at 50.
fn size_factor(size: f64) -> f32 {
    4f32.powf((size.clamp(0.0, 100.0) as f32 - 50.0) / 50.0)
}

/// The blurs' sigmas in this image's pixels, for `scale` (the full output's
/// [`Tile::scale`]).
fn sigmas(g: &Glow, scale: f32) -> (f32, f32) {
    let f = size_factor(g.size) * scale;
    ((HALATION_SIGMA * f).max(0.5), (BLOOM_SIGMA * f).max(0.5))
}

/// How far the stage reads around a pixel (for region renders).
pub fn reach(g: &Glow, scale: f32) -> usize {
    if g.is_neutral() {
        return 0;
    }
    let (h, b) = sigmas(g, scale);
    let mut r = 0.0f32;
    if g.halation > 0.0 {
        r = r.max(h);
    }
    if g.bloom > 0.0 {
        r = r.max(b);
    }
    (3.0 * r).ceil() as usize + 2
}

/// The light above `threshold`, per pixel (one plane, from luminance) or per
/// channel.
fn above(img: &Image, threshold: f32, per_channel: bool) -> Vec<Vec<f32>> {
    let w = crate::buffer::luminance_weights();
    if per_channel {
        (0..3)
            .map(|c| {
                img.rgb
                    .par_chunks_exact(3)
                    .map(|p| (p[c] - threshold).max(0.0))
                    .collect()
            })
            .collect()
    } else {
        vec![
            img.rgb
                .par_chunks_exact(3)
                .map(|p| (w[0] * p[0] + w[1] * p[1] + w[2] * p[2] - threshold).max(0.0))
                .collect(),
        ]
    }
}

/// Add halation and bloom to `img` (scene-linear), for an image that is
/// `tile` of the full output.
pub fn apply_in(img: &mut Image, g: &Glow, tile: &Tile) {
    if g.is_neutral() || img.width == 0 || img.height == 0 {
        return;
    }
    let (w, h) = (img.width, img.height);
    let (hs, bs) = sigmas(g, tile.scale());
    let halo = (g.halation > 0.0).then(|| {
        let src = above(img, HALATION_THRESHOLD, false).remove(0);
        gaussian_blur(&src, w, h, hs)
    });
    let bloom = (g.bloom > 0.0).then(|| {
        above(img, BLOOM_THRESHOLD, true)
            .into_iter()
            .map(|p| gaussian_blur(&p, w, h, bs))
            .collect::<Vec<_>>()
    });
    let kh = HALATION_GAIN * (g.halation.clamp(0.0, 100.0) / 100.0) as f32;
    let kb = BLOOM_GAIN * (g.bloom.clamp(0.0, 100.0) / 100.0) as f32;
    img.rgb
        .par_chunks_exact_mut(3)
        .enumerate()
        .for_each(|(i, p)| {
            if let Some(halo) = &halo {
                for c in 0..3 {
                    p[c] += kh * HALATION_TINT[c] * halo[i];
                }
            }
            if let Some(bloom) = &bloom {
                for c in 0..3 {
                    p[c] += kb * bloom[c][i];
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A dark frame with one bright lamp in the middle.
    fn lamp(w: usize, h: usize) -> Image {
        let mut img = Image {
            width: w,
            height: h,
            rgb: vec![0.05; w * h * 3],
        };
        for y in h / 2 - 2..h / 2 + 2 {
            for x in w / 2 - 2..w / 2 + 2 {
                img.rgb[(y * w + x) * 3..][..3].copy_from_slice(&[8.0, 8.0, 8.0]);
            }
        }
        img
    }

    fn at(img: &Image, x: usize, y: usize) -> [f32; 3] {
        let i = (y * img.width + x) * 3;
        [img.rgb[i], img.rgb[i + 1], img.rgb[i + 2]]
    }

    #[test]
    fn halation_puts_a_red_halo_around_a_lamp_and_leaves_the_dark_alone() {
        let (w, h) = (200, 200);
        let mut img = lamp(w, h);
        let g = Glow {
            halation: 100.0,
            ..Glow::default()
        };
        apply_in(&mut img, &g, &Tile::whole(7211, 1));
        let near = at(&img, w / 2 + 6, h / 2);
        assert!(near[0] > 0.06, "a halo next to the lamp: {near:?}");
        assert!(
            near[0] > near[1] && near[1] > near[2],
            "red-orange: {near:?}"
        );
        let far = at(&img, 5, 5);
        assert!(
            (far[0] - 0.05).abs() < 1e-4,
            "the far corner untouched: {far:?}"
        );
    }

    #[test]
    fn bloom_keeps_the_light_s_colour_and_nothing_changes_at_zero() {
        let (w, h) = (200, 200);
        let mut img = lamp(w, h);
        let before = img.clone();
        apply_in(&mut img, &Glow::default(), &Tile::whole(w, h));
        assert_eq!(img.rgb, before.rgb);
        let g = Glow {
            bloom: 100.0,
            ..Glow::default()
        };
        apply_in(&mut img, &g, &Tile::whole(7211, 1));
        let near = at(&img, w / 2 + 10, h / 2);
        assert!(
            near[0] > 0.05 && (near[0] - near[2]).abs() < 1e-5,
            "{near:?}"
        );
    }

    #[test]
    fn the_glow_scales_with_the_photo() {
        let g = Glow {
            halation: 50.0,
            ..Glow::default()
        };
        let small = reach(&g, Tile::whole(1500, 1000).scale());
        let big = reach(&g, Tile::whole(6000, 4000).scale());
        assert!(big > 3 * small, "{small} {big}");
        assert_eq!(reach(&Glow::default(), 1.0), 0);
    }
}
