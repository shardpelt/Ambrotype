// SPDX-License-Identifier: GPL-3.0-or-later
//! `amb contact`: contact sheets, the photos small on pages to print.

use std::collections::BTreeMap;
use std::path::Path;

use amb_export::contact::{Sheet, Style, paper};

use crate::render::export_settings;

/// Contact sheets of the photos named (folders: the photos in them) at `out`.
pub(crate) fn contact(
    args: &[&str],
    out: &Path,
    options: &BTreeMap<&str, &str>,
) -> Result<String, String> {
    let photos = crate::organise::photos(args)?;
    let mut sheet = Sheet::default();
    if let Some(s) = options.get("--style") {
        sheet.style =
            Style::from_name(s).ok_or_else(|| format!("bad --style {s:?} (film, plain)"))?;
        if sheet.style == Style::Plain {
            sheet.columns = 5;
        }
    }
    if let Some(c) = options.get("--columns") {
        sheet.columns = c
            .parse()
            .ok()
            .filter(|c| (1..=20).contains(c))
            .ok_or_else(|| format!("bad --columns {c:?} (1 to 20)"))?;
    }
    if let Some(p) = options.get("--paper") {
        (sheet.width, sheet.height) =
            paper(p).ok_or_else(|| format!("bad --paper {p:?} (a4, a3, letter, 8x10, 11x14)"))?;
    }
    if options.contains_key("--landscape") {
        (sheet.width, sheet.height) = (sheet.height, sheet.width);
    }
    sheet.title = options
        .get("--title")
        .map(|t| t.to_string())
        .unwrap_or_default();
    let mut export = export_settings(out, options)?;
    export.resize = amb_export::Resize::None;
    let pages = amb_library::contact::write(&photos, &sheet, &export, out)?;
    Ok(pages
        .iter()
        .map(|p| format!("wrote {}", p.display()))
        .collect::<Vec<_>>()
        .join("\n")
        + &format!(
            "\n{} photo(s), {} a page ({} × {} px, 300 dpi)",
            photos.len(),
            sheet.per_page(),
            sheet.width,
            sheet.height
        ))
}
