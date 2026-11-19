// SPDX-License-Identifier: GPL-3.0-or-later
//! Loading a model and running it on a photo.

use super::spec::{Activation, Layout, ModelSpec};
#[cfg(test)]
use super::store::find_spec;
use super::store::{chosen, file_of, install, is_installed};
use crate::AUTO_INSTALL_BYTES;
use amb_engine::subject::{Map, MapKind};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tract_onnx::prelude::*;

type Model = Arc<crate::net::Net>;

pub(super) fn load(path: &Path, spec: &ModelSpec) -> Result<Model, String> {
    let s = spec.input.size;
    tract_onnx::onnx()
        .model_for_path(path)
        .and_then(|m| {
            let shape = match spec.input.layout {
                Layout::Nchw => [1, 3, s, s],
                Layout::Nhwc => [1, s, s, 3],
            };
            m.with_input_fact(0, f32::fact(shape).into())
        })
        .and_then(crate::net::build)
        .map_err(|e| format!("{} didn't load: {e}", spec.name))
}

/// Run a model on `size`×`size` sRGB (0–1, interleaved): its map, 0–1.
pub(super) fn run(model: &Model, spec: &ModelSpec, rgb: &[f32]) -> Result<Vec<f32>, String> {
    let (size, mean, std) = (spec.input.size, spec.input.mean, spec.input.std);
    let max = rgb.iter().copied().fold(1e-6f32, f32::max);
    let px = |y: usize, x: usize, c: usize| (rgb[(y * size + x) * 3 + c] / max - mean[c]) / std[c];
    let input = match spec.input.layout {
        Layout::Nchw => {
            tract_ndarray::Array4::from_shape_fn((1, 3, size, size), |(_, c, y, x)| px(y, x, c))
        }
        Layout::Nhwc => {
            tract_ndarray::Array4::from_shape_fn((1, size, size, 3), |(_, y, x, c)| px(y, x, c))
        }
    };
    let out = model
        .run(tvec!(Tensor::from(input).into()))
        .map_err(|e| format!("{} failed: {e}", spec.name))?;
    let view = out[0]
        .to_plain_array_view::<f32>()
        .map_err(|e| e.to_string())?;
    let shape = view.shape().to_vec();
    let values: Vec<f32> = view.iter().copied().collect();
    let n = size * size;
    if values.is_empty() || values.len() % n != 0 {
        return Err(format!(
            "{} gave {} values ({shape:?}), not maps of {size}×{size}",
            spec.name,
            values.len()
        ));
    }
    // One map, or one per class: colour-last ([1, s, s, C]) or planes.
    let classes = values.len() / n;
    let last_dim = shape.last().copied().unwrap_or(1);
    let at = |i: usize, c: usize| {
        if classes > 1 && last_dim == classes {
            values[i * classes + c]
        } else {
            values[c * n + i]
        }
    };
    let channel = match spec.output.channel {
        Some(c) if c < classes => c,
        Some(c) => {
            return Err(format!(
                "{} gives {classes} maps; channel {c} isn't one (0 to {})",
                spec.name,
                classes - 1
            ));
        }
        None if classes == 1 => 0,
        None => {
            return Err(format!(
                "{} gives {classes} maps; say which one to use (channel)",
                spec.name
            ));
        }
    };
    let mut map: Vec<f32> = match spec.output.activation {
        Activation::Softmax => (0..n)
            .map(|i| {
                let top = (0..classes).map(|c| at(i, c)).fold(f32::MIN, f32::max);
                let sum: f32 = (0..classes).map(|c| (at(i, c) - top).exp()).sum();
                (at(i, channel) - top).exp() / sum
            })
            .collect(),
        Activation::Sigmoid => (0..n)
            .map(|i| 1.0 / (1.0 + (-at(i, channel)).exp()))
            .collect(),
        Activation::None => (0..n).map(|i| at(i, channel)).collect(),
    };
    if spec.output.activation != Activation::Softmax {
        // Stretched to 0–1, as the authors do.
        let (lo, hi) = map
            .iter()
            .fold((f32::MAX, f32::MIN), |(a, b), &v| (a.min(v), b.max(v)));
        let span = (hi - lo).max(1e-6);
        for v in &mut map {
            *v = (*v - lo) / span;
        }
    }
    if spec.output.invert {
        for v in &mut map {
            *v = 1.0 - *v;
        }
    }
    Ok(map)
}

/// A short fingerprint of what the photo looks like (bright or dark against
/// its average, on a 16×16 grid), so a result can be kept while sliders
/// move: exposure, colour and contrast changes leave it the same, a new
/// photo or crop doesn't.
pub(super) fn fingerprint(rgb: &[f32], w: usize, h: usize) -> [u64; 4] {
    let small = amb_engine::subject::shrink(rgb, w, h, 16);
    let lum: Vec<f32> = small
        .chunks(3)
        .map(|p| 0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2])
        .collect();
    let mean = lum.iter().sum::<f32>() / 256.0;
    let mut bits = [0u64; 4];
    for (i, l) in lum.iter().enumerate() {
        if *l > mean {
            bits[i / 64] |= 1 << (i % 64);
        }
    }
    bits
}

/// The finder to register with [`amb_engine::subject::set_finder`]: for
/// each kind of mask, the model its task uses (the small default downloads
/// itself on first use), with the last few results kept.
pub fn finder(dir: PathBuf) -> Arc<amb_engine::subject::Finder> {
    // Multi-threaded matrix work on rayon's pool, which the engine uses too.
    tract_linalg::multithread::set_default_executor(
        tract_linalg::multithread::Executor::RayonGlobal,
    );
    let models: Mutex<HashMap<String, Model>> = Mutex::new(HashMap::new());
    // After a failed download or load, don't try again on every render.
    let failed: Mutex<HashMap<String, (std::time::Instant, String)>> = Mutex::new(HashMap::new());
    type Kept = (String, [u64; 4], Arc<Vec<f32>>);
    let kept: Mutex<Vec<Kept>> = Mutex::new(Vec::new());
    Arc::new(
        move |kind: MapKind, rgb: &[f32], w: usize, h: usize| -> Result<Map, String> {
            if rgb.len() != w * h * 3 || w == 0 || h == 0 {
                return Err("wrong input size".into());
            }
            let spec = chosen(&dir, kind.task())?;
            let size = spec.input.size;
            let key = fingerprint(rgb, w, h);
            let found = kept
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .iter()
                .find(|(id, k, _)| *id == spec.id && *k == key)
                .map(|(_, _, m)| m.clone());
            let data = match found {
                Some(m) => m,
                None => {
                    let model = {
                        let mut loaded = models.lock().unwrap_or_else(|p| p.into_inner());
                        match loaded.get(&spec.id) {
                            Some(m) => m.clone(),
                            None => {
                                let mut f = failed.lock().unwrap_or_else(|p| p.into_inner());
                                if let Some((when, why)) = f.get(&spec.id) {
                                    if when.elapsed() < std::time::Duration::from_secs(300) {
                                        return Err(why.clone());
                                    }
                                }
                                let got = if is_installed(&dir, &spec)
                                    || (spec.bytes <= AUTO_INSTALL_BYTES && !spec.custom)
                                {
                                    install(&dir, &spec.id, &|_, _| {})
                                        .and_then(|s| load(&file_of(&dir, &s.id), &s))
                                } else {
                                    Err(format!(
                                        "install a model for {} first: Settings → AI models, or `amb models install {}`",
                                        kind.task(),
                                        spec.id
                                    ))
                                };
                                match got {
                                    Ok(m) => {
                                        loaded.insert(spec.id.clone(), m.clone());
                                        m
                                    }
                                    Err(e) => {
                                        f.insert(
                                            spec.id.clone(),
                                            (std::time::Instant::now(), e.clone()),
                                        );
                                        return Err(e);
                                    }
                                }
                            }
                        }
                    };
                    let small = amb_engine::subject::shrink(rgb, w, h, size);
                    let map = Arc::new(run(&model, &spec, &small)?);
                    let mut k = kept.lock().unwrap_or_else(|p| p.into_inner());
                    k.push((spec.id.clone(), key, map.clone()));
                    if k.len() > 8 {
                        k.remove(0);
                    }
                    map
                }
            };
            Ok(Map {
                width: size,
                height: size,
                data: data.as_ref().clone(),
            })
        },
    )
}

/// Run model `id` on a photo's sRGB (`w`×`h`) once, for trying a model:
/// its map and how long it took. Only the tests use it now (no `amb models
/// try` command exists).
#[cfg(test)]
pub(crate) fn try_model(
    dir: &Path,
    id: &str,
    rgb: &[f32],
    w: usize,
    h: usize,
) -> Result<(Map, std::time::Duration), String> {
    let spec = find_spec(dir, id)?;
    if !is_installed(dir, &spec) {
        return Err(format!("{} isn't installed", spec.name));
    }
    let model = load(&file_of(dir, id), &spec)?;
    let small = amb_engine::subject::shrink(rgb, w, h, spec.input.size);
    let t = std::time::Instant::now();
    let data = run(&model, &spec, &small)?;
    Ok((
        Map {
            width: spec.input.size,
            height: spec.input.size,
            data,
        },
        t.elapsed(),
    ))
}
