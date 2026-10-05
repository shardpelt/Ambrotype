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
    /// JSON text: one command object or an array of commands.
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
    let commands = parse_commands(req.commands)?;
    let created = !req.sidecar.exists();
    let mut state = if created {
        DevelopState::new()
    } else {
        DevelopState::load(req.sidecar)?
    };
    let results = state.execute_all(&commands)?;
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
