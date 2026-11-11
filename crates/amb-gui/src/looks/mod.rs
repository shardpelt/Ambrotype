// SPDX-License-Identifier: GPL-3.0-or-later
//! Looks: shareable UI layouts and themes (docs/design/customization-and-hub.md,
//! format reference in docs/ai/ui-layouts.md).
//!
//! A *layout* says what goes where: the toolbar, the two side columns, the
//! panels in them and the widget each setting uses. A *theme* is a set of
//! colour tokens. Both are small JSON files. The built-ins ship with the app
//! and are read-only (clone one to change it); the user's own live in the
//! config folder (`looks/layouts`, `looks/themes`), where they can be shared,
//! or written by an AI assistant.
//!
//! The UI renders a layout; this module validates and stores them, so a
//! broken file is reported clearly instead of half-rendering. Problems are
//! errors (the look can't be used) or warnings (unknown names, e.g. from a
//! newer version: shown as placeholders or ignored).

use crate::schema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

mod check;
mod setups;
#[cfg(test)]
mod tests;

pub use check::{check, check_files, check_layout, complete_theme};
pub use setups::{Setup, delete_setup, restore_setup, save_setup, setups};

pub const LAYOUT_FORMAT: &str = "ambrotype-layout/1";
pub const THEME_FORMAT: &str = "ambrotype-theme/1";
pub const DEFAULT_LAYOUT: &str = "builtin:classic";
pub const DEFAULT_THEME: &str = "builtin:dark";

const BUILTIN_LAYOUTS: [(&str, &str); 15] = [
    (
        "classic",
        include_str!("../../data/hub/looks/layouts/classic.json"),
    ),
    (
        "beginner",
        include_str!("../../data/hub/looks/layouts/beginner.json"),
    ),
    (
        "compact",
        include_str!("../../data/hub/looks/layouts/compact.json"),
    ),
    (
        "big-controls",
        include_str!("../../data/hub/looks/layouts/big-controls.json"),
    ),
    (
        "studio",
        include_str!("../../data/hub/looks/layouts/studio.json"),
    ),
    (
        "left-handed",
        include_str!("../../data/hub/looks/layouts/left-handed.json"),
    ),
    (
        "focus",
        include_str!("../../data/hub/looks/layouts/focus.json"),
    ),
    (
        "console",
        include_str!("../../data/hub/looks/layouts/console.json"),
    ),
    (
        "two-benches",
        include_str!("../../data/hub/looks/layouts/two-benches.json"),
    ),
    (
        "film-lab",
        include_str!("../../data/hub/looks/layouts/film-lab.json"),
    ),
    (
        "color-grading",
        include_str!("../../data/hub/looks/layouts/color-grading.json"),
    ),
    (
        "black-and-white",
        include_str!("../../data/hub/looks/layouts/black-and-white.json"),
    ),
    (
        "retouch",
        include_str!("../../data/hub/looks/layouts/retouch.json"),
    ),
    (
        "collage-prep",
        include_str!("../../data/hub/looks/layouts/collage-prep.json"),
    ),
    (
        "minimalism",
        include_str!("../../data/hub/looks/layouts/minimalism.json"),
    ),
];
const BUILTIN_THEMES: [(&str, &str); 13] = [
    (
        "dark",
        include_str!("../../data/hub/looks/themes/dark.json"),
    ),
    (
        "light",
        include_str!("../../data/hub/looks/themes/light.json"),
    ),
    (
        "high-contrast",
        include_str!("../../data/hub/looks/themes/high-contrast.json"),
    ),
    (
        "neutral-grey",
        include_str!("../../data/hub/looks/themes/neutral-grey.json"),
    ),
    (
        "cupertino-light",
        include_str!("../../data/hub/looks/themes/cupertino-light.json"),
    ),
    (
        "cupertino-dark",
        include_str!("../../data/hub/looks/themes/cupertino-dark.json"),
    ),
    (
        "swiss-print",
        include_str!("../../data/hub/looks/themes/swiss-print.json"),
    ),
    (
        "telemetry",
        include_str!("../../data/hub/looks/themes/telemetry.json"),
    ),
    (
        "stone",
        include_str!("../../data/hub/looks/themes/stone.json"),
    ),
    (
        "graphite",
        include_str!("../../data/hub/looks/themes/graphite.json"),
    ),
    (
        "minimalism",
        include_str!("../../data/hub/looks/themes/minimalism.json"),
    ),
    (
        "glass-light",
        include_str!("../../data/hub/looks/themes/glass-light.json"),
    ),
    (
        "glass-dark",
        include_str!("../../data/hub/looks/themes/glass-dark.json"),
    ),
];

/// Toolbar buttons a layout can place; "|" is a separator. "looks" is always
/// shown (added at the end if a layout leaves it out), so there's a way back.
pub const TOOLBAR: [&str; 25] = [
    "open", "library", "save", "export", "collage", "undo", "redo", "auto", "select", "before",
    "crop", "masks", "spots", "zoom", "wb", "clip", "frame", "hub", "plugins", "looks", "edit",
    "settings", "search", "keys", "feedback",
];
/// Column items that aren't panels.
pub const BLOCKS: [&str; 7] = [
    "presets",
    "info",
    "history",
    "histogram",
    "tools",
    "quick",
    "retouch",
];
/// Widgets that edit several settings at once; each can appear once.
pub const COMPOSITES: [&str; 2] = ["point-curve", "color-wheels"];
pub const DENSITIES: [&str; 3] = ["compact", "normal", "large"];
/// Colour tokens a theme sets (CSS custom properties `--<name>`).
pub const COLOR_TOKENS: [&str; 16] = [
    "bg",
    "bg-deep",
    "viewer",
    "panel",
    "panel-2",
    "line",
    "hover",
    "tab-active",
    "text",
    "muted",
    "faint",
    "accent",
    "accent-ink",
    "danger",
    "thumb",
    "track",
];
/// Number tokens (in px) with their allowed range. `glass` above 0 makes
/// panels frosted glass, blurring what's behind them by that much; their
/// colours then want some see-through (`#rrggbbaa`).
pub const NUMBER_TOKENS: [(&str, f64, f64); 2] = [("radius", 0.0, 16.0), ("glass", 0.0, 40.0)];
/// The widest a side column may be, in px.
const MAX_WIDTH: f64 = 800.0;

/// Widgets for a control kind ("number", "bool", "choice"); the first is the default.
pub fn widgets_for(kind: &str) -> &'static [&'static str] {
    match kind {
        "bool" => &["checkbox", "toggle"],
        "choice" => &["select", "segmented"],
        _ => &["slider", "knob", "bar", "number"],
    }
}

/// Every per-setting widget name.
fn all_widgets() -> impl Iterator<Item = &'static str> {
    ["number", "bool", "choice"]
        .into_iter()
        .flat_map(|k| widgets_for(k).iter().copied())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Layout,
    Theme,
}

impl Kind {
    fn folder(self) -> &'static str {
        match self {
            Kind::Layout => "layouts",
            Kind::Theme => "themes",
        }
    }

    /// The kind a file declares through its `format`.
    pub fn of(data: &Value) -> Option<Kind> {
        match data.get("format")?.as_str()? {
            LAYOUT_FORMAT => Some(Kind::Layout),
            THEME_FORMAT => Some(Kind::Theme),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Problem {
    pub error: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Look {
    /// `builtin:<name>` or `user:<file stem>`.
    pub id: String,
    pub kind: Kind,
    pub name: String,
    pub builtin: bool,
    /// The look this one was cloned from, if any.
    pub based_on: Option<String>,
    /// False when there are errors; warnings alone don't stop it being used.
    pub usable: bool,
    pub problems: Vec<Problem>,
    pub data: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct Looks {
    pub layouts: Vec<Look>,
    pub themes: Vec<Look>,
    pub active_layout: String,
    pub active_theme: String,
    /// Where the user's own looks are kept.
    pub folder: Option<String>,
}

// ---------------------------------------------------------------- storage

fn look(id: String, kind: Kind, builtin: bool, mut data: Value) -> Look {
    let problems = check(kind, &data);
    if kind == Kind::Theme && !problems.iter().any(|p| p.error) {
        complete_theme(&mut data);
    }
    let text = |k: &str| data.get(k).and_then(Value::as_str).map(str::to_string);
    Look {
        name: text("name").unwrap_or_else(|| id.clone()),
        based_on: text("based_on"),
        usable: !problems.iter().any(|p| p.error),
        problems,
        id,
        kind,
        builtin,
        data,
    }
}

fn builtins(kind: Kind) -> Vec<Look> {
    let files: &[(&str, &str)] = match kind {
        Kind::Layout => &BUILTIN_LAYOUTS,
        Kind::Theme => &BUILTIN_THEMES,
    };
    files
        .iter()
        .map(|(stem, text)| {
            let data = serde_json::from_str(text).expect("built-in looks are valid JSON");
            look(format!("builtin:{stem}"), kind, true, data)
        })
        .collect()
}

/// The user's looks of one kind, by name; unreadable files are listed as unusable.
fn user_looks(dir: &Path, kind: Kind) -> Vec<Look> {
    let Ok(entries) = std::fs::read_dir(dir.join(kind.folder())) else {
        return Vec::new();
    };
    let mut out: Vec<Look> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .is_some_and(|x| x.eq_ignore_ascii_case("json"))
        })
        .filter_map(|path| {
            let stem = path.file_stem()?.to_str()?.to_string();
            let id = format!("user:{stem}");
            Some(
                match std::fs::read_to_string(&path)
                    .map_err(|e| e.to_string())
                    .and_then(|t| serde_json::from_str::<Value>(&t).map_err(|e| e.to_string()))
                {
                    Ok(data) => look(id, kind, false, data),
                    Err(e) => Look {
                        name: stem,
                        id,
                        kind,
                        builtin: false,
                        based_on: None,
                        usable: false,
                        problems: vec![Problem {
                            error: true,
                            message: format!("can't read {}: {e}", path.display()),
                        }],
                        data: Value::Null,
                    },
                },
            )
        })
        .collect();
    out.sort_by_key(|l| l.name.to_lowercase());
    out
}

/// Built-ins first, then the user's own.
pub fn all(dir: Option<&Path>, kind: Kind) -> Vec<Look> {
    let mut out = builtins(kind);
    if let Some(dir) = dir {
        out.extend(user_looks(dir, kind));
    }
    out
}

/// Everything the Looks dialog shows. Missing or unusable active looks fall
/// back to the defaults.
pub fn list(dir: Option<&Path>, active_layout: Option<&str>, active_theme: Option<&str>) -> Looks {
    let layouts = all(dir, Kind::Layout);
    let themes = all(dir, Kind::Theme);
    let pick = |looks: &[Look], want: Option<&str>, default: &str| {
        want.filter(|w| looks.iter().any(|l| l.id == *w && l.usable))
            .unwrap_or(default)
            .to_string()
    };
    Looks {
        active_layout: pick(&layouts, active_layout, DEFAULT_LAYOUT),
        active_theme: pick(&themes, active_theme, DEFAULT_THEME),
        layouts,
        themes,
        folder: dir.map(|d| d.display().to_string()),
    }
}

/// A file name from a look's name: lower case, letters, digits and dashes.
fn slug(name: &str) -> String {
    let mut s = String::new();
    for c in name.trim().chars().flat_map(char::to_lowercase) {
        if c.is_alphanumeric() {
            s.push(c);
        } else if !s.ends_with('-') && !s.is_empty() {
            s.push('-');
        }
    }
    let s = s.trim_end_matches('-').chars().take(60).collect::<String>();
    if s.is_empty() { "look".into() } else { s }
}

fn user_path(dir: &Path, kind: Kind, id: &str) -> Option<PathBuf> {
    let stem = id.strip_prefix("user:")?;
    (!stem.is_empty() && !stem.contains(['/', '\\', '.']))
        .then(|| dir.join(kind.folder()).join(format!("{stem}.json")))
}

/// Save a look of the user's: over `id` when it's an existing user look,
/// otherwise as a new file named after it. Returns its id. Looks with errors
/// aren't saved.
pub fn save(dir: &Path, kind: Kind, id: Option<&str>, mut data: Value) -> Result<String, String> {
    if Kind::of(&data) != Some(kind) {
        return Err(format!(
            "not a {} file (\"format\" must be \"{}\")",
            kind.folder().trim_end_matches('s'),
            match kind {
                Kind::Layout => LAYOUT_FORMAT,
                Kind::Theme => THEME_FORMAT,
            }
        ));
    }
    if let Some(obj) = data.as_object_mut() {
        obj.remove("id");
    }
    let errors: Vec<String> = check(kind, &data)
        .into_iter()
        .filter(|p| p.error)
        .map(|p| p.message)
        .collect();
    if !errors.is_empty() {
        return Err(errors.join("\n"));
    }
    let folder = dir.join(kind.folder());
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let path = match id.and_then(|id| user_path(dir, kind, id)) {
        Some(p) if p.exists() => p,
        _ => {
            let base = slug(data.get("name").and_then(Value::as_str).unwrap_or(""));
            let mut n = 1;
            loop {
                let stem = if n == 1 {
                    base.clone()
                } else {
                    format!("{base}-{n}")
                };
                let p = folder.join(format!("{stem}.json"));
                if !p.exists() {
                    break p;
                }
                n += 1;
            }
        }
    };
    let text = serde_json::to_string_pretty(&data).map_err(|e| e.to_string())?;
    std::fs::write(&path, text + "\n").map_err(|e| e.to_string())?;
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    Ok(format!("user:{stem}"))
}

pub fn delete(dir: &Path, kind: Kind, id: &str) -> Result<(), String> {
    if id.starts_with("builtin:") {
        return Err("built-in looks can't be deleted".into());
    }
    let path = user_path(dir, kind, id).ok_or_else(|| format!("unknown look {id}"))?;
    std::fs::remove_file(&path).map_err(|e| format!("{}: {e}", path.display()))
}

/// Read a shared look file and save it as one of the user's.
pub fn import(dir: &Path, path: &Path) -> Result<(Kind, String), String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let data: Value = serde_json::from_str(&text)
        .map_err(|e| format!("{}: not valid JSON: {e}", path.display()))?;
    let kind = Kind::of(&data).ok_or_else(|| {
        format!(
            "{}: not an Ambrotype layout or theme (\"format\" is missing)",
            path.display()
        )
    })?;
    save(dir, kind, None, data).map(|id| (kind, id))
}

/// The JSON of a look, for sharing.
pub fn export_text(dir: Option<&Path>, kind: Kind, id: &str) -> Result<String, String> {
    let look = all(dir, kind)
        .into_iter()
        .find(|l| l.id == id)
        .ok_or_else(|| format!("unknown look {id}"))?;
    serde_json::to_string_pretty(&look.data)
        .map(|t| t + "\n")
        .map_err(|e| e.to_string())
}
