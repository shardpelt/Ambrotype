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
