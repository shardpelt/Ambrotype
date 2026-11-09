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
mod config;
mod folder;
mod hub;
mod interface;
mod keys;
mod library;
mod looks;
mod monitor;
mod plugins;
mod preset_files;
mod schema;
mod session;
mod sizes;
mod slideshow;
mod state;
mod uitest;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use amb_presets::Value;
use amb_protocol::Command;
use preset_files::PresetEntry;
use serde::{Deserialize, Serialize};
use session::{CropRect, CropState, LensSources, Session};
use sizes::SizePreset;
use tauri::Manager;
use tauri::ipc::Response;
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

// Helpers the command modules share with each other.
use commands::appearance::*;
use commands::batch::*;
use commands::collage::*;
use commands::export::*;
use commands::hub::*;
use commands::plugins::*;
use commands::presets::*;
use config::{BUILTIN_MASK_EFFECTS, Config, EditIn, ImportChoice, MaskEffect, save_config};
use state::{AppState, empty_lens, save_open_edits};

/// Recipes the user saved, per plugin id.
type PluginRecipes = std::collections::BTreeMap<String, Vec<amb_plugin::Recipe>>;

type Res<T> = Result<T, String>;

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

fn main() {
    // `ambrotype --check-look FILE...` validates layout/theme files and exits.
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--check-look") {
        std::process::exit(if looks::check_files(&args[2..]) { 0 } else { 1 });
    }
    // `ambrotype --check-hub HUB_FOLDER` installs every hub item in a scratch
    // folder, as the app would, and exits (the hub's CI).
    if args.get(1).map(String::as_str) == Some("--check-hub") {
        let Some(hub) = args.get(2) else {
            eprintln!("--check-hub needs the hub's folder");
            std::process::exit(2);
        };
        std::process::exit(if hub::check_hub(std::path::Path::new(hub)) {
            0
        } else {
            1
        });
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
    // `ambrotype --slideshow ... PHOTO|DIR...` (slideshow.rs).
    if let Some(show) = slideshow::take(&mut rest) {
        *slideshow::PENDING.lock().expect("fresh mutex") = Some(show);
    }
    let state = AppState::default();
    *state.preset_dirs.lock().expect("fresh mutex") = preset_files::default_preset_dirs();
    // Lens profile sources can take a moment (thousands of files): load them in
    // the background; previews use them once ready.
    let lens = state.lens.clone();
    std::thread::spawn(move || {
        let _ = lens.set(LensSources::load_default());
    });
    // The GPU too (and its kernels compiled); previews use the CPU until then.
    // Exports use the same one (`amb_gpu::full::shared`).
    let gpu = state.gpu.clone();
    std::thread::spawn(move || {
        let found = amb_gpu::full::shared();
        if let Some(g) = found {
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
        .plugin(tauri_plugin_clipboard_manager::init())
        // The window opens where and as big as it was left.
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .manage(state)
        // A custom interface's files (interface.rs).
        .register_uri_scheme_protocol(interface::SCHEME, |_ctx, request| {
            interface::serve(request.uri().path())
        })
        // The menu bar's items run their action in the UI (`ui/core/menu.js`).
        .on_menu_event(|app, event| {
            use tauri::Emitter;
            // A way back that works whatever the custom interface does.
            if event.id().as_ref() == "builtin" {
                let state = app.state::<AppState>();
                let _ = state.change_prefs(|p| p.interface = None);
                let _ = commands::settings::show_interface(app, None);
                return;
            }
            let _ = app.emit("menu", event.id().as_ref());
        })
        .setup(|app| {
            // Lightroom catalogues opened in place: their photos' edits are
            // read from them from the start.
            if let Ok(dir) = app.path().app_config_dir() {
                commands::catalog::lightroom_at(&dir);
            }
            // Load saved settings (added preset folders) from the config folder.
            if let Ok(dir) = app.path().app_config_dir() {
                let path = dir.join("settings.json");
                let config = config::read_config(&path);
                let state = app.state::<AppState>();
                let interface = config::apply_config(&state, config);
                // A custom interface, unless this run keeps to the built-in one.
                if let Some(dir) = interface {
                    if !interface::builtin_forced() {
                        let _ = commands::settings::show_interface(app.handle(), Some(dir));
                    }
                }
                // Select Subject: the model in the models folder, fetched on first use.
                if let Some(models) = path.parent().map(|d| d.join("models")) {
                    amb_engine::subject::set_finder(amb_ai::finder(models.clone()));
                    amb_engine::subject::set_selector(amb_ai::objects::selector(models.clone()));
                    amb_engine::subject::set_face_finder(amb_ai::faces::finder(models.clone()));
                    amb_engine::retouch::set_inpainter(amb_ai::remove::inpainter(models.clone()));
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
        .invoke_handler(commands::handler())
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
    /// The UI's files with this extension, in `ui/` and its subfolders, with
    /// their paths from `ui/` (`library/grid.js`).
    fn ui_files(ext: &str) -> Vec<(String, std::path::PathBuf)> {
        let ui = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("ui");
        let mut dirs = vec![ui.clone()];
        let mut files = Vec::new();
        while let Some(dir) = dirs.pop() {
            for entry in std::fs::read_dir(&dir).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    dirs.push(path);
                } else if path.extension().is_some_and(|x| x == ext) {
                    let rel = path
                        .strip_prefix(&ui)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/");
                    files.push((rel, path));
                }
            }
        }
        files.sort();
        files
    }

    /// Two elements with one id: `$(id)` finds only the first, so the
    /// other's code silently acts on the wrong element.
    #[test]
    fn page_ids_are_unique() {
        let page = include_str!("../ui/index.html");
        let mut seen = std::collections::BTreeSet::new();
        let mut twice = Vec::new();
        for part in page.split(" id=\"").skip(1) {
            let id = part.split('"').next().unwrap_or_default();
            if !seen.insert(id) {
                twice.push(id);
            }
        }
        assert!(twice.is_empty(), "ids used twice in index.html: {twice:?}");
    }

    /// Every script in `ui/` is loaded by the page, once, and every script
    /// the page loads exists: a file split in two whose new half isn't in
    /// index.html would leave its functions undefined.
    #[test]
    fn every_ui_script_is_loaded_once() {
        let page = include_str!("../ui/index.html");
        let mut loaded = Vec::new();
        for part in page.split("<script src=\"").skip(1) {
            loaded.push(part.split('"').next().unwrap_or_default().to_string());
        }
        let files: Vec<String> = ui_files("js").into_iter().map(|(rel, _)| rel).collect();
        for file in &files {
            let times = loaded.iter().filter(|l| *l == file).count();
            assert_eq!(times, 1, "{file} is loaded {times} times by index.html");
        }
        for script in &loaded {
            assert!(
                files.contains(script),
                "index.html loads {script}, which isn't in ui/"
            );
        }
    }

    /// The UI's scripts share one global scope (plain `<script>` files), so
    /// two top-level functions or variables with one name would silently
    /// replace each other.
    #[test]
    fn ui_scripts_declare_each_top_level_name_once() {
        let mut seen = std::collections::BTreeMap::<String, String>::new();
        let mut clashes = Vec::new();
        for (file, path) in ui_files("js") {
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
    /// written with them never asks. askYes() (core.js) is the way.
    #[test]
    fn ui_scripts_ask_with_ask_yes() {
        for (_, path) in ui_files("js") {
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
