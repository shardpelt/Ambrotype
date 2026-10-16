// SPDX-License-Identifier: GPL-3.0-or-later
//! The hub: browsing and installing shared items, and the preset library.

use crate::*;

pub(crate) fn hub_index(state: &AppState) -> String {
    state
        .hub_url
        .lock()
        .ok()
        .and_then(|h| h.clone())
        .unwrap_or_else(|| hub::DEFAULT_INDEX.to_string())
}

/// The hub's address (the default unless changed in Settings).
#[tauri::command]
pub(crate) fn hub_url(state: tauri::State<'_, AppState>) -> String {
    hub_index(state.inner())
}

/// Use another hub; `None` goes back to the default.
#[tauri::command]
pub(crate) fn set_hub_url(url: Option<String>, state: tauri::State<'_, AppState>) -> Res<String> {
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
pub(crate) struct HubList {
    pub(crate) url: String,
    pub(crate) entries: Vec<hub::HubEntry>,
}

/// Everything in the hub, with previews and what's installed already.
#[tauri::command]
pub(crate) async fn hub_browse(state: tauri::State<'_, AppState>) -> Res<HubList> {
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
pub(crate) async fn hub_install_preset(
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

pub(crate) fn library_cache(state: &AppState) -> Option<PathBuf> {
    let path = state.config_path.lock().ok()?;
    Some(
        path.as_ref()?
            .parent()?
            .join("cache")
            .join("preset-library"),
    )
}

/// The Preset Library: the hub's presets, or the built-in starters.
#[tauri::command]
pub(crate) async fn preset_library(
    state: tauri::State<'_, AppState>,
) -> Res<Vec<hub::LibraryPreset>> {
    let url = hub_index(state.inner());
    let cache = library_cache(state.inner()).ok_or("the settings folder isn't available")?;
    let presets = own_presets_dir(state.inner());
    tauri::async_runtime::spawn_blocking(move || {
        hub::preset_library(&url, &cache, presets.as_deref())
    })
    .await
    .map_err(|e| e.to_string())
}

/// Add a Preset Library preset to the presets list; returns the list.
#[tauri::command]
pub(crate) async fn library_install_preset(
    id: String,
    path: String,
    state: tauri::State<'_, AppState>,
) -> Res<Vec<PresetEntry>> {
    let dir = own_presets_dir(state.inner()).ok_or("the settings folder isn't available")?;
    let cache = library_cache(state.inner()).ok_or("the settings folder isn't available")?;
    hub::install_library_preset(&dir, &cache, &id, Path::new(&path))?;
    presets(state).await
}

/// Install (or update) a collage template from the hub; returns the templates.
#[tauri::command]
pub(crate) async fn hub_install_collage(
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
pub(crate) async fn hub_install_sizes(
    id: String,
    state: tauri::State<'_, AppState>,
) -> Res<Vec<SizePreset>> {
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
pub(crate) async fn hub_install_plugin(
    id: String,
    state: tauri::State<'_, AppState>,
) -> Res<String> {
    let url = hub_index(state.inner());
    let dir = plugins_dir(state.inner()).ok_or("the plugins folder isn't available")?;
    tauri::async_runtime::spawn_blocking(move || hub::install_plugin(&dir, &url, &id))
        .await
        .map_err(|e| e.to_string())?
}

/// Install (or update) a look from the hub.
#[tauri::command]
pub(crate) async fn hub_install(id: String, state: tauri::State<'_, AppState>) -> Res<SavedLook> {
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

/// Install a layout or theme from a link someone shared (a file, or a GitHub
/// repo with `theme.json` or `layout.json`).
#[tauri::command]
pub(crate) async fn hub_install_link(
    link: String,
    state: tauri::State<'_, AppState>,
) -> Res<SavedLook> {
    let dir = need_looks_dir(state.inner())?;
    let (kind, id) = tauri::async_runtime::spawn_blocking(move || hub::install_link(&dir, &link))
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
pub(crate) fn hub_submit(
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
