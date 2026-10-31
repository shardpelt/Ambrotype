// SPDX-License-Identifier: GPL-3.0-or-later
//! `amb convert`: raws to JPEGs in one go (`amb_library::convert`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::render::export_settings;

/// Convert the photos named (folders: the photos in them) into `--to`.
pub(crate) fn convert(
    args: &[&str],
    options: &BTreeMap<&str, &str>,
    json: bool,
) -> Result<String, String> {
    use amb_library::convert::{self, Look};
    let to = PathBuf::from(
        options
            .get("--to")
            .ok_or("convert needs --to FOLDER for the JPEGs")?,
    );
    let look = match options.get("--look") {
        Some(l) => {
            Look::parse(l).ok_or_else(|| format!("bad --look {l:?} (camera, auto, like:PHOTO)"))?
        }
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
        &|n, total| {
            eprint!("\r{n}/{total}");
        },
    );
    eprintln!();
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
    for e in &done.errors {
        out += &format!("\nfailed: {e}");
    }
    Ok(out)
}
