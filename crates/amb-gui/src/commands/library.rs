// SPDX-License-Identifier: GPL-3.0-or-later
//! The library: folders, thumbnails, info edits, virtual copies.

use crate::*;

/// The raws in the open photo's folder, for the filmstrip.
#[tauri::command]
pub(crate) async fn folder_photos(path: String) -> Res<Vec<folder::FolderPhoto>> {
    tauri::async_runtime::spawn_blocking(move || folder::list(Path::new(&path)))
        .await
        .map_err(|e| e.to_string())?
}

/// One photo as the viewer's info panel shows it: the library's record (its
/// rating, label, flag, keywords, title, and the shot from its header) and
/// the file's size.
#[derive(Serialize)]
pub(crate) struct PhotoDetails {
    #[serde(flatten)]
    photo: library::LibraryPhoto,
    bytes: Option<u64>,
}

#[tauri::command]
pub(crate) async fn photo_details(path: String) -> Res<PhotoDetails> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = PathBuf::from(&path);
        let photo = library::list_paths(std::slice::from_ref(&p))
            .0
            .into_iter()
            .next()
            .ok_or_else(|| format!("{path}: not found"))?;
        let bytes = std::fs::metadata(amb_engine::source_path(&p))
            .ok()
            .map(|m| m.len());
        Ok(PhotoDetails { photo, bytes })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The JPEGs shot with these raws, which the library shows as one photo
/// with the raw (Filter → File: JPEG only lists them).
#[tauri::command]
pub(crate) async fn library_twin_jpegs(paths: Vec<String>) -> Res<Vec<library::LibraryPhoto>> {
    tauri::async_runtime::spawn_blocking(move || {
        let raws: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
        library::list_paths(&amb_library::folder::raw_twins(&raws)).0
    })
    .await
    .map_err(|e| e.to_string())
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

/// The folders inside `dir`, for the library's folder strip.
#[tauri::command]
pub(crate) async fn library_subfolders(dir: String) -> Res<Vec<amb_library::folder::Subfolder>> {
    tauri::async_runtime::spawn_blocking(move || amb_library::folder::subfolders(Path::new(&dir)))
        .await
        .map_err(|e| e.to_string())
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
    state.change_prefs(|p| {
        let recent = &mut p.recent_keywords;
        for k in updates.iter().flat_map(|u| u.info.keywords.iter()).rev() {
            recent.retain(|r| !r.eq_ignore_ascii_case(k));
            recent.insert(0, k.clone());
        }
        recent.truncate(200);
    })?;
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
                let text = amb_engine::sidecar_text(&photo);
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
