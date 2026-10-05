//! Flightbroom desktop editor (Tauri 2).
//!
//! The web UI in `ui/` (plain HTML/CSS/JS, no build step) calls these
//! commands; every edit goes through `flb-protocol` exactly like the CLI and
//! AI clients, so the GUI adds no editing logic of its own.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod schema;
mod session;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use flb_presets::Value;
use flb_protocol::Command;
use serde::{Deserialize, Serialize};
use session::{LensSources, PresetEntry, Session};
use tauri::ipc::Response;
use tauri_plugin_dialog::DialogExt;

#[derive(Default)]
struct AppState {
    session: Arc<Mutex<Option<Session>>>,
    lens: Arc<OnceLock<LensSources>>,
    preset_dirs: Arc<Mutex<Vec<PathBuf>>>,
    /// A photo given on the command line, handed to the UI once it asks.
    initial_path: Mutex<Option<String>>,
}

type Res<T> = Result<T, String>;

fn empty_lens() -> &'static LensSources {
    static EMPTY: OnceLock<LensSources> = OnceLock::new();
    EMPTY.get_or_init(LensSources::default)
}

fn to_value(v: &serde_json::Value) -> Option<Value> {
    Some(match v {
        serde_json::Value::Bool(b) => Value::Bool(*b),
        serde_json::Value::Number(n) => Value::Number(n.as_f64()?),
        serde_json::Value::String(s) => Value::Text(s.clone()),
        _ => return None,
    })
}

#[derive(Serialize)]
struct Opened {
    info: session::PhotoInfo,
    state: session::StateInfo,
}

#[tauri::command]
fn take_initial_path(state: tauri::State<'_, AppState>) -> Option<String> {
    state.initial_path.lock().ok()?.take()
}

#[tauri::command]
fn editor_schema() -> Vec<schema::Panel> {
    schema::panels()
}

#[tauri::command]
async fn pick_photo(app: tauri::AppHandle) -> Option<String> {
    app.dialog()
        .file()
        .set_title("Open a raw photo")
        .add_filter(
            "Raw photos",
            &[
                "dng", "nef", "nrw", "cr2", "cr3", "crw", "arw", "srf", "sr2", "raf", "orf", "rw2",
                "pef", "srw", "3fr", "iiq", "erf", "mef", "mos", "kdc", "dcr",
            ],
        )
        .add_filter("All files", &["*"])
        .blocking_pick_file()
        .and_then(|p| p.into_path().ok())
        .map(|p| p.display().to_string())
}

#[tauri::command]
async fn open_photo(path: String, state: tauri::State<'_, AppState>) -> Res<Opened> {
    let slot = state.session.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let s = Session::open(Path::new(&path))?;
        let opened = Opened {
            info: s.info(),
            state: s.state_info(),
        };
        *slot.lock().map_err(|e| e.to_string())? = Some(s);
        Ok(opened)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Serialize)]
struct Executed {
    state: session::StateInfo,
    notes: Vec<String>,
}

#[tauri::command]
fn execute(commands: Vec<Command>, state: tauri::State<'_, AppState>) -> Res<Executed> {
    let mut guard = state.session.lock().map_err(|e| e.to_string())?;
    let s = guard.as_mut().ok_or("no photo open")?;
    let outcomes = s.execute(&commands)?;
    let notes = outcomes
        .iter()
        .flat_map(|o| {
            o.notes
                .iter()
                .cloned()
                .chain(o.clamped.iter().map(|c| c.to_string()))
        })
        .collect();
    Ok(Executed {
        state: s.state_info(),
        notes,
    })
}

#[derive(Serialize)]
struct PreviewMeta {
    width: usize,
    height: usize,
    histogram: [Vec<u32>; 3],
    millis: u128,
    lens_profile: Option<String>,
}

/// Preview JPEG with a JSON header: `[u32 LE header length][header JSON][JPEG]`.
#[tauri::command]
async fn preview(
    max_edge: usize,
    overrides: BTreeMap<String, serde_json::Value>,
    before: bool,
    state: tauri::State<'_, AppState>,
) -> Res<Response> {
    let slot = state.session.clone();
    let lens = state.lens.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let overrides: BTreeMap<String, Value> = overrides
            .iter()
            .filter_map(|(k, v)| Some((k.clone(), to_value(v)?)))
            .collect();
        let mut guard = slot.lock().map_err(|e| e.to_string())?;
        let s = guard.as_mut().ok_or("no photo open")?;
        let lens = lens.get().unwrap_or_else(|| empty_lens());
        let p = s.preview(max_edge, &overrides, before, lens);
        let meta = serde_json::to_vec(&PreviewMeta {
            width: p.width,
            height: p.height,
            histogram: p.histogram,
            millis: p.millis,
            lens_profile: p.lens_profile,
        })
        .map_err(|e| e.to_string())?;
        let mut out = Vec::with_capacity(4 + meta.len() + p.jpeg.len());
        out.extend_from_slice(&(meta.len() as u32).to_le_bytes());
        out.extend_from_slice(&meta);
        out.extend_from_slice(&p.jpeg);
        Ok(Response::new(out))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn save_sidecar(state: tauri::State<'_, AppState>) -> Res<String> {
    let mut guard = state.session.lock().map_err(|e| e.to_string())?;
    guard.as_mut().ok_or("no photo open")?.save_sidecar()
}

#[derive(Deserialize)]
struct ExportOptions {
    format: String,
    quality: u8,
    bits: Option<u32>,
    color_space: String,
    long_edge: Option<u32>,
    sharpen: Option<String>,
    metadata: String,
    max_kb: Option<u64>,
}

#[derive(Serialize)]
struct Exported {
    path: String,
    width: usize,
    height: usize,
    bytes: u64,
    quality: u8,
}

#[tauri::command]
async fn pick_export_path(
    app: tauri::AppHandle,
    suggested: String,
    format: String,
) -> Option<String> {
    let ext = flb_export::Format::from_name(&format)?.extension();
    app.dialog()
        .file()
        .set_title("Export")
        .set_file_name(format!("{suggested}.{ext}"))
        .add_filter(format.to_uppercase(), &[ext])
        .blocking_save_file()
        .and_then(|p| p.into_path().ok())
        .map(|p| p.display().to_string())
}

#[tauri::command]
async fn export(
    path: String,
    options: ExportOptions,
    state: tauri::State<'_, AppState>,
) -> Res<Exported> {
    let slot = state.session.clone();
    let lens = state.lens.clone();
    tauri::async_runtime::spawn_blocking(move || {
        use flb_export::{BitDepth, ExportSettings, Format, MetadataPolicy, Resize};
        let format = Format::from_name(&options.format)
            .ok_or(format!("unknown format {}", options.format))?;
        let mut s = ExportSettings::new(format);
        s.quality = options.quality.clamp(1, 100);
        s.bit_depth = options.bits.and_then(BitDepth::from_bits);
        s.color_space = flb_color::space::OutputSpace::from_id(&options.color_space)
            .ok_or(format!("unknown colour space {}", options.color_space))?;
        if let Some(edge) = options.long_edge.filter(|e| *e > 0) {
            s.resize = Resize::LongEdge(edge);
        }
        s.sharpen = options
            .sharpen
            .as_deref()
            .and_then(flb_export::parse_sharpen);
        s.metadata =
            MetadataPolicy::from_id(&options.metadata).unwrap_or(MetadataPolicy::AllExceptLocation);
        s.max_bytes = options
            .max_kb
            .filter(|k| *k > 0 && format == Format::Jpeg)
            .map(|k| k * 1000);
        let mut guard = slot.lock().map_err(|e| e.to_string())?;
        let session = guard.as_mut().ok_or("no photo open")?;
        let lens = lens.get().unwrap_or_else(|| empty_lens());
        let r = session.export(Path::new(&path), &s, lens)?;
        Ok(Exported {
            path,
            width: r.width,
            height: r.height,
            bytes: r.bytes,
            quality: r.quality,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn presets(state: tauri::State<'_, AppState>) -> Res<Vec<PresetEntry>> {
    let dirs = state.preset_dirs.lock().map_err(|e| e.to_string())?.clone();
    tauri::async_runtime::spawn_blocking(move || session::scan_presets(&dirs))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn add_preset_folder(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Res<Vec<PresetEntry>> {
    if let Some(dir) = app
        .dialog()
        .file()
        .set_title("Add a preset folder")
        .blocking_pick_folder()
        .and_then(|p| p.into_path().ok())
    {
        let mut dirs = state.preset_dirs.lock().map_err(|e| e.to_string())?;
        if !dirs.contains(&dir) {
            dirs.push(dir);
        }
    }
    presets(state).await
}

fn main() {
    let state = AppState::default();
    *state.preset_dirs.lock().expect("fresh mutex") = session::default_preset_dirs();
    // Lens profile sources can take a moment (thousands of files): load them in
    // the background; previews use them once ready.
    let lens = state.lens.clone();
    std::thread::spawn(move || {
        let _ = lens.set(LensSources::load_default());
    });
    *state.initial_path.lock().expect("fresh mutex") = std::env::args().nth(1);
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            take_initial_path,
            editor_schema,
            pick_photo,
            open_photo,
            execute,
            preview,
            save_sidecar,
            pick_export_path,
            export,
            presets,
            add_preset_folder,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Flightbroom");
}
