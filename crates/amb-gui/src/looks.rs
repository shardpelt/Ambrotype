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

pub const LAYOUT_FORMAT: &str = "ambrotype-layout/1";
pub const THEME_FORMAT: &str = "ambrotype-theme/1";
pub const DEFAULT_LAYOUT: &str = "builtin:classic";
pub const DEFAULT_THEME: &str = "builtin:dark";

const BUILTIN_LAYOUTS: [(&str, &str); 5] = [
    (
        "classic",
        include_str!("../data/looks/layouts/classic.json"),
    ),
    (
        "compact",
        include_str!("../data/looks/layouts/compact.json"),
    ),
    (
        "big-controls",
        include_str!("../data/looks/layouts/big-controls.json"),
    ),
    ("studio", include_str!("../data/looks/layouts/studio.json")),
    (
        "left-handed",
        include_str!("../data/looks/layouts/left-handed.json"),
    ),
];
const BUILTIN_THEMES: [(&str, &str); 4] = [
    ("dark", include_str!("../data/looks/themes/dark.json")),
    ("light", include_str!("../data/looks/themes/light.json")),
    (
        "high-contrast",
        include_str!("../data/looks/themes/high-contrast.json"),
    ),
    (
        "neutral-grey",
        include_str!("../data/looks/themes/neutral-grey.json"),
    ),
];

/// Toolbar buttons a layout can place; "|" is a separator. "looks" is always
/// shown (added at the end if a layout leaves it out), so there's a way back.
pub const TOOLBAR: [&str; 14] = [
    "open", "save", "export", "undo", "redo", "before", "crop", "zoom", "wb", "clip", "looks",
    "edit", "settings", "feedback",
];
/// Column items that aren't panels.
pub const BLOCKS: [&str; 3] = ["presets", "history", "histogram"];
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
/// Number tokens (in px) with their allowed range.
pub const NUMBER_TOKENS: [(&str, f64, f64); 1] = [("radius", 0.0, 16.0)];
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

// ---------------------------------------------------------------- validation

/// What a layout can refer to, from the editor schema.
struct Catalog {
    panels: Vec<schema::Panel>,
    kinds: HashMap<String, &'static str>,
}

fn catalog() -> &'static Catalog {
    static CATALOG: OnceLock<Catalog> = OnceLock::new();
    CATALOG.get_or_init(|| {
        let panels = schema::panels();
        let kinds = panels
            .iter()
            .flat_map(|p| p.sections.iter())
            .flat_map(|s| s.controls.iter())
            .map(|c| (c.name.clone(), c.kind))
            .collect();
        Catalog { panels, kinds }
    })
}

/// Sections of the Color Grading panel that the colour wheels draw.
pub fn is_wheel_section(panel: &str, section: &str) -> bool {
    panel == "color_grading" && ["Shadow", "Midtone", "Highlight", "Global"].contains(&section)
}

/// The composite widget a whole schema panel brings with it.
fn panel_composite(panel: &str) -> Option<&'static str> {
    match panel {
        "tone_curve" => Some("point-curve"),
        "color_grading" => Some("color-wheels"),
        _ => None,
    }
}

#[derive(Default)]
struct Checker {
    problems: Vec<Problem>,
    controls: HashSet<String>,
    composites: HashSet<&'static str>,
    blocks: HashSet<String>,
}

impl Checker {
    fn error(&mut self, message: impl Into<String>) {
        self.problems.push(Problem {
            error: true,
            message: message.into(),
        });
    }

    fn warn(&mut self, message: impl Into<String>) {
        self.problems.push(Problem {
            error: false,
            message: message.into(),
        });
    }

    fn known_keys(&mut self, obj: &Map<String, Value>, keys: &[&str], at: &str) {
        for k in obj.keys() {
            if !keys.contains(&k.as_str()) {
                self.warn(format!("{at}: unknown key \"{k}\" is ignored"));
            }
        }
    }

    fn header(&mut self, obj: &Map<String, Value>, format: &str) {
        if obj.get("format").and_then(Value::as_str) != Some(format) {
            self.error(format!("\"format\" must be \"{format}\""));
        }
        match obj.get("name").and_then(Value::as_str) {
            Some(n) if !n.trim().is_empty() => {}
            _ => self.error("\"name\" must be a non-empty string"),
        }
        for key in ["description", "author", "based_on"] {
            if obj.get(key).is_some_and(|v| !v.is_string() && !v.is_null()) {
                self.error(format!("\"{key}\" must be a string"));
            }
        }
    }

    /// A control shown once; `widget` checked against its kind.
    fn control(&mut self, name: &str, widget: Option<&Value>, at: &str) {
        let Some(kind) = catalog().kinds.get(name).copied() else {
            self.warn(format!(
                "{at}: unknown setting \"{name}\" (shown as a placeholder)"
            ));
            return;
        };
        if !self.controls.insert(name.to_string()) {
            self.error(format!(
                "{at}: \"{name}\" appears more than once; each setting can be shown once"
            ));
        }
        self.widget(kind, name, widget, at);
    }

    fn widget(&mut self, kind: &str, name: &str, widget: Option<&Value>, at: &str) {
        let Some(w) = widget else { return };
        let Some(w) = w.as_str() else {
            self.error(format!("{at}: the widget for \"{name}\" must be a string"));
            return;
        };
        if widgets_for(kind).contains(&w) {
            return;
        }
        if all_widgets().any(|x| x == w) {
            self.error(format!(
                "{at}: \"{name}\" is a {kind} setting and can't use the \"{w}\" widget (use one of: {})",
                widgets_for(kind).join(", ")
            ));
        } else {
            self.warn(format!(
                "{at}: unknown widget \"{w}\" for \"{name}\" (the default is used)"
            ));
        }
    }

    fn composite(&mut self, name: &'static str, at: &str) {
        if !self.composites.insert(name) {
            self.error(format!("{at}: the {name} widget appears more than once"));
        }
    }

    /// Every control of a schema panel (or one section), minus `hide`.
    fn schema_controls(
        &mut self,
        panel: &schema::Panel,
        section: Option<&str>,
        widgets: Option<&Map<String, Value>>,
        hide: &[&str],
        at: &str,
    ) {
        let whole = section.is_none();
        if whole {
            if let Some(c) = panel_composite(panel.id) {
                self.composite(c, at);
            }
        }
        for s in &panel.sections {
            if section.is_some_and(|x| x != s.label)
                || (whole && is_wheel_section(panel.id, &s.label))
            {
                continue;
            }
            for c in &s.controls {
                if !hide.contains(&c.name.as_str()) {
                    self.control(&c.name, widgets.and_then(|w| w.get(&c.name)), at);
                }
            }
        }
    }

    fn column(&mut self, side: &str, v: &Value) {
        let Some(obj) = v.as_object() else {
            self.error(format!(
                "\"{side}\" must be an object with \"width\" and \"items\""
            ));
            return;
        };
        self.known_keys(obj, &["width", "items"], side);
        match obj.get("width") {
            None => {}
            Some(w) if w.as_f64().is_some_and(|w| (0.0..=MAX_WIDTH).contains(&w)) => {}
            Some(_) => self.error(format!(
                "{side}.width must be a number from 0 to {MAX_WIDTH}"
            )),
        }
        let Some(items) = obj.get("items") else {
            return;
        };
        let Some(items) = items.as_array() else {
            self.error(format!("{side}.items must be a list"));
            return;
        };
        for (i, item) in items.iter().enumerate() {
            let at = format!("{side}.items[{i}]");
            match item {
                Value::String(b) if BLOCKS.contains(&b.as_str()) => {
                    if !self.blocks.insert(b.clone()) {
                        self.error(format!("{at}: \"{b}\" appears more than once"));
                    }
                }
                Value::String(b) => self.warn(format!(
                    "{at}: unknown block \"{b}\" is ignored (blocks: {})",
                    BLOCKS.join(", ")
                )),
                Value::Object(p) => self.panel(p, &at),
                _ => self.error(format!("{at}: must be a block name or a panel object")),
            }
        }
    }

    fn panel(&mut self, p: &Map<String, Value>, at: &str) {
        if p.get("title").is_some_and(|t| !t.is_string()) {
            self.error(format!("{at}: \"title\" must be a string"));
        }
        if p.get("open").is_some_and(|o| !o.is_boolean()) {
            self.error(format!("{at}: \"open\" must be true or false"));
        }
        if let Some(id) = p.get("panel") {
            self.known_keys(p, &["panel", "title", "open", "widgets", "hide"], at);
            let Some(id) = id.as_str() else {
                self.error(format!("{at}: \"panel\" must be a panel id"));
                return;
            };
            let widgets = match p.get("widgets") {
                None => None,
                Some(Value::Object(w)) => Some(w),
                Some(_) => {
                    self.error(format!(
                        "{at}: \"widgets\" must map setting names to widgets"
                    ));
                    None
                }
            };
            let hide: Vec<&str> = match p.get("hide") {
                None => Vec::new(),
                Some(Value::Array(h)) if h.iter().all(Value::is_string) => {
                    h.iter().filter_map(Value::as_str).collect()
                }
                Some(_) => {
                    self.error(format!("{at}: \"hide\" must be a list of setting names"));
                    Vec::new()
                }
            };
            let Some(panel) = catalog().panels.iter().find(|x| x.id == id) else {
                self.warn(format!("{at}: unknown panel \"{id}\" is ignored"));
                return;
            };
            let in_panel = |n: &str| {
                panel
                    .sections
                    .iter()
                    .any(|s| s.controls.iter().any(|c| c.name == n))
            };
            let named: Vec<&str> = widgets
                .into_iter()
                .flat_map(|w| w.keys().map(String::as_str))
                .chain(hide.iter().copied())
                .collect();
            for n in named {
                if !in_panel(n) {
                    self.warn(format!("{at}: \"{n}\" is not in the {id} panel"));
                }
            }
            self.schema_controls(panel, None, widgets, &hide, at);
            return;
        }
        self.known_keys(p, &["title", "open", "columns", "items"], at);
        if !p.get("title").is_some_and(Value::is_string) {
            self.error(format!(
                "{at}: a panel needs \"panel\" (a built-in panel) or a \"title\""
            ));
        }
        if let Some(c) = p.get("columns") {
            if !c.as_u64().is_some_and(|c| (1..=4).contains(&c)) {
                self.error(format!("{at}: \"columns\" must be 1 to 4"));
            }
        }
        let Some(items) = p.get("items").and_then(Value::as_array) else {
            self.error(format!("{at}: \"items\" must be a list"));
            return;
        };
        for (i, entry) in items.iter().enumerate() {
            let at = format!("{at}.items[{i}]");
            let Some(e) = entry.as_object() else {
                self.error(format!("{at}: must be an object"));
                continue;
            };
            if let Some(name) = e.get("setting") {
                self.known_keys(e, &["setting", "widget", "label"], &at);
                match name.as_str() {
                    Some(name) => self.control(name, e.get("widget"), &at),
                    None => self.error(format!("{at}: \"setting\" must be a setting name")),
                }
                if e.get("label").is_some_and(|l| !l.is_string()) {
                    self.error(format!("{at}: \"label\" must be a string"));
                }
            } else if let Some(include) = e.get("include") {
                self.known_keys(e, &["include"], &at);
                let Some(include) = include.as_str() else {
                    self.error(format!(
                        "{at}: \"include\" must be \"panel\" or \"panel/Section\""
                    ));
                    continue;
                };
                let (id, section) = match include.split_once('/') {
                    Some((p, s)) => (p, Some(s)),
                    None => (include, None),
                };
                match catalog().panels.iter().find(|x| x.id == id) {
                    None => self.warn(format!("{at}: unknown panel \"{id}\" is ignored")),
                    Some(panel) => {
                        if section.is_some_and(|s| !panel.sections.iter().any(|x| x.label == s)) {
                            self.warn(format!(
                                "{at}: the {id} panel has no section \"{}\"",
                                section.unwrap_or_default()
                            ));
                        } else {
                            self.schema_controls(panel, section, None, &[], &at);
                        }
                    }
                }
            } else if let Some(w) = e.get("widget") {
                self.known_keys(e, &["widget"], &at);
                match COMPOSITES.iter().find(|c| Some(**c) == w.as_str()) {
                    Some(c) => self.composite(c, &at),
                    None => self.warn(format!(
                        "{at}: unknown widget {w} is ignored (widgets without a setting: {})",
                        COMPOSITES.join(", ")
                    )),
                }
            } else if let Some(l) = e.get("label") {
                self.known_keys(e, &["label"], &at);
                if !l.is_string() {
                    self.error(format!("{at}: \"label\" must be a string"));
                }
            } else {
                self.error(format!(
                    "{at}: needs one of \"setting\", \"include\", \"widget\" or \"label\""
                ));
            }
        }
    }
}

/// Problems with a layout (empty when it's fine).
pub fn check_layout(v: &Value) -> Vec<Problem> {
    let mut c = Checker::default();
    let Some(obj) = v.as_object() else {
        c.error("a layout must be a JSON object");
        return c.problems;
    };
    c.header(obj, LAYOUT_FORMAT);
    c.known_keys(
        obj,
        &[
            "format",
            "name",
            "description",
            "author",
            "based_on",
            "theme",
            "density",
            "toolbar",
            "left",
            "right",
        ],
        "layout",
    );
    if obj.get("theme").is_some_and(|t| !t.is_string()) {
        c.error("\"theme\" must be a theme id such as \"builtin:dark\"");
    }
    if let Some(d) = obj.get("density") {
        if !d.as_str().is_some_and(|d| DENSITIES.contains(&d)) {
            c.warn(format!(
                "unknown density {d} (one of: {}); \"normal\" is used",
                DENSITIES.join(", ")
            ));
        }
    }
    if let Some(t) = obj.get("toolbar") {
        match t.as_array() {
            None => c.error("\"toolbar\" must be a list of button names"),
            Some(items) => {
                let mut seen = HashSet::new();
                for b in items {
                    match b.as_str() {
                        Some("|") => {}
                        Some(b) if TOOLBAR.contains(&b) => {
                            if !seen.insert(b) {
                                c.error(format!("toolbar: \"{b}\" appears more than once"));
                            }
                        }
                        Some(b) => c.warn(format!(
                            "toolbar: unknown button \"{b}\" is ignored (buttons: {})",
                            TOOLBAR.join(", ")
                        )),
                        None => c.error("toolbar: entries must be strings"),
                    }
                }
            }
        }
    }
    for side in ["left", "right"] {
        if let Some(col) = obj.get(side) {
            c.column(side, col);
        }
    }
    c.problems
}

fn is_hex_colour(s: &str) -> bool {
    s.strip_prefix('#').is_some_and(|h| {
        [3, 4, 6, 8].contains(&h.len()) && h.chars().all(|c| c.is_ascii_hexdigit())
    })
}

/// Problems with a theme (empty when it's fine). Themes hold only values for
/// known tokens, never CSS, so a shared theme can't change anything else.
pub fn check_theme(v: &Value) -> Vec<Problem> {
    let mut c = Checker::default();
    let Some(obj) = v.as_object() else {
        c.error("a theme must be a JSON object");
        return c.problems;
    };
    c.header(obj, THEME_FORMAT);
    c.known_keys(
        obj,
        &[
            "format",
            "name",
            "description",
            "author",
            "based_on",
            "scheme",
            "tokens",
        ],
        "theme",
    );
    if let Some(s) = obj.get("scheme") {
        if !matches!(s.as_str(), Some("dark" | "light")) {
            c.error("\"scheme\" must be \"dark\" or \"light\"");
        }
    }
    let Some(tokens) = obj.get("tokens").and_then(Value::as_object) else {
        c.error("\"tokens\" must be an object of token: value");
        return c.problems;
    };
    for (k, v) in tokens {
        if COLOR_TOKENS.contains(&k.as_str()) {
            if !v.as_str().is_some_and(is_hex_colour) {
                c.error(format!(
                    "token \"{k}\" must be a hex colour such as \"#1b1c1f\""
                ));
            }
        } else if let Some((_, lo, hi)) = NUMBER_TOKENS.iter().find(|(n, ..)| n == k) {
            if !v.as_f64().is_some_and(|n| (*lo..=*hi).contains(&n)) {
                c.error(format!("token \"{k}\" must be a number from {lo} to {hi}"));
            }
        } else {
            c.warn(format!("unknown token \"{k}\" is ignored"));
        }
    }
    c.problems
}

pub fn check(kind: Kind, v: &Value) -> Vec<Problem> {
    match kind {
        Kind::Layout => check_layout(v),
        Kind::Theme => check_theme(v),
    }
}

// ---------------------------------------------------------------- storage

fn look(id: String, kind: Kind, builtin: bool, data: Value) -> Look {
    let problems = check(kind, &data);
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

/// `ambrotype --check-look FILE...`: print each file's problems; false if any has errors.
pub fn check_files(paths: &[String]) -> bool {
    let mut ok = true;
    for path in paths {
        let result = std::fs::read_to_string(path)
            .map_err(|e| e.to_string())
            .and_then(|t| {
                serde_json::from_str::<Value>(&t).map_err(|e| format!("not valid JSON: {e}"))
            })
            .and_then(|data| {
                Kind::of(&data)
                    .map(|k| (k, check(k, &data)))
                    .ok_or_else(|| {
                        format!("\"format\" must be \"{LAYOUT_FORMAT}\" or \"{THEME_FORMAT}\"")
                    })
            });
        match result {
            Err(e) => {
                ok = false;
                println!("{path}: error: {e}");
            }
            Ok((kind, problems)) => {
                let kind = kind.folder().trim_end_matches('s');
                if problems.is_empty() {
                    println!("{path}: ok ({kind})");
                }
                for p in problems {
                    ok &= !p.error;
                    let level = if p.error { "error" } else { "warning" };
                    println!("{path}: {level}: {}", p.message);
                }
            }
        }
    }
    ok
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("amb-looks-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn messages(p: &[Problem]) -> Vec<String> {
        p.iter().map(|p| p.message.clone()).collect()
    }

    #[test]
    fn builtins_are_clean() {
        for kind in [Kind::Layout, Kind::Theme] {
            for look in builtins(kind) {
                assert!(
                    look.problems.is_empty(),
                    "{}: {:?}",
                    look.id,
                    messages(&look.problems)
                );
            }
        }
        // Every built-in theme sets every token, so switching themes changes everything.
        for theme in builtins(Kind::Theme) {
            let tokens = theme.data["tokens"].as_object().unwrap();
            for t in COLOR_TOKENS
                .iter()
                .copied()
                .chain(NUMBER_TOKENS.iter().map(|t| t.0))
            {
                assert!(tokens.contains_key(t), "{} misses {t}", theme.id);
            }
        }
    }

    #[test]
    fn classic_shows_every_editor_control() {
        let classic = builtins(Kind::Layout).remove(0);
        assert_eq!(classic.id, DEFAULT_LAYOUT);
        let mut c = Checker::default();
        for side in ["left", "right"] {
            c.column(side, &classic.data[side]);
        }
        let wheel_tags = 12;
        assert_eq!(c.controls.len() + wheel_tags, catalog().kinds.len());
        assert_eq!(c.composites.len(), 2);
        assert_eq!(c.blocks.len(), 3);
    }

    #[test]
    fn layout_problems_are_specific() {
        let base = |items: Value| json!({ "format": LAYOUT_FORMAT, "name": "T", "right": { "width": 300, "items": items } });
        // Unknown names are warnings: the layout still works.
        let p = check_layout(&base(json!([
            "histogram", "filmstrip",
            { "title": "Mine", "items": [{ "setting": "NoSuchSetting" }, { "widget": "sparkles" }] }
        ])));
        assert!(p.iter().all(|p| !p.error), "{:?}", messages(&p));
        assert_eq!(p.len(), 3);
        // A widget of the wrong kind, a duplicate and a missing title are errors.
        let p = check_layout(&base(json!([
            { "title": "A", "items": [{ "setting": "Exposure2012", "widget": "toggle" }] },
            { "panel": "basic" },
            { "items": [] }
        ])));
        let m = messages(&p);
        assert!(
            m.iter()
                .any(|m| m.contains("can't use the \"toggle\" widget")),
            "{m:?}"
        );
        assert!(
            m.iter()
                .any(|m| m.contains("\"Exposure2012\" appears more than once")),
            "{m:?}"
        );
        assert!(m.iter().any(|m| m.contains("needs \"panel\"")), "{m:?}");
        // Two curves (the tone curve panel brings one) are an error.
        let p = check_layout(&base(json!([
            { "panel": "tone_curve" },
            { "title": "B", "items": [{ "widget": "point-curve" }] }
        ])));
        assert!(
            messages(&p)
                .iter()
                .any(|m| m.contains("point-curve widget appears more than once"))
        );
        // Hiding a setting frees it for another panel; includes count too.
        let p = check_layout(&base(json!([
            { "panel": "basic", "hide": ["Exposure2012"] },
            { "title": "C", "columns": 2, "items": [
                { "setting": "Exposure2012", "widget": "knob" },
                { "include": "detail/Sharpening" }
            ] },
            { "panel": "detail", "hide": [] }
        ])));
        let m = messages(&p);
        assert!(
            m.iter()
                .filter(|m| m.contains("appears more than once"))
                .count()
                >= 1,
            "{m:?}"
        );
        assert!(!m.iter().any(|m| m.contains("Exposure2012")), "{m:?}");
    }

    #[test]
    fn theme_problems() {
        let p = check_theme(&json!({
            "format": THEME_FORMAT, "name": "x", "scheme": "dim",
            "tokens": { "bg": "red", "radius": 40, "sparkle": "#fff", "accent": "#ffcc00" }
        }));
        let m = messages(&p);
        assert_eq!(p.iter().filter(|p| p.error).count(), 3, "{m:?}");
        assert!(m.iter().any(|m| m.contains("unknown token \"sparkle\"")));
        assert!(is_hex_colour("#abc") && is_hex_colour("#aabbccdd") && !is_hex_colour("#ggg"));
    }

    #[test]
    fn save_clone_import_delete() {
        let dir = temp_dir("store");
        let mut data = builtins(Kind::Layout)[0].data.clone();
        data["name"] = json!("My Look!");
        data["based_on"] = json!(DEFAULT_LAYOUT);
        let id = save(&dir, Kind::Layout, None, data.clone()).unwrap();
        assert_eq!(id, "user:my-look");
        // Saving over an existing id keeps the file; a new one gets a fresh name.
        data["density"] = json!("compact");
        assert_eq!(
            save(&dir, Kind::Layout, Some(&id), data.clone()).unwrap(),
            id
        );
        assert_eq!(
            save(&dir, Kind::Layout, None, data.clone()).unwrap(),
            "user:my-look-2"
        );
        let looks = list(Some(&dir), Some("user:my-look"), Some("user:nope"));
        assert_eq!(looks.active_layout, "user:my-look");
        assert_eq!(looks.active_theme, DEFAULT_THEME);
        let mine = looks.layouts.iter().find(|l| l.id == id).unwrap();
        assert!(!mine.builtin && mine.usable);
        assert_eq!(mine.based_on.as_deref(), Some(DEFAULT_LAYOUT));
        assert_eq!(mine.data["density"], "compact");

        // Errors aren't saved; nor is a theme saved as a layout.
        let mut bad = data.clone();
        bad["toolbar"] = json!("open");
        assert!(save(&dir, Kind::Layout, None, bad).is_err());
        assert!(save(&dir, Kind::Theme, None, data.clone()).is_err());

        // Export and import round trip.
        let file = dir.join("shared.json");
        std::fs::write(&file, export_text(Some(&dir), Kind::Layout, &id).unwrap()).unwrap();
        assert_eq!(
            import(&dir, &file).unwrap(),
            (Kind::Layout, "user:my-look-3".into())
        );

        // An unreadable file is listed, not fatal.
        std::fs::write(dir.join("layouts/broken.json"), "{ nope").unwrap();
        let broken = all(Some(&dir), Kind::Layout)
            .into_iter()
            .find(|l| l.id == "user:broken")
            .unwrap();
        assert!(!broken.usable);

        assert!(delete(&dir, Kind::Layout, DEFAULT_LAYOUT).is_err());
        assert!(delete(&dir, Kind::Layout, "user:../x").is_err());
        delete(&dir, Kind::Layout, &id).unwrap();
        assert!(!all(Some(&dir), Kind::Layout).iter().any(|l| l.id == id));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// docs/ai/ui-layouts.md is what AI assistants read: it must name everything a layout can use.
    #[test]
    fn format_reference_is_complete() {
        let doc = include_str!("../../../docs/ai/ui-layouts.md");
        let mut names: Vec<String> = TOOLBAR
            .iter()
            .chain(&BLOCKS)
            .chain(&COMPOSITES)
            .chain(&DENSITIES)
            .chain(&COLOR_TOKENS)
            .copied()
            .chain(all_widgets())
            .chain(NUMBER_TOKENS.iter().map(|t| t.0))
            .map(|n| format!("`{n}`"))
            .collect();
        for p in &catalog().panels {
            names.push(format!("`{}`", p.id));
            names.extend(p.sections.iter().map(|s| format!("`{}`", s.label)));
        }
        let missing: Vec<_> = names.iter().filter(|n| !doc.contains(n.as_str())).collect();
        assert!(
            missing.is_empty(),
            "docs/ai/ui-layouts.md doesn't mention {missing:?}"
        );
    }

    #[test]
    fn slugs() {
        assert_eq!(slug("  Warm & Cosy  v2 "), "warm-cosy-v2");
        assert_eq!(slug("!!!"), "look");
    }
}
