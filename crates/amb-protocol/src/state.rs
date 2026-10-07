//! Develop state: a sparse Camera Raw settings map with undo/redo history.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use amb_presets::registry::{Registry, TagInfo};
use amb_presets::{Preset, Value};
use serde::Serialize;

use crate::catalog::Catalog;
use crate::command::{Command, Query};
use crate::convert::{self, Clamp};
use crate::{Error, sidecar};

/// One setting's change. `None` means "not set" (the default applies).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Change {
    pub name: String,
    pub before: Option<Value>,
    pub after: Option<Value>,
}

/// One undoable step.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HistoryEntry {
    pub label: String,
    pub changes: Vec<Change>,
}

/// What a command did.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Outcome {
    /// The command's label (for undo/redo: the step that was reverted or re-applied).
    pub command: String,
    /// Settings that changed, sorted by name. Empty when nothing changed.
    pub diff: Vec<Change>,
    /// Numbers pulled back into their registry range.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub clamped: Vec<Clamp>,
    /// Other things worth telling the user (side effects, legacy tags, ...).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

/// Groups never copied from a preset file: they describe the preset, not an edit.
const PRESET_SKIPPED_GROUPS: &[&str] = &["metadata"];
/// Process-group tags copied from a preset (the rest describe the file's writer).
const PRESET_PROCESS_TAGS: &[&str] = &["ProcessVersion"];

#[derive(Debug, Clone, Default, PartialEq)]
pub struct DevelopState {
    settings: BTreeMap<String, Value>,
    undo: Vec<HistoryEntry>,
    redo: Vec<HistoryEntry>,
}

impl DevelopState {
    pub fn new() -> Self {
        Self::default()
    }

    /// A state holding `settings` (normalised to typed values), with no history.
    pub fn from_settings(settings: BTreeMap<String, Value>) -> Self {
        DevelopState {
            settings: settings
                .into_iter()
                .map(|(k, v)| {
                    let v = convert::normalise(&k, v);
                    (k, v)
                })
                .collect(),
            ..Self::default()
        }
    }

    pub fn settings(&self) -> &BTreeMap<String, Value> {
        &self.settings
    }

    pub fn get(&self, name: &str) -> Option<&Value> {
        self.settings.get(name)
    }

    pub fn undo_stack(&self) -> &[HistoryEntry] {
        &self.undo
    }

    pub fn redo_stack(&self) -> &[HistoryEntry] {
        &self.redo
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    // ---- persistence -------------------------------------------------------

    /// Parse an XMP packet (sidecar or preset) into a fresh state.
    pub fn from_xmp_str(text: &str) -> Result<Self, Error> {
        let preset = Preset::parse_xmp(text).map_err(|e| Error::Load(e.to_string()))?;
        Ok(Self::from_settings(preset.settings))
    }

    /// The settings as a standalone XMP packet.
    pub fn to_xmp(&self) -> String {
        amb_presets::xmp_writer::write_packet(&self.settings, amb_presets::is_signed_tag)
    }

    /// JSON form: `{"format": "ambrotype-develop-state", "version": 1,
    /// "settings": {...}, "history": {"undo": [...], "redo": [...]}}`.
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "format": "ambrotype-develop-state",
            "version": 1,
            "settings": self.settings,
            "history": { "undo": self.undo, "redo": self.redo },
        })
    }

    /// Read the JSON form. A bare settings object (as printed by
    /// `amb preset dump` under "settings") is accepted too.
    pub fn from_json_str(text: &str) -> Result<Self, Error> {
        let json: serde_json::Value =
            serde_json::from_str(text).map_err(|e| Error::Json(format!("invalid JSON: {e}")))?;
        let obj = json
            .as_object()
            .ok_or_else(|| Error::Json("state must be a JSON object".into()))?;
        let settings_json = match obj.get("settings") {
            Some(s) => s
                .as_object()
                .ok_or_else(|| Error::Json("\"settings\" must be an object".into()))?,
            None => obj,
        };
        let mut settings = BTreeMap::new();
        for (name, v) in settings_json {
            settings.insert(name.clone(), convert::value_from_saved(name, v)?);
        }
        let history = obj.get("history");
        let stack = |key: &str| -> Result<Vec<HistoryEntry>, Error> {
            let Some(items) = history.and_then(|h| h.get(key)) else {
                return Ok(Vec::new());
            };
            let items = items
                .as_array()
                .ok_or_else(|| Error::Json(format!("history.{key} must be an array")))?;
            items.iter().map(entry_from_json).collect()
        };
        Ok(DevelopState {
            settings,
            undo: stack("undo")?,
            redo: stack("redo")?,
        })
    }

    /// Load from a file: `.json` uses the JSON form, anything else is read as
    /// XMP (sidecars, presets, files with an embedded XMP packet, .lrtemplate).
    pub fn load(path: &Path) -> Result<Self, Error> {
        let at = |e: String| Error::Load(format!("{}: {e}", path.display()));
        if has_extension(path, "json") {
            let text = std::fs::read_to_string(path).map_err(|e| at(e.to_string()))?;
            return Self::from_json_str(&text).map_err(|e| at(e.to_string()));
        }
        let preset = Preset::from_path(path).map_err(|e| at(e.to_string()))?;
        Ok(Self::from_settings(preset.settings))
    }

    /// Save to `.json` (with history) or `.xmp` (settings only). When the XMP
    /// file already exists, its non-Camera-Raw metadata (rating, keywords, ...)
    /// is preserved. Writes atomically (temporary file, then rename).
    pub fn save(&self, path: &Path) -> Result<(), Error> {
        self.save_merged(path, None)
    }

    /// Like [`save`](Self::save), but for `.xmp` the non-Camera-Raw metadata
    /// comes from `base` (the XMP text of the sidecar the state was loaded
    /// from) instead of from whatever already exists at `path`. Use it when
    /// writing an edit to a different file, so the photo's own rating,
    /// keywords and history travel with it. `None` behaves like `save`.
    pub fn save_merged(&self, path: &Path, base: Option<&str>) -> Result<(), Error> {
        let text = if has_extension(path, "json") {
            let mut s = serde_json::to_string_pretty(&self.to_json())
                .map_err(|e| Error::Json(e.to_string()))?;
            s.push('\n');
            s
        } else if has_extension(path, "xmp") {
            match base {
                Some(base) => sidecar::merge(base, &self.settings)?,
                None => match std::fs::read_to_string(path) {
                    Ok(existing) => sidecar::merge(&existing, &self.settings)?,
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => self.to_xmp(),
                    Err(e) => return Err(Error::Io(format!("{}: {e}", path.display()))),
                },
            }
        } else {
            return Err(Error::Io(format!(
                "refusing to write {}: develop state is saved only as .xmp or .json",
                path.display()
            )));
        };
        sidecar::write_atomic(path, &text)
    }

    // ---- commands ----------------------------------------------------------

    /// Run one command. On error the state is unchanged.
    pub fn execute(&mut self, cmd: &Command) -> Result<Outcome, Error> {
        match cmd {
            Command::Undo => {
                let entry = self.undo.pop().ok_or(Error::NothingToUndo)?;
                for c in &entry.changes {
                    set_or_remove(&mut self.settings, &c.name, c.before.clone());
                }
                let diff = entry
                    .changes
                    .iter()
                    .map(|c| Change {
                        name: c.name.clone(),
                        before: c.after.clone(),
                        after: c.before.clone(),
                    })
                    .collect();
                let outcome = Outcome {
                    command: format!("undo {}", entry.label),
                    diff,
                    clamped: Vec::new(),
                    notes: Vec::new(),
                };
                self.redo.push(entry);
                Ok(outcome)
            }
            Command::Redo => {
                let entry = self.redo.pop().ok_or(Error::NothingToRedo)?;
                for c in &entry.changes {
                    set_or_remove(&mut self.settings, &c.name, c.after.clone());
                }
                let outcome = Outcome {
                    command: format!("redo {}", entry.label),
                    diff: entry.changes.clone(),
                    clamped: Vec::new(),
                    notes: Vec::new(),
                };
                self.undo.push(entry);
                Ok(outcome)
            }
            _ => {
                let mut next = self.settings.clone();
                let mut clamped = Vec::new();
                let mut notes = Vec::new();
                self.plan(cmd, &mut next, &mut clamped, &mut notes)?;
                let diff = diff_maps(&self.settings, &next);
                let label = cmd.label();
                if diff.is_empty() {
                    notes.push("no change".into());
                } else {
                    self.settings = next;
                    self.undo.push(HistoryEntry {
                        label: label.clone(),
                        changes: diff.clone(),
                    });
                    self.redo.clear();
                }
                Ok(Outcome {
                    command: label,
                    diff,
                    clamped,
                    notes,
                })
            }
        }
    }

    /// Run several commands as one transaction: if any fails, the state is
    /// left exactly as it was and the error names the failing command.
    pub fn execute_all(&mut self, cmds: &[Command]) -> Result<Vec<Outcome>, Error> {
        let saved = self.clone();
        let mut outcomes = Vec::with_capacity(cmds.len());
        for (i, cmd) in cmds.iter().enumerate() {
            match self.execute(cmd) {
                Ok(o) => outcomes.push(o),
                Err(e) => {
                    *self = saved;
                    return Err(Error::InCommand {
                        index: i,
                        label: cmd.label(),
                        source: Box::new(e),
                    });
                }
            }
        }
        Ok(outcomes)
    }

    /// Like [`execute_all`](Self::execute_all), but the commands become a
    /// single undo step labelled `label` (e.g. a colour-wheel drag that sets
    /// hue and saturation, or a white-balance pick). Undo/redo commands are
    /// not allowed inside a group.
    pub fn execute_grouped(
        &mut self,
        label: &str,
        cmds: &[Command],
    ) -> Result<Vec<Outcome>, Error> {
        if cmds
            .iter()
            .any(|c| matches!(c, Command::Undo | Command::Redo))
        {
            return Err(Error::InvalidValue {
                name: label.to_string(),
                reason: "undo/redo cannot be part of a grouped edit".into(),
            });
        }
        let depth = self.undo.len();
        let outcomes = self.execute_all(cmds)?;
        if self.undo.len() > depth + 1 {
            // Merge the new steps: first "before" and last "after" per setting.
            let steps: Vec<HistoryEntry> = self.undo.drain(depth..).collect();
            let mut changes: Vec<Change> = Vec::new();
            for step in steps {
                for c in step.changes {
                    match changes.iter_mut().find(|e| e.name == c.name) {
                        Some(existing) => existing.after = c.after,
                        None => changes.push(c),
                    }
                }
            }
            changes.retain(|c| c.before != c.after);
            changes.sort_by(|a, b| a.name.cmp(&b.name));
            if !changes.is_empty() {
                self.undo.push(HistoryEntry {
                    label: label.to_string(),
                    changes,
                });
            }
        } else if let Some(last) = self.undo.get_mut(depth) {
            last.label = label.to_string();
        }
        Ok(outcomes)
    }

    fn plan(
        &self,
        cmd: &Command,
        next: &mut BTreeMap<String, Value>,
        clamped: &mut Vec<Clamp>,
        notes: &mut Vec<String>,
    ) -> Result<(), Error> {
        match cmd {
            Command::SetParam { name, value } => {
                let info = known(name)?;
                let v = convert::value_from_command(&info, value, clamped)?;
                next.insert(name.clone(), v);
                after_set(name, next, notes)?;
            }
            Command::AdjustParam { name, delta } => {
                let info = known(name)?;
                if info.kind != "number" {
                    return Err(Error::WrongKind {
                        name: name.clone(),
                        expected: "a numeric setting for adjust_param".into(),
                        got: format!("a {} setting", info.kind),
                    });
                }
                if !delta.is_finite() {
                    return Err(Error::InvalidValue {
                        name: name.clone(),
                        reason: "delta must be a finite number".into(),
                    });
                }
                // A non-numeric or non-finite stored value ("NaN") counts as unset.
                let base = match self.settings.get(name).and_then(convert::finite) {
                    Some(v) => v,
                    None => Registry::builtin()
                        .numeric_default(name)
                        .ok_or_else(|| Error::NoBase { name: name.clone() })?,
                };
                let sum = base + delta;
                if !sum.is_finite() {
                    return Err(Error::InvalidValue {
                        name: name.clone(),
                        reason: format!("{base} + {delta} is not a finite number"),
                    });
                }
                let v = convert::clamp_number(&info, sum, clamped);
                next.insert(name.clone(), Value::Number(v));
                after_set(name, next, notes)?;
            }
            Command::ResetParam { name } => {
                if !self.settings.contains_key(name) {
                    known(name)?;
                }
                next.remove(name);
            }
            Command::ResetGroup { group } => {
                known_group(group)?;
                next.retain(|name, _| group_of(name).as_deref() != Some(group.as_str()));
            }
            Command::ApplyPreset {
                path,
                amount,
                include_groups,
                exclude_groups,
            } => {
                if !amount.is_finite() || !(0.0..=200.0).contains(amount) {
                    return Err(Error::InvalidValue {
                        name: "amount".into(),
                        reason: format!("{amount} is outside 0-200 (percent)"),
                    });
                }
                for g in include_groups.iter().chain(exclude_groups).flatten() {
                    known_group(g)?;
                }
                let preset = Preset::from_path(Path::new(path))
                    .map_err(|e| Error::Load(format!("{path}: {e}")))?;
                if *amount != 100.0 && !preset.supports_amount() {
                    notes.push(format!(
                        "preset does not declare SupportsAmount; applied at {amount}% anyway"
                    ));
                }
                let mut filtered = BTreeMap::new();
                let mut unknown = Vec::new();
                let mut rejected = Vec::new();
                for (name, value) in &preset.settings {
                    let group = group_of(name);
                    let keep = match group.as_deref() {
                        Some(g) if PRESET_SKIPPED_GROUPS.contains(&g) => false,
                        Some("process") => PRESET_PROCESS_TAGS.contains(&name.as_str()),
                        _ => true,
                    };
                    let included = match (include_groups, group.as_deref()) {
                        (Some(inc), Some(g)) => inc.iter().any(|i| i == g),
                        (Some(_), None) => false,
                        (None, _) => true,
                    };
                    let excluded = exclude_groups.as_ref().is_some_and(|ex| {
                        group.as_deref().is_some_and(|g| ex.iter().any(|e| e == g))
                    });
                    if !(keep && included && !excluded) {
                        continue;
                    }
                    let value = convert::normalise(name, value.clone());
                    match Registry::builtin().lookup(name) {
                        None => {
                            unknown.push(name.clone());
                            filtered.insert(name.clone(), value);
                        }
                        // Same kind checks and range clamping as set_param.
                        Some(info) => match preset_value(&info, value, clamped) {
                            Ok(v) => {
                                filtered.insert(name.clone(), v);
                            }
                            Err(e) => rejected.push(e.to_string()),
                        },
                    }
                }
                if !unknown.is_empty() {
                    notes.push(format!(
                        "copied {} setting(s) unknown to the registry without validation: {}",
                        unknown.len(),
                        unknown.join(", ")
                    ));
                }
                let unscaled =
                    apply_amount(next, &filtered, amount / 100.0, clamped, &mut rejected);
                if !rejected.is_empty() {
                    notes.push(format!(
                        "skipped {} invalid preset value(s): {}",
                        rejected.len(),
                        rejected.join("; ")
                    ));
                }
                if !unscaled.is_empty() {
                    notes.push(format!(
                        "copied {} unscaled at {amount}%: no current value or fixed default to \
                         scale from (white balance depends on the image)",
                        unscaled.join(", ")
                    ));
                }
            }
            Command::Undo | Command::Redo => unreachable!("handled by execute"),
        }
        Ok(())
    }

    // ---- queries -----------------------------------------------------------

    /// Answer a query as JSON.
    pub fn query(&self, q: &Query) -> Result<serde_json::Value, Error> {
        let catalog = Catalog::builtin();
        Ok(match q {
            Query::GetParam { name } => {
                let info = self.info_for(name)?;
                self.param_json(&info, false)
            }
            Query::Describe { name } => {
                let info = self.info_for(name)?;
                self.param_json(&info, true)
            }
            Query::ListParams { group, only_set } => {
                if let Some(g) = group {
                    known_group(g)?;
                }
                let only_set = only_set.unwrap_or(group.is_none());
                let mut names: Vec<String> = match group {
                    Some(g) => catalog.group(g).map(|g| g.tags.clone()).unwrap_or_default(),
                    None => catalog.tags().iter().map(|t| t.name.clone()).collect(),
                };
                // Settings present in the state but not listed in the registry
                // (prefix-rule tags, unknown tags copied from presets).
                let listed: BTreeSet<String> = names.iter().cloned().collect();
                for name in self.settings.keys() {
                    if !listed.contains(name)
                        && group
                            .as_deref()
                            .is_none_or(|g| group_of(name).as_deref() == Some(g))
                    {
                        names.push(name.clone());
                    }
                }
                let params: Vec<serde_json::Value> = names
                    .iter()
                    .filter(|n| !only_set || self.settings.contains_key(*n))
                    .map(|n| match catalog.lookup(n) {
                        Some(info) => self.param_json(&info, false),
                        None => serde_json::json!({
                            "name": n, "group": null, "value": self.settings.get(n),
                        }),
                    })
                    .collect();
                serde_json::json!({ "group": group, "only_set": only_set, "params": params })
            }
            Query::ListGroups => {
                let groups: Vec<serde_json::Value> = catalog
                    .groups()
                    .iter()
                    .map(|g| {
                        let set = self
                            .settings
                            .keys()
                            .filter(|n| group_of(n).as_deref() == Some(g.name.as_str()))
                            .count();
                        serde_json::json!({
                            "name": g.name, "label": g.label, "description": g.description,
                            "tags": g.tags.len(), "set": set,
                        })
                    })
                    .collect();
                serde_json::json!({ "groups": groups })
            }
        })
    }

    fn info_for(&self, name: &str) -> Result<TagInfo, Error> {
        known(name)
    }

    fn param_json(&self, info: &TagInfo, full: bool) -> serde_json::Value {
        let catalog = Catalog::builtin();
        let value = self.settings.get(&info.name);
        let default = info
            .default
            .clone()
            .filter(|d| !d.is_null())
            .unwrap_or(serde_json::Value::Null);
        let effective = match value {
            Some(v) => serde_json::to_value(v).unwrap_or_default(),
            None => default.clone(),
        };
        let text = catalog.text(&info.name);
        let mut out = serde_json::json!({
            "name": info.name,
            "label": text.map(|t| t.label.clone()),
            "group": info.group,
            "kind": info.kind,
            "min": info.min,
            "max": info.max,
            "default": default,
            "value": value,
            "effective": effective,
        });
        if full {
            let obj = out.as_object_mut().expect("object");
            obj.insert(
                "status".into(),
                serde_json::to_value(info.status).unwrap_or_default(),
            );
            obj.insert(
                "units".into(),
                serde_json::json!(text.and_then(|t| t.units.clone())),
            );
            obj.insert(
                "description".into(),
                serde_json::json!(text.map(|t| t.description.clone())),
            );
            obj.insert(
                "group_label".into(),
                serde_json::json!(catalog.group(&info.group).map(|g| g.label.clone())),
            );
        }
        out
    }
}

/// Registry info for a settable name, or an error with suggestions.
fn known(name: &str) -> Result<TagInfo, Error> {
    Catalog::builtin()
        .lookup(name)
        .ok_or_else(|| Error::UnknownTag {
            name: name.to_string(),
            suggestions: Catalog::builtin().suggest(name),
        })
}

fn known_group(group: &str) -> Result<(), Error> {
    let c = Catalog::builtin();
    if c.group(group).is_some() {
        return Ok(());
    }
    Err(Error::UnknownGroup {
        name: group.to_string(),
        suggestions: c.suggest_group(group),
        known: c.groups().iter().map(|g| g.name.clone()).collect(),
    })
}

fn group_of(name: &str) -> Option<String> {
    Registry::builtin().lookup(name).map(|i| i.group)
}

fn has_extension(path: &Path, ext: &str) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case(ext))
}

fn set_or_remove(map: &mut BTreeMap<String, Value>, name: &str, value: Option<Value>) {
    match value {
        Some(v) => {
            map.insert(name.to_string(), v);
        }
        None => {
            map.remove(name);
        }
    }
}

/// Validate a (normalised) preset value for a registry tag the way set_param
/// validates a command value: scalars (numbers, switches, enums) go through
/// [`convert::value_from_command`], so wrong kinds and out-of-set enum values
/// are refused and numbers are clamped to the range (and reported). Curves,
/// sequences, structs and text are taken as they are, as before.
fn preset_value(info: &TagInfo, value: Value, clamped: &mut Vec<Clamp>) -> Result<Value, Error> {
    if !matches!(info.kind.as_str(), "number" | "bool" | "enum") {
        return Ok(value);
    }
    // A non-finite number serialises as null and is refused like any wrong kind.
    let json = serde_json::to_value(&value).unwrap_or_default();
    convert::value_from_command(info, &json, clamped)
}

/// Apply validated preset values on top of `next` at `fraction` (1.0 = 100%).
///
/// Numeric settings move linearly from the current value (or the registry
/// default) towards the preset's value, may extrapolate past it above 100%,
/// and are clamped to their range with the clamping reported. Numeric settings
/// with neither a current value nor a fixed default (Temperature, Tint) are
/// copied unscaled; their names are returned. Everything else is copied from
/// the preset whenever `fraction > 0`.
fn apply_amount(
    next: &mut BTreeMap<String, Value>,
    preset: &BTreeMap<String, Value>,
    fraction: f64,
    clamped: &mut Vec<Clamp>,
    rejected: &mut Vec<String>,
) -> Vec<String> {
    let mut unscaled = Vec::new();
    if fraction <= 0.0 {
        return unscaled;
    }
    let registry = Registry::builtin();
    for (name, target) in preset {
        let info = registry.lookup(name).filter(|i| i.kind == "number");
        let (Some(info), Some(to)) = (info, convert::finite(target)) else {
            next.insert(name.clone(), target.clone());
            continue;
        };
        let from = next
            .get(name)
            .and_then(convert::finite)
            .or_else(|| registry.numeric_default(name));
        let Some(from) = from else {
            if fraction != 1.0 {
                unscaled.push(name.clone());
            }
            next.insert(name.clone(), target.clone());
            continue;
        };
        let raw = from + (to - from) * fraction;
        if !raw.is_finite() {
            rejected.push(format!("{name}: scaling to {fraction}x overflows"));
            continue;
        }
        let mut v = convert::clamp_number(&info, raw, clamped);
        // Interpolation leaves float noise (1.0 + (0.2 - 1.0) * 1.0 != 0.2);
        // snap to the preset's own value when that is meant.
        if (v - to).abs() < 1e-9 {
            v = to;
        }
        next.insert(name.clone(), Value::Number(v));
    }
    unscaled
}

/// Side effects Lightroom applies when a slider moves, plus guidance notes.
///
/// Moving Temperature or Tint under As Shot/Auto switches WhiteBalance to
/// Custom. Lightroom keeps the as-shot value of the other slider, but that
/// value lives in the raw file, not in the settings, so this layer cannot fill
/// it in: a Custom white balance without Temperature is not rendered at all,
/// and one without Tint renders with tint 0. Tint alone is therefore refused,
/// and Temperature alone carries a warning.
fn after_set(
    name: &str,
    next: &mut BTreeMap<String, Value>,
    notes: &mut Vec<String>,
) -> Result<(), Error> {
    if matches!(name, "Temperature" | "Tint") {
        let mode = next.get("WhiteBalance").and_then(Value::as_str);
        if matches!(mode, None | Some("As Shot") | Some("Auto")) {
            let has = |n: &str| next.get(n).and_then(convert::finite).is_some();
            let (has_temperature, has_tint) = (has("Temperature"), has("Tint"));
            if !has_temperature {
                return Err(Error::InvalidValue {
                    name: name.to_string(),
                    reason: format!(
                        "white balance is {}, so Tint needs a Temperature to switch to Custom; \
                         set Temperature first (in the same batch, before Tint), e.g. the as-shot \
                         temperature that `amb render` prints",
                        mode.unwrap_or("As Shot")
                    ),
                });
            }
            next.insert("WhiteBalance".into(), Value::Text("Custom".into()));
            notes.push(format!(
                "WhiteBalance set to \"Custom\" so that {name} takes effect (as Lightroom does when the slider moves)"
            ));
            if !has_tint {
                notes.push(
                    "Tint is not set, so the custom white balance uses tint 0 instead of the \
                     as-shot tint; set Tint too (`amb render` prints the as-shot values, e.g. \
                     `as shot: 5150 K, tint +4`)"
                        .into(),
                );
            }
        }
    }
    if group_of(name).as_deref() == Some("tone_legacy") {
        let modern = match name {
            "Exposure" => "Exposure2012",
            "Contrast" => "Contrast2012",
            "Shadows" => "Blacks2012 (legacy Shadows is the old Blacks slider)",
            "HighlightRecovery" => "Highlights2012",
            "FillLight" => "Shadows2012",
            "Clarity" => "Clarity2012",
            _ => "Exposure2012 and the tone curve",
        };
        notes.push(format!(
            "{name} is a legacy PV2010/PV2003 control; current process versions use {modern}"
        ));
    }
    Ok(())
}

fn diff_maps(before: &BTreeMap<String, Value>, after: &BTreeMap<String, Value>) -> Vec<Change> {
    let names: BTreeSet<&String> = before.keys().chain(after.keys()).collect();
    names
        .into_iter()
        .filter(|n| !convert::same(before.get(*n), after.get(*n)))
        .map(|n| Change {
            name: n.clone(),
            before: before.get(n).cloned(),
            after: after.get(n).cloned(),
        })
        .collect()
}

fn entry_from_json(json: &serde_json::Value) -> Result<HistoryEntry, Error> {
    let bad = || Error::Json("history entries need \"label\" and \"changes\"".into());
    let label = json.get("label").and_then(|l| l.as_str()).ok_or_else(bad)?;
    let changes = json
        .get("changes")
        .and_then(|c| c.as_array())
        .ok_or_else(bad)?;
    let side = |c: &serde_json::Value, key: &str, name: &str| -> Result<Option<Value>, Error> {
        match c.get(key) {
            None | Some(serde_json::Value::Null) => Ok(None),
            Some(v) => convert::value_from_saved(name, v).map(Some),
        }
    };
    let changes = changes
        .iter()
        .map(|c| {
            let name = c.get("name").and_then(|n| n.as_str()).ok_or_else(bad)?;
            Ok(Change {
                name: name.to_string(),
                before: side(c, "before", name)?,
                after: side(c, "after", name)?,
            })
        })
        .collect::<Result<_, Error>>()?;
    Ok(HistoryEntry {
        label: label.to_string(),
        changes,
    })
}
