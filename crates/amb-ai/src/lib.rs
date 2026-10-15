// SPDX-License-Identifier: GPL-3.0-or-later
//! Local AI models. For now one: U²-Net (small, "u2netp") to find the main
//! subject of a photo, for Select Subject masks
//! ([`amb_engine::subject`]).
//!
//! - **The model** is Xuebin Qin et al.'s U²-Net (Apache-2.0,
//!   <https://github.com/xuebinqin/U-2-Net>), converted to ONNX by the rembg
//!   project (MIT). About 4.6 MB. It isn't shipped with the app: it is
//!   downloaded in the background when the editor first starts (or when a
//!   subject mask first needs it), checked against its sha256, and kept in
//!   the editor's config folder (`models/`).
//! - **Running** is local, with `tract` (pure Rust), on all cores: about a
//!   second on a laptop. The result for a photo is kept, so moving sliders
//!   doesn't run it again; only a new photo or crop does.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tract_onnx::prelude::*;

pub const MODEL_FILE: &str = "u2netp.onnx";
pub const MODEL_SHA256: &str = "309c8469258dda742793dce0ebea8e6dd393174f89934733ecc8b14c76f4ddd8";
/// Where the model comes from, tried in order: our copy in the hub repo,
/// then the rembg release it was taken from.
pub const MODEL_URLS: [&str; 2] = [
    "https://raw.githubusercontent.com/Ambrotypehq/ambrotype-hub/main/models/u2netp.onnx",
    "https://github.com/danielgatis/rembg/releases/download/v0.0.0/u2netp.onnx",
];
const SIZE: usize = amb_engine::subject::SIZE;
/// ImageNet statistics, as the model was trained with.
const MEAN: [f32; 3] = [0.485, 0.456, 0.406];
const STD: [f32; 3] = [0.229, 0.224, 0.225];

type Model = Arc<TypedRunnableModel>;
/// A photo's fingerprint and its subject map.
type Kept = ([u64; 4], Arc<Vec<f32>>);

fn sha256(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The model file in `dir`, downloading it first if it isn't there (or
/// isn't the right file).
pub fn ensure_model(dir: &Path) -> Result<PathBuf, String> {
    // One download at a time (the start-up fetch and a first subject mask).
    static BUSY: Mutex<()> = Mutex::new(());
    let _one = BUSY.lock().unwrap_or_else(|p| p.into_inner());
    let path = dir.join(MODEL_FILE);
    if std::fs::read(&path).is_ok_and(|b| sha256(&b) == MODEL_SHA256) {
        return Ok(path);
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut problems = Vec::new();
    for url in MODEL_URLS {
        let agent = ureq::AgentBuilder::new()
            .timeout(std::time::Duration::from_secs(60))
            .try_proxy_from_env(true)
            .build();
        let got = agent
            .get(url)
            .call()
            .map_err(|e| e.to_string())
            .and_then(|r| {
                let mut buf = Vec::new();
                std::io::Read::read_to_end(
                    &mut std::io::Read::take(r.into_reader(), 20 << 20),
                    &mut buf,
                )
                .map(|_| buf)
                .map_err(|e| e.to_string())
            });
        match got {
            Ok(bytes) if sha256(&bytes) == MODEL_SHA256 => {
                let part = dir.join(format!("{MODEL_FILE}.part"));
                std::fs::write(&part, &bytes).map_err(|e| e.to_string())?;
                std::fs::rename(&part, &path).map_err(|e| e.to_string())?;
                return Ok(path);
            }
            Ok(_) => problems.push(format!("{url}: not the expected file")),
            Err(e) => problems.push(format!("{url}: {e}")),
        }
    }
    Err(format!(
        "couldn't download the subject model ({})",
        problems.join("; ")
    ))
}

fn load(path: &Path) -> Result<Model, String> {
    tract_onnx::onnx()
        .model_for_path(path)
        .and_then(|m| m.with_input_fact(0, f32::fact([1, 3, SIZE, SIZE]).into()))
        .and_then(|m| m.into_optimized())
        .and_then(|m| m.into_runnable())
        .map_err(|e| format!("the subject model didn't load: {e}"))
}

/// Run the model on `SIZE`×`SIZE` sRGB (0–1, interleaved): the subject's
/// share of each pixel, 0–1.
fn run(model: &Model, rgb: &[f32]) -> Result<Vec<f32>, String> {
    let max = rgb.iter().copied().fold(1e-6f32, f32::max);
    let input = tract_ndarray::Array4::from_shape_fn((1, 3, SIZE, SIZE), |(_, c, y, x)| {
        (rgb[(y * SIZE + x) * 3 + c] / max - MEAN[c]) / STD[c]
    });
    let out = model
        .run(tvec!(Tensor::from(input).into()))
        .map_err(|e| format!("the subject model failed: {e}"))?;
    let map: Vec<f32> = out[0]
        .to_plain_array_view::<f32>()
        .map_err(|e| e.to_string())?
        .iter()
        .copied()
        .collect();
    if map.len() != SIZE * SIZE {
        return Err(format!("the subject model gave {} values", map.len()));
    }
    // Stretched to 0–1, as its authors do.
    let (lo, hi) = map
        .iter()
        .fold((f32::MAX, f32::MIN), |(a, b), &v| (a.min(v), b.max(v)));
    let span = (hi - lo).max(1e-6);
    Ok(map.into_iter().map(|v| (v - lo) / span).collect())
}

/// A short fingerprint of what the photo looks like (bright or dark against
/// its average, on a 16×16 grid), so a result can be kept while sliders
/// move: exposure, colour and contrast changes leave it the same, a new
/// photo or crop doesn't.
fn fingerprint(rgb: &[f32]) -> [u64; 4] {
    let cell = SIZE / 16;
    let lum: Vec<f32> = (0..256)
        .map(|i| {
            let (cx, cy) = (i % 16, i / 16);
            let mut s = 0.0;
            for y in cy * cell..(cy + 1) * cell {
                for x in cx * cell..(cx + 1) * cell {
                    let p = &rgb[(y * SIZE + x) * 3..][..3];
                    s += 0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2];
                }
            }
            s
        })
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

/// The finder to register with [`amb_engine::subject::set_finder`]: the
/// model from `dir` (downloaded on first use), with the last few results
/// kept.
pub fn finder(dir: PathBuf) -> Arc<amb_engine::subject::Finder> {
    // Multi-threaded matrix work on rayon's pool, which the engine uses too.
    tract_linalg::multithread::set_default_executor(
        tract_linalg::multithread::Executor::RayonGlobal,
    );
    let model: Mutex<Option<Model>> = Mutex::new(None);
    // After a failed download, don't try again on every render.
    let failed: Mutex<Option<(std::time::Instant, String)>> = Mutex::new(None);
    let kept: Mutex<Vec<Kept>> = Mutex::new(Vec::new());
    Arc::new(move |rgb: &[f32]| -> Result<Vec<f32>, String> {
        if rgb.len() != SIZE * SIZE * 3 {
            return Err("wrong input size".into());
        }
        let key = fingerprint(rgb);
        if let Some((_, m)) = kept
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .iter()
            .find(|(k, _)| *k == key)
        {
            return Ok(m.as_ref().clone());
        }
        let m = {
            let mut slot = model.lock().unwrap_or_else(|p| p.into_inner());
            if slot.is_none() {
                let mut f = failed.lock().unwrap_or_else(|p| p.into_inner());
                if let Some((when, why)) = f.as_ref() {
                    if when.elapsed() < std::time::Duration::from_secs(300) {
                        return Err(why.clone());
                    }
                }
                match ensure_model(&dir).and_then(|p| load(&p)) {
                    Ok(m) => *slot = Some(m),
                    Err(e) => {
                        *f = Some((std::time::Instant::now(), e.clone()));
                        return Err(e);
                    }
                }
            }
            slot.clone().expect("loaded")
        };
        let map = Arc::new(run(&m, rgb)?);
        let mut k = kept.lock().unwrap_or_else(|p| p.into_inner());
        k.push((key, map.clone()));
        if k.len() > 8 {
            k.remove(0);
        }
        Ok(map.as_ref().clone())
    })
}

/// The editor's models folder (next to its settings).
pub fn default_dir() -> Option<PathBuf> {
    let env = |v: &str| {
        std::env::var_os(v)
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
    };
    if let Some(d) = env("AMB_MODELS") {
        return Some(d);
    }
    let config = if cfg!(windows) {
        env("APPDATA")?
    } else if cfg!(target_os = "macos") {
        env("HOME")?.join("Library/Application Support")
    } else {
        env("XDG_CONFIG_HOME").or_else(|| env("HOME").map(|h| h.join(".config")))?
    };
    Some(config.join("org.ambrotype.editor").join("models"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprints_ignore_exposure_but_not_the_picture() {
        let img: Vec<f32> = (0..SIZE * SIZE)
            .flat_map(|i| {
                let (x, y) = (i % SIZE, i / SIZE);
                let v = if (x / 40 + y / 60) % 2 == 0 { 0.2 } else { 0.7 };
                [v, v * 0.9, v * 0.8]
            })
            .collect();
        let brighter: Vec<f32> = img.iter().map(|v| (v * 1.3).min(1.0)).collect();
        assert_eq!(fingerprint(&img), fingerprint(&brighter));
        let flipped: Vec<f32> = img.iter().map(|v| 0.9 - v).collect();
        assert_ne!(fingerprint(&img), fingerprint(&flipped));
    }

    /// With the model at hand (`U2NETP=/path/u2netp.onnx`): a bright disc on
    /// a dark ground is found as the subject.
    #[test]
    #[ignore]
    fn finds_a_bright_disc() {
        let model = load(Path::new(&std::env::var("U2NETP").unwrap())).unwrap();
        let img: Vec<f32> = (0..SIZE * SIZE)
            .flat_map(|i| {
                let (x, y) = ((i % SIZE) as f32 - 160.0, (i / SIZE) as f32 - 160.0);
                if x * x + y * y < 70.0 * 70.0 {
                    [0.9, 0.5, 0.2]
                } else {
                    [0.1, 0.12, 0.15]
                }
            })
            .collect();
        let t = std::time::Instant::now();
        let map = run(&model, &img).unwrap();
        eprintln!("{:?}", t.elapsed());
        let at = |x: usize, y: usize| map[y * SIZE + x];
        assert!(at(160, 160) > 0.7, "centre {}", at(160, 160));
        assert!(at(10, 10) < 0.3, "corner {}", at(10, 10));
    }
}
