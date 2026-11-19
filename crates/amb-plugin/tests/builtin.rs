// SPDX-License-Identifier: GPL-3.0-or-later
//! The built-in plugins (`amb_plugin::BUILTIN`) are the folders in
//! `plugins/`: a new plugin folder that isn't built in, or a built-in id
//! with no folder, fails here.

use std::collections::BTreeSet;

/// Folders in `plugins/` kept as examples, not built in.
const EXAMPLES: &[&str] = &[];

#[test]
fn the_built_in_list_matches_the_plugins_folder() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins");
    let mut folders = BTreeSet::new();
    for entry in std::fs::read_dir(&root).unwrap() {
        let manifest = entry.unwrap().path().join("plugin.json");
        let Ok(text) = std::fs::read_to_string(&manifest) else {
            continue;
        };
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        let id = v["id"].as_str().unwrap().to_string();
        if !EXAMPLES.contains(&id.as_str()) {
            folders.insert(id);
        }
    }
    let built_in: BTreeSet<String> = amb_plugin::BUILTIN
        .iter()
        .map(|(_, id, _)| id.to_string())
        .collect();
    assert_eq!(
        folders, built_in,
        "plugins/ (left) and BUILTIN (right) must hold the same ids"
    );
}
