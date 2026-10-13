// SPDX-License-Identifier: GPL-3.0-or-later
//! Ambrotype desktop editor (Tauri 2).
//!
//! The web UI in `ui/` (plain HTML/CSS/JS, no build step) calls these
//! commands; every edit goes through `amb-protocol` exactly like the CLI and
//! AI clients, so the GUI adds no editing logic of its own.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod assistant;
mod borders;
mod collages;
mod folder;
mod hub;
mod keys;
mod library;
mod looks;
mod plugins;
mod schema;
mod session;
mod sizes;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use amb_presets::Value;
use amb_protocol::Command;
use serde::{Deserialize, Serialize};
use session::{CropRect, CropState, LensSources, PresetEntry, Session};
use sizes::SizePreset;
use tauri::Manager;
use tauri::ipc::Response;
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

#[derive(Default)]
struct AppState {
    session: Arc<Mutex<Option<Session>>>,
    lens: Arc<LensCell>,
    /// The GPU for previews, once set up (`None` inside: there is none).
    gpu: Arc<OnceLock<Option<amb_gpu::Gpu>>>,
    preset_dirs: Arc<Mutex<Vec<PathBuf>>>,
    /// The user's own size presets (the built-ins are added when listing).
    size_presets: Mutex<Vec<SizePreset>>,
    /// A photo given on the command line, handed to the UI once it asks.
    initial_path: Mutex<Option<String>>,
    /// Where `Config` is saved (the app's config folder), once known.
    config_path: Mutex<Option<PathBuf>>,
    /// The active layout and theme ids, as chosen in the Looks dialog.
    look: Mutex<(Option<String>, Option<String>)>,
    keybindings: Mutex<keys::Bindings>,
    hub_url: Mutex<Option<String>>,
    border_presets: Mutex<Vec<borders::BorderPreset>>,
    recent_keywords: Mutex<Vec<String>>,
    collage_templates: Mutex<Vec<collages::TemplateEntry>>,
    plugin_approvals: Mutex<plugins::Approvals>,
    plugin_recipes: Mutex<PluginRecipes>,
    import_choice: Mutex<Option<ImportChoice>>,
    export_presets: Mutex<Vec<ExportPreset>>,
    /// The presets starred as favourites, by path.
    preset_favourites: Mutex<Vec<String>>,
    mask_effects: Mutex<Vec<MaskEffect>>,
    edit_in: Mutex<EditIn>,
    /// Watches the folders on screen for photos added, removed or renamed.
    folder_watch: Mutex<Option<notify::RecommendedWatcher>>,
    /// Watches the open photo's sidecar for changes made outside the editor.
    sidecar_watch: Mutex<Option<notify::RecommendedWatcher>>,
    assistant: Mutex<assistant::Provider>,
    /// The assistant's key for this run, when the keychain can't keep it.
    assistant_key: Mutex<Option<String>>,
}

/// The import dialog's last destination and choices.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ImportChoice {
    dest: String,
    options: amb_library::import::ImportOptions,
}

/// Recipes the user saved, per plugin id.
type PluginRecipes = std::collections::BTreeMap<String, Vec<amb_plugin::Recipe>>;

/// Settings kept between runs.
#[derive(Default, Serialize, Deserialize)]
struct Config {
    #[serde(default)]
    preset_dirs: Vec<PathBuf>,
    #[serde(default)]
    size_presets: Vec<SizePreset>,
    /// The active layout and theme (look ids).
    #[serde(default)]
    layout: Option<String>,
    #[serde(default)]
    theme: Option<String>,
    /// Keyboard shortcuts the user changed (action id -> keys).
    #[serde(default)]
    keybindings: keys::Bindings,
    /// Another hub than the default (an index.json address).
    #[serde(default)]
    hub_url: Option<String>,
    /// The user's border presets (the built-ins are added when listing).
    #[serde(default)]
    border_presets: Vec<borders::BorderPreset>,
    /// Keywords used lately, newest first (for suggestions).
    #[serde(default)]
    recent_keywords: Vec<String>,
    /// The user's collage templates (the built-ins are added when listing).
    #[serde(default)]
    collage_templates: Vec<collages::TemplateEntry>,
    /// Plugins whose permissions the user allowed: id -> the fingerprint allowed.
    #[serde(default)]
    plugin_approvals: plugins::Approvals,
    /// Plugin recipes the user saved (plugin id -> recipes).
    #[serde(default)]
    plugin_recipes: PluginRecipes,
    /// The import dialog's last destination and choices.
    #[serde(default)]
    import: Option<ImportChoice>,
    /// The assistant's provider and model (its key is in the keychain).
    #[serde(default)]
    assistant: assistant::Provider,
    #[serde(default)]
    export_presets: Vec<ExportPreset>,
    #[serde(default)]
    preset_favourites: Vec<String>,
    /// The user's own mask effects (the built-ins are added when listing).
    #[serde(default)]
    mask_effects: Vec<MaskEffect>,
    /// Edit in another app: which app and what file.
    #[serde(default)]
    edit_in: EditIn,
}

/// Lightroom's "Edit in": the photo, as edited, goes to another app (an
/// image editor) as a TIFF next to it, `IMG_0001-Edit.tif`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct EditIn {
    /// The app (a program, or a `.app` on macOS); none: the system's
    /// default for TIFFs.
    #[serde(default)]
    app: Option<String>,
    #[serde(default = "prophoto")]
    color_space: String,
    #[serde(default = "sixteen")]
    bits: u32,
}

fn prophoto() -> String {
    "prophoto".into()
}

fn sixteen() -> u32 {
    16
}

impl Default for EditIn {
    fn default() -> EditIn {
        EditIn {
            app: None,
            color_space: prophoto(),
            bits: sixteen(),
        }
    }
}

/// A mask's sliders saved by name (the Masks panel's Effect menu), in the
/// panel's units: Exposure in stops, the rest -100..100.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct MaskEffect {
    name: String,
    values: BTreeMap<String, f64>,
}

const BUILTIN_MASK_EFFECTS: &str = include_str!("../data/mask-effects.json");

fn save_config(state: &AppState) {
    let Ok(path) = state.config_path.lock() else {
        return;
    };
    let Some(path) = path.as_ref() else {
        return;
    };
    let dirs = state
        .preset_dirs
        .lock()
        .map(|d| d.clone())
        .unwrap_or_default();
    // Lightroom's own folders are found automatically; store only added ones.
    let defaults = session::default_preset_dirs();
    let config = Config {
        preset_dirs: dirs.into_iter().filter(|d| !defaults.contains(d)).collect(),
        size_presets: state
            .size_presets
            .lock()
            .map(|p| p.clone())
            .unwrap_or_default(),
        layout: state.look.lock().ok().and_then(|l| l.0.clone()),
        theme: state.look.lock().ok().and_then(|l| l.1.clone()),
        keybindings: state
            .keybindings
            .lock()
            .map(|k| k.clone())
            .unwrap_or_default(),
        hub_url: state.hub_url.lock().ok().and_then(|h| h.clone()),
        border_presets: state
            .border_presets
            .lock()
            .map(|b| b.clone())
            .unwrap_or_default(),
        recent_keywords: state
            .recent_keywords
            .lock()
            .map(|k| k.clone())
            .unwrap_or_default(),
        collage_templates: state
            .collage_templates
            .lock()
            .map(|t| t.clone())
            .unwrap_or_default(),
        plugin_approvals: state
            .plugin_approvals
            .lock()
            .map(|a| a.clone())
            .unwrap_or_default(),
        plugin_recipes: state
            .plugin_recipes
            .lock()
            .map(|r| r.clone())
            .unwrap_or_default(),
        import: state.import_choice.lock().ok().and_then(|c| c.clone()),
        assistant: state
            .assistant
            .lock()
            .map(|a| a.clone())
            .unwrap_or_default(),
        export_presets: state
            .export_presets
            .lock()
            .map(|p| p.clone())
            .unwrap_or_default(),
        mask_effects: state
            .mask_effects
            .lock()
            .map(|p| p.clone())
            .unwrap_or_default(),
        preset_favourites: state
            .preset_favourites
            .lock()
            .map(|p| p.clone())
            .unwrap_or_default(),
        edit_in: state.edit_in.lock().map(|e| e.clone()).unwrap_or_default(),
    };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(text) = serde_json::to_string_pretty(&config) {
        let _ = std::fs::write(path, text);
    }
}

type Res<T> = Result<T, String>;

/// The lens database and profiles, loaded in the background at start and
/// loaded again after `amb lens fetch` (Download lens database). A
/// replaced one stays in memory until the app closes, so a render that
/// started with it can finish; that happens once per download.
#[derive(Default)]
struct LensCell(Mutex<Option<&'static LensSources>>);

impl LensCell {
    fn get(&self) -> Option<&'static LensSources> {
        *self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn set(&self, sources: LensSources) -> Result<(), ()> {
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) = Some(Box::leak(Box::new(sources)));
        Ok(())
    }
}

fn empty_lens() -> &'static LensSources {
    static EMPTY: OnceLock<LensSources> = OnceLock::new();
    EMPTY.get_or_init(LensSources::default)
}

fn to_value(v: &serde_json::Value) -> Option<Value> {
    Some(match v {
        serde_json::Value::Bool(b) => Value::Bool(*b),
        serde_json::Value::Number(n) => Value::Number(n.as_f64()?),
        serde_json::Value::String(s) => Value::Text(s.clone()),
        // Curves: [[x, y], ...] → the XMP list form `curve_points()` reads.
        serde_json::Value::Array(items) => Value::Seq(
            items
                .iter()
                .map(|i| match i {
                    serde_json::Value::Array(p) if p.len() == 2 => Some(Value::Text(format!(
                        "{}, {}",
                        p[0].as_f64()?,
                        p[1].as_f64()?
                    ))),
                    other => to_value(other),
                })
                .collect::<Option<Vec<_>>>()?,
        ),
        _ => return None,
    })
}

#[derive(Serialize)]
struct Opened {
    info: session::PhotoInfo,
    state: session::StateInfo,
    /// The sidecar the previous photo's unsaved edits were saved to, if any.
    saved_previous: Option<String>,
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
        .set_title("Open a photo")
        .add_filter("Photos", &folder::photo_extensions())
        .add_filter("All files", &["*"])
        .blocking_pick_file()
        .and_then(|p| p.into_path().ok())
        .map(|p| p.display().to_string())
}

/// Pick several photos (for the collage maker).
#[tauri::command]
async fn pick_photos(app: tauri::AppHandle) -> Res<Vec<folder::FolderPhoto>> {
    let paths: Vec<PathBuf> = app
        .dialog()
        .file()
        .set_title("Add photos")
        .add_filter("Photos", &folder::photo_extensions())
        .blocking_pick_files()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|p| p.into_path().ok())
        .collect();
    tauri::async_runtime::spawn_blocking(move || folder::describe(paths))
        .await
        .map_err(|e| e.to_string())
}

/// Pick reference photos for Match look.
#[tauri::command]
async fn pick_references(app: tauri::AppHandle) -> Vec<String> {
    app.dialog()
        .file()
        .set_title("Reference photos: the look to match")
        .add_filter("Photos", &folder::photo_extensions())
        .blocking_pick_files()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|p| p.into_path().ok())
        .map(|p| p.display().to_string())
        .collect()
}

#[derive(Serialize)]
struct MatchLine {
    name: String,
    label: String,
    before: String,
    after: String,
}

/// What Match look proposes: the commands, the same as readable lines,
/// and how close it gets.
#[derive(Serialize)]
struct MatchProposal {
    commands: Vec<Command>,
    lines: Vec<MatchLine>,
    before: f64,
    after: f64,
    renders: usize,
}

/// Auto for the open photo: the commands for its tone and/or white balance.
#[tauri::command]
async fn auto_settings(
    tone: bool,
    white: bool,
    state: tauri::State<'_, AppState>,
) -> Res<Vec<Command>> {
    let slot = state.session.clone();
    let lens = state.lens.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut guard = slot.lock().map_err(|e| e.to_string())?;
        let s = guard.as_mut().ok_or("no photo open")?;
        let lens = lens.get().unwrap_or_else(|| empty_lens());
        Ok(amb_library::look::commands(
            &s.auto(tone, white, lens).changes,
        ))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Find the settings that make the open photo look like `references`.
#[tauri::command]
async fn match_look(
    references: Vec<String>,
    parts: Vec<String>,
    state: tauri::State<'_, AppState>,
) -> Res<MatchProposal> {
    use amb_engine::matching::Part;
    let slot = state.session.clone();
    let lens = state.lens.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let parts = parts
            .iter()
            .map(|p| Part::parse(p).ok_or_else(|| format!("unknown part {p:?}")))
            .collect::<Result<Vec<_>, _>>()?;
        let paths: Vec<&Path> = references.iter().map(Path::new).collect();
        // The references are measured before the photo is locked.
        let target = amb_library::look::target_look(&paths)?;
        let mut guard = slot.lock().map_err(|e| e.to_string())?;
        let s = guard.as_mut().ok_or("no photo open")?;
        let lens = lens.get().unwrap_or_else(|| empty_lens());
        let result = s.match_look(&target, parts, lens);
        let current = s.state.settings();
        let shot = s.info();
        let catalog = amb_protocol::catalog::Catalog::builtin();
        let registry = amb_presets::registry::Registry::builtin();
        let lines = result
            .changes
            .iter()
            .filter(|(name, _)| name != "WhiteBalance")
            .map(|(name, v)| {
                // Unset white balance is the camera's.
                let as_shot = match name.as_str() {
                    "Temperature" => Some(shot.as_shot_temperature.round()),
                    "Tint" => Some(shot.as_shot_tint.round()),
                    _ => None,
                };
                let old = current.get(name).cloned().or_else(|| {
                    as_shot
                        .or_else(|| registry.numeric_default(name))
                        .map(Value::Number)
                });
                MatchLine {
                    name: name.clone(),
                    label: catalog
                        .text(name)
                        .map_or_else(|| name.clone(), |t| t.label.clone()),
                    before: amb_protocol::convert::display(name, old.as_ref()),
                    after: amb_protocol::convert::display(name, Some(v)),
                }
            })
            .collect();
        Ok(MatchProposal {
            commands: amb_library::look::commands(&result.changes),
            lines,
            before: result.before,
            after: result.after,
            renders: result.evaluations,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Serialize)]
struct AssistantSettings {
    provider: assistant::Provider,
    has_key: bool,
}

/// The assistant's provider and model, and whether a key is saved.
#[tauri::command]
async fn assistant_settings(state: tauri::State<'_, AppState>) -> Res<AssistantSettings> {
    let provider = state.assistant.lock().map_err(|e| e.to_string())?.clone();
    let session_key = state
        .assistant_key
        .lock()
        .map_err(|e| e.to_string())?
        .is_some();
    let p = provider.clone();
    let saved = tauri::async_runtime::spawn_blocking(move || assistant::load_key(&p).is_some())
        .await
        .map_err(|e| e.to_string())?;
    Ok(AssistantSettings {
        provider,
        has_key: saved || session_key,
    })
}

/// Save the assistant's provider and model; a `key` goes to the keychain
/// ("" removes it). Returns a note when the keychain couldn't keep the key.
#[tauri::command]
async fn assistant_save(
    provider: assistant::Provider,
    key: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Res<Option<String>> {
    *state.assistant.lock().map_err(|e| e.to_string())? = provider.clone();
    save_config(&state);
    let Some(key) = key.map(|k| k.trim().to_string()) else {
        return Ok(None);
    };
    let p = provider.clone();
    let k = key.clone();
    let stored = tauri::async_runtime::spawn_blocking(move || assistant::store_key(&p, &k))
        .await
        .map_err(|e| e.to_string())?;
    let mut session_key = state.assistant_key.lock().map_err(|e| e.to_string())?;
    match stored {
        Ok(()) => {
            *session_key = None;
            Ok(None)
        }
        Err(e) => {
            *session_key = (!key.is_empty()).then_some(key);
            Ok(Some(format!(
                "The keychain couldn't keep the key ({e}); it is used until Ambrotype closes."
            )))
        }
    }
}

#[derive(Serialize)]
struct AssistantAnswer {
    #[serde(flatten)]
    outcome: assistant::Outcome,
    state: session::StateInfo,
}

/// One request to the assistant about the open photo; its edits are applied
/// as ordinary commands, one undo step per `amb:` line.
#[tauri::command]
async fn assistant_ask(
    history: Vec<assistant::Message>,
    request: String,
    see: Option<bool>,
    state: tauri::State<'_, AppState>,
) -> Res<AssistantAnswer> {
    let provider = state.assistant.lock().map_err(|e| e.to_string())?.clone();
    let session_key = state
        .assistant_key
        .lock()
        .map_err(|e| e.to_string())?
        .clone();
    let slot = state.session.clone();
    let lens = state.lens.clone();
    let log = assistant_log_path(&state);
    tauri::async_runtime::spawn_blocking(move || {
        let key = session_key.or_else(|| assistant::load_key(&provider));
        let lens = lens.get().unwrap_or_else(|| empty_lens());
        let photo = slot
            .lock()
            .map_err(|e| e.to_string())?
            .as_ref()
            .ok_or("no photo open")?
            .path
            .clone();
        // The photo is locked only to look and to edit, not while the
        // model thinks; a request ends if another photo was opened.
        let with_photo = |f: &mut dyn FnMut(&mut Session) -> Res<String>| -> Res<String> {
            let mut guard = slot.lock().map_err(|e| e.to_string())?;
            match guard.as_mut() {
                Some(s) if s.path == photo => f(s),
                _ => Err("another photo was opened; the request stopped".into()),
            }
        };
        let system = assistant::system_prompt();
        // The photo as edited now, small, when the user lets the model see it.
        let mut picture = || {
            with_photo(&mut |s| {
                let p = s.preview(512, &BTreeMap::new(), false, false, lens);
                Ok(plugins::base64(&p.jpeg))
            })
        };
        let outcome = assistant::converse(
            |messages| assistant::call(&provider, key.as_deref(), &system, messages),
            history,
            &request,
            || with_photo(&mut |s| Ok(s.assistant_context(lens))),
            see.unwrap_or(false)
                .then_some(&mut picture as &mut dyn FnMut() -> Res<String>),
            |items| {
                // Every edit the model asks for is logged, done or not.
                let logged = |result: &Res<String>| {
                    log_assistant(
                        log.as_deref(),
                        serde_json::json!({
                            "photo": photo.display().to_string(),
                            "request": request,
                            "edits": items,
                            "result": match result { Ok(r) => r.clone(), Err(e) => format!("error: {e}") },
                        }),
                    );
                };
                let commands = match amb_protocol::compact::parse(items).map_err(|e| e.to_string()) {
                    Ok(c) => c,
                    Err(e) => {
                        logged(&Err(e.clone()));
                        return Err(e);
                    }
                };
                let done = with_photo(&mut |s| {
                    let all = s.white_balance_from_as_shot(commands.clone());
                    let mut results = s.execute(&all, Some("Assistant"))?;
                    // The as-shot start was ours: its notes would only confuse.
                    for o in results.iter_mut().take(all.len() - commands.len()) {
                        o.notes.clear();
                    }
                    let report = amb_protocol::edit::EditReport {
                        sidecar: s.sidecar.clone(),
                        created: false,
                        results,
                        written: None,
                        dry_run: false,
                    };
                    Ok(report.to_brief())
                });
                logged(&done);
                done
            },
        )?;
        let guard = slot.lock().map_err(|e| e.to_string())?;
        let s = guard.as_ref().ok_or("no photo open")?;
        Ok(AssistantAnswer {
            outcome,
            state: s.state_info(),
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The assistant's log: one JSON line per edit it asked for (the photo,
/// the request, the edits in the compact syntax, and what happened), in
/// the app's settings folder, so the user can see what the model did.
fn assistant_log_path(state: &AppState) -> Option<PathBuf> {
    let path = state.config_path.lock().ok()?;
    Some(path.as_ref()?.parent()?.join("assistant-log.jsonl"))
}

fn log_assistant(path: Option<&Path>, mut entry: serde_json::Value) {
    let Some(path) = path else { return };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    entry["time"] = serde_json::json!(now);
    let line = format!("{entry}\n");
    use std::io::Write;
    let _ = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .and_then(|mut f| f.write_all(line.as_bytes()));
}

/// Show the assistant's log in the file manager's default app.
#[tauri::command]
fn open_assistant_log(app: tauri::AppHandle, state: tauri::State<'_, AppState>) -> Res<()> {
    let path = assistant_log_path(&state).ok_or("no settings folder")?;
    if !path.is_file() {
        return Err("the assistant hasn't made any edits yet".into());
    }
    app.opener()
        .open_path(path.display().to_string(), None::<&str>)
        .map_err(|e| e.to_string())
}

/// Pick a folder and list its raws (for the collage maker).
#[tauri::command]
async fn pick_photo_folder(app: tauri::AppHandle) -> Res<Option<Vec<folder::FolderPhoto>>> {
    let Some(dir) = app
        .dialog()
        .file()
        .set_title("Add a folder of photos")
        .blocking_pick_folder()
        .and_then(|p| p.into_path().ok())
    else {
        return Ok(None);
    };
    tauri::async_runtime::spawn_blocking(move || folder::list_dir(&dir).map(Some))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn open_photo(
    path: String,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Res<Opened> {
    let slot = state.session.clone();
    let opened = tauri::async_runtime::spawn_blocking(move || {
        let s = Session::open(Path::new(&path))?;
        let sidecar = s.sidecar.clone();
        let mut guard = slot.lock().map_err(|e| e.to_string())?;
        // Moving to another photo keeps the edits: save them first, like
        // Lightroom writing its sidecars as you go.
        let saved_previous = match guard.as_mut() {
            Some(prev) if prev.has_unsaved_edits() => Some(prev.save_sidecar()?),
            _ => None,
        };
        let opened = Opened {
            info: s.info(),
            state: s.state_info(),
            saved_previous,
        };
        *guard = Some(s);
        Ok::<_, String>((opened, sidecar))
    })
    .await
    .map_err(|e| e.to_string())??;
    let (opened, sidecar) = opened;
    *state.sidecar_watch.lock().map_err(|e| e.to_string())? = watch_sidecar(app, &sidecar);
    Ok(opened)
}

/// Tell the page ("sidecar-changed") when the open photo's sidecar changes
/// on disk; [`reload_sidecar`] then tells our own saves from other writers.
fn watch_sidecar(app: tauri::AppHandle, sidecar: &Path) -> Option<notify::RecommendedWatcher> {
    use notify::{EventKind, RecursiveMode, Watcher};
    use std::sync::atomic::{AtomicBool, Ordering};
    let dir = sidecar.parent()?.to_path_buf();
    let target = sidecar.file_name()?.to_os_string();
    let pending = Arc::new(AtomicBool::new(false));
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        let Ok(event) = event else { return };
        let ours = event
            .paths
            .iter()
            .any(|p| p.file_name() == Some(target.as_os_str()));
        let written = matches!(
            event.kind,
            EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
        );
        if ours && written && !pending.swap(true, Ordering::SeqCst) {
            let (app, pending) = (app.clone(), pending.clone());
            std::thread::spawn(move || {
                // Let the writer finish.
                std::thread::sleep(std::time::Duration::from_millis(300));
                pending.store(false, Ordering::SeqCst);
                use tauri::Emitter;
                let _ = app.emit("sidecar-changed", ());
            });
        }
    })
    .ok()?;
    watcher.watch(&dir, RecursiveMode::NonRecursive).ok()?;
    Some(watcher)
}

#[derive(Serialize)]
struct Reloaded {
    state: session::StateInfo,
    /// Develop settings that changed.
    changes: usize,
}

/// Take the sidecar's settings, rating and keywords if something else
/// changed it since we read or wrote it. `None` when it is as we left it.
#[tauri::command]
fn reload_sidecar(state: tauri::State<'_, AppState>) -> Res<Option<Reloaded>> {
    let mut guard = state.session.lock().map_err(|e| e.to_string())?;
    let Some(s) = guard.as_mut() else {
        return Ok(None);
    };
    Ok(s.reload_sidecar()?.map(|changes| Reloaded {
        state: s.state_info(),
        changes,
    }))
}

#[derive(Serialize)]
struct Executed {
    state: session::StateInfo,
    notes: Vec<String>,
}

#[tauri::command]
fn execute(
    commands: Vec<Command>,
    group: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Res<Executed> {
    let mut guard = state.session.lock().map_err(|e| e.to_string())?;
    let s = guard.as_mut().ok_or("no photo open")?;
    let outcomes = s.execute(&commands, group.as_deref())?;
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

/// A border caption as the open photo's export would set it, and the tokens.
#[tauri::command]
fn caption_preview(
    template: String,
    state: tauri::State<'_, AppState>,
) -> Res<(String, Vec<(String, String)>)> {
    let guard = state.session.lock().map_err(|e| e.to_string())?;
    let text = guard
        .as_ref()
        .map(|s| s.caption_text(&template))
        .unwrap_or_default();
    let tokens = amb_export::border::CAPTION_TOKENS
        .iter()
        .map(|(t, d)| (t.to_string(), d.to_string()))
        .collect();
    Ok((text, tokens))
}

/// The committed crop, upright on the uncropped photo (the crop tool's start).
#[tauri::command]
fn crop_state(state: tauri::State<'_, AppState>) -> Res<CropState> {
    let guard = state.session.lock().map_err(|e| e.to_string())?;
    Ok(guard.as_ref().ok_or("no photo open")?.crop_state())
}

/// A donor for a new spot (the spot tool shows it, and the user can move it).
#[tauri::command]
async fn spot_source(
    x: f64,
    y: f64,
    radius: f64,
    state: tauri::State<'_, AppState>,
) -> Res<[f64; 2]> {
    let slot = state.session.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let guard = slot.lock().map_err(|e| e.to_string())?;
        let (sx, sy) = guard
            .as_ref()
            .ok_or("no photo open")?
            .spot_source(x, y, radius);
        Ok([sx, sy])
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The mask tool's map from the shown photo to mask coordinates.
#[tauri::command]
fn mask_frame(state: tauri::State<'_, AppState>) -> Res<session::MaskFrame> {
    let guard = state.session.lock().map_err(|e| e.to_string())?;
    Ok(guard.as_ref().ok_or("no photo open")?.mask_frame())
}

#[derive(Serialize)]
struct MaskSlider {
    tag: &'static str,
    label: &'static str,
    min: f64,
    max: f64,
    step: f64,
    section: &'static str,
    rendered: bool,
}

/// The masks' local sliders, in panel order (UI units).
#[tauri::command]
fn mask_sliders() -> Vec<MaskSlider> {
    amb_presets::masks::LOCAL_SLIDERS
        .iter()
        .map(|s| MaskSlider {
            tag: s.tag,
            label: s.label,
            min: s.min,
            max: s.max,
            step: s.step,
            section: s.section,
            rendered: s.rendered,
        })
        .collect()
}

/// The protocol commands that set the crop to `rect`; the UI runs them as one
/// undo step.
#[tauri::command]
fn crop_commands(rect: CropRect, state: tauri::State<'_, AppState>) -> Res<Vec<Command>> {
    let guard = state.session.lock().map_err(|e| e.to_string())?;
    Ok(guard.as_ref().ok_or("no photo open")?.crop_commands(&rect))
}

#[derive(Serialize)]
struct PreviewMeta {
    width: usize,
    height: usize,
    histogram: [Vec<u32>; 3],
    millis: u128,
    lens_profile: Option<String>,
    /// The GPU's name, or why this preview was rendered on the CPU.
    gpu: Option<String>,
    cpu_reason: Option<String>,
}

/// Preview JPEG with a JSON header: `[u32 LE header length][header JSON][JPEG]`.
#[tauri::command]
async fn preview(
    max_edge: usize,
    overrides: BTreeMap<String, serde_json::Value>,
    before: bool,
    clipping: bool,
    mask_overlay: Option<usize>,
    state: tauri::State<'_, AppState>,
) -> Res<Response> {
    let slot = state.session.clone();
    let lens = state.lens.clone();
    let gpu = state.gpu.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let overrides: BTreeMap<String, Value> = overrides
            .iter()
            .filter_map(|(k, v)| Some((k.clone(), to_value(v)?)))
            .collect();
        let mut guard = slot.lock().map_err(|e| e.to_string())?;
        let s = guard.as_mut().ok_or("no photo open")?;
        let lens = lens.get().unwrap_or_else(|| empty_lens());
        let gpu = gpu.get().and_then(Option::as_ref);
        let p = s.preview_masked(
            max_edge,
            &overrides,
            before,
            clipping,
            lens,
            mask_overlay,
            gpu,
        );
        let meta = serde_json::to_vec(&PreviewMeta {
            width: p.width,
            height: p.height,
            histogram: p.histogram,
            millis: p.millis,
            lens_profile: p.lens_profile,
            gpu: p.gpu.as_ref().and_then(|g| g.as_ref().ok().cloned()),
            cpu_reason: p.gpu.and_then(Result::err),
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

#[derive(Serialize)]
struct RegionMeta {
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    full_width: usize,
    full_height: usize,
    millis: u128,
}

/// A reduced 1:1 crop with uncommitted slider changes, for the 100% view
/// while a slider moves (framed like `region`).
#[tauri::command]
async fn region_draft(
    cx: f64,
    cy: f64,
    width: usize,
    height: usize,
    overrides: BTreeMap<String, serde_json::Value>,
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
        let r = s.region_draft(cx, cy, width, height, &overrides, lens);
        framed_region(&r)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// A region as the page reads it: the length of its JSON facts, the facts,
/// then the JPEG.
fn framed_region(r: &session::Region) -> Res<Response> {
    let meta = serde_json::to_vec(&RegionMeta {
        x: r.x,
        y: r.y,
        width: r.width,
        height: r.height,
        full_width: r.full_width,
        full_height: r.full_height,
        millis: r.millis,
    })
    .map_err(|e| e.to_string())?;
    let mut out = Vec::with_capacity(4 + meta.len() + r.jpeg.len());
    out.extend_from_slice(&(meta.len() as u32).to_le_bytes());
    out.extend_from_slice(&meta);
    out.extend_from_slice(&r.jpeg);
    Ok(Response::new(out))
}

/// 1:1 crop for the 100% view, framed like `preview`: `[u32 LE len][JSON][JPEG]`.
#[tauri::command]
async fn region(
    cx: f64,
    cy: f64,
    width: usize,
    height: usize,
    before: bool,
    clipping: bool,
    state: tauri::State<'_, AppState>,
) -> Res<Response> {
    let slot = state.session.clone();
    let lens = state.lens.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut guard = slot.lock().map_err(|e| e.to_string())?;
        let s = guard.as_mut().ok_or("no photo open")?;
        let lens = lens.get().unwrap_or_else(|| empty_lens());
        let r = s.region(cx, cy, width, height, before, clipping, lens);
        framed_region(&r)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// White balance from a clicked neutral spot: sets WB to Custom with the
/// picked Temperature/Tint (one undoable step).
#[tauri::command]
fn pick_white(u: f64, v: f64, state: tauri::State<'_, AppState>) -> Res<Executed> {
    let mut guard = state.session.lock().map_err(|e| e.to_string())?;
    let s = guard.as_mut().ok_or("no photo open")?;
    let [(temp_name, t), (tint_name, tint)] = s
        .pick_white(u, v)
        .ok_or("pick a neutral grey or white area that is not clipped")?;
    s.execute(
        &[
            Command::SetParam {
                name: "WhiteBalance".into(),
                value: serde_json::json!("Custom"),
            },
            Command::SetParam {
                name: temp_name.into(),
                value: serde_json::json!(t),
            },
            Command::SetParam {
                name: tint_name.into(),
                value: serde_json::json!(tint),
            },
        ],
        Some("white balance picker"),
    )?;
    let note = if temp_name == "Temperature" {
        format!("White balance set to {t:.0} K, tint {tint:+.0}")
    } else {
        format!("White balance set to temp {t:+.0}, tint {tint:+.0}")
    };
    Ok(Executed {
        state: s.state_info(),
        notes: vec![note],
    })
}

/// Where feedback goes: a pre-filled GitHub issue (nothing is sent until the
/// user submits it there).
const FEEDBACK_URL: &str = "https://github.com/shardpelt/Ambrotype/issues/new";

fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// The feedback text that would be sent (shown to the user before opening).
#[tauri::command]
fn feedback_body(
    kind: String,
    text: String,
    include_context: bool,
    state: tauri::State<'_, AppState>,
) -> String {
    let mut body = format!("**Kind:** {kind}\n\n{}\n", text.trim());
    if include_context {
        body.push_str("\n**Context** (added by Ambrotype; no photo is attached)\n");
        match state.session.lock().ok().as_ref().and_then(|g| g.as_ref()) {
            Some(s) => body.push_str(&s.feedback_context(true)),
            None => body.push_str(&format!(
                "- Ambrotype {} on {} ({}), no photo open\n",
                env!("CARGO_PKG_VERSION"),
                std::env::consts::OS,
                std::env::consts::ARCH
            )),
        }
    }
    body
}

/// Open the pre-filled GitHub issue in the browser.
#[tauri::command]
fn open_feedback(app: tauri::AppHandle, kind: String, title: String, body: String) -> Res<()> {
    // Keep URLs within what browsers and GitHub accept.
    let mut body = body;
    if body.len() > 6000 {
        let mut cut = 6000;
        while !body.is_char_boundary(cut) {
            cut -= 1;
        }
        body.truncate(cut);
        body.push_str("\n… (truncated)");
    }
    let label = match kind.as_str() {
        "Bug" => "bug",
        "Idea" => "enhancement",
        "Looks different from Lightroom" => "fidelity",
        "Slow" => "performance",
        _ => "feedback",
    };
    let url = format!(
        "{FEEDBACK_URL}?labels={}&title={}&body={}",
        percent_encode(&format!("feedback,{label}")),
        percent_encode(&format!("[{kind}] {}", title.trim())),
        percent_encode(&body)
    );
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| e.to_string())
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
    #[serde(default)]
    long_edge: Option<u32>,
    /// Resize to a long or short edge, a width or height (px), megapixels
    /// or a percent; wins over `long_edge`.
    #[serde(default)]
    resize: Option<ResizeChoice>,
    /// Exact output size (a size preset); wins over the others.
    exact: Option<(u32, u32)>,
    sharpen: Option<String>,
    metadata: String,
    max_kb: Option<u64>,
    /// A frame around the photo.
    #[serde(default)]
    border: Option<amb_export::border::Border>,
    /// Put the photo's rating and keywords in the file's metadata.
    #[serde(default = "yes")]
    include_tags: bool,
    /// File names for a batch, as Import's templates (`{name}`, `{seq}`, ...).
    #[serde(default)]
    name_template: Option<String>,
    /// Text or a logo on the photo.
    #[serde(default)]
    watermark: Option<amb_export::watermark::Watermark>,
    /// JPEG chroma subsampling ("444", "422", "420"); none for automatic.
    #[serde(default)]
    subsampling: Option<String>,
    #[serde(default)]
    progressive: bool,
    /// AVIF encoder speed, 1 (slow, small) to 10.
    #[serde(default)]
    avif_speed: Option<u8>,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
struct ResizeChoice {
    /// long, short, width, height, mp or percent.
    mode: String,
    value: f64,
}

impl ResizeChoice {
    fn resize(&self) -> Res<amb_export::Resize> {
        use amb_export::Resize;
        let v = self.value;
        if !(v.is_finite() && v > 0.0) {
            return Err(format!("resize: {v} isn't a size"));
        }
        let px = v.round().clamp(1.0, 65_535.0) as u32;
        Ok(match self.mode.as_str() {
            "long" => Resize::LongEdge(px),
            "short" => Resize::ShortEdge(px),
            "width" => Resize::Width(px),
            "height" => Resize::Height(px),
            "mp" => Resize::Megapixels(v),
            "percent" => Resize::Percent(v),
            other => return Err(format!("unknown resize {other:?}")),
        })
    }
}

/// Export choices saved under a name (the dialog's fields, as it shows them).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ExportPreset {
    name: String,
    fields: serde_json::Map<String, serde_json::Value>,
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
    let ext = amb_export::Format::from_name(&format)?.extension();
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
        let mut s = export_settings(&options)?;
        if options.include_tags {
            if let Some(info) = slot
                .lock()
                .ok()
                .and_then(|g| g.as_ref().map(|s| s.info.clone()))
            {
                s.tags = amb_export::exif::Tags {
                    rating: info.rating,
                    keywords: info.keywords,
                    label: info.label,
                    title: info.title,
                    caption: info.caption,
                    creator: info.creator,
                    copyright: info.copyright,
                };
            }
        }
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

/// The export dialog's choices as export settings (without the photo's tags).
fn export_settings(options: &ExportOptions) -> Res<amb_export::ExportSettings> {
    use amb_export::{BitDepth, ExportSettings, Format, MetadataPolicy, Resize};
    {
        let format = Format::from_name(&options.format)
            .ok_or(format!("unknown format {}", options.format))?;
        let mut s = ExportSettings::new(format);
        s.quality = options.quality.clamp(1, 100);
        s.bit_depth = options.bits.and_then(BitDepth::from_bits);
        s.color_space = amb_color::space::OutputSpace::from_id(&options.color_space)
            .ok_or(format!("unknown colour space {}", options.color_space))?;
        if let Some((w, h)) = options.exact.filter(|(w, h)| *w > 0 && *h > 0) {
            s.resize = Resize::Exact(w, h);
        } else if let Some(r) = &options.resize {
            s.resize = r.resize()?;
        } else if let Some(edge) = options.long_edge.filter(|e| *e > 0) {
            s.resize = Resize::LongEdge(edge);
        }
        s.sharpen = options
            .sharpen
            .as_deref()
            .and_then(amb_export::parse_sharpen);
        s.metadata =
            MetadataPolicy::from_id(&options.metadata).unwrap_or(MetadataPolicy::AllExceptLocation);
        if let Some(b) = &options.border {
            b.validate()?;
            s.border = Some(b.clone());
        }
        if let Some(w) = &options.watermark {
            w.validate()?;
            s.watermark = Some(w.clone());
        }
        s.subsampling = match options.subsampling.as_deref() {
            None | Some("") => None,
            Some("444") => Some(amb_export::Subsampling::S444),
            Some("422") => Some(amb_export::Subsampling::S422),
            Some("420") => Some(amb_export::Subsampling::S420),
            Some(other) => return Err(format!("unknown subsampling {other:?}")),
        };
        s.progressive = options.progressive;
        if let Some(speed) = options.avif_speed {
            s.avif_speed = speed.clamp(1, 10);
        }
        s.max_bytes = options
            .max_kb
            .filter(|k| *k > 0 && format == Format::Jpeg)
            .map(|k| k * 1000);
        Ok(s)
    }
}

/// The presets starred as favourites (paths).
#[tauri::command]
fn preset_favourites(state: tauri::State<'_, AppState>) -> Res<Vec<String>> {
    Ok(state
        .preset_favourites
        .lock()
        .map_err(|e| e.to_string())?
        .clone())
}

/// Star or unstar a preset.
#[tauri::command]
fn set_preset_favourite(path: String, on: bool, state: tauri::State<'_, AppState>) -> Res<()> {
    {
        let mut list = state.preset_favourites.lock().map_err(|e| e.to_string())?;
        list.retain(|p| *p != path);
        if on {
            list.push(path);
        }
    }
    save_config(&state);
    Ok(())
}

#[derive(Serialize)]
struct MaskEffects {
    builtin: Vec<MaskEffect>,
    mine: Vec<MaskEffect>,
}

/// The built-in mask effects and the user's.
#[tauri::command]
fn mask_effects(state: tauri::State<'_, AppState>) -> Res<MaskEffects> {
    #[derive(Deserialize)]
    struct File {
        effects: Vec<MaskEffect>,
    }
    let file: File = serde_json::from_str(BUILTIN_MASK_EFFECTS).map_err(|e| e.to_string())?;
    Ok(MaskEffects {
        builtin: file.effects,
        mine: state
            .mask_effects
            .lock()
            .map_err(|e| e.to_string())?
            .clone(),
    })
}

/// Save (or replace) one of the user's mask effects.
#[tauri::command]
fn save_mask_effect(
    name: String,
    values: BTreeMap<String, f64>,
    state: tauri::State<'_, AppState>,
) -> Res<MaskEffects> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("give the effect a name".into());
    }
    let values: BTreeMap<String, f64> = values
        .into_iter()
        .filter(|(k, v)| v.is_finite() && amb_presets::masks::local_slider(k).is_some())
        .collect();
    {
        let mut list = state.mask_effects.lock().map_err(|e| e.to_string())?;
        list.retain(|e| e.name != name);
        list.push(MaskEffect { name, values });
        list.sort_by_key(|e| e.name.to_lowercase());
    }
    save_config(&state);
    mask_effects(state)
}

#[tauri::command]
fn delete_mask_effect(name: String, state: tauri::State<'_, AppState>) -> Res<MaskEffects> {
    state
        .mask_effects
        .lock()
        .map_err(|e| e.to_string())?
        .retain(|e| e.name != name);
    save_config(&state);
    mask_effects(state)
}

/// The user's export presets, by name.
#[tauri::command]
fn export_presets(state: tauri::State<'_, AppState>) -> Res<Vec<ExportPreset>> {
    Ok(state
        .export_presets
        .lock()
        .map_err(|e| e.to_string())?
        .clone())
}

/// Save (or replace) an export preset.
#[tauri::command]
fn save_export_preset(
    name: String,
    fields: serde_json::Map<String, serde_json::Value>,
    state: tauri::State<'_, AppState>,
) -> Res<()> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("give the preset a name".into());
    }
    {
        let mut list = state.export_presets.lock().map_err(|e| e.to_string())?;
        list.retain(|p| p.name != name);
        list.push(ExportPreset { name, fields });
        list.sort_by_key(|p| p.name.to_lowercase());
    }
    save_config(&state);
    Ok(())
}

#[tauri::command]
fn delete_export_preset(name: String, state: tauri::State<'_, AppState>) -> Res<()> {
    state
        .export_presets
        .lock()
        .map_err(|e| e.to_string())?
        .retain(|p| p.name != name);
    save_config(&state);
    Ok(())
}

#[tauri::command]
async fn presets(state: tauri::State<'_, AppState>) -> Res<Vec<PresetEntry>> {
    let mut dirs = state.preset_dirs.lock().map_err(|e| e.to_string())?.clone();
    // Presets saved in the app ("Save as preset…").
    if let Some(own) = own_presets_dir(state.inner()).filter(|d| d.is_dir() && !dirs.contains(d)) {
        dirs.push(own);
    }
    tauri::async_runtime::spawn_blocking(move || session::scan_presets(&dirs))
        .await
        .map_err(|e| e.to_string())
}

fn own_presets_dir(state: &AppState) -> Option<PathBuf> {
    let path = state.config_path.lock().ok()?;
    Some(path.as_ref()?.parent()?.join("presets"))
}

/// Save the open photo's edit as a preset (crop, masks and geometry left
/// out) in the app's presets folder; returns the presets list.
#[tauri::command]
async fn save_preset(
    name: String,
    group: String,
    state: tauri::State<'_, AppState>,
) -> Res<Vec<PresetEntry>> {
    let dir = own_presets_dir(state.inner()).ok_or("the settings folder isn't available")?;
    let settings = {
        let guard = state.session.lock().map_err(|e| e.to_string())?;
        guard
            .as_ref()
            .ok_or("open a photo first")?
            .state
            .settings()
            .clone()
    };
    tauri::async_runtime::spawn_blocking(move || {
        session::save_preset(&dir, &name, &group, &settings)
    })
    .await
    .map_err(|e| e.to_string())??;
    presets(state).await
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
        {
            let mut dirs = state.preset_dirs.lock().map_err(|e| e.to_string())?;
            if !dirs.contains(&dir) {
                dirs.push(dir);
            }
        }
        save_config(state.inner());
    }
    presets(state).await
}

/// Built-in size presets followed by the user's.
#[tauri::command]
fn size_presets(state: tauri::State<'_, AppState>) -> Res<Vec<SizePreset>> {
    let custom = state.size_presets.lock().map_err(|e| e.to_string())?;
    Ok(sizes::all(&custom))
}

/// Add a user size preset (or a clone of a built-in), or update one.
#[tauri::command]
fn save_size_preset(preset: SizePreset, state: tauri::State<'_, AppState>) -> Res<Vec<SizePreset>> {
    {
        let mut custom = state.size_presets.lock().map_err(|e| e.to_string())?;
        sizes::upsert(&mut custom, &preset)?;
    }
    save_config(state.inner());
    size_presets(state)
}

#[tauri::command]
fn delete_size_preset(id: String, state: tauri::State<'_, AppState>) -> Res<Vec<SizePreset>> {
    {
        let mut custom = state.size_presets.lock().map_err(|e| e.to_string())?;
        sizes::remove(&mut custom, &id)?;
    }
    save_config(state.inner());
    size_presets(state)
}

/// The folder for the user's own looks (next to the settings file).
fn looks_dir(state: &AppState) -> Option<PathBuf> {
    let path = state.config_path.lock().ok()?;
    Some(path.as_ref()?.parent()?.join("looks"))
}

fn looks_list(state: &AppState) -> Res<looks::Looks> {
    let (layout, theme) = state.look.lock().map_err(|e| e.to_string())?.clone();
    Ok(looks::list(
        looks_dir(state).as_deref(),
        layout.as_deref(),
        theme.as_deref(),
    ))
}

/// Built-in and user layouts and themes, and which are active.
#[tauri::command]
fn looks(state: tauri::State<'_, AppState>) -> Res<looks::Looks> {
    looks_list(state.inner())
}

#[derive(Serialize)]
struct SavedLook {
    id: String,
    kind: looks::Kind,
    looks: looks::Looks,
}

fn need_looks_dir(state: &AppState) -> Res<PathBuf> {
    looks_dir(state).ok_or_else(|| "the app's settings folder isn't available".to_string())
}

/// Save a user look (a clone, a rename, an edited theme). `id` updates an
/// existing one; without it a new file is made.
#[tauri::command]
fn save_look(
    kind: looks::Kind,
    id: Option<String>,
    data: serde_json::Value,
    state: tauri::State<'_, AppState>,
) -> Res<SavedLook> {
    let dir = need_looks_dir(state.inner())?;
    let id = looks::save(&dir, kind, id.as_deref(), data)?;
    Ok(SavedLook {
        id,
        kind,
        looks: looks_list(state.inner())?,
    })
}

#[tauri::command]
fn delete_look(
    kind: looks::Kind,
    id: String,
    state: tauri::State<'_, AppState>,
) -> Res<looks::Looks> {
    looks::delete(&need_looks_dir(state.inner())?, kind, &id)?;
    looks_list(state.inner())
}

/// Remember the active layout and theme.
#[tauri::command]
fn set_active_look(layout: String, theme: String, state: tauri::State<'_, AppState>) -> Res<()> {
    *state.look.lock().map_err(|e| e.to_string())? = (Some(layout), Some(theme));
    save_config(state.inner());
    Ok(())
}

/// Import a shared layout or theme file (picked in a dialog).
#[tauri::command]
async fn import_look(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Res<Option<SavedLook>> {
    let dir = need_looks_dir(state.inner())?;
    let Some(path) = app
        .dialog()
        .file()
        .set_title("Import a layout or theme")
        .add_filter("Ambrotype layouts and themes", &["json"])
        .blocking_pick_file()
        .and_then(|p| p.into_path().ok())
    else {
        return Ok(None);
    };
    let (kind, id) = looks::import(&dir, &path)?;
    Ok(Some(SavedLook {
        id,
        kind,
        looks: looks_list(state.inner())?,
    }))
}

/// Save a look as a file to share.
#[tauri::command]
async fn export_look(
    app: tauri::AppHandle,
    kind: looks::Kind,
    id: String,
    name: String,
    state: tauri::State<'_, AppState>,
) -> Res<Option<String>> {
    let text = looks::export_text(looks_dir(state.inner()).as_deref(), kind, &id)?;
    let Some(path) = app
        .dialog()
        .file()
        .set_title("Share this layout or theme")
        .set_file_name(format!("{name}.json"))
        .add_filter("Ambrotype layouts and themes", &["json"])
        .blocking_save_file()
        .and_then(|p| p.into_path().ok())
    else {
        return Ok(None);
    };
    std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(Some(path.display().to_string()))
}

/// Show the folder with the user's looks (where an AI assistant can put new ones).
#[tauri::command]
fn open_looks_folder(app: tauri::AppHandle, state: tauri::State<'_, AppState>) -> Res<String> {
    let dir = need_looks_dir(state.inner())?;
    for kind in ["layouts", "themes"] {
        std::fs::create_dir_all(dir.join(kind)).map_err(|e| e.to_string())?;
    }
    app.opener()
        .open_path(dir.display().to_string(), None::<&str>)
        .map_err(|e| e.to_string())?;
    Ok(dir.display().to_string())
}

/// The keyboard shortcuts the user changed (the UI knows the defaults).
#[tauri::command]
fn keybindings(state: tauri::State<'_, AppState>) -> Res<keys::Bindings> {
    Ok(state.keybindings.lock().map_err(|e| e.to_string())?.clone())
}

#[tauri::command]
fn set_keybindings(bindings: keys::Bindings, state: tauri::State<'_, AppState>) -> Res<()> {
    let clean = keys::validate(&bindings)?;
    *state.keybindings.lock().map_err(|e| e.to_string())? = clean;
    save_config(state.inner());
    Ok(())
}

#[derive(Serialize)]
struct SettingsInfo {
    version: &'static str,
    folder: Option<String>,
}

/// For the Settings dialog: the app version and where its settings live.
#[tauri::command]
fn settings_info(state: tauri::State<'_, AppState>) -> SettingsInfo {
    let folder = state
        .config_path
        .lock()
        .ok()
        .and_then(|p| Some(p.as_ref()?.parent()?.display().to_string()));
    SettingsInfo {
        version: env!("CARGO_PKG_VERSION"),
        folder,
    }
}

/// Where the source code is (the GPL's "corresponding source").
const SOURCE_URL: &str = "https://github.com/shardpelt/Ambrotype";

/// For Settings → About: the licence and notices, built into the app so
/// they can be read offline.
#[tauri::command]
fn about_text(which: String) -> Res<&'static str> {
    match which.as_str() {
        "licence" => Ok(include_str!("../../../LICENSE")),
        "notice" => Ok(include_str!("../../../NOTICE")),
        _ => Err(format!("no text {which:?}")),
    }
}

#[tauri::command]
fn open_source_page(app: tauri::AppHandle) -> Res<()> {
    app.opener()
        .open_url(SOURCE_URL, None::<&str>)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn open_settings_folder(app: tauri::AppHandle, state: tauri::State<'_, AppState>) -> Res<()> {
    let folder = settings_info(state)
        .folder
        .ok_or("the settings folder isn't available")?;
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    app.opener()
        .open_path(folder, None::<&str>)
        .map_err(|e| e.to_string())
}

#[derive(Serialize)]
struct InfoState {
    info: amb_protocol::sidecar::PhotoInfo,
    /// Keywords used lately, for suggestions.
    recent: Vec<String>,
}

fn info_state(state: &AppState) -> Res<InfoState> {
    let info = state
        .session
        .lock()
        .map_err(|e| e.to_string())?
        .as_ref()
        .map(|s| s.info.clone())
        .unwrap_or_default();
    let recent = state
        .recent_keywords
        .lock()
        .map_err(|e| e.to_string())?
        .clone();
    Ok(InfoState { info, recent })
}

/// The open photo's rating and keywords.
#[tauri::command]
fn photo_info(state: tauri::State<'_, AppState>) -> Res<InfoState> {
    info_state(state.inner())
}

/// Set the open photo's rating and keywords; they're written to its sidecar at once.
#[tauri::command]
fn set_photo_info(
    info: amb_protocol::sidecar::PhotoInfo,
    state: tauri::State<'_, AppState>,
) -> Res<InfoState> {
    let saved = {
        let mut guard = state.session.lock().map_err(|e| e.to_string())?;
        let s = guard.as_mut().ok_or("no photo is open")?;
        s.set_info(&info)?
    };
    {
        let mut recent = state.recent_keywords.lock().map_err(|e| e.to_string())?;
        for k in saved.keywords.iter().rev() {
            recent.retain(|r| !r.eq_ignore_ascii_case(k));
            recent.insert(0, k.clone());
        }
        recent.truncate(200);
    }
    save_config(state.inner());
    info_state(state.inner())
}

/// Built-in collage templates followed by the user's.
#[tauri::command]
fn collage_templates(state: tauri::State<'_, AppState>) -> Res<Vec<collages::TemplateEntry>> {
    let custom = state.collage_templates.lock().map_err(|e| e.to_string())?;
    Ok(collages::all(&custom))
}

#[tauri::command]
fn save_collage_template(
    template: collages::TemplateEntry,
    state: tauri::State<'_, AppState>,
) -> Res<(Vec<collages::TemplateEntry>, String)> {
    let id = {
        let mut custom = state.collage_templates.lock().map_err(|e| e.to_string())?;
        collages::upsert(&mut custom, &template)?
    };
    save_config(state.inner());
    Ok((collage_templates(state)?, id))
}

#[tauri::command]
fn delete_collage_template(
    id: String,
    state: tauri::State<'_, AppState>,
) -> Res<Vec<collages::TemplateEntry>> {
    {
        let mut custom = state.collage_templates.lock().map_err(|e| e.to_string())?;
        collages::remove(&mut custom, &id)?;
    }
    save_config(state.inner());
    collage_templates(state)
}

/// Save a collage template as a file to share.
#[tauri::command]
async fn share_collage_template(
    app: tauri::AppHandle,
    template: amb_export::collage::Template,
) -> Res<Option<String>> {
    template.validate()?;
    let Some(path) = app
        .dialog()
        .file()
        .set_title("Share this collage layout")
        .set_file_name(format!("{}.json", template.name))
        .add_filter("Collage layouts", &["json"])
        .blocking_save_file()
        .and_then(|p| p.into_path().ok())
    else {
        return Ok(None);
    };
    let text = serde_json::to_string_pretty(&template).map_err(|e| e.to_string())? + "\n";
    std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(Some(path.display().to_string()))
}

/// Save the collage as a project file (to reopen and edit later). Without a
/// path, asks where.
#[tauri::command]
async fn save_collage_project(
    app: tauri::AppHandle,
    project: collages::ProjectIn,
    path: Option<String>,
    suggested: String,
) -> Res<Option<String>> {
    let path = match path {
        Some(p) => PathBuf::from(p),
        None => {
            let Some(p) = app
                .dialog()
                .file()
                .set_title("Save the collage")
                .set_file_name(format!("{suggested}.{}", collages::PROJECT_EXTENSION))
                .add_filter("Ambrotype collage", &[collages::PROJECT_EXTENSION])
                .blocking_save_file()
                .and_then(|p| p.into_path().ok())
            else {
                return Ok(None);
            };
            p
        }
    };
    let path = if path
        .extension()
        .is_some_and(|e| e == collages::PROJECT_EXTENSION)
    {
        path
    } else {
        path.with_extension(collages::PROJECT_EXTENSION)
    };
    tauri::async_runtime::spawn_blocking(move || {
        collages::save_project(&path, &project).map(|()| Some(path.display().to_string()))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Open a collage project file (given, or picked in a dialog).
#[tauri::command]
async fn open_collage_project(
    app: tauri::AppHandle,
    path: Option<String>,
) -> Res<Option<collages::OpenedProject>> {
    let path = match path {
        Some(p) => PathBuf::from(p),
        None => {
            let Some(p) = app
                .dialog()
                .file()
                .set_title("Open a collage")
                .add_filter("Ambrotype collage", &[collages::PROJECT_EXTENSION])
                .blocking_pick_file()
                .and_then(|p| p.into_path().ok())
            else {
                return Ok(None);
            };
            p
        }
    };
    tauri::async_runtime::spawn_blocking(move || collages::open_project(&path).map(Some))
        .await
        .map_err(|e| e.to_string())?
}

/// Read a shared collage template file (picked in a dialog).
#[tauri::command]
async fn import_collage_template(
    app: tauri::AppHandle,
) -> Res<Option<amb_export::collage::Template>> {
    let Some(path) = app
        .dialog()
        .file()
        .set_title("Open a collage layout")
        .add_filter("Collage layouts", &["json"])
        .blocking_pick_file()
        .and_then(|p| p.into_path().ok())
    else {
        return Ok(None);
    };
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    amb_export::collage::Template::from_json(&text)
        .map(Some)
        .map_err(|e| format!("{}: {e}", path.display()))
}

/// Collages read each photo's edits from its sidecar: write the open photo's
/// unsaved edits there first.
fn save_open_edits(slot: &Mutex<Option<Session>>) -> Res<()> {
    let mut guard = slot.lock().map_err(|e| e.to_string())?;
    if let Some(s) = guard.as_mut().filter(|s| s.has_unsaved_edits()) {
        s.save_sidecar()?;
    }
    Ok(())
}

/// A JPEG of a photo with its edits (crop included), for the collage canvas:
/// the open photo's current edits, or another photo's sidecar.
#[tauri::command]
async fn edited_thumbnail(
    path: String,
    edge: Option<u32>,
    state: tauri::State<'_, AppState>,
) -> Res<Response> {
    let slot = state.session.clone();
    let lens = state.lens.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let lens = lens.get().unwrap_or_else(|| empty_lens());
        let edge = edge.unwrap_or(1024).clamp(64, 2048) as usize;
        let none = Default::default();
        {
            let mut guard = slot.lock().map_err(|e| e.to_string())?;
            if let Some(s) = guard.as_mut().filter(|s| s.path == Path::new(&path)) {
                return Ok(Response::new(
                    s.preview(edge, &none, false, false, lens).jpeg,
                ));
            }
        }
        let mut s = Session::open(Path::new(&path))?;
        Ok(Response::new(
            s.preview(edge, &none, false, false, lens).jpeg,
        ))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Clone, Serialize)]
struct CollageProgress {
    page: usize,
    pages: usize,
    done: usize,
    total: usize,
}

/// Render one collage (each photo with its own edits) and save it.
#[tauri::command]
async fn export_collage(
    app: tauri::AppHandle,
    template: amb_export::collage::Template,
    cells: Vec<collages::CellPhoto>,
    path: String,
    options: ExportOptions,
    state: tauri::State<'_, AppState>,
) -> Res<Exported> {
    use tauri::Emitter;
    let lens = state.lens.clone();
    let slot = state.session.clone();
    tauri::async_runtime::spawn_blocking(move || {
        save_open_edits(&slot)?;
        let s = export_settings(&options)?;
        let lens = lens.get().unwrap_or_else(|| empty_lens());
        let mut progress = |done, total| {
            let _ = app.emit(
                "collage-progress",
                CollageProgress {
                    page: 1,
                    pages: 1,
                    done,
                    total,
                },
            );
        };
        let r = collages::render(&template, &cells, Path::new(&path), &s, lens, &mut progress)?;
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

#[derive(Deserialize)]
struct BatchPhoto {
    path: String,
    /// Width / height of the upright photo (from its thumbnail).
    aspect: f64,
}

/// Many photos into pages of the same layout: DIR/<name>-001.<ext>, ...
#[tauri::command]
async fn export_collage_batch(
    app: tauri::AppHandle,
    template: amb_export::collage::Template,
    photos: Vec<BatchPhoto>,
    options: ExportOptions,
    state: tauri::State<'_, AppState>,
) -> Res<Option<Vec<String>>> {
    use tauri::Emitter;
    let Some(dir) = app
        .dialog()
        .file()
        .set_title("Folder for the collage pages")
        .blocking_pick_folder()
        .and_then(|p| p.into_path().ok())
    else {
        return Ok(None);
    };
    let lens = state.lens.clone();
    let slot = state.session.clone();
    tauri::async_runtime::spawn_blocking(move || {
        save_open_edits(&slot)?;
        template.validate()?;
        let s = export_settings(&options)?;
        let lens = lens.get().unwrap_or_else(|| empty_lens());
        let photos: Vec<(String, f64)> = photos.into_iter().map(|p| (p.path, p.aspect)).collect();
        let written = collages::render_batch(
            &template,
            &photos,
            &dir,
            &s,
            lens,
            &mut |page, pages, done, total| {
                let _ = app.emit(
                    "collage-progress",
                    CollageProgress {
                        page,
                        pages,
                        done,
                        total,
                    },
                );
            },
        )?;
        let written: Vec<String> = written.iter().map(|p| p.display().to_string()).collect();
        Ok(Some(written))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The raws in the open photo's folder, for the filmstrip.
#[tauri::command]
async fn folder_photos(path: String) -> Res<Vec<folder::FolderPhoto>> {
    tauri::async_runtime::spawn_blocking(move || folder::list(Path::new(&path)))
        .await
        .map_err(|e| e.to_string())?
}

/// A small JPEG of a raw for the filmstrip (cached in the app's cache folder).
#[tauri::command]
async fn thumbnail(
    path: String,
    edge: Option<u32>,
    edited: Option<bool>,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Res<Response> {
    let cache = app
        .path()
        .app_cache_dir()
        .ok()
        .map(|d| d.join("thumbnails"));
    let lens = state.lens.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let edge = edge.unwrap_or(folder::THUMB_EDGE);
        if edited == Some(true) {
            let lens = lens.get().unwrap_or_else(|| empty_lens());
            folder::edited_thumbnail(Path::new(&path), edge, cache.as_deref(), lens)
        } else {
            folder::thumbnail(Path::new(&path), edge, cache.as_deref())
        }
        .map(Response::new)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Built-in border presets followed by the user's.
#[tauri::command]
fn border_presets(state: tauri::State<'_, AppState>) -> Res<Vec<borders::BorderPreset>> {
    let custom = state.border_presets.lock().map_err(|e| e.to_string())?;
    Ok(borders::all(&custom))
}

/// Add or update a border preset of the user's; returns the list and the saved id.
#[tauri::command]
fn save_border_preset(
    preset: borders::BorderPreset,
    state: tauri::State<'_, AppState>,
) -> Res<(Vec<borders::BorderPreset>, String)> {
    let id = {
        let mut custom = state.border_presets.lock().map_err(|e| e.to_string())?;
        borders::upsert(&mut custom, &preset)?
    };
    save_config(state.inner());
    Ok((border_presets(state)?, id))
}

#[tauri::command]
fn delete_border_preset(
    id: String,
    state: tauri::State<'_, AppState>,
) -> Res<Vec<borders::BorderPreset>> {
    {
        let mut custom = state.border_presets.lock().map_err(|e| e.to_string())?;
        borders::remove(&mut custom, &id)?;
    }
    save_config(state.inner());
    border_presets(state)
}

fn hub_index(state: &AppState) -> String {
    state
        .hub_url
        .lock()
        .ok()
        .and_then(|h| h.clone())
        .unwrap_or_else(|| hub::DEFAULT_INDEX.to_string())
}

/// The hub's address (the default unless changed in Settings).
#[tauri::command]
fn hub_url(state: tauri::State<'_, AppState>) -> String {
    hub_index(state.inner())
}

/// Use another hub; `None` goes back to the default.
#[tauri::command]
fn set_hub_url(url: Option<String>, state: tauri::State<'_, AppState>) -> Res<String> {
    let url = url
        .map(|u| u.trim().to_string())
        .filter(|u| !u.is_empty() && u != hub::DEFAULT_INDEX);
    if let Some(u) = &url {
        if !(u.starts_with("https://") || u.starts_with("file://")) || !u.ends_with(".json") {
            return Err("a hub address is an https:// link to its index.json".into());
        }
    }
    *state.hub_url.lock().map_err(|e| e.to_string())? = url;
    save_config(state.inner());
    Ok(hub_index(state.inner()))
}

#[derive(Serialize)]
struct HubList {
    url: String,
    entries: Vec<hub::HubEntry>,
}

/// Everything in the hub, with previews and what's installed already.
#[tauri::command]
async fn hub_browse(state: tauri::State<'_, AppState>) -> Res<HubList> {
    let url = hub_index(state.inner());
    let dir = looks_dir(state.inner());
    let plugins = plugins_dir(state.inner());
    let presets = own_presets_dir(state.inner());
    let collages = state
        .collage_templates
        .lock()
        .map_err(|e| e.to_string())?
        .clone();
    let sizes = state
        .size_presets
        .lock()
        .map_err(|e| e.to_string())?
        .clone();
    let u = url.clone();
    let entries = tauri::async_runtime::spawn_blocking(move || {
        let mine = hub::Mine {
            looks: dir.as_deref(),
            plugins: plugins.as_deref(),
            presets: presets.as_deref(),
            collages: &collages,
            sizes: &sizes,
        };
        hub::browse(&u, mine)
    })
    .await
    .map_err(|e| e.to_string())??;
    Ok(HubList { url, entries })
}

/// Install (or update) a develop preset from the hub; returns the presets list.
#[tauri::command]
async fn hub_install_preset(
    id: String,
    state: tauri::State<'_, AppState>,
) -> Res<Vec<PresetEntry>> {
    let url = hub_index(state.inner());
    let dir = own_presets_dir(state.inner()).ok_or("the settings folder isn't available")?;
    tauri::async_runtime::spawn_blocking(move || hub::install_preset(&dir, &url, &id))
        .await
        .map_err(|e| e.to_string())??;
    presets(state).await
}

/// Install (or update) a collage template from the hub; returns the templates.
#[tauri::command]
async fn hub_install_collage(
    id: String,
    state: tauri::State<'_, AppState>,
) -> Res<Vec<collages::TemplateEntry>> {
    let url = hub_index(state.inner());
    let mut custom = state
        .collage_templates
        .lock()
        .map_err(|e| e.to_string())?
        .clone();
    let custom = tauri::async_runtime::spawn_blocking(move || {
        hub::install_collage(&mut custom, &url, &id).map(|_| custom)
    })
    .await
    .map_err(|e| e.to_string())??;
    *state.collage_templates.lock().map_err(|e| e.to_string())? = custom;
    save_config(state.inner());
    collage_templates(state)
}

/// Install (or update) a size profile from the hub; returns the size presets.
#[tauri::command]
async fn hub_install_sizes(id: String, state: tauri::State<'_, AppState>) -> Res<Vec<SizePreset>> {
    let url = hub_index(state.inner());
    let mut custom = state
        .size_presets
        .lock()
        .map_err(|e| e.to_string())?
        .clone();
    let custom = tauri::async_runtime::spawn_blocking(move || {
        hub::install_sizes(&mut custom, &url, &id).map(|_| custom)
    })
    .await
    .map_err(|e| e.to_string())??;
    *state.size_presets.lock().map_err(|e| e.to_string())? = custom;
    save_config(state.inner());
    size_presets(state)
}

/// Install (or update) a plugin from the hub; returns its id. It asks for
/// its permissions when first run, like any plugin.
#[tauri::command]
async fn hub_install_plugin(id: String, state: tauri::State<'_, AppState>) -> Res<String> {
    let url = hub_index(state.inner());
    let dir = plugins_dir(state.inner()).ok_or("the plugins folder isn't available")?;
    tauri::async_runtime::spawn_blocking(move || hub::install_plugin(&dir, &url, &id))
        .await
        .map_err(|e| e.to_string())?
}

/// Install (or update) a look from the hub.
#[tauri::command]
async fn hub_install(id: String, state: tauri::State<'_, AppState>) -> Res<SavedLook> {
    let url = hub_index(state.inner());
    let dir = need_looks_dir(state.inner())?;
    let (kind, id) = tauri::async_runtime::spawn_blocking(move || hub::install(&dir, &url, &id))
        .await
        .map_err(|e| e.to_string())??;
    Ok(SavedLook {
        id,
        kind,
        looks: looks_list(state.inner())?,
    })
}

/// Open GitHub with this look filled in as a new file for the hub (a pull request).
#[tauri::command]
fn hub_submit(
    app: tauri::AppHandle,
    kind: looks::Kind,
    id: String,
    state: tauri::State<'_, AppState>,
) -> Res<String> {
    let look = looks::all(looks_dir(state.inner()).as_deref(), kind)
        .into_iter()
        .find(|l| l.id == id && l.usable)
        .ok_or_else(|| format!("unknown layout or theme {id}"))?;
    let url = hub::submit_url(&hub_index(state.inner()), kind, &look.data)?;
    app.opener()
        .open_url(url.clone(), None::<&str>)
        .map_err(|e| e.to_string())?;
    Ok(url)
}

// ---------------------------------------------------------------- LUTs

/// Where the user keeps `.cube` files for the LUT menu.
fn luts_dir(state: &AppState) -> Option<PathBuf> {
    let path = state.config_path.lock().ok()?;
    Some(path.as_ref()?.parent()?.join("luts"))
}

#[derive(Serialize)]
struct LutEntry {
    name: String,
    path: String,
}

/// The `.cube` files in the LUT folder (and its subfolders), by name.
#[tauri::command]
fn luts_list(state: tauri::State<'_, AppState>) -> Vec<LutEntry> {
    fn walk(dir: &Path, depth: usize, out: &mut Vec<LutEntry>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() && depth < 3 {
                walk(&p, depth + 1, out);
            } else if p
                .extension()
                .is_some_and(|x| x.eq_ignore_ascii_case("cube"))
            {
                out.push(LutEntry {
                    name: p
                        .file_stem()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                    path: p.display().to_string(),
                });
            }
        }
    }
    let mut out = Vec::new();
    if let Some(dir) = luts_dir(&state) {
        walk(&dir, 0, &mut out);
    }
    out.sort_by_key(|l| l.name.to_lowercase());
    out
}

/// Pick a `.cube` file anywhere.
#[tauri::command]
async fn pick_lut(app: tauri::AppHandle) -> Option<String> {
    app.dialog()
        .file()
        .set_title("Choose a LUT")
        .add_filter("LUT (.cube)", &["cube", "CUBE"])
        .blocking_pick_file()
        .and_then(|p| p.into_path().ok())
        .map(|p| p.display().to_string())
}

/// A PNG logo for the export watermark.
#[tauri::command]
async fn pick_watermark_image(app: tauri::AppHandle) -> Option<String> {
    app.dialog()
        .file()
        .set_title("Choose a logo")
        .add_filter("PNG", &["png", "PNG"])
        .blocking_pick_file()
        .and_then(|p| p.into_path().ok())
        .map(|p| p.display().to_string())
}

/// Open the LUT folder (made if missing), to drop `.cube` files in.
#[tauri::command]
fn open_luts_folder(app: tauri::AppHandle, state: tauri::State<'_, AppState>) -> Res<()> {
    let dir = luts_dir(&state).ok_or("the LUT folder isn't available")?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    app.opener()
        .open_path(dir.display().to_string(), None::<&str>)
        .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------- plugins

fn plugins_dir(state: &AppState) -> Option<PathBuf> {
    let path = state.config_path.lock().ok()?;
    Some(path.as_ref()?.parent()?.join("plugins"))
}

fn plugins_state(state: &AppState) -> Res<Vec<plugins::PluginEntry>> {
    let approvals = state
        .plugin_approvals
        .lock()
        .map_err(|e| e.to_string())?
        .clone();
    Ok(plugins::list(plugins_dir(state).as_deref(), &approvals))
}

/// Built-in plugins, then the user's, with whether each is allowed to run.
#[tauri::command]
fn plugins_list(state: tauri::State<'_, AppState>) -> Res<Vec<plugins::PluginEntry>> {
    plugins_state(state.inner())
}

/// Install a plugin from a folder the user picks (it is checked first).
#[tauri::command]
async fn install_plugin(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Res<Option<(Vec<plugins::PluginEntry>, String)>> {
    let dir = plugins_dir(state.inner()).ok_or("the plugins folder isn't available")?;
    let Some(src) = app
        .dialog()
        .file()
        .set_title("Install a plugin: pick its folder (with plugin.json)")
        .blocking_pick_folder()
        .and_then(|p| p.into_path().ok())
    else {
        return Ok(None);
    };
    let id = tauri::async_runtime::spawn_blocking(move || plugins::install(&dir, &src))
        .await
        .map_err(|e| e.to_string())??;
    Ok(Some((plugins_state(state.inner())?, id)))
}

#[tauri::command]
fn remove_plugin(id: String, state: tauri::State<'_, AppState>) -> Res<Vec<plugins::PluginEntry>> {
    let dir = plugins_dir(state.inner()).ok_or("the plugins folder isn't available")?;
    plugins::remove(&dir, &id)?;
    if let Ok(mut a) = state.plugin_approvals.lock() {
        a.remove(&id);
    }
    save_config(state.inner());
    plugins_state(state.inner())
}

/// Allow a plugin's permissions (for this exact plugin: a changed one asks again).
#[tauri::command]
fn approve_plugin(
    id: String,
    fingerprint: String,
    state: tauri::State<'_, AppState>,
) -> Res<Vec<plugins::PluginEntry>> {
    let plugin = plugins::find(plugins_dir(state.inner()).as_deref(), &id)?;
    if plugin.fingerprint() != fingerprint {
        return Err("the plugin changed since it was shown; look again before allowing it".into());
    }
    state
        .plugin_approvals
        .lock()
        .map_err(|e| e.to_string())?
        .insert(id, fingerprint);
    save_config(state.inner());
    plugins_state(state.inner())
}

#[derive(Serialize)]
struct PluginRun {
    state: session::StateInfo,
    notes: Vec<String>,
    message: Option<String>,
    log: Vec<String>,
    edits: usize,
}

/// Run a plugin on the open photo: its edits become one undo step.
#[tauri::command]
async fn run_plugin(
    id: String,
    params: serde_json::Map<String, serde_json::Value>,
    state: tauri::State<'_, AppState>,
) -> Res<PluginRun> {
    let plugin = plugins::find(plugins_dir(state.inner()).as_deref(), &id)?;
    let allowed = plugin.manifest.permissions.is_empty()
        || state
            .plugin_approvals
            .lock()
            .map_err(|e| e.to_string())?
            .get(&id)
            == Some(&plugin.fingerprint());
    if !allowed {
        return Err("needs-approval".into());
    }
    let slot = state.session.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(1);
        let ctx = {
            let guard = slot.lock().map_err(|e| e.to_string())?;
            guard
                .as_ref()
                .ok_or("open a photo first")?
                .plugin_context(&plugin.manifest.permissions, seed)
        };
        let out = plugin.run(&ctx, &params)?;
        let mut guard = slot.lock().map_err(|e| e.to_string())?;
        let s = guard.as_mut().ok_or("no photo open")?;
        let outcomes = if out.commands.is_empty() {
            Vec::new()
        } else {
            s.execute(
                &out.commands,
                Some(&format!("plugin {}", plugin.manifest.name)),
            )?
        };
        Ok(PluginRun {
            state: s.state_info(),
            notes: outcomes
                .iter()
                .flat_map(|o| {
                    o.notes
                        .iter()
                        .cloned()
                        .chain(o.clamped.iter().map(|c| c.to_string()))
                })
                .collect(),
            message: out.message,
            log: out.log,
            edits: out.commands.len(),
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Serialize)]
struct PluginBatch {
    /// The photos the plugin changed.
    changed: Vec<String>,
    errors: Vec<String>,
    /// The open photo's new state, when it was one of them.
    state: Option<session::StateInfo>,
    /// The plugin's message for the last photo, if any.
    message: Option<String>,
}

/// Run a plugin on many photos, each with its own settings: the open photo
/// as one undo step, the others in their sidecars. One failure doesn't stop
/// the rest.
#[tauri::command]
async fn run_plugin_many(
    app: tauri::AppHandle,
    id: String,
    params: serde_json::Map<String, serde_json::Value>,
    paths: Vec<String>,
    state: tauri::State<'_, AppState>,
) -> Res<PluginBatch> {
    use tauri::Emitter;
    let plugin = plugins::find(plugins_dir(state.inner()).as_deref(), &id)?;
    let allowed = plugin.manifest.permissions.is_empty()
        || state
            .plugin_approvals
            .lock()
            .map_err(|e| e.to_string())?
            .get(&id)
            == Some(&plugin.fingerprint());
    if !allowed {
        return Err("needs-approval".into());
    }
    let slot = state.session.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(1);
        let perms = &plugin.manifest.permissions;
        let label = format!("plugin {}", plugin.manifest.name);
        let mut out = PluginBatch {
            changed: Vec::new(),
            errors: Vec::new(),
            state: None,
            message: None,
        };
        for (i, path) in paths.iter().enumerate() {
            let photo = Path::new(path);
            let name = photo
                .file_name()
                .map_or_else(|| path.clone(), |n| n.to_string_lossy().into_owned());
            let _ = app.emit(
                "library-progress",
                LibraryProgress {
                    done: i,
                    total: paths.len(),
                    name: name.clone(),
                },
            );
            // Random plugins give each photo its own result.
            let seed = seed.wrapping_add(i as u64);
            let result = (|| -> Res<bool> {
                let open = {
                    let guard = slot.lock().map_err(|e| e.to_string())?;
                    guard
                        .as_ref()
                        .filter(|s| s.path == photo)
                        .map(|s| s.plugin_context(perms, seed))
                };
                let ctx = match open.clone() {
                    Some(ctx) => ctx,
                    None => plugins::context_for_file(photo, perms, seed)?,
                };
                let run = plugin.run(&ctx, &params)?;
                if run.message.is_some() {
                    out.message = run.message.clone();
                }
                if run.commands.is_empty() {
                    return Ok(false);
                }
                if open.is_some() {
                    let mut guard = slot.lock().map_err(|e| e.to_string())?;
                    if let Some(s) = guard.as_mut().filter(|s| s.path == photo) {
                        s.execute(&run.commands, Some(&label))?;
                        out.state = Some(s.state_info());
                        return Ok(true);
                    }
                }
                library::paste_to_file(photo, &run.commands)?;
                Ok(true)
            })();
            match result {
                Ok(true) => out.changed.push(path.clone()),
                Ok(false) => {}
                Err(e) => out.errors.push(format!("{name}: {e}")),
            }
        }
        let _ = app.emit(
            "library-progress",
            LibraryProgress {
                done: paths.len(),
                total: paths.len(),
                name: String::new(),
            },
        );
        Ok(out)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The settings `commands` would change on the open photo, as preview
/// overrides; nothing is applied (a preset's look while hovering it).
#[tauri::command]
fn trial_overrides(
    commands: Vec<Command>,
    state: tauri::State<'_, AppState>,
) -> Res<serde_json::Map<String, serde_json::Value>> {
    let guard = state.session.lock().map_err(|e| e.to_string())?;
    let s = guard.as_ref().ok_or("no photo open")?;
    let mut trial = s.state.clone();
    trial.execute_all(&commands).map_err(|e| e.to_string())?;
    let before = s.state.to_json()["settings"].clone();
    let after = trial.to_json()["settings"].clone();
    Ok(after
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(k, v)| before.get(k.as_str()) != Some(*v))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect())
}

#[derive(Serialize)]
struct Guided {
    commands: Vec<Command>,
    /// How far the worst guide is still off, degrees.
    worst_degrees: f64,
}

/// Guided Upright for the open photo: the Transform sliders that straighten
/// `guides` (fractions of the upright, uncropped photo).
#[tauri::command]
fn guided_upright(guides: Vec<[f64; 4]>, state: tauri::State<'_, AppState>) -> Res<Guided> {
    let guard = state.session.lock().map_err(|e| e.to_string())?;
    let s = guard.as_ref().ok_or("no photo open")?;
    let (commands, worst_degrees) = s.guided_upright(&guides)?;
    Ok(Guided {
        commands,
        worst_degrees,
    })
}

/// The guides Guided Upright kept for the open photo.
#[tauri::command]
fn upright_guides(state: tauri::State<'_, AppState>) -> Res<Vec<[f64; 4]>> {
    let guard = state.session.lock().map_err(|e| e.to_string())?;
    Ok(guard
        .as_ref()
        .map(Session::upright_guides)
        .unwrap_or_default())
}

#[derive(Serialize)]
struct LensDbState {
    /// Where a downloaded database goes, and whether one is there.
    dir: Option<String>,
    timestamp: Option<u64>,
    /// Lenses the editor has now (from any database found).
    lenses: Option<usize>,
}

/// The lens database: what the editor has, and the downloaded one.
#[tauri::command]
fn lens_db_state(state: tauri::State<'_, AppState>) -> LensDbState {
    let dir = amb_library::lensdb::user_dir();
    LensDbState {
        timestamp: dir
            .as_deref()
            .and_then(amb_library::lensdb::installed_timestamp),
        dir: dir.map(|d| d.display().to_string()),
        lenses: state
            .lens
            .get()
            .map(|l| l.db.as_ref().map_or(0, |db| db.lenses.len())),
    }
}

/// Download lensfun's database (or update it), then load it.
#[tauri::command]
async fn fetch_lens_db(state: tauri::State<'_, AppState>) -> Res<LensDbState> {
    let lens = state.lens.clone();
    tauri::async_runtime::spawn_blocking(move || -> Res<()> {
        amb_library::lensdb::fetch(None, false)?;
        let _ = lens.set(LensSources::load_default());
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())??;
    Ok(lens_db_state(state))
}

/// Watch `dirs` (the library's folders and the filmstrip's) for photos
/// added, removed or renamed, and tell the page ("folder-changed", at most
/// about once a second). Sidecars are ignored, so our own saves don't
/// trigger it. An empty list stops watching.
#[tauri::command]
fn watch_folders(
    dirs: Vec<String>,
    recursive: bool,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Res<()> {
    use notify::{EventKind, RecursiveMode, Watcher};
    use std::sync::atomic::{AtomicBool, Ordering};
    let mut slot = state.folder_watch.lock().map_err(|e| e.to_string())?;
    *slot = None;
    if dirs.is_empty() {
        return Ok(());
    }
    let pending = Arc::new(AtomicBool::new(false));
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        let Ok(event) = event else { return };
        let photos = event
            .paths
            .iter()
            .any(|p| amb_engine::raw::is_raw_path(p) || amb_engine::raster::is_raster(p));
        let relevant = matches!(
            event.kind,
            EventKind::Create(_)
                | EventKind::Remove(_)
                | EventKind::Modify(notify::event::ModifyKind::Name(_))
        );
        if photos && relevant && !pending.swap(true, Ordering::SeqCst) {
            let (app, pending) = (app.clone(), pending.clone());
            std::thread::spawn(move || {
                // Let a copy of many files finish first.
                std::thread::sleep(std::time::Duration::from_millis(900));
                pending.store(false, Ordering::SeqCst);
                use tauri::Emitter;
                let _ = app.emit("folder-changed", ());
            });
        }
    })
    .map_err(|e| e.to_string())?;
    let mode = if recursive {
        RecursiveMode::Recursive
    } else {
        RecursiveMode::NonRecursive
    };
    for dir in &dirs {
        // A folder that's gone just isn't watched.
        let _ = watcher.watch(Path::new(dir), mode);
    }
    *slot = Some(watcher);
    Ok(())
}

/// The open photo's film negative analysis as commands that store it with
/// Auto off ("Freeze").
#[tauri::command]
async fn negative_freeze(state: tauri::State<'_, AppState>) -> Res<Vec<Command>> {
    let slot = state.session.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let guard = slot.lock().map_err(|e| e.to_string())?;
        let s = guard.as_ref().ok_or("no photo open")?;
        let frozen = amb_library::negative::frozen(&[s.negative_analysis()]);
        Ok(amb_library::negative::commands(&frozen))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Crop the open scan to its frame; `None` when no border was found.
#[tauri::command]
async fn negative_frame(state: tauri::State<'_, AppState>) -> Res<Option<Vec<Command>>> {
    let slot = state.session.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let guard = slot.lock().map_err(|e| e.to_string())?;
        Ok(guard.as_ref().ok_or("no photo open")?.negative_frame())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Serialize)]
struct RollSync {
    changed: Vec<String>,
    errors: Vec<String>,
    state: Option<session::StateInfo>,
}

/// Give a roll of negatives the same conversion: each frame is analysed,
/// and the median is stored on all of them with Auto off.
#[tauri::command]
async fn negative_sync(paths: Vec<String>, state: tauri::State<'_, AppState>) -> Res<RollSync> {
    let slot = state.session.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let photos: Vec<&Path> = paths.iter().map(Path::new).collect();
        let frames = amb_library::negative::analyze_files(&photos)?;
        let cmds = amb_library::negative::commands(&amb_library::negative::frozen(&frames));
        let mut out = RollSync {
            changed: Vec::new(),
            errors: Vec::new(),
            state: None,
        };
        for path in &paths {
            let mut guard = slot.lock().map_err(|e| e.to_string())?;
            let result = match guard.as_mut().filter(|s| s.path == Path::new(path)) {
                Some(s) => s
                    .execute(&cmds, Some("sync negative roll"))
                    .map(|_| out.state = Some(s.state_info())),
                None => library::paste_to_file(Path::new(path), &cmds),
            };
            match result {
                Ok(()) => out.changed.push(path.clone()),
                Err(e) => out.errors.push(e),
            }
        }
        Ok(out)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Serialize)]
struct Variant {
    seed: u64,
    commands: Vec<Command>,
    /// A small preview, as a `data:` URL.
    image: String,
}

/// Surprise me: `count` random looks from the built-in Random look plugin
/// (seeds `seed`, `seed + 1`, ...), each with a small preview of the photo
/// in it. Nothing is applied.
#[tauri::command]
async fn surprise(
    seed: u64,
    count: usize,
    params: serde_json::Map<String, serde_json::Value>,
    state: tauri::State<'_, AppState>,
) -> Res<Vec<Variant>> {
    let plugin = plugins::find(plugins_dir(state.inner()).as_deref(), "random-look")?;
    let slot = state.session.clone();
    let lens = state.lens.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut guard = slot.lock().map_err(|e| e.to_string())?;
        let s = guard.as_mut().ok_or("open a photo first")?;
        let lens = lens.get().unwrap_or_else(|| empty_lens());
        (0..count.min(12) as u64)
            .map(|i| {
                let seed = seed.wrapping_add(i);
                let ctx = s.plugin_context(&plugin.manifest.permissions, seed);
                let commands = plugin.run(&ctx, &params)?.commands;
                // The look on a copy of the edit, shown as overrides.
                let mut trial = s.state.clone();
                trial.execute_all(&commands).map_err(|e| e.to_string())?;
                let overrides: BTreeMap<String, Value> = trial
                    .settings()
                    .iter()
                    .filter(|(k, v)| s.state.settings().get(*k) != Some(*v))
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                let p = s.preview(240, &overrides, false, false, lens);
                Ok(Variant {
                    seed,
                    commands,
                    image: format!("data:image/jpeg;base64,{}", plugins::base64(&p.jpeg)),
                })
            })
            .collect()
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The recipes the user saved, per plugin.
#[tauri::command]
fn plugin_recipes(state: tauri::State<'_, AppState>) -> Res<PluginRecipes> {
    Ok(state
        .plugin_recipes
        .lock()
        .map_err(|e| e.to_string())?
        .clone())
}

/// Save (or replace, by name) a recipe for a plugin: its parameter values.
#[tauri::command]
fn save_plugin_recipe(
    id: String,
    name: String,
    params: serde_json::Map<String, serde_json::Value>,
    state: tauri::State<'_, AppState>,
) -> Res<PluginRecipes> {
    let plugin = plugins::find(plugins_dir(state.inner()).as_deref(), &id)?;
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("give the recipe a name".into());
    }
    let params = plugin.manifest.resolve_params(&params)?;
    {
        let mut all = state.plugin_recipes.lock().map_err(|e| e.to_string())?;
        let list = all.entry(id).or_default();
        list.retain(|r| !r.name.eq_ignore_ascii_case(&name));
        list.push(amb_plugin::Recipe { name, params });
    }
    save_config(state.inner());
    plugin_recipes(state)
}

#[tauri::command]
fn delete_plugin_recipe(
    id: String,
    name: String,
    state: tauri::State<'_, AppState>,
) -> Res<PluginRecipes> {
    {
        let mut all = state.plugin_recipes.lock().map_err(|e| e.to_string())?;
        if let Some(list) = all.get_mut(&id) {
            list.retain(|r| r.name != name);
        }
    }
    save_config(state.inner());
    plugin_recipes(state)
}

/// Show the plugins folder (or one plugin's folder) in the file manager.
#[tauri::command]
fn open_plugins_folder(
    id: Option<String>,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Res<()> {
    let dir = plugins_dir(state.inner()).ok_or("the plugins folder isn't available")?;
    let dir = match id {
        Some(id) if plugins::builtin(&id).is_none() => dir.join(id),
        _ => dir,
    };
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    app.opener()
        .open_path(dir.display().to_string(), None::<&str>)
        .map_err(|e| e.to_string())
}

/// What sharing a plugin would send, to show before sending.
#[tauri::command]
async fn plugin_share_plan(
    id: String,
    state: tauri::State<'_, AppState>,
) -> Res<plugins::SharePlan> {
    let dir = plugins_dir(state.inner()).ok_or("the plugins folder isn't available")?;
    let index = hub_index(state.inner());
    tauri::async_runtime::spawn_blocking(move || plugins::share_plan(&dir, &id, &index))
        .await
        .map_err(|e| e.to_string())?
}

/// Share a plugin as a pull request to the hub, with the GitHub CLI (gh), or
/// in the browser (GitHub's upload page, with the plugin folder shown).
/// Returns the pull request or upload page address.
#[tauri::command]
async fn share_plugin(
    id: String,
    browser: bool,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Res<String> {
    let dir = plugins_dir(state.inner()).ok_or("the plugins folder isn't available")?;
    let index = hub_index(state.inner());
    let folder = dir.join(&id);
    let url = tauri::async_runtime::spawn_blocking(move || {
        let plan = plugins::share_plan(&dir, &id, &index)?;
        if browser {
            Ok(plan.upload_url)
        } else {
            plugins::share(&plan)
        }
    })
    .await
    .map_err(|e| e.to_string())??;
    if browser {
        let _ = app
            .opener()
            .open_path(folder.display().to_string(), None::<&str>);
    }
    app.opener()
        .open_url(url.clone(), None::<&str>)
        .map_err(|e| e.to_string())?;
    Ok(url)
}

// ---------------------------------------------------------------- library

/// The raws in a folder (and any `more` folders shown with it), with
/// ratings, labels, keywords and capture times.
#[tauri::command]
async fn library_list(
    dir: String,
    more: Option<Vec<String>>,
    subfolders: Option<bool>,
) -> Res<Vec<library::LibraryPhoto>> {
    tauri::async_runtime::spawn_blocking(move || {
        let dirs: Vec<PathBuf> = std::iter::once(dir)
            .chain(more.unwrap_or_default())
            .map(PathBuf::from)
            .collect();
        library::list_many(&dirs, subfolders.unwrap_or(false))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Pick a folder for the library.
#[tauri::command]
async fn library_pick_folder(app: tauri::AppHandle) -> Option<String> {
    app.dialog()
        .file()
        .set_title("Show a folder in the library")
        .blocking_pick_folder()
        .and_then(|p| p.into_path().ok())
        .map(|p| p.display().to_string())
}

#[derive(Serialize)]
struct InfoUpdate {
    path: String,
    info: amb_protocol::sidecar::PhotoInfo,
}

/// Rate, label or keyword many photos: each sidecar is written at once (the
/// open photo's through its session, so the editor stays in step).
#[tauri::command]
async fn library_set_info(
    paths: Vec<String>,
    change: library::InfoChange,
    state: tauri::State<'_, AppState>,
) -> Res<Vec<InfoUpdate>> {
    let slot = state.session.clone();
    let updates = tauri::async_runtime::spawn_blocking(move || -> Res<Vec<InfoUpdate>> {
        let mut out = Vec::new();
        for path in paths {
            let info = {
                let mut guard = slot.lock().map_err(|e| e.to_string())?;
                match guard.as_mut().filter(|s| s.path == Path::new(&path)) {
                    Some(s) => {
                        let new = change.apply(&s.info);
                        s.set_info(&new)?
                    }
                    None => library::change_info(Path::new(&path), &change)?,
                }
            };
            out.push(InfoUpdate { path, info });
        }
        Ok(out)
    })
    .await
    .map_err(|e| e.to_string())??;
    {
        let mut recent = state.recent_keywords.lock().map_err(|e| e.to_string())?;
        for k in updates.iter().flat_map(|u| u.info.keywords.iter()).rev() {
            recent.retain(|r| !r.eq_ignore_ascii_case(k));
            recent.insert(0, k.clone());
        }
        recent.truncate(200);
    }
    save_config(state.inner());
    Ok(updates)
}

/// The setting groups "Paste settings" offers.
#[tauri::command]
fn paste_groups() -> Vec<library::Group> {
    library::paste_groups()
}

#[derive(Serialize)]
struct Pasted {
    pasted: usize,
    errors: Vec<String>,
    /// The open photo's new state, when it was one of the targets.
    state: Option<session::StateInfo>,
}

/// Give `targets` the settings of `source` in `groups` (Lightroom's Sync).
/// The open photo gets them as one undo step; the others in their sidecars.
#[tauri::command]
async fn paste_settings(
    source: String,
    targets: Vec<String>,
    groups: Vec<String>,
    state: tauri::State<'_, AppState>,
) -> Res<Pasted> {
    let slot = state.session.clone();
    tauri::async_runtime::spawn_blocking(move || {
        // The source's edit as it is now (the open photo's unsaved edits too).
        save_open_edits(&slot)?;
        let source_xmp = session::sidecar_path(Path::new(&source));
        let cmds = library::paste_commands(&source_xmp, &groups);
        let mut out = Pasted {
            pasted: 0,
            errors: Vec::new(),
            state: None,
        };
        for target in targets.iter().filter(|t| **t != source) {
            let mut guard = slot.lock().map_err(|e| e.to_string())?;
            let result = match guard.as_mut().filter(|s| s.path == Path::new(target)) {
                Some(s) => s
                    .execute(&cmds, Some("paste settings"))
                    .map(|_| out.state = Some(s.state_info())),
                None => library::paste_to_file(Path::new(target), &cmds),
            };
            match result {
                Ok(()) => out.pasted += 1,
                Err(e) => out.errors.push(e),
            }
        }
        Ok(out)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Clone, Serialize)]
struct LibraryProgress {
    done: usize,
    total: usize,
    name: String,
}

/// Export many photos with the same export choices, each with its own edit,
/// rating and keywords, into a folder the user picks. One failure doesn't
/// stop the rest.
#[tauri::command]
async fn export_photos(
    app: tauri::AppHandle,
    paths: Vec<String>,
    options: ExportOptions,
    state: tauri::State<'_, AppState>,
) -> Res<Option<library::ExportedMany>> {
    use tauri::Emitter;
    let Some(dir) = app
        .dialog()
        .file()
        .set_title("Export the selected photos to…")
        .blocking_pick_folder()
        .and_then(|p| p.into_path().ok())
    else {
        return Ok(None);
    };
    let slot = state.session.clone();
    let lens = state.lens.clone();
    tauri::async_runtime::spawn_blocking(move || {
        save_open_edits(&slot)?;
        let settings = export_settings(&options)?;
        let lens = lens.get().unwrap_or_else(|| empty_lens());
        let template = options
            .name_template
            .as_deref()
            .filter(|t| !t.trim().is_empty());
        if let Some(t) = template {
            amb_library::import::check_template(t)?;
        }
        let out = library::export_many(
            &paths,
            &dir,
            template,
            &settings,
            options.include_tags,
            lens,
            &mut |done, total, name| {
                let _ = app.emit(
                    "library-progress",
                    LibraryProgress {
                        done,
                        total,
                        name: name.to_string(),
                    },
                );
            },
        );
        Ok(Some(out))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The camera profiles found for the open photo's camera; `None` while the
/// profile folders are still being read.
#[tauri::command]
fn camera_profiles(state: tauri::State<'_, AppState>) -> Res<Option<Vec<String>>> {
    let Some(lens) = state.lens.get() else {
        return Ok(None);
    };
    let guard = state.session.lock().map_err(|e| e.to_string())?;
    Ok(Some(
        guard
            .as_ref()
            .map(|s| {
                s.camera_profiles(lens)
                    .into_iter()
                    .map(|f| f.name)
                    .collect()
            })
            .unwrap_or_default(),
    ))
}

/// Make a virtual copy of a photo with its edit as it is now (the open
/// photo's unsaved edit included). Returns the copy's path.
#[tauri::command]
async fn create_virtual_copy(path: String, state: tauri::State<'_, AppState>) -> Res<String> {
    let slot = state.session.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let photo = PathBuf::from(&path);
        let open = {
            let guard = slot.lock().map_err(|e| e.to_string())?;
            guard
                .as_ref()
                .filter(|s| s.path == photo)
                .map(|s| (s.state.settings().clone(), s.info.keywords.clone()))
        };
        let (settings, keywords) = match open {
            Some(found) => found,
            None => {
                let text = std::fs::read_to_string(session::sidecar_path(&photo)).ok();
                let settings = text
                    .as_deref()
                    .and_then(|t| amb_presets::xmp::parse_crs(t).ok())
                    .unwrap_or_default();
                let keywords = text
                    .as_deref()
                    .map(amb_protocol::sidecar::read_info)
                    .unwrap_or_default()
                    .keywords;
                (settings, keywords)
            }
        };
        library::create_virtual_copy(&photo, &settings, keywords).map(|p| p.display().to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Photo Merge > HDR: merge the photos into a DNG next to them, and with
/// `auto_tone` give it the built-in Auto tone (saved to its sidecar).
#[tauri::command]
async fn merge_hdr(
    paths: Vec<String>,
    align: bool,
    deghost: String,
    auto_tone: bool,
) -> Res<amb_library::merge::HdrReport> {
    use amb_library::merge::{self, Deghost, HdrOptions};
    tauri::async_runtime::spawn_blocking(move || {
        let opts = HdrOptions {
            align,
            deghost: Deghost::parse(&deghost).ok_or("unknown deghost level")?,
        };
        let photos: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
        let report = merge::hdr(&photos, &opts, None)?;
        if auto_tone {
            auto_tone_merged(&report.output)?;
        }
        Ok(report)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Photo Merge > Panorama: stitch the photos into a DNG next to them, each
/// corrected with its lens profile first when `lens_corrections` is on, and
/// with `auto_tone` give it the built-in Auto tone.
#[tauri::command]
async fn merge_pano(
    paths: Vec<String>,
    projection: String,
    auto_crop: bool,
    lens_corrections: bool,
    auto_tone: bool,
    state: tauri::State<'_, AppState>,
) -> Res<amb_library::merge::PanoReport> {
    use amb_library::merge::{self, Lenses, PanoOptions, Projection};
    let lens = state.lens.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let opts = PanoOptions {
            projection: Projection::parse(&projection).ok_or("unknown projection")?,
            auto_crop,
        };
        let sources = lens.get().unwrap_or_else(|| empty_lens());
        let lenses = Lenses {
            lcp: sources.lcp.as_ref(),
            db: sources.db.as_ref(),
            correct: lens_corrections,
        };
        let photos: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
        let report = merge::pano(&photos, &opts, None, lenses)?;
        if auto_tone {
            auto_tone_merged(&report.output)?;
        }
        Ok(report)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Give a merged photo the built-in Auto tone, saved to its sidecar (the
/// user asked for it in the merge dialog: no separate approval).
fn auto_tone_merged(path: &Path) -> Res<()> {
    let plugin = plugins::builtin("auto-tone").ok_or("Auto tone is missing")?;
    let mut s = Session::open(path)?;
    let ctx = s.plugin_context(&plugin.manifest.permissions, 1);
    let out = plugin.run(&ctx, &serde_json::Map::new())?;
    if !out.commands.is_empty() {
        s.execute(&out.commands, Some("Auto tone"))?;
        s.save_sidecar()?;
    }
    Ok(())
}

/// Delete a virtual copy (not while it's open in the editor).
#[tauri::command]
fn delete_virtual_copy(path: String, state: tauri::State<'_, AppState>) -> Res<()> {
    let copy = PathBuf::from(&path);
    let open = state
        .session
        .lock()
        .map_err(|e| e.to_string())?
        .as_ref()
        .is_some_and(|s| s.path == copy);
    if open {
        return Err("this copy is open in the editor; open another photo first".into());
    }
    library::delete_virtual_copy(&copy)
}

/// Where collections are kept: the app's config folder.
fn collections_dir(app: &tauri::AppHandle) -> Res<PathBuf> {
    app.path()
        .app_config_dir()
        .map(|d| d.join("collections"))
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn collections_list(app: tauri::AppHandle) -> Res<Vec<amb_library::collections::Summary>> {
    Ok(amb_library::collections::list(&collections_dir(&app)?))
}

#[derive(Serialize)]
struct CollectionView {
    name: String,
    photos: Vec<library::LibraryPhoto>,
    /// Photos moved or deleted since they were added.
    missing: Vec<String>,
}

/// A collection's photos for the library.
#[tauri::command]
async fn collection_photos(app: tauri::AppHandle, name: String) -> Res<CollectionView> {
    let dir = collections_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let (_, c) = amb_library::collections::find(&dir, &name)
            .ok_or_else(|| format!("no collection {name:?}"))?;
        let (photos, missing) = library::list_paths(&c.photos);
        Ok(CollectionView {
            name: c.name,
            photos,
            missing,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

fn summary(c: amb_library::collections::Collection) -> amb_library::collections::Summary {
    c.into()
}

/// Put a collection in a collection set ("" for none).
#[tauri::command]
fn collection_set(
    app: tauri::AppHandle,
    name: String,
    set: String,
) -> Res<amb_library::collections::Summary> {
    amb_library::collections::move_to_set(&collections_dir(&app)?, &name, &set).map(summary)
}

/// Add photos to a collection (made if it doesn't exist).
#[tauri::command]
fn collection_add(
    app: tauri::AppHandle,
    name: String,
    paths: Vec<String>,
) -> Res<amb_library::collections::Summary> {
    let paths: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
    amb_library::collections::add(&collections_dir(&app)?, &name, &paths).map(summary)
}

#[tauri::command]
fn collection_remove(
    app: tauri::AppHandle,
    name: String,
    paths: Vec<String>,
) -> Res<amb_library::collections::Summary> {
    let paths: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
    amb_library::collections::remove(&collections_dir(&app)?, &name, &paths).map(summary)
}

#[tauri::command]
fn collection_rename(
    app: tauri::AppHandle,
    from: String,
    to: String,
) -> Res<amb_library::collections::Summary> {
    amb_library::collections::rename(&collections_dir(&app)?, &from, &to).map(summary)
}

#[tauri::command]
fn collection_delete(app: tauri::AppHandle, name: String) -> Res<()> {
    amb_library::collections::delete(&collections_dir(&app)?, &name)
}

#[tauri::command]
fn smart_list(app: tauri::AppHandle) -> Res<Vec<amb_library::smart::SmartCollection>> {
    Ok(amb_library::smart::all(&collections_dir(&app)?))
}

/// Save a smart collection (replacing `previous`, when renamed).
#[tauri::command]
fn smart_save(
    app: tauri::AppHandle,
    collection: amb_library::smart::SmartCollection,
    previous: Option<String>,
) -> Res<()> {
    let mut c = collection;
    c.format = amb_library::smart::FORMAT.into();
    c.name = c.name.trim().to_string();
    amb_library::smart::save(&collections_dir(&app)?, &c, previous.as_deref())
}

#[tauri::command]
fn smart_delete(app: tauri::AppHandle, name: String) -> Res<()> {
    amb_library::smart::delete(&collections_dir(&app)?, &name)
}

/// A smart collection's photos, worked out now.
#[tauri::command]
async fn smart_photos(app: tauri::AppHandle, name: String) -> Res<CollectionView> {
    let dir = collections_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let (_, s) = amb_library::smart::find(&dir, &name)
            .ok_or_else(|| format!("no smart collection {name:?}"))?;
        Ok(CollectionView {
            photos: library::smart_photos(&s),
            name: s.name,
            missing: Vec::new(),
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Serialize)]
struct ImportDefaults {
    dest: Option<String>,
    options: amb_library::import::ImportOptions,
    /// Template tokens and what they stand for.
    tokens: Vec<(String, String)>,
}

/// The import dialog's starting choices: the last ones used.
#[tauri::command]
fn import_defaults(state: tauri::State<'_, AppState>) -> ImportDefaults {
    let last = state.import_choice.lock().ok().and_then(|c| c.clone());
    ImportDefaults {
        dest: last.as_ref().map(|c| c.dest.clone()),
        options: last.map(|c| c.options).unwrap_or_default(),
        tokens: amb_library::import::TOKENS
            .iter()
            .map(|(t, d)| (t.to_string(), d.to_string()))
            .collect(),
    }
}

/// The Edit in settings.
#[tauri::command]
fn edit_in_settings(state: tauri::State<'_, AppState>) -> Res<EditIn> {
    Ok(state.edit_in.lock().map_err(|e| e.to_string())?.clone())
}

#[tauri::command]
fn set_edit_in(settings: EditIn, state: tauri::State<'_, AppState>) -> Res<EditIn> {
    amb_color::space::OutputSpace::from_id(&settings.color_space)
        .ok_or(format!("unknown colour space {}", settings.color_space))?;
    if !matches!(settings.bits, 8 | 16) {
        return Err("8 or 16 bits".into());
    }
    let settings = EditIn {
        app: settings
            .app
            .map(|a| a.trim().to_string())
            .filter(|a| !a.is_empty()),
        ..settings
    };
    *state.edit_in.lock().map_err(|e| e.to_string())? = settings.clone();
    save_config(state.inner());
    Ok(settings)
}

/// Choose the app to edit in.
#[tauri::command]
async fn pick_edit_app(app: tauri::AppHandle) -> Option<String> {
    let mut dialog = app.dialog().file().set_title("Edit in: choose the app");
    if cfg!(target_os = "macos") {
        dialog = dialog.set_directory("/Applications");
    }
    dialog
        .blocking_pick_file()
        .and_then(|p| p.into_path().ok())
        .map(|p| p.display().to_string())
}

/// After an export, as Lightroom's Post-Processing: show the files in their
/// folder, or open them in the app for Edit in (the system's default when
/// none is set).
#[tauri::command]
fn after_export(
    paths: Vec<String>,
    action: String,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Res<()> {
    if paths.is_empty() {
        return Ok(());
    }
    match action.as_str() {
        "" => Ok(()),
        "reveal" => app
            .opener()
            .reveal_items_in_dir(&paths)
            .map_err(|e| e.to_string()),
        "open" => {
            let with = state.edit_in.lock().ok().and_then(|e| e.app.clone());
            // A batch could start hundreds of windows: the first few.
            for p in paths.iter().take(12) {
                app.opener()
                    .open_path(p, with.clone())
                    .map_err(|e| format!("{p}: {e}"))?;
            }
            Ok(())
        }
        other => Err(format!("unknown after-export action {other:?}")),
    }
}

/// Edit the open photo in another app: write it, as edited, to a TIFF next
/// to it (`IMG_0001-Edit.tif`, then `-Edit-2`, …) and open that there.
/// The TIFF then shows in the folder like any photo.
#[tauri::command]
async fn edit_in(app: tauri::AppHandle, state: tauri::State<'_, AppState>) -> Res<String> {
    let settings = state.edit_in.lock().map_err(|e| e.to_string())?.clone();
    let slot = state.session.clone();
    let lens = state.lens.clone();
    let path = tauri::async_runtime::spawn_blocking(move || -> Res<PathBuf> {
        use amb_export::{BitDepth, ExportSettings, Format, MetadataPolicy};
        let mut s = ExportSettings::new(Format::Tiff);
        s.bit_depth = BitDepth::from_bits(settings.bits);
        s.color_space = amb_color::space::OutputSpace::from_id(&settings.color_space)
            .unwrap_or(amb_color::space::OutputSpace::ProPhoto);
        s.metadata = MetadataPolicy::All;
        let mut guard = slot.lock().map_err(|e| e.to_string())?;
        let session = guard.as_mut().ok_or("no photo open")?;
        let i = &session.info;
        s.tags = amb_export::exif::Tags {
            rating: i.rating,
            keywords: i.keywords.clone(),
            label: i.label.clone(),
            title: i.title.clone(),
            caption: i.caption.clone(),
            creator: i.creator.clone(),
            copyright: i.copyright.clone(),
        };
        let out = session::edit_copy_path(&session.path);
        let lens = lens.get().unwrap_or_else(|| empty_lens());
        session.export(&out, &s, lens)?;
        Ok(out)
    })
    .await
    .map_err(|e| e.to_string())??;
    let with = state.edit_in.lock().ok().and_then(|e| e.app.clone());
    app.opener()
        .open_path(path.display().to_string(), with)
        .map_err(|e| format!("wrote {}, but couldn't open it: {e}", path.display()))?;
    Ok(path.display().to_string())
}

/// Pick a folder (import source or destination).
#[tauri::command]
async fn pick_folder(app: tauri::AppHandle, title: String) -> Option<String> {
    app.dialog()
        .file()
        .set_title(&title)
        .blocking_pick_folder()
        .and_then(|p| p.into_path().ok())
        .map(|p| p.display().to_string())
}

/// Pick a Lightroom Classic catalogue.
#[tauri::command]
async fn pick_lrcat(app: tauri::AppHandle) -> Option<String> {
    app.dialog()
        .file()
        .set_title("Move from Lightroom Classic: choose its catalogue")
        .add_filter("Lightroom Classic catalogues", &["lrcat"])
        .blocking_pick_file()
        .and_then(|p| p.into_path().ok())
        .map(|p| p.display().to_string())
}

fn lrcat_options(from: &str, to: &str) -> amb_library::lrcat::Options {
    let mut options = amb_library::lrcat::Options::default();
    if !from.trim().is_empty() && !to.trim().is_empty() {
        options
            .remap
            .push((from.trim().to_string(), to.trim().to_string()));
    }
    options
}

/// What moving from a Lightroom catalogue would do, nothing written.
#[tauri::command]
async fn lrcat_plan(catalog: String, from: String, to: String) -> Res<serde_json::Value> {
    tauri::async_runtime::spawn_blocking(move || {
        let plan = amb_library::lrcat::plan(Path::new(&catalog), &lrcat_options(&from, &to))?;
        let mut v = serde_json::to_value(&plan).map_err(|e| e.to_string())?;
        v["counts"] = serde_json::json!({
            "write": plan.count("write"),
            "keep": plan.count("keep"),
            "missing": plan.count("missing"),
        });
        Ok(v)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Move from a Lightroom catalogue: sidecars next to the photos, and its
/// collections as ours.
#[tauri::command]
async fn lrcat_apply(
    app: tauri::AppHandle,
    catalog: String,
    from: String,
    to: String,
) -> Res<amb_library::lrcat::Report> {
    let dir = collections_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let plan = amb_library::lrcat::plan(Path::new(&catalog), &lrcat_options(&from, &to))?;
        Ok(amb_library::lrcat::apply(&plan, &dir))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Serialize)]
struct ImportRow {
    source: String,
    dest: String,
    /// Companion files (JPEG, sidecar) that go with it.
    extra: usize,
    captured: String,
    skip: Option<String>,
}

/// What an import would do, nothing copied.
#[tauri::command]
async fn import_plan(
    source: String,
    dest: String,
    options: amb_library::import::ImportOptions,
) -> Res<Vec<ImportRow>> {
    tauri::async_runtime::spawn_blocking(move || {
        let plan = amb_library::import::plan(Path::new(&source), Path::new(&dest), &options)?;
        Ok(plan
            .into_iter()
            .map(|p| ImportRow {
                source: p.files[0].0.display().to_string(),
                dest: p.files[0].1.display().to_string(),
                extra: p.files.len() - 1,
                captured: p.captured,
                skip: p.skip,
            })
            .collect())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Import: plan again (the card may have changed) and copy, with
/// "import-progress" events. The choices are remembered.
#[tauri::command]
async fn import_run(
    app: tauri::AppHandle,
    source: String,
    dest: String,
    options: amb_library::import::ImportOptions,
    state: tauri::State<'_, AppState>,
) -> Res<amb_library::import::Report> {
    use tauri::Emitter;
    if let Ok(mut c) = state.import_choice.lock() {
        *c = Some(ImportChoice {
            dest: dest.clone(),
            options: options.clone(),
        });
    }
    save_config(&state);
    tauri::async_runtime::spawn_blocking(move || {
        let plan = amb_library::import::plan(Path::new(&source), Path::new(&dest), &options)?;
        Ok(amb_library::import::run(
            &plan,
            &options,
            &mut |done, total, name| {
                let _ = app.emit(
                    "import-progress",
                    LibraryProgress {
                        done,
                        total,
                        name: name.to_string(),
                    },
                );
            },
        ))
    })
    .await
    .map_err(|e| e.to_string())?
}

fn main() {
    // `ambrotype --check-look FILE...` validates layout/theme files and exits.
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--check-look") {
        std::process::exit(if looks::check_files(&args[2..]) { 0 } else { 1 });
    }
    let state = AppState::default();
    *state.preset_dirs.lock().expect("fresh mutex") = session::default_preset_dirs();
    // Lens profile sources can take a moment (thousands of files): load them in
    // the background; previews use them once ready.
    let lens = state.lens.clone();
    std::thread::spawn(move || {
        let _ = lens.set(LensSources::load_default());
    });
    // The GPU too (and its kernels compiled); previews use the CPU until then.
    let gpu = state.gpu.clone();
    std::thread::spawn(move || {
        let found = amb_gpu::Gpu::new(false);
        if let Some(g) = &found {
            g.warm_up();
            eprintln!("previews on the GPU: {}", g.name());
        }
        let _ = gpu.set(found);
    });
    *state.initial_path.lock().expect("fresh mutex") = std::env::args().nth(1);
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        // The window opens where and as big as it was left.
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .manage(state)
        .setup(|app| {
            // Load saved settings (added preset folders) from the config folder.
            if let Ok(dir) = app.path().app_config_dir() {
                let path = dir.join("settings.json");
                let config: Config = std::fs::read_to_string(&path)
                    .ok()
                    .and_then(|t| serde_json::from_str(&t).ok())
                    .unwrap_or_default();
                let state = app.state::<AppState>();
                if let Ok(mut dirs) = state.preset_dirs.lock() {
                    for d in config.preset_dirs {
                        if d.is_dir() && !dirs.contains(&d) {
                            dirs.push(d);
                        }
                    }
                }
                if let Ok(mut p) = state.size_presets.lock() {
                    *p = config.size_presets;
                }
                if let Ok(mut l) = state.look.lock() {
                    *l = (config.layout, config.theme);
                }
                if let Ok(mut k) = state.keybindings.lock() {
                    *k = keys::validate(&config.keybindings).unwrap_or_default();
                }
                if let Ok(mut h) = state.hub_url.lock() {
                    *h = config.hub_url;
                }
                if let Ok(mut b) = state.border_presets.lock() {
                    *b = config.border_presets;
                }
                if let Ok(mut k) = state.recent_keywords.lock() {
                    *k = config.recent_keywords;
                }
                if let Ok(mut t) = state.collage_templates.lock() {
                    *t = config.collage_templates;
                }
                if let Ok(mut a) = state.plugin_approvals.lock() {
                    *a = config.plugin_approvals;
                }
                if let Ok(mut r) = state.plugin_recipes.lock() {
                    *r = config.plugin_recipes;
                }
                if let Ok(mut c) = state.import_choice.lock() {
                    *c = config.import;
                }
                if let Ok(mut a) = state.assistant.lock() {
                    *a = config.assistant;
                }
                if let Ok(mut p) = state.export_presets.lock() {
                    *p = config.export_presets;
                }
                if let Ok(mut p) = state.preset_favourites.lock() {
                    *p = config.preset_favourites;
                }
                if let Ok(mut p) = state.mask_effects.lock() {
                    *p = config.mask_effects;
                }
                if let Ok(mut p) = state.edit_in.lock() {
                    *p = config.edit_in;
                }
                if let Ok(mut p) = state.config_path.lock() {
                    *p = Some(path);
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            take_initial_path,
            editor_schema,
            pick_photo,
            pick_references,
            match_look,
            auto_settings,
            surprise,
            open_photo,
            execute,
            preview,
            region,
            region_draft,
            pick_white,
            feedback_body,
            open_feedback,
            save_sidecar,
            pick_export_path,
            export,
            presets,
            add_preset_folder,
            crop_state,
            caption_preview,
            crop_commands,
            mask_frame,
            mask_sliders,
            spot_source,
            size_presets,
            save_size_preset,
            delete_size_preset,
            looks,
            save_look,
            delete_look,
            set_active_look,
            import_look,
            export_look,
            open_looks_folder,
            keybindings,
            set_keybindings,
            settings_info,
            about_text,
            open_source_page,
            open_settings_folder,
            hub_url,
            set_hub_url,
            hub_browse,
            hub_install,
            hub_install_plugin,
            hub_install_preset,
            hub_install_collage,
            hub_install_sizes,
            hub_submit,
            border_presets,
            save_border_preset,
            delete_border_preset,
            photo_info,
            set_photo_info,
            folder_photos,
            thumbnail,
            collage_templates,
            save_collage_template,
            delete_collage_template,
            share_collage_template,
            import_collage_template,
            export_collage,
            export_collage_batch,
            edited_thumbnail,
            save_collage_project,
            open_collage_project,
            pick_photos,
            pick_photo_folder,
            library_list,
            assistant_settings,
            assistant_save,
            assistant_ask,
            run_plugin_many,
            trial_overrides,
            luts_list,
            pick_watermark_image,
            negative_freeze,
            open_assistant_log,
            watch_folders,
            reload_sidecar,
            lens_db_state,
            fetch_lens_db,
            negative_frame,
            guided_upright,
            upright_guides,
            negative_sync,
            export_presets,
            mask_effects,
            save_mask_effect,
            delete_mask_effect,
            preset_favourites,
            set_preset_favourite,
            save_export_preset,
            delete_export_preset,
            pick_lut,
            open_luts_folder,
            library_pick_folder,
            import_defaults,
            collections_list,
            create_virtual_copy,
            merge_hdr,
            merge_pano,
            camera_profiles,
            delete_virtual_copy,
            collection_photos,
            collection_add,
            collection_remove,
            collection_rename,
            collection_set,
            collection_delete,
            smart_list,
            smart_save,
            smart_delete,
            smart_photos,
            pick_folder,
            import_plan,
            import_run,
            edit_in_settings,
            after_export,
            set_edit_in,
            pick_edit_app,
            edit_in,
            pick_lrcat,
            lrcat_plan,
            lrcat_apply,
            library_set_info,
            paste_groups,
            paste_settings,
            export_photos,
            plugins_list,
            plugin_recipes,
            save_plugin_recipe,
            delete_plugin_recipe,
            save_preset,
            install_plugin,
            remove_plugin,
            approve_plugin,
            run_plugin,
            open_plugins_folder,
            plugin_share_plan,
            share_plugin,
        ])
        .build(tauri::generate_context!())
        .expect("error while starting Ambrotype")
        .run(|app, event| {
            // Quitting (closing the window, Cmd+Q) writes the open photo's
            // unsaved edits, and their history, to its sidecar.
            if let tauri::RunEvent::ExitRequested { .. } = event {
                let state = app.state::<AppState>();
                if let Err(e) = save_open_edits(&state.session) {
                    eprintln!("could not save the open photo's edits: {e}");
                }
            }
        });
}

#[cfg(test)]
mod tests {

    #[test]
    fn built_in_mask_effects_use_local_sliders_in_range() {
        let v: serde_json::Value = serde_json::from_str(super::BUILTIN_MASK_EFFECTS).unwrap();
        let effects = v["effects"].as_array().unwrap();
        assert!(effects.len() >= 5);
        for e in effects {
            let values: std::collections::BTreeMap<String, f64> =
                serde_json::from_value(e["values"].clone()).unwrap();
            assert!(!values.is_empty(), "{e}");
            for (tag, value) in values {
                let s = amb_presets::masks::local_slider(&tag).unwrap_or_else(|| panic!("{tag}"));
                assert!(
                    s.rendered && (s.min..=s.max).contains(&value),
                    "{tag} {value}"
                );
            }
        }
    }
    use super::*;

    #[test]
    fn the_assistant_log_gets_one_line_per_edit() {
        let path =
            std::env::temp_dir().join(format!("amb-assistant-log-{}.jsonl", std::process::id()));
        let _ = std::fs::remove_file(&path);
        log_assistant(
            Some(&path),
            serde_json::json!({"edits": "exp=+0.3", "result": "Exposure 0 -> 0.3"}),
        );
        log_assistant(
            Some(&path),
            serde_json::json!({"edits": "nope", "result": "error: unknown"}),
        );
        let text = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<serde_json::Value> = text
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0]["edits"], "exp=+0.3");
        assert!(lines[1]["time"].as_u64().unwrap() > 1_700_000_000);
        log_assistant(None, serde_json::json!({}));
        let _ = std::fs::remove_file(&path);
    }
}
