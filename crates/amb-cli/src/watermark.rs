// SPDX-License-Identifier: GPL-3.0-or-later
//! `amb watermark`: marks kept by name (`amb_library::watermarks`), and the
//! one place `--watermark` and its `--watermark-*` flags are read.

use amb_export::watermark::{Anchor, Watermark};
use amb_library::watermarks;
use std::path::{Path, PathBuf};

use crate::organise::Options;
use crate::print_json;

const USAGE: &str = "watermark list | show <NAME> | save <NAME> <FROM> | delete <NAME>  (FROM: text, logo.png or mark.json)";

fn marks_dir() -> Result<PathBuf, String> {
    watermarks::editor_dir().ok_or_else(|| "no config folder (set HOME)".into())
}

/// A mark from `--watermark`'s value: a `.json` file, a `.png` logo, a saved
/// watermark's name, or text; the `--watermark-*` flags then apply on top.
pub(crate) fn mark_from(source: &str, options: &Options) -> Result<Watermark, String> {
    let lower = source.to_lowercase();
    let mut mark = if lower.ends_with(".json") {
        let text = std::fs::read_to_string(source).map_err(|e| format!("{source}: {e}"))?;
        serde_json::from_str::<Watermark>(&text).map_err(|e| format!("{source}: {e}"))?
    } else if lower.ends_with(".png") {
        Watermark {
            image: Some(PathBuf::from(source)),
            ..Watermark::text("")
        }
    } else if let Some(saved) = marks_dir()
        .ok()
        .and_then(|dir| watermarks::find(&dir, source).ok())
    {
        saved.1.mark
    } else {
        Watermark::text(source)
    };
    if let Some(a) = options.get("--watermark-at") {
        mark.anchor =
            Anchor::from_name(a).ok_or_else(|| format!("unknown --watermark-at {a:?}"))?;
    }
    let number = |key: &str| -> Result<Option<f64>, String> {
        options
            .get(key)
            .map(|v| {
                v.parse::<f64>()
                    .map_err(|_| format!("{key} {v:?} is not a number"))
            })
            .transpose()
    };
    if let Some(v) = number("--watermark-size")? {
        mark.size = v;
    }
    if let Some(v) = number("--watermark-opacity")? {
        mark.opacity = v;
    }
    if let Some(v) = number("--watermark-rotate")? {
        mark.rotate = v;
    }
    if options.contains_key("--watermark-tile") {
        mark.tile = true;
    }
    mark.validate()?;
    Ok(mark)
}

pub(crate) fn watermark(args: &[&str], options: &Options, json: bool) -> Result<(), String> {
    let dir = marks_dir()?;
    match args {
        ["list"] | [] => list(&dir, json),
        ["show", name] => {
            let (path, _) = watermarks::find(&dir, name)?;
            let text =
                std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            println!("{text}");
            Ok(())
        }
        ["save", name, from] => save(&dir, name, from, options, json),
        ["delete", name] => {
            watermarks::delete(&dir, name)?;
            println!("Deleted the watermark “{name}”.");
            Ok(())
        }
        _ => Err(USAGE.into()),
    }
}

fn list(dir: &Path, json: bool) -> Result<(), String> {
    let all = watermarks::list(dir);
    if json {
        let items: Vec<_> = all
            .iter()
            .map(|(path, s)| serde_json::json!({ "name": s.name, "mark": s.mark, "path": path }))
            .collect();
        return print_json(&serde_json::Value::Array(items));
    }
    if all.is_empty() {
        println!("No watermarks yet. `amb watermark save NAME \"© You\"` keeps one.");
    }
    for (_, s) in &all {
        let what = match &s.mark.image {
            Some(_) => "logo".to_string(),
            None => format!("“{}”", s.mark.text),
        };
        println!("{}  ({what})", s.name);
    }
    Ok(())
}

fn save(dir: &Path, name: &str, from: &str, options: &Options, json: bool) -> Result<(), String> {
    let mark = mark_from(from, options)?;
    let path = watermarks::save(dir, name, &mark)?;
    if json {
        return print_json(&serde_json::json!({ "name": name.trim(), "path": path }));
    }
    println!(
        "Saved the watermark “{}” to {}.",
        name.trim(),
        path.display()
    );
    Ok(())
}
