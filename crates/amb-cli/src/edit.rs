// SPDX-License-Identifier: GPL-3.0-or-later
//! `amb edit` and `amb do`: undoable edits to a photo's sidecar.

use crate::*;

/// Read a JSON argument file, or stdin for `-`.
pub(crate) fn read_input(path: &str) -> Result<String, String> {
    if path == "-" {
        let mut text = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut text)
            .map_err(|e| format!("stdin: {e}"))?;
        Ok(text)
    } else {
        std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))
    }
}

/// `amb edit`: returns what to print, so it can be tested without a process.
/// How an edit's report is printed.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Output {
    Text,
    /// Net changes only, one line each.
    Brief,
    Json,
}

pub(crate) fn report_output(
    report: &amb_protocol::edit::EditReport,
    output: Output,
) -> Result<String, String> {
    match output {
        Output::Json => serde_json::to_string_pretty(&report.to_json()).map_err(|e| e.to_string()),
        Output::Brief => Ok(report.to_brief()),
        Output::Text => Ok(report.to_text().trim_end().to_string()),
    }
}

pub(crate) fn edit_output(
    sidecar: &Path,
    commands: &str,
    out: Option<&Path>,
    dry_run: bool,
    output: Output,
) -> Result<String, String> {
    let report = amb_protocol::edit::run(&amb_protocol::edit::EditRequest {
        sidecar,
        commands,
        out,
        dry_run,
    })
    .map_err(|e| e.to_string())?;
    report_output(&report, output)
}

pub(crate) fn edit(
    sidecar: &Path,
    commands: &str,
    out: Option<&Path>,
    dry_run: bool,
    output: Output,
) -> Result<(), String> {
    let commands = read_input(commands)?;
    println!("{}", edit_output(sidecar, &commands, out, dry_run, output)?);
    Ok(())
}

/// The sidecar `amb do` edits: an `.xmp` itself, or the photo's.
pub(crate) fn edit_target(target: &Path) -> PathBuf {
    let is_xmp = target
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("xmp"));
    if is_xmp {
        target.to_path_buf()
    } else {
        amb_engine::sidecar_path(target)
    }
}

/// `amb do`: a compact edit line, with a brief report.
pub(crate) fn do_edits(
    target: &Path,
    edits: &str,
    dry_run: bool,
    json: bool,
) -> Result<String, String> {
    let sidecar = edit_target(target);
    let report = amb_protocol::edit::run_compact(&amb_protocol::edit::EditRequest {
        sidecar: &sidecar,
        commands: edits,
        out: None,
        dry_run,
    })
    .map_err(|e| e.to_string())?;
    report_output(&report, if json { Output::Json } else { Output::Brief })
}
