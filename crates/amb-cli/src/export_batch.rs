// SPDX-License-Identifier: GPL-3.0-or-later
//! `amb render` on folders: every photo in them (and, with `--recursive`,
//! their subfolders) exported with its own edit into `--out-dir`, keeping
//! the folder structure. The work is `amb_ops::export_batch`.

use crate::*;
use amb_ops::export_batch::{self, Options};
use std::sync::atomic::AtomicBool;

/// Flags that only the folder form knows.
const BATCH_FLAGS: [&str; 3] = ["--recursive", "--flat", "--skip-existing"];

/// `amb render INPUT... --out-dir DIR`: folders and the batch flags go
/// through the batch export; plain photo lists keep the older per-photo
/// path (`--name` templates, a line per photo).
pub(crate) fn render_dir(
    inputs: &[&str],
    dir: &Path,
    options: &BTreeMap<&str, &str>,
    dry_run: bool,
    json: bool,
) -> Result<(), String> {
    let batch = inputs.iter().any(|i| Path::new(i).is_dir())
        || BATCH_FLAGS.iter().any(|f| options.contains_key(f));
    if !batch {
        return render_many(inputs, dir, options);
    }
    if options.contains_key("--name") {
        return Err("--name works on a list of photos, not on folders".into());
    }
    let batch_options = Options {
        recursive: options.contains_key("--recursive"),
        keep_structure: !options.contains_key("--flat"),
        skip_existing: options.contains_key("--skip-existing"),
        // A folder's photos have no output name to take the format from.
        export: export_settings(Path::new("out.jpg"), options)?,
    };
    let sources: Vec<PathBuf> = inputs.iter().map(PathBuf::from).collect();
    let jobs = export_batch::plan(&sources, dir, &batch_options)?;
    if dry_run {
        if json {
            println!(
                "{}",
                serde_json::to_string_pretty(&jobs).unwrap_or_default()
            );
        } else {
            for j in &jobs {
                println!("{} -> {}", j.source.display(), j.out.display());
            }
            println!("{} photos would be written", jobs.len());
        }
        return Ok(());
    }
    if jobs.is_empty() {
        return Err("no photos found (for the photos in subfolders, add --recursive)".into());
    }
    let render = |photo: &Path, out: &Path, export: &amb_export::ExportSettings| {
        render_quiet(photo, out, export, options)
    };
    let progress = |done: usize, total: usize| {
        if !json {
            eprint!("\r[{done}/{total}]");
        }
    };
    let report = export_batch::run(
        &jobs,
        &batch_options,
        &render,
        &progress,
        &AtomicBool::new(false),
    );
    if !json {
        eprintln!();
    }
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).unwrap_or_default()
        );
    } else {
        println!("{}", summary(&report));
    }
    match report.failed.len() {
        0 => Ok(()),
        n => Err(format!("{n} failed")),
    }
}

/// "312 photos, 4.1 GB → 0.6 GB, 3 skipped, 1 failed: a.nef (why)".
fn summary(r: &export_batch::Report) -> String {
    let size = |b: u64| {
        if b >= 1_000_000_000 {
            format!("{:.1} GB", b as f64 / 1e9)
        } else {
            format!("{:.1} MB", b as f64 / 1e6)
        }
    };
    let mut text = format!(
        "{} photos, {} → {}",
        r.done,
        size(r.bytes_in),
        size(r.bytes_out)
    );
    if r.skipped > 0 {
        text += &format!(", {} skipped", r.skipped);
    }
    if !r.failed.is_empty() {
        let list: Vec<String> = r
            .failed
            .iter()
            .map(|(p, why)| format!("{} ({why})", p.display()))
            .collect();
        text += &format!(", {} failed: {}", r.failed.len(), list.join("; "));
    }
    format!("{text} in {:.0}s", r.seconds)
}
