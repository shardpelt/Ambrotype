// SPDX-License-Identifier: GPL-3.0-or-later
//! The size budget (docs/design/structure.md): a Rust file of at most 800
//! production lines (the lines before its `#[cfg(test)]` part), a UI script
//! of at most 700, a function of at most 150 lines. Test files don't count.
//! Files and functions already past it are listed with their own limit until
//! they are split (docs/codestyle.md §3 and §11).

use std::path::{Path, PathBuf};

const RUST_BUDGET: usize = 800;
const JS_BUDGET: usize = 700;

/// Files past the budget, with the most lines each may have: its size on the
/// day the limit was set (2026-11-12) rounded up to the next 50. Splitting a
/// file takes it off this list. A file that grows past its number is split
/// (docs/codestyle.md §11 says how), not given a bigger number.
const OVER_BUDGET: &[(&str, usize)] = &[];

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
                "{rel}: {lines} lines, past its limit of {limit}: split it (docs/codestyle.md §11), or raise its number in OVER_BUDGET and say why"
            )),
            Some(_) if lines <= budget => problems.push(format!(
                "{rel}: {lines} lines, within the budget of {budget} now: take it off OVER_BUDGET"
            )),
            None if lines > budget => problems.push(format!(
                "{rel}: {lines} lines, past the budget of {budget}: split it along its sections (docs/codestyle.md §11)"
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

/// The most lines a function may have (docs/codestyle.md §3).
const FUNCTION_BUDGET: usize = 150;

/// Functions past it, with the most lines each may have: its length on the
/// day the limit was set (2026-11-20). Shortening one to the budget takes it
/// off; one that would grow is split into helpers first.
const LONG_FUNCTIONS: &[(&str, &str, usize)] = &[
    ("crates/amb-ai/src/net/plan.rs", "matmul", 199),
    ("crates/amb-cli/src/lens.rs", "lens_match_output", 158),
    ("crates/amb-cli/src/render.rs", "export_settings", 187),
    (
        "crates/amb-engine/src/lens/camera.rs",
        "apply_camera_window",
        215,
    ),
    ("crates/amb-engine/src/mask/geometry.rs", "draw_shape", 158),
    ("crates/amb-engine/src/merge.rs", "hdr", 152),
    ("crates/amb-engine/src/pano/align.rs", "align", 157),
    (
        "crates/amb-engine/src/retouch/heal.rs",
        "search_offset",
        160,
    ),
    ("crates/amb-export/src/exif.rs", "export_dirs", 153),
    ("crates/amb-ops/src/object/words.rs", "select_words", 197),
    ("crates/amb-plugin/src/fuji.rs", "parse", 180),
    ("crates/amb-plugin/src/manifest.rs", "validate", 187),
    ("crates/amb-protocol/src/masks/mod.rs", "paint", 167),
    ("crates/amb-protocol/src/sidecar/read.rs", "read_info", 153),
    (
        "crates/amb-protocol/src/sidecar/write.rs",
        "apply_info",
        166,
    ),
    ("plugins/film-recipe/src/lib.rs", "look", 167),
    ("plugins/film-recipe/src/lib.rs", "run", 158),
    ("crates/amb-gui/ui/dialogs/hub.js", "hubFlow", 177),
    ("crates/amb-gui/ui/library/library.js", "wireLibrary", 164),
    (
        "crates/amb-gui/ui/looks/editor-inspector.js",
        "openInspector",
        153,
    ),
];

/// Functions the budget leaves alone, and why: dispatch and tables that grow
/// by one line per feature (docs/recipes.md says where), and algorithms
/// ported whole, kept as their source has them.
const GROWING_FUNCTIONS: &[(&str, &str, &str)] = &[
    (
        "crates/amb-cli/src/main.rs",
        "main",
        "amb's commands, one arm each",
    ),
    (
        "crates/amb-gui/src/commands/mod.rs",
        "handler",
        "the editor commands' list",
    ),
    (
        "crates/amb-engine/src/pipeline/settings/from_settings.rs",
        "from_settings",
        "one line per setting",
    ),
    (
        "crates/amb-engine/src/pipeline/mod.rs",
        "develop",
        "the stages, in pipeline order",
    ),
    (
        "crates/amb-engine/src/plan.rs",
        "plan_with",
        "a GPU step per stage",
    ),
    (
        "crates/amb-gpu/src/develop.rs",
        "render",
        "a kernel per stage",
    ),
    (
        "crates/amb-engine/src/xtrans.rs",
        "run",
        "Markesteijn's demosaic, ported",
    ),
    (
        "crates/amb-engine/src/rcd.rs",
        "band",
        "RCD demosaic, as its sources have it",
    ),
    (
        "crates/amb-gui/ui/core/app.js",
        "init",
        "the toolbar's click handlers",
    ),
];

/// `src` with comments, strings and character literals blanked, their line
/// breaks kept, so the braces left are the code's (as tools/code-metrics.py).
fn quiet(src: &str, rust: bool) -> String {
    let b = src.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match literal_end(b, i, rust) {
            Some(end) => {
                out.extend(b[i..end].iter().filter(|&&c| c == b'\n'));
                i = end;
            }
            None => {
                out.push(b[i]);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Where the comment or literal that starts at `i` ends, if one starts there.
fn literal_end(b: &[u8], i: usize, rust: bool) -> Option<usize> {
    let at = |from: usize, s: &[u8]| b[from..].starts_with(s);
    let past = |from: usize, s: &[u8]| {
        b[from..]
            .windows(s.len())
            .position(|w| w == s)
            .map_or(b.len(), |p| from + p + s.len())
    };
    // A quoted literal from its opening quote at `q`, with `\` escapes.
    let quoted = |q: usize, multiline: bool| {
        let mut j = q + 1;
        while j < b.len() {
            match b[j] {
                b'\\' => j += 2,
                b'\n' if !multiline => return None,
                c if c == b[q] => return Some(j + 1),
                _ => j += 1,
            }
        }
        multiline.then_some(b.len())
    };
    if at(i, b"//") {
        return Some(past(i, b"\n") - usize::from(b[..past(i, b"\n")].ends_with(b"\n")));
    }
    if at(i, b"/*") {
        return Some(past(i + 2, b"*/"));
    }
    if !rust {
        return match b[i] {
            b'`' => quoted(i, true),
            b'\'' | b'"' => quoted(i, false),
            _ => None,
        };
    }
    let word_before = i > 0 && (b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_');
    let raw = if at(i, b"br") { i + 2 } else { i + 1 };
    if !word_before && (at(i, b"r") || at(i, b"br")) {
        let hashes = b[raw..].iter().take_while(|&&c| c == b'#').count();
        if b.get(raw + hashes) == Some(&b'"') {
            let close: Vec<u8> = std::iter::once(b'"')
                .chain(std::iter::repeat_n(b'#', hashes))
                .collect();
            return Some(past(raw + hashes + 1, &close));
        }
    }
    let q = if at(i, b"b\"") || at(i, b"b'") {
        i + 1
    } else {
        i
    };
    match b[q] {
        b'"' => quoted(q, true),
        // A character literal ('x', '\n'), not a lifetime ('a).
        b'\'' if b.get(q + 1) == Some(&b'\\') => (b.get(q + 3) == Some(&b'\'')).then_some(q + 4),
        b'\'' => {
            let len = match b.get(q + 1)? {
                c if *c < 0x80 => 1,
                c if *c >= 0xF0 => 4,
                c if *c >= 0xE0 => 3,
                _ => 2,
            };
            (*b.get(q + 1)? != b'\'' && b.get(q + 1 + len) == Some(&b'\'')).then_some(q + 2 + len)
        }
        _ => None,
    }
}

/// The name a line starts a function of, as tools/code-metrics.py reads it:
/// `[pub[(…)]] [const] [async] [unsafe] fn name` in Rust,
/// `[async] function[*] name` in a script.
fn function_name(line: &str, rust: bool) -> Option<&str> {
    let word = |s: &str| {
        s.len()
            - s.trim_start_matches(|c: char| c.is_alphanumeric() || c == '_')
                .len()
    };
    let space = |s: &str| s.len() - s.trim_start().len();
    let mut s = line.trim_start();
    // `kw` and the whitespace after it, if `s` starts with them.
    let take = |s: &mut &str, kw: &str, need_space: bool| {
        if let Some(rest) = s.strip_prefix(kw)
            && (!need_space || space(rest) > 0)
        {
            *s = rest.trim_start();
            return true;
        }
        false
    };
    if rust {
        if let Some(rest) = s.strip_prefix("pub") {
            let rest = match rest.strip_prefix('(') {
                Some(r) => r.split_once(')').map_or(rest, |(_, after)| after),
                None => rest,
            };
            if space(rest) > 0 {
                s = rest.trim_start();
            }
        }
        for kw in ["const", "async", "unsafe"] {
            take(&mut s, kw, true);
        }
        if !take(&mut s, "fn", true) {
            return None;
        }
    } else {
        take(&mut s, "async", true);
        if !take(&mut s, "function", false) {
            return None;
        }
        s = s.strip_prefix('*').unwrap_or(s).trim_start();
    }
    let n = word(s);
    (n > 0).then(|| &s[..n])
}

/// (lines, name) of each function in `lines` (quiet), by matching braces.
fn functions(lines: &[&str], rust: bool) -> Vec<(usize, String)> {
    let mut found = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let Some(name) = function_name(line, rust) else {
            continue;
        };
        if line.trim_end().ends_with(';') {
            continue;
        }
        let (mut depth, mut started, mut end) = (0i64, false, lines.len() - 1);
        for (j, l) in lines.iter().enumerate().skip(i) {
            depth += l.matches('{').count() as i64 - l.matches('}').count() as i64;
            started |= l.contains('{');
            if started && depth <= 0 {
                end = j;
                break;
            }
        }
        found.push((end - i + 1, name.to_string()));
    }
    found
}

#[test]
fn functions_stay_within_their_budget() {
    let mut long = Vec::new();
    for dir in ["crates", "plugins"] {
        for rel in files(dir, "rs") {
            let text = std::fs::read_to_string(root().join(&rel)).unwrap();
            let quiet = quiet(&text, true);
            let lines: Vec<&str> = quiet.lines().take(rust_lines(&rel, &text)).collect();
            long.extend(
                functions(&lines, true)
                    .into_iter()
                    .map(|(n, f)| (rel.clone(), f, n)),
            );
        }
    }
    for rel in files("crates/amb-gui/ui", "js") {
        let text = std::fs::read_to_string(root().join(&rel)).unwrap();
        let quiet = quiet(&text, false);
        let lines: Vec<&str> = quiet.lines().collect();
        long.extend(
            functions(&lines, false)
                .into_iter()
                .map(|(n, f)| (rel.clone(), f, n)),
        );
    }
    long.retain(|(rel, name, n)| {
        *n > FUNCTION_BUDGET
            && !GROWING_FUNCTIONS
                .iter()
                .any(|(f, g, _)| f == rel && g == name)
    });

    let mut problems = Vec::new();
    for (rel, name, n) in &long {
        match LONG_FUNCTIONS.iter().find(|(f, g, _)| f == rel && g == name) {
            Some((_, _, limit)) if n > limit => problems.push(format!(
                "{rel}: `{name}` is {n} lines, past its limit of {limit}: move a part into a helper first"
            )),
            None => problems.push(format!(
                "{rel}: `{name}` is {n} lines, past the budget of {FUNCTION_BUDGET}: split it into helpers (dispatch or a table that grows per feature goes in GROWING_FUNCTIONS, with why)"
            )),
            _ => {}
        }
    }
    for (rel, name, _) in LONG_FUNCTIONS {
        if !long.iter().any(|(f, g, _)| f == rel && g == name) {
            problems.push(format!(
                "{rel}: `{name}` is within {FUNCTION_BUDGET} lines now (or gone): take it off LONG_FUNCTIONS"
            ));
        }
    }
    assert!(problems.is_empty(), "\n{}\n", problems.join("\n"));
}
