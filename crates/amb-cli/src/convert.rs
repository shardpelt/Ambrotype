// SPDX-License-Identifier: GPL-3.0-or-later
//! `amb convert`: raws to JPEGs in one go (`amb_ops::convert`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::render::export_settings;

/// `--look style:NAME`: the saved style, at `--amount` percent (100 unless said).
fn style_look(
    name: &str,
    options: &BTreeMap<&str, &str>,
) -> Result<amb_ops::convert::Look, String> {
    let dir = amb_library::styles::editor_dir().ok_or("no config folder (set HOME)")?;
    let (_, style) = amb_library::styles::find(&dir, name)?;
    let amount = match options.get("--amount") {
        Some(a) => a
            .parse::<f64>()
            .ok()
            .filter(|a| (0.0..=150.0).contains(a))
            .ok_or_else(|| format!("--amount {a}: 0 to 150"))?,
        None => 100.0,
    };
    Ok(amb_ops::convert::Look::Style(
        Box::new(style),
        amount / 100.0,
    ))
}

/// Convert the photos named (folders: the photos in them) into `--to`.
pub(crate) fn convert(
    args: &[&str],
    options: &BTreeMap<&str, &str>,
    json: bool,
) -> Result<String, String> {
    use amb_ops::convert::{self, Look};
    let to = PathBuf::from(
        options
            .get("--to")
            .ok_or("convert needs --to FOLDER for the JPEGs")?,
    );
    let look = match options.get("--look") {
        Some(l) if l.starts_with("style:") => style_look(&l["style:".len()..], options)?,
        Some(l) => Look::parse(l)
            .ok_or_else(|| format!("bad --look {l:?} (camera, auto, like:PHOTO, style:NAME)"))?,
        None => Look::Camera,
    };
    if let Look::Like(p) = &look {
        if !p.is_file() {
            return Err(format!("--look like:{}: no such photo", p.display()));
        }
    }
    let mut photos = crate::organise::photos(args)?;
    // --best-of-bursts: only the best photo of each burst.
    let mut left_out = 0;
    if options.contains_key("--best-of-bursts") {
        let catalog = crate::organise::open_catalog(options)?;
        (photos, left_out) = convert::best_of_bursts(&catalog, &photos);
    }
    // The format from --format (JPEG unless said), the rest from the export options.
    let name = format!("x.{}", options.get("--format").copied().unwrap_or("jpg"));
    let mut export = export_settings(Path::new(&name), options)?;
    export.metadata = amb_export::MetadataPolicy::All;
    let mut done = convert::convert(
        &photos,
        &look,
        &export,
        &to,
        !options.contains_key("--no-edits"),
        amb_gpu::full::shared(),
        &crate::progress::line(false, |n, total| format!("{n}/{total}")),
    );
    done.left_out = left_out;
    if json {
        return serde_json::to_string_pretty(&done).map_err(|e| e.to_string());
    }
    let mut out = format!(
        "{} written to {}{}{}",
        done.written.len(),
        to.display(),
        if done.camera_jpegs > 0 {
            format!(" ({} as the camera's own JPEG)", done.camera_jpegs)
        } else {
            String::new()
        },
        if done.skipped > 0 {
            format!("; {} already there", done.skipped)
        } else {
            String::new()
        }
    );
    if done.left_out > 0 {
        out += &format!("\n{} left out: not the best of their burst", done.left_out);
    }
    if let Some(Ok(gpu)) = amb_gpu::full::last() {
        out += &format!("\ndeveloped on the GPU ({gpu})");
    }
    for w in &done.warnings {
        out += &format!("\nwarning: {w}");
    }
    for e in &done.errors {
        out += &format!("\nfailed: {e}");
    }
    Ok(out)
}
