// SPDX-License-Identifier: GPL-3.0-or-later
//! Appearance: layouts, themes and saved setups.

use crate::*;

/// The folder for the user's own looks (next to the settings file).
pub(crate) fn looks_dir(state: &AppState) -> Option<PathBuf> {
    let path = state.config_path.lock().ok()?;
    Some(path.as_ref()?.parent()?.join("looks"))
}

pub(crate) fn looks_list(state: &AppState) -> Res<looks::Looks> {
    let (layout, theme) = state.look.lock().map_err(|e| e.to_string())?.clone();
    Ok(looks::list(
        looks_dir(state).as_deref(),
        layout.as_deref(),
        theme.as_deref(),
    ))
}

/// Built-in and user layouts and themes, and which are active.
#[tauri::command]
pub(crate) fn looks(state: tauri::State<'_, AppState>) -> Res<looks::Looks> {
    looks_list(state.inner())
}

#[derive(Serialize)]
pub(crate) struct SavedLook {
    pub(crate) id: String,
    pub(crate) kind: looks::Kind,
    pub(crate) looks: looks::Looks,
}

pub(crate) fn need_looks_dir(state: &AppState) -> Res<PathBuf> {
    looks_dir(state).ok_or_else(|| "the app's settings folder isn't available".to_string())
}

/// Save a user look (a clone, a rename, an edited theme). `id` updates an
/// existing one; without it a new file is made.
#[tauri::command]
pub(crate) fn save_look(
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
pub(crate) fn delete_look(
    kind: looks::Kind,
    id: String,
    state: tauri::State<'_, AppState>,
) -> Res<looks::Looks> {
    looks::delete(&need_looks_dir(state.inner())?, kind, &id)?;
    looks_list(state.inner())
}

/// The kept setups (a layout and theme together), newest first.
#[tauri::command]
pub(crate) fn setups_list(state: tauri::State<'_, AppState>) -> Res<Vec<looks::Setup>> {
    Ok(looks_dir(state.inner())
        .map(|d| looks::setups(&d))
        .unwrap_or_default())
}

/// Keep the layout and theme given (as they are now) as the setup `name`.
#[tauri::command]
pub(crate) fn setup_save(
    name: String,
    layout: String,
    theme: String,
    state: tauri::State<'_, AppState>,
) -> Res<Vec<looks::Setup>> {
    let dir = need_looks_dir(state.inner())?;
    looks::save_setup(&dir, &name, &layout, &theme)?;
    Ok(looks::setups(&dir))
}

#[derive(Serialize)]
pub(crate) struct RestoredSetup {
    pub(crate) layout: String,
    pub(crate) theme: String,
    pub(crate) looks: looks::Looks,
}

/// The layout and theme that bring a setup back (saved again as the user's
/// own looks if they have changed since).
#[tauri::command]
pub(crate) fn setup_restore(name: String, state: tauri::State<'_, AppState>) -> Res<RestoredSetup> {
    let dir = need_looks_dir(state.inner())?;
    let (layout, theme) = looks::restore_setup(&dir, &name)?;
    Ok(RestoredSetup {
        layout,
        theme,
        looks: looks_list(state.inner())?,
    })
}

#[tauri::command]
pub(crate) fn setup_delete(
    name: String,
    state: tauri::State<'_, AppState>,
) -> Res<Vec<looks::Setup>> {
    let dir = need_looks_dir(state.inner())?;
    looks::delete_setup(&dir, &name)?;
    Ok(looks::setups(&dir))
}

/// Remember the active layout and theme.
#[tauri::command]
pub(crate) fn set_active_look(
    layout: String,
    theme: String,
    state: tauri::State<'_, AppState>,
) -> Res<()> {
    *state.look.lock().map_err(|e| e.to_string())? = (Some(layout), Some(theme));
    save_config(state.inner());
    Ok(())
}

/// Import a shared layout or theme file (picked in a dialog).
#[tauri::command]
pub(crate) async fn import_look(
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
pub(crate) async fn export_look(
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
pub(crate) fn open_looks_folder(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Res<String> {
    let dir = need_looks_dir(state.inner())?;
    for kind in ["layouts", "themes"] {
        std::fs::create_dir_all(dir.join(kind)).map_err(|e| e.to_string())?;
    }
    app.opener()
        .open_path(dir.display().to_string(), None::<&str>)
        .map_err(|e| e.to_string())?;
    Ok(dir.display().to_string())
}
