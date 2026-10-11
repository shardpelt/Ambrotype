// SPDX-License-Identifier: GPL-3.0-or-later
//! The `amb edit` / `amb query` workflows, kept here so they are testable
//! without spawning the binary.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::convert::display;
use crate::{DevelopState, Error, Outcome, parse_commands, parse_queries};

/// Inputs of `amb edit <sidecar> --commands <file> [--out <file>] [--dry-run]`.
#[derive(Debug, Clone)]
pub struct EditRequest<'a> {
    /// Sidecar to read. If it does not exist, editing starts from an empty
    /// state and the file is created.
    pub sidecar: &'a Path,
    /// JSON text: one command object or an array of commands ([`run`]), or
    /// a [`crate::compact`] line ([`run_compact`]).
    pub commands: &'a str,
    /// Where to write (default: the sidecar itself). `.xmp` or `.json`.
    pub out: Option<&'a Path>,
    /// Report the diff without writing anything.
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct EditReport {
    pub sidecar: PathBuf,
    /// True when the sidecar did not exist and editing started from nothing.
    pub created: bool,
    pub results: Vec<Outcome>,
    /// The file written, if any.
    pub written: Option<PathBuf>,
    pub dry_run: bool,
}

/// Apply the commands as one transaction and write the result. Nothing is
/// written if any command fails.
pub fn run(req: &EditRequest) -> Result<EditReport, Error> {
    apply(req, &parse_commands(req.commands)?)
}

/// [`run`] with the commands as a compact line (`exp=+0.5 con+=10`).
pub fn run_compact(req: &EditRequest) -> Result<EditReport, Error> {
    apply(req, &crate::compact::parse(req.commands)?)
}

fn apply(req: &EditRequest, commands: &[crate::Command]) -> Result<EditReport, Error> {
    let created = !req.sidecar.exists();
    let mut state = if created {
        DevelopState::new()
    } else {
        DevelopState::load(req.sidecar)?
    };
    let results = state.execute_all(commands)?;
    let dest = req.out.unwrap_or(req.sidecar);
    let changed = results.iter().any(|o| !o.diff.is_empty());
    let needs_write = changed || created || req.out.is_some_and(|o| o != req.sidecar);
    let written = if req.dry_run || !needs_write {
        None
    } else {
        // Writing an XMP sidecar to another .xmp: the photo's own non-crs
        // metadata (rating, keywords, history) must come along, rather than
        // being dropped (new file) or taken from an unrelated existing file.
        let base = if !created && dest != req.sidecar && is_xmp(dest) && is_xmp(req.sidecar) {
            Some(
                std::fs::read_to_string(req.sidecar)
                    .map_err(|e| Error::Io(format!("{}: {e}", req.sidecar.display())))?,
            )
        } else {
            None
        };
        state.save_merged(dest, base.as_deref())?;
        Some(dest.to_path_buf())
    };
    Ok(EditReport {
        sidecar: req.sidecar.to_path_buf(),
        created,
        results,
        written,
        dry_run: req.dry_run,
    })
}

/// A value for [`EditReport::to_brief`]: an unset number shows its default.
fn brief(name: &str, v: Option<&amb_presets::Value>) -> String {
    let default = amb_presets::registry::Registry::builtin().numeric_default(name);
    match (v, default) {
        (None, Some(d)) => display(name, Some(&amb_presets::Value::Number(d))),
        _ => display(name, v),
    }
}

fn is_xmp(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("xmp"))
}

impl EditReport {
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or_default()
    }

    /// Human-readable diff, one block per command.
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        if self.created {
            let _ = writeln!(
                out,
                "note: {} did not exist; starting from an empty sidecar",
                self.sidecar.display()
            );
        }
        let mut total = 0;
        for o in &self.results {
            let _ = writeln!(out, "{}", o.command);
            for c in &o.diff {
                let _ = writeln!(
                    out,
                    "  {}: {} -> {}",
                    c.name,
                    display(&c.name, c.before.as_ref()),
                    display(&c.name, c.after.as_ref())
                );
            }
            total += o.diff.len();
            for c in &o.clamped {
                let _ = writeln!(out, "  ! clamped: {c}");
            }
            for n in &o.notes {
                let _ = writeln!(out, "  note: {n}");
            }
        }
        match (&self.written, self.dry_run) {
            (_, true) => {
                let _ = writeln!(out, "dry run: {total} change(s), nothing written");
            }
            (Some(path), _) => {
                let _ = writeln!(out, "wrote {} ({total} change(s))", path.display());
            }
            (None, _) => {
                let _ = writeln!(out, "no changes; nothing written");
            }
        }
        out
    }
}

impl EditReport {
    /// The net change per setting over all commands, one line each
    /// (`Exposure2012 0→+0.5`), then clamps, notes and what was written:
    /// the fewest words that say what happened.
    pub fn to_brief(&self) -> String {
        let mut net: Vec<(
            &str,
            Option<&amb_presets::Value>,
            Option<&amb_presets::Value>,
        )> = Vec::new();
        for c in self.results.iter().flat_map(|o| &o.diff) {
            match net.iter_mut().find(|(n, _, _)| *n == c.name) {
                Some(entry) => entry.2 = c.after.as_ref(),
                None => net.push((&c.name, c.before.as_ref(), c.after.as_ref())),
            }
        }
        let mut out = String::new();
        let mut changed = 0;
        for (name, before, after) in net {
            if before != after {
                changed += 1;
                let _ = writeln!(out, "{name} {}→{}", brief(name, before), brief(name, after));
            }
        }
        for o in &self.results {
            for c in &o.clamped {
                let _ = writeln!(out, "! {c}");
            }
            for n in &o.notes {
                let _ = writeln!(out, "note: {n}");
            }
        }
        if changed == 0 {
            let _ = writeln!(out, "no change");
        }
        if self.dry_run {
            let _ = writeln!(out, "dry run, not saved");
        } else if let Some(path) = &self.written {
            let name = path.file_name().map_or(path.as_os_str(), |n| n);
            let _ = writeln!(out, "saved {}", name.to_string_lossy());
        }
        out.trim_end().to_string()
    }
}

/// `amb query <state> --queries <json>`: answer each query against the state
/// loaded from `path` (an empty state if it does not exist).
pub fn run_queries(path: &Path, queries: &str) -> Result<serde_json::Value, Error> {
    let queries = parse_queries(queries)?;
    let state = if path.exists() {
        DevelopState::load(path)?
    } else {
        DevelopState::new()
    };
    let answers = queries
        .iter()
        .enumerate()
        .map(|(i, q)| {
            state.query(q).map_err(|e| Error::InCommand {
                index: i,
                label: serde_json::to_value(q)
                    .ok()
                    .and_then(|v| v["query"].as_str().map(String::from))
                    .unwrap_or_default(),
                source: Box::new(e),
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(serde_json::Value::Array(answers))
}
