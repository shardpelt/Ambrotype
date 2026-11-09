// SPDX-License-Identifier: GPL-3.0-or-later
//! Snapshots: named copies of the settings, kept in the sidecar.

use super::*;

impl DevelopState {
    /// The snapshots, in the order they were made.
    pub fn snapshots(&self) -> &[Snapshot] {
        &self.snapshots
    }

    pub(super) fn snapshot(&self, name: &str) -> Result<&Snapshot, Error> {
        self.snapshots
            .iter()
            .find(|s| s.name == name)
            .ok_or_else(|| Error::InvalidValue {
                name: "snapshot".into(),
                reason: format!("no snapshot named {name:?}"),
            })
    }

    /// Save, rename or delete a snapshot: not an edit, so no history step.
    pub(super) fn manage_snapshot(&mut self, cmd: &Command) -> Result<String, Error> {
        let clean = |name: &str| -> Result<String, Error> {
            let name = name.trim();
            if name.is_empty() {
                return Err(Error::InvalidValue {
                    name: "snapshot".into(),
                    reason: "a snapshot needs a name".into(),
                });
            }
            Ok(name.to_string())
        };
        Ok(match cmd {
            Command::SaveSnapshot { name } => {
                let name = clean(name)?;
                let settings = self.settings.clone();
                match self.snapshots.iter_mut().find(|s| s.name == name) {
                    Some(s) => {
                        s.settings = settings;
                        format!("updated snapshot {name:?}")
                    }
                    None => {
                        self.snapshots.push(Snapshot {
                            name: name.clone(),
                            settings,
                        });
                        format!("saved snapshot {name:?}")
                    }
                }
            }
            Command::RenameSnapshot { name, to } => {
                let to = clean(to)?;
                self.snapshot(name)?;
                if to != *name && self.snapshots.iter().any(|s| s.name == to) {
                    return Err(Error::InvalidValue {
                        name: "snapshot".into(),
                        reason: format!("there is already a snapshot named {to:?}"),
                    });
                }
                let s = self.snapshots.iter_mut().find(|s| s.name == *name);
                s.expect("checked above").name = to.clone();
                format!("renamed snapshot {name:?} to {to:?}")
            }
            Command::DeleteSnapshot { name } => {
                self.snapshot(name)?;
                self.snapshots.retain(|s| s.name != *name);
                format!("deleted snapshot {name:?}")
            }
            _ => unreachable!("not a snapshot command"),
        })
    }

    /// The snapshots as stored in a sidecar (JSON text), `None` when there are none.
    pub(super) fn snapshots_for_sidecar(&self) -> Option<String> {
        (!self.snapshots.is_empty())
            .then(|| serde_json::json!({ "version": 1, "snapshots": self.snapshots }).to_string())
    }

    /// Read stored snapshots (a bad one is skipped).
    pub(super) fn restore_snapshots(&mut self, json: &serde_json::Value) {
        let Some(items) = json.get("snapshots").and_then(|v| v.as_array()) else {
            return;
        };
        for item in items {
            let name = item.get("name").and_then(|n| n.as_str());
            let settings = item.get("settings").and_then(|s| s.as_object());
            let (Some(name), Some(settings)) = (name, settings) else {
                continue;
            };
            let settings: Option<BTreeMap<String, Value>> = settings
                .iter()
                .map(|(k, v)| convert::value_from_saved(k, v).ok().map(|v| (k.clone(), v)))
                .collect();
            if let Some(settings) = settings {
                if !self.snapshots.iter().any(|s| s.name == name) {
                    self.snapshots.push(Snapshot {
                        name: name.to_string(),
                        settings,
                    });
                }
            }
        }
    }
}
