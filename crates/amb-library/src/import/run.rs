// SPDX-License-Identifier: GPL-3.0-or-later
//! Import's run: the safe copy, then keywords, preset and backup copies.

use std::sync::atomic::{AtomicBool, Ordering};

use super::*;

/// What a hidden temporary copy's name ends with: `.IMG_1.NEF.amb-import`.
const TEMP_SUFFIX: &str = ".amb-import";

/// Whether a file name is the temporary copy of an import that was cut off.
pub(super) fn is_leftover(name: &str) -> bool {
    name.starts_with('.') && name.ends_with(TEMP_SUFFIX)
}

/// Delete the temporary copies an earlier, stopped import left in the
/// folders `plan` writes to. How many.
fn clear_leftovers(plan: &[Planned]) -> usize {
    let dirs: HashSet<&Path> = plan
        .iter()
        .flat_map(|p| p.files.iter().filter_map(|(_, to)| to.parent()))
        .collect();
    let mut cleared = 0;
    for dir in dirs {
        for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let leftover = is_leftover(&e.file_name().to_string_lossy())
                && e.metadata().is_ok_and(|m| m.is_file());
            if leftover && std::fs::remove_file(e.path()).is_ok() {
                cleared += 1;
            }
        }
    }
    cleared
}

/// Copy `from` to `to` through a hidden temporary file, keeping the
/// modified time.
pub(crate) fn copy_file(from: &Path, to: &Path) -> Result<(), String> {
    let err = |e: std::io::Error| format!("{}: {e}", to.display());
    if let Some(dir) = to.parent() {
        std::fs::create_dir_all(dir).map_err(err)?;
    }
    if to.exists() {
        return Err(format!("{}: already exists", to.display()));
    }
    let name = to.file_name().unwrap_or_default().to_string_lossy();
    let tmp = to.with_file_name(format!(".{name}{TEMP_SUFFIX}"));
    let copied = std::fs::copy(from, &tmp).map_err(err)?;
    let len = std::fs::metadata(from).map_err(err)?.len();
    if copied != len {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("{}: copied {copied} of {len} bytes", to.display()));
    }
    if let Ok(modified) = std::fs::metadata(from).and_then(|m| m.modified()) {
        if let Ok(f) = std::fs::File::options().write(true).open(&tmp) {
            let _ = f.set_modified(modified);
        }
    }
    std::fs::rename(&tmp, to).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        err(e)
    })
}

/// Add `keywords` to the sidecar of `photo` (making one if needed).
fn add_keywords(photo: &Path, keywords: &[String]) -> Result<(), String> {
    let mut info = crate::info::read_info(photo);
    info.keywords.extend(keywords.iter().cloned());
    crate::info::write_info(photo, &info).map(|_| ())
}

/// Apply `preset` to `photo`'s edit, unless its sidecar already has
/// develop settings (an edit made before import).
fn apply_preset(photo: &Path, preset: &Path) -> Result<bool, String> {
    let path = amb_engine::sidecar_path(photo);
    if let Ok(text) = std::fs::read_to_string(&path) {
        let settings = amb_presets::xmp::parse_crs(&text).unwrap_or_default();
        if settings
            .keys()
            .any(|k| !matches!(k.as_str(), "Version" | "ProcessVersion"))
        {
            return Ok(false);
        }
    }
    let command = serde_json::json!({ "cmd": "apply_preset", "path": preset });
    amb_protocol::edit::run(&amb_protocol::edit::EditRequest {
        sidecar: &path,
        commands: &command.to_string(),
        out: None,
        dry_run: false,
    })
    .map_err(|e| e.to_string())?;
    Ok(true)
}

/// Where today's backup copies go: `<backup>/Imported on YYYY-MM-DD`.
fn backup_folder(backup: &Path) -> PathBuf {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let w = When::from_unix(secs);
    backup.join(format!("Imported on {:04}-{:02}-{:02}", w.y, w.mo, w.d))
}

/// Copy `from` into `dir` under its own name; a file with the same bytes
/// already there is left, another one gets `-2`, `-3`, ….
fn back_up(from: &Path, dir: &Path) -> Result<(), String> {
    let (stem, ext) = split_name(from);
    for n in 1.. {
        let name = if n == 1 {
            format!("{stem}{ext}")
        } else {
            format!("{stem}-{n}{ext}")
        };
        let to = dir.join(name);
        if !to.exists() {
            return copy_file(from, &to);
        }
        if same_file(from, &to) {
            return Ok(());
        }
    }
    unreachable!("some name is free")
}

/// Delete a moved photo's originals, each only if its copy is in place (it
/// was size-checked when copied; a sidecar may have grown since, with
/// keywords or a preset). A file that can't be deleted is reported, not fatal.
fn remove_originals(p: &Planned, name: &str, report: &mut Report) {
    for (from, to) in &p.files {
        if !to.is_file() {
            report.errors.push(format!(
                "{name}: {} left where it was, its copy isn't there",
                from.display()
            ));
        } else if let Err(e) = std::fs::remove_file(from) {
            report
                .errors
                .push(format!("{name}: could not delete {}: {e}", from.display()));
        }
    }
}

/// Carry out a plan. One failed photo doesn't stop the rest. With `stop`
/// set it ends after the photo under way (`stopped` in the report): run
/// the import again and the photos already in place are skipped.
/// `progress(done, total, name)` is called before each photo.
pub fn run(
    plan: &[Planned],
    options: &ImportOptions,
    stop: &AtomicBool,
    progress: &mut dyn FnMut(usize, usize, &str),
) -> Report {
    let mut report = Report {
        cleared: clear_leftovers(plan),
        ..Report::default()
    };
    let keywords: Vec<String> = options
        .keywords
        .iter()
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty())
        .collect();
    let backup = options.backup.as_deref().map(backup_folder);
    if let Some(dir) = &backup
        && let Err(e) = std::fs::create_dir_all(dir)
    {
        report.errors.push(format!("{}: {e}", dir.display()));
        return report;
    }
    for (i, p) in plan.iter().enumerate() {
        if stop.load(Ordering::Relaxed) {
            report.stopped = true;
            break;
        }
        let Some((main_src, main_dest)) = p.files.first() else {
            continue;
        };
        let name = main_src
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        progress(i, plan.len(), &name);
        if p.skip.is_some() {
            report.skipped += 1;
            continue;
        }
        match p
            .files
            .iter()
            .try_for_each(|(from, to)| copy_file(from, to))
        {
            Ok(()) => {
                report.imported += 1;
                if let Some(dir) = main_dest.parent() {
                    if !report.folders.iter().any(|f| f == dir) {
                        report.folders.push(dir.to_path_buf());
                    }
                }
                // The photo is in; a sidecar that won't take keywords is
                // reported but doesn't undo that.
                if let Some(preset) = &options.preset {
                    if let Err(e) = apply_preset(main_dest, preset) {
                        report
                            .errors
                            .push(format!("{name}: preset not applied: {e}"));
                    }
                }
                if !keywords.is_empty() {
                    if let Err(e) = add_keywords(main_dest, &keywords) {
                        report
                            .errors
                            .push(format!("{name}: keywords not added: {e}"));
                    }
                }
                let mut backed_up = true;
                if let Some(dir) = &backup {
                    for (from, _) in &p.files {
                        if let Err(e) = back_up(from, dir) {
                            backed_up = false;
                            report.errors.push(format!("{name}: no backup copy: {e}"));
                        }
                    }
                }
                if options.mode == ImportMode::Move {
                    if backed_up {
                        remove_originals(p, &name, &mut report);
                    } else {
                        report.errors.push(format!(
                            "{name}: not moved, the originals stay where they are"
                        ));
                    }
                }
            }
            Err(e) => report.errors.push(format!("{name}: {e}")),
        }
    }
    if !report.stopped {
        progress(plan.len(), plan.len(), "");
    }
    report
}
