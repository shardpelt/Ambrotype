// SPDX-License-Identifier: GPL-3.0-or-later
//! The size budget (docs/design/structure.md): a Rust file of at most 800
//! production lines (the lines before its `#[cfg(test)]` part), a UI script
//! of at most 700. Test files don't count. Files already past it are listed
//! with their own limit until they are split (docs/cleanup.md, phase 2).

use std::path::{Path, PathBuf};

const RUST_BUDGET: usize = 800;
const JS_BUDGET: usize = 700;

/// Files past the budget, with the most lines each may have: its size on
/// 2026-11-09 rounded up to the next hundred, plus a hundred. Splitting a
/// file takes it off this list. A file that would grow past its number is
/// split instead (docs/cleanup.md says how), or its number is raised in the
/// same commit, saying why in the commit message.
const OVER_BUDGET: &[(&str, usize)] = &[
    ("crates/amb-protocol/src/masks.rs", 1300),
    ("crates/amb-ai/src/net.rs", 1300),
    ("crates/amb-engine/src/lens.rs", 1200),
    ("crates/amb-engine/src/retouch.rs", 1100),
    ("crates/amb-protocol/src/sidecar.rs", 1100),
    ("crates/amb-export/src/embed.rs", 1100),
    ("crates/amb-library/src/object.rs", 1100),
    ("crates/amb-engine/src/matching.rs", 1100),
    ("crates/amb-engine/src/pipeline/settings.rs", 1100),
    ("crates/amb-gpu/src/nn.rs", 1000),
    ("crates/amb-ai/src/lib.rs", 1000),
    ("crates/amb-gui/ui/library.js", 1600),
    ("crates/amb-gui/ui/collage.js", 1600),
    ("crates/amb-gui/ui/app.js", 1300),
    ("crates/amb-gui/ui/masks.js", 1200),
    ("crates/amb-gui/ui/layout.js", 1200),
    ("crates/amb-gui/ui/editor.js", 1100),
];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Files with this extension under `dir`, as paths from the repository's
/// root with `/` between the parts (build folders left out).
fn files(dir: &str, ext: &str) -> Vec<String> {
    let root = root();
    let mut dirs = vec![root.join(dir)];
    let mut found = Vec::new();
    while let Some(d) = dirs.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if path.is_dir() {
                if name != "target" && !name.starts_with('.') {
                    dirs.push(path);
                }
            } else if path.extension().is_some_and(|x| x == ext) {
                let rel = path.strip_prefix(&root).unwrap();
                let parts: Vec<String> = rel
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().to_string())
                    .collect();
                found.push(parts.join("/"));
            }
        }
    }
    found.sort();
    found
}

/// A Rust file's production lines: those before its first `#[cfg(test)]`
/// at the start of a line. Test files have none.
fn rust_lines(rel: &str, text: &str) -> usize {
    if rel.contains("/tests/") || rel.ends_with("/tests.rs") {
        return 0;
    }
    text.lines()
        .position(|l| l.starts_with("#[cfg(test)]"))
        .unwrap_or_else(|| text.lines().count())
}

#[test]
fn files_stay_within_the_size_budget() {
    let mut sizes = Vec::new();
    for dir in ["crates", "plugins"] {
        for rel in files(dir, "rs") {
            let text = std::fs::read_to_string(root().join(&rel)).unwrap();
            sizes.push((rust_lines(&rel, &text), RUST_BUDGET, rel));
        }
    }
    for rel in files("crates/amb-gui/ui", "js") {
        let text = std::fs::read_to_string(root().join(&rel)).unwrap();
        sizes.push((text.lines().count(), JS_BUDGET, rel));
    }

    let mut problems = Vec::new();
    for (lines, budget, rel) in &sizes {
        match OVER_BUDGET.iter().find(|(f, _)| f == rel) {
            Some((_, limit)) if lines > limit => problems.push(format!(
                "{rel}: {lines} lines, past its limit of {limit}: split it (docs/cleanup.md), or raise its number in OVER_BUDGET and say why"
            )),
            Some(_) if lines <= budget => problems.push(format!(
                "{rel}: {lines} lines, within the budget of {budget} now: take it off OVER_BUDGET"
            )),
            None if lines > budget => problems.push(format!(
                "{rel}: {lines} lines, past the budget of {budget}: split it along its sections (docs/cleanup.md)"
            )),
            _ => {}
        }
    }
    for (rel, _) in OVER_BUDGET {
        if !sizes.iter().any(|(_, _, f)| f == rel) {
            problems.push(format!(
                "{rel} is gone (split or moved?): take it off OVER_BUDGET"
            ));
        }
    }
    assert!(problems.is_empty(), "\n{}\n", problems.join("\n"));
}
