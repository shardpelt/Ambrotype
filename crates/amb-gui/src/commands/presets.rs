// SPDX-License-Identifier: GPL-3.0-or-later
//! Develop presets, size presets, mask effects, LUTs, Auto, Match look and Surprise me.

use crate::*;

/// Pick reference photos for Match look.
#[tauri::command]
pub(crate) async fn pick_references(app: tauri::AppHandle) -> Vec<String> {
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
pub(crate) struct MatchLine {
    pub(crate) name: String,
    pub(crate) label: String,
    pub(crate) before: String,
    pub(crate) after: String,
}

/// What Match look proposes: the commands, the same as readable lines,
/// and how close it gets.
#[derive(Serialize)]
pub(crate) struct MatchProposal {
    pub(crate) commands: Vec<Command>,
    pub(crate) lines: Vec<MatchLine>,
    pub(crate) before: f64,
    pub(crate) after: f64,
    pub(crate) renders: usize,
}

/// Auto for the open photo: the commands for its tone and/or white balance.
#[tauri::command]
pub(crate) async fn auto_settings(
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
pub(crate) async fn match_look(
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

/// The presets starred as favourites (paths).
#[tauri::command]
pub(crate) fn preset_favourites(state: tauri::State<'_, AppState>) -> Res<Vec<String>> {
    Ok(state
        .preset_favourites
        .lock()
        .map_err(|e| e.to_string())?
        .clone())
}

/// Star or unstar a preset.
#[tauri::command]
pub(crate) fn set_preset_favourite(
    path: String,
    on: bool,
    state: tauri::State<'_, AppState>,
) -> Res<()> {
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
pub(crate) struct MaskEffects {
    pub(crate) builtin: Vec<MaskEffect>,
    pub(crate) mine: Vec<MaskEffect>,
}

/// The built-in mask effects and the user's.
#[tauri::command]
pub(crate) fn mask_effects(state: tauri::State<'_, AppState>) -> Res<MaskEffects> {
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
pub(crate) fn save_mask_effect(
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
pub(crate) fn delete_mask_effect(
    name: String,
    state: tauri::State<'_, AppState>,
) -> Res<MaskEffects> {
    state
        .mask_effects
        .lock()
        .map_err(|e| e.to_string())?
        .retain(|e| e.name != name);
    save_config(&state);
    mask_effects(state)
}

#[tauri::command]
pub(crate) async fn presets(state: tauri::State<'_, AppState>) -> Res<Vec<PresetEntry>> {
    let mut dirs = state.preset_dirs.lock().map_err(|e| e.to_string())?.clone();
    // Presets saved in the app ("Save as preset…").
    if let Some(own) = own_presets_dir(state.inner()).filter(|d| d.is_dir() && !dirs.contains(d)) {
        dirs.push(own);
    }
    tauri::async_runtime::spawn_blocking(move || session::scan_presets(&dirs))
        .await
        .map_err(|e| e.to_string())
}

pub(crate) fn own_presets_dir(state: &AppState) -> Option<PathBuf> {
    let path = state.config_path.lock().ok()?;
    Some(path.as_ref()?.parent()?.join("presets"))
}

/// Save the open photo's edit as a preset (crop, masks and geometry left
/// out) in the app's presets folder; returns the presets list.
#[tauri::command]
pub(crate) async fn save_preset(
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
pub(crate) async fn add_preset_folder(
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
pub(crate) fn size_presets(state: tauri::State<'_, AppState>) -> Res<Vec<SizePreset>> {
    let custom = state.size_presets.lock().map_err(|e| e.to_string())?;
    Ok(sizes::all(&custom))
}

/// Add a user size preset (or a clone of a built-in), or update one.
#[tauri::command]
pub(crate) fn save_size_preset(
    preset: SizePreset,
    state: tauri::State<'_, AppState>,
) -> Res<Vec<SizePreset>> {
    {
        let mut custom = state.size_presets.lock().map_err(|e| e.to_string())?;
        sizes::upsert(&mut custom, &preset)?;
    }
    save_config(state.inner());
    size_presets(state)
}

#[tauri::command]
pub(crate) fn delete_size_preset(
    id: String,
    state: tauri::State<'_, AppState>,
) -> Res<Vec<SizePreset>> {
    {
        let mut custom = state.size_presets.lock().map_err(|e| e.to_string())?;
        sizes::remove(&mut custom, &id)?;
    }
    save_config(state.inner());
    size_presets(state)
}

/// Where the user keeps `.cube` files for the LUT menu.
pub(crate) fn luts_dir(state: &AppState) -> Option<PathBuf> {
    let path = state.config_path.lock().ok()?;
    Some(path.as_ref()?.parent()?.join("luts"))
}

#[derive(Serialize)]
pub(crate) struct LutEntry {
    pub(crate) name: String,
    pub(crate) path: String,
}

/// The `.cube` files in the LUT folder (and its subfolders), by name.
#[tauri::command]
pub(crate) fn luts_list(state: tauri::State<'_, AppState>) -> Vec<LutEntry> {
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
pub(crate) async fn pick_lut(app: tauri::AppHandle) -> Option<String> {
    app.dialog()
        .file()
        .set_title("Choose a LUT")
        .add_filter("LUT (.cube)", &["cube", "CUBE"])
        .blocking_pick_file()
        .and_then(|p| p.into_path().ok())
        .map(|p| p.display().to_string())
}

/// Open the LUT folder (made if missing), to drop `.cube` files in.
#[tauri::command]
pub(crate) fn open_luts_folder(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Res<()> {
    let dir = luts_dir(&state).ok_or("the LUT folder isn't available")?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    app.opener()
        .open_path(dir.display().to_string(), None::<&str>)
        .map_err(|e| e.to_string())
}

/// Run the same edit commands on several photos (a preset on the whole
/// folder): the open one through its session (one undo step), the others
/// into their sidecars.
#[tauri::command]
pub(crate) async fn apply_to_photos(
    paths: Vec<String>,
    commands: Vec<Command>,
    label: String,
    state: tauri::State<'_, AppState>,
) -> Res<RollSync> {
    let slot = state.session.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut out = RollSync {
            changed: Vec::new(),
            errors: Vec::new(),
            state: None,
        };
        for path in &paths {
            let mut guard = slot.lock().map_err(|e| e.to_string())?;
            let result = match guard.as_mut().filter(|s| s.path == Path::new(path)) {
                Some(s) => s
                    .execute(&commands, Some(&label))
                    .map(|_| out.state = Some(s.state_info())),
                None => library::paste_to_file(Path::new(path), &commands),
            };
            match result {
                Ok(()) => out.changed.push(path.clone()),
                Err(e) => out.errors.push(format!("{path}: {e}")),
            }
        }
        Ok(out)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Serialize)]
pub(crate) struct Variant {
    pub(crate) seed: u64,
    pub(crate) commands: Vec<Command>,
    /// A small preview, as a `data:` URL.
    pub(crate) image: String,
}

/// Small previews of the open photo with each preset in `paths` applied
/// (at `amount`), as `data:` URLs; `None` for a preset that can't be read.
/// Nothing is applied.
#[tauri::command]
pub(crate) async fn preset_thumbnails(
    paths: Vec<String>,
    amount: f64,
    state: tauri::State<'_, AppState>,
) -> Res<Vec<Option<String>>> {
    let slot = state.session.clone();
    let lens = state.lens.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut guard = slot.lock().map_err(|e| e.to_string())?;
        let s = guard.as_mut().ok_or("open a photo first")?;
        let lens = lens.get().unwrap_or_else(|| empty_lens());
        Ok(paths
            .iter()
            .take(64)
            .map(|path| {
                let commands = [Command::ApplyPreset {
                    path: path.clone(),
                    amount,
                    include_groups: None,
                    exclude_groups: None,
                }];
                let mut trial = s.state.clone();
                trial.execute_all(&commands).ok()?;
                let overrides: BTreeMap<String, Value> = trial
                    .settings()
                    .iter()
                    .filter(|(k, v)| s.state.settings().get(*k) != Some(*v))
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                let p = s.preview(160, &overrides, false, false, lens);
                Some(format!(
                    "data:image/jpeg;base64,{}",
                    plugins::base64(&p.jpeg)
                ))
            })
            .collect())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Surprise me: `count` random looks from the built-in Random look plugin
/// (seeds `seed`, `seed + 1`, ...), each with a small preview of the photo
/// in it. Nothing is applied.
#[tauri::command]
pub(crate) async fn surprise(
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
