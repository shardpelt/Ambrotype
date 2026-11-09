// SPDX-License-Identifier: GPL-3.0-or-later
//! Reading and writing a state: XMP, JSON, the history in a sidecar.

use super::*;

impl DevelopState {
    // ---- persistence -------------------------------------------------------

    /// Parse an XMP packet (sidecar or preset) into a fresh state, with the
    /// edit history stored in it when that history ends at these settings
    /// (if another app changed them since, it no longer fits and is dropped).
    pub fn from_xmp_str(text: &str) -> Result<Self, Error> {
        let preset = Preset::parse_xmp(text).map_err(|e| Error::Load(e.to_string()))?;
        let mut state = Self::from_settings(preset.settings);
        if let Some(history) = sidecar::read_history(text) {
            state.restore_history(&history);
        }
        if let Some(snapshots) = sidecar::read_snapshots(text) {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&snapshots) {
                state.restore_snapshots(&json);
            }
        }
        Ok(state)
    }

    /// Take the stored history if it is ours and ends at the current settings.
    fn restore_history(&mut self, text: &str) {
        let Ok(json) = serde_json::from_str::<serde_json::Value>(text) else {
            return;
        };
        let fits = json.get("version").and_then(|v| v.as_u64()) == Some(1)
            && json.get("settings").and_then(|v| v.as_str()) == Some(&fingerprint(&self.settings));
        if !fits {
            return;
        }
        let stack = |key: &str| -> Option<Vec<HistoryEntry>> {
            json.get(key)?
                .as_array()?
                .iter()
                .map(|e| entry_from_json(e).ok())
                .collect()
        };
        if let (Some(undo), Some(redo)) = (stack("undo"), stack("redo")) {
            self.undo = undo;
            self.redo = redo;
            self.trimmed = json.get("trimmed").and_then(|v| v.as_bool()) == Some(true);
        }
    }

    /// The history as stored in a sidecar (JSON text), `None` when there is
    /// none. It carries a fingerprint of the settings as they will read back
    /// from the sidecar, and keeps the newest steps within [`HISTORY_BUDGET`].
    fn history_for_sidecar(&self) -> Option<String> {
        if self.undo.is_empty() && self.redo.is_empty() {
            return None;
        }
        let written = Self::from_settings(
            Preset::parse_xmp(&self.to_xmp())
                .map(|p| p.settings)
                .unwrap_or_default(),
        );
        let mut budget = HISTORY_BUDGET;
        let mut keep = |entries: &[HistoryEntry]| -> Vec<serde_json::Value> {
            // Newest (last) first, then back in order.
            let mut kept: Vec<serde_json::Value> = Vec::new();
            for e in entries.iter().rev() {
                let json = serde_json::to_value(e).unwrap_or_default();
                let size = json.to_string().len();
                if size > budget {
                    break;
                }
                budget -= size;
                kept.push(json);
            }
            kept.reverse();
            kept
        };
        let undo = keep(&self.undo);
        let redo = keep(&self.redo);
        let trimmed = self.trimmed || undo.len() < self.undo.len();
        let json = serde_json::json!({
            "version": 1,
            "settings": fingerprint(&written.settings),
            "trimmed": trimmed,
            "undo": undo,
            "redo": redo,
        });
        Some(json.to_string())
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
            "snapshots": self.snapshots,
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
        let mut state = DevelopState {
            settings,
            undo: stack("undo")?,
            redo: stack("redo")?,
            ..DevelopState::default()
        };
        state.restore_snapshots(&json);
        Ok(state)
    }

    /// Load from a file: `.json` uses the JSON form, anything else is read as
    /// XMP (sidecars, presets, files with an embedded XMP packet, .lrtemplate).
    pub fn load(path: &Path) -> Result<Self, Error> {
        let at = |e: String| Error::Load(format!("{}: {e}", path.display()));
        if has_extension(path, "json") {
            let text = std::fs::read_to_string(path).map_err(|e| at(e.to_string()))?;
            return Self::from_json_str(&text).map_err(|e| at(e.to_string()));
        }
        if has_extension(path, "xmp") {
            // A sidecar: with its stored history. A Lightroom catalogue
            // opened in place may know it better (`amb_presets::linked`).
            let text = match amb_presets::linked::text(path) {
                Some(t) => t,
                None => std::fs::read_to_string(path).map_err(|e| at(e.to_string()))?,
            };
            return Self::from_xmp_str(&text).map_err(|e| at(e.to_string()));
        }
        let preset = Preset::from_path(path).map_err(|e| at(e.to_string()))?;
        Ok(Self::from_settings(preset.settings))
    }

    /// [`load`](Self::load) a sidecar, or a new state when there is none
    /// (and no Lightroom catalogue opened in place knows its photo).
    pub fn load_or_new(path: &Path) -> Result<Self, Error> {
        if path.exists() || amb_presets::linked::get(path).is_some() {
            Self::load(path)
        } else {
            Ok(Self::new())
        }
    }

    /// Save to `.json` or `.xmp`, both with the history (in the XMP as
    /// `amblib:History`, newest steps first if it is long). When the XMP
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
            let history = self.history_for_sidecar();
            let snapshots = self.snapshots_for_sidecar();
            let linked = || amb_presets::linked::get(path).and(amb_presets::linked::text(path));
            let original = match base {
                Some(base) => base.to_string(),
                // A Lightroom catalogue opened in place: its metadata stays.
                None if !path.exists() || amb_presets::linked::get(path).is_some() => {
                    linked().unwrap_or_default()
                }
                None => match std::fs::read_to_string(path) {
                    Ok(existing) => existing,
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
                    Err(e) => return Err(Error::Io(format!("{}: {e}", path.display()))),
                },
            };
            let stored = sidecar::Stored {
                history: history.as_deref(),
                snapshots: snapshots.as_deref(),
            };
            sidecar::merge_with_stored(&original, &self.settings, stored)?
        } else {
            return Err(Error::Io(format!(
                "refusing to write {}: develop state is saved only as .xmp or .json",
                path.display()
            )));
        };
        sidecar::write_atomic(path, &text)
    }
}

fn has_extension(path: &Path, ext: &str) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case(ext))
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
