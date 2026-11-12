// SPDX-License-Identifier: GPL-3.0-or-later
//! Guard tests: the help text against the dispatch and the features list,
//! and the counts that may only fall (docs/codestyle.md).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(root().join(rel)).unwrap()
}

/// The command names of `USAGE`: `amb NAME` at the start of a line, indented four spaces.
fn usage_names() -> BTreeSet<String> {
    read("crates/amb-cli/src/usage.rs")
        .lines()
        .filter_map(|l| l.strip_prefix("    amb "))
        .map(|rest| {
            rest.chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
                .collect::<String>()
        })
        .filter(|n| !n.is_empty())
        .collect()
}

#[test]
fn every_usage_command_has_an_arm() {
    let main = read("crates/amb-cli/src/main.rs");
    let missing: Vec<_> = usage_names()
        .into_iter()
        .filter(|n| !main.contains(&format!("\"{n}\"")))
        .collect();
    assert!(
        missing.is_empty(),
        "in the help text but not in main.rs's match: {missing:?}"
    );
}

#[test]
fn every_features_command_is_in_usage() {
    let usage = usage_names();
    let features = read("docs/FEATURES.md");
    let mut missing = BTreeSet::new();
    for part in features.split("`amb ").skip(1) {
        let name: String = part
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
            .collect();
        if !name.is_empty() && !usage.contains(&name) {
            missing.insert(name);
        }
    }
    assert!(
        missing.is_empty(),
        "docs/FEATURES.md names `amb` commands the help text lacks: {missing:?}"
    );
}

/// Every `.rs` file under `crates/*/src` and `plugins/*/src`, with its
/// production lines (those before its first `#[cfg(test)]`; test files have none:
/// `tests/`, `tests.rs` and the editor's `checks.rs`, a module only tests build).
fn production_sources() -> Vec<(String, String)> {
    let mut found = Vec::new();
    for top in ["crates", "plugins"] {
        for entry in std::fs::read_dir(root().join(top)).unwrap().flatten() {
            let mut dirs = vec![entry.path().join("src")];
            while let Some(d) = dirs.pop() {
                let Ok(read_dir) = std::fs::read_dir(&d) else {
                    continue;
                };
                for e in read_dir.flatten() {
                    let p = e.path();
                    let name = p.to_string_lossy().replace('\\', "/");
                    if p.is_dir() {
                        dirs.push(p);
                    } else if name.ends_with(".rs")
                        && !name.contains("/tests/")
                        && !name.ends_with("/tests.rs")
                        && !name.ends_with("/amb-gui/src/checks.rs")
                    {
                        let text = std::fs::read_to_string(&p).unwrap();
                        let production: Vec<&str> = text
                            .lines()
                            .take_while(|l| !l.starts_with("#[cfg(test)]"))
                            .collect();
                        found.push((name, production.join("\n")));
                    }
                }
            }
        }
    }
    found
}

// Lower a limit when you remove some, never raise it.
const ALLOW_LIMIT: usize = 16;
const UNWRAP_LIMIT: usize = 1;

#[test]
fn counts_may_only_fall() {
    let sources = production_sources();
    let count = |needles: &[&str]| -> Vec<(String, usize)> {
        sources
            .iter()
            .map(|(f, t)| {
                (
                    f.clone(),
                    needles.iter().map(|n| t.matches(n).count()).sum::<usize>(),
                )
            })
            .filter(|(_, n)| *n > 0)
            .collect()
    };
    let allows = count(&["#[allow(", "#![allow("]);
    let total: usize = allows.iter().map(|(_, n)| n).sum();
    assert!(
        total <= ALLOW_LIMIT,
        "#[allow] count {total} is past {ALLOW_LIMIT}: fix the cause or use an Options struct. In: {allows:?}"
    );
    let unwraps = count(&[".unwrap()"]);
    let total: usize = unwraps.iter().map(|(_, n)| n).sum();
    assert!(
        total <= UNWRAP_LIMIT,
        "bare unwrap() count {total} is past {UNWRAP_LIMIT}: return the crate's Error, or expect(\"why it can't fail\"). In: {unwraps:?}"
    );
}
