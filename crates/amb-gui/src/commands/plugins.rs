// SPDX-License-Identifier: GPL-3.0-or-later
//! Plugins: installing, running, recipes and sharing.

use crate::*;

pub(crate) fn plugins_dir(state: &AppState) -> Option<PathBuf> {
    let path = state.config_path.lock().ok()?;
    Some(path.as_ref()?.parent()?.join("plugins"))
}

pub(crate) fn plugins_state(state: &AppState) -> Res<Vec<plugins::PluginEntry>> {
    let approvals = state
        .plugin_approvals
        .lock()
        .map_err(|e| e.to_string())?
        .clone();
    Ok(plugins::list(plugins_dir(state).as_deref(), &approvals))
}

/// Built-in plugins, then the user's, with whether each is allowed to run.
#[tauri::command]
pub(crate) fn plugins_list(state: tauri::State<'_, AppState>) -> Res<Vec<plugins::PluginEntry>> {
    plugins_state(state.inner())
}

/// Install a plugin from a folder the user picks (it is checked first).
#[tauri::command]
pub(crate) async fn install_plugin(
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
pub(crate) fn remove_plugin(
    id: String,
    state: tauri::State<'_, AppState>,
) -> Res<Vec<plugins::PluginEntry>> {
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
pub(crate) fn approve_plugin(
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
pub(crate) struct PluginRun {
    pub(crate) state: session::StateInfo,
    pub(crate) notes: Vec<String>,
    pub(crate) message: Option<String>,
    pub(crate) log: Vec<String>,
    pub(crate) edits: usize,
}

/// Run a plugin on the open photo: its edits become one undo step.
#[tauri::command]
pub(crate) async fn run_plugin(
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
pub(crate) struct PluginBatch {
    /// The photos the plugin changed.
    pub(crate) changed: Vec<String>,
    pub(crate) errors: Vec<String>,
    /// The open photo's new state, when it was one of them.
    pub(crate) state: Option<session::StateInfo>,
    /// The plugin's message for the last photo, if any.
    pub(crate) message: Option<String>,
}

/// Run a plugin on many photos, each with its own settings: the open photo
/// as one undo step, the others in their sidecars. One failure doesn't stop
/// the rest.
#[tauri::command]
pub(crate) async fn run_plugin_many(
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

/// The recipes the user saved, per plugin.
#[tauri::command]
pub(crate) fn plugin_recipes(state: tauri::State<'_, AppState>) -> Res<PluginRecipes> {
    Ok(state
        .plugin_recipes
        .lock()
        .map_err(|e| e.to_string())?
        .clone())
}

/// Save (or replace, by name) a recipe for a plugin: its parameter values.
#[tauri::command]
pub(crate) fn save_plugin_recipe(
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
pub(crate) fn delete_plugin_recipe(
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
pub(crate) fn open_plugins_folder(
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
pub(crate) async fn plugin_share_plan(
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
pub(crate) async fn share_plugin(
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
