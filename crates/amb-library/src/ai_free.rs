// SPDX-License-Identifier: GPL-3.0-or-later
//! The saved AI-free setting (`amb_presets::ai_free` is the switch itself):
//! `ai-free.json` in the editor's config folder, `{"ai_free": true}`.
//!
//! A missing file means off. A file that exists but can't be read or parsed
//! means ON: someone who asked for AI-free photos is trusted over
//! convenience, so a damaged file never lets a model run.

use std::path::{Path, PathBuf};

const FILE: &str = "ai-free.json";

fn path() -> Option<PathBuf> {
    crate::editor_config_dir().map(|d| d.join(FILE))
}

/// The saved setting.
pub fn load() -> bool {
    path().is_some_and(|p| load_from(&p))
}

/// Save the setting.
pub fn save(on: bool) -> Result<(), String> {
    let p = path().ok_or("No settings folder here to save to")?;
    save_to(&p, on)
}

pub(crate) fn load_from(path: &Path) -> bool {
    match std::fs::read_to_string(path) {
        Ok(text) => serde_json::from_str::<serde_json::Value>(&text)
            .ok()
            .and_then(|v| v.get("ai_free").and_then(|b| b.as_bool()))
            .unwrap_or(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
        Err(_) => true,
    }
}

pub(crate) fn save_to(path: &Path, on: bool) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("Can't save the setting: {e}"))?;
    }
    std::fs::write(path, serde_json::json!({ "ai_free": on }).to_string())
        .map_err(|e| format!("Can't save the setting: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_is_off_saved_is_kept_and_damaged_is_on() {
        let dir = std::env::temp_dir().join(format!("amb-aifree-{}", std::process::id()));
        let file = dir.join(FILE);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(!load_from(&file), "missing file is off");
        save_to(&file, true).unwrap();
        assert!(load_from(&file));
        save_to(&file, false).unwrap();
        assert!(!load_from(&file));
        std::fs::write(&file, "not json").unwrap();
        assert!(load_from(&file), "unparsable file is on");
        std::fs::write(&file, "{}").unwrap();
        assert!(load_from(&file), "a file without the key is on");
        // A directory where the file should be: exists, can't be read.
        std::fs::remove_file(&file).unwrap();
        std::fs::create_dir(&file).unwrap();
        assert!(load_from(&file), "unreadable file is on");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
