// SPDX-License-Identifier: GPL-3.0-or-later
//! Guard tests: the lists a feature is added to, and the old idioms whose
//! counts may only fall (docs/codestyle.md).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn gui_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

/// Every file under `dir` with this extension, as (path from `dir`, text).
fn files_under(dir: &Path, ext: &str) -> Vec<(String, String)> {
    let mut dirs = vec![dir.to_path_buf()];
    let mut found = Vec::new();
    while let Some(d) = dirs.pop() {
        for entry in std::fs::read_dir(&d).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                dirs.push(path);
            } else if path.extension().is_some_and(|x| x == ext) {
                let rel = path
                    .strip_prefix(dir)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                found.push((rel, std::fs::read_to_string(&path).unwrap()));
            }
        }
    }
    found.sort();
    found
}

/// Every file under `crates/amb-gui/ui/` with this extension: (path from `ui/`, text).
fn ui_text(ext: &str) -> Vec<(String, String)> {
    files_under(&gui_root().join("ui"), ext)
}

/// Every `.rs` file under `crates/amb-gui/src/`: (path from `src/`, text).
fn src_text() -> Vec<(String, String)> {
    files_under(&gui_root().join("src"), "rs")
}

/// The names `#[tauri::command]` marks, in every source file.
fn declared_commands() -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for (_, text) in src_text() {
        let lines: Vec<&str> = text.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            if line.trim() != "#[tauri::command]" {
                continue;
            }
            let decl = lines[i + 1..]
                .iter()
                .find(|l| {
                    let t = l.trim_start();
                    !t.starts_with("#[") && !t.starts_with("//")
                })
                .expect("a function after #[tauri::command]");
            let after_fn = decl
                .split("fn ")
                .nth(1)
                .expect("`fn` after #[tauri::command]");
            let name: String = after_fn
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            names.insert(name);
        }
    }
    names
}

/// The last path part of each entry of `tauri::generate_handler![ ... ]`.
fn handler_commands() -> BTreeSet<String> {
    let text = std::fs::read_to_string(gui_root().join("src/commands/mod.rs")).unwrap();
    let list = text
        .split("tauri::generate_handler![")
        .nth(1)
        .expect("generate_handler! in commands/mod.rs")
        .split(']')
        .next()
        .unwrap();
    list.lines()
        .map(|l| l.split("//").next().unwrap_or_default())
        .flat_map(|l| l.split(','))
        .map(|e| e.trim().rsplit("::").next().unwrap_or_default().to_string())
        .filter(|e| !e.is_empty())
        .collect()
}

#[test]
fn every_tauri_command_is_in_the_handler() {
    let declared = declared_commands();
    let listed = handler_commands();
    let missing: Vec<_> = declared.difference(&listed).collect();
    let extra: Vec<_> = listed.difference(&declared).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "add to `handler` in src/commands/mod.rs: {missing:?}; listed but not a #[tauri::command]: {extra:?}"
    );
}

#[test]
fn the_ui_calls_only_registered_commands() {
    let listed = handler_commands();
    let mut scripts = ui_text("js");
    scripts.extend(files_under(&gui_root().join("tests/ui"), "js"));
    let mut unknown = BTreeSet::new();
    for (file, text) in &scripts {
        for part in text.split("invoke(").skip(1) {
            let part = part.trim_start();
            if let Some(rest) = part.strip_prefix('"') {
                let name = rest.split('"').next().unwrap_or_default();
                if !listed.contains(name) {
                    unknown.insert(format!("{name} (in {file})"));
                }
            }
        }
    }
    assert!(
        unknown.is_empty(),
        "the UI calls commands the handler doesn't list: {unknown:?}"
    );
}

#[test]
fn every_wire_function_is_called() {
    let scripts = ui_text("js");
    let mut uncalled = Vec::new();
    for (file, text) in &scripts {
        for line in text.lines() {
            let rest = line
                .strip_prefix("async function ")
                .or_else(|| line.strip_prefix("function "));
            let Some(rest) = rest.filter(|r| r.starts_with("wire")) else {
                continue;
            };
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            let uses: usize = scripts.iter().map(|(_, t)| word_count(t, &name)).sum();
            if uses < 2 {
                uncalled.push(format!("{name} (in {file})"));
            }
        }
    }
    assert!(
        uncalled.is_empty(),
        "wire functions nobody calls (call it from the part's wire function or `init` in core/app.js): {uncalled:?}"
    );
}

/// How often `word` appears in `text` as a whole identifier.
fn word_count(text: &str, word: &str) -> usize {
    let ident = |c: char| c.is_alphanumeric() || c == '_' || c == '$';
    text.match_indices(word)
        .filter(|(i, _)| {
            let before = text[..*i].chars().next_back();
            let after = text[i + word.len()..].chars().next();
            !before.is_some_and(ident) && !after.is_some_and(ident)
        })
        .count()
}

// The old idioms, as they stand. Lower a limit when you remove some, never
// raise it: the helper in the message does the same job.
const OBJECT_ASSIGN_LIMIT: usize = 365;
const INNER_HTML_LIMIT: usize = 15;
const INLINE_STYLE_LIMIT: usize = 17;
const TOP_LEVEL_LET_LIMIT: usize = 7;
const TOAST_STRING_LIMIT: usize = 0;
const LOCAL_STORAGE_LIMIT: usize = 0;

#[test]
fn old_idioms_may_only_fall() {
    let scripts = ui_text("js");
    let count = |needle: &str, skip: &str| -> usize {
        scripts
            .iter()
            .filter(|(f, _)| f != skip)
            .map(|(_, t)| t.matches(needle).count())
            .sum()
    };
    let page = std::fs::read_to_string(gui_root().join("ui/index.html")).unwrap();
    let top_level_let: usize = scripts
        .iter()
        .map(|(_, t)| t.lines().filter(|l| l.starts_with("let ")).count())
        .sum();
    let checks = [
        (
            "Object.assign(document.createElement(",
            count("Object.assign(document.createElement(", ""),
            OBJECT_ASSIGN_LIMIT,
            "`el` from core.js",
        ),
        (
            "innerHTML",
            count("innerHTML", ""),
            INNER_HTML_LIMIT,
            "`textContent`, or `el` with children",
        ),
        (
            "style=\" in index.html",
            page.matches("style=\"").count(),
            INLINE_STYLE_LIMIT,
            "a `class` in the stylesheet",
        ),
        (
            "top-level let",
            top_level_let,
            TOP_LEVEL_LET_LIMIT,
            "a `const` state object",
        ),
        (
            "toast(String(",
            count("toast(String(", ""),
            TOAST_STRING_LIMIT,
            "`showError`",
        ),
        (
            "localStorage outside core/core.js",
            count("localStorage", "core/core.js"),
            LOCAL_STORAGE_LIMIT,
            "`store`",
        ),
    ];
    let mut problems = Vec::new();
    for (idiom, now, limit, helper) in checks {
        if now > limit {
            problems.push(format!(
                "{idiom}: {now} uses, limit {limit}. Use {helper} instead; the count may only fall"
            ));
        }
    }
    assert!(problems.is_empty(), "\n{}\n", problems.join("\n"));
}
