// SPDX-License-Identifier: GPL-3.0-or-later
//! The collage maker: templates, projects and export.

use crate::*;

/// Built-in collage templates followed by the user's.
#[tauri::command]
pub(crate) fn collage_templates(
    state: tauri::State<'_, AppState>,
) -> Res<Vec<collages::TemplateEntry>> {
    let custom = state.collage_templates.lock().map_err(|e| e.to_string())?;
    Ok(collages::all(&custom))
}

#[tauri::command]
pub(crate) fn save_collage_template(
    template: collages::TemplateEntry,
    state: tauri::State<'_, AppState>,
) -> Res<(Vec<collages::TemplateEntry>, String)> {
    let id = {
        let mut custom = state.collage_templates.lock().map_err(|e| e.to_string())?;
        collages::upsert(&mut custom, &template)?
    };
    save_config(state.inner());
    Ok((collage_templates(state)?, id))
}

#[tauri::command]
pub(crate) fn delete_collage_template(
    id: String,
    state: tauri::State<'_, AppState>,
) -> Res<Vec<collages::TemplateEntry>> {
    {
        let mut custom = state.collage_templates.lock().map_err(|e| e.to_string())?;
        collages::remove(&mut custom, &id)?;
    }
    save_config(state.inner());
    collage_templates(state)
}

/// Save a collage template as a file to share.
#[tauri::command]
pub(crate) async fn share_collage_template(
    app: tauri::AppHandle,
    template: amb_export::collage::Template,
) -> Res<Option<String>> {
    template.validate()?;
    let Some(path) = app
        .dialog()
        .file()
        .set_title("Share this collage layout")
        .set_file_name(format!("{}.json", template.name))
        .add_filter("Collage layouts", &["json"])
        .blocking_save_file()
        .and_then(|p| p.into_path().ok())
    else {
        return Ok(None);
    };
    let text = serde_json::to_string_pretty(&template).map_err(|e| e.to_string())? + "\n";
    std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(Some(path.display().to_string()))
}

/// Save the collage as a project file (to reopen and edit later). Without a
/// path, asks where.
#[tauri::command]
pub(crate) async fn save_collage_project(
    app: tauri::AppHandle,
    project: collages::ProjectIn,
    path: Option<String>,
    suggested: String,
) -> Res<Option<String>> {
    let path = match path {
        Some(p) => PathBuf::from(p),
        None => {
            let Some(p) = app
                .dialog()
                .file()
                .set_title("Save the collage")
                .set_file_name(format!("{suggested}.{}", collages::PROJECT_EXTENSION))
                .add_filter("Ambrotype collage", &[collages::PROJECT_EXTENSION])
                .blocking_save_file()
                .and_then(|p| p.into_path().ok())
            else {
                return Ok(None);
            };
            p
        }
    };
    let path = if path
        .extension()
        .is_some_and(|e| e == collages::PROJECT_EXTENSION)
    {
        path
    } else {
        path.with_extension(collages::PROJECT_EXTENSION)
    };
    tauri::async_runtime::spawn_blocking(move || {
        collages::save_project(&path, &project).map(|()| Some(path.display().to_string()))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Open a collage project file (given, or picked in a dialog).
#[tauri::command]
pub(crate) async fn open_collage_project(
    app: tauri::AppHandle,
    path: Option<String>,
) -> Res<Option<collages::OpenedProject>> {
    let path = match path {
        Some(p) => PathBuf::from(p),
        None => {
            let Some(p) = app
                .dialog()
                .file()
                .set_title("Open a collage")
                .add_filter("Ambrotype collage", &[collages::PROJECT_EXTENSION])
                .blocking_pick_file()
                .and_then(|p| p.into_path().ok())
            else {
                return Ok(None);
            };
            p
        }
    };
    tauri::async_runtime::spawn_blocking(move || collages::open_project(&path).map(Some))
        .await
        .map_err(|e| e.to_string())?
}

/// Read a shared collage template file (picked in a dialog).
#[tauri::command]
pub(crate) async fn import_collage_template(
    app: tauri::AppHandle,
) -> Res<Option<amb_export::collage::Template>> {
    let Some(path) = app
        .dialog()
        .file()
        .set_title("Open a collage layout")
        .add_filter("Collage layouts", &["json"])
        .blocking_pick_file()
        .and_then(|p| p.into_path().ok())
    else {
        return Ok(None);
    };
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    amb_export::collage::Template::from_json(&text)
        .map(Some)
        .map_err(|e| format!("{}: {e}", path.display()))
}

/// A JPEG of a photo with its edits (crop included), for the collage canvas:
/// the open photo's current edits, or another photo's sidecar.
#[tauri::command]
pub(crate) async fn edited_thumbnail(
    path: String,
    edge: Option<u32>,
    state: tauri::State<'_, AppState>,
) -> Res<Response> {
    let slot = state.session.clone();
    let lens = state.lens.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let lens = lens.get().unwrap_or_else(|| empty_lens());
        let edge = edge.unwrap_or(1024).clamp(64, 2048) as usize;
        let none = Default::default();
        {
            let mut guard = slot.lock().map_err(|e| e.to_string())?;
            if let Some(s) = guard.as_mut().filter(|s| s.path == Path::new(&path)) {
                return Ok(Response::new(
                    s.preview(edge, &none, false, false, lens).jpeg,
                ));
            }
        }
        let mut s = Session::open(Path::new(&path))?;
        Ok(Response::new(
            s.preview(edge, &none, false, false, lens).jpeg,
        ))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Clone, Serialize)]
pub(crate) struct CollageProgress {
    pub(crate) page: usize,
    pub(crate) pages: usize,
    pub(crate) done: usize,
    pub(crate) total: usize,
}

/// Render one collage (each photo with its own edits) and save it.
#[tauri::command]
pub(crate) async fn export_collage(
    app: tauri::AppHandle,
    template: amb_export::collage::Template,
    cells: Vec<collages::CellPhoto>,
    path: String,
    options: ExportOptions,
    state: tauri::State<'_, AppState>,
) -> Res<Exported> {
    use tauri::Emitter;
    let lens = state.lens.clone();
    let slot = state.session.clone();
    tauri::async_runtime::spawn_blocking(move || {
        save_open_edits(&slot)?;
        let s = export_settings(&options)?;
        let lens = lens.get().unwrap_or_else(|| empty_lens());
        let mut progress = |done, total| {
            let _ = app.emit(
                "collage-progress",
                CollageProgress {
                    page: 1,
                    pages: 1,
                    done,
                    total,
                },
            );
        };
        let r = collages::render(&template, &cells, Path::new(&path), &s, lens, &mut progress)?;
        Ok(Exported {
            path,
            width: r.width,
            height: r.height,
            bytes: r.bytes,
            quality: r.quality,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Deserialize)]
pub(crate) struct BatchPhoto {
    pub(crate) path: String,
    /// Width / height of the upright photo (from its thumbnail).
    pub(crate) aspect: f64,
}

/// Many photos into pages of the same layout: DIR/<name>-001.<ext>, ...
#[tauri::command]
pub(crate) async fn export_collage_batch(
    app: tauri::AppHandle,
    template: amb_export::collage::Template,
    photos: Vec<BatchPhoto>,
    options: ExportOptions,
    state: tauri::State<'_, AppState>,
) -> Res<Option<Vec<String>>> {
    use tauri::Emitter;
    let Some(dir) = app
        .dialog()
        .file()
        .set_title("Folder for the collage pages")
        .blocking_pick_folder()
        .and_then(|p| p.into_path().ok())
    else {
        return Ok(None);
    };
    let lens = state.lens.clone();
    let slot = state.session.clone();
    tauri::async_runtime::spawn_blocking(move || {
        save_open_edits(&slot)?;
        template.validate()?;
        let s = export_settings(&options)?;
        let lens = lens.get().unwrap_or_else(|| empty_lens());
        let photos: Vec<(String, f64)> = photos.into_iter().map(|p| (p.path, p.aspect)).collect();
        let written = collages::render_batch(
            &template,
            &photos,
            &dir,
            &s,
            lens,
            &mut |page, pages, done, total| {
                let _ = app.emit(
                    "collage-progress",
                    CollageProgress {
                        page,
                        pages,
                        done,
                        total,
                    },
                );
            },
        )?;
        let written: Vec<String> = written.iter().map(|p| p.display().to_string()).collect();
        Ok(Some(written))
    })
    .await
    .map_err(|e| e.to_string())?
}
