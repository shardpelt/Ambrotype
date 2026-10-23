// SPDX-License-Identifier: GPL-3.0-or-later
//! File care from the library (Organise): moving, renaming and deleting
//! photos with their sidecars and companions (`amb_library::files`),
//! finding photos moved outside the app, the keyword list, and metadata
//! templates. The catalogue and collections follow every move.

use crate::*;
use amb_library::files::{self, Done, Planned, Target};
use commands::library::change_one;

/// What a move would do, photo by photo, before anything happens.
#[tauri::command]
pub(crate) async fn files_plan(paths: Vec<String>, target: Target) -> Res<Vec<Planned>> {
    tauri::async_runtime::spawn_blocking(move || {
        files::plan(
            &paths.iter().map(PathBuf::from).collect::<Vec<_>>(),
            &target,
        )
    })
    .await
    .map_err(|e| e.to_string())?
}

/// What moving did, and where the open photo is now, if it moved.
#[derive(Serialize)]
pub(crate) struct Moved {
    #[serde(flatten)]
    done: Done,
    reopen: Option<String>,
}

/// Carry out a plan (or, given a done move's files reversed, undo it).
/// The open photo's edits are saved first, and it is closed when it moves;
/// the page opens it again from `reopen`.
#[tauri::command]
pub(crate) async fn files_apply(
    plan: Vec<Planned>,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Res<Moved> {
    let slot = state.session.clone();
    let collections = commands::catalog::collections_dir(&app).ok();
    tauri::async_runtime::spawn_blocking(move || {
        let open = close_if_touched(
            &slot,
            plan.iter().flat_map(|p| p.files.iter().map(|f| &f.0)),
        )?;
        let done = files::apply(&plan);
        follow(&done.moves, collections.as_deref());
        let reopen = open.map(|o| {
            done.moves
                .iter()
                .find(|(a, _)| *a == o)
                .map_or(o.clone(), |(_, b)| b.clone())
                .display()
                .to_string()
        });
        Ok(Moved { done, reopen })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Move photos and all their files to the system's trash. Returns how many
/// files went, and whether the open photo was one of them (it is closed).
#[tauri::command]
pub(crate) async fn files_trash(
    paths: Vec<String>,
    state: tauri::State<'_, AppState>,
) -> Res<(usize, bool)> {
    let slot = state.session.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let photos: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
        let all: Vec<PathBuf> = photos.iter().flat_map(|p| files::companions(p)).collect();
        let closed = close_if_touched(&slot, photos.iter().chain(&all))?.is_some();
        let n = files::trash(&photos)?;
        folder::catalog().forget(&photos);
        Ok((n, closed))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Save and close the open photo when one of `files` is it (or its source
/// file). Returns its path when it was closed.
fn close_if_touched<'a>(
    slot: &Mutex<Option<Session>>,
    files: impl Iterator<Item = &'a PathBuf>,
) -> Res<Option<PathBuf>> {
    let files: std::collections::HashSet<&PathBuf> = files.collect();
    let mut guard = slot.lock().map_err(|e| e.to_string())?;
    let touched = guard.as_ref().is_some_and(|s| {
        files.contains(&s.path) || files.contains(&amb_engine::source_path(&s.path))
    });
    if !touched {
        return Ok(None);
    }
    if let Some(s) = guard.as_mut().filter(|s| s.has_unsaved_edits()) {
        s.save_sidecar()?;
    }
    Ok(guard.take().map(|s| s.path))
}

/// The catalogue and collections follow moved files.
fn follow(moves: &[(PathBuf, PathBuf)], collections: Option<&Path>) {
    folder::catalog().forget(&moves.iter().map(|m| m.0.clone()).collect::<Vec<_>>());
    if let Some(dir) = collections {
        amb_library::collections::relink(dir, moves);
    }
}

/// Photos the catalogue knows whose files are gone from a folder that can
/// be reached (moved or deleted outside the app).
#[tauri::command]
pub(crate) async fn files_missing() -> Res<Vec<String>> {
    tauri::async_runtime::spawn_blocking(|| {
        Ok(folder::catalog()
            .missing()
            .into_iter()
            .map(|(p, _)| p.display().to_string())
            .collect())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Look for the missing photos in `folder` (and its subfolders) by name
/// and size; the ones found are followed there (catalogue, collections).
/// Returns them, old and new.
#[tauri::command]
pub(crate) async fn files_find_moved(
    folder: String,
    app: tauri::AppHandle,
) -> Res<Vec<(PathBuf, PathBuf)>> {
    let collections = commands::catalog::collections_dir(&app).ok();
    tauri::async_runtime::spawn_blocking(move || {
        let found = files::find_moved(&folder::catalog().missing(), Path::new(&folder));
        follow(&found, collections.as_deref());
        // Read where they are now, so All photos lists them at once.
        folder::catalog().records(&found.iter().map(|f| f.1.clone()).collect::<Vec<_>>());
        Ok(found)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Every keyword in the catalogue and how many photos have it, A to Z.
#[tauri::command]
pub(crate) async fn keywords_all() -> Res<Vec<(String, usize)>> {
    tauri::async_runtime::spawn_blocking(|| {
        let mut counts: BTreeMap<String, (String, usize)> = BTreeMap::new();
        for (_, record) in current_records() {
            for k in &record.info.keywords {
                counts
                    .entry(k.to_lowercase())
                    .or_insert_with(|| (k.clone(), 0))
                    .1 += 1;
            }
        }
        Ok(counts.into_values().collect())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Rename or merge keywords (`from` become `to`), or remove them (`to`
/// empty), on every photo in the catalogue that has them and can be
/// reached. Returns how many photos changed.
#[tauri::command]
pub(crate) async fn keywords_change(
    from: Vec<String>,
    to: String,
    state: tauri::State<'_, AppState>,
) -> Res<usize> {
    let slot = state.session.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let wanted: Vec<String> = from.iter().map(|k| k.trim().to_lowercase()).collect();
        let change = library::InfoChange {
            remove_keywords: from.clone(),
            add_keywords: Some(to.trim().to_string())
                .filter(|t| !t.is_empty())
                .into_iter()
                .collect(),
            ..Default::default()
        };
        let mut n = 0;
        for (path, record) in current_records() {
            let has = record
                .info
                .keywords
                .iter()
                .any(|k| wanted.contains(&k.trim().to_lowercase()));
            if has {
                change_one(&slot, &path.display().to_string(), &change)?;
                n += 1;
            }
        }
        Ok(n)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Every photo the catalogue knows that can be reached, with its record
/// read again where its file or sidecar changed since.
fn current_records() -> Vec<(PathBuf, amb_library::catalog::Record)> {
    let paths: Vec<PathBuf> = folder::catalog()
        .all()
        .into_iter()
        .filter(|e| !e.offline)
        .map(|e| e.path)
        .collect();
    let records = folder::catalog().records(&paths);
    paths.into_iter().zip(records).collect()
}

/// A saved set of metadata to give photos at once (Lightroom's metadata
/// presets): title, caption, creator, copyright and keywords.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct MetadataTemplate {
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) title: Option<String>,
    #[serde(default)]
    pub(crate) caption: Option<String>,
    #[serde(default)]
    pub(crate) creator: Option<String>,
    #[serde(default)]
    pub(crate) copyright: Option<String>,
    #[serde(default)]
    pub(crate) keywords: Vec<String>,
}

#[tauri::command]
pub(crate) fn metadata_templates(state: tauri::State<'_, AppState>) -> Res<Vec<MetadataTemplate>> {
    Ok(state
        .metadata_templates
        .lock()
        .map_err(|e| e.to_string())?
        .clone())
}

/// Save a template (replacing one with its name), or with no fields but
/// the name, delete it.
#[tauri::command]
pub(crate) fn save_metadata_template(
    template: MetadataTemplate,
    delete: Option<bool>,
    state: tauri::State<'_, AppState>,
) -> Res<Vec<MetadataTemplate>> {
    let name = template.name.trim().to_string();
    if name.is_empty() {
        return Err("the template needs a name".into());
    }
    {
        let mut all = state.metadata_templates.lock().map_err(|e| e.to_string())?;
        all.retain(|t| !t.name.eq_ignore_ascii_case(&name));
        if delete != Some(true) {
            all.push(MetadataTemplate { name, ..template });
            all.sort_by_key(|t| t.name.to_lowercase());
        }
    }
    save_config(state.inner());
    metadata_templates(state)
}
