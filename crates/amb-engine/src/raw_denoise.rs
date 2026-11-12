// SPDX-License-Identifier: GPL-3.0-or-later
//! AI Denoise on the raw. The engine doesn't run AI models itself (they live
//! in `amb-ai`, which builds on the engine): the app registers a
//! [`RawDenoiser`], and [`load`] decodes a raw with it.
//!
//! - **Bayer sensors:** the denoiser gets the sensor's samples (black and
//!   white level scaled to 0–1, not white balanced) in place of our
//!   demosaic, and gives camera RGB back, denoised and demosaiced.
//! - **Others** (X-Trans, …): our demosaic first, then the denoiser gets
//!   linear Rec.2020 at the as-shot white balance and gives the same back.
//!
//! Everything after (highlight rebuilding, lens corrections, the edit) is
//! as for any raw. A denoised photo is written as a new linear DNG
//! (`amb_ops::denoise`), as Lightroom's Denoise does.

use std::cell::Cell;
use std::path::Path;
use std::sync::{Arc, RwLock};

use crate::Error;
use crate::raw::CameraImage;
use amb_color::matrix::{Mat3, invert3, mul3};

/// What a denoiser gets.
pub enum DenoiseInput<'a> {
    /// A Bayer mosaic, `size` (width, height), `pattern[r][c]` the colour
    /// (0 R, 1 G, 2 B) at row `r % 2`, column `c % 2`. Out: interleaved
    /// camera RGB of the same size.
    Bayer {
        samples: &'a [f32],
        size: (usize, usize),
        pattern: [[u8; 2]; 2],
    },
    /// Interleaved linear Rec.2020, white balanced. Out: the same.
    Linear {
        rgb: &'a [f32],
        size: (usize, usize),
    },
}

/// Denoises; `progress(done, total)` as it goes.
pub type RawDenoiser =
    dyn Fn(DenoiseInput<'_>, &dyn Fn(usize, usize)) -> Result<Vec<f32>, String> + Send + Sync;

static DENOISER: RwLock<Option<Arc<RawDenoiser>>> = RwLock::new(None);

/// Register the app's denoiser.
pub fn set_raw_denoiser(d: Arc<RawDenoiser>) {
    *DENOISER.write().unwrap_or_else(|e| e.into_inner()) = Some(d);
}

/// The denoiser as the raw decoder uses it.
pub(crate) struct Denoise<'a> {
    run: &'a RawDenoiser,
    /// 0–1: how much of the denoised photo, mixed with the photo as it
    /// would be without it.
    amount: f32,
    progress: &'a dyn Fn(usize, usize),
    /// The Bayer path ran.
    used: Cell<bool>,
}

impl Denoise<'_> {
    pub(crate) fn bayer(
        &self,
        samples: &[f32],
        size: (usize, usize),
        pattern: [[u8; 2]; 2],
        wb: [f32; 3],
    ) -> Result<Vec<f32>, Error> {
        self.used.set(true);
        let mut rgb = (self.run)(
            DenoiseInput::Bayer {
                samples,
                size,
                pattern,
            },
            self.progress,
        )
        .map_err(Error::Decode)?;
        if rgb.len() != size.0 * size.1 * 3 {
            return Err(Error::Decode("AI Denoise gave the wrong size".into()));
        }
        if self.amount < 1.0 {
            let plain = crate::rcd::demosaic(samples, size.0, size.1, &pattern, wb);
            mix(&mut rgb, &plain, self.amount);
        }
        Ok(rgb)
    }
}

/// `out` = `plain` + (`out` − `plain`) × `amount`.
fn mix(out: &mut [f32], plain: &[f32], amount: f32) {
    for (o, p) in out.iter_mut().zip(plain) {
        *o = p + (*o - p) * amount;
    }
}

/// Decode `path` with the registered denoiser, `amount` (0–1) of it. A
/// rendered image (JPEG, TIFF…) has no raw noise to take out, so it is
/// refused.
pub fn load(
    path: &Path,
    amount: f32,
    progress: &dyn Fn(usize, usize),
) -> Result<CameraImage, Error> {
    if crate::raster::is_raster(path) {
        return Err(Error::Unsupported(
            "AI Denoise works on raws, not on JPEG, TIFF or other rendered images".into(),
        ));
    }
    let run = DENOISER
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .ok_or_else(|| Error::Unsupported("AI Denoise isn't available here".into()))?;
    let d = Denoise {
        run: run.as_ref(),
        amount: amount.clamp(0.0, 1.0),
        progress,
        used: Cell::new(false),
    };
    let mut img = crate::raw::load_file(path, Some(&d))?;
    if let Some(o) = crate::orientation::from_sidecar(path) {
        img.metadata.orientation = o;
    }
    if !d.used.get() {
        // Not a Bayer sensor: demosaiced first, then denoised in Rec.2020.
        let to = mul3(&PROPHOTO_TO_REC2020, &img.to_prophoto(img.as_shot_neutral));
        let back = invert3(&to);
        let mut rgb = img.rgb.clone();
        apply(&to, &mut rgb);
        let mut out = (run)(
            DenoiseInput::Linear {
                rgb: &rgb,
                size: (img.width, img.height),
            },
            progress,
        )
        .map_err(Error::Decode)?;
        if out.len() != rgb.len() {
            return Err(Error::Decode("AI Denoise gave the wrong size".into()));
        }
        mix(&mut out, &rgb, amount.clamp(0.0, 1.0));
        apply(&back, &mut out);
        out.iter_mut().for_each(|v| *v = v.max(0.0));
        img.rgb = out;
    }
    Ok(img)
}

/// Linear ProPhoto (D50) → linear Rec.2020 (D65), through XYZ with a
/// Bradford adaptation.
const PROPHOTO_TO_REC2020: Mat3 = [
    [1.2007, -0.0575, -0.1431],
    [-0.0699, 1.0806, -0.0107],
    [0.0055, -0.0408, 1.0350],
];

fn apply(m: &Mat3, rgb: &mut [f32]) {
    let m: [[f32; 3]; 3] = m.map(|r| r.map(|v| v as f32));
    for p in rgb.chunks_exact_mut(3) {
        let (r, g, b) = (p[0], p[1], p[2]);
        for (k, row) in m.iter().enumerate() {
            p[k] = row[0] * r + row[1] * g + row[2] * b;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_inverse_undoes_the_matrix() {
        let back = invert3(&PROPHOTO_TO_REC2020);
        let id = mul3(&back, &PROPHOTO_TO_REC2020);
        for (r, row) in id.iter().enumerate() {
            for (c, v) in row.iter().enumerate() {
                assert!((v - if r == c { 1.0 } else { 0.0 }).abs() < 1e-9, "{id:?}");
            }
        }
        // White stays white (both are white balanced).
        let w: Vec<f64> = PROPHOTO_TO_REC2020.iter().map(|r| r.iter().sum()).collect();
        assert!(w.iter().all(|v| (v - 1.0).abs() < 0.01), "{w:?}");
    }

    #[test]
    fn rendered_images_are_refused() {
        let Err(e) = load(Path::new("photo.jpg"), 1.0, &|_, _| {}) else {
            panic!("a JPEG was denoised");
        };
        assert!(e.to_string().contains("raws"), "{e}");
    }
}
