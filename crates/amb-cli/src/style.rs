// SPDX-License-Identifier: GPL-3.0-or-later
//! `amb style`: saved looks that fit themselves to each photo
//! (`amb_library::styles`, `amb_ops::style`).

use amb_engine::matching::Focus;
use amb_library::styles;
use amb_ops::style::{self as apply_style, Options as Apply};
use std::path::{Path, PathBuf};

use crate::organise::{Options, photos, photos_deep};
use crate::print_json;

const USAGE: &str =
    "style list | save <NAME> <REF>... | apply <STYLE> <PHOTO|DIR>... | delete <NAME>";

fn styles_dir() -> Result<PathBuf, String> {
    styles::editor_dir().ok_or_else(|| "no config folder (set HOME)".into())
}

/// `--amount` in percent as a fraction.
fn amount(options: &Options) -> Result<f64, String> {
    match options.get("--amount") {
        Some(a) => a
            .parse::<f64>()
            .ok()
            .filter(|a| (0.0..=150.0).contains(a))
            .map(|a| a / 100.0)
            .ok_or_else(|| format!("--amount {a}: 0 to 150")),
        None => Ok(1.0),
    }
}

pub(crate) fn style(args: &[&str], options: &Options, json: bool) -> Result<(), String> {
    let dir = styles_dir()?;
    match args {
        ["list"] | [] => list(&dir, json),
        ["save", name, refs @ ..] if !refs.is_empty() => save(&dir, name, refs, options, json),
        ["apply", style, inputs @ ..] if !inputs.is_empty() => {
            apply(&dir, style, inputs, options, json)
        }
        ["delete", name] => {
            styles::delete(&dir, name)?;
            println!("Deleted the style “{name}”.");
            Ok(())
        }
        _ => Err(USAGE.into()),
    }
}

fn list(dir: &Path, json: bool) -> Result<(), String> {
    let all = styles::list(dir);
    if json {
        let items: Vec<_> = all
            .iter()
            .map(|(path, s)| {
                serde_json::json!({
                    "name": s.name, "focus": s.focus, "references": s.references, "path": path,
                })
            })
            .collect();
        return print_json(&serde_json::Value::Array(items));
    }
    if all.is_empty() {
        println!("No styles yet. `amb style save NAME PHOTO...` makes one from photos you like.");
    }
    for (_, s) in &all {
        let focus = serde_json::to_value(s.focus)
            .ok()
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_default();
        println!("{}  ({focus}; from {})", s.name, s.references.join(", "));
    }
    Ok(())
}

fn save(
    dir: &Path,
    name: &str,
    refs: &[&str],
    options: &Options,
    json: bool,
) -> Result<(), String> {
    let focus = match options.get("--focus") {
        Some(f) => Focus::parse(f).ok_or_else(|| format!("--focus {f}: all, tone or colour"))?,
        None => Focus::All,
    };
    let paths: Vec<&Path> = refs.iter().map(Path::new).collect();
    let style = styles::from_photos(name, &paths, focus)?;
    let path = styles::save(dir, &style)?;
    if json {
        return print_json(&serde_json::json!({ "name": style.name, "path": path }));
    }
    println!("Saved the style “{}” to {}.", style.name, path.display());
    Ok(())
}

fn apply(
    dir: &Path,
    name: &str,
    inputs: &[&str],
    options: &Options,
    json: bool,
) -> Result<(), String> {
    let (_, style) = styles::find(dir, name)?;
    let amount = amount(options)?;
    let list = if options.contains_key("--recursive") {
        photos_deep(inputs)?
    } else {
        photos(inputs)?
    };
    let line = crate::progress::line(json, |done, total| format!("Styling… {done} of {total}"));
    let report = apply_style::apply(
        &Apply {
            photos: &list,
            style: &style,
            amount,
        },
        &line,
    );
    if json {
        return print_json(&serde_json::to_value(&report).map_err(|e| e.to_string())?);
    }
    println!(
        "Styled {} photo{} with “{}”; {} already looked like it.",
        report.changed.len(),
        if report.changed.len() == 1 { "" } else { "s" },
        style.name,
        report.unchanged.len()
    );
    for e in &report.errors {
        eprintln!("{e}");
    }
    if report.errors.is_empty() {
        Ok(())
    } else {
        Err(format!("{} could not be styled", report.errors.len()))
    }
}
