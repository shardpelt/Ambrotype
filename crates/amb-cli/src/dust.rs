// SPDX-License-Identifier: GPL-3.0-or-later
//! `amb dust`: dust and hairs on a film scan, as heal spots.

use std::collections::BTreeMap;
use std::path::Path;

use crate::edit::edit_target;

/// Numbers separated by commas: `X0,Y0,X1,Y1`.
fn numbers(text: &str, usage: &str) -> Result<Vec<f64>, String> {
    text.split(',')
        .map(|x| x.trim().parse::<f64>())
        .collect::<Result<_, _>>()
        .map_err(|_| usage.to_string())
}

/// Find dust on the photo's raw; with `--line` a straight scratch along
/// it, with `--area` or `--along` faint dust in a box or along a stroke;
/// with `--apply`, add a heal spot for each as one undo step.
pub(crate) fn dust(
    photo: &Path,
    options: &BTreeMap<&str, &str>,
    json: bool,
) -> Result<String, String> {
    let apply = options.contains_key("--apply");
    let (found, label) = if let Some(line) = options.get("--line") {
        let usage = "--line takes X0,Y0,X1,Y1 (fractions of the photo)";
        let [x0, y0, x1, y1] = numbers(line, usage)?[..] else {
            return Err(usage.into());
        };
        let commands = amb_ops::dust::line_file(photo, [x0, y0], [x1, y1])?;
        let found = amb_ops::dust::Found {
            hairs: commands.len(),
            commands,
            ..Default::default()
        };
        (found, "Scratch")
    } else if let Some(area) = options.get("--area") {
        let usage = "--area takes X0,Y0,X1,Y1 (fractions of the photo)";
        let [x0, y0, x1, y1] = numbers(area, usage)?[..] else {
            return Err(usage.into());
        };
        (
            amb_ops::dust::area_file(photo, [x0, y0, x1, y1])?,
            "Dust in an area",
        )
    } else if let Some(along) = options.get("--along") {
        let usage = "--along takes X,Y;X,Y;… (fractions of the photo) and --radius R (a fraction of the long side)";
        let stroke: Vec<[f64; 2]> = along
            .split(';')
            .map(|p| match numbers(p, usage)?[..] {
                [x, y] => Ok([x, y]),
                _ => Err(usage.to_string()),
            })
            .collect::<Result<_, _>>()?;
        let radius = options
            .get("--radius")
            .and_then(|r| r.parse::<f64>().ok())
            .filter(|r| *r > 0.0)
            .ok_or(usage)?;
        (
            amb_ops::dust::along_file(photo, &stroke, radius)?,
            "Dust along a stroke",
        )
    } else {
        (amb_ops::dust::find_file(photo)?, "Dust and hairs")
    };
    let sidecar = edit_target(photo);
    let created = !sidecar.exists();
    let mut state = amb_protocol::DevelopState::load_or_new(&sidecar).map_err(|e| e.to_string())?;
    let results = if found.commands.is_empty() {
        Vec::new()
    } else {
        state
            .execute_grouped(label, &found.commands)
            .map_err(|e| e.to_string())?
    };
    let written = if apply && !found.commands.is_empty() {
        state.save(&sidecar).map_err(|e| e.to_string())?;
        Some(sidecar.clone())
    } else {
        None
    };
    if json {
        return serde_json::to_string_pretty(&serde_json::json!({
            "specks": found.specks,
            "hairs": found.hairs,
            "covered": found.covered,
            "spots": found.commands,
            "written": written,
        }))
        .map_err(|e| e.to_string());
    }
    let report = amb_protocol::edit::EditReport {
        sidecar,
        created,
        results,
        written,
        dry_run: !apply,
    };
    let mut text = if options.contains_key("--line") {
        format!("a scratch to heal, in {} piece(s)\n", found.hairs)
    } else {
        format!(
            "{} speck(s) and {} hair(s) to heal{}\n",
            found.specks,
            found.hairs,
            if found.covered > 0 {
                format!("; {} already under a spot", found.covered)
            } else {
                String::new()
            }
        )
    };
    if !found.commands.is_empty() {
        text += &report
            .to_brief()
            .replace("dry run, not saved", "not applied (add --apply)");
    }
    Ok(text)
}
