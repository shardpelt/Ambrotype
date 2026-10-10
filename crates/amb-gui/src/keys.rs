// SPDX-License-Identifier: GPL-3.0-or-later
//! Keyboard shortcuts the user changed. The actions and their default keys
//! live in the UI (`ui/keys.js`); only the changes are kept here, in the
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

#[cfg(test)]
mod tests {
    use super::*;

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
