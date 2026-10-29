// SPDX-License-Identifier: GPL-3.0-or-later
//! Opening a photo and editing it: edits, crop, masks, spots, info, negatives.

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
        .map_err(|e| e.to_string())
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
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub(crate) async fn open_photo(
    path: String,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Res<Opened> {
    let slot = state.session.clone();
    let proof = *state.proof.lock().map_err(|e| e.to_string())?;
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
    .await
    .map_err(|e| e.to_string())??;
    let (opened, sidecar) = opened;
    *state.sidecar_watch.lock().map_err(|e| e.to_string())? = watch_sidecar(app, &sidecar);
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

/// The committed crop, upright on the uncropped photo (the crop tool's start).
#[tauri::command]
pub(crate) fn crop_state(state: tauri::State<'_, AppState>) -> Res<CropState> {
    let guard = state.session.lock().map_err(|e| e.to_string())?;
    Ok(guard.as_ref().ok_or("no photo open")?.crop_state())
}

/// A donor for a new spot (the spot tool shows it, and the user can move it).
#[tauri::command]
pub(crate) async fn spot_source(
    x: f64,
    y: f64,
    radius: f64,
    points: Option<Vec<[f64; 2]>>,
    feather: Option<f64>,
    state: tauri::State<'_, AppState>,
) -> Res<[f64; 2]> {
    let slot = state.session.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let guard = slot.lock().map_err(|e| e.to_string())?;
        let session = guard.as_ref().ok_or("no photo open")?;
        // A brushed spot: searched for the stroke's own shape.
        let (sx, sy) = match &points {
            Some(p) => session.stroke_source(p, radius, feather.unwrap_or(50.0))?,
            None => session.spot_source(x, y, radius),
        };
        Ok([sx, sy])
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Find dust and hairs on the scan and heal them, as one undoable step.
#[tauri::command]
pub(crate) async fn find_dust(state: tauri::State<'_, AppState>) -> Res<Executed> {
    let slot = state.session.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut guard = slot.lock().map_err(|e| e.to_string())?;
        let s = guard.as_mut().ok_or("no photo open")?;
        let found = s.find_dust();
        if !found.commands.is_empty() {
            s.execute(&found.commands, Some("dust and hairs"))?;
        }
        let note = match (found.specks, found.hairs) {
            (0, 0) if found.covered > 0 => "No new dust: spots already cover what was found.".to_string(),
            (0, 0) => "No dust found. Faint specks, busy areas and long straight scratches are left to you.".to_string(),
            (specks, hairs) => format!(
                "Healed {specks} speck{} and {hairs} hair{}. Click a spot to change or delete it.",
                if specks == 1 { "" } else { "s" },
                if hairs == 1 { "" } else { "s" }
            ),
        };
        Ok(Executed {
            state: s.state_info(),
            notes: vec![note],
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Heal a straight scratch along a line drawn on the photo, as one step.
#[tauri::command]
pub(crate) async fn heal_line(
    from: [f64; 2],
    to: [f64; 2],
    state: tauri::State<'_, AppState>,
) -> Res<Executed> {
    let slot = state.session.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut guard = slot.lock().map_err(|e| e.to_string())?;
        let s = guard.as_mut().ok_or("no photo open")?;
        let cmds = s.scratch_spots(from, to);
        s.execute(&cmds, Some("scratch"))?;
        Ok(Executed {
            state: s.state_info(),
            notes: Vec::new(),
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The mask tool's map from the shown photo to mask coordinates.
#[tauri::command]
pub(crate) fn mask_frame(state: tauri::State<'_, AppState>) -> Res<session::MaskFrame> {
    let guard = state.session.lock().map_err(|e| e.to_string())?;
    Ok(guard.as_ref().ok_or("no photo open")?.mask_frame())
}

#[derive(Serialize)]
pub(crate) struct MaskSlider {
    pub(crate) tag: &'static str,
    pub(crate) label: &'static str,
    pub(crate) min: f64,
    pub(crate) max: f64,
    pub(crate) step: f64,
    pub(crate) section: &'static str,
    pub(crate) rendered: bool,
}

/// The masks' local sliders, in panel order (UI units).
#[tauri::command]
pub(crate) fn mask_sliders() -> Vec<MaskSlider> {
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
pub(crate) fn crop_commands(
    rect: CropRect,
    state: tauri::State<'_, AppState>,
) -> Res<Vec<Command>> {
    let guard = state.session.lock().map_err(|e| e.to_string())?;
    Ok(guard.as_ref().ok_or("no photo open")?.crop_commands(&rect))
}

/// White balance from a clicked neutral spot: sets WB to Custom with the
/// picked Temperature/Tint (one undoable step).
#[tauri::command]
pub(crate) fn pick_white(u: f64, v: f64, state: tauri::State<'_, AppState>) -> Res<Executed> {
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

/// Film base from a click on the clear film between frames: stores it (and
/// turns the conversion on) as one undoable step.
#[tauri::command]
pub(crate) fn pick_film_base(u: f64, v: f64, state: tauri::State<'_, AppState>) -> Res<Executed> {
    let mut guard = state.session.lock().map_err(|e| e.to_string())?;
    let s = guard.as_mut().ok_or("no photo open")?;
    let (base, warning) = s
        .pick_film_base(u, v)
        .ok_or("pick the clear film between frames: that spot is clipped or black")?;
    let round = |v: f64| (v * 10_000.0).round() / 10_000.0;
    let mut cmds = vec![Command::SetParam {
        name: "amb:NegativeConvert".into(),
        value: serde_json::json!(true),
    }];
    for (c, name) in ["Red", "Green", "Blue"].iter().enumerate() {
        cmds.push(Command::SetParam {
            name: format!("amb:FilmBase{name}"),
            value: serde_json::json!(round(base[c])),
        });
    }
    s.execute(&cmds, Some("film base picker"))?;
    Ok(Executed {
        state: s.state_info(),
        notes: vec![warning.unwrap_or_else(|| {
            "Film base set. Alt-click Pick base to measure it from the scan again.".into()
        })],
    })
}

#[tauri::command]
pub(crate) fn save_sidecar(state: tauri::State<'_, AppState>) -> Res<String> {
    let mut guard = state.session.lock().map_err(|e| e.to_string())?;
    guard.as_mut().ok_or("no photo open")?.save_sidecar()
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

/// The settings `commands` would change on the open photo, as preview
/// overrides; nothing is applied (a preset's look while hovering it).
#[tauri::command]
pub(crate) fn trial_overrides(
    commands: Vec<Command>,
    state: tauri::State<'_, AppState>,
) -> Res<serde_json::Map<String, serde_json::Value>> {
    let guard = state.session.lock().map_err(|e| e.to_string())?;
    let s = guard.as_ref().ok_or("no photo open")?;
    let mut trial = s.state.clone();
    trial.execute_all(&commands).map_err(|e| e.to_string())?;
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
pub(crate) struct Guided {
    pub(crate) commands: Vec<Command>,
    /// How far the worst guide is still off, degrees.
    pub(crate) worst_degrees: f64,
}

/// Guided Upright for the open photo: the Transform sliders that straighten
/// `guides` (fractions of the upright, uncropped photo).
#[tauri::command]
pub(crate) fn guided_upright(
    guides: Vec<[f64; 4]>,
    state: tauri::State<'_, AppState>,
) -> Res<Guided> {
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
pub(crate) fn upright_guides(state: tauri::State<'_, AppState>) -> Res<Vec<[f64; 4]>> {
    let guard = state.session.lock().map_err(|e| e.to_string())?;
    Ok(guard
        .as_ref()
        .map(Session::upright_guides)
        .unwrap_or_default())
}

#[derive(Serialize)]
pub(crate) struct LensDbState {
    /// Where a downloaded database goes, and whether one is there.
    pub(crate) dir: Option<String>,
    pub(crate) timestamp: Option<u64>,
    /// Lenses the editor has now (from any database found).
    pub(crate) lenses: Option<usize>,
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
    .await
    .map_err(|e| e.to_string())??;
    Ok(lens_db_state(state))
}

/// The open photo's film negative analysis as commands that store it with
/// Auto off ("Freeze").
#[tauri::command]
pub(crate) async fn negative_freeze(state: tauri::State<'_, AppState>) -> Res<Vec<Command>> {
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
            let mut guard = slot.lock().map_err(|e| e.to_string())?;
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
    })
    .await
    .map_err(|e| e.to_string())??;
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
    app.clipboard().write_text(text).map_err(|e| e.to_string())
}

/// The crop tool's Level: the straighten angle that levels the open photo;
/// `None` when it has too few long lines to tell.
#[tauri::command]
pub(crate) async fn level_angle(state: tauri::State<'_, AppState>) -> Res<Option<f64>> {
    let slot = state.session.clone();
    let lens = state.lens.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut guard = slot.lock().map_err(|e| e.to_string())?;
        let s = guard.as_mut().ok_or("no photo open")?;
        let lens = lens.get().unwrap_or_else(|| empty_lens());
        Ok(s.level_angle(lens))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The open photo's long straight lines, for the crop tool's guides.
#[tauri::command]
pub(crate) async fn guide_lines(state: tauri::State<'_, AppState>) -> Res<Vec<[f64; 4]>> {
    let slot = state.session.clone();
    let lens = state.lens.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut guard = slot.lock().map_err(|e| e.to_string())?;
        let s = guard.as_mut().ok_or("no photo open")?;
        let lens = lens.get().unwrap_or_else(|| empty_lens());
        Ok(s.guide_lines(lens))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Crop the open scan to its frame; `None` when no border was found.
#[tauri::command]
pub(crate) async fn negative_frame(state: tauri::State<'_, AppState>) -> Res<Option<Vec<Command>>> {
    let slot = state.session.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let guard = slot.lock().map_err(|e| e.to_string())?;
        Ok(guard.as_ref().ok_or("no photo open")?.negative_frame())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The camera profiles found for the open photo's camera; `None` while the
/// profile folders are still being read.
#[tauri::command]
pub(crate) fn camera_profiles(state: tauri::State<'_, AppState>) -> Res<Option<Vec<String>>> {
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
