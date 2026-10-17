// SPDX-License-Identifier: GPL-3.0-or-later
//! AI masks that come from a model: Select Subject (`Mask/Image` subtype 1;
//! Select Background is the same part inverted) and Select People (subtype
//! 3, everyone at once for now).
//!
//! The engine doesn't run models itself: the editor and `amb` register a
//! [`Finder`] (`amb-ai`, which runs the model the user chose for each kind).
//! Without one these masks select nothing and are listed as not rendered.

use std::sync::{Arc, RwLock};

/// What a model is asked to find.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MapKind {
    /// The photo's main subject.
    Subject,
    /// Every person in the photo.
    People,
}

impl MapKind {
    /// The model task name (`amb models`).
    pub fn task(self) -> &'static str {
        match self {
            MapKind::Subject => "subject",
            MapKind::People => "people",
        }
    }
}

/// A map from a model: how much each cell is the thing asked for (0–1),
/// over the whole photo, `width`×`height` cells.
#[derive(Debug, Clone)]
pub struct Map {
    pub width: usize,
    pub height: usize,
    pub data: Vec<f32>,
}

/// Given the photo as tone-encoded sRGB (0–1, interleaved, `w`×`h`), the
/// map of what `kind` asks for. Each model shrinks the photo to its own
/// input size ([`shrink`]).
pub type Finder = dyn Fn(MapKind, &[f32], usize, usize) -> Result<Map, String> + Send + Sync;

static FINDER: RwLock<Option<Arc<Finder>>> = RwLock::new(None);

pub fn set_finder(f: Arc<Finder>) {
    *FINDER.write().unwrap_or_else(|e| e.into_inner()) = Some(f);
}

/// Can model masks be drawn here?
pub fn available() -> bool {
    FINDER.read().unwrap_or_else(|e| e.into_inner()).is_some()
}

pub(crate) fn find(kind: MapKind, rgb: &[f32], w: usize, h: usize) -> Result<Map, String> {
    let f = FINDER.read().unwrap_or_else(|e| e.into_inner()).clone();
    match f {
        Some(f) => f(kind, rgb, w, h),
        None => Err("no AI models here".into()),
    }
}

/// `rgb` (`w`×`h`, interleaved) box-filtered to `size`×`size` (stretched to
/// a square, as the models expect).
pub fn shrink(rgb: &[f32], w: usize, h: usize, size: usize) -> Vec<f32> {
    let mut out = vec![0.0f32; size * size * 3];
    for ty in 0..size {
        let (y0, y1) = (
            ty * h / size,
            ((ty + 1) * h / size).max(ty * h / size + 1).min(h),
        );
        for tx in 0..size {
            let (x0, x1) = (
                tx * w / size,
                ((tx + 1) * w / size).max(tx * w / size + 1).min(w),
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
            out[(ty * size + tx) * 3..][..3].copy_from_slice(&sum.map(|v| v / n));
        }
    }
    out
}
