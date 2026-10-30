// SPDX-License-Identifier: GPL-3.0-or-later
//! The library: folders, thumbnails, batch edits and export, merges, virtual copies.

use crate::*;

/// The raws in the open photo's folder, for the filmstrip.
#[tauri::command]
pub(crate) async fn folder_photos(path: String) -> Res<Vec<folder::FolderPhoto>> {
    tauri::async_runtime::spawn_blocking(move || folder::list(Path::new(&path)))
        .await
        .map_err(|e| e.to_string())?
}

/// A small JPEG of a raw for the filmstrip (cached in the app's cache folder).
#[tauri::command]
pub(crate) async fn thumbnail(
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

/// Photos among `paths` that are the same file, alike, or one burst
/// (`amb_library::duplicates`), best first in each group. Progress goes to
/// the page as "duplicates-progress" ([done, total]).
#[tauri::command]
pub(crate) async fn library_find_duplicates(
    paths: Vec<String>,
    burst_seconds: f64,
    max_bits: u32,
    app: tauri::AppHandle,
) -> Res<Vec<amb_library::duplicates::Group>> {
    use tauri::Emitter;
    let cache = app
        .path()
        .app_cache_dir()
        .ok()
        .map(|d| d.join("thumbnails"));
    tauri::async_runtime::spawn_blocking(move || {
        let photos: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
        // The camera's preview, or the thumbnail the library shows.
        let thumb = |p: &Path| {
            amb_engine::raw::embedded_thumbnail(p, 256)
                .ok()
                .flatten()
                .or_else(|| {
                    let jpeg = folder::thumbnail(p, 256, cache.as_deref()).ok()?;
                    amb_library::duplicates::decode_jpeg(&jpeg)
                })
        };
        let last = std::sync::Mutex::new(std::time::Instant::now());
        let progress = |done: usize, total: usize| {
            let mut last = last.lock().unwrap_or_else(|e| e.into_inner());
            if done == total || last.elapsed().as_millis() > 200 {
                *last = std::time::Instant::now();
                let _ = app.emit("duplicates-progress", [done, total]);
            }
        };
        let options = amb_library::duplicates::Options {
            burst_seconds,
            max_bits: max_bits.min(20),
        };
        Ok(amb_library::duplicates::find(
            folder::catalog(),
            &photos,
            options,
            &thumb,
            &progress,
        ))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Watch `dirs` (the library's folders and the filmstrip's) for photos
/// added, removed or renamed, and tell the page ("folder-changed", at most
/// about once a second). Sidecars are ignored, so our own saves don't
/// trigger it. An empty list stops watching.
#[tauri::command]
pub(crate) fn watch_folders(
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

#[derive(Serialize)]
pub(crate) struct RollSync {
    pub(crate) changed: Vec<String>,
    pub(crate) errors: Vec<String>,
    pub(crate) state: Option<session::StateInfo>,
    /// How many scenes the roll had, and how many frames kept their own.
    pub(crate) scenes: usize,
    pub(crate) odd: usize,
}

/// Give a roll of negatives one conversion: each frame is analysed, and
/// its scene's baseline (the median, or a chosen frame) is stored on it
/// with Auto off; odd frames keep their own (`amb_library::negative::roll`).
#[tauri::command]
pub(crate) async fn negative_sync(
    paths: Vec<String>,
    options: Option<amb_library::negative::RollOptions>,
    state: tauri::State<'_, AppState>,
) -> Res<RollSync> {
    let slot = state.session.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let photos: Vec<&Path> = paths.iter().map(Path::new).collect();
        let frames = amb_library::negative::analyze_files(&photos)?;
        let plan = amb_library::negative::roll(&frames, &options.unwrap_or_default());
        let mut out = RollSync {
            changed: Vec::new(),
            errors: Vec::new(),
            state: None,
            scenes: plan.iter().map(|f| f.scene + 1).max().unwrap_or(0),
            odd: plan.iter().filter(|f| f.odd).count(),
        };
        for (path, frame) in paths.iter().zip(&plan) {
            let cmds = amb_library::negative::commands(&frame.settings);
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

/// The raws in a folder (and any `more` folders shown with it), with
/// ratings, labels, keywords and capture times.
#[tauri::command]
pub(crate) async fn library_list(
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
pub(crate) async fn library_pick_folder(app: tauri::AppHandle) -> Option<String> {
    app.dialog()
        .file()
        .set_title("Show a folder in the library")
        .blocking_pick_folder()
        .and_then(|p| p.into_path().ok())
        .map(|p| p.display().to_string())
}

#[derive(Serialize)]
pub(crate) struct InfoUpdate {
    pub(crate) path: String,
    pub(crate) info: amb_protocol::sidecar::PhotoInfo,
}

/// Rate, label or keyword many photos: each sidecar is written at once (the
/// open photo's through its session, so the editor stays in step).
#[tauri::command]
pub(crate) async fn library_set_info(
    paths: Vec<String>,
    change: library::InfoChange,
    state: tauri::State<'_, AppState>,
) -> Res<Vec<InfoUpdate>> {
    let slot = state.session.clone();
    let updates = tauri::async_runtime::spawn_blocking(move || -> Res<Vec<InfoUpdate>> {
        let mut out = Vec::new();
        for path in paths {
            let info = change_one(&slot, &path, &change)?;
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

/// Change one photo's info: through its session when it is the open photo
/// (so the editor stays in step), in its sidecar otherwise.
pub(crate) fn change_one(
    slot: &Mutex<Option<Session>>,
    path: &str,
    change: &library::InfoChange,
) -> Res<amb_protocol::sidecar::PhotoInfo> {
    let mut guard = slot.lock().map_err(|e| e.to_string())?;
    match guard.as_mut().filter(|s| s.path == Path::new(path)) {
        Some(s) => {
            let new = change.apply(&s.info);
            s.set_info(&new)
        }
        None => library::change_info(Path::new(path), change),
    }
}

/// The setting groups "Paste settings" offers.
#[tauri::command]
pub(crate) fn paste_groups() -> Vec<library::Group> {
    library::paste_groups()
}

#[derive(Serialize)]
pub(crate) struct Pasted {
    pub(crate) pasted: usize,
    pub(crate) errors: Vec<String>,
    /// The open photo's new state, when it was one of the targets.
    pub(crate) state: Option<session::StateInfo>,
}

/// Give `targets` the settings of `source` in `groups` (Lightroom's Sync).
/// The open photo gets them as one undo step; the others in their sidecars.
#[tauri::command]
pub(crate) async fn paste_settings(
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
pub(crate) struct LibraryProgress {
    pub(crate) done: usize,
    pub(crate) total: usize,
    pub(crate) name: String,
}

/// Export many photos with the same export choices, each with its own edit,
/// rating and keywords, into a folder the user picks. One failure doesn't
/// stop the rest.
#[tauri::command]
pub(crate) async fn export_photos(
    app: tauri::AppHandle,
    paths: Vec<String>,
    options: ExportOptions,
    dir: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Res<Option<library::ExportedMany>> {
    use tauri::Emitter;
    let mut dialog = app.dialog().file();
    // Where the last export went.
    if let Some(d) = dir.filter(|d| Path::new(d).is_dir()) {
        dialog = dialog.set_directory(d);
    }
    let Some(dir) = dialog
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

/// Make a virtual copy of a photo with its edit as it is now (the open
/// photo's unsaved edit included). Returns the copy's path.
#[tauri::command]
pub(crate) async fn create_virtual_copy(
    path: String,
    state: tauri::State<'_, AppState>,
) -> Res<String> {
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
pub(crate) async fn merge_hdr(
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
pub(crate) async fn merge_pano(
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
pub(crate) fn auto_tone_merged(path: &Path) -> Res<()> {
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
pub(crate) fn delete_virtual_copy(path: String, state: tauri::State<'_, AppState>) -> Res<()> {
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

/// Pick a folder (import source or destination).
#[tauri::command]
pub(crate) async fn pick_folder(app: tauri::AppHandle, title: String) -> Option<String> {
    app.dialog()
        .file()
        .set_title(&title)
        .blocking_pick_folder()
        .and_then(|p| p.into_path().ok())
        .map(|p| p.display().to_string())
}

/// The contact sheet dialog's choices.
#[derive(Debug, Deserialize)]
pub(crate) struct ContactOptions {
    style: String,
    columns: usize,
    paper: String,
    landscape: bool,
    title: String,
    format: String,
}

/// Contact sheets of `paths` (`amb_library::contact`), saved where the user
/// picks; the pages written, `None` when they cancel.
#[tauri::command]
pub(crate) async fn contact_sheet(
    app: tauri::AppHandle,
    paths: Vec<String>,
    options: ContactOptions,
    dir: Option<String>,
) -> Res<Option<Vec<String>>> {
    use amb_export::contact::{Sheet, Style, paper};
    let format = amb_export::Format::from_name(&options.format).ok_or("unknown format")?;
    let name = format!("contact-sheet.{}", format.extension());
    let out = if let Some(t) = crate::uitest::current() {
        Some(t.out_dir.join(&name))
    } else {
        let mut dialog = app.dialog().file();
        if let Some(d) = dir.filter(|d| Path::new(d).is_dir()) {
            dialog = dialog.set_directory(d);
        }
        dialog
            .set_title("Save the contact sheet")
            .set_file_name(&name)
            .add_filter(format.extension().to_uppercase(), &[format.extension()])
            .blocking_save_file()
            .and_then(|p| p.into_path().ok())
    };
    let Some(out) = out else {
        return Ok(None);
    };
    let (width, height) = paper(&options.paper).ok_or("unknown paper size")?;
    let mut sheet = Sheet {
        width,
        height,
        columns: options.columns.clamp(1, 20),
        style: Style::from_name(&options.style).ok_or("unknown style")?,
        title: options.title.trim().to_string(),
    };
    if options.landscape {
        (sheet.width, sheet.height) = (sheet.height, sheet.width);
    }
    let photos: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
    tauri::async_runtime::spawn_blocking(move || {
        let export = amb_export::ExportSettings::new(format);
        let pages = amb_library::contact::write(&photos, &sheet, &export, &out)?;
        Ok(Some(
            pages.iter().map(|p| p.display().to_string()).collect(),
        ))
    })
    .await
    .map_err(|e| e.to_string())?
}
