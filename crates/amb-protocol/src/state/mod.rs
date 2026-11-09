// SPDX-License-Identifier: GPL-3.0-or-later
//! Develop state: a sparse Camera Raw settings map with undo/redo history.

mod files;
mod plan;
mod query;
mod snapshots;

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use amb_presets::registry::{Registry, TagInfo};
use amb_presets::{Preset, Value};
use serde::Serialize;

use crate::catalog::Catalog;
use crate::command::{Command, Query};
use crate::convert::{self, Clamp};
use crate::{Error, masks, sidecar, spots};

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

/// A named copy of the settings, kept with the photo (Lightroom's
/// Snapshots panel).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Snapshot {
    pub name: String,
    pub settings: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct DevelopState {
    settings: BTreeMap<String, Value>,
    undo: Vec<HistoryEntry>,
    redo: Vec<HistoryEntry>,
    snapshots: Vec<Snapshot>,
    /// The history read from the sidecar left out older steps (to keep the
    /// sidecar small): its first step isn't the photo's original state.
    trimmed: bool,
}

/// Most of a sidecar the stored history may take; older steps are left out
/// past it. Brush strokes store the whole mask before and after, so a long
/// painting session would otherwise grow without bound.
const HISTORY_BUDGET: usize = 512 * 1024;

/// A fingerprint of settings (FNV-1a over their JSON; stable across runs).
fn fingerprint(settings: &BTreeMap<String, Value>) -> String {
    let json = serde_json::to_string(settings).unwrap_or_default();
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in json.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}")
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

    /// The history doesn't reach back to the photo's original state (older
    /// steps were left out of the sidecar).
    pub fn history_trimmed(&self) -> bool {
        self.trimmed
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
            Command::SaveSnapshot { .. }
            | Command::RenameSnapshot { .. }
            | Command::DeleteSnapshot { .. } => {
                let note = self.manage_snapshot(cmd)?;
                Ok(Outcome {
                    command: cmd.label(),
                    diff: Vec::new(),
                    clamped: Vec::new(),
                    notes: vec![note],
                })
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

    /// Take `settings` as they are, as one undo step labelled `label`: an
    /// edit made elsewhere (another app, or an AI agent, writing the
    /// sidecar). Returns the changes (none when nothing differs).
    pub fn adopt(&mut self, settings: BTreeMap<String, Value>, label: &str) -> Vec<Change> {
        let diff = diff_maps(&self.settings, &settings);
        if !diff.is_empty() {
            self.settings = settings;
            self.undo.push(HistoryEntry {
                label: label.to_string(),
                changes: diff.clone(),
            });
            self.redo.clear();
        }
        diff
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
}

/// Registry info for a settable name, or an error with suggestions.
pub(super) fn known(name: &str) -> Result<TagInfo, Error> {
    Catalog::builtin()
        .lookup(name)
        .ok_or_else(|| Error::UnknownTag {
            name: name.to_string(),
            suggestions: Catalog::builtin().suggest(name),
        })
}

pub(super) fn known_group(group: &str) -> Result<(), Error> {
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

pub(super) fn group_of(name: &str) -> Option<String> {
    Registry::builtin().lookup(name).map(|i| i.group)
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
