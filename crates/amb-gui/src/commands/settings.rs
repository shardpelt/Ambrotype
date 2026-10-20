// SPDX-License-Identifier: GPL-3.0-or-later
//! Settings, shortcuts, About and feedback.

use crate::*;

/// Where feedback goes: a pre-filled GitHub issue (nothing is sent until the
/// user submits it there).
pub(crate) const FEEDBACK_URL: &str = "https://github.com/shardpelt/Ambrotype/issues/new";

pub(crate) fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// The feedback text that would be sent (shown to the user before opening).
#[tauri::command]
pub(crate) fn feedback_body(
    kind: String,
    text: String,
    include_context: bool,
    state: tauri::State<'_, AppState>,
) -> String {
    let mut body = format!("**Kind:** {kind}\n\n{}\n", text.trim());
    if include_context {
        body.push_str("\n**Context** (added by Ambrotype; no photo is attached)\n");
        match state.session.lock().ok().as_ref().and_then(|g| g.as_ref()) {
            Some(s) => body.push_str(&s.feedback_context(true)),
            None => body.push_str(&format!(
                "- Ambrotype {} on {} ({}), no photo open\n",
                env!("CARGO_PKG_VERSION"),
                std::env::consts::OS,
                std::env::consts::ARCH
            )),
        }
    }
    body
}

/// The new-issue address for a piece of feedback. It goes through the
/// repository's issue forms (`.github/ISSUE_TEMPLATE`): their labels apply
/// for anyone, where a `labels=` in the address only works for maintainers.
/// The text fills the form's first box (`what`).
fn feedback_url(kind: &str, title: &str, body: &str) -> String {
    let template = if kind == "Idea" {
        "feature.yml"
    } else {
        "bug.yml"
    };
    let mut url = format!(
        "{FEEDBACK_URL}?template={template}&title={}&what={}",
        percent_encode(&format!("[{kind}] {}", title.trim())),
        percent_encode(body)
    );
    if template == "bug.yml" {
        url.push_str(&format!(
            "&system={}&version={}",
            percent_encode(&format!(
                "{} ({})",
                std::env::consts::OS,
                std::env::consts::ARCH
            )),
            percent_encode(env!("CARGO_PKG_VERSION"))
        ));
    }
    url
}

/// Open the pre-filled GitHub issue in the browser.
#[tauri::command]
pub(crate) fn open_feedback(
    app: tauri::AppHandle,
    kind: String,
    title: String,
    body: String,
) -> Res<()> {
    // Keep URLs within what browsers and GitHub accept.
    let mut body = body;
    if body.len() > 6000 {
        let mut cut = 6000;
        while !body.is_char_boundary(cut) {
            cut -= 1;
        }
        body.truncate(cut);
        body.push_str("\n… (truncated)");
    }
    let url = feedback_url(&kind, &title, &body);
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| e.to_string())
}

/// The keyboard shortcuts the user changed (the UI knows the defaults).
#[tauri::command]
pub(crate) fn keybindings(state: tauri::State<'_, AppState>) -> Res<keys::Bindings> {
    Ok(state.keybindings.lock().map_err(|e| e.to_string())?.clone())
}

#[tauri::command]
pub(crate) fn set_keybindings(
    bindings: keys::Bindings,
    state: tauri::State<'_, AppState>,
) -> Res<()> {
    let clean = keys::validate(&bindings)?;
    *state.keybindings.lock().map_err(|e| e.to_string())? = clean;
    save_config(state.inner());
    Ok(())
}

#[derive(Serialize)]
pub(crate) struct SettingsInfo {
    pub(crate) version: &'static str,
    pub(crate) folder: Option<String>,
}

/// For the Settings dialog: the app version and where its settings live.
#[tauri::command]
pub(crate) fn settings_info(state: tauri::State<'_, AppState>) -> SettingsInfo {
    let folder = state
        .config_path
        .lock()
        .ok()
        .and_then(|p| Some(p.as_ref()?.parent()?.display().to_string()));
    SettingsInfo {
        version: env!("CARGO_PKG_VERSION"),
        folder,
    }
}

/// Where the source code is (the GPL's "corresponding source").
pub(crate) const SOURCE_URL: &str = "https://github.com/shardpelt/Ambrotype";

/// For Settings → About: the licence and notices, built into the app so
/// they can be read offline.
#[tauri::command]
pub(crate) fn about_text(which: String) -> Res<&'static str> {
    match which.as_str() {
        "licence" => Ok(include_str!("../../../../LICENSE")),
        "notice" => Ok(include_str!("../../../../NOTICE")),
        _ => Err(format!("no text {which:?}")),
    }
}

#[tauri::command]
pub(crate) fn open_source_page(app: tauri::AppHandle) -> Res<()> {
    app.opener()
        .open_url(SOURCE_URL, None::<&str>)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) fn open_settings_folder(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Res<()> {
    let folder = settings_info(state)
        .folder
        .ok_or("the settings folder isn't available")?;
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    app.opener()
        .open_path(folder, None::<&str>)
        .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------- custom interfaces

/// Where installed custom interfaces live: `interfaces/<name>/` in the settings folder.
pub(crate) fn interfaces_dir(state: &AppState) -> Res<PathBuf> {
    let path = state.config_path.lock().map_err(|e| e.to_string())?;
    path.as_ref()
        .and_then(|p| p.parent())
        .map(|d| d.join("interfaces"))
        .ok_or_else(|| "the settings folder isn't available".into())
}

#[derive(Serialize)]
pub(crate) struct InterfaceEntry {
    pub(crate) path: PathBuf,
    #[serde(flatten)]
    pub(crate) manifest: crate::interface::Manifest,
}

#[derive(Serialize)]
pub(crate) struct Interfaces {
    /// The one chosen, if not the built-in.
    pub(crate) active: Option<PathBuf>,
    pub(crate) installed: Vec<InterfaceEntry>,
    pub(crate) folder: PathBuf,
}

#[tauri::command]
pub(crate) fn interfaces(state: tauri::State<'_, AppState>) -> Res<Interfaces> {
    let folder = interfaces_dir(state.inner())?;
    let mut installed: Vec<InterfaceEntry> = std::fs::read_dir(&folder)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| crate::interface::is_interface(p))
                .map(|path| InterfaceEntry {
                    manifest: crate::interface::manifest(&path),
                    path,
                })
                .collect()
        })
        .unwrap_or_default();
    installed.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(Interfaces {
        active: state.interface.lock().ok().and_then(|i| i.clone()),
        installed,
        folder,
    })
}

/// Show a custom interface (a folder with an `index.html`), or the built-in
/// one with none. The window switches at once; the choice is kept.
#[tauri::command]
pub(crate) fn set_interface(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    path: Option<String>,
) -> Res<()> {
    let dir = path.map(PathBuf::from);
    if let Some(d) = &dir {
        if !crate::interface::is_interface(d) {
            return Err(format!("{} has no index.html", d.display()));
        }
    }
    if let Ok(mut i) = state.interface.lock() {
        *i = dir.clone();
    }
    save_config(&state);
    show_interface(&app, dir)
}

/// Point the window at a custom interface, or the built-in one with none.
pub(crate) fn show_interface(app: &tauri::AppHandle, dir: Option<PathBuf>) -> Res<()> {
    let custom = dir.is_some();
    crate::interface::set_active(dir);
    // The photo that's open opens again in the new page, edits and all.
    let state = app.state::<AppState>();
    let open = state
        .session
        .lock()
        .ok()
        .and_then(|s| s.as_ref().map(|s| s.path.display().to_string()));
    if let (Some(path), Ok(mut initial)) = (open, state.initial_path.lock()) {
        initial.get_or_insert(path);
    }
    // The built-in interface puts up its own menus when it loads.
    if custom {
        crate::commands::menu::interface_menu(app)?;
    }
    let url = if custom {
        crate::interface::custom_url()
    } else {
        crate::interface::builtin_url()
    };
    let window = app.get_webview_window("main").ok_or("no main window")?;
    window
        .navigate(url.parse().map_err(|e| format!("{e}"))?)
        .map_err(|e| e.to_string())
}

/// What sharing the interface in `path` to the hub would send.
#[tauri::command]
pub(crate) async fn interface_share_plan(
    path: String,
    state: tauri::State<'_, AppState>,
) -> Res<crate::plugins::SharePlan> {
    let index = crate::commands::hub::hub_index(state.inner());
    tauri::async_runtime::spawn_blocking(move || {
        crate::interface::share_plan(Path::new(&path), &index)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Share an interface as a pull request to the hub, like a plugin: with the
/// GitHub CLI, or in the browser (GitHub's upload page, with the folder shown).
#[tauri::command]
pub(crate) async fn share_interface(
    path: String,
    browser: bool,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Res<String> {
    let index = crate::commands::hub::hub_index(state.inner());
    let folder = path.clone();
    let url = tauri::async_runtime::spawn_blocking(move || {
        let plan = crate::interface::share_plan(Path::new(&path), &index)?;
        if browser {
            Ok(plan.upload_url)
        } else {
            crate::plugins::share(&plan)
        }
    })
    .await
    .map_err(|e| e.to_string())??;
    if browser {
        let _ = app.opener().open_path(folder, None::<&str>);
    }
    app.opener()
        .open_url(url.clone(), None::<&str>)
        .map_err(|e| e.to_string())?;
    Ok(url)
}

#[tauri::command]
pub(crate) async fn pick_interface_folder(app: tauri::AppHandle) -> Option<String> {
    app.dialog()
        .file()
        .set_title("Choose an interface folder (with an index.html)")
        .blocking_pick_folder()
        .and_then(|p| p.into_path().ok())
        .map(|p| p.display().to_string())
}

#[tauri::command]
pub(crate) fn open_interfaces_folder(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Res<()> {
    let folder = interfaces_dir(state.inner())?;
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    app.opener()
        .open_path(folder.display().to_string(), None::<&str>)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    /// The address names an issue form that exists, and fields it has.
    #[test]
    fn feedback_fills_the_repository_issue_forms() {
        let root =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.github/ISSUE_TEMPLATE");
        for (kind, template) in [
            ("Bug", "bug.yml"),
            ("Idea", "feature.yml"),
            ("Slow", "bug.yml"),
        ] {
            let url = super::feedback_url(kind, " It crashes ", "Line 1\nLine & 2");
            assert!(url.starts_with(super::FEEDBACK_URL), "{url}");
            assert!(url.contains(&format!("template={template}&")), "{url}");
            assert!(
                url.contains(&format!("title=%5B{kind}%5D%20It%20crashes&")),
                "{url}"
            );
            assert!(url.contains("what=Line%201%0ALine%20%26%202"), "{url}");
            assert!(!url.contains("labels="), "{url}");
            let form = std::fs::read_to_string(root.join(template)).unwrap();
            for field in url.split('&').skip(2).map(|p| p.split('=').next().unwrap()) {
                assert!(
                    form.contains(&format!("id: {field}\n")),
                    "{template} has no field {field}"
                );
            }
        }
    }
}
