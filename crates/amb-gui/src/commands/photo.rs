// SPDX-License-Identifier: GPL-3.0-or-later
//! Opening a photo and editing it: edits, info, the clipboard, lens database.

use crate::*;

#[derive(Serialize)]
pub(crate) struct Opened {
    pub(crate) info: session::PhotoInfo,
    pub(crate) state: session::StateInfo,
    /// The sidecar the previous photo's unsaved edits were saved to, if any.
    pub(crate) saved_previous: Option<String>,
}

#[tauri::command]
pub(crate) fn take_initial_path(state: tauri::State<'_, AppState>) -> Option<String> {
    state.initial_path.lock().ok()?.take()
}

#[tauri::command]
pub(crate) fn editor_schema() -> Vec<schema::Panel> {
    schema::panels()
}

#[tauri::command]
pub(crate) async fn pick_photo(app: tauri::AppHandle) -> Option<String> {
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
pub(crate) async fn pick_photos(app: tauri::AppHandle) -> Res<Vec<folder::FolderPhoto>> {
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
        .map_err(Into::into)
}

/// Pick a folder and list its raws (for the collage maker).
#[tauri::command]
pub(crate) async fn pick_photo_folder(
    app: tauri::AppHandle,
) -> Res<Option<Vec<folder::FolderPhoto>>> {
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
        .await?
        .map_err(Into::into)
}

#[tauri::command]
pub(crate) async fn open_photo(
    path: String,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Res<Opened> {
    let slot = state.session.clone();
    let proof = *state.proof.lock()?;
    let opened = tauri::async_runtime::spawn_blocking(move || {
        // The photo that's open already (another interface's page asking
        // again) keeps its edits and history.
        if let Some(s) = slot.lock().map_err(|e| e.to_string())?.as_ref() {
            if s.path == Path::new(&path) {
                let opened = Opened {
                    info: s.info(),
                    state: s.state_info(),
                    saved_previous: None,
                };
                return Ok((opened, s.sidecar.clone()));
            }
        }
        let mut s = Session::open(Path::new(&path))?;
        s.on_screen = true;
        s.set_proof(proof);
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
    .await??;
    let (opened, sidecar) = opened;
    *state.sidecar_watch.lock()? = watch_sidecar(app, &sidecar);
    Ok(opened)
}

/// Tell the page ("sidecar-changed") when the open photo's sidecar changes
/// on disk; [`reload_sidecar`] then tells our own saves from other writers.
pub(crate) fn watch_sidecar(
    app: tauri::AppHandle,
    sidecar: &Path,
) -> Option<notify::RecommendedWatcher> {
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
pub(crate) struct Reloaded {
    pub(crate) state: session::StateInfo,
    /// Develop settings that changed.
    pub(crate) changes: usize,
}

/// Take the sidecar's settings, rating and keywords if something else
/// changed it since we read or wrote it. `None` when it is as we left it.
#[tauri::command]
pub(crate) fn reload_sidecar(state: tauri::State<'_, AppState>) -> Res<Option<Reloaded>> {
    let mut guard = state.session.lock()?;
    let Some(s) = guard.as_mut() else {
        return Ok(None);
    };
    Ok(s.reload_sidecar()?.map(|changes| Reloaded {
        state: s.state_info(),
        changes,
    }))
}

#[derive(Serialize)]
pub(crate) struct Executed {
    pub(crate) state: session::StateInfo,
    pub(crate) notes: Vec<String>,
}

#[tauri::command]
pub(crate) fn execute(
    commands: Vec<Command>,
    group: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Res<Executed> {
    let mut guard = state.session.lock()?;
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

#[tauri::command]
pub(crate) fn save_sidecar(state: tauri::State<'_, AppState>) -> Res<String> {
    let mut guard = state.session.lock()?;
    guard
        .as_mut()
        .ok_or("no photo open")?
        .save_sidecar()
        .map_err(Into::into)
}

#[derive(Serialize)]
pub(crate) struct InfoState {
    pub(crate) info: amb_protocol::sidecar::PhotoInfo,
    /// Keywords used lately, for suggestions.
    pub(crate) recent: Vec<String>,
}

pub(crate) fn info_state(state: &AppState) -> Res<InfoState> {
    let info = state
        .session
        .lock()?
        .as_ref()
        .map(|s| s.info.clone())
        .unwrap_or_default();
    let recent = state.prefs(|p| p.recent_keywords.clone())?;
    Ok(InfoState { info, recent })
}

/// The open photo's rating and keywords.
#[tauri::command]
pub(crate) fn photo_info(state: tauri::State<'_, AppState>) -> Res<InfoState> {
    info_state(state.inner())
}

/// Set the open photo's rating and keywords; they're written to its sidecar at once.
#[tauri::command]
pub(crate) fn set_photo_info(
    info: amb_protocol::sidecar::PhotoInfo,
    state: tauri::State<'_, AppState>,
) -> Res<InfoState> {
    let saved = {
        let mut guard = state.session.lock()?;
        let s = guard.as_mut().ok_or("no photo is open")?;
        // Only Rotate and Flip turn a photo: a rating or keyword written
        // with the info the page had keeps the turn made since.
        let info = amb_protocol::sidecar::PhotoInfo {
            orientation: s.info.orientation,
            ..info
        };
        s.set_info(&info)?
    };
    state.change_prefs(|p| {
        let recent = &mut p.recent_keywords;
        for k in saved.keywords.iter().rev() {
            recent.retain(|r| !r.eq_ignore_ascii_case(k));
            recent.insert(0, k.clone());
        }
        recent.truncate(200);
    })?;
    info_state(state.inner())
}

/// The settings `commands` would change on the open photo, as preview
/// overrides; nothing is applied (a preset's look while hovering it).
#[tauri::command]
pub(crate) fn trial_overrides(
    commands: Vec<Command>,
    state: tauri::State<'_, AppState>,
) -> Res<serde_json::Map<String, serde_json::Value>> {
    let guard = state.session.lock()?;
    let s = guard.as_ref().ok_or("no photo open")?;
    let mut trial = s.state.clone();
    trial.execute_all(&commands)?;
    let before = s.state.to_json()["settings"].clone();
    let after = trial.to_json()["settings"].clone();
    let mut out: serde_json::Map<String, serde_json::Value> = after
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(k, v)| before.get(k.as_str()) != Some(*v))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    // A setting the commands reset shows at its default (a preset applied
    // to the original resets the look first).
    for k in before.as_object().into_iter().flatten().map(|(k, _)| k) {
        if after.get(k.as_str()).is_none() {
            if let Some(d) = amb_presets::registry::Registry::builtin()
                .lookup(k)
                .and_then(|t| t.default)
                .filter(|d| !d.is_null())
            {
                out.insert(k.clone(), d);
            }
        }
    }
    Ok(out)
}

#[derive(Serialize)]
pub(crate) struct LensDbState {
    /// Where a downloaded database goes, and whether one is there.
    pub(crate) dir: Option<String>,
    pub(crate) timestamp: Option<u64>,
    /// Lenses the editor has now (from any database found).
    pub(crate) lenses: Option<usize>,
    /// A UI test is running: nothing is downloaded by itself.
    pub(crate) testing: bool,
}

/// The lens database: what the editor has, and the downloaded one.
#[tauri::command]
pub(crate) fn lens_db_state(state: tauri::State<'_, AppState>) -> LensDbState {
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
        testing: crate::uitest::current().is_some(),
    }
}

/// Download lensfun's database (or update it), then load it.
#[tauri::command]
pub(crate) async fn fetch_lens_db(state: tauri::State<'_, AppState>) -> Res<LensDbState> {
    let lens = state.lens.clone();
    tauri::async_runtime::spawn_blocking(move || -> Res<()> {
        amb_library::lensdb::fetch(None, false)?;
        let _ = lens.set(LensSources::load_default());
        Ok(())
    })
    .await??;
    Ok(lens_db_state(state))
}

/// Long edge of a photo copied to the clipboard: big enough to paste into a
/// chat, mail or document, small enough to copy at once.
const COPY_EDGE: usize = 3072;

/// Put a photo, with its edit, on the clipboard as a picture (the open
/// photo with its edit as it is now, saved or not). Returns its size.
#[tauri::command]
pub(crate) async fn copy_image(
    app: tauri::AppHandle,
    path: String,
    state: tauri::State<'_, AppState>,
) -> Res<(u32, u32)> {
    use tauri_plugin_clipboard_manager::ClipboardExt;
    let slot = state.session.clone();
    let lens = state.lens.clone();
    let rgb = tauri::async_runtime::spawn_blocking(move || -> Res<_> {
        let lens = lens.get().unwrap_or_else(|| empty_lens());
        let none = Default::default();
        let jpeg = {
            let mut guard = slot.lock()?;
            match guard.as_mut().filter(|s| s.path == Path::new(&path)) {
                Some(s) => {
                    // In sRGB, not the monitor's colours the preview is in.
                    let screen = std::mem::replace(&mut s.on_screen, false);
                    let p = s.preview(COPY_EDGE, &none, false, false, lens);
                    s.on_screen = screen;
                    p.jpeg
                }
                None => {
                    drop(guard);
                    Session::open(Path::new(&path))?
                        .preview(COPY_EDGE, &none, false, false, lens)
                        .jpeg
                }
            }
        };
        amb_library::duplicates::decode_jpeg(&jpeg)
            .ok_or_else(|| "couldn't render the photo".to_string())
            .map_err(Into::into)
    })
    .await??;
    let (w, h) = rgb.dimensions();
    let rgba: Vec<u8> = rgb.pixels().flat_map(|p| [p[0], p[1], p[2], 255]).collect();
    app.clipboard()
        .write_image(&tauri::image::Image::new_owned(rgba, w, h))
        .map_err(|e| format!("couldn't copy to the clipboard: {e}"))?;
    Ok((w, h))
}

/// Put text (a file's path) on the clipboard.
#[tauri::command]
pub(crate) fn copy_text(app: tauri::AppHandle, text: String) -> Res<()> {
    use tauri_plugin_clipboard_manager::ClipboardExt;
    app.clipboard().write_text(text).map_err(Into::into)
}

/// The camera profiles found for the open photo's camera; `None` while the
/// profile folders are still being read.
#[tauri::command]
pub(crate) fn camera_profiles(state: tauri::State<'_, AppState>) -> Res<Option<Vec<String>>> {
    let Some(lens) = state.lens.get() else {
        return Ok(None);
    };
    let guard = state.session.lock()?;
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
