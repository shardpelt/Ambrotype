// SPDX-License-Identifier: GPL-3.0-or-later
//! Styles: a saved look, not saved moves. A preset sets the same values on
//! every photo (+2 Exposure); a style keeps what Match look aims at, the
//! measured look of reference photos, and solves each photo's own settings
//! to reach it (+1 on one photo, +5 on another). Each is a small JSON file
//! in a folder (the editor's config folder, so `amb` and the editor share
//! them):
//!
//! ```json
//! { "format": "ambrotype-style/1", "name": "Warm dusk", "focus": "all", "look": {…}, "references": ["a.jpg"] }
//! ```

use std::path::{Path, PathBuf};

use amb_engine::matching::{Focus, LookStats, MatchResult, Settings};
use amb_presets::registry::Registry;
use amb_protocol::Command;
use serde::{Deserialize, Serialize};

pub const FORMAT: &str = "ambrotype-style/1";

/// A style as stored.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Style {
    pub format: String,
    pub name: String,
    /// What of the look it takes onto a photo.
    pub focus: Focus,
    pub look: LookStats,
    /// The references' file names (not paths), to say what it came from.
    pub references: Vec<String>,
}

/// The folder the editor keeps its styles in.
pub fn editor_dir() -> Option<PathBuf> {
    Some(crate::editor_config_dir()?.join("styles"))
}

/// A style measured from reference photos (JPEGs, or raws with their edits).
pub fn from_photos(name: &str, refs: &[&Path], focus: Focus) -> Result<Style, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("give the style a name".into());
    }
    Ok(Style {
        format: FORMAT.into(),
        name: name.into(),
        focus,
        look: crate::look::target_look(refs)?,
        references: refs
            .iter()
            .filter_map(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .collect(),
    })
}

/// A file name from a style's name: lower case, letters, digits and dashes.
fn slug(name: &str) -> String {
    crate::slug(name, "style")
}

/// Write the style as `<dir>/<slug>.json`, replacing one of the same name.
pub fn save(dir: &Path, style: &Style) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let path = dir.join(format!("{}.json", slug(&style.name)));
    let text = serde_json::to_string_pretty(style).map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}

pub fn load(path: &Path) -> Result<Style, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let style: Style = serde_json::from_str(&text)
        .map_err(|e| format!("{} is not a style file: {e}", path.display()))?;
    if style.format != FORMAT {
        return Err(format!(
            "{}: a style in format {:?}; this version reads {FORMAT:?}",
            path.display(),
            style.format
        ));
    }
    Ok(style)
}

/// The styles in the folder by name; unreadable files are skipped.
pub fn list(dir: &Path) -> Vec<(PathBuf, Style)> {
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

/// A style by `.json` path, or by name (any case).
pub fn find(dir: &Path, name_or_path: &str) -> Result<(PathBuf, Style), String> {
    let as_path = Path::new(name_or_path);
    if name_or_path.to_lowercase().ends_with(".json") && as_path.is_file() {
        return Ok((as_path.to_path_buf(), load(as_path)?));
    }
    let wanted = name_or_path.trim().to_lowercase();
    list(dir)
        .into_iter()
        .find(|(_, s)| s.name.to_lowercase() == wanted)
        .ok_or_else(|| format!("No style called “{name_or_path}”. `amb style list` shows them."))
}

pub fn delete(dir: &Path, name: &str) -> Result<(), String> {
    let (path, _) = find(dir, name)?;
    std::fs::remove_file(&path).map_err(|e| format!("{}: {e}", path.display()))
}

/// The solved settings blended at `amount` (1.0 = 100 %) onto `base`, as
/// commands: the settings that end up different from `base`.
pub fn commands(base: &Settings, result: &MatchResult, amount: f64) -> Vec<Command> {
    let solved: Settings = result.changes.iter().cloned().collect();
    let blended = amb_presets::amount::apply(base, &solved, amount, Registry::builtin());
    let changes: Vec<_> = result
        .changes
        .iter()
        .filter_map(|(name, _)| {
            let v = blended.get(name)?;
            (base.get(name) != Some(v)).then(|| (name.clone(), v.clone()))
        })
        .collect();
    let mut seen = std::collections::BTreeSet::new();
    let changes: Vec<_> = changes
        .into_iter()
        .filter(|(n, _)| seen.insert(n.clone()))
        .collect();
    crate::look::commands(&changes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use amb_presets::Value;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("amb-styles-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn sample(name: &str) -> Style {
        let dir = temp(&format!("photo-{}", name.replace(' ', "")));
        std::fs::create_dir_all(&dir).unwrap();
        let raw = dir.join("ref.dng");
        let img = amb_testkit::patterns::generate("ramp").unwrap();
        amb_testkit::dng::write(&raw, &img, &Default::default()).unwrap();
        from_photos(name, &[raw.as_path()], Focus::Tone).unwrap()
    }

    #[test]
    fn save_load_list_find_delete() {
        let dir = temp("round");
        let warm = sample("Warm Dusk");
        assert_eq!(warm.references, ["ref.dng"]);
        let path = save(&dir, &warm).unwrap();
        assert_eq!(path, dir.join("warm-dusk.json"));
        assert_eq!(load(&path).unwrap(), warm);
        save(&dir, &sample("Another")).unwrap();
        let names: Vec<_> = list(&dir).into_iter().map(|(_, s)| s.name).collect();
        assert_eq!(names, ["Another", "Warm Dusk"]);
        assert_eq!(find(&dir, "warm DUSK").unwrap().1, warm);
        assert_eq!(find(&dir, path.to_str().unwrap()).unwrap().1, warm);
        assert!(find(&dir, "nope").unwrap_err().contains("No style called"));
        std::fs::write(dir.join("junk.json"), "{}").unwrap();
        assert_eq!(list(&dir).len(), 2);
        delete(&dir, "Another").unwrap();
        assert_eq!(list(&dir).len(), 1);
        assert!(delete(&dir, "Another").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_style_from_another_format_says_so() {
        let dir = temp("format");
        let mut s = sample("X");
        s.format = "ambrotype-style/9".into();
        let path = save(&dir, &s).unwrap();
        assert!(load(&path).unwrap_err().contains("ambrotype-style/9"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_amount_blends_the_solved_settings() {
        let base: Settings = [("Exposure2012".to_string(), Value::Number(0.0))].into();
        let result = MatchResult {
            changes: vec![("Exposure2012".into(), Value::Number(2.0))],
            before: 1.0,
            after: 0.0,
            evaluations: 1,
        };
        let value = |amount: f64| {
            let cmds = commands(&base, &result, amount);
            cmds.first().map(|c| match c {
                Command::SetParam { value, .. } => value.as_f64().unwrap(),
                _ => panic!("a set_param"),
            })
        };
        assert_eq!(value(0.0), None);
        assert_eq!(value(1.0), Some(2.0));
        assert_eq!(value(0.5), Some(1.0));
    }
}
