// SPDX-License-Identifier: GPL-3.0-or-later
//! Duplicating a preset and editing the user's own (the file work is in
//! `crate::preset_files`).

use super::presets::{own_presets_dir, presets};
use crate::*;

/// A copy of a preset in the app's own folder, now the user's to edit.
#[derive(Serialize)]
pub(crate) struct DuplicatedPreset {
    presets: Vec<PresetEntry>,
    path: String,
}

/// Duplicate any preset as "NAME copy" in the app's presets folder.
#[tauri::command]
pub(crate) async fn duplicate_preset(
    path: String,
    state: tauri::State<'_, AppState>,
) -> Res<DuplicatedPreset> {
    let own = own_presets_dir(state.inner()).ok_or("the settings folder isn't available")?;
    let copy = tauri::async_runtime::spawn_blocking(move || {
        preset_files::duplicate_preset(Path::new(&path), &own)
    })
    .await??;
    Ok(DuplicatedPreset {
        presets: presets(state).await?,
        path: copy.display().to_string(),
    })
}

/// What the Edit dialog lists for a preset: the settings it holds, and the
/// open photo's settings it could take in (none without a photo).
#[derive(Serialize)]
pub(crate) struct PresetContents {
    held: Vec<preset_files::Held>,
    photo: Vec<preset_files::Held>,
}

#[tauri::command]
pub(crate) fn preset_contents(
    path: String,
    state: tauri::State<'_, AppState>,
) -> Res<PresetContents> {
    let preset = amb_presets::Preset::from_path(Path::new(&path))?;
    let photo = state
        .session
        .lock()?
        .as_ref()
        .map(|s| preset_files::photo_additions(s.state.settings(), &preset.settings))
        .unwrap_or_default();
    Ok(PresetContents {
        held: preset_files::held_settings(&preset.settings),
        photo,
    })
}

/// Edit one of the user's own presets: name, group, the settings it holds,
/// and, with `from_photo`, their values from the open photo. Refused for
/// presets outside the app's own folder, as `delete_preset` is.
#[tauri::command]
pub(crate) async fn update_preset(
    path: String,
    name: String,
    group: String,
    keep: Vec<String>,
    from_photo: bool,
    state: tauri::State<'_, AppState>,
) -> Res<Vec<PresetEntry>> {
    let own = own_presets_dir(state.inner());
    let file = PathBuf::from(&path);
    let Some(own) = own.filter(|o| preset_files::preset_deletable(&file, o)) else {
        return Err(
            "Only presets saved in Ambrotype can be edited here. Duplicate it first.".into(),
        );
    };
    let photo = if from_photo {
        let guard = state.session.lock()?;
        Some(
            guard
                .as_ref()
                .ok_or("open a photo first")?
                .state
                .settings()
                .clone(),
        )
    } else {
        None
    };
    let moved = tauri::async_runtime::spawn_blocking(move || {
        preset_files::update_preset(&file, &own, &name, &group, &keep, photo.as_ref())
    })
    .await??
    .display()
    .to_string();
    if moved != path {
        state.change_prefs(|p| {
            for list in [&mut p.preset_favourites, &mut p.preset_hidden] {
                for x in list.iter_mut().filter(|x| **x == path) {
                    *x = moved.clone();
                }
            }
        })?;
    }
    presets(state).await
}
