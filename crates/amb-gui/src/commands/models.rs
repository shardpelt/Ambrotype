// SPDX-License-Identifier: GPL-3.0-or-later
//! The AI models behind subject, background and people masks (Settings →
//! AI models): list, install, choose, add your own, remove (`amb-ai`).

use crate::*;

/// The models folder, next to the settings.
pub(crate) fn models_dir(state: &AppState) -> Res<PathBuf> {
    state
        .config_path
        .lock()
        .ok()
        .and_then(|p| {
            p.as_ref()
                .and_then(|p| p.parent().map(|d| d.join("models")))
        })
        .or_else(amb_ai::default_dir)
        .ok_or_else(|| "no settings folder".to_string())
}

#[derive(Serialize)]
pub(crate) struct ModelsList {
    pub(crate) tasks: Vec<(&'static str, &'static str)>,
    pub(crate) models: Vec<amb_ai::ModelState>,
    pub(crate) folder: String,
}

fn list(state: &AppState) -> Res<ModelsList> {
    let dir = models_dir(state)?;
    Ok(ModelsList {
        tasks: amb_ai::TASKS.to_vec(),
        models: amb_ai::list(&dir),
        folder: dir.display().to_string(),
    })
}

/// A change of model changes how masks render: the next preview runs it.
fn forget_renders(state: &AppState) {
    if let Ok(mut slot) = state.session.lock() {
        if let Some(s) = slot.as_mut() {
            s.forget_renders();
        }
    }
}

#[tauri::command]
pub(crate) fn ai_models(state: tauri::State<'_, AppState>) -> Res<ModelsList> {
    list(&state)
}

/// Download a model; `model-progress` events say how far it is.
#[tauri::command]
pub(crate) async fn ai_model_install(
    id: String,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Res<ModelsList> {
    use tauri::Emitter;
    let dir = models_dir(&state)?;
    let shown = id.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let last = std::sync::atomic::AtomicU64::new(u64::MAX);
        amb_ai::install(&dir, &id, &|done, total| {
            let pct = (done * 100).checked_div(total).unwrap_or(0);
            if last.swap(pct, std::sync::atomic::Ordering::Relaxed) != pct {
                let _ = app.emit(
                    "model-progress",
                    serde_json::json!({ "id": id, "percent": pct }),
                );
            }
        })
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| format!("{shown}: {e}"))?;
    list(&state)
}

#[tauri::command]
pub(crate) fn ai_model_use(id: String, state: tauri::State<'_, AppState>) -> Res<ModelsList> {
    amb_ai::choose(&models_dir(&state)?, &id)?;
    forget_renders(&state);
    list(&state)
}

#[tauri::command]
pub(crate) fn ai_model_remove(id: String, state: tauri::State<'_, AppState>) -> Res<ModelsList> {
    amb_ai::remove(&models_dir(&state)?, &id)?;
    forget_renders(&state);
    list(&state)
}

#[tauri::command]
pub(crate) async fn pick_model_file(app: tauri::AppHandle) -> Option<String> {
    app.dialog()
        .file()
        .set_title("Your own model (ONNX)")
        .add_filter("ONNX model", &["onnx"])
        .blocking_pick_file()
        .and_then(|f| f.into_path().ok())
        .map(|p| p.display().to_string())
}

/// Add the user's own model (checked that it runs first).
#[tauri::command]
pub(crate) async fn ai_model_add(
    file: String,
    model: serde_json::Value,
    state: tauri::State<'_, AppState>,
) -> Res<ModelsList> {
    let dir = models_dir(&state)?;
    let m: amb_ai::CustomModel = serde_json::from_value(model).map_err(|e| e.to_string())?;
    tauri::async_runtime::spawn_blocking(move || amb_ai::add_custom(&dir, Path::new(&file), m))
        .await
        .map_err(|e| e.to_string())??;
    list(&state)
}
