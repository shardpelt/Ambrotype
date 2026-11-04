// SPDX-License-Identifier: GPL-3.0-or-later
//! AI Remove: the inpainter the engine fills content-aware spots with
//! ([`amb_engine::retouch::set_inpainter`]), running the model the "remove"
//! task uses with tract, once the user installs it: Big-LaMa (Apache-2.0,
//! 208 MB, the best fills) or MI-GAN (MIT, 30 MB, several times faster).
//! Nothing is downloaded by itself. Without one, the engine heals such
//! spots from a searched donor.
//!
//! LaMa takes the photo (0..1, colour planes first) and the mask (1 = fill)
//! at 512 × 512, and gives the filled photo in 0..255. MI-GAN takes and
//! gives bytes (0..255), its mask 255 where the photo is kept. The last
//! results are kept, so a spot is filled once per preview size, not on
//! every render.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use amb_engine::retouch::{INPAINT_SIZE, Inpainter};
use tract_onnx::prelude::*;

use crate::Pixels;

/// A loaded remove model, and what its pixels are.
#[derive(Clone)]
pub struct Model {
    runnable: Arc<TypedRunnableModel>,
    pixels: Pixels,
}

/// Results kept: (a fingerprint of the input, the fill).
const KEPT: usize = 16;

/// The inpainter for models in `dir`: `Err` (so the engine heals instead)
/// until the "remove" task's model is installed.
pub fn inpainter(dir: PathBuf) -> Arc<Inpainter> {
    let model: Mutex<Option<(String, Model)>> = Mutex::new(None);
    let kept: Mutex<VecDeque<(u64, Arc<Vec<f32>>)>> = Mutex::new(VecDeque::new());
    Arc::new(
        move |rgb: &[f32], mask: &[f32]| -> Result<Vec<f32>, String> {
            let n = INPAINT_SIZE;
            if rgb.len() != n * n * 3 || mask.len() != n * n {
                return Err("wrong input size".into());
            }
            let key = fingerprint(rgb, mask);
            if let Some((_, out)) = kept
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .iter()
                .find(|(k, _)| *k == key)
            {
                return Ok(out.as_ref().clone());
            }
            let spec = crate::chosen(&dir, "remove")?;
            let file = dir.join(format!("{}.onnx", spec.id));
            if !crate::is_installed(&dir, &spec) {
                return Err(format!("{} isn't installed", spec.name));
            }
            let runnable = {
                let mut m = model.lock().unwrap_or_else(|e| e.into_inner());
                match m.as_ref() {
                    Some((id, r)) if *id == spec.id => r.clone(),
                    _ => {
                        let r = load(&file, spec.input.pixels)?;
                        *m = Some((spec.id.clone(), r.clone()));
                        r
                    }
                }
            };
            let out = run(&runnable, rgb, mask)?;
            let out = Arc::new(out);
            let mut k = kept.lock().unwrap_or_else(|e| e.into_inner());
            k.push_front((key, out.clone()));
            k.truncate(KEPT);
            Ok(out.as_ref().clone())
        },
    )
}

/// A remove model with its two inputs fixed at the inpainter's size.
pub fn load(path: &Path, pixels: Pixels) -> Result<Model, String> {
    let n = INPAINT_SIZE;
    let name = path.display();
    tract_linalg::multithread::set_default_executor(
        tract_linalg::multithread::Executor::RayonGlobal,
    );
    let fact = |shape: [usize; 4]| -> InferenceFact {
        match pixels {
            Pixels::Float => f32::fact(shape).into(),
            Pixels::Bytes => u8::fact(shape).into(),
        }
    };
    tract_onnx::onnx()
        .model_for_path(path)
        .and_then(|m| m.with_input_fact(0, fact([1, 3, n, n])))
        .and_then(|m| m.with_input_fact(1, fact([1, 1, n, n])))
        .and_then(|m| m.into_optimized())
        .and_then(|m| m.into_runnable())
        .map(|runnable| Model { runnable, pixels })
        .map_err(|e| format!("{name} didn't load: {e}"))
}

/// Fill `rgb` (interleaved 0..1) where `mask` is set; interleaved 0..1 out.
pub fn run(model: &Model, rgb: &[f32], mask: &[f32]) -> Result<Vec<f32>, String> {
    let n = INPAINT_SIZE;
    let planes: Vec<f32> = (0..3)
        .flat_map(|c| (0..n * n).map(move |i| rgb[i * 3 + c]))
        .collect();
    let shape = |c: usize| (1, c, n, n);
    let inputs: TVec<TValue> = match model.pixels {
        Pixels::Float => {
            let image = tract_ndarray::Array4::from_shape_vec(shape(3), planes);
            let hole = tract_ndarray::Array4::from_shape_vec(shape(1), mask.to_vec());
            let (image, hole) = image
                .and_then(|i| hole.map(|h| (i, h)))
                .map_err(|e| e.to_string())?;
            tvec!(Tensor::from(image).into(), Tensor::from(hole).into())
        }
        Pixels::Bytes => {
            let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
            let image = tract_ndarray::Array4::from_shape_vec(
                shape(3),
                planes.iter().map(|&v| byte(v)).collect(),
            );
            // MI-GAN keeps where the mask is 255.
            let keep = tract_ndarray::Array4::from_shape_vec(
                shape(1),
                mask.iter()
                    .map(|&m| if m >= 0.5 { 0u8 } else { 255u8 })
                    .collect(),
            );
            let (image, keep) = image
                .and_then(|i| keep.map(|k| (i, k)))
                .map_err(|e| e.to_string())?;
            tvec!(Tensor::from(image).into(), Tensor::from(keep).into())
        }
    };
    let out = model
        .runnable
        .run(inputs)
        .map_err(|e| format!("AI Remove: {e}"))?;
    let t = out[0]
        .cast_to::<f32>()
        .map_err(|e| e.to_string())?
        .into_owned();
    if t.shape() != [1, 3, n, n] {
        return Err(format!("AI Remove gave {:?}", t.shape()));
    }
    let flat: Vec<f32> = t
        .to_plain_array_view::<f32>()
        .map_err(|e| e.to_string())?
        .iter()
        .copied()
        .collect();
    Ok((0..n * n * 3)
        .map(|i| (flat[(i % 3) * n * n + i / 3] / 255.0).clamp(0.0, 1.0))
        .collect())
}

/// A 64-bit hash of the input (FNV-1a over 8-bit values: enough to tell
/// renders apart, and cheap).
fn fingerprint(rgb: &[f32], mask: &[f32]) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for v in rgb.iter().chain(mask) {
        h ^= (v.clamp(0.0, 1.0) * 255.0).round() as u64;
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Needs the model: `AMB_LAMA=path/to/lama.onnx cargo test -p amb-ai
    /// --release -- --ignored stripes` (MI-GAN: `AMB_LAMA=migan.onnx
    /// AMB_REMOVE_BYTES=1`).
    #[test]
    #[ignore]
    fn lama_continues_stripes_through_the_hole() {
        let Some(path) = std::env::var_os("AMB_LAMA") else {
            return;
        };
        let bytes = std::env::var_os("AMB_REMOVE_BYTES").is_some();
        let model = load(
            Path::new(&path),
            if bytes { Pixels::Bytes } else { Pixels::Float },
        )
        .unwrap();
        let n = INPAINT_SIZE;
        // Vertical stripes, 32 px wide; a hole in the middle.
        let stripe = |x: usize| if (x / 32) % 2 == 0 { 0.8 } else { 0.2 };
        let truth: Vec<f32> = (0..n * n)
            .flat_map(|i| {
                let v = stripe(i % n);
                [v, v * 0.9, v * 0.8]
            })
            .collect();
        let hole = |i: usize| {
            let (x, y) = ((i % n) as f32 - 256.0, (i / n) as f32 - 256.0);
            x * x + y * y < 60.0 * 60.0
        };
        let mask: Vec<f32> = (0..n * n).map(|i| f32::from(hole(i))).collect();
        let mut input = truth.clone();
        for i in 0..n * n {
            if hole(i) {
                input[i * 3..i * 3 + 3].fill(0.0);
            }
        }
        let out = run(&model, &input, &mask).unwrap();
        let (mut err, mut count) = (0.0, 0);
        for i in (0..n * n).filter(|&i| hole(i)) {
            err += (out[i * 3] - truth[i * 3]).abs();
            count += 1;
        }
        let err = err / count as f32;
        // A flat fill would be 0.3 off on average.
        assert!(err < 0.12, "mean error in the hole {err}");
    }
}
