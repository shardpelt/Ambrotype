// SPDX-License-Identifier: GPL-3.0-or-later
//! Raws to JPEGs in one go (`amb convert`, the library's Convert raws…), for
//! a card shot in RAW by mistake, with one of three looks:
//!
//! - **Camera:** what the camera would have saved as JPEG. Most raws keep
//!   that JPEG inside; when it is big enough it is used as it is, else the
//!   raw is developed and matched to it.
//! - **Auto:** each photo's own Auto tone and white balance.
//! - **Like:** each photo matched to the look of one the user edited.
//!
//! Auto and Like write their settings into each raw's sidecar too, so a
//! photo can be changed and exported again. A photo the user edited already
//! keeps that edit with Camera and Auto. A JPEG already there is left
//! alone, so a stopped run carries on where it was.

use std::path::{Path, PathBuf};

use amb_engine::matching::{self, LookStats, MatchOptions};
use amb_engine::pipeline;
use amb_protocol::DevelopState;

use crate::look;

/// The camera's own JPEG is used as it is from this long side up; a smaller
/// one is only a guide to its colours.
pub const CAMERA_JPEG_EDGE: u32 = 2000;

#[derive(Debug, Clone, PartialEq)]
pub enum Look {
    Camera,
    Auto,
    /// Like this photo (its own edit).
    Like(PathBuf),
}

impl Look {
    /// `camera`, `auto` or `like:PHOTO`.
    pub fn parse(text: &str) -> Option<Look> {
        match text {
            "camera" => Some(Look::Camera),
            "auto" => Some(Look::Auto),
            _ => text
                .strip_prefix("like:")
                .map(|p| Look::Like(PathBuf::from(p))),
        }
    }
}

/// What a conversion did.
#[derive(Debug, Default, serde::Serialize)]
pub struct Converted {
    pub written: Vec<PathBuf>,
    /// Photos whose JPEG was there already.
    pub skipped: usize,
    /// Photos given the camera's own JPEG as it is.
    pub camera_jpegs: usize,
    pub errors: Vec<String>,
}

/// Convert `photos` into `out_dir` with `look`, written with `export`
/// (its format decides the extension). `edits`: keep Auto's and Like's
/// settings in the raws' sidecars. `progress` hears how many are done.
pub fn convert(
    photos: &[PathBuf],
    look: &Look,
    export: &amb_export::ExportSettings,
    out_dir: &Path,
    edits: bool,
    progress: &dyn Fn(usize, usize),
) -> Converted {
    let mut done = Converted::default();
    if let Err(e) = std::fs::create_dir_all(out_dir) {
        done.errors.push(format!("{}: {e}", out_dir.display()));
        return done;
    }
    // Like: the reference's look, measured once.
    let target = match look {
        Look::Like(reference) => match look::target_look(&[reference.as_path()]) {
            Ok(t) => Some(t),
            Err(e) => {
                done.errors.push(e);
                return done;
            }
        },
        _ => None,
    };
    let ext = export.format.extension();
    for (i, photo) in photos.iter().enumerate() {
        let stem = photo.file_stem().unwrap_or_default().to_string_lossy();
        let out = out_dir.join(format!("{stem}.{ext}"));
        if out.exists() {
            done.skipped += 1;
        } else {
            // A photo the user edited already keeps that edit (Camera, Auto).
            let edited = look::photo_settings(photo).is_ok_and(|s| !s.is_empty());
            let result = match look {
                Look::Camera | Look::Auto if edited => {
                    developed(photo, export, &out, false, "", &|_, _, _| no_change())
                }
                Look::Camera => camera(photo, export, &out, edits).map(|used| {
                    done.camera_jpegs += used as usize;
                }),
                Look::Auto => developed(photo, export, &out, edits, "Auto", &|img, base, d| {
                    matching::auto(img, base, true, true, d)
                }),
                Look::Like(reference) if reference == photo => {
                    developed(photo, export, &out, false, "", &|_, _, _| no_change())
                }
                Look::Like(_) => {
                    let target = target.as_ref().expect("measured above");
                    developed(photo, export, &out, edits, "Match look", &|img, base, d| {
                        matching::solve(img, base, target, &MatchOptions::default(), d)
                    })
                }
            };
            match result {
                Ok(()) => done.written.push(out),
                Err(e) => done.errors.push(format!("{}: {e}", photo.display())),
            }
        }
        progress(i + 1, photos.len());
    }
    done
}

fn no_change() -> matching::MatchResult {
    matching::MatchResult {
        changes: Vec::new(),
        before: 0.0,
        after: 0.0,
        evaluations: 0,
    }
}

/// The camera's look: its own JPEG when it keeps a big one (true), else the
/// raw developed and matched to the small one (false).
fn camera(
    photo: &Path,
    export: &amb_export::ExportSettings,
    out: &Path,
    edits: bool,
) -> Result<bool, String> {
    let source = amb_engine::source_path(photo);
    let preview = amb_engine::raw::embedded_preview(&source).map_err(|e| e.to_string())?;
    let Some(preview) = preview else {
        // No JPEG inside: Auto is the nearest thing.
        developed(photo, export, out, edits, "Auto", &|img, base, d| {
            matching::auto(img, base, true, true, d)
        })?;
        return Ok(false);
    };
    // Through a file, so it reads like any JPEG (sRGB, as the camera saved it).
    let tmp = std::env::temp_dir().join(format!(
        "amb-convert-{}-{}.jpg",
        std::process::id(),
        photo.file_stem().unwrap_or_default().to_string_lossy()
    ));
    preview
        .save_with_format(&tmp, image::ImageFormat::Jpeg)
        .map_err(|e| e.to_string())?;
    let result = if preview.width().max(preview.height()) >= CAMERA_JPEG_EDGE {
        (|| {
            let img = amb_engine::raw::load(&tmp).map_err(|e| e.to_string())?;
            let rendered = pipeline::render(&img, &matching::develop_plain(&Default::default()));
            // The raw's date, camera and lens; the JPEG is upright already.
            let (_, mut meta) =
                amb_engine::raw::read_metadata(&source).map_err(|e| e.to_string())?;
            meta.orientation = 1;
            amb_export::export(&rendered, &meta, export, out).map_err(|e| e.to_string())?;
            Ok(true)
        })()
    } else {
        look::reference_look(&tmp).and_then(|target: LookStats| {
            developed(photo, export, out, edits, "Camera look", &|img, base, d| {
                matching::solve(img, base, &target, &MatchOptions::default(), d)
            })
            .map(|()| false)
        })
    };
    let _ = std::fs::remove_file(&tmp);
    result
}

type Solver<'a> = dyn Fn(
        &amb_engine::raw::CameraImage,
        &matching::Settings,
        &dyn Fn(&matching::Settings) -> pipeline::DevelopSettings,
    ) -> matching::MatchResult
    + 'a;

/// Solve settings on the photo developed small, keep them in its sidecar
/// (with `edits`, as one undo step called `label`), and export it developed
/// at full size with them.
fn developed(
    photo: &Path,
    export: &amb_export::ExportSettings,
    out: &Path,
    edits: bool,
    label: &str,
    solve: &Solver,
) -> Result<(), String> {
    let source = amb_engine::source_path(photo);
    let full = amb_engine::raw::load(&source).map_err(|e| format!("{}: {e}", source.display()))?;
    let small = amb_engine::raw::downscale(&full, look::EDGE);
    let base = look::photo_settings(photo)?;
    let result = solve(&small, &base, &|s| matching::develop_plain(s));
    let commands = look::commands(&result.changes);
    let sidecar = amb_engine::sidecar_path(photo);
    let mut state = if sidecar.is_file() {
        DevelopState::load(&sidecar).map_err(|e| e.to_string())?
    } else {
        DevelopState::from_settings(base.clone())
    };
    if !commands.is_empty() {
        state
            .execute_grouped(label, &commands)
            .map_err(|e| e.to_string())?;
        if edits {
            state.save(&sidecar).map_err(|e| e.to_string())?;
        }
    }
    let rendered = pipeline::render(&full, &matching::develop_plain(state.settings()));
    amb_export::export(&rendered, &full.metadata, export, out).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn looks_read_from_text() {
        assert_eq!(Look::parse("camera"), Some(Look::Camera));
        assert_eq!(
            Look::parse("like:a.nef"),
            Some(Look::Like(PathBuf::from("a.nef")))
        );
        assert_eq!(Look::parse("sepia"), None);
    }
}
