// SPDX-License-Identifier: GPL-3.0-or-later
//! One open photo: its decoded raw, the develop state (with undo), previews,
//! sidecar saving and export. No Tauri types here, so it is unit-testable;
//! `commands/` exposes it to the web UI as Tauri commands.

mod context;
mod render;
mod retouch;
mod tools;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use amb_engine::pipeline::{self, DevelopSettings};
use amb_engine::raw::{self, CameraImage};
use amb_lens::lcp::LcpLibrary;
use amb_lens::lensfun::LensfunDb;
use amb_presets::Value;
use amb_protocol::{Command, DevelopState, Outcome};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

/// Lens and camera profile sources, loaded once per app run.
#[derive(Default)]
pub struct LensSources {
    pub db: Option<LensfunDb>,
    pub lcp: Option<LcpLibrary>,
    /// The user's camera profiles (`.dcp`), read in place.
    pub dcp: amb_engine::profile::ProfileLibrary,
}

impl LensSources {
    pub fn load_default() -> LensSources {
        let lcp = LcpLibrary::scan_default();
        LensSources {
            db: LensfunDb::load_default(),
            lcp: (!lcp.files.is_empty()).then_some(lcp),
            dcp: amb_engine::profile::ProfileLibrary::load_default(),
        }
    }
}

pub struct Session {
    pub path: PathBuf,
    full: CameraImage,
    /// Downscaled copy for interactive previews, with its long edge.
    preview_src: Option<(u32, CameraImage)>,
    pub state: DevelopState,
    pub sidecar: PathBuf,
    /// Its JPEGs are for the editor's window, so they are converted to the
    /// monitor's profile (`crate::monitor`); off for thumbnails kept on disk
    /// and pictures sent elsewhere.
    pub on_screen: bool,
    /// Soft proofing: the space to show the photo as it exports to, and
    /// whether to mark the colours beyond it.
    proof: Option<(amb_color::space::OutputSpace, bool)>,
    /// XMP text of the sidecar we loaded, so saving keeps its other metadata.
    base_xmp: Option<String>,
    /// Resolved lens profile, keyed by (profile filename hint, auto CA or profile on).
    lens_cache: Option<(Option<String>, Option<amb_lens::Correction>)>,
    /// The Upright correction found for this photo, keyed by the mode and the
    /// lens geometry it was found through (found once, not per preview).
    upright_cache: Option<(String, Option<amb_engine::transform::Keystone>)>,
    /// Why the edit's camera profile isn't used, from the last render.
    profile_note: Option<String>,
    /// The 100% view's renders, tile by tile, for the committed settings.
    region_cache: Option<RegionCache>,
    /// Small copy of the photo the 100% view measures whole-photo
    /// statistics on ([`pipeline::measure_globals`]).
    globals_src: Option<CameraImage>,
    /// A reduced copy for the 100% view's drafts while a slider moves,
    /// with its long edge.
    draft_src: Option<(usize, CameraImage)>,
    /// Rating and keywords, as in the sidecar (they are saved as they change).
    pub info: amb_protocol::sidecar::PhotoInfo,
    /// The develop settings and snapshots as last loaded or saved (for
    /// "unsaved edits").
    saved: BTreeMap<String, Value>,
    saved_snapshots: Vec<amb_protocol::Snapshot>,
    /// Select Object: the object last selected, for its actions.
    pub selection: Option<amb_ops::object::Selection>,
}

/// Full-resolution upright sRGB renders for the 100% view: the parts
/// rendered so far, in tiles of [`REGION_TILE`] pixels, for one set of
/// committed settings.
struct RegionCache {
    key: String,
    srgb: Vec<u8>,
    /// Which tiles hold rendered pixels (row-major).
    done: Vec<bool>,
    globals: pipeline::Globals,
}

/// Tile edge of the 100% view's cache, in pixels.
const REGION_TILE: usize = 256;

/// A 1:1 crop of the full-resolution render, for the 100% view.
#[derive(Debug, Clone)]
pub struct Region {
    pub jpeg: Vec<u8>,
    /// Top-left of the crop in the upright full-size image.
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
    pub full_width: usize,
    pub full_height: usize,
    pub millis: u128,
}

#[derive(Debug, Clone, Serialize)]
pub struct PhotoInfo {
    pub name: String,
    pub path: String,
    pub sidecar: String,
    pub sidecar_loaded: bool,
    pub width: usize,
    pub height: usize,
    pub camera: String,
    pub lens: Option<String>,
    pub exposure: String,
    pub as_shot_temperature: f64,
    pub as_shot_tint: f64,
    /// A rendered image (JPEG, PNG, TIFF, WebP), not a raw.
    pub rendered: bool,
    /// Settings whose default differs from the registry's for this photo.
    pub defaults: BTreeMap<String, f64>,
}

/// A History step's simple changes (numbers, switches, choices; not masks
/// or curves), the first few of them.
#[derive(Debug, Clone, Serialize)]
pub struct StepChanges {
    pub shown: Vec<amb_protocol::state::Change>,
    /// Changes not in `shown`.
    pub more: usize,
}

impl StepChanges {
    const SHOWN: usize = 3;

    fn of(entry: &amb_protocol::state::HistoryEntry) -> StepChanges {
        use amb_presets::Value;
        let simple = |v: &Option<Value>| {
            matches!(
                v,
                None | Some(Value::Number(_) | Value::Bool(_) | Value::Text(_))
            )
        };
        let shown: Vec<_> = entry
            .changes
            .iter()
            .filter(|c| simple(&c.before) && simple(&c.after))
            .take(Self::SHOWN)
            .cloned()
            .collect();
        StepChanges {
            more: entry.changes.len() - shown.len(),
            shown,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct StateInfo {
    pub settings: serde_json::Value,
    pub can_undo: bool,
    pub can_redo: bool,
    /// The steps done, oldest first.
    pub history: Vec<String>,
    /// The steps undone and not yet replaced by a new edit, next to redo first.
    pub redo_history: Vec<String>,
    /// What each step of `history` and then `redo_history` changed, for the
    /// History panel's before → after values.
    pub history_changes: Vec<StepChanges>,
    /// The history read from the sidecar starts after older steps that
    /// weren't kept, not at the photo's original state.
    pub history_trimmed: bool,
    /// The snapshots as `list_snapshots` describes them.
    pub snapshots: serde_json::Value,
    /// Settings present but not rendered by this engine version.
    pub not_rendered: Vec<String>,
    /// The masks as `list_masks` describes them.
    pub masks: serde_json::Value,
    /// The spots as `list_spots` describes them.
    pub spots: serde_json::Value,
}

/// How the mask tool maps the shown photo to mask coordinates: a point at
/// fractions (u, v) of the upright, cropped preview is at
/// `(m[0]·u + m[1]·v + m[2], m[3]·u + m[4]·v + m[5])` as fractions of the
/// uncropped photo in stored orientation (the frame masks use).
#[derive(Debug, Clone, Serialize)]
pub struct MaskFrame {
    pub matrix: [f64; 6],
    /// Uncropped photo, stored orientation, full-size pixels.
    pub frame_width: usize,
    pub frame_height: usize,
    /// The shown (upright, cropped) photo in full-size pixels.
    pub view_width: usize,
    pub view_height: usize,
}

/// The crop as the crop tool works with it: a rectangle on the upright,
/// uncropped photo, in its full-size pixels (`amb_export::geometry::UprightCrop`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CropRect {
    pub cx: f64,
    pub cy: f64,
    pub width: f64,
    pub height: f64,
    /// Straighten, degrees as seen on screen (`CropAngle`, negated for
    /// mirrored orientations).
    pub angle: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CropState {
    /// `HasCrop`; when off, `rect` is the whole photo.
    pub enabled: bool,
    pub rect: CropRect,
    /// `amb_engine::crop::ANGLE_SIGN`: the rectangle's x axis runs at
    /// `angle_sign × angle` degrees over the photo (y down).
    pub angle_sign: f64,
}

/// Crop tags written by the crop tool (and cleared by its reset).
const CROP_TAGS: [&str; 5] = [
    "CropLeft",
    "CropTop",
    "CropRight",
    "CropBottom",
    "CropAngle",
];

#[derive(Debug, Clone)]
pub struct Preview {
    pub jpeg: Vec<u8>,
    pub width: usize,
    pub height: usize,
    /// Per-channel 256-bin histogram of the displayed (sRGB) preview.
    pub histogram: [Vec<u32>; 3],
    pub millis: u128,
    /// Lens profile used, if any.
    pub lens_profile: Option<String>,
    /// The GPU it was developed on, or why not (`None`: no GPU).
    pub gpu: Option<Result<String, String>>,
}

/// What an Upright mode does when the photo has too few lines.
const NO_UPRIGHT: amb_engine::transform::Keystone = amb_engine::transform::Keystone {
    alpha: 0.0,
    beta: 0.0,
    theta: 0.0,
    focal: 1.0,
};

pub use amb_engine::raw::downscale;

/// Where a photo's edits are kept ([`amb_engine::sidecar_path`]).
pub fn sidecar_path(photo: &Path) -> PathBuf {
    amb_engine::sidecar_path(photo)
}

/// Where a photo goes to be edited in another app: `IMG_0001-Edit.tif`
/// next to it, or `-Edit-2.tif`, … when that is taken (Lightroom's names).
/// A virtual copy's goes next to its photo.
pub fn edit_copy_path(photo: &Path) -> PathBuf {
    let source = amb_engine::source_path(photo);
    let stem = source
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    (1..)
        .map(|n| {
            let suffix = if n == 1 {
                String::new()
            } else {
                format!("-{n}")
            };
            source.with_file_name(format!("{stem}-Edit{suffix}.tif"))
        })
        .find(|p| !p.exists())
        .expect("some name is free")
}

fn ratio_text(r: Option<(u32, u32)>, f: impl Fn(f64) -> String) -> Option<String> {
    r.filter(|r| r.1 != 0)
        .map(|(n, d)| f(f64::from(n) / f64::from(d)))
}

impl Session {
    pub fn open(path: &Path) -> Result<Session, String> {
        // A virtual copy is a sidecar; its photo is next to it.
        let source = amb_engine::source_path(path);
        let full = raw::load(&source).map_err(|e| format!("{}: {e}", source.display()))?;
        let sidecar = sidecar_path(path);
        // An empty sidecar file counts as none; a Lightroom catalogue opened
        // in place may know the photo instead.
        let own = amb_engine::sidecar_text(path);
        let (state, base_xmp) = match own {
            Some(text) => {
                let state = DevelopState::from_xmp_str(&text).map_err(|e| {
                    format!(
                        "{} (next to the photo) isn't an XMP file Ambrotype can read: {e}. Move or rename it to open the photo without it.",
                        sidecar.display()
                    )
                })?;
                (state, Some(text))
            }
            // No sidecar yet: a JPEG or TIFF may carry Lightroom's edits
            // inside; metadata there that isn't readable is left alone.
            None => full
                .rendered
                .then(|| std::fs::read(path).ok())
                .flatten()
                .and_then(|b| amb_presets::find_embedded_xmp(&b).map(str::to_string))
                .and_then(|text| Some((DevelopState::from_xmp_str(&text).ok()?, Some(text))))
                .unwrap_or_else(|| (DevelopState::new(), None)),
        };
        let saved = state.settings().clone();
        let saved_snapshots = state.snapshots().to_vec();
        Ok(Session {
            saved,
            saved_snapshots,
            path: path.to_path_buf(),
            full,
            preview_src: None,
            state,
            info: base_xmp
                .as_deref()
                .map(amb_protocol::sidecar::read_info)
                .unwrap_or_default(),
            sidecar,
            on_screen: false,
            proof: None,
            base_xmp,
            lens_cache: None,
            upright_cache: None,
            profile_note: None,
            region_cache: None,
            globals_src: None,
            draft_src: None,
            selection: None,
        })
    }

    /// Drop the 100% view's kept pixels, after something outside the edit
    /// changed how it renders (another AI model chosen, say).
    pub fn forget_renders(&mut self) {
        self.region_cache = None;
    }

    /// Show the photo as it exports to `space` (soft proofing), marking the
    /// colours beyond it when `warn`; `None` shows it as it is.
    pub fn set_proof(&mut self, proof: Option<(amb_color::space::OutputSpace, bool)>) {
        if self.proof != proof {
            self.proof = proof;
            // The 100% view keeps display pixels.
            self.region_cache = None;
        }
    }

    /// A border caption template filled in for this photo, as the export
    /// will set it.
    pub fn caption_text(&self, template: &str) -> String {
        let i = &self.info;
        let tags = amb_export::exif::Tags {
            title: i.title.clone(),
            creator: i.creator.clone(),
            copyright: i.copyright.clone(),
            ..Default::default()
        };
        amb_export::border::caption_text(template, &self.full.metadata, &tags)
    }

    pub fn info(&self) -> PhotoInfo {
        let m = &self.full.metadata;
        let (t, tint) = pipeline::as_shot_temp_tint(&self.full);
        let exposure = [
            ratio_text(m.exposure_time, |v| {
                if v < 1.0 {
                    format!("1/{:.0} s", 1.0 / v)
                } else {
                    format!("{v} s")
                }
            }),
            ratio_text(m.fnumber, |v| format!("f/{v:.1}")),
            m.iso.map(|i| format!("ISO {i}")),
            ratio_text(m.focal_length, |v| format!("{v:.0} mm")),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join("  ·  ");
        let swap = (5..=8).contains(&m.orientation);
        PhotoInfo {
            name: crate::folder::display_name(&self.path),
            path: self.path.display().to_string(),
            sidecar: self.sidecar.display().to_string(),
            sidecar_loaded: self.base_xmp.is_some(),
            width: if swap {
                self.full.height
            } else {
                self.full.width
            },
            height: if swap {
                self.full.width
            } else {
                self.full.height
            },
            camera: format!("{} {}", self.full.make, self.full.model),
            lens: self.full.shot.lens_model.clone(),
            exposure,
            as_shot_temperature: t,
            as_shot_tint: tint,
            rendered: self.full.rendered,
            defaults: if self.full.rendered {
                amb_engine::pipeline::RENDERED_DEFAULTS
                    .iter()
                    .map(|(k, v)| (k.to_string(), *v))
                    .collect()
            } else {
                BTreeMap::new()
            },
        }
    }

    /// The camera profiles on this computer for this photo's camera (none
    /// for rendered images).
    pub fn camera_profiles(&self, lens: &LensSources) -> Vec<amb_engine::profile::Found> {
        if self.full.rendered {
            return Vec::new();
        }
        lens.dcp.for_camera(&self.full.make, &self.full.model)
    }

    pub fn state_info(&self) -> StateInfo {
        let (_, mut not_rendered) = DevelopSettings::from_settings(self.state.settings());
        not_rendered.extend(self.profile_note.clone());
        StateInfo {
            settings: self.state.to_json()["settings"].clone(),
            can_undo: self.state.can_undo(),
            can_redo: self.state.can_redo(),
            history: self
                .state
                .undo_stack()
                .iter()
                .map(|h| h.label.clone())
                .collect(),
            redo_history: self
                .state
                .redo_stack()
                .iter()
                .rev()
                .map(|h| h.label.clone())
                .collect(),
            history_changes: (self.state.undo_stack().iter())
                .chain(self.state.redo_stack().iter().rev())
                .map(StepChanges::of)
                .collect(),
            history_trimmed: self.state.history_trimmed(),
            snapshots: self
                .state
                .query(&amb_protocol::Query::ListSnapshots)
                .map(|q| q["snapshots"].clone())
                .unwrap_or_default(),
            not_rendered,
            masks: amb_protocol::masks::describe(self.state.settings()),
            spots: amb_protocol::spots::describe(self.state.settings()),
        }
    }

    /// Run commands; with a `group` label they become one undo step.
    pub fn execute(
        &mut self,
        cmds: &[Command],
        group: Option<&str>,
    ) -> Result<Vec<Outcome>, String> {
        match group {
            Some(label) => self.state.execute_grouped(label, cmds),
            None => self.state.execute_all(cmds),
        }
        .map_err(|e| e.to_string())
    }

    /// Write the develop settings to the photo's XMP sidecar (keeping the
    /// sidecar's other metadata, like Lightroom's "Save Metadata to File").
    pub fn save_sidecar(&mut self) -> Result<String, String> {
        self.state
            .save_merged(&self.sidecar, self.base_xmp.as_deref())
            .map_err(|e| e.to_string())?;
        self.base_xmp = std::fs::read_to_string(&self.sidecar).ok();
        self.saved = self.state.settings().clone();
        self.saved_snapshots = self.state.snapshots().to_vec();
        Ok(self.sidecar.display().to_string())
    }

    /// Read the sidecar again if something else changed it since we read or
    /// wrote it (another app, or an AI agent over MCP): its develop settings
    /// become one undo step, its rating and keywords are taken as they are.
    /// `None` when the file is as we left it.
    pub fn reload_sidecar(&mut self) -> Result<Option<usize>, String> {
        let Ok(text) = std::fs::read_to_string(&self.sidecar) else {
            return Ok(None);
        };
        if self.base_xmp.as_deref() == Some(text.as_str()) {
            return Ok(None);
        }
        let theirs = DevelopState::from_xmp_str(&text).map_err(|e| e.to_string())?;
        let changes = self
            .state
            .adopt(theirs.settings().clone(), "Changed outside Ambrotype");
        self.info = amb_protocol::sidecar::read_info(&text);
        self.base_xmp = Some(text);
        self.saved = self.state.settings().clone();
        // Turned by another writer (`amb tag --rotate`): shown turned too.
        self.follow_orientation();
        Ok(Some(changes.len()))
    }

    /// The develop settings or snapshots differ from what's in the sidecar.
    pub fn has_unsaved_edits(&self) -> bool {
        self.state.settings() != &self.saved || self.state.snapshots() != self.saved_snapshots
    }

    /// Set the rating and keywords and write them to the sidecar at once.
    /// Only those change on disk: the develop settings in the file stay as
    /// they were (unsaved edits are saved with Save, as before).
    pub fn set_info(
        &mut self,
        info: &amb_protocol::sidecar::PhotoInfo,
    ) -> Result<amb_protocol::sidecar::PhotoInfo, String> {
        use amb_protocol::sidecar;
        let on_disk = std::fs::read_to_string(&self.sidecar).ok();
        let text = sidecar::with_info(on_disk.as_deref(), info).map_err(|e| e.to_string())?;
        sidecar::write_atomic(&self.sidecar, &text).map_err(|e| e.to_string())?;
        self.info = sidecar::read_info(&text);
        self.base_xmp = Some(text);
        self.follow_orientation();
        Ok(self.info.clone())
    }

    /// The sidecar's turn (`info.orientation`), on what is shown and
    /// exported; the pixels don't change.
    fn follow_orientation(&mut self) {
        let Some(o) = self
            .info
            .orientation
            .filter(|&o| o != self.full.metadata.orientation)
        else {
            return;
        };
        self.full.metadata.orientation = o;
        for img in [
            self.preview_src.as_mut().map(|(_, i)| i),
            self.globals_src.as_mut(),
            self.draft_src.as_mut().map(|(_, i)| i),
        ]
        .into_iter()
        .flatten()
        {
            img.metadata.orientation = o;
        }
        self.region_cache = None;
    }

    /// Full-resolution render (on the GPU when there is one) and export.
    pub fn export(
        &mut self,
        out: &Path,
        settings: &amb_export::ExportSettings,
        lens: &LensSources,
    ) -> Result<amb_export::ExportReport, String> {
        let current = self.state.settings().clone();
        let develop = self.develop(&current, lens);
        let (developed, _) = amb_gpu::full::render(amb_gpu::full::shared(), &self.full, &develop);
        amb_export::export(&developed, &self.full.metadata, settings, out)
            .map_err(|e| format!("{}: {e}", out.display()))
    }
}

#[cfg(test)]
mod tests;
