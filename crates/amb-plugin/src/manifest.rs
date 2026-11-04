// SPDX-License-Identifier: GPL-3.0-or-later
//! `plugin.json`: what a plugin is called, what it may see, and the
//! parameters the app shows for it.
//!
//! ```json
//! { "format": "ambrotype-plugin/1", "id": "auto-tone", "name": "Auto tone",
//!   "version": "1.0.0", "author": "you", "description": "Sets exposure and contrast from the histogram.",
//!   "permissions": ["photo-stats"],
//!   "params": [
//!     { "name": "strength", "label": "Strength", "type": "number", "min": 0, "max": 100, "default": 100 },
//!     { "name": "style", "label": "Style", "type": "choice", "options": ["Natural", "Punchy"], "default": "Natural" },
//!     { "name": "keep_wb", "label": "Keep white balance", "type": "bool", "default": true } ] }
//! ```

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub const FORMAT: &str = "ambrotype-plugin/1";
const MAX_PARAMS: usize = 32;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub format: String,
    /// Lower case letters, digits and dashes; also the folder name.
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub author: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// What the plugin is: "action" (a button that makes edits), "effect"
    /// (a look rendered with the photo, kept in its edit) or "task" (a tab
    /// in the task bar: a layout and tool buttons, no code;
    /// docs/design/plugin-kinds.md).
    #[serde(default = "action")]
    pub kind: String,
    /// For an effect: "color" (one colour to another, made into a colour
    /// table) or "pixels" (the image itself).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub effect: String,
    /// For a pixel effect: how far (in pixels of the whole photo) it looks
    /// at neighbours, so strips it gets overlap by that much (0–256).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub margin: u32,
    #[serde(default)]
    pub permissions: Vec<Permission>,
    #[serde(default)]
    pub params: Vec<Param>,
    /// The module file, next to the manifest.
    #[serde(default = "default_wasm")]
    pub wasm: String,
    /// Where the source lives (a URL), shown to the user.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source: String,
    /// Ready-made sets of parameter values to start from.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recipes: Vec<Recipe>,
    /// For a task: its tab.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task: Option<Task>,
}

/// A task plugin's tab: its name in the task bar, the layout it opens, and
/// the tool buttons its layout's `tools` block shows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Task {
    pub label: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub hint: String,
    /// A layout (`ambrotype-layout/1`, as a layout file holds).
    pub layout: Value,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<Tool>,
}

/// A task's button: what it says and what it does, which is one of
/// `command` (an editor command by its shortcut id, as Keyboard Shortcuts
/// lists them: "auto-edit", "remove-object"), `effect` (an effect plugin
/// put on the photo, from one of its `recipe`s or `params`, in place of
/// the one already there) or `plugin` (an action plugin run with
/// `params`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tool {
    pub label: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub hint: String,
    /// Shown wide, first.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub big: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub command: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub effect: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub plugin: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub recipe: String,
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub params: Map<String, Value>,
}

/// At most this many tool buttons in a task.
const MAX_TOOLS: usize = 24;

/// A named set of parameter values (left-out parameters keep their default).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Recipe {
    pub name: String,
    #[serde(default)]
    pub params: Map<String, Value>,
}

fn action() -> String {
    "action".into()
}

fn is_zero(n: &u32) -> bool {
    *n == 0
}

fn default_wasm() -> String {
    "plugin.wasm".into()
}

/// What a plugin may see besides the develop settings and its parameters.
/// There is no permission for files or the network: plugins never get them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Permission {
    /// Camera, lens, size and exposure of the photo.
    PhotoInfo,
    /// Histograms and the mean colour of a small render of the edited photo.
    PhotoStats,
}

impl Permission {
    pub fn id(self) -> &'static str {
        match self {
            Permission::PhotoInfo => "photo-info",
            Permission::PhotoStats => "photo-stats",
        }
    }

    /// How the app explains it when asking.
    pub fn describe(self) -> &'static str {
        match self {
            Permission::PhotoInfo => {
                "See the camera, lens, size, exposure and white balance of the photo"
            }
            Permission::PhotoStats => "See the photo's histogram and average colour",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Param {
    /// The key the plugin reads (letters, digits, `_`).
    pub name: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(flatten)]
    pub kind: ParamKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ParamKind {
    Number {
        min: f64,
        max: f64,
        default: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        step: Option<f64>,
    },
    Choice {
        options: Vec<String>,
        default: String,
    },
    Bool {
        #[serde(default)]
        default: bool,
    },
}

impl Manifest {
    pub fn is_effect(&self) -> bool {
        self.kind == "effect"
    }

    /// A task has no module: it is a layout and buttons.
    pub fn is_task(&self) -> bool {
        self.kind == "task"
    }

    pub fn from_json(text: &str) -> Result<Manifest, String> {
        let m: Manifest =
            serde_json::from_str(text).map_err(|e| format!("not a plugin manifest: {e}"))?;
        m.validate()?;
        Ok(m)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.format != FORMAT {
            return Err(format!("\"format\" must be \"{FORMAT}\""));
        }
        let id_ok = !self.id.is_empty()
            && self.id.len() <= 64
            && self
                .id
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
            && !self.id.starts_with('-');
        if !id_ok {
            return Err(format!(
                "bad id {:?}: use lower case letters, digits and dashes (e.g. \"auto-tone\")",
                self.id
            ));
        }
        if self.name.trim().is_empty() || self.name.len() > 80 {
            return Err("the name must be 1 to 80 characters".into());
        }
        if self.version.trim().is_empty() {
            return Err("give a version, e.g. \"1.0.0\"".into());
        }
        match (self.kind.as_str(), self.effect.as_str()) {
            ("action", "") => {
                if self.margin != 0 {
                    return Err("\"margin\" is for pixel effects".into());
                }
            }
            ("action", _) => return Err("\"effect\" is for effect plugins".into()),
            ("effect", "color" | "pixels") => {
                // An effect runs on every render, with no one to ask.
                if !self.permissions.is_empty() {
                    return Err("effects can't ask for permissions".into());
                }
                if self.margin > 256 || (self.effect == "color" && self.margin != 0) {
                    return Err("\"margin\" must be 0–256, and only for pixel effects".into());
                }
            }
            ("effect", other) => {
                return Err(format!(
                    "an effect needs \"effect\": \"color\" or \"pixels\", not {other:?}"
                ));
            }
            ("task", "") => {
                if !self.permissions.is_empty() || !self.params.is_empty() || self.margin != 0 {
                    return Err(
                        "a task has no permissions, parameters or margin: its tools have them"
                            .into(),
                    );
                }
                let task = self
                    .task
                    .as_ref()
                    .ok_or("a task needs \"task\": its label, layout and tools")?;
                let label = task.label.trim();
                if label.is_empty() || label.chars().count() > 24 {
                    return Err("a task's label must be 1 to 24 characters".into());
                }
                if !task.layout.is_object() {
                    return Err("a task's \"layout\" must be a layout object".into());
                }
                if task.tools.len() > MAX_TOOLS {
                    return Err(format!("at most {MAX_TOOLS} tools"));
                }
                for t in &task.tools {
                    let does = [&t.command, &t.effect, &t.plugin]
                        .iter()
                        .filter(|s| !s.is_empty())
                        .count();
                    if t.label.trim().is_empty() || does != 1 {
                        return Err(format!(
                            "tool {:?}: give it a label and one of \"command\", \"effect\" or \"plugin\"",
                            t.label
                        ));
                    }
                    if !t.recipe.is_empty() && t.effect.is_empty() {
                        return Err(format!("tool {:?}: \"recipe\" is for an effect", t.label));
                    }
                }
            }
            ("task", _) => return Err("\"effect\" is for effect plugins".into()),
            (other, _) => {
                return Err(format!(
                    "unknown kind {other:?} (\"action\", \"effect\" or \"task\")"
                ));
            }
        }
        if self.task.is_some() && !self.is_task() {
            return Err("\"task\" is for task plugins".into());
        }
        let wasm_ok = self.wasm.ends_with(".wasm")
            && !self.wasm.contains(['/', '\\'])
            && !self.wasm.starts_with('.');
        if !wasm_ok {
            return Err(format!(
                "\"wasm\" must be a .wasm file name next to plugin.json, not {:?}",
                self.wasm
            ));
        }
        if self.params.len() > MAX_PARAMS {
            return Err(format!("at most {MAX_PARAMS} parameters"));
        }
        let mut seen = std::collections::BTreeSet::new();
        for p in &self.params {
            let name_ok = !p.name.is_empty()
                && p.name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_');
            if !name_ok {
                return Err(format!(
                    "bad parameter name {:?} (letters, digits and _)",
                    p.name
                ));
            }
            if !seen.insert(&p.name) {
                return Err(format!("parameter {:?} is listed twice", p.name));
            }
            match &p.kind {
                ParamKind::Number {
                    min,
                    max,
                    default,
                    step,
                } => {
                    let ok = min.is_finite()
                        && max.is_finite()
                        && min < max
                        && (*min..=*max).contains(default)
                        && step.is_none_or(|s| s > 0.0 && s.is_finite());
                    if !ok {
                        return Err(format!(
                            "parameter {:?}: need min < max, the default between them, and a positive step",
                            p.name
                        ));
                    }
                }
                ParamKind::Choice { options, default } => {
                    if options.is_empty() || !options.contains(default) {
                        return Err(format!(
                            "parameter {:?}: the default must be one of the options",
                            p.name
                        ));
                    }
                }
                ParamKind::Bool { .. } => {}
            }
        }
        for r in &self.recipes {
            if r.name.trim().is_empty() {
                return Err("every recipe needs a name".into());
            }
            self.resolve_params(&r.params)
                .map_err(|e| format!("recipe {:?}: {e}", r.name))?;
        }
        Ok(())
    }

    /// Every parameter with a value: the given one (clamped, or checked
    /// against the options) or the default.
    pub fn resolve_params(&self, given: &Map<String, Value>) -> Result<Map<String, Value>, String> {
        if let Some(unknown) = given
            .keys()
            .find(|k| !self.params.iter().any(|p| &p.name == *k))
        {
            return Err(format!("{} has no parameter {unknown:?}", self.name));
        }
        let mut out = Map::new();
        for p in &self.params {
            let v = given.get(&p.name);
            let value = match &p.kind {
                ParamKind::Number {
                    min, max, default, ..
                } => {
                    let n = match v {
                        None => *default,
                        Some(v) => number(v).ok_or(format!("{} must be a number", p.name))?,
                    };
                    Value::from(n.clamp(*min, *max))
                }
                ParamKind::Choice { options, default } => match v {
                    None => Value::from(default.clone()),
                    Some(v) => {
                        let s = v.as_str().ok_or(format!(
                            "{} must be one of {}",
                            p.name,
                            options.join(", ")
                        ))?;
                        if !options.iter().any(|o| o == s) {
                            return Err(format!(
                                "{} must be one of {}",
                                p.name,
                                options.join(", ")
                            ));
                        }
                        Value::from(s)
                    }
                },
                ParamKind::Bool { default } => match v {
                    None => Value::from(*default),
                    Some(Value::Bool(b)) => Value::from(*b),
                    Some(Value::String(s)) if s == "true" || s == "false" => {
                        Value::from(s == "true")
                    }
                    Some(_) => return Err(format!("{} must be true or false", p.name)),
                },
            };
            out.insert(p.name.clone(), value);
        }
        Ok(out)
    }
}

/// A number, or a string holding one (from the command line).
fn number(v: &Value) -> Option<f64> {
    v.as_f64()
        .or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
        .filter(|n: &f64| n.is_finite())
}
