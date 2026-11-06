// SPDX-License-Identifier: GPL-3.0-or-later
//! Select Object: the selector the engine asks for the object under a
//! click or in a box ([`amb_engine::subject::set_selector`]), running the
//! "objects" task's model (SAM 2.1 tiny, Apache-2.0) with tract, once the
//! user installs it (130 MB).
//!
//! SAM comes in two halves. The encoder looks at the whole picture once
//! (about 9 s on a laptop CPU), stretched to 1024 × 1024; its features are
//! kept for the last pictures. The decoder then answers each click or box
//! in a fraction of a second: four masks of 256 × 256 logits and how good
//! it thinks each is. For one click it picks the largest of the three it
//! offers that isn't most of the picture (the click may mean a part or
//! the whole, and removing wants the whole), or for a `Part` click the
//! one it scores best; for a box or several clicks, the first.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use amb_engine::subject::{Map, Prompt, Selector};
use tract_onnx::prelude::*;

use crate::ModelSpec;

type Runnable = Arc<crate::net::Net>;

/// The encoder's three outputs for one picture.
type Features = Arc<[TValue; 3]>;

/// Pictures whose features are kept.
const KEPT: usize = 3;

struct Loaded {
    id: String,
    encoder: Runnable,
    /// Decoders by number of points (tract fixes the shape).
    decoders: HashMap<usize, Runnable>,
}

/// The selector for models in `dir`: `Err` until the "objects" task's model
/// is installed.
pub fn selector(dir: PathBuf) -> Arc<Selector> {
    let loaded: Mutex<Option<Loaded>> = Mutex::new(None);
    let kept: Mutex<Vec<(u64, Features)>> = Mutex::new(Vec::new());
    Arc::new(
        move |rgb: &[f32], w: usize, h: usize, prompts: &[Prompt]| -> Result<Map, String> {
            if rgb.len() != w * h * 3 || w == 0 || h == 0 || prompts.is_empty() {
                return Err("wrong input".into());
            }
            let spec = crate::chosen(&dir, "objects")?;
            if !crate::is_installed(&dir, &spec) {
                return Err(format!(
                    "selecting an object by a click needs {}: Settings → AI models, or `amb models install {}`",
                    spec.name, spec.id
                ));
            }
            let mut guard = loaded.lock().unwrap_or_else(|e| e.into_inner());
            if guard.as_ref().is_none_or(|l| l.id != spec.id) {
                let encoder = load(&crate::file_of(&dir, &spec.id), None, &spec)?;
                *guard = Some(Loaded {
                    id: spec.id.clone(),
                    encoder,
                    decoders: HashMap::new(),
                });
            }
            let model = guard.as_mut().expect("loaded above");
            let size = spec.input.size;
            let small = amb_engine::subject::shrink(rgb, w, h, size);
            let key = hash(&small);
            let found = kept
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, f)| f.clone());
            let features = match found {
                Some(f) => f,
                None => {
                    let f = encode(&model.encoder, &spec, &small)?;
                    let mut k = kept.lock().unwrap_or_else(|e| e.into_inner());
                    k.push((key, f.clone()));
                    if k.len() > KEPT {
                        k.remove(0);
                    }
                    f
                }
            };
            // Points in the encoder's 1024 × 1024 picture; a box is two
            // points labelled 2 and 3, as SAM takes it.
            let s = size as f32;
            let mut coords = Vec::new();
            let mut labels = Vec::new();
            for p in prompts {
                match *p {
                    Prompt::Point { x, y, positive } => {
                        coords.extend([x as f32 * s, y as f32 * s]);
                        labels.push(i64::from(positive));
                    }
                    Prompt::Part { x, y } => {
                        coords.extend([x as f32 * s, y as f32 * s]);
                        labels.push(1);
                    }
                    Prompt::Box { x0, y0, x1, y1 } => {
                        coords.extend([x0 as f32 * s, y0 as f32 * s, x1 as f32 * s, y1 as f32 * s]);
                        labels.extend([2, 3]);
                    }
                }
            }
            let n = labels.len();
            let decoder = match model.decoders.entry(n) {
                std::collections::hash_map::Entry::Occupied(e) => e.into_mut(),
                std::collections::hash_map::Entry::Vacant(e) => {
                    let path = dir.join(format!("{}.decoder.onnx", spec.id));
                    e.insert(load(&path, Some(n), &spec)?)
                }
            };
            let points = tract_ndarray::Array3::from_shape_vec((1, n, 2), coords)
                .map_err(|e| e.to_string())?;
            let labels_t =
                tract_ndarray::Array2::from_shape_vec((1, n), labels).map_err(|e| e.to_string())?;
            let [embed, high0, high1] = features.as_ref().clone();
            let out = decoder
                .run(tvec!(
                    embed,
                    high0,
                    high1,
                    Tensor::from(points).into(),
                    Tensor::from(labels_t).into()
                ))
                .map_err(|e| format!("{} failed: {e}", spec.name))?;
            let masks = out[0]
                .to_plain_array_view::<f32>()
                .map_err(|e| e.to_string())?;
            let iou = out[1]
                .to_plain_array_view::<f32>()
                .map_err(|e| e.to_string())?;
            let shape = masks.shape().to_vec();
            let (count, mh, mw) = (shape[1], shape[2], shape[3]);
            let one_click = prompts.len() == 1
                && matches!(prompts[0], Prompt::Point { .. } | Prompt::Part { .. });
            let part = matches!(prompts[0], Prompt::Part { .. });
            let area = |k: usize| {
                (0..mh * mw)
                    .filter(|&i| masks[[0, k, i / mw, i % mw]] > 0.0)
                    .count()
            };
            // One click is ambiguous (a stud, the pattern, the whole ball),
            // and SAM's own scores favour the small parts: the object is the
            // largest it offers that isn't most of the picture (removing
            // wants it all); a `Part` click takes the part it is surest of.
            let total = mh * mw;
            let pick = if one_click && count > 1 {
                if part {
                    (1..count)
                        .max_by(|&a, &b| iou[[0, a]].total_cmp(&iou[[0, b]]))
                        .unwrap_or(0)
                } else {
                    (1..count)
                        .filter(|&k| area(k) * 10 < total * 6)
                        .max_by_key(|&k| area(k))
                        .unwrap_or(0)
                }
            } else {
                0
            };
            let data: Vec<f32> = (0..mh * mw)
                .map(|i| {
                    let v = masks[[0, pick, i / mw, i % mw]];
                    1.0 / (1.0 + (-v).exp())
                })
                .collect();
            Ok(Map {
                width: mw,
                height: mh,
                data,
            })
        },
    )
}

/// The encoder (`points` None) or the decoder for `points` points.
fn load(
    path: &std::path::Path,
    points: Option<usize>,
    spec: &ModelSpec,
) -> Result<Runnable, String> {
    let s = spec.input.size;
    tract_linalg::multithread::set_default_executor(
        tract_linalg::multithread::Executor::RayonGlobal,
    );
    let model = tract_onnx::onnx().model_for_path(path);
    let model = match points {
        None => model.and_then(|m| m.with_input_fact(0, f32::fact([1, 3, s, s]).into())),
        Some(n) => model
            .and_then(|m| m.with_input_fact(3, f32::fact([1, n, 2]).into()))
            .and_then(|m| m.with_input_fact(4, i64::fact([1, n]).into())),
    };
    model
        .and_then(crate::net::build)
        .map_err(|e| format!("{} didn't load ({}): {e}", spec.name, path.display()))
}

/// The encoder's features for a `size`×`size` picture (sRGB 0–1).
fn encode(encoder: &Runnable, spec: &ModelSpec, small: &[f32]) -> Result<Features, String> {
    let (size, mean, std) = (spec.input.size, spec.input.mean, spec.input.std);
    let input = tract_ndarray::Array4::from_shape_fn((1, 3, size, size), |(_, c, y, x)| {
        (small[(y * size + x) * 3 + c] - mean[c]) / std[c]
    });
    let out = encoder
        .run(tvec!(Tensor::from(input).into()))
        .map_err(|e| format!("{} failed: {e}", spec.name))?;
    // The outputs by their shape: the embedding and two finer feature maps.
    let by = |c: usize| {
        out.iter()
            .find(|o| o.shape().get(1) == Some(&c))
            .cloned()
            .ok_or_else(|| format!("{} gave no {c}-channel output", spec.name))
    };
    Ok(Arc::new([by(256)?, by(32)?, by(64)?]))
}

/// A 64-bit hash of the picture (FNV-1a over 8-bit values).
fn hash(rgb: &[f32]) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for v in rgb {
        h ^= (v.clamp(0.0, 1.0) * 255.0).round() as u64;
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Needs the model: `AMB_MODELS=dir-with-sam2.1-tiny cargo test -p
    /// amb-ai --release -- --ignored click`.
    #[test]
    #[ignore]
    fn a_click_selects_the_disc_it_is_on() {
        let Some(dir) = std::env::var_os("AMB_MODELS") else {
            return;
        };
        let select = selector(PathBuf::from(dir));
        let (w, h) = (600, 400);
        // A bright disc on a dark ground.
        let rgb: Vec<f32> = (0..w * h)
            .flat_map(|i| {
                let (x, y) = ((i % w) as f32 - 200.0, (i / w) as f32 - 200.0);
                if x * x + y * y < 80.0 * 80.0 {
                    [0.9, 0.7, 0.2]
                } else {
                    [0.1, 0.15, 0.2]
                }
            })
            .collect();
        let at = |m: &Map, u: f64, v: f64| {
            m.data[(v * m.height as f64) as usize * m.width + (u * m.width as f64) as usize]
        };
        let m = select(
            &rgb,
            w,
            h,
            &[Prompt::Point {
                x: 200.0 / 600.0,
                y: 0.5,
                positive: true,
            }],
        )
        .unwrap();
        assert!(at(&m, 200.0 / 600.0, 0.5) > 0.5);
        assert!(at(&m, 0.8, 0.5) < 0.5);
        // A box round it too.
        let b = select(
            &rgb,
            w,
            h,
            &[Prompt::Box {
                x0: 0.15,
                y0: 0.2,
                x1: 0.52,
                y1: 0.8,
            }],
        )
        .unwrap();
        assert!(at(&b, 200.0 / 600.0, 0.5) > 0.5 && at(&b, 0.8, 0.5) < 0.5);
    }
}
