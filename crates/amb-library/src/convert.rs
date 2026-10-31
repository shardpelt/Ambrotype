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
//!
//! Photos are developed on the GPU when there is one ([`amb_gpu::full`]),
//! and a few at a time, so one is read while another is developed and a
//! third saved.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use amb_engine::matching::{self, LookStats, MatchOptions};
use amb_engine::pipeline;
use amb_protocol::DevelopState;

use crate::look;

/// The camera's own JPEG is used as it is from this long side up; a smaller
/// one is only a guide to its colours.
pub const CAMERA_JPEG_EDGE: u32 = 2000;

/// Photos converted at once: enough to keep reading, developing and saving
/// busy, few enough for the memory (each full-size photo is a few hundred MB).
pub const AT_ONCE: usize = 3;

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
    /// Where they were developed: "on the GPU (name)" or "on the CPU (why)".
    pub developed: Option<String>,
    /// Photos not converted: not the best of their burst
    /// ([`best_of_bursts`]; set by the caller).
    pub left_out: usize,
}

/// Convert `photos` into `out_dir` with `look`, written with `export`
/// (its format decides the extension), developed on `gpu` when given.
/// `edits`: keep Auto's and Like's settings in the raws' sidecars.
/// `progress` hears how many are done.
pub fn convert(
    photos: &[PathBuf],
    look: &Look,
    export: &amb_export::ExportSettings,
    out_dir: &Path,
    edits: bool,
    gpu: Option<&amb_gpu::Gpu>,
    progress: &(dyn Fn(usize, usize) + Sync),
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
    let job = Job {
        look,
        target: target.as_ref(),
        export,
        edits,
        gpu,
    };
    let ext = export.format.extension();
    let next = AtomicUsize::new(0);
    let finished = AtomicUsize::new(0);
    let results = Mutex::new(vec![None; photos.len()]);
    std::thread::scope(|scope| {
        for _ in 0..AT_ONCE.min(photos.len()) {
            scope.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(photo) = photos.get(i) else { break };
                    let stem = photo.file_stem().unwrap_or_default().to_string_lossy();
                    let out = out_dir.join(format!("{stem}.{ext}"));
                    let result = if out.exists() {
                        Outcome::Skipped
                    } else {
                        match job.one(photo, &out) {
                            Ok(camera_jpeg) => Outcome::Written(out, camera_jpeg),
                            Err(e) => Outcome::Failed(format!("{}: {e}", photo.display())),
                        }
                    };
                    results.lock().expect("results")[i] = Some(result);
                    progress(finished.fetch_add(1, Ordering::Relaxed) + 1, photos.len());
                }
            });
        }
    });
    // In the photos' order, whichever finished first.
    for result in results.into_inner().expect("results").into_iter().flatten() {
        match result {
            Outcome::Skipped => done.skipped += 1,
            Outcome::Written(out, camera_jpeg) => {
                done.camera_jpegs += camera_jpeg as usize;
                done.written.push(out);
            }
            Outcome::Failed(e) => done.errors.push(e),
        }
    }
    done.developed = amb_gpu::full::last_where();
    done
}

#[derive(Clone)]
enum Outcome {
    Skipped,
    /// Written, and whether as the camera's own JPEG.
    Written(PathBuf, bool),
    Failed(String),
}

/// What every photo of a conversion shares.
struct Job<'a> {
    look: &'a Look,
    target: Option<&'a LookStats>,
    export: &'a amb_export::ExportSettings,
    edits: bool,
    gpu: Option<&'a amb_gpu::Gpu>,
}

impl Job<'_> {
    /// Convert one photo to `out`; true when the camera's own JPEG was used.
    fn one(&self, photo: &Path, out: &Path) -> Result<bool, String> {
        let (export, edits) = (self.export, self.edits);
        let developed = |edits: bool, label: &str, solve: &Solver| {
            developed(photo, export, out, edits, label, self.gpu, solve).map(|()| false)
        };
        // A photo the user edited already keeps that edit (Camera, Auto).
        let edited = look::photo_settings(photo).is_ok_and(|s| !s.is_empty());
        match self.look {
            Look::Camera | Look::Auto if edited => developed(false, "", &|_, _, _| no_change()),
            Look::Camera => camera(photo, export, out, edits, self.gpu),
            Look::Auto => developed(edits, "Auto", &|img, base, d| {
                matching::auto(img, base, true, true, d)
            }),
            Look::Like(reference) if reference == photo => {
                developed(false, "", &|_, _, _| no_change())
            }
            Look::Like(_) => {
                let target = self.target.expect("measured before");
                developed(edits, "Match look", &|img, base, d| {
                    matching::solve(img, base, target, &MatchOptions::default(), d)
                })
            }
        }
    }
}

/// `photos` with only the best of each burst (and one of identical files),
/// as the duplicates finder ranks them ([`crate::duplicates`]), in their
/// order; and how many were left out.
pub fn best_of_bursts(
    catalog: &crate::catalog::Catalog,
    photos: &[PathBuf],
) -> (Vec<PathBuf>, usize) {
    let options = crate::duplicates::Options {
        max_bits: 0,
        ..Default::default()
    };
    let thumb = |p: &Path| amb_engine::raw::embedded_thumbnail(p, 256).ok().flatten();
    let groups = crate::duplicates::find(catalog, photos, options, &thumb, &|_, _| {});
    keep_best(photos, &groups)
}

/// `photos` without all but the first (best) of each group.
fn keep_best(photos: &[PathBuf], groups: &[crate::duplicates::Group]) -> (Vec<PathBuf>, usize) {
    let left_out: std::collections::HashSet<&PathBuf> = groups
        .iter()
        .flat_map(|g| g.photos.iter().skip(1))
        .collect();
    let kept: Vec<PathBuf> = (photos.iter())
        .filter(|p| !left_out.contains(p))
        .cloned()
        .collect();
    let n = photos.len() - kept.len();
    (kept, n)
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
    gpu: Option<&amb_gpu::Gpu>,
) -> Result<bool, String> {
    let source = amb_engine::source_path(photo);
    let preview = amb_engine::raw::embedded_preview(&source).map_err(|e| e.to_string())?;
    let Some(preview) = preview else {
        // No JPEG inside: Auto is the nearest thing.
        developed(photo, export, out, edits, "Auto", gpu, &|img, base, d| {
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
            let develop = matching::develop_plain(&Default::default());
            let (rendered, _) = amb_gpu::full::render(gpu, &img, &develop);
            // The raw's date, camera and lens; the JPEG is upright already.
            let (_, mut meta) =
                amb_engine::raw::read_metadata(&source).map_err(|e| e.to_string())?;
            meta.orientation = 1;
            amb_export::export(&rendered, &meta, export, out).map_err(|e| e.to_string())?;
            Ok(true)
        })()
    } else {
        look::reference_look(&tmp).and_then(|target: LookStats| {
            developed(
                photo,
                export,
                out,
                edits,
                "Camera look",
                gpu,
                &|img, base, d| matching::solve(img, base, &target, &MatchOptions::default(), d),
            )
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
/// at full size with them (on `gpu` when given).
fn developed(
    photo: &Path,
    export: &amb_export::ExportSettings,
    out: &Path,
    edits: bool,
    label: &str,
    gpu: Option<&amb_gpu::Gpu>,
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
    let develop = matching::develop_plain(state.settings());
    let (rendered, _) = amb_gpu::full::render(gpu, &full, &develop);
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

    #[test]
    fn only_the_best_of_a_burst_is_kept() {
        use crate::duplicates::{Group, Kind};
        let p = |s: &str| PathBuf::from(s);
        let photos = [p("a"), p("b1"), p("b2"), p("b3"), p("c")];
        let burst = Group {
            kind: Kind::Burst,
            photos: vec![p("b2"), p("b1"), p("b3")],
        };
        let (kept, left_out) = keep_best(&photos, &[burst]);
        assert_eq!(kept, [p("a"), p("b2"), p("c")]);
        assert_eq!(left_out, 2);
    }
}
