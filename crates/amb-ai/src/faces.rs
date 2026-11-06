// SPDX-License-Identifier: GPL-3.0-or-later
//! Faces: the finder the engine asks for the faces in a picture
//! ([`amb_engine::subject::set_face_finder`]), running the "faces" task's
//! model (YuNet, MIT, 0.2 MB) with tract, once the user installs it.
//!
//! YuNet looks at a 640 × 640 picture (the photo shrunk to fit, the rest
//! black) in BGR, 0–255. For each cell of three grids (every 8, 16 and 32
//! pixels) it gives how sure it is of a face there (the square root of two
//! scores), the box as offsets from the cell, and five points: the right
//! and left eye, the nose and the corners of the mouth. Overlapping boxes
//! are thinned to the surest.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use amb_engine::subject::{Face, FaceFinder};
use tract_onnx::prelude::*;

type Runnable = Arc<crate::net::Net>;

/// The side of the picture YuNet looks at.
const SIZE: usize = 640;
/// Faces it is less sure of are left out.
const SURE: f32 = 0.6;

/// The finder for models in `dir`: `Err` until the "faces" task's model is
/// installed.
pub fn finder(dir: PathBuf) -> Arc<FaceFinder> {
    let loaded: Mutex<Option<(String, Runnable)>> = Mutex::new(None);
    Arc::new(
        move |rgb: &[f32], w: usize, h: usize| -> Result<Vec<Face>, String> {
            if rgb.len() != w * h * 3 || w == 0 || h == 0 {
                return Err("wrong input".into());
            }
            let spec = crate::chosen(&dir, "faces")?;
            if !crate::is_installed(&dir, &spec) {
                return Err(format!(
                    "finding faces needs {}: Settings → AI models, or `amb models install {}`",
                    spec.name, spec.id
                ));
            }
            let mut guard = loaded.lock().unwrap_or_else(|e| e.into_inner());
            if guard.as_ref().is_none_or(|(id, _)| *id != spec.id) {
                let path = crate::file_of(&dir, &spec.id);
                tract_linalg::multithread::set_default_executor(
                    tract_linalg::multithread::Executor::RayonGlobal,
                );
                let model = tract_onnx::onnx()
                    .model_for_path(&path)
                    .and_then(|m| m.with_input_fact(0, f32::fact([1, 3, SIZE, SIZE]).into()))
                    .and_then(crate::net::build)
                    .map_err(|e| format!("{} didn't load ({}): {e}", spec.name, path.display()))?;
                *guard = Some((spec.id.clone(), model));
            }
            let model = &guard.as_ref().expect("loaded above").1;
            let (input, scale) = letterbox(rgb, w, h);
            let out = model
                .run(tvec!(Tensor::from(input).into()))
                .map_err(|e| format!("{} failed: {e}", spec.name))?;
            let view = |i: usize| -> Result<Vec<f32>, String> {
                Ok(out[i]
                    .to_plain_array_view::<f32>()
                    .map_err(|e| e.to_string())?
                    .iter()
                    .copied()
                    .collect())
            };
            // Outputs: cls, obj, bbox, kps, each for strides 8, 16, 32.
            let mut found = Vec::new();
            for (k, stride) in [8usize, 16, 32].into_iter().enumerate() {
                let (cls, obj, bbox, kps) = (view(k)?, view(3 + k)?, view(6 + k)?, view(9 + k)?);
                found.extend(decode(&cls, &obj, &bbox, &kps, stride));
            }
            Ok(thin(found)
                .into_iter()
                .map(|f| {
                    // Back to fractions of the photo.
                    let (fx, fy) = (scale * w as f32, scale * h as f32);
                    Face {
                        x0: (f.x0 / fx).clamp(0.0, 1.0),
                        y0: (f.y0 / fy).clamp(0.0, 1.0),
                        x1: (f.x1 / fx).clamp(0.0, 1.0),
                        y1: (f.y1 / fy).clamp(0.0, 1.0),
                        eyes: f.eyes.map(|[x, y]| [x / fx, y / fy]),
                        score: f.score,
                    }
                })
                .collect())
        },
    )
}

/// The photo shrunk to fit `SIZE` (box-averaged), at the top left of a
/// black square, as BGR 0–255 planes; and the scale it was shrunk by.
fn letterbox(rgb: &[f32], w: usize, h: usize) -> (tract_ndarray::Array4<f32>, f32) {
    let scale = SIZE as f32 / w.max(h) as f32;
    let (sw, sh) = (
        ((w as f32 * scale).round() as usize).clamp(1, SIZE),
        ((h as f32 * scale).round() as usize).clamp(1, SIZE),
    );
    let mut input = tract_ndarray::Array4::<f32>::zeros((1, 3, SIZE, SIZE));
    for y in 0..sh {
        let (ya, yb) = (y * h / sh, ((y + 1) * h / sh).max(y * h / sh + 1).min(h));
        for x in 0..sw {
            let (xa, xb) = (x * w / sw, ((x + 1) * w / sw).max(x * w / sw + 1).min(w));
            let mut sum = [0.0f32; 3];
            for py in ya..yb {
                for px in xa..xb {
                    for (c, s) in sum.iter_mut().enumerate() {
                        *s += rgb[(py * w + px) * 3 + c];
                    }
                }
            }
            let n = ((yb - ya) * (xb - xa)) as f32;
            for c in 0..3 {
                // BGR: blue first.
                input[[0, 2 - c, y, x]] = (sum[c] / n).clamp(0.0, 1.0) * 255.0;
            }
        }
    }
    (input, scale)
}

/// The faces one grid's outputs point at, in pixels of the 640 square.
fn decode(cls: &[f32], obj: &[f32], bbox: &[f32], kps: &[f32], stride: usize) -> Vec<Face> {
    let cols = SIZE / stride;
    let s = stride as f32;
    let mut out = Vec::new();
    for i in 0..cls.len().min(obj.len()) {
        let score = (cls[i].clamp(0.0, 1.0) * obj[i].clamp(0.0, 1.0)).sqrt();
        if score < SURE {
            continue;
        }
        let (r, c) = ((i / cols) as f32, (i % cols) as f32);
        let b = &bbox[i * 4..i * 4 + 4];
        let (cx, cy) = ((c + b[0]) * s, (r + b[1]) * s);
        let (bw, bh) = (b[2].exp() * s, b[3].exp() * s);
        let k = &kps[i * 10..i * 10 + 10];
        out.push(Face {
            x0: cx - bw / 2.0,
            y0: cy - bh / 2.0,
            x1: cx + bw / 2.0,
            y1: cy + bh / 2.0,
            eyes: [
                [(k[0] + c) * s, (k[1] + r) * s],
                [(k[2] + c) * s, (k[3] + r) * s],
            ],
            score,
        });
    }
    out
}

/// The surest faces, leaving out boxes that overlap a surer one.
fn thin(mut faces: Vec<Face>) -> Vec<Face> {
    faces.sort_by(|a, b| b.score.total_cmp(&a.score));
    let mut keep: Vec<Face> = Vec::new();
    for f in faces {
        if keep.iter().all(|k| overlap(k, &f) < 0.3) {
            keep.push(f);
        }
    }
    keep
}

/// Intersection over union of two boxes.
fn overlap(a: &Face, b: &Face) -> f32 {
    let iw = (a.x1.min(b.x1) - a.x0.max(b.x0)).max(0.0);
    let ih = (a.y1.min(b.y1) - a.y0.max(b.y0)).max(0.0);
    let i = iw * ih;
    let area = |f: &Face| (f.x1 - f.x0) * (f.y1 - f.y0);
    i / (area(a) + area(b) - i).max(1e-6)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlapping_boxes_thin_to_the_surest() {
        let face = |x: f32, score: f32| Face {
            x0: x,
            y0: 0.0,
            x1: x + 10.0,
            y1: 10.0,
            eyes: [[0.0; 2]; 2],
            score,
        };
        let kept = thin(vec![face(0.0, 0.7), face(1.0, 0.9), face(30.0, 0.8)]);
        assert_eq!(kept.len(), 2);
        assert_eq!(kept[0].score, 0.9);
        assert_eq!(kept[1].x0, 30.0);
    }

    #[test]
    fn a_cell_decodes_to_its_box_and_eyes() {
        // One cell of the 32 grid (20 × 20), at row 2, column 3.
        let n = 400;
        let (mut cls, mut obj) = (vec![0.0; n], vec![0.0; n]);
        let (mut bbox, mut kps) = (vec![0.0; n * 4], vec![0.0; n * 10]);
        let i = 2 * 20 + 3;
        cls[i] = 0.9;
        obj[i] = 0.9;
        bbox[i * 4..i * 4 + 4].copy_from_slice(&[0.5, 0.5, 0.0, 0.0]);
        kps[i * 10..i * 10 + 4].copy_from_slice(&[0.25, 0.5, 0.75, 0.5]);
        let f = decode(&cls, &obj, &bbox, &kps, 32);
        assert_eq!(f.len(), 1);
        let f = f[0];
        assert!(
            (f.x0 - (3.5 * 32.0 - 16.0)).abs() < 1e-3 && (f.y1 - (2.5 * 32.0 + 16.0)).abs() < 1e-3
        );
        assert!(
            (f.eyes[0][0] - 3.25 * 32.0).abs() < 1e-3 && (f.eyes[1][0] - 3.75 * 32.0).abs() < 1e-3
        );
        assert!((f.score - 0.9).abs() < 1e-5);
    }

    /// Needs the model and a photo of a face: `AMB_MODELS=dir AMB_FACE=photo.ppm
    /// cargo test -p amb-ai --release -- --ignored face`.
    #[test]
    #[ignore]
    fn a_face_is_found_with_its_eyes() {
        let (Some(dir), Some(photo)) =
            (std::env::var_os("AMB_MODELS"), std::env::var_os("AMB_FACE"))
        else {
            return;
        };
        // A binary PPM (P6, 8-bit): its header is three numbers after "P6".
        let bytes = std::fs::read(photo).unwrap();
        let mut at = 2;
        let mut num = || {
            while bytes[at].is_ascii_whitespace() {
                at += 1;
            }
            let start = at;
            while bytes[at].is_ascii_digit() {
                at += 1;
            }
            std::str::from_utf8(&bytes[start..at])
                .unwrap()
                .parse::<usize>()
                .unwrap()
        };
        let (w, h, _) = (num(), num(), num());
        let rgb: Vec<f32> = bytes[at + 1..]
            .iter()
            .map(|v| f32::from(*v) / 255.0)
            .collect();
        let faces = finder(PathBuf::from(dir))(&rgb, w, h).unwrap();
        assert!(!faces.is_empty());
        let f = faces[0];
        for [x, y] in f.eyes {
            assert!(x > f.x0 && x < f.x1 && y > f.y0 && y < f.y1, "{f:?}");
        }
        println!("{faces:?}");
    }
}
