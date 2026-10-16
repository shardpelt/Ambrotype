// SPDX-License-Identifier: GPL-3.0-or-later
//! Collections, smart collections, the catalogue, import and moving from Lightroom.

use crate::*;

/// Where collections are kept: the app's config folder.
pub(crate) fn collections_dir(app: &tauri::AppHandle) -> Res<PathBuf> {
    app.path()
        .app_config_dir()
        .map(|d| d.join("collections"))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) fn collections_list(
    app: tauri::AppHandle,
) -> Res<Vec<amb_library::collections::Summary>> {
    Ok(amb_library::collections::list(&collections_dir(&app)?))
}

#[derive(Serialize)]
pub(crate) struct CollectionView {
    pub(crate) name: String,
    pub(crate) photos: Vec<library::LibraryPhoto>,
    /// Photos moved or deleted since they were added.
    pub(crate) missing: Vec<String>,
}

/// A collection's photos for the library.
#[tauri::command]
pub(crate) async fn collection_photos(app: tauri::AppHandle, name: String) -> Res<CollectionView> {
    let dir = collections_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let (_, c) = amb_library::collections::find(&dir, &name)
            .ok_or_else(|| format!("no collection {name:?}"))?;
        let (photos, missing) = library::list_paths(&c.photos);
        Ok(CollectionView {
            name: c.name,
            photos,
            missing,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

pub(crate) fn summary(
    c: amb_library::collections::Collection,
) -> amb_library::collections::Summary {
    c.into()
}

/// Put a collection in a collection set ("" for none).
#[tauri::command]
pub(crate) fn collection_set(
    app: tauri::AppHandle,
    name: String,
    set: String,
) -> Res<amb_library::collections::Summary> {
    amb_library::collections::move_to_set(&collections_dir(&app)?, &name, &set).map(summary)
}

/// Add photos to a collection (made if it doesn't exist).
#[tauri::command]
pub(crate) fn collection_add(
    app: tauri::AppHandle,
    name: String,
    paths: Vec<String>,
) -> Res<amb_library::collections::Summary> {
    let paths: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
    amb_library::collections::add(&collections_dir(&app)?, &name, &paths).map(summary)
}

#[tauri::command]
pub(crate) fn collection_remove(
    app: tauri::AppHandle,
    name: String,
    paths: Vec<String>,
) -> Res<amb_library::collections::Summary> {
    let paths: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
    amb_library::collections::remove(&collections_dir(&app)?, &name, &paths).map(summary)
}

#[tauri::command]
pub(crate) fn collection_rename(
    app: tauri::AppHandle,
    from: String,
    to: String,
) -> Res<amb_library::collections::Summary> {
    amb_library::collections::rename(&collections_dir(&app)?, &from, &to).map(summary)
}

#[tauri::command]
pub(crate) fn collection_delete(app: tauri::AppHandle, name: String) -> Res<()> {
    amb_library::collections::delete(&collections_dir(&app)?, &name)
}

#[tauri::command]
pub(crate) fn smart_list(app: tauri::AppHandle) -> Res<Vec<amb_library::smart::SmartCollection>> {
    Ok(amb_library::smart::all(&collections_dir(&app)?))
}

/// Save a smart collection (replacing `previous`, when renamed).
#[tauri::command]
pub(crate) fn smart_save(
    app: tauri::AppHandle,
    collection: amb_library::smart::SmartCollection,
    previous: Option<String>,
) -> Res<()> {
    let mut c = collection;
    c.format = amb_library::smart::FORMAT.into();
    c.name = c.name.trim().to_string();
    amb_library::smart::save(&collections_dir(&app)?, &c, previous.as_deref())
}

#[tauri::command]
pub(crate) fn smart_delete(app: tauri::AppHandle, name: String) -> Res<()> {
    amb_library::smart::delete(&collections_dir(&app)?, &name)
}

/// A smart collection's photos, worked out now.
#[tauri::command]
pub(crate) async fn smart_photos(app: tauri::AppHandle, name: String) -> Res<CollectionView> {
    let dir = collections_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let (_, s) = amb_library::smart::find(&dir, &name)
            .ok_or_else(|| format!("no smart collection {name:?}"))?;
        Ok(CollectionView {
            photos: library::smart_photos(&s),
            name: s.name,
            missing: Vec::new(),
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Every photo the catalogue knows, newest first.
#[tauri::command]
pub(crate) async fn catalog_photos() -> Res<CollectionView> {
    tauri::async_runtime::spawn_blocking(|| CollectionView {
        name: "All photos".into(),
        photos: library::all_photos(),
        missing: Vec::new(),
    })
    .await
    .map_err(|e| e.to_string())
}

/// Forget the catalogue's photos in `folder` (the files stay). They come
/// back when the folder is shown again.
#[tauri::command]
pub(crate) fn catalog_forget(folder: String) -> usize {
    folder::catalog().forget_folder(Path::new(&folder))
}

#[derive(Serialize)]
pub(crate) struct ImportDefaults {
    pub(crate) dest: Option<String>,
    pub(crate) options: amb_library::import::ImportOptions,
    /// Template tokens and what they stand for.
    pub(crate) tokens: Vec<(String, String)>,
}

/// The import dialog's starting choices: the last ones used.
#[tauri::command]
pub(crate) fn import_defaults(state: tauri::State<'_, AppState>) -> ImportDefaults {
    let last = state.import_choice.lock().ok().and_then(|c| c.clone());
    ImportDefaults {
        dest: last.as_ref().map(|c| c.dest.clone()),
        options: last.map(|c| c.options).unwrap_or_default(),
        tokens: amb_library::import::TOKENS
            .iter()
            .map(|(t, d)| (t.to_string(), d.to_string()))
            .collect(),
    }
}

/// Pick a Lightroom Classic catalogue.
#[tauri::command]
pub(crate) async fn pick_lrcat(app: tauri::AppHandle) -> Option<String> {
    app.dialog()
        .file()
        .set_title("Move from Lightroom Classic: choose its catalogue")
        .add_filter("Lightroom Classic catalogues", &["lrcat"])
        .blocking_pick_file()
        .and_then(|p| p.into_path().ok())
        .map(|p| p.display().to_string())
}

pub(crate) fn lrcat_options(from: &str, to: &str) -> amb_library::lrcat::Options {
    let mut options = amb_library::lrcat::Options::default();
    if !from.trim().is_empty() && !to.trim().is_empty() {
        options
            .remap
            .push((from.trim().to_string(), to.trim().to_string()));
    }
    options
}

/// What moving from a Lightroom catalogue would do, nothing written.
#[tauri::command]
pub(crate) async fn lrcat_plan(
    catalog: String,
    from: String,
    to: String,
) -> Res<serde_json::Value> {
    tauri::async_runtime::spawn_blocking(move || {
        let plan = amb_library::lrcat::plan(Path::new(&catalog), &lrcat_options(&from, &to))?;
        let mut v = serde_json::to_value(&plan).map_err(|e| e.to_string())?;
        v["counts"] = serde_json::json!({
            "write": plan.count("write"),
            "keep": plan.count("keep"),
            "missing": plan.count("missing"),
        });
        Ok(v)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Move from a Lightroom catalogue: sidecars next to the photos, and its
/// collections as ours.
#[tauri::command]
pub(crate) async fn lrcat_apply(
    app: tauri::AppHandle,
    catalog: String,
    from: String,
    to: String,
) -> Res<amb_library::lrcat::Report> {
    let dir = collections_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let plan = amb_library::lrcat::plan(Path::new(&catalog), &lrcat_options(&from, &to))?;
        Ok(amb_library::lrcat::apply(&plan, &dir))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Serialize)]
pub(crate) struct ImportRow {
    pub(crate) source: String,
    pub(crate) dest: String,
    /// Companion files (JPEG, sidecar) that go with it.
    pub(crate) extra: usize,
    pub(crate) captured: String,
    pub(crate) skip: Option<String>,
}

/// What an import would do, nothing copied.
#[tauri::command]
pub(crate) async fn import_plan(
    source: String,
    dest: String,
    options: amb_library::import::ImportOptions,
) -> Res<Vec<ImportRow>> {
    tauri::async_runtime::spawn_blocking(move || {
        let plan = amb_library::import::plan(Path::new(&source), Path::new(&dest), &options)?;
        Ok(plan
            .into_iter()
            .map(|p| ImportRow {
                source: p.files[0].0.display().to_string(),
                dest: p.files[0].1.display().to_string(),
                extra: p.files.len() - 1,
                captured: p.captured,
                skip: p.skip,
            })
            .collect())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Import: plan again (the card may have changed) and copy, with
/// "import-progress" events. The choices are remembered.
#[tauri::command]
pub(crate) async fn import_run(
    app: tauri::AppHandle,
    source: String,
    dest: String,
    options: amb_library::import::ImportOptions,
    state: tauri::State<'_, AppState>,
) -> Res<amb_library::import::Report> {
    use tauri::Emitter;
    if let Ok(mut c) = state.import_choice.lock() {
        *c = Some(ImportChoice {
            dest: dest.clone(),
            options: options.clone(),
        });
    }
    save_config(&state);
    tauri::async_runtime::spawn_blocking(move || {
        let plan = amb_library::import::plan(Path::new(&source), Path::new(&dest), &options)?;
        Ok(amb_library::import::run(
            &plan,
            &options,
            &mut |done, total, name| {
                let _ = app.emit(
                    "import-progress",
                    LibraryProgress {
                        done,
                        total,
                        name: name.to_string(),
                    },
                );
            },
        ))
    })
    .await
    .map_err(|e| e.to_string())?
}
