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
