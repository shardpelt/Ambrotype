// SPDX-License-Identifier: GPL-3.0-or-later
//! Local AI models, for the masks that need one (Select Subject and
//! Background, Select People; [`amb_engine::subject`]).
//!
//! - **Which models:** `data/models.json` lists the ones we can install:
//!   only models whose weights are published under a licence that allows use
//!   in free software (Apache-2.0, MIT, …), each with its licence, authors
//!   and source. None ships with the app. Each task (`subject`, `people`) has
//!   a default; the user (or their AI assistant, through `amb models` or MCP)
//!   can install others and choose one per task.
//! - **Your own models:** any ONNX model that takes a square RGB photo and
//!   gives a map of the same side can be added with [`add_custom`], with its
//!   input size, normalisation and output activation.
//! - **Where:** the editor's config folder, `models/` (or `AMB_MODELS`):
//!   `<id>.onnx` for each installed model, `choices.json` for the chosen ones,
//!   and `custom/<id>.json` for the user's own models' descriptions.
//! - **Downloads** come from where the authors (or the converter) publish
//!   the model, and are checked against the sha256 in the list.
//! - **Running** is local, with `tract` (pure Rust), on all cores. The result
//!   for a photo is kept, so moving sliders doesn't run it again; only a new
//!   photo or crop does.

use std::collections::{BTreeMap, HashMap};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use amb_engine::subject::{Map, MapKind};
use serde::{Deserialize, Serialize};
use tract_onnx::prelude::*;

/// The tasks a model can do, with what they're for.
pub const TASKS: [(&str, &str); 2] = [
    ("subject", "Select Subject and Select Background"),
    ("people", "Select People (everyone, as one mask)"),
];

/// Models up to this size install by themselves the first time a mask needs
/// them; bigger ones are installed on request.
const AUTO_INSTALL_BYTES: u64 = 20 << 20;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Input {
    /// The square side the model takes, in pixels.
    pub size: usize,
    /// Colour normalisation: `(v / brightest − mean) / std`, per channel.
    pub mean: [f32; 3],
    pub std: [f32; 3],
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Activation {
    /// The output is used as it is (then stretched to 0–1).
    #[default]
    None,
    /// A logistic function first (models that give logits).
    Sigmoid,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Output {
    #[serde(default)]
    pub activation: Activation,
}

/// A model in the list, or one of the user's own.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelSpec {
    pub id: String,
    pub task: String,
    pub name: String,
    /// The task's model when none was chosen.
    #[serde(default)]
    pub default: bool,
    #[serde(default)]
    pub about: String,
    pub licence: String,
    #[serde(default)]
    pub by: String,
    #[serde(default)]
    pub source: String,
    /// Anything else to know before using it (for example a dataset's terms).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
    /// The file's size; 0 when unknown (the user's own).
    #[serde(default)]
    pub bytes: u64,
    #[serde(default)]
    pub sha256: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub urls: Vec<String>,
    pub input: Input,
    #[serde(default)]
    pub output: Output,
    /// Added by the user ([`add_custom`]), not from the list.
    #[serde(default)]
    pub custom: bool,
}

/// A model as [`list`] shows it.
#[derive(Debug, Clone, Serialize)]
pub struct ModelState {
    #[serde(flatten)]
    pub spec: ModelSpec,
    pub installed: bool,
    /// The one its task uses now.
    pub chosen: bool,
}

/// What the user says about their own model ([`add_custom`]).
#[derive(Debug, Clone, Deserialize)]
pub struct CustomModel {
    pub task: String,
    pub name: String,
    pub size: usize,
    #[serde(default = "imagenet_mean")]
    pub mean: [f32; 3],
    #[serde(default = "imagenet_std")]
    pub std: [f32; 3],
    #[serde(default)]
    pub activation: Activation,
    #[serde(default)]
    pub licence: String,
    #[serde(default)]
    pub source: String,
}

fn imagenet_mean() -> [f32; 3] {
    [0.485, 0.456, 0.406]
}

fn imagenet_std() -> [f32; 3] {
    [0.229, 0.224, 0.225]
}

#[derive(Deserialize)]
struct Catalogue {
    models: Vec<ModelSpec>,
}

/// The models we can install.
pub fn catalogue() -> Vec<ModelSpec> {
    let c: Catalogue =
        serde_json::from_str(include_str!("../data/models.json")).expect("models.json is valid");
    c.models
}

fn known_task(task: &str) -> Result<(), String> {
    if TASKS.iter().any(|(t, _)| *t == task) {
        Ok(())
    } else {
        Err(format!(
            "{task:?} is not a task; the tasks are: {}",
            TASKS.map(|(t, _)| t).join(", ")
        ))
    }
}

fn file_of(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.onnx"))
}

fn is_installed(dir: &Path, spec: &ModelSpec) -> bool {
    std::fs::metadata(file_of(dir, &spec.id))
        .is_ok_and(|m| m.is_file() && (spec.bytes == 0 || m.len() == spec.bytes))
}

fn custom_specs(dir: &Path) -> Vec<ModelSpec> {
    let mut out: Vec<ModelSpec> = std::fs::read_dir(dir.join("custom"))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| std::fs::read_to_string(e.path()).ok())
        .filter_map(|t| serde_json::from_str::<ModelSpec>(&t).ok())
        .map(|mut s| {
            s.custom = true;
            s.default = false;
            s
        })
        .collect();
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

fn all_specs(dir: &Path) -> Vec<ModelSpec> {
    let mut all = catalogue();
    all.extend(custom_specs(dir));
    all
}

fn find_spec(dir: &Path, id: &str) -> Result<ModelSpec, String> {
    all_specs(dir)
        .into_iter()
        .find(|s| s.id == id)
        .ok_or_else(|| format!("no model {id:?} (`amb models list` shows them)"))
}

fn choices(dir: &Path) -> BTreeMap<String, String> {
    std::fs::read_to_string(dir.join("choices.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn write_choices(dir: &Path, c: &BTreeMap<String, String>) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let text = serde_json::to_string_pretty(c).expect("plain map");
    std::fs::write(dir.join("choices.json"), text).map_err(|e| e.to_string())
}

/// The model `task` uses: the chosen one if it's still there, else the
/// task's default.
pub fn chosen(dir: &Path, task: &str) -> Result<ModelSpec, String> {
    known_task(task)?;
    let all = all_specs(dir);
    if let Some(id) = choices(dir).get(task) {
        if let Some(s) = all.iter().find(|s| &s.id == id && s.task == task) {
            if is_installed(dir, s) {
                return Ok(s.clone());
            }
        }
    }
    all.into_iter()
        .find(|s| s.task == task && s.default)
        .ok_or_else(|| format!("no model for {task}"))
}

/// Every model, installed or not, with the chosen ones marked.
pub fn list(dir: &Path) -> Vec<ModelState> {
    let chosen_ids: Vec<String> = TASKS
        .iter()
        .filter_map(|(t, _)| chosen(dir, t).ok().map(|s| s.id))
        .collect();
    all_specs(dir)
        .into_iter()
        .map(|spec| ModelState {
            installed: is_installed(dir, &spec),
            chosen: chosen_ids.contains(&spec.id),
            spec,
        })
        .collect()
}

/// Use model `id` for its task. It has to be installed first.
pub fn choose(dir: &Path, id: &str) -> Result<ModelSpec, String> {
    let spec = find_spec(dir, id)?;
    if !is_installed(dir, &spec) {
        return Err(format!(
            "{} isn't installed yet (`amb models install {id}`)",
            spec.name
        ));
    }
    let mut c = choices(dir);
    c.insert(spec.task.clone(), spec.id.clone());
    write_choices(dir, &c)?;
    Ok(spec)
}

fn sha256_file(path: &Path) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    let mut f = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// Download model `id` (if it isn't there already) and check it.
/// `progress` hears the bytes so far and the total.
pub fn install(dir: &Path, id: &str, progress: &dyn Fn(u64, u64)) -> Result<ModelSpec, String> {
    // One download at a time (the start-up fetch and a first mask, say).
    static BUSY: Mutex<()> = Mutex::new(());
    let _one = BUSY.lock().unwrap_or_else(|p| p.into_inner());
    let spec = find_spec(dir, id)?;
    if is_installed(dir, &spec) {
        return Ok(spec);
    }
    if spec.custom {
        return Err(format!("{}'s file is missing; add it again", spec.name));
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let path = file_of(dir, id);
    let part = dir.join(format!("{id}.onnx.part"));
    let mut problems = Vec::new();
    for url in &spec.urls {
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(std::time::Duration::from_secs(30))
            .timeout_read(std::time::Duration::from_secs(60))
            .try_proxy_from_env(true)
            .build();
        let got = (|| -> Result<(), String> {
            let r = agent.get(url).call().map_err(|e| e.to_string())?;
            let mut src = r.into_reader().take(spec.bytes + 1);
            let mut out = std::fs::File::create(&part).map_err(|e| e.to_string())?;
            let mut buf = vec![0u8; 1 << 16];
            let mut done = 0u64;
            loop {
                let n = src.read(&mut buf).map_err(|e| e.to_string())?;
                if n == 0 {
                    break;
                }
                out.write_all(&buf[..n]).map_err(|e| e.to_string())?;
                done += n as u64;
                progress(done, spec.bytes);
            }
            out.flush().map_err(|e| e.to_string())?;
            if done != spec.bytes || sha256_file(&part)? != spec.sha256 {
                return Err("not the expected file".into());
            }
            std::fs::rename(&part, &path).map_err(|e| e.to_string())
        })();
        match got {
            Ok(()) => return Ok(spec),
            Err(e) => {
                let _ = std::fs::remove_file(&part);
                problems.push(format!("{url}: {e}"));
            }
        }
    }
    Err(format!(
        "couldn't download {} ({})",
        spec.name,
        problems.join("; ")
    ))
}

/// Delete model `id`'s file (and, for the user's own, its description). Its
/// task goes back to its default.
pub fn remove(dir: &Path, id: &str) -> Result<(), String> {
    let spec = find_spec(dir, id)?;
    let _ = std::fs::remove_file(file_of(dir, id));
    if spec.custom {
        let _ = std::fs::remove_file(dir.join("custom").join(format!("{id}.json")));
    }
    let mut c = choices(dir);
    if c.get(&spec.task) == Some(&spec.id) {
        c.remove(&spec.task);
        write_choices(dir, &c)?;
    }
    Ok(())
}

/// Add the user's own ONNX model: copied into the models folder, checked
/// that it loads and gives a map, and listed as `own-<name>`.
pub fn add_custom(dir: &Path, file: &Path, m: CustomModel) -> Result<ModelSpec, String> {
    known_task(&m.task)?;
    if !(16..=4096).contains(&m.size) {
        return Err(format!("size {} should be 16 to 4096 pixels", m.size));
    }
    let slug: String = m
        .name
        .to_ascii_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if slug.is_empty() {
        return Err("give the model a name".into());
    }
    let spec = ModelSpec {
        id: format!("own-{slug}"),
        task: m.task,
        name: m.name,
        default: false,
        about: "Your own model.".into(),
        licence: if m.licence.is_empty() {
            "(yours)".into()
        } else {
            m.licence
        },
        by: String::new(),
        source: m.source,
        notes: String::new(),
        bytes: std::fs::metadata(file)
            .map_err(|e| format!("{}: {e}", file.display()))?
            .len(),
        sha256: sha256_file(file)?,
        urls: Vec::new(),
        input: Input {
            size: m.size,
            mean: m.mean,
            std: m.std,
        },
        output: Output {
            activation: m.activation,
        },
        custom: true,
    };
    // It has to run before it's listed.
    let model = load(file, &spec)?;
    let grey = vec![0.5f32; m.size * m.size * 3];
    run(&model, &spec, &grey)?;
    std::fs::create_dir_all(dir.join("custom")).map_err(|e| e.to_string())?;
    std::fs::copy(file, file_of(dir, &spec.id)).map_err(|e| e.to_string())?;
    let text = serde_json::to_string_pretty(&spec).expect("plain spec");
    std::fs::write(dir.join("custom").join(format!("{}.json", spec.id)), text)
        .map_err(|e| e.to_string())?;
    Ok(spec)
}

/// The default subject model, downloaded if it isn't there (the editor
/// fetches it in the background when it starts).
pub fn ensure_model(dir: &Path) -> Result<PathBuf, String> {
    let spec = chosen(dir, "subject")?;
    install(dir, &spec.id, &|_, _| {})?;
    Ok(file_of(dir, &spec.id))
}

type Model = Arc<TypedRunnableModel>;

fn load(path: &Path, spec: &ModelSpec) -> Result<Model, String> {
    let s = spec.input.size;
    tract_onnx::onnx()
        .model_for_path(path)
        .and_then(|m| m.with_input_fact(0, f32::fact([1, 3, s, s]).into()))
        .and_then(|m| m.into_optimized())
        .and_then(|m| m.into_runnable())
        .map_err(|e| format!("{} didn't load: {e}", spec.name))
}

/// Run a model on `size`×`size` sRGB (0–1, interleaved): its map, 0–1.
fn run(model: &Model, spec: &ModelSpec, rgb: &[f32]) -> Result<Vec<f32>, String> {
    let (size, mean, std) = (spec.input.size, spec.input.mean, spec.input.std);
    let max = rgb.iter().copied().fold(1e-6f32, f32::max);
    let input = tract_ndarray::Array4::from_shape_fn((1, 3, size, size), |(_, c, y, x)| {
        (rgb[(y * size + x) * 3 + c] / max - mean[c]) / std[c]
    });
    let out = model
        .run(tvec!(Tensor::from(input).into()))
        .map_err(|e| format!("{} failed: {e}", spec.name))?;
    let mut map: Vec<f32> = out[0]
        .to_plain_array_view::<f32>()
        .map_err(|e| e.to_string())?
        .iter()
        .copied()
        .collect();
    if map.len() != size * size {
        return Err(format!(
            "{} gave {} values, not {}×{}",
            spec.name,
            map.len(),
            size,
            size
        ));
    }
    if spec.output.activation == Activation::Sigmoid {
        for v in &mut map {
            *v = 1.0 / (1.0 + (-*v).exp());
        }
    }
    // Stretched to 0–1, as the authors do.
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
fn fingerprint(rgb: &[f32], w: usize, h: usize) -> [u64; 4] {
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

/// Run model `id` on a photo's sRGB (`w`×`h`) once, for trying a model
/// (`amb models try`): its map and how long it took.
pub fn try_model(
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

    fn checker(w: usize, h: usize, scale: f32) -> Vec<f32> {
        (0..w * h)
            .flat_map(|i| {
                let (x, y) = (i % w, i / w);
                let v = if (x / 40 + y / 60) % 2 == 0 { 0.2 } else { 0.7 };
                [v * scale, v * 0.9 * scale, v * 0.8 * scale].map(|c| c.min(1.0))
            })
            .collect()
    }

    #[test]
    fn fingerprints_ignore_exposure_but_not_the_picture() {
        let img = checker(320, 320, 1.0);
        assert_eq!(
            fingerprint(&img, 320, 320),
            fingerprint(&checker(320, 320, 1.3), 320, 320)
        );
        let flipped: Vec<f32> = img.iter().map(|v| 0.9 - v).collect();
        assert_ne!(fingerprint(&img, 320, 320), fingerprint(&flipped, 320, 320));
    }

    #[test]
    fn the_list_has_one_default_per_task_and_free_licences() {
        let all = catalogue();
        for (task, _) in TASKS {
            assert_eq!(
                all.iter().filter(|s| s.task == task && s.default).count(),
                1,
                "{task}"
            );
        }
        let mut ids: Vec<_> = all.iter().map(|s| &s.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), all.len(), "ids are unique");
        for s in &all {
            known_task(&s.task).unwrap();
            assert!(
                ["Apache-2.0", "MIT", "BSD-3-Clause"].contains(&s.licence.as_str()),
                "{}",
                s.id
            );
            assert!(
                s.bytes > 0 && s.sha256.len() == 64 && !s.urls.is_empty(),
                "{}",
                s.id
            );
            assert!(s.source.starts_with("https://"), "{}", s.id);
        }
    }

    #[test]
    fn choosing_needs_an_installed_model_and_removing_goes_back_to_the_default() {
        let dir = std::env::temp_dir().join(format!("amb-ai-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(chosen(&dir, "subject").unwrap().id, "u2netp");
        assert!(choose(&dir, "u2net").is_err());
        assert!(chosen(&dir, "nothing").is_err());
        // A file of the right size stands in for the download.
        std::fs::create_dir_all(&dir).unwrap();
        let big = catalogue().into_iter().find(|s| s.id == "u2net").unwrap();
        std::fs::File::create(file_of(&dir, "u2net"))
            .unwrap()
            .set_len(big.bytes)
            .unwrap();
        choose(&dir, "u2net").unwrap();
        assert_eq!(chosen(&dir, "subject").unwrap().id, "u2net");
        let l = list(&dir);
        let row = l.iter().find(|m| m.spec.id == "u2net").unwrap();
        assert!(row.installed && row.chosen);
        assert!(!l.iter().find(|m| m.spec.id == "u2netp").unwrap().chosen);
        remove(&dir, "u2net").unwrap();
        assert_eq!(chosen(&dir, "subject").unwrap().id, "u2netp");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// With the models downloaded (`AMB_MODELS=dir`): each finds a bright
    /// disc on a dark ground as the subject.
    #[test]
    #[ignore]
    fn installed_models_find_a_bright_disc() {
        let dir = PathBuf::from(std::env::var("AMB_MODELS").unwrap());
        let (w, h) = (640, 480);
        let img: Vec<f32> = (0..w * h)
            .flat_map(|i| {
                let (x, y) = ((i % w) as f32 - 320.0, (i / w) as f32 - 240.0);
                if x * x + y * y < 120.0 * 120.0 {
                    [0.9, 0.5, 0.2]
                } else {
                    [0.1, 0.12, 0.15]
                }
            })
            .collect();
        for m in list(&dir)
            .into_iter()
            .filter(|m| m.installed && m.spec.task == "subject")
        {
            let (map, took) = try_model(&dir, &m.spec.id, &img, w, h).unwrap();
            let at = |u: f32, v: f32| {
                map.data
                    [(v * map.height as f32) as usize * map.width + (u * map.width as f32) as usize]
            };
            eprintln!(
                "{}: {took:?} centre {} corner {}",
                m.spec.id,
                at(0.5, 0.5),
                at(0.03, 0.03)
            );
            assert!(at(0.5, 0.5) > 0.7, "{}", m.spec.id);
            assert!(at(0.03, 0.03) < 0.3, "{}", m.spec.id);
        }
    }
}
