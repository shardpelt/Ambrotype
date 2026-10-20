// SPDX-License-Identifier: GPL-3.0-or-later
//! Custom interfaces: a whole other front end, written by you or an AI
//! agent, in place of the built-in one. It's a folder with an `index.html`
//! (and whatever scripts, styles and pictures it needs), served to the
//! window from `ambui://localhost/` and calling the same commands as the
//! built-in interface (docs/ai/custom-interface.md).
//!
//! It is code with the app's full reach (files, edits, settings), unlike a
//! layout, which is only data: use your own, or ones you have read. Running
//! `ambrotype --builtin-ui` (or `AMB_BUILTIN_UI=1`) starts with the built-in
//! interface whatever is chosen.

use std::path::{Path, PathBuf};
use std::sync::RwLock;

/// The folder the window is showing, when it's a custom interface.
static ACTIVE: RwLock<Option<PathBuf>> = RwLock::new(None);

pub const SCHEME: &str = "ambui";

/// Where a custom interface's page is, on this system.
pub fn custom_url() -> &'static str {
    if cfg!(windows) {
        "http://ambui.localhost/index.html"
    } else {
        "ambui://localhost/index.html"
    }
}

/// Where the built-in interface's page is.
pub fn builtin_url() -> &'static str {
    if cfg!(windows) {
        "http://tauri.localhost/index.html"
    } else {
        "tauri://localhost/index.html"
    }
}

/// Whether this run was told to keep to the built-in interface.
pub fn builtin_forced() -> bool {
    std::env::args().any(|a| a == "--builtin-ui")
        || std::env::var("AMB_BUILTIN_UI").is_ok_and(|v| v == "1")
}

/// A folder is an interface when it has an `index.html`.
pub fn is_interface(dir: &Path) -> bool {
    dir.join("index.html").is_file()
}

pub fn set_active(dir: Option<PathBuf>) {
    if let Ok(mut a) = ACTIVE.write() {
        *a = dir;
    }
}

/// Serve a file of the active custom interface (the `ambui` scheme).
pub fn serve(path: &str) -> tauri::http::Response<Vec<u8>> {
    let not_found = || {
        tauri::http::Response::builder()
            .status(404)
            .body(b"not found".to_vec())
            .unwrap_or_default()
    };
    let Some(root) = ACTIVE.read().ok().and_then(|a| a.clone()) else {
        return not_found();
    };
    let Some(file) = resolve(&root, path) else {
        return not_found();
    };
    match std::fs::read(&file) {
        Ok(bytes) => tauri::http::Response::builder()
            .header("Content-Type", mime(&file))
            .body(bytes)
            .unwrap_or_default(),
        Err(_) => not_found(),
    }
}

/// The file a request path names inside `root`; never one outside it.
fn resolve(root: &Path, path: &str) -> Option<PathBuf> {
    let decoded = percent_decode(path.trim_start_matches('/'));
    let rel = if decoded.is_empty() {
        "index.html".to_string()
    } else {
        decoded
    };
    let root = std::fs::canonicalize(root).ok()?;
    let file = std::fs::canonicalize(root.join(rel)).ok()?;
    (file.starts_with(&root) && file.is_file()).then_some(file)
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
            if let Ok(b) = u8::from_str_radix(hex, 16) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn mime(file: &Path) -> &'static str {
    match file
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("html" | "htm") => "text/html; charset=utf-8",
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("gif") => "image/gif",
        Some("ico") => "image/x-icon",
        Some("woff2") => "font/woff2",
        Some("woff") => "font/woff",
        Some("ttf") => "font/ttf",
        Some("wasm") => "application/wasm",
        _ => "application/octet-stream",
    }
}

/// What an installed interface says about itself (`interface.json`, optional).
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Manifest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
}

pub fn manifest(dir: &Path) -> Manifest {
    std::fs::read_to_string(dir.join("interface.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

/// What sharing an interface would send to the hub: every file in its
/// folder, as `interfaces/<folder name>/` (the hub repo's README).
pub fn share_plan(dir: &Path, index_url: &str) -> Result<crate::plugins::SharePlan, String> {
    use crate::plugins::{ShareFile, SharePlan};
    if !is_interface(dir) {
        return Err(format!("{} has no index.html", dir.display()));
    }
    let name = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .filter(|n| {
            !n.is_empty()
                && n.len() <= 63
                && n.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        })
        .ok_or(
            "name the interface's folder in lower case with dashes (e.g. big-buttons) to share it",
        )?;
    let manifest = manifest(dir);
    let title = manifest
        .name
        .clone()
        .filter(|n| !n.trim().is_empty())
        .ok_or("give it a name in interface.json first (\"name\": \"…\")")?;
    let (repo, branch, folder) = crate::hub::github_repo(index_url).ok_or(
        "sharing needs a hub on GitHub (an address on raw.githubusercontent.com); see Settings → General",
    )?;
    let mut files = Vec::new();
    walk(dir, dir, &mut files)?;
    if files.len() > crate::hub::MAX_INTERFACE_FILES {
        return Err(format!(
            "more than {} files",
            crate::hub::MAX_INTERFACE_FILES
        ));
    }
    let total: u64 = files.iter().map(|f: &ShareFile| f.bytes).sum();
    if total > crate::hub::MAX_INTERFACE_BYTES {
        return Err(format!(
            "{} MB; an interface on the hub is at most {} MB",
            total >> 20,
            crate::hub::MAX_INTERFACE_BYTES >> 20
        ));
    }
    let target = format!("{folder}interfaces/{name}/");
    let body = format!(
        "## {title}\n\n{desc}\n\n- **Author:** {author}\n- **Licence:** {license}\n- **Files:** {count}\n\n## Checklist\n\n- [ ] I made this interface or may share it under its licence\n- [ ] It has a way back to the built-in interface\n- [ ] It loads nothing from the internet\n\n_Sent from Ambrotype's Settings → General → Interface._\n",
        desc = manifest
            .description
            .as_deref()
            .unwrap_or("(no description)"),
        author = manifest.author.as_deref().unwrap_or("(not given)"),
        license = manifest.license.as_deref().unwrap_or("(not given)"),
        count = files.len(),
    );
    Ok(SharePlan {
        id: name.clone(),
        pr_branch: format!("interface-{name}"),
        upload_url: format!("https://github.com/{repo}/upload/{branch}/{target}"),
        title: format!("Add interface: {title}"),
        body,
        gh_user: crate::plugins::gh_user(),
        repo,
        branch,
        target,
        files,
        has_source: true,
    })
}

/// The files of an interface's folder, each one a hub interface may hold
/// (`.hub.json` and other dot files stay behind).
fn walk(root: &Path, at: &Path, out: &mut Vec<crate::plugins::ShareFile>) -> Result<(), String> {
    let mut entries: Vec<_> = std::fs::read_dir(at)
        .map_err(|e| format!("{}: {e}", at.display()))?
        .flatten()
        .collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        if e.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        let path = e.path();
        let kind = e.file_type().map_err(|e| e.to_string())?;
        if kind.is_dir() {
            walk(root, &path, out)?;
        } else if kind.is_file() {
            let rel = path
                .strip_prefix(root)
                .map_err(|e| e.to_string())?
                .components()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            crate::hub::interface_file_ok(&rel)?;
            out.push(crate::plugins::ShareFile {
                bytes: e.metadata().map(|m| m.len()).unwrap_or(0),
                path: rel,
                source: path,
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_files_inside_the_interface_are_served() {
        let dir = std::env::temp_dir().join(format!("amb-ui-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("ui/css")).unwrap();
        std::fs::write(dir.join("ui/index.html"), "<h1>hi</h1>").unwrap();
        std::fs::write(dir.join("ui/css/my style.css"), "h1{}").unwrap();
        std::fs::write(dir.join("secret.txt"), "no").unwrap();
        let root = dir.join("ui");
        assert!(is_interface(&root));
        assert!(resolve(&root, "/").unwrap().ends_with("index.html"));
        assert!(resolve(&root, "/css/my%20style.css").is_some());
        assert!(resolve(&root, "/../secret.txt").is_none());
        assert!(resolve(&root, "/%2E%2E/secret.txt").is_none());
        assert!(resolve(&root, "/nothing.js").is_none());
        set_active(Some(root.clone()));
        let r = serve("/index.html");
        assert_eq!(r.status(), 200);
        assert_eq!(r.headers()["Content-Type"], "text/html; charset=utf-8");
        assert_eq!(serve("/../secret.txt").status(), 404);
        set_active(None);
        assert_eq!(serve("/index.html").status(), 404);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sharing_lists_the_folders_files() {
        let hub = "https://raw.githubusercontent.com/someone/hub/main/index.json";
        let dir = std::env::temp_dir().join(format!("amb-ui-share-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let ui = dir.join("big-buttons");
        std::fs::create_dir_all(ui.join("css")).unwrap();
        std::fs::write(ui.join("index.html"), "<h1>hi</h1>").unwrap();
        std::fs::write(ui.join("css/look.css"), "h1{}").unwrap();
        std::fs::write(ui.join(".hub.json"), "{}").unwrap();
        // A name first.
        assert!(share_plan(&ui, hub).unwrap_err().contains("name"));
        std::fs::write(ui.join("interface.json"), r#"{"name": "Big buttons"}"#).unwrap();
        let plan = share_plan(&ui, hub).unwrap();
        assert_eq!(plan.target, "interfaces/big-buttons/");
        assert_eq!(plan.pr_branch, "interface-big-buttons");
        let files: Vec<_> = plan.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(files, ["css/look.css", "index.html", "interface.json"]);
        // Only what pages are made of.
        std::fs::write(ui.join("run.sh"), "rm -rf /").unwrap();
        assert!(share_plan(&ui, hub).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
