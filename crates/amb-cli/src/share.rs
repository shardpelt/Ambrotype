// SPDX-License-Identifier: GPL-3.0-or-later
//! `amb share`: a web page with the photo before and after its edit.

use std::collections::BTreeMap;
use std::path::Path;

use crate::render::develop_from;

/// The photo before its edit (framed as the edit is) and after, `--long-edge`
/// pixels (default 2048), in one HTML file at `out`.
pub(crate) fn share(
    photo: &Path,
    out: &Path,
    options: &BTreeMap<&str, &str>,
) -> Result<String, String> {
    let long_edge: u32 = match options.get("--long-edge") {
        Some(v) => v.parse().map_err(|_| format!("bad --long-edge {v:?}"))?,
        None => 2048,
    };
    let source = amb_engine::source_path(photo);
    let mut img =
        amb_engine::raw::load(&source).map_err(|e| format!("{}: {e}", source.display()))?;
    let after = std::fs::read_to_string(amb_engine::sidecar_path(photo))
        .ok()
        .and_then(|t| amb_presets::xmp::parse_crs(&t).ok())
        .unwrap_or_default();
    let before = amb_protocol::catalog::Catalog::builtin().framing(&after);
    let tmp = std::env::temp_dir().join(format!("amb-share-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    let mut settings = amb_export::ExportSettings::new(amb_export::Format::Jpeg);
    settings.quality = 88;
    settings.resize = amb_export::Resize::LongEdge(long_edge);
    settings.metadata = amb_export::MetadataPolicy::None;
    let mut jpegs = Vec::new();
    for (name, s) in [("before", &before), ("after", &after)] {
        let (develop, _, _) = develop_from(&mut img, s, None, None, options)?;
        let developed = amb_engine::pipeline::render(&img, &develop);
        let file = tmp.join(format!("{name}.jpg"));
        let report = amb_export::export(&developed, &img.metadata, &settings, &file)
            .map_err(|e| e.to_string())?;
        jpegs.push((std::fs::read(&file).map_err(|e| e.to_string())?, report));
    }
    let _ = std::fs::remove_dir_all(&tmp);
    let (w, h) = (jpegs[1].1.width, jpegs[1].1.height);
    let title = photo
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Photo".into());
    let html = amb_export::compare::page(&title, &jpegs[0].0, &jpegs[1].0, w, h);
    std::fs::write(out, &html).map_err(|e| format!("{}: {e}", out.display()))?;
    Ok(format!(
        "wrote {} ({w}x{h}, {:.1} MB)",
        out.display(),
        html.len() as f64 / 1e6
    ))
}
