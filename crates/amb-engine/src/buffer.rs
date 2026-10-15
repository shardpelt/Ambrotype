// SPDX-License-Identifier: GPL-3.0-or-later
//! Working image buffer shared by all pipeline stages.

use amb_color::matrix::PROPHOTO_TO_XYZ_D50;
use rayon::prelude::*;

/// Interleaved RGB image in linear ProPhoto (D50). Depending on the stage the
/// values are scene-linear (unbounded above) or display-linear (nominally 0..1).
#[derive(Debug, Clone)]
pub struct Image {
    pub width: usize,
    pub height: usize,
    /// Row-major interleaved RGB, `width * height * 3` values.
    pub rgb: Vec<f32>,
}

/// Where a working buffer sits in the full output. A normal render works on
/// the whole output; a 1:1 region render on part of it, and every stage that
/// places something (vignette, grain, masks, spots) or scales with the photo
/// (filter radii) goes by the full output, so the region matches the same
/// part of the full render.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tile {
    /// The buffer's top-left pixel in the full output.
    pub x: usize,
    pub y: usize,
    /// The full output's size.
    pub full_width: usize,
    pub full_height: usize,
}

impl Tile {
    /// The whole output of this size.
    pub fn whole(width: usize, height: usize) -> Tile {
        Tile {
            x: 0,
            y: 0,
            full_width: width,
            full_height: height,
        }
    }

    /// Whether a `width` × `height` buffer at this tile is the whole output.
    pub fn is_whole(&self, width: usize, height: usize) -> bool {
        *self == Tile::whole(width, height)
    }

    /// The whole of `img`.
    pub fn of(img: &Image) -> Tile {
        Tile::whole(img.width, img.height)
    }

    /// The full output's size relative to the 24 MP reference (by diagonal),
    /// which spatial filters scale with.
    pub fn scale(&self) -> f32 {
        crate::local::reference_scale(self.full_width, self.full_height)
    }
}

/// A rectangle of pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
}

impl Rect {
    pub fn right(&self) -> usize {
        self.x + self.width
    }

    pub fn bottom(&self) -> usize {
        self.y + self.height
    }

    /// Whether `other` lies wholly inside this rectangle.
    pub fn contains(&self, other: &Rect) -> bool {
        other.x >= self.x
            && other.y >= self.y
            && other.right() <= self.right()
            && other.bottom() <= self.bottom()
    }
}

/// ProPhoto luminance weights (the Y row of ProPhoto → XYZ).
pub fn luminance_weights() -> [f32; 3] {
    PROPHOTO_TO_XYZ_D50[1].map(|v| v as f32)
}

impl Image {
    pub fn new(width: usize, height: usize) -> Image {
        Image {
            width,
            height,
            rgb: vec![0.0; width * height * 3],
        }
    }

    pub fn from_fn(
        width: usize,
        height: usize,
        f: impl Fn(usize, usize) -> [f32; 3] + Sync,
    ) -> Image {
        let mut img = Image::new(width, height);
        img.rgb
            .par_chunks_mut(width * 3)
            .enumerate()
            .for_each(|(y, row)| {
                for x in 0..width {
                    row[x * 3..x * 3 + 3].copy_from_slice(&f(x, y));
                }
            });
        img
    }

    pub fn pixel_count(&self) -> usize {
        self.width * self.height
    }

    pub fn get(&self, x: usize, y: usize) -> [f32; 3] {
        let i = (y * self.width + x) * 3;
        [self.rgb[i], self.rgb[i + 1], self.rgb[i + 2]]
    }

    pub fn set(&mut self, x: usize, y: usize, v: [f32; 3]) {
        let i = (y * self.width + x) * 3;
        self.rgb[i..i + 3].copy_from_slice(&v);
    }

    /// Luminance plane (ProPhoto Y), one value per pixel.
    pub fn luminance(&self) -> Vec<f32> {
        let w = luminance_weights();
        self.rgb
            .par_chunks(3)
            .map(|p| w[0] * p[0] + w[1] * p[1] + w[2] * p[2])
            .collect()
    }

    /// Apply `f` to every pixel in parallel.
    pub fn map_pixels(&mut self, f: impl Fn([f32; 3]) -> [f32; 3] + Sync) {
        self.rgb.par_chunks_mut(3).for_each(|p| {
            let out = f([p[0], p[1], p[2]]);
            p.copy_from_slice(&out);
        });
    }

    /// Apply `f(pixel, index)` to every pixel in parallel (index = y * width + x).
    pub fn map_pixels_indexed(&mut self, f: impl Fn([f32; 3], usize) -> [f32; 3] + Sync) {
        self.rgb.par_chunks_mut(3).enumerate().for_each(|(i, p)| {
            let out = f([p[0], p[1], p[2]], i);
            p.copy_from_slice(&out);
        });
    }
}
