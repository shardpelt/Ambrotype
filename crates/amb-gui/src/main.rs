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
mod commands;
mod folder;
mod hub;
mod interface;
mod keys;
mod library;
mod looks;
mod monitor;
mod plugins;
mod schema;
mod session;
mod sizes;
mod uitest;

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

// Helpers the command modules share with each other.
use commands::appearance::*;
use commands::collage::*;
use commands::export::*;
use commands::hub::*;
use commands::library::*;
use commands::plugins::*;
use commands::presets::*;

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
    /// Metadata templates (library → Organise).
    metadata_templates: Mutex<Vec<commands::files::MetadataTemplate>>,
    /// The monitor profile choice (Settings).
    monitor: Mutex<monitor::Choice>,
    /// Soft proofing (for this run): the space, and the gamut warning.
    proof: Mutex<Option<(amb_color::space::OutputSpace, bool)>>,
    /// Watches the folders on screen for photos added, removed or renamed.
    folder_watch: Mutex<Option<notify::RecommendedWatcher>>,
    /// Watches the open photo's sidecar for changes made outside the editor.
    sidecar_watch: Mutex<Option<notify::RecommendedWatcher>>,
    assistant: Mutex<assistant::Provider>,
    /// The assistant's key for this run, when the keychain can't keep it.
    assistant_key: Mutex<Option<String>>,
    /// The custom interface chosen in Settings (a folder), if any.
    interface: Mutex<Option<PathBuf>>,
    /// The menu bar's items by action id, to grey them out.
    menu_items: Mutex<std::collections::HashMap<String, tauri::menu::MenuItem<tauri::Wry>>>,
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
    #[serde(default)]
    metadata_templates: Vec<commands::files::MetadataTemplate>,
    /// Which monitor profile the preview is shown in.
    #[serde(default)]
    monitor: monitor::Choice,
    /// A custom interface folder shown instead of the built-in one.
    #[serde(default)]
    interface: Option<PathBuf>,
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
        monitor: state.monitor.lock().map(|m| m.clone()).unwrap_or_default(),
        metadata_templates: state
            .metadata_templates
            .lock()
            .map(|m| m.clone())
            .unwrap_or_default(),
        interface: state.interface.lock().ok().and_then(|i| i.clone()),
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

/// Collages read each photo's edits from its sidecar: write the open photo's
/// unsaved edits there first.
fn save_open_edits(slot: &Mutex<Option<Session>>) -> Res<()> {
    let mut guard = slot.lock().map_err(|e| e.to_string())?;
    if let Some(s) = guard.as_mut().filter(|s| s.has_unsaved_edits()) {
        s.save_sidecar()?;
    }
    Ok(())
}

fn main() {
    // `ambrotype --check-look FILE...` validates layout/theme files and exits.
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--check-look") {
        std::process::exit(if looks::check_files(&args[2..]) { 0 } else { 1 });
    }
    // `ambrotype --ui-test SCRIPT.js [PHOTO]` runs a scripted UI test (uitest.rs).
    let mut rest: Vec<String> = args.iter().skip(1).cloned().collect();
    if let Some(i) = rest.iter().position(|a| a == "--ui-test") {
        let Some(script) = rest.get(i + 1).cloned() else {
            eprintln!("--ui-test needs a script");
            std::process::exit(2);
        };
        if let Err(e) = uitest::start(&script) {
            eprintln!("{e}");
            std::process::exit(2);
        }
        rest.drain(i..i + 2);
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
    *state.initial_path.lock().expect("fresh mutex") =
        rest.into_iter().find(|a| !a.starts_with("--"));
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        // The window opens where and as big as it was left.
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .manage(state)
        // A custom interface's files (interface.rs).
        .register_uri_scheme_protocol(interface::SCHEME, |_ctx, request| {
            interface::serve(request.uri().path())
        })
        // The menu bar's items run their action in the UI (`ui/menu.js`).
        .on_menu_event(|app, event| {
            use tauri::Emitter;
            // A way back that works whatever the custom interface does.
            if event.id().as_ref() == "builtin" {
                let state = app.state::<AppState>();
                if let Ok(mut i) = state.interface.lock() {
                    *i = None;
                }
                save_config(&state);
                let _ = commands::settings::show_interface(app, None);
                return;
            }
            let _ = app.emit("menu", event.id().as_ref());
        })
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
                if let Ok(mut m) = state.monitor.lock() {
                    *m = config.monitor;
                }
                if let Ok(mut m) = state.metadata_templates.lock() {
                    *m = config.metadata_templates;
                }
                // A custom interface, unless this run keeps to the built-in one.
                if let Some(dir) = config.interface.filter(|d| interface::is_interface(d)) {
                    if let Ok(mut i) = state.interface.lock() {
                        *i = Some(dir.clone());
                    }
                    if !interface::builtin_forced() {
                        let _ = commands::settings::show_interface(app.handle(), Some(dir));
                    }
                }
                // Select Subject: the model in the models folder, fetched on first use.
                if let Some(models) = path.parent().map(|d| d.join("models")) {
                    amb_engine::subject::set_finder(amb_ai::finder(models.clone()));
                    // Fetched in the background at the first start, so the
                    // first subject mask doesn't wait for it.
                    std::thread::spawn(move || {
                        if let Err(e) = amb_ai::ensure_model(&models) {
                            eprintln!("{e}");
                        }
                    });
                }
                // Effect plugins render from the plugins folder next to the settings.
                let plugins = path.parent().map(|d| d.join("plugins"));
                amb_engine::plugin_effects::set_resolver(amb_plugin::effect::resolver(move |id| {
                    amb_plugin::find(plugins.as_deref(), id)
                }));
                if let Ok(mut p) = state.config_path.lock() {
                    *p = Some(path);
                }
                // The thumbnail cache kept within its size, out of the way of the start.
                if let Ok(cache) = app.path().app_cache_dir() {
                    std::thread::spawn(move || {
                        let (n, bytes) = folder::prune_cache(
                            &cache.join("thumbnails"),
                            folder::THUMB_CACHE_BYTES,
                        );
                        if n > 0 {
                            eprintln!("thumbnail cache: let {n} go ({} MB)", bytes >> 20);
                        }
                    });
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::photo::take_initial_path,
            uitest::ui_test,
            uitest::ui_test_done,
            commands::menu::set_menu,
            commands::settings::interfaces,
            commands::settings::set_interface,
            commands::settings::pick_interface_folder,
            commands::settings::monitor_profile,
            commands::settings::set_monitor_profile,
            commands::settings::pick_monitor_profile,
            commands::settings::open_interfaces_folder,
            commands::connect::ai_connect_info,
            commands::connect::open_terminal,
            commands::sample::sample_photo,
            commands::settings::interface_share_plan,
            commands::settings::share_interface,
            commands::menu::menu_enable,
            commands::photo::editor_schema,
            commands::photo::pick_photo,
            commands::presets::pick_references,
            commands::presets::match_look,
            commands::presets::auto_settings,
            commands::presets::surprise,
            commands::photo::open_photo,
            commands::photo::execute,
            commands::preview::preview,
            commands::preview::region,
            commands::preview::region_draft,
            commands::photo::pick_white,
            commands::settings::feedback_body,
            commands::settings::open_feedback,
            commands::models::ai_models,
            commands::models::ai_model_install,
            commands::models::ai_model_use,
            commands::models::ai_model_remove,
            commands::models::pick_model_file,
            commands::models::ai_model_add,
            commands::photo::save_sidecar,
            commands::export::pick_export_path,
            commands::export::export,
            commands::presets::presets,
            commands::presets::add_preset_folder,
            commands::photo::crop_state,
            commands::export::caption_preview,
            commands::export::fonts,
            commands::export::font_data,
            commands::photo::crop_commands,
            commands::photo::mask_frame,
            commands::photo::mask_sliders,
            commands::photo::spot_source,
            commands::presets::size_presets,
            commands::presets::save_size_preset,
            commands::presets::delete_size_preset,
            commands::appearance::looks,
            commands::appearance::save_look,
            commands::appearance::delete_look,
            commands::appearance::set_active_look,
            commands::appearance::setups_list,
            commands::appearance::setup_save,
            commands::appearance::setup_restore,
            commands::appearance::setup_delete,
            commands::appearance::import_look,
            commands::appearance::export_look,
            commands::appearance::open_looks_folder,
            commands::settings::keybindings,
            commands::settings::set_keybindings,
            commands::settings::settings_info,
            commands::settings::about_text,
            commands::settings::open_source_page,
            commands::settings::open_settings_folder,
            commands::hub::hub_url,
            commands::hub::set_hub_url,
            commands::hub::hub_browse,
            commands::hub::hub_install,
            commands::hub::hub_install_link,
            commands::hub::hub_install_plugin,
            commands::hub::hub_install_interface,
            commands::hub::hub_install_preset,
            commands::hub::preset_library,
            commands::presets::preset_thumbnails,
            commands::hub::library_install_preset,
            commands::hub::hub_install_collage,
            commands::hub::hub_install_border,
            commands::hub::hub_install_sizes,
            commands::hub::hub_submit,
            commands::export::border_presets,
            commands::export::share_border_preset,
            commands::export::import_border_preset,
            commands::export::save_border_preset,
            commands::export::delete_border_preset,
            commands::photo::photo_info,
            commands::photo::set_photo_info,
            commands::library::folder_photos,
            commands::library::thumbnail,
            commands::collage::collage_templates,
            commands::collage::save_collage_template,
            commands::collage::delete_collage_template,
            commands::collage::share_collage_template,
            commands::collage::import_collage_template,
            commands::collage::export_collage,
            commands::collage::export_collage_batch,
            commands::collage::edited_thumbnail,
            commands::collage::save_collage_project,
            commands::collage::open_collage_project,
            commands::photo::pick_photos,
            commands::photo::pick_photo_folder,
            commands::library::library_list,
            commands::assistant::assistant_settings,
            commands::assistant::assistant_save,
            commands::assistant::assistant_ask,
            commands::plugins::run_plugin_many,
            commands::photo::trial_overrides,
            commands::presets::luts_list,
            commands::export::pick_watermark_image,
            commands::photo::negative_freeze,
            commands::assistant::open_assistant_log,
            commands::library::watch_folders,
            commands::photo::reload_sidecar,
            commands::photo::lens_db_state,
            commands::photo::fetch_lens_db,
            commands::photo::negative_frame,
            commands::photo::guided_upright,
            commands::photo::upright_guides,
            commands::library::negative_sync,
            commands::presets::apply_to_photos,
            commands::export::export_presets,
            commands::presets::mask_effects,
            commands::presets::save_mask_effect,
            commands::presets::delete_mask_effect,
            commands::presets::preset_favourites,
            commands::presets::set_preset_favourite,
            commands::export::save_export_preset,
            commands::export::delete_export_preset,
            commands::presets::pick_lut,
            commands::presets::open_luts_folder,
            commands::library::library_pick_folder,
            commands::catalog::import_defaults,
            commands::catalog::collections_list,
            commands::library::create_virtual_copy,
            commands::library::merge_hdr,
            commands::library::merge_pano,
            commands::photo::camera_profiles,
            commands::library::delete_virtual_copy,
            commands::catalog::collection_photos,
            commands::catalog::collection_add,
            commands::catalog::collection_remove,
            commands::catalog::collection_rename,
            commands::catalog::collection_set,
            commands::catalog::collection_delete,
            commands::catalog::smart_list,
            commands::catalog::smart_save,
            commands::catalog::smart_delete,
            commands::catalog::smart_photos,
            commands::catalog::catalog_photos,
            commands::catalog::catalog_forget,
            commands::catalog::library_folders,
            commands::catalog::add_library_folder,
            commands::catalog::remove_library_folder,
            commands::catalog::index_library,
            commands::library::pick_folder,
            commands::catalog::import_plan,
            commands::catalog::import_run,
            commands::export::edit_in_settings,
            commands::preview::set_proof,
            commands::export::after_export,
            commands::export::set_edit_in,
            commands::export::pick_edit_app,
            commands::export::edit_in,
            commands::catalog::pick_lrcat,
            commands::catalog::lrcat_plan,
            commands::catalog::lrcat_apply,
            commands::library::library_set_info,
            commands::library::library_find_duplicates,
            commands::files::files_plan,
            commands::files::files_apply,
            commands::files::files_trash,
            commands::files::files_missing,
            commands::files::files_find_moved,
            commands::files::keywords_all,
            commands::files::keywords_change,
            commands::files::metadata_templates,
            commands::files::save_metadata_template,
            commands::places::pick_gpx,
            commands::places::open_osm_copyright,
            commands::places::library_match_gpx,
            commands::library::paste_groups,
            commands::library::paste_settings,
            commands::library::export_photos,
            commands::plugins::plugins_list,
            commands::plugins::plugin_recipes,
            commands::plugins::save_plugin_recipe,
            commands::plugins::delete_plugin_recipe,
            commands::presets::save_preset,
            commands::plugins::install_plugin,
            commands::plugins::remove_plugin,
            commands::plugins::approve_plugin,
            commands::plugins::run_plugin,
            commands::plugins::open_plugins_folder,
            commands::plugins::plugin_share_plan,
            commands::plugins::share_plugin,
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
            // Moved to another monitor: show the preview in its colours.
            if let tauri::RunEvent::WindowEvent {
                event: tauri::WindowEvent::Moved(_),
                ..
            } = event
            {
                use tauri::Emitter;
                let (status, changed) = commands::settings::recheck_monitor(app);
                if changed {
                    let _ = app.emit("monitor-changed", status);
                }
            }
        });
}

#[cfg(test)]
mod tests {
    /// The UI's scripts share one global scope (plain `<script>` files), so
    /// two top-level functions or variables with one name would silently
    /// replace each other.
    #[test]
    fn ui_scripts_declare_each_top_level_name_once() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("ui");
        let mut seen = std::collections::BTreeMap::<String, String>::new();
        let mut clashes = Vec::new();
        for entry in std::fs::read_dir(&dir).unwrap().flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|x| x != "js") {
                continue;
            }
            let file = path.file_name().unwrap().to_string_lossy().to_string();
            for line in std::fs::read_to_string(&path).unwrap().lines() {
                let rest = line.strip_prefix("async ").unwrap_or(line);
                let name = ["function ", "const ", "let ", "var ", "class "]
                    .iter()
                    .find_map(|k| rest.strip_prefix(k))
                    .map(|r| {
                        r.chars()
                            .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$')
                            .collect::<String>()
                    })
                    .filter(|n| !n.is_empty());
                if let Some(name) = name {
                    if let Some(first) = seen.insert(name.clone(), file.clone()) {
                        clashes.push(format!("{name}: {first} and {file}"));
                    }
                }
            }
        }
        assert!(clashes.is_empty(), "{clashes:?}");
    }

    /// The dialog plugin replaces the page's confirm() and alert() with
    /// calls that don't block (confirm()'s even fails), so a "Delete…?" guard
    /// written with them never asks. askYes() (app.js) is the way.
    #[test]
    fn ui_scripts_ask_with_ask_yes() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("ui");
        for entry in std::fs::read_dir(&dir).unwrap().flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|x| x != "js") {
                continue;
            }
            for (i, line) in std::fs::read_to_string(&path).unwrap().lines().enumerate() {
                let code = line.split("//").next().unwrap_or("");
                for call in ["confirm(", "alert(", "prompt("] {
                    let bare = code.match_indices(call).any(|(at, _)| {
                        !code[..at].ends_with(|c: char| c.is_alphanumeric() || c == '.' || c == '_')
                    });
                    assert!(!bare, "{}:{}: {call}", path.display(), i + 1);
                }
            }
        }
    }

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
}
