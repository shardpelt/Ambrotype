// SPDX-License-Identifier: GPL-3.0-or-later
//! Select Subject (`Mask/Image` subtype 1): where the photo's main subject
//! is, from an AI model the app provides (`amb-ai`: U²-Net, run locally).
//!
//! The engine doesn't run models itself: the editor and `amb` register a
//! [`Finder`]. Without one a subject mask selects nothing and is listed as
//! not rendered.

use std::sync::{Arc, RwLock};

/// The model's input and output size, a side.
pub const SIZE: usize = 320;

/// Given the photo as `SIZE`×`SIZE` sRGB (0–1, interleaved, stretched to a
/// square), how much each of `SIZE`×`SIZE` pixels is the subject (0–1).
pub type Finder = dyn Fn(&[f32]) -> Result<Vec<f32>, String> + Send + Sync;

static FINDER: RwLock<Option<Arc<Finder>>> = RwLock::new(None);

pub fn set_finder(f: Arc<Finder>) {
    *FINDER.write().unwrap_or_else(|e| e.into_inner()) = Some(f);
}

/// Can subject masks be drawn here?
pub fn available() -> bool {
    FINDER.read().unwrap_or_else(|e| e.into_inner()).is_some()
}

pub(crate) fn find(rgb: &[f32]) -> Result<Vec<f32>, String> {
    let f = FINDER.read().unwrap_or_else(|e| e.into_inner()).clone();
    match f {
        Some(f) => f(rgb),
        None => Err("no subject model here".into()),
    }
}

/// `rgb` (`w`×`h`, interleaved) box-filtered to `SIZE`×`SIZE`.
pub(crate) fn shrink(rgb: &[f32], w: usize, h: usize) -> Vec<f32> {
    let mut out = vec![0.0f32; SIZE * SIZE * 3];
    for ty in 0..SIZE {
        let (y0, y1) = (
            ty * h / SIZE,
            ((ty + 1) * h / SIZE).max(ty * h / SIZE + 1).min(h),
        );
        for tx in 0..SIZE {
            let (x0, x1) = (
                tx * w / SIZE,
                ((tx + 1) * w / SIZE).max(tx * w / SIZE + 1).min(w),
            );
            let mut sum = [0.0f32; 3];
            for y in y0..y1 {
                for x in x0..x1 {
                    let p = &rgb[(y * w + x) * 3..][..3];
                    sum[0] += p[0];
                    sum[1] += p[1];
                    sum[2] += p[2];
                }
            }
            let n = ((y1 - y0) * (x1 - x0)).max(1) as f32;
            out[(ty * SIZE + tx) * 3..][..3].copy_from_slice(&sum.map(|v| v / n));
        }
    }
    out
}
