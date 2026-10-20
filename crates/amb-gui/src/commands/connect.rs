// SPDX-License-Identifier: GPL-3.0-or-later
//! Connect an AI app: where this computer's `amb` is, for any MCP app or
//! command-line agent, and a terminal to run one in (`ui/connect.js`;
//! docs/ai/mcp.md). Nothing here is tied to one AI maker.

use crate::*;
use std::ffi::OsString;

#[derive(Serialize)]
pub(crate) struct AiConnect {
    /// The `amb` program, if it's found.
    pub(crate) amb: Option<PathBuf>,
    /// Where a terminal opens: the open photo's folder, or home.
    pub(crate) folder: PathBuf,
}

/// `amb`: next to the editor (the installers put it there), else on the PATH.
fn amb_path() -> Option<PathBuf> {
    let name = format!("amb{}", std::env::consts::EXE_SUFFIX);
    std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|d| d.join(&name)))
        .filter(|p| p.is_file())
        .or_else(|| {
            let paths = std::env::var_os("PATH")?;
            std::env::split_paths(&paths)
                .map(|d| d.join(&name))
                .find(|p| p.is_file())
        })
}

fn work_folder(state: &AppState) -> PathBuf {
    let photo = state.session.lock().ok().and_then(|s| {
        s.as_ref()
            .and_then(|s| s.path.parent().map(Path::to_path_buf))
    });
    photo
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."))
}

#[tauri::command]
pub(crate) fn ai_connect_info(state: tauri::State<'_, AppState>) -> AiConnect {
    AiConnect {
        amb: amb_path(),
        folder: work_folder(state.inner()),
    }
}

/// The PATH with `amb`'s folder first, so typing `amb` works in the terminal.
fn path_with(dir: Option<&Path>) -> Option<OsString> {
    let current = std::env::var_os("PATH").unwrap_or_default();
    let dir = dir?;
    std::env::join_paths(std::iter::once(dir.to_path_buf()).chain(std::env::split_paths(&current)))
        .ok()
}

/// A POSIX shell word for `s`, in single quotes.
fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// macOS opens a `.command` file in Terminal; Terminal starts with its own
/// environment, so the script sets the folder and the PATH itself.
fn mac_script(folder: &Path, amb_dir: Option<&Path>) -> String {
    let path = amb_dir
        .map(|d| {
            format!(
                "export PATH={}:\"$PATH\"\n",
                sh_quote(&d.display().to_string())
            )
        })
        .unwrap_or_default();
    format!(
        "#!/bin/sh\ncd {} || exit\n{path}clear\necho 'Ambrotype: amb is ready (amb --help).'\nexec \"${{SHELL:-/bin/zsh}}\" -l\n",
        sh_quote(&folder.display().to_string())
    )
}

/// Open the system's own terminal in the photo's folder, with `amb` on the
/// PATH, for any command-line AI agent (or by hand).
#[tauri::command]
pub(crate) fn open_terminal(state: tauri::State<'_, AppState>) -> Res<()> {
    let folder = work_folder(state.inner());
    let amb_dir = amb_path().and_then(|p| p.parent().map(Path::to_path_buf));
    let path = path_with(amb_dir.as_deref());
    let mut cmd = if cfg!(target_os = "macos") {
        let script = std::env::temp_dir().join("ambrotype-terminal.command");
        std::fs::write(&script, mac_script(&folder, amb_dir.as_deref()))
            .map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))
                .map_err(|e| e.to_string())?;
        }
        let mut c = std::process::Command::new("open");
        c.args(["-a", "Terminal"]).arg(script);
        c
    } else if cfg!(windows) {
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", "start", "Ambrotype", "/D"])
            .arg(&folder)
            .args(["cmd", "/K"]);
        c
    } else {
        // The desktop's choice first, then the common ones.
        let wanted = std::env::var("TERMINAL").ok();
        let known = [
            "x-terminal-emulator",
            "gnome-terminal",
            "konsole",
            "xfce4-terminal",
            "kitty",
            "alacritty",
            "foot",
            "xterm",
        ];
        let found = wanted
            .iter()
            .map(String::as_str)
            .chain(known)
            .find(|t| which(t, path.as_deref()))
            .ok_or("no terminal app was found (set the TERMINAL variable to yours)")?;
        let mut c = std::process::Command::new(found);
        if found == "gnome-terminal" {
            c.arg(format!("--working-directory={}", folder.display()));
        }
        c
    };
    cmd.current_dir(&folder);
    if let Some(p) = path {
        cmd.env("PATH", p);
    }
    cmd.spawn()
        .map(drop)
        .map_err(|e| format!("couldn't open a terminal: {e}"))
}

fn which(name: &str, path: Option<&std::ffi::OsStr>) -> bool {
    let paths = path
        .map(OsString::from)
        .or_else(|| std::env::var_os("PATH"))
        .unwrap_or_default();
    std::env::split_paths(&paths).any(|d| d.join(name).is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_terminal_starts_in_the_folder_with_amb_first() {
        let s = mac_script(Path::new("/Photos/It's mine"), Some(Path::new("/Apps/A b")));
        assert!(s.contains(r"cd '/Photos/It'\''s mine' || exit"), "{s}");
        assert!(s.contains("export PATH='/Apps/A b':\"$PATH\""), "{s}");
        assert!(!mac_script(Path::new("/p"), None).contains("PATH"));
        let path = path_with(Some(Path::new("/opt/amb"))).unwrap();
        assert_eq!(
            std::env::split_paths(&path).next().unwrap(),
            Path::new("/opt/amb")
        );
        assert!(path_with(None).is_none());
    }
}
