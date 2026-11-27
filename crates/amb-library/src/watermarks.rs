// SPDX-License-Identifier: GPL-3.0-or-later
//! Saved watermarks: a text or logo mark kept under a name, so Export and
//! `amb render --watermark NAME` can pick it. Each is a small JSON file in a
//! folder (the editor's config folder, so `amb` and the editor share them);
//! a logo is copied in beside it as `<name>.png`:
//!
//! ```json
//! { "format": "ambrotype-watermark/1", "name": "Signature", "mark": { "text": "© Ada", … } }
//! ```

use std::path::{Path, PathBuf};

use amb_export::watermark::Watermark;
use serde::{Deserialize, Serialize};

pub const FORMAT: &str = "ambrotype-watermark/1";

/// A watermark as stored. In the file `mark.image` is the logo's file name
/// in the folder; [`load`] gives it as a full path.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedWatermark {
    pub format: String,
    pub name: String,
    pub mark: Watermark,
}

/// The folder the editor keeps its watermarks in.
pub fn editor_dir() -> Option<PathBuf> {
    Some(crate::editor_config_dir()?.join("watermarks"))
}

fn slug(name: &str) -> String {
    crate::slug(name, "watermark")
}

/// Write the mark as `<dir>/<slug>.json` (and its logo as `<slug>.png`),
/// replacing one of the same name.
pub fn save(dir: &Path, name: &str, mark: &Watermark) -> Result<PathBuf, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("give the watermark a name".into());
    }
    mark.validate()?;
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let slug = slug(name);
    let mut stored = mark.clone();
    if let Some(image) = &mark.image {
        let from = if image.is_relative() && dir.join(image).is_file() {
            dir.join(image)
        } else {
            image.clone()
        };
        let file = format!("{slug}.png");
        let to = dir.join(&file);
        let same = match (from.canonicalize(), to.canonicalize()) {
            (Ok(a), Ok(b)) => a == b,
            _ => false,
        };
        if !same {
            std::fs::copy(&from, &to).map_err(|e| format!("the logo {}: {e}", from.display()))?;
        }
        stored.image = Some(PathBuf::from(file));
    }
    let saved = SavedWatermark {
        format: FORMAT.into(),
        name: name.into(),
        mark: stored,
    };
    let path = dir.join(format!("{slug}.json"));
    let text = serde_json::to_string_pretty(&saved).map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}

/// A saved watermark, its logo (if any) as a full path.
pub fn load(path: &Path) -> Result<SavedWatermark, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut saved: SavedWatermark = serde_json::from_str(&text)
        .map_err(|e| format!("{} is not a watermark file: {e}", path.display()))?;
    if saved.format != FORMAT {
        return Err(format!(
            "{}: a watermark in format {:?}; this version reads {FORMAT:?}",
            path.display(),
            saved.format
        ));
    }
    if let (Some(image), Some(dir)) = (&saved.mark.image, path.parent()) {
        if image.is_relative() {
            saved.mark.image = Some(dir.join(image));
        }
    }
    Ok(saved)
}

/// The watermarks in the folder by name; unreadable files are skipped.
pub fn list(dir: &Path) -> Vec<(PathBuf, SavedWatermark)> {
    let Ok(read) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<_> = read
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .filter_map(|p| load(&p).ok().map(|s| (p, s)))
        .collect();
    out.sort_by_key(|(_, s)| s.name.to_lowercase());
    out
}

/// A watermark by `.json` path, or by name (any case).
pub fn find(dir: &Path, name_or_path: &str) -> Result<(PathBuf, SavedWatermark), String> {
    let as_path = Path::new(name_or_path);
    if name_or_path.to_lowercase().ends_with(".json") && as_path.is_file() {
        return Ok((as_path.to_path_buf(), load(as_path)?));
    }
    let wanted = name_or_path.trim().to_lowercase();
    list(dir)
        .into_iter()
        .find(|(_, s)| s.name.to_lowercase() == wanted)
        .ok_or_else(|| {
            format!("No watermark called “{name_or_path}”. `amb watermark list` shows them.")
        })
}

/// Remove the watermark and its logo.
pub fn delete(dir: &Path, name: &str) -> Result<(), String> {
    let (path, saved) = find(dir, name)?;
    std::fs::remove_file(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    if let Some(image) = saved.mark.image.filter(|i| i.starts_with(dir)) {
        let _ = std::fs::remove_file(image);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("amb-watermarks-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn logo(dir: &Path) -> PathBuf {
        std::fs::create_dir_all(dir).unwrap();
        let path = dir.join("logo.png");
        image::RgbaImage::from_pixel(8, 8, image::Rgba([255, 0, 0, 255]))
            .save(&path)
            .unwrap();
        path
    }

    #[test]
    fn a_text_mark_round_trips() {
        let dir = temp("text");
        let mut mark = Watermark::text("© Me");
        mark.rotate = 15.0;
        let path = save(&dir, "My Mark", &mark).unwrap();
        assert_eq!(path, dir.join("my-mark.json"));
        let loaded = load(&path).unwrap();
        assert_eq!((loaded.name.as_str(), loaded.mark), ("My Mark", mark));
        assert_eq!(list(&dir).len(), 1);
        assert!(save(&dir, " ", &Watermark::text("x")).is_err());
        assert!(save(&dir, "Empty", &Watermark::text("")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_logo_is_copied_and_resolved() {
        let dir = temp("logo");
        let src = temp("logo-src");
        let mark = Watermark {
            image: Some(logo(&src)),
            ..Watermark::text("")
        };
        let path = save(&dir, "Logo", &mark).unwrap();
        assert!(dir.join("logo.png").is_file());
        let stored: SavedWatermark =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(stored.mark.image, Some(PathBuf::from("logo.png")));
        let loaded = load(&path).unwrap();
        assert_eq!(loaded.mark.image, Some(dir.join("logo.png")));
        // Saving again from the loaded mark copies nothing onto itself.
        save(&dir, "Logo", &loaded.mark).unwrap();
        assert!(dir.join("logo.png").is_file());
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&src);
    }

    #[test]
    fn delete_removes_the_file_and_its_logo() {
        let dir = temp("delete");
        let src = temp("delete-src");
        let mark = Watermark {
            image: Some(logo(&src)),
            ..Watermark::text("")
        };
        save(&dir, "Gone", &mark).unwrap();
        delete(&dir, "gone").unwrap();
        assert!(!dir.join("gone.json").exists() && !dir.join("gone.png").exists());
        assert!(delete(&dir, "Gone").is_err());
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&src);
    }

    #[test]
    fn find_ignores_case_and_says_what_is_missing() {
        let dir = temp("find");
        save(&dir, "Warm Sig", &Watermark::text("a")).unwrap();
        assert_eq!(find(&dir, "warm SIG").unwrap().1.name, "Warm Sig");
        let path = dir.join("warm-sig.json");
        assert_eq!(
            find(&dir, path.to_str().unwrap()).unwrap().1.name,
            "Warm Sig"
        );
        let err = find(&dir, "nope").unwrap_err();
        assert!(err.contains("No watermark called “nope”") && err.contains("amb watermark list"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn another_format_is_refused() {
        let dir = temp("format");
        let path = save(&dir, "X", &Watermark::text("a")).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        std::fs::write(&path, text.replace("watermark/1", "watermark/9")).unwrap();
        assert!(load(&path).unwrap_err().contains("watermark/9"));
        assert!(list(&dir).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
