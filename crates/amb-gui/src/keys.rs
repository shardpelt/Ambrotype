// SPDX-License-Identifier: GPL-3.0-or-later
//! Keyboard shortcuts the user changed. The actions and their default keys
//! live in the UI (`ui/core/keys.js`); only the changes are kept here, in the
//! app's settings file, so new actions and new defaults still arrive.

use std::collections::BTreeMap;

/// Action id -> its keys (e.g. "Ctrl+Shift+Z"), for the changed actions only.
pub type Bindings = BTreeMap<String, Vec<String>>;

const MAX_ACTIONS: usize = 128;
const MAX_KEYS: usize = 4;
const MAX_LEN: usize = 32;

/// A cleaned-up copy, or why it can't be saved.
pub fn validate(bindings: &Bindings) -> Result<Bindings, String> {
    if bindings.len() > MAX_ACTIONS {
        return Err(format!("at most {MAX_ACTIONS} actions"));
    }
    let mut out = Bindings::new();
    for (id, keys) in bindings {
        let id_ok = !id.is_empty()
            && id.len() <= MAX_LEN
            && id
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
        if !id_ok {
            return Err(format!("bad action name {id:?}"));
        }
        if keys.len() > MAX_KEYS {
            return Err(format!("{id}: at most {MAX_KEYS} keys"));
        }
        let mut clean: Vec<String> = Vec::new();
        for k in keys {
            let k = k.trim();
            if k.is_empty() || k.len() > MAX_LEN || k.chars().any(char::is_control) {
                return Err(format!("{id}: bad key {k:?}"));
            }
            if !clean.iter().any(|c| c == k) {
                clean.push(k.to_string());
            }
        }
        out.insert(id.clone(), clean);
    }
    Ok(out)
}

/// The changed shortcuts as a file to share: the same map as in the
/// settings file, one action per line.
pub fn to_file(bindings: &Bindings) -> Result<String, String> {
    let clean = validate(bindings)?;
    serde_json::to_string_pretty(&clean)
        .map(|t| t + "\n")
        .map_err(|e| e.to_string())
}

/// A shared shortcuts file. Whether each action exists is for the editor,
/// which knows them.
pub fn from_file(text: &str) -> Result<Bindings, String> {
    let bindings: Bindings = serde_json::from_str(text)
        .map_err(|_| "this isn't a shortcuts file (action name -> keys)".to_string())?;
    validate(&bindings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shortcuts_file_round_trips_and_a_stranger_is_refused() {
        let mut b = Bindings::new();
        b.insert("undo".into(), vec!["Ctrl+Alt+Q".into()]);
        b.insert("redo".into(), vec![]);
        assert_eq!(from_file(&to_file(&b).unwrap()).unwrap(), b);
        assert!(from_file("[1, 2]").is_err());
        assert!(from_file(r#"{"Bad Name": ["A"]}"#).is_err());
    }

    #[test]
    fn validates_and_dedups() {
        let mut b = Bindings::new();
        b.insert(
            "crop".into(),
            vec!["C".into(), " C ".into(), "Ctrl+Shift+K".into()],
        );
        b.insert("zoom".into(), vec![]); // no keys at all is allowed
        let v = validate(&b).unwrap();
        assert_eq!(v["crop"], vec!["C", "Ctrl+Shift+K"]);
        assert!(v["zoom"].is_empty());

        let bad = |id: &str, keys: Vec<&str>| {
            let mut b = Bindings::new();
            b.insert(id.into(), keys.into_iter().map(String::from).collect());
            validate(&b).is_err()
        };
        assert!(bad("Crop!", vec!["C"]));
        assert!(bad("crop", vec![""]));
        assert!(bad("crop", vec!["A", "B", "C", "D", "E"]));
        assert!(bad("crop", vec!["Ctrl+\tA"]));
    }
}
