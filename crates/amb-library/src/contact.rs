// SPDX-License-Identifier: GPL-3.0-or-later
//! Contact sheets of photo files (`amb contact`, the library's Contact
//! sheet…): each photo developed small with its own edit, laid out by
//! [`amb_export::contact`], page by page.

use std::path::{Path, PathBuf};

use amb_export::contact::Sheet;
use rayon::prelude::*;

/// A photo developed small with its own edit (crop included; no lens or
/// camera profiles, which a contact sheet doesn't need), upright.
pub fn small(photo: &Path, long_edge: usize) -> Result<amb_engine::buffer::Image, String> {
    let source = amb_engine::source_path(photo);
    let img = amb_engine::raw::load(&source).map_err(|e| format!("{}: {e}", source.display()))?;
    let img = amb_engine::raw::downscale(&img, long_edge);
    let develop = amb_engine::matching::develop_plain(&crate::look::photo_settings(photo)?);
    let developed = amb_engine::pipeline::render(&img, &develop);
    Ok(amb_export::geometry::orient(
        &developed,
        img.metadata.orientation,
    ))
}

/// Write the sheets for `photos` to `out` (`name-001.jpg`, … when there is
/// more than one page) with `export`; the pages written.
pub fn write(
    photos: &[PathBuf],
    sheet: &Sheet,
    export: &amb_export::ExportSettings,
    out: &Path,
) -> Result<Vec<PathBuf>, String> {
    if photos.is_empty() {
        return Err("no photos for the contact sheet".into());
    }
    let per = sheet.per_page().max(1);
    let pages = photos.len().div_ceil(per);
    // Developed at about twice the frame's size, for a sharp print.
    let edge = (sheet.width / sheet.columns.max(1) * 2).clamp(256, 2048);
    let stem = out.file_stem().unwrap_or_default().to_string_lossy();
    let ext = out.extension().unwrap_or_default().to_string_lossy();
    let mut written = Vec::new();
    for (p, chunk) in photos.chunks(per).enumerate() {
        let developed: Vec<_> = chunk.par_iter().map(|path| small(path, edge)).collect();
        let mut page = sheet.page(p + 1, pages, chunk.len());
        for (i, (path, img)) in chunk.iter().zip(developed).enumerate() {
            let label = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            page.place(i, &img?, &label, p * per + i + 1);
        }
        let path = if pages == 1 {
            out.to_path_buf()
        } else {
            out.with_file_name(format!("{stem}-{:03}.{ext}", p + 1))
        };
        amb_export::export(&page.image, &Default::default(), export, &path)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        written.push(path);
    }
    Ok(written)
}
