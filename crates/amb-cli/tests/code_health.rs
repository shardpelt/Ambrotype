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

/// AI-free mode ([`amb_presets::ai_free`]) promises no model touches a photo
/// by any path. Two source checks keep that true when code is added:
///
/// 1. Every engine hook setter (`pub fn set_…` in `amb-engine`) is listed in
///    `GATED_HOOKS`, and the engine files that call the hooks keep their
///    gates (a count of `ai_free::` uses).
/// 2. Model loading is only in `amb-ai`, and in each of its files the places
///    that load a model (`model_for_path(`) or fetch one (`ureq::`) are no
///    more than that file's `ai_free::check` calls. A new load site without a
///    gate (or a new file with one) fails here. The table below lists the
///    load sites by file, so a new site is a conscious edit.
#[test]
fn every_model_path_passes_ai_free_mode() {
    let sources = production_sources();

    let uses = |files: &[(String, String)], file: &str, needle: &str| -> usize {
        files
            .iter()
            .filter(|(f, _)| f.ends_with(file))
            .map(|(_, t)| t.matches(needle).count())
            .sum()
    };

    // 1. Hooks. (Whole files: `subject.rs` has a test-only function before
    // its setters, which `production_sources` would cut at.)
    let whole = |krate: &str| -> Vec<(String, String)> {
        let mut files = Vec::new();
        let mut dirs = vec![root().join(format!("crates/{krate}/src"))];
        while let Some(d) = dirs.pop() {
            for e in std::fs::read_dir(&d).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    dirs.push(p);
                } else if p.extension().is_some_and(|x| x == "rs") && !p.ends_with("tests.rs") {
                    files.push((
                        p.to_string_lossy().replace('\\', "/"),
                        std::fs::read_to_string(&p).unwrap(),
                    ));
                }
            }
        }
        files
    };
    let engine_files = whole("amb-engine");
    let mut setters = BTreeSet::new();
    for (_, text) in &engine_files {
        for line in text.lines() {
            if let Some(rest) = line.trim().strip_prefix("pub fn set_") {
                let name: String = rest
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect();
                setters.insert(format!("set_{name}"));
            }
        }
    }
    // The plugin resolver resolves effects; it runs no model.
    setters.remove("set_resolver");
    let listed: BTreeSet<String> = amb_presets::ai_free::GATED_HOOKS
        .iter()
        .map(|h| h.rsplit("::").next().unwrap_or(h).to_string())
        .collect();
    assert_eq!(
        setters, listed,
        "engine hook setters and amb_presets::ai_free::GATED_HOOKS differ: gate the new hook at call time and list it"
    );
    for (file, at_least) in [
        ("subject.rs", 4),
        ("retouch/inpaint.rs", 1),
        ("raw_denoise.rs", 2),
    ] {
        let n: usize = engine_files
            .iter()
            .filter(|(f, _)| f.ends_with(file))
            .map(|(_, t)| t.matches("ai_free::").count())
            .sum();
        assert!(
            n >= at_least,
            "amb-engine/src/{file} should check AI-free mode at least {at_least} times, has {n}"
        );
    }

    // 2. Model loading.
    for (file, text) in &sources {
        if !file.contains("/amb-ai/src/")
            && (text.contains("model_for_path(") || text.contains("tract_onnx::onnx()"))
        {
            panic!(
                "{file} loads a model outside amb-ai; models go through amb-ai (and its ai_free checks)"
            );
        }
        if !file.contains("/amb-gpu/src/") && !file.contains("/amb-ai/src/net/") {
            assert!(
                !text.contains(".nn()"),
                "{file} opens the GPU's neural-network session; only amb-ai's net may"
            );
        }
    }
    // (file, model load sites): the whole list.
    let sites = [
        ("model.rs", 1),
        ("remove.rs", 1),
        ("objects.rs", 1),
        ("faces.rs", 1),
        ("denoise.rs", 1),
        ("search.rs", 1),
    ];
    let loads = |text: &str| text.matches("model_for_path(").count();
    let ai_files = whole("amb-ai");
    let total: usize = ai_files.iter().map(|(_, t)| loads(t)).sum();
    assert_eq!(
        total,
        sites.iter().map(|(_, n)| n).sum::<usize>(),
        "amb-ai loads models somewhere not in this table: add the site (and its ai_free::check) here"
    );
    for (file, n) in sites {
        assert_eq!(uses(&ai_files, file, "model_for_path("), n, "{file}");
    }
    for (file, text) in &ai_files {
        let gates = text.matches("ai_free::check").count();
        let paths = loads(text) + text.matches("ureq::").count();
        assert!(
            gates >= paths,
            "{file}: {paths} model load or download sites but {gates} ai_free::check calls"
        );
    }
    // The single runner and loader.
    assert!(
        uses(&ai_files, "net/mod.rs", "ai_free::check") >= 2,
        "amb-ai's net::build and Net::run must both check AI-free mode"
    );
}
