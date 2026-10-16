// SPDX-License-Identifier: GPL-3.0-or-later
//! docs/recipes.md names files and what's in them; check they still exist.
//! A list item's paths (`crates/...`, `docs/...`, `tools/...`, `.claude/...`)
//! must exist, and the names written with them (`Name`) must appear in the
//! first file of that item. Names in its sub-items must appear somewhere
//! under `crates/`.

use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn ticks(line: &str) -> Vec<&str> {
    line.split('`').skip(1).step_by(2).collect()
}

fn is_path(t: &str) -> bool {
    !t.contains(' ')
        && ["crates/", "docs/", "tools/", ".claude/"]
            .iter()
            .any(|p| t.starts_with(p))
}

fn is_name(t: &str) -> bool {
    let mut c = t.chars();
    c.next()
        .is_some_and(|f| f.is_ascii_alphabetic() || f == '_')
        && c.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn anywhere(dir: &Path, name: &str) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    entries.flatten().any(|e| {
        let p = e.path();
        if p.is_dir() {
            p.file_name().is_some_and(|n| n != "target") && anywhere(&p, name)
        } else {
            std::fs::read_to_string(&p).is_ok_and(|t| t.contains(name))
        }
    })
}

#[test]
fn recipes_name_files_and_names_that_exist() {
    let root = root();
    let text = std::fs::read_to_string(root.join("docs/recipes.md")).unwrap();
    let mut file: Option<PathBuf> = None;
    let mut problems = Vec::new();
    let mut checked = 0;
    for line in text.lines() {
        let top = !line.starts_with(' ');
        let toks = ticks(line);
        if top {
            file = None;
        }
        for t in &toks {
            if is_path(t) {
                let p = root.join(t.trim_end_matches('/'));
                if !p.exists() {
                    problems.push(format!("missing path {t}"));
                } else if top && file.is_none() && p.is_file() {
                    file = Some(p);
                }
            }
        }
        for t in toks.iter().filter(|t| is_name(t)) {
            checked += 1;
            let found = match (&file, top) {
                (Some(f), true) => std::fs::read_to_string(f).is_ok_and(|s| s.contains(t)),
                _ => anywhere(&root.join("crates"), t),
            };
            if !found {
                let place = file
                    .as_ref()
                    .map_or("crates/".into(), |f| f.display().to_string());
                problems.push(format!("`{t}` not found in {place}"));
            }
        }
    }
    assert!(checked > 40, "only {checked} names checked");
    assert!(
        problems.is_empty(),
        "docs/recipes.md is out of date:\n{}",
        problems.join("\n")
    );
}
