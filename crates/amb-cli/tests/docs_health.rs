// SPDX-License-Identifier: GPL-3.0-or-later
//! Guard tests: the docs map, the links between docs, and the crates the
//! docs name (docs/README.md, docs/codestyle.md).

use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap()
}

/// Every file under `dir` with this extension.
fn files_with(dir: &Path, ext: &str) -> Vec<PathBuf> {
    let mut dirs = vec![dir.to_path_buf()];
    let mut found = Vec::new();
    while let Some(d) = dirs.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                if p.file_name().is_some_and(|n| n != "target" && n != ".git") {
                    dirs.push(p);
                }
            } else if p.extension().is_some_and(|x| x == ext) {
                found.push(p);
            }
        }
    }
    found.sort();
    found
}

/// The text outside code: fenced blocks and inline `code` spans removed, so a
/// doc may write about link syntax without making a link.
fn outside_code(text: &str) -> String {
    let mut fenced = false;
    let mut out = String::new();
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
        } else if !fenced {
            for part in line.split('`').step_by(2) {
                out.push_str(part);
            }
            out.push('\n');
        }
    }
    out
}

/// The targets of `](...)` links in a text, without anchors, URLs excluded.
fn link_targets(text: &str) -> Vec<String> {
    outside_code(text)
        .split("](")
        .skip(1)
        .filter_map(|rest| rest.split(')').next())
        .map(|t| t.split('#').next().unwrap_or_default().trim().to_string())
        .filter(|t| !t.is_empty() && !t.contains("://") && !t.starts_with("mailto:"))
        .collect()
}

/// The docs reachable from `docs/README.md` by relative links: the map lists
/// the entry points (`ai/README.md`, `design/tabs.md`), and those link on.
fn reachable_from_the_map() -> Vec<PathBuf> {
    let docs = root().join("docs").canonicalize().unwrap();
    let mut seen = vec![docs.join("README.md")];
    let mut todo = seen.clone();
    while let Some(file) = todo.pop() {
        for target in link_targets(&read(&file)) {
            let Ok(path) = file.parent().unwrap().join(&target).canonicalize() else {
                continue;
            };
            if path.starts_with(&docs)
                && path.extension().is_some_and(|x| x == "md")
                && !seen.contains(&path)
            {
                seen.push(path.clone());
                todo.push(path);
            }
        }
    }
    seen
}

#[test]
fn every_doc_is_in_the_map() {
    let docs = root().join("docs").canonicalize().unwrap();
    let reachable = reachable_from_the_map();
    let mut missing = Vec::new();
    for file in files_with(&docs, "md") {
        let rel = file
            .strip_prefix(&docs)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let numbered_research = rel
            .strip_prefix("research/")
            .is_some_and(|r| r.chars().next().is_some_and(|c| c.is_ascii_digit()));
        if numbered_research || rel.starts_with("release-notes/") {
            continue;
        }
        if !reachable.contains(&file) {
            missing.push(rel);
        }
    }
    assert!(
        missing.is_empty(),
        "link each from docs/README.md, or from a doc it links to: {missing:?}"
    );
}

#[test]
fn relative_links_resolve() {
    let root = root();
    let mut files = vec![
        root.join("README.md"),
        root.join("AGENTS.md"),
        root.join("CONTRIBUTING.md"),
    ];
    files.extend(files_with(&root.join("docs"), "md"));
    let mut broken = Vec::new();
    for file in files.into_iter().filter(|f| f.exists()) {
        let dir = file.parent().unwrap();
        for target in link_targets(&read(&file)) {
            if !dir.join(&target).exists() {
                broken.push(format!(
                    "{} -> {target}",
                    file.strip_prefix(&root).unwrap().display()
                ));
            }
        }
    }
    assert!(broken.is_empty(), "broken links: {broken:#?}");
}

#[test]
fn every_crate_is_named() {
    let root = root();
    let docs = ["AGENTS.md", "README.md", "docs/ARCHITECTURE.md"];
    let mut missing = Vec::new();
    for entry in std::fs::read_dir(root.join("crates")).unwrap().flatten() {
        if !entry.path().is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        for doc in docs {
            if !read(&root.join(doc)).contains(&name) {
                missing.push(format!("{name} in {doc}"));
            }
        }
    }
    assert!(
        missing.is_empty(),
        "crates the docs don't name: {missing:?}"
    );
}
