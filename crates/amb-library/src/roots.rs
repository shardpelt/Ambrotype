// SPDX-License-Identifier: GPL-3.0-or-later
//! The library folders: folders, or whole drives, whose photos the
//! catalogue keeps indexed (`Catalog::index`), so they can be browsed and
//! searched together, also while a drive is unplugged. The list is the
//! user's, so it lives in `library-folders.json` beside the collections, not
//! in the catalogue (which is only a cache):
//!
//! ```json
//! { "format": "ambrotype-library-folders/1", "folders": ["/Volumes/Photos"] }
//! ```

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const FORMAT: &str = "ambrotype-library-folders/1";

#[derive(Debug, Default, Serialize, Deserialize)]
struct File {
    format: String,
    folders: Vec<PathBuf>,
}

/// Where the editor keeps the list: its config folder.
pub fn default_path() -> Option<PathBuf> {
    Some(
        crate::collections::editor_dir()?
            .parent()?
            .join("library-folders.json"),
    )
}

/// The library folders in `file`; none when it doesn't exist yet.
pub fn load(file: &Path) -> Vec<PathBuf> {
    std::fs::read_to_string(file)
        .ok()
        .and_then(|t| serde_json::from_str::<File>(&t).ok())
        .map(|f| f.folders)
        .unwrap_or_default()
}

fn save(file: &Path, folders: &[PathBuf]) -> Result<(), String> {
    let text = serde_json::to_string_pretty(&File {
        format: FORMAT.into(),
        folders: folders.to_vec(),
    })
    .map_err(|e| e.to_string())?;
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let tmp = file.with_extension("json.tmp");
    std::fs::write(&tmp, text).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, file).map_err(|e| format!("{}: {e}", file.display()))
}

/// Add `folder` (it must exist now). A folder inside one already listed
/// adds nothing; listed folders inside it are replaced by it.
pub fn add(file: &Path, folder: &Path) -> Result<Vec<PathBuf>, String> {
    if !folder.is_dir() {
        return Err(format!("{} is not a folder", folder.display()));
    }
    let folder = std::fs::canonicalize(folder).unwrap_or_else(|_| folder.to_path_buf());
    let mut folders = load(file);
    if folders.iter().any(|f| folder.starts_with(f)) {
        return Ok(folders);
    }
    folders.retain(|f| !f.starts_with(&folder));
    folders.push(folder);
    folders.sort();
    save(file, &folders)?;
    Ok(folders)
}

/// Stop indexing `folder`; its photos stay in the catalogue until forgotten.
pub fn remove(file: &Path, folder: &Path) -> Result<Vec<PathBuf>, String> {
    let mut folders = load(file);
    folders.retain(|f| f != folder);
    save(file, &folders)?;
    Ok(folders)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folders_inside_others_are_not_listed_twice() {
        let dir = std::env::temp_dir().join(format!("amb-roots-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let (a, sub, b) = (dir.join("a"), dir.join("a/2026"), dir.join("b"));
        for d in [&a, &sub, &b] {
            std::fs::create_dir_all(d).unwrap();
        }
        let (a, sub, b) = (
            a.canonicalize().unwrap(),
            sub.canonicalize().unwrap(),
            b.canonicalize().unwrap(),
        );
        let file = dir.join("library-folders.json");
        assert!(load(&file).is_empty());
        assert_eq!(add(&file, &sub).unwrap(), vec![sub.clone()]);
        // The parent replaces the folder inside it; adding that again changes nothing.
        assert_eq!(add(&file, &a).unwrap(), vec![a.clone()]);
        assert_eq!(add(&file, &sub).unwrap(), vec![a.clone()]);
        assert_eq!(add(&file, &b).unwrap(), [a.clone(), b.clone()]);
        assert!(add(&file, &dir.join("nowhere")).is_err());
        assert_eq!(remove(&file, &a).unwrap(), vec![b.clone()]);
        assert_eq!(load(&file), [b]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
