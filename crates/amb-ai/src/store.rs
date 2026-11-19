// SPDX-License-Identifier: GPL-3.0-or-later
//! Installed models and the person's choice per task: install, download, remove, add their own.

use super::model::{load, run};
use super::spec::{CustomModel, Input, ModelSpec, ModelState, Output, Pixels, catalogue};
use crate::{TASKS, archive};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub(super) fn known_task(task: &str) -> Result<(), String> {
    if TASKS.iter().any(|(t, _)| *t == task) {
        Ok(())
    } else {
        Err(format!(
            "{task:?} is not a task; the tasks are: {}",
            TASKS.map(|(t, _)| t).join(", ")
        ))
    }
}

pub(crate) fn file_of(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.onnx"))
}

pub(crate) fn is_installed(dir: &Path, spec: &ModelSpec) -> bool {
    let there = |path: PathBuf, bytes: u64| {
        std::fs::metadata(path).is_ok_and(|m| m.is_file() && (bytes == 0 || m.len() == bytes))
    };
    there(file_of(dir, &spec.id), spec.bytes)
        && spec
            .files
            .iter()
            .all(|f| there(dir.join(format!("{}.{}", spec.id, f.name)), f.bytes))
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

pub(super) fn find_spec(dir: &Path, id: &str) -> Result<ModelSpec, String> {
    all_specs(dir)
        .into_iter()
        .find(|s| s.id == id)
        .ok_or_else(|| format!("no model {id:?} (`amb models list` shows them)"))
}

pub(super) fn choices(dir: &Path) -> BTreeMap<String, String> {
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
    let mut all = all_specs(dir);
    // Per task, the finest first.
    all.sort_by(|a, b| a.task.cmp(&b.task).then(b.detail.cmp(&a.detail)));
    all.into_iter()
        .map(|spec| ModelState {
            total_bytes: spec.total_bytes(),
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
    if let Some(a) = &spec.archive {
        let zip = dir.join(format!("{id}.zip"));
        download(&zip, a.bytes, &a.sha256, &a.urls, &|done| {
            progress(done, a.bytes)
        })
        .map_err(|e| format!("couldn't download {} ({e})", spec.name))?;
        let mut parts = vec![(
            file_of(dir, id),
            a.member.clone(),
            spec.bytes,
            spec.sha256.clone(),
        )];
        for f in &spec.files {
            parts.push((
                dir.join(format!("{id}.{}", f.name)),
                f.member.clone(),
                f.bytes,
                f.sha256.clone(),
            ));
        }
        let got = archive::unpack(&zip, &parts);
        let _ = std::fs::remove_file(&zip);
        got.map_err(|e| format!("couldn't install {} ({e})", spec.name))?;
        return Ok(spec);
    }
    // Every file, one after the other; progress counts them all.
    let total = spec.total_bytes();
    let mut before = 0;
    let mut parts = vec![(
        file_of(dir, id),
        spec.bytes,
        spec.sha256.clone(),
        spec.urls.clone(),
    )];
    for f in &spec.files {
        parts.push((
            dir.join(format!("{id}.{}", f.name)),
            f.bytes,
            f.sha256.clone(),
            f.urls.clone(),
        ));
    }
    for (path, bytes, sha256, urls) in parts {
        let ok = std::fs::metadata(&path).is_ok_and(|m| m.len() == bytes);
        if !ok {
            download(&path, bytes, &sha256, &urls, &|done| {
                progress(before + done, total)
            })
            .map_err(|e| format!("couldn't download {} ({e})", spec.name))?;
        }
        before += bytes;
    }
    Ok(spec)
}

/// Fetch one file from the first of `urls` that gives the expected one
/// (`bytes` long, with this sha256) into `path`.
pub(super) fn download(
    path: &Path,
    bytes: u64,
    sha256: &str,
    urls: &[String],
    progress: &dyn Fn(u64),
) -> Result<(), String> {
    let part = path.with_extension("part");
    let mut problems = Vec::new();
    for url in urls {
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(std::time::Duration::from_secs(30))
            .timeout_read(std::time::Duration::from_secs(60))
            .try_proxy_from_env(true)
            .build();
        let got = (|| -> Result<(), String> {
            let r = agent.get(url).call().map_err(|e| e.to_string())?;
            let mut src = r.into_reader().take(bytes + 1);
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
                progress(done);
            }
            out.flush().map_err(|e| e.to_string())?;
            if done != bytes || sha256_file(&part)? != sha256 {
                return Err("not the expected file".into());
            }
            std::fs::rename(&part, path).map_err(|e| e.to_string())
        })();
        match got {
            Ok(()) => return Ok(()),
            Err(e) => {
                let _ = std::fs::remove_file(&part);
                problems.push(format!("{url}: {e}"));
            }
        }
    }
    Err(problems.join("; "))
}

/// Delete model `id`'s file (and, for the user's own, its description). Its
/// task goes back to its default.
pub fn remove(dir: &Path, id: &str) -> Result<(), String> {
    let spec = find_spec(dir, id)?;
    let _ = std::fs::remove_file(file_of(dir, id));
    for f in &spec.files {
        let _ = std::fs::remove_file(dir.join(format!("{id}.{}", f.name)));
    }
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
        seconds: 0.0,
        detail: 0,
        files: Vec::new(),
        search: None,
        archive: None,
        input: Input {
            size: m.size,
            mean: m.mean,
            std: m.std,
            layout: m.layout,
            pixels: Pixels::Float,
        },
        output: Output {
            activation: m.activation,
            channel: m.channel,
            invert: m.invert,
        },
        custom: true,
    };
    // It has to run before it's listed.
    let model = load(file, &spec)?;
    let grey = vec![0.5f32; spec.input.size * spec.input.size * 3];
    let t = std::time::Instant::now();
    run(&model, &spec, &grey)?;
    let mut spec = spec;
    spec.seconds = (t.elapsed().as_secs_f64() * 10.0).round() / 10.0;
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
