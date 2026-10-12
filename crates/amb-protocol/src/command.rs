// SPDX-License-Identifier: GPL-3.0-or-later
//! Wire types: commands change the develop state, queries read it.
//!
//! Both are JSON objects tagged by a `cmd` / `query` field, e.g.
//! `{"cmd": "set_param", "name": "Exposure2012", "value": 0.5}`.

use serde::{Deserialize, Serialize};

fn default_amount() -> f64 {
    100.0
}

fn is_default_amount(a: &f64) -> bool {
    *a == 100.0
}

fn default_brush_size() -> f64 {
    0.05
}

fn default_feather() -> f64 {
    50.0
}

fn is_false(b: &bool) -> bool {
    !*b
}

fn default_spot_radius() -> f64 {
    0.02
}

/// A state-changing, undoable edit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    /// Set a setting to an absolute value (numbers are clamped to the range).
    SetParam {
        name: String,
        value: serde_json::Value,
    },
    /// Add `delta` to a numeric setting (from its current value, or its default).
    AdjustParam { name: String, delta: f64 },
    /// Remove a setting so it falls back to its default.
    ResetParam { name: String },
    /// Remove every setting of a registry group (e.g. `"hsl"`).
    ResetGroup { group: String },
    /// Apply a preset file at `amount` percent (0-200, default 100),
    /// optionally limited to or excluding some registry groups.
    ApplyPreset {
        path: String,
        #[serde(default = "default_amount", skip_serializing_if = "is_default_amount")]
        amount: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        include_groups: Option<Vec<String>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        exclude_groups: Option<Vec<String>>,
    },
    /// Add a mask of `kind` (linear, radial, luminance, color) with optional
    /// part `values`; with `mask`, add it as a new part of that mask, joined
    /// by `mode` (add, subtract, intersect).
    AddMask {
        kind: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mask: Option<usize>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mode: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        values: Option<serde_json::Map<String, serde_json::Value>>,
    },
    /// Change a mask's local sliders (UI units), `name`, `active` or `amount`;
    /// or, with `part`, that part's geometry, `mode`, `invert`, `active`.
    SetMask {
        mask: usize,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        part: Option<usize>,
        values: serde_json::Map<String, serde_json::Value>,
    },
    /// Paint (or erase) a brush stroke through `points` (image fractions).
    /// Without `part` a new brush part is made, in a new mask without `mask`.
    PaintMask {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mask: Option<usize>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        part: Option<usize>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mode: Option<String>,
        points: Vec<[f64; 2]>,
        /// Radius as a fraction of the image's width.
        #[serde(default = "default_brush_size")]
        size: f64,
        #[serde(default = "default_feather")]
        feather: f64,
        #[serde(default = "default_amount")]
        flow: f64,
        #[serde(default = "default_amount")]
        density: f64,
        #[serde(default, skip_serializing_if = "is_false")]
        erase: bool,
    },
    /// Add a round spot at `at` (image fractions): `kind` heal (default) or
    /// clone, `radius` a fraction of the width, `source` the donor centre
    /// (searched for when absent), `feather` and `opacity` 0-100.
    AddSpot {
        at: [f64; 2],
        #[serde(default = "default_spot_radius")]
        radius: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        kind: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source: Option<[f64; 2]>,
        /// The source was picked by a search (Lightroom's "auto").
        #[serde(default, skip_serializing_if = "is_false")]
        source_auto: bool,
        #[serde(default = "default_feather")]
        feather: f64,
        #[serde(default = "default_amount")]
        opacity: f64,
    },
    /// Change a spot: `at`, `radius`, `source` (null to search), `kind`,
    /// `feather`, `opacity`, `active`.
    SetSpot {
        spot: usize,
        values: serde_json::Map<String, serde_json::Value>,
    },
    /// Remove a spot.
    RemoveSpot { spot: usize },
    /// Remove a mask, or one of its parts.
    RemoveMask {
        mask: usize,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        part: Option<usize>,
    },
    /// Copy a mask, placing the copy just after it.
    DuplicateMask { mask: usize },
    /// Move a mask to position `to` in the list (0-based).
    MoveMask { mask: usize, to: usize },
    /// Keep the current settings as a snapshot called `name` (replacing a
    /// snapshot of that name). Snapshots are kept with the photo; saving,
    /// renaming or deleting one doesn't change the edit, so it adds no
    /// history step.
    SaveSnapshot { name: String },
    /// Set every setting to the snapshot's (one undoable step).
    ApplySnapshot { name: String },
    /// Rename a snapshot.
    RenameSnapshot { name: String, to: String },
    /// Delete a snapshot.
    DeleteSnapshot { name: String },
    /// Revert the most recent command.
    Undo,
    /// Re-apply the most recently undone command.
    Redo,
}

impl Command {
    /// Short label used in history and diffs, e.g. `set_param Exposure2012`.
    pub fn label(&self) -> String {
        match self {
            Command::SetParam { name, .. } => format!("set_param {name}"),
            Command::AdjustParam { name, delta } => format!("adjust_param {name} {delta:+}"),
            Command::ResetParam { name } => format!("reset_param {name}"),
            Command::ResetGroup { group } => format!("reset_group {group}"),
            Command::ApplyPreset { path, amount, .. } => {
                format!("apply_preset {path} at {amount}%")
            }
            Command::AddMask { kind, mask, .. } => match mask {
                Some(m) => format!("add_mask {kind} to mask {m}"),
                None => format!("add_mask {kind}"),
            },
            Command::SetMask { mask, part, .. } => match part {
                Some(p) => format!("set_mask {mask} part {p}"),
                None => format!("set_mask {mask}"),
            },
            Command::PaintMask { mask, erase, .. } => {
                let verb = if *erase { "erase_mask" } else { "paint_mask" };
                match mask {
                    Some(m) => format!("{verb} {m}"),
                    None => format!("{verb} new"),
                }
            }
            Command::RemoveMask { mask, part } => match part {
                Some(p) => format!("remove_mask {mask} part {p}"),
                None => format!("remove_mask {mask}"),
            },
            Command::DuplicateMask { mask } => format!("duplicate_mask {mask}"),
            Command::MoveMask { mask, to } => format!("move_mask {mask} to {to}"),
            Command::AddSpot { kind, .. } => {
                format!("add_spot {}", kind.as_deref().unwrap_or("heal"))
            }
            Command::SetSpot { spot, .. } => format!("set_spot {spot}"),
            Command::RemoveSpot { spot } => format!("remove_spot {spot}"),
            Command::SaveSnapshot { name } => format!("save_snapshot {name}"),
            Command::ApplySnapshot { name } => format!("apply_snapshot {name}"),
            Command::RenameSnapshot { name, .. } => format!("rename_snapshot {name}"),
            Command::DeleteSnapshot { name } => format!("delete_snapshot {name}"),
            Command::Undo => "undo".into(),
            Command::Redo => "redo".into(),
        }
    }
}

/// A read-only request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "query", rename_all = "snake_case", deny_unknown_fields)]
pub enum Query {
    /// Current value, default and range of one setting.
    GetParam { name: String },
    /// Settings with their current values. Without `group`: only the settings
    /// present in the state. With `group`: every setting of that group.
    /// `only_set` overrides either default.
    ListParams {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        group: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        only_set: Option<bool>,
    },
    /// Full documentation of one setting, plus its current value.
    Describe { name: String },
    /// All registry groups with labels, descriptions and counts.
    ListGroups,
    /// Every mask with its sliders (UI units) and parts.
    ListMasks,
    /// Every spot (heal and clone) with its position, size and source.
    ListSpots,
    /// The snapshots: name, how many settings each has, and whether it
    /// matches the current settings.
    ListSnapshots,
}

/// Parse one command or a JSON array of commands.
pub fn parse_commands(text: &str) -> Result<Vec<Command>, crate::Error> {
    parse_list(text, "command")
}

/// Parse one query or a JSON array of queries.
pub fn parse_queries(text: &str) -> Result<Vec<Query>, crate::Error> {
    parse_list(text, "query")
}

fn parse_list<T: serde::de::DeserializeOwned>(
    text: &str,
    what: &str,
) -> Result<Vec<T>, crate::Error> {
    let json: serde_json::Value =
        serde_json::from_str(text).map_err(|e| crate::Error::Json(format!("invalid JSON: {e}")))?;
    let items = match json {
        serde_json::Value::Array(items) => items,
        single => vec![single],
    };
    items
        .into_iter()
        .enumerate()
        .map(|(i, item)| {
            serde_json::from_value(item).map_err(|e| crate::Error::Json(format!("{what} {i}: {e}")))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn wire_format() {
        let cmds = parse_commands(
            r#"[{"cmd":"set_param","name":"Exposure2012","value":0.5},
                {"cmd":"apply_preset","path":"p.xmp","exclude_groups":["tone"]},
                {"cmd":"undo"}]"#,
        )
        .unwrap();
        assert_eq!(
            cmds[1],
            Command::ApplyPreset {
                path: "p.xmp".into(),
                amount: 100.0,
                include_groups: None,
                exclude_groups: Some(vec!["tone".into()]),
            }
        );
        assert_eq!(
            serde_json::to_value(&cmds[2]).unwrap(),
            json!({"cmd": "undo"})
        );
        let single = parse_commands(r#"{"cmd":"reset_group","group":"hsl"}"#).unwrap();
        assert_eq!(single.len(), 1);
        let err = parse_commands(r#"[{"cmd":"set_param","name":"X"}]"#).unwrap_err();
        assert!(err.to_string().contains("command 0"), "{err}");
        assert!(parse_commands(r#"{"cmd":"set_param","name":"X","value":1,"typo":2}"#).is_err());
        let q = parse_queries(r#"{"query":"list_params","group":"hsl"}"#).unwrap();
        assert_eq!(
            q[0],
            Query::ListParams {
                group: Some("hsl".into()),
                only_set: None
            }
        );
    }
}
