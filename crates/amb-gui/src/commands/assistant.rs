// SPDX-License-Identifier: GPL-3.0-or-later
//! The assistant panel: its settings, questions and log.

use crate::*;

#[derive(Serialize)]
pub(crate) struct AssistantSettings {
    pub(crate) provider: assistant::Provider,
    pub(crate) has_key: bool,
}

/// The assistant's provider and model, and whether a key is saved.
#[tauri::command]
pub(crate) async fn assistant_settings(
    state: tauri::State<'_, AppState>,
) -> Res<AssistantSettings> {
    let provider = state.assistant.lock().map_err(|e| e.to_string())?.clone();
    let session_key = state
        .assistant_key
        .lock()
        .map_err(|e| e.to_string())?
        .is_some();
    let p = provider.clone();
    let saved = tauri::async_runtime::spawn_blocking(move || assistant::load_key(&p).is_some())
        .await
        .map_err(|e| e.to_string())?;
    Ok(AssistantSettings {
        provider,
        has_key: saved || session_key,
    })
}

/// Save the assistant's provider and model; a `key` goes to the keychain
/// ("" removes it). Returns a note when the keychain couldn't keep the key.
#[tauri::command]
pub(crate) async fn assistant_save(
    provider: assistant::Provider,
    key: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Res<Option<String>> {
    *state.assistant.lock().map_err(|e| e.to_string())? = provider.clone();
    save_config(&state);
    let Some(key) = key.map(|k| k.trim().to_string()) else {
        return Ok(None);
    };
    let p = provider.clone();
    let k = key.clone();
    let stored = tauri::async_runtime::spawn_blocking(move || assistant::store_key(&p, &k))
        .await
        .map_err(|e| e.to_string())?;
    let mut session_key = state.assistant_key.lock().map_err(|e| e.to_string())?;
    match stored {
        Ok(()) => {
            *session_key = None;
            Ok(None)
        }
        Err(e) => {
            *session_key = (!key.is_empty()).then_some(key);
            Ok(Some(format!(
                "The keychain couldn't keep the key ({e}); it is used until Ambrotype closes."
            )))
        }
    }
}

#[derive(Serialize)]
pub(crate) struct AssistantAnswer {
    #[serde(flatten)]
    pub(crate) outcome: assistant::Outcome,
    pub(crate) state: session::StateInfo,
}

/// One request to the assistant about the open photo; its edits are applied
/// as ordinary commands, one undo step per `amb:` line.
#[tauri::command]
pub(crate) async fn assistant_ask(
    history: Vec<assistant::Message>,
    request: String,
    see: Option<bool>,
    state: tauri::State<'_, AppState>,
) -> Res<AssistantAnswer> {
    let provider = state.assistant.lock().map_err(|e| e.to_string())?.clone();
    let session_key = state
        .assistant_key
        .lock()
        .map_err(|e| e.to_string())?
        .clone();
    let slot = state.session.clone();
    let lens = state.lens.clone();
    let log = assistant_log_path(&state);
    tauri::async_runtime::spawn_blocking(move || {
        let key = session_key.or_else(|| assistant::load_key(&provider));
        let lens = lens.get().unwrap_or_else(|| empty_lens());
        let photo = slot
            .lock()
            .map_err(|e| e.to_string())?
            .as_ref()
            .ok_or("no photo open")?
            .path
            .clone();
        // The photo is locked only to look and to edit, not while the
        // model thinks; a request ends if another photo was opened.
        let with_photo = |f: &mut dyn FnMut(&mut Session) -> Res<String>| -> Res<String> {
            let mut guard = slot.lock().map_err(|e| e.to_string())?;
            match guard.as_mut() {
                Some(s) if s.path == photo => f(s),
                _ => Err("another photo was opened; the request stopped".into()),
            }
        };
        let system = assistant::system_prompt();
        // The photo as edited now, small, when the user lets the model see it.
        let mut picture = || {
            with_photo(&mut |s| {
                let p = s.preview(512, &BTreeMap::new(), false, false, lens);
                Ok(plugins::base64(&p.jpeg))
            })
        };
        let outcome = assistant::converse(
            |messages| assistant::call(&provider, key.as_deref(), &system, messages),
            history,
            &request,
            || with_photo(&mut |s| Ok(s.assistant_context(lens))),
            see.unwrap_or(false)
                .then_some(&mut picture as &mut dyn FnMut() -> Res<String>),
            |items| {
                // Every edit the model asks for is logged, done or not.
                let logged = |result: &Res<String>| {
                    log_assistant(
                        log.as_deref(),
                        serde_json::json!({
                            "photo": photo.display().to_string(),
                            "request": request,
                            "edits": items,
                            "result": match result { Ok(r) => r.clone(), Err(e) => format!("error: {e}") },
                        }),
                    );
                };
                let commands = match amb_protocol::compact::parse(items).map_err(|e| e.to_string()) {
                    Ok(c) => c,
                    Err(e) => {
                        logged(&Err(e.clone()));
                        return Err(e);
                    }
                };
                let done = with_photo(&mut |s| {
                    let all = s.white_balance_from_as_shot(commands.clone());
                    let mut results = s.execute(&all, Some("Assistant"))?;
                    // The as-shot start was ours: its notes would only confuse.
                    for o in results.iter_mut().take(all.len() - commands.len()) {
                        o.notes.clear();
                    }
                    let report = amb_protocol::edit::EditReport {
                        sidecar: s.sidecar.clone(),
                        created: false,
                        results,
                        written: None,
                        dry_run: false,
                    };
                    Ok(report.to_brief())
                });
                logged(&done);
                done
            },
        )?;
        let guard = slot.lock().map_err(|e| e.to_string())?;
        let s = guard.as_ref().ok_or("no photo open")?;
        Ok(AssistantAnswer {
            outcome,
            state: s.state_info(),
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The assistant's log: one JSON line per edit it asked for (the photo,
/// the request, the edits in the compact syntax, and what happened), in
/// the app's settings folder, so the user can see what the model did.
pub(crate) fn assistant_log_path(state: &AppState) -> Option<PathBuf> {
    let path = state.config_path.lock().ok()?;
    Some(path.as_ref()?.parent()?.join("assistant-log.jsonl"))
}

pub(crate) fn log_assistant(path: Option<&Path>, mut entry: serde_json::Value) {
    let Some(path) = path else { return };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    entry["time"] = serde_json::json!(now);
    let line = format!("{entry}\n");
    use std::io::Write;
    let _ = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .and_then(|mut f| f.write_all(line.as_bytes()));
}

/// Show the assistant's log in the file manager's default app.
#[tauri::command]
pub(crate) fn open_assistant_log(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Res<()> {
    let path = assistant_log_path(&state).ok_or("no settings folder")?;
    if !path.is_file() {
        return Err("the assistant hasn't made any edits yet".into());
    }
    app.opener()
        .open_path(path.display().to_string(), None::<&str>)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_assistant_log_gets_one_line_per_edit() {
        let path =
            std::env::temp_dir().join(format!("amb-assistant-log-{}.jsonl", std::process::id()));
        let _ = std::fs::remove_file(&path);
        log_assistant(
            Some(&path),
            serde_json::json!({"edits": "exp=+0.3", "result": "Exposure 0 -> 0.3"}),
        );
        log_assistant(
            Some(&path),
            serde_json::json!({"edits": "nope", "result": "error: unknown"}),
        );
        let text = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<serde_json::Value> = text
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0]["edits"], "exp=+0.3");
        assert!(lines[1]["time"].as_u64().unwrap() > 1_700_000_000);
        log_assistant(None, serde_json::json!({}));
        let _ = std::fs::remove_file(&path);
    }
}
