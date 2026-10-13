// SPDX-License-Identifier: GPL-3.0-or-later
//! One open photo: its decoded raw, the develop state (with undo), previews,
//! sidecar saving and export. No Tauri types here, so it is unit-testable;
//! `main.rs` exposes it to the web UI as commands.

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

#[derive(Debug, Clone, Serialize)]
pub struct StateInfo {
    pub settings: serde_json::Value,
    pub can_undo: bool,
    pub can_redo: bool,
    /// The steps done, oldest first.
    pub history: Vec<String>,
    /// The steps undone and not yet replaced by a new edit, next to redo first.
    pub redo_history: Vec<String>,
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
        // No sidecar yet: a JPEG or TIFF may carry Lightroom's edits inside.
        let text = std::fs::read_to_string(&sidecar).ok().or_else(|| {
            full.rendered
                .then(|| std::fs::read(path).ok())
                .flatten()
                .and_then(|b| amb_presets::find_embedded_xmp(&b).map(str::to_string))
        });
        let (state, base_xmp) = match text {
            Some(text) => (
                DevelopState::from_xmp_str(&text).map_err(|e| e.to_string())?,
                Some(text),
            ),
            None => (DevelopState::new(), None),
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
            proof: None,
            base_xmp,
            lens_cache: None,
            upright_cache: None,
            profile_note: None,
            region_cache: None,
            globals_src: None,
            draft_src: None,
        })
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

    /// The mask tool's coordinate map for the committed crop.
    pub fn mask_frame(&self) -> MaskFrame {
        let (develop, _) = DevelopSettings::from_settings(self.state.settings());
        let (w, h) = (self.full.width, self.full.height);
        let o = self.full.metadata.orientation;
        let g = develop.crop.geometry(w, h);
        let (cw, ch) = g.as_ref().map_or((w, h), |g| (g.width, g.height));
        let to_frame = |u: f64, v: f64| -> (f64, f64) {
            let (s, t) = amb_export::geometry::stored_fraction(o, u, v);
            match &g {
                Some(g) => {
                    let (fx, fy) = g.source(s * cw as f64 - 0.5, t * ch as f64 - 0.5);
                    ((fx + 0.5) / w as f64, (fy + 0.5) / h as f64)
                }
                None => (s, t),
            }
        };
        let o0 = to_frame(0.0, 0.0);
        let ou = to_frame(1.0, 0.0);
        let ov = to_frame(0.0, 1.0);
        let (vw, vh) = amb_export::geometry::oriented_size(cw, ch, o);
        MaskFrame {
            matrix: [
                ou.0 - o0.0,
                ov.0 - o0.0,
                o0.0,
                ou.1 - o0.1,
                ov.1 - o0.1,
                o0.1,
            ],
            frame_width: w,
            frame_height: h,
            view_width: vw,
            view_height: vh,
        }
    }

    /// A donor for a new spot at `(x, y)` with `radius` (fractions; the radius
    /// of the width): the engine's search on a small render of the photo.
    pub fn spot_source(&self, x: f64, y: f64, radius: f64) -> (f64, f64) {
        let (develop, _) = DevelopSettings::from_settings(self.state.settings());
        let src = downscale(&self.full, 1024);
        let out = pipeline::render(&src, &develop);
        let crop = develop.crop.geometry(src.width, src.height);
        amb_engine::retouch::find_source(
            &out,
            (src.width, src.height),
            crop.as_ref(),
            (x, y),
            radius,
        )
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

    /// The committed crop, upright on the uncropped photo.
    pub fn crop_state(&self) -> CropState {
        let (develop, _) = DevelopSettings::from_settings(self.state.settings());
        let r = amb_export::geometry::upright_crop(
            &develop.crop,
            self.full.width,
            self.full.height,
            self.full.metadata.orientation,
        );
        CropState {
            enabled: develop.crop.enabled,
            rect: CropRect {
                cx: r.cx,
                cy: r.cy,
                width: r.width,
                height: r.height,
                angle: r.angle,
            },
            angle_sign: amb_engine::crop::ANGLE_SIGN,
        }
    }

    /// The commands that set the crop to `rect` (the crop tool's Done). The
    /// whole photo with no angle turns the crop off.
    pub fn crop_commands(&self, rect: &CropRect) -> Vec<Command> {
        let (w, h) = (self.full.width, self.full.height);
        let o = self.full.metadata.orientation;
        let (ow, oh) = amb_export::geometry::oriented_size(w, h, o);
        let (ow, oh) = (ow as f64, oh as f64);
        let set = |name: &str, value: serde_json::Value| Command::SetParam {
            name: name.into(),
            value,
        };
        let near = |a: f64, b: f64| (a - b).abs() < 0.5;
        if near(rect.cx, ow / 2.0)
            && near(rect.cy, oh / 2.0)
            && near(rect.width, ow)
            && near(rect.height, oh)
            && rect.angle.abs() < 0.005
        {
            let mut cmds = vec![set("HasCrop", false.into())];
            cmds.extend(
                CROP_TAGS
                    .iter()
                    .map(|t| Command::ResetParam { name: (*t).into() }),
            );
            return cmds;
        }
        let crop = amb_export::geometry::stored_crop(
            &amb_export::geometry::UprightCrop {
                cx: rect.cx,
                cy: rect.cy,
                width: rect.width,
                height: rect.height,
                angle: rect.angle,
            },
            w,
            h,
            o,
        );
        // Six decimals like Lightroom; tiny overshoots from rounding stay in range.
        let frac = |v: f64| ((v * 1e6).round() / 1e6).clamp(0.0, 1.0);
        vec![
            set("HasCrop", true.into()),
            set("CropLeft", frac(crop.left).into()),
            set("CropTop", frac(crop.top).into()),
            set("CropRight", frac(crop.right).into()),
            set("CropBottom", frac(crop.bottom).into()),
            set("CropAngle", ((crop.angle * 1e4).round() / 1e4).into()),
        ]
    }

    /// Develop settings for `settings`, with the lens profile resolved (cached).
    fn develop(
        &mut self,
        settings: &BTreeMap<String, Value>,
        lens: &LensSources,
    ) -> DevelopSettings {
        let (mut develop, _) = DevelopSettings::from_settings(settings);
        if develop.lens.profile_enable || develop.lens.auto_ca {
            let key = develop.lens.profile_filename.clone();
            let cached = self.lens_cache.as_ref().filter(|(k, _)| *k == key);
            let profile = match cached {
                Some((_, p)) => p.clone(),
                None => {
                    let mut shot = self.full.shot.clone();
                    if shot.crop_factor.is_none() {
                        shot.crop_factor = lens
                            .db
                            .as_ref()
                            .and_then(|db| db.camera_crop(&shot.camera_make, &shot.camera_model));
                    }
                    self.full.shot.crop_factor = shot.crop_factor;
                    let hint = amb_lens::ProfileHint {
                        name: develop.lens.profile_name.clone(),
                        filename: key.clone(),
                    };
                    let p = amb_lens::resolve(&shot, &hint, lens.lcp.as_ref(), lens.db.as_ref());
                    self.lens_cache = Some((key, p.clone()));
                    p
                }
            };
            develop.lens.profile = profile;
        }
        if let Some(name) = develop.profile_name.clone() {
            develop.camera_profile = (!self.full.rendered)
                .then(|| lens.dcp.find(&self.full.make, &self.full.model, &name))
                .flatten();
        }
        self.profile_note = develop.profile_note();
        let mode = develop.transform.upright_mode;
        if mode.is_solved() {
            let l = &develop.lens;
            let key = format!(
                "{mode:?} {:?} {} {} {} {} {}",
                l.profile_filename,
                l.profile_enable && l.profile.is_some(),
                l.distortion_scale,
                l.manual_distortion,
                l.constrain_crop,
                l.auto_ca,
            );
            let found = match self.upright_cache.as_ref().filter(|(k, _)| *k == key) {
                Some((_, u)) => *u,
                None => {
                    let cal = &develop.calibration;
                    let u = amb_engine::upright::analyse(
                        &self.full,
                        &develop.lens,
                        &cal.lens,
                        mode,
                        &cal.upright,
                    );
                    self.upright_cache = Some((key, u));
                    u
                }
            };
            // `None` (too few lines) stays uncorrected rather than being
            // searched again on the preview.
            develop.transform.upright = Some(found.unwrap_or(NO_UPRIGHT));
        }
        develop
    }

    /// Render a preview at most `max_edge` pixels on the long side. `overrides`
    /// are applied on top of the state without touching history (slider drags);
    /// `before` renders with no edits.
    pub fn preview(
        &mut self,
        max_edge: usize,
        overrides: &BTreeMap<String, Value>,
        before: bool,
        clipping: bool,
        lens: &LensSources,
    ) -> Preview {
        self.preview_masked(max_edge, overrides, before, clipping, lens, None, None)
    }

    /// [`preview`](Self::preview), with mask `overlay` (its position in
    /// `MaskGroupBasedCorrections`) tinted red as the mask tool shows it.
    #[allow(clippy::too_many_arguments)]
    pub fn preview_masked(
        &mut self,
        max_edge: usize,
        overrides: &BTreeMap<String, Value>,
        before: bool,
        clipping: bool,
        lens: &LensSources,
        overlay: Option<usize>,
        gpu: Option<&amb_gpu::Gpu>,
    ) -> Preview {
        let start = Instant::now();
        let settings = self.effective_settings(overrides, before);
        let mut develop = self.develop(&settings, lens);
        develop.masks.overlay = overlay;
        // `max_edge` is for what is shown: a crop needs a larger source to
        // come out at that size.
        let long = self.full.width.max(self.full.height);
        let crop_long = develop
            .crop
            .geometry(self.full.width, self.full.height)
            .map_or(long, |g| g.width.max(g.height).max(1));
        let max_edge = (max_edge as f64 * long as f64 / crop_long as f64).round() as usize;
        let max_edge = max_edge.clamp(256, 4096);
        if self.preview_src.as_ref().map(|(e, _)| *e as usize) != Some(max_edge) {
            self.preview_src = Some((max_edge as u32, downscale(&self.full, max_edge)));
        }
        let src = &self.preview_src.as_ref().expect("set above").1;
        let lens_profile =
            (develop.lens.for_image(&src.shot).active_profile()).map(|c| c.description.clone());
        // On the GPU when there is one and the edit allows it.
        let plan = gpu.map(|g| (g, amb_engine::plan::plan(src, &develop)));
        let (developed, gpu) = match plan {
            Some((g, Ok(plan))) => (g.render(&plan), Some(Ok(g.name()))),
            Some((_, Err(why))) => (pipeline::render(src, &develop), Some(Err(why))),
            None => (pipeline::render(src, &develop), None),
        };
        let upright = amb_export::geometry::orient(&developed, self.full.metadata.orientation);
        let (mut bytes, gamut) = display(self.proof, &upright);
        let mut histogram = [vec![0u32; 256], vec![0u32; 256], vec![0u32; 256]];
        for p in bytes.chunks(3) {
            for c in 0..3 {
                histogram[c][p[c] as usize] += 1;
            }
        }
        if let Some(out) = &gamut {
            mark_gamut(&mut bytes, out);
        }
        if clipping {
            mark_clipping(&mut bytes);
        }
        Preview {
            jpeg: encode_jpeg(&bytes, upright.width, upright.height, 88),
            width: upright.width,
            height: upright.height,
            histogram,
            millis: start.elapsed().as_millis(),
            lens_profile,
            gpu,
        }
    }

    /// The settings to render: the state (or nothing, for "before") plus
    /// uncommitted slider overrides.
    fn effective_settings(
        &self,
        overrides: &BTreeMap<String, Value>,
        before: bool,
    ) -> BTreeMap<String, Value> {
        if before {
            return BTreeMap::new();
        }
        let mut settings = self.state.settings().clone();
        for (k, v) in overrides {
            settings.insert(k.clone(), v.clone());
        }
        // A Temperature/Tint drag implies a custom white balance.
        if [
            "Temperature",
            "Tint",
            "IncrementalTemperature",
            "IncrementalTint",
        ]
        .iter()
        .any(|k| overrides.contains_key(*k))
        {
            settings.insert("WhiteBalance".into(), Value::Text("Custom".into()));
        }
        settings
    }

    /// A 1:1 crop of the full-resolution render around (cx, cy) (fractions of
    /// the upright image), at most `max_w × max_h` pixels. Only the tiles the
    /// crop needs are rendered ([`pipeline::render_region`]); they are kept
    /// until the committed settings change, so panning back is instant.
    #[allow(clippy::too_many_arguments)]
    pub fn region(
        &mut self,
        cx: f64,
        cy: f64,
        max_w: usize,
        max_h: usize,
        before: bool,
        clipping: bool,
        lens: &LensSources,
    ) -> Region {
        let start = Instant::now();
        let settings = self.effective_settings(&BTreeMap::new(), before);
        let key = serde_json::to_string(&settings).unwrap_or_default();
        let develop = self.develop(&settings, lens);
        let orientation = self.full.metadata.orientation;
        let (sw, sh) = pipeline::output_size(&self.full, &develop);
        let (uw, uh) = amb_export::geometry::oriented_size(sw, sh, orientation);
        if self.region_cache.as_ref().map(|c| &c.key) != Some(&key) {
            let globals = if pipeline::needs_globals(&develop) {
                let full = &self.full;
                let small = self
                    .globals_src
                    .get_or_insert_with(|| raw::downscale(full, pipeline::GLOBALS_LONG_SIDE));
                pipeline::measure_globals(full, small, &develop)
            } else {
                pipeline::Globals::default()
            };
            let tiles = uw.div_ceil(REGION_TILE) * uh.div_ceil(REGION_TILE);
            self.region_cache = Some(RegionCache {
                key,
                srgb: vec![0; uw * uh * 3],
                done: vec![false; tiles],
                globals,
            });
        }
        let proof = self.proof;
        let cache = self.region_cache.as_mut().expect("set above");
        let w = max_w.clamp(16, uw);
        let h = max_h.clamp(16, uh);
        let x = ((cx * uw as f64) as isize - w as isize / 2).clamp(0, (uw - w) as isize) as usize;
        let y = ((cy * uh as f64) as isize - h as isize / 2).clamp(0, (uh - h) as isize) as usize;

        // Render the tiles the crop needs that aren't rendered yet, as one
        // rectangle (the filters' margin is paid once).
        let tiles_x = uw.div_ceil(REGION_TILE);
        let (tx0, tx1) = (x / REGION_TILE, (x + w).div_ceil(REGION_TILE));
        let (ty0, ty1) = (y / REGION_TILE, (y + h).div_ceil(REGION_TILE));
        let missing: Vec<(usize, usize)> = (ty0..ty1)
            .flat_map(|ty| (tx0..tx1).map(move |tx| (tx, ty)))
            .filter(|&(tx, ty)| !cache.done[ty * tiles_x + tx])
            .collect();
        if !missing.is_empty() {
            let (mx0, mx1) = missing.iter().fold((usize::MAX, 0), |(a, b), &(tx, _)| {
                (a.min(tx), b.max(tx + 1))
            });
            let (my0, my1) = missing.iter().fold((usize::MAX, 0), |(a, b), &(_, ty)| {
                (a.min(ty), b.max(ty + 1))
            });
            let (rx, ry) = (mx0 * REGION_TILE, my0 * REGION_TILE);
            let rw = (mx1 * REGION_TILE).min(uw) - rx;
            let rh = (my1 * REGION_TILE).min(uh) - ry;
            // The same rectangle in the stored orientation the engine renders.
            let corner = |ux: usize, uy: usize| {
                amb_export::geometry::stored_position(orientation, sw, sh, ux, uy)
            };
            let (ax, ay) = corner(rx, ry);
            let (bx, by) = corner(rx + rw - 1, ry + rh - 1);
            let stored = amb_engine::buffer::Rect {
                x: ax.min(bx),
                y: ay.min(by),
                width: ax.abs_diff(bx) + 1,
                height: ay.abs_diff(by) + 1,
            };
            let part = pipeline::render_region(&self.full, &develop, stored, &cache.globals);
            let upright = amb_export::geometry::orient(&part, orientation);
            let (mut srgb, gamut) = display(proof, &upright);
            if let Some(out) = &gamut {
                mark_gamut(&mut srgb, out);
            }
            for row in 0..rh {
                let dst = ((ry + row) * uw + rx) * 3;
                cache.srgb[dst..dst + rw * 3]
                    .copy_from_slice(&srgb[row * rw * 3..(row + 1) * rw * 3]);
            }
            for ty in my0..my1 {
                for tx in mx0..mx1 {
                    cache.done[ty * tiles_x + tx] = true;
                }
            }
        }
        let mut crop = Vec::with_capacity(w * h * 3);
        for row in y..y + h {
            let start = (row * uw + x) * 3;
            crop.extend_from_slice(&cache.srgb[start..start + w * 3]);
        }
        if clipping {
            mark_clipping(&mut crop);
        }
        Region {
            jpeg: encode_jpeg(&crop, w, h, 92),
            x,
            y,
            width: w,
            height: h,
            full_width: uw,
            full_height: uh,
            millis: start.elapsed().as_millis(),
        }
    }

    /// The 100% view's crop while a slider moves (`overrides`): rendered
    /// from a copy at a fraction of full size, so it keeps up, and shown
    /// enlarged until the full-size crop replaces it. The position and size
    /// are in full-size pixels, as [`Session::region`] gives them.
    pub fn region_draft(
        &mut self,
        cx: f64,
        cy: f64,
        max_w: usize,
        max_h: usize,
        overrides: &BTreeMap<String, Value>,
        lens: &LensSources,
    ) -> Region {
        let start = Instant::now();
        let settings = self.effective_settings(overrides, false);
        let develop = self.develop(&settings, lens);
        let orientation = self.full.metadata.orientation;
        let (sw, sh) = pipeline::output_size(&self.full, &develop);
        let (uw, uh) = amb_export::geometry::oriented_size(sw, sh, orientation);
        let w = max_w.clamp(16, uw);
        let h = max_h.clamp(16, uh);
        let x = ((cx * uw as f64) as isize - w as isize / 2).clamp(0, (uw - w) as isize) as usize;
        let y = ((cy * uh as f64) as isize - h as isize / 2).clamp(0, (uh - h) as isize) as usize;
        // About half a megapixel of draft, at least half size down.
        let factor = ((w * h) as f64 / 500_000.0).sqrt().max(2.0);
        let long = self.full.width.max(self.full.height);
        let edge = ((long as f64 / factor).round() as usize).max(256);
        if self.draft_src.as_ref().map(|(e, _)| *e) != Some(edge) {
            self.draft_src = Some((edge, downscale(&self.full, edge)));
        }
        let src = &self.draft_src.as_ref().expect("set above").1;
        let (dsw, dsh) = pipeline::output_size(src, &develop);
        let (duw, duh) = amb_export::geometry::oriented_size(dsw, dsh, orientation);
        // The same window on the reduced copy.
        let (fx, fy) = (duw as f64 / uw as f64, duh as f64 / uh as f64);
        let dx = ((x as f64 * fx) as usize).min(duw.saturating_sub(1));
        let dy = ((y as f64 * fy) as usize).min(duh.saturating_sub(1));
        let dw = ((w as f64 * fx).ceil() as usize).clamp(1, duw - dx);
        let dh = ((h as f64 * fy).ceil() as usize).clamp(1, duh - dy);
        // The whole-photo measurements of the full-size crop on screen will
        // do for a draft (measuring them again each step costs more than the
        // draft itself).
        let globals = match &self.region_cache {
            Some(c) => c.globals.clone(),
            None if pipeline::needs_globals(&develop) => {
                let full = &self.full;
                let small = self
                    .globals_src
                    .get_or_insert_with(|| raw::downscale(full, pipeline::GLOBALS_LONG_SIDE));
                pipeline::measure_globals(src, small, &develop)
            }
            None => pipeline::Globals::default(),
        };
        let corner = |ux: usize, uy: usize| {
            amb_export::geometry::stored_position(orientation, dsw, dsh, ux, uy)
        };
        let (ax, ay) = corner(dx, dy);
        let (bx, by) = corner(dx + dw - 1, dy + dh - 1);
        let stored = amb_engine::buffer::Rect {
            x: ax.min(bx),
            y: ay.min(by),
            width: ax.abs_diff(bx) + 1,
            height: ay.abs_diff(by) + 1,
        };
        let part = pipeline::render_region(src, &develop, stored, &globals);
        let upright = amb_export::geometry::orient(&part, orientation);
        Region {
            jpeg: {
                let (mut bytes, gamut) = display(self.proof, &upright);
                if let Some(out) = &gamut {
                    mark_gamut(&mut bytes, out);
                }
                encode_jpeg(&bytes, upright.width, upright.height, 85)
            },
            x,
            y,
            width: w,
            height: h,
            full_width: uw,
            full_height: uh,
            millis: start.elapsed().as_millis(),
        }
    }

    /// White balance that makes the clicked spot neutral: (u, v) are
    /// fractions of the upright, uncropped photo. Returns (Temperature, Tint).
    /// The white balance settings that make the spot at (`u`, `v`) neutral:
    /// Temperature and Tint on a raw, their relative versions on a rendered image.
    pub fn pick_white(&self, u: f64, v: f64) -> Option<[(&'static str, f64); 2]> {
        let img = &self.full;
        let o = img.metadata.orientation;
        let (ow, oh) = amb_export::geometry::oriented_size(img.width, img.height, o);
        let ux = ((u.clamp(0.0, 1.0) * (ow - 1) as f64).round()) as usize;
        let uy = ((v.clamp(0.0, 1.0) * (oh - 1) as f64).round()) as usize;
        let (sx, sy) = amb_export::geometry::stored_position(o, img.width, img.height, ux, uy);
        // Average a small patch to beat noise.
        let r = (img.width.max(img.height) / 600).max(2);
        let mut acc = [0.0f64; 3];
        let mut n = 0.0;
        for y in sy.saturating_sub(r)..(sy + r + 1).min(img.height) {
            for x in sx.saturating_sub(r)..(sx + r + 1).min(img.width) {
                let i = (y * img.width + x) * 3;
                for (a, v) in acc.iter_mut().zip(&img.rgb[i..i + 3]) {
                    *a += f64::from(*v);
                }
                n += 1.0;
            }
        }
        let mean = acc.map(|a| a / n);
        // Clipped or black spots can't define a white.
        if mean[1] <= 1e-4 || mean.iter().any(|c| *c >= 0.98 || *c <= 0.0) {
            return None;
        }
        let neutral = [mean[0] / mean[1], 1.0, mean[2] / mean[1]];
        let (x, y) = img.white_xy(neutral);
        if img.rendered {
            let wb = amb_engine::pipeline::incremental_for_xy(img, x, y);
            return (wb.temperature.is_finite() && wb.tint.is_finite()).then_some([
                (
                    "IncrementalTemperature",
                    wb.temperature.round().clamp(-100.0, 100.0),
                ),
                ("IncrementalTint", wb.tint.round().clamp(-100.0, 100.0)),
            ]);
        }
        let tt = amb_color::xy_to_temp_tint(x, y);
        (tt.temperature.is_finite() && tt.tint.is_finite()).then_some([
            ("Temperature", tt.temperature.round()),
            ("Tint", tt.tint.round()),
        ])
    }

    /// Context for a feedback report: app, system, photo and the edit.
    pub fn feedback_context(&self, include_settings: bool) -> String {
        let info = self.info();
        let state = self.state_info();
        let mut out = format!(
            "- Ambrotype {} on {} ({})\n- Camera: {}\n- Lens: {}\n- Size: {} x {}\n",
            env!("CARGO_PKG_VERSION"),
            std::env::consts::OS,
            std::env::consts::ARCH,
            info.camera,
            info.lens.as_deref().unwrap_or("unknown"),
            info.width,
            info.height
        );
        if !state.not_rendered.is_empty() {
            out.push_str(&format!(
                "- Not rendered yet: {}\n",
                state.not_rendered.join(", ")
            ));
        }
        if include_settings {
            let settings: Vec<String> = self
                .state
                .settings()
                .iter()
                .map(|(k, v)| {
                    let text = serde_json::to_string(v).unwrap_or_default();
                    format!("{k}={}", text.trim_matches('"'))
                })
                .collect();
            out.push_str(&format!("- Settings: {}\n", settings.join("; ")));
        }
        out
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
        Ok(self.info.clone())
    }

    /// What the assistant is told about the photo: what it is, its settings
    /// that differ from the defaults, and its numbers as edited now.
    pub fn assistant_context(&mut self, lens: &LensSources) -> String {
        let info = self.info();
        let settings = self.state.settings().clone();
        let mut set = Vec::new();
        let mut other = Vec::new();
        // Bookkeeping that says nothing about the look.
        let skip = [
            "Version",
            "ProcessVersion",
            "CompatibleVersion",
            "HasSettings",
            "AlreadyApplied",
        ];
        for (name, v) in settings.iter().filter(|(n, _)| !skip.contains(&n.as_str())) {
            match v {
                Value::Seq(_) | Value::Struct(_) => other.push(name.as_str()),
                _ => set.push(format!(
                    "{name}={}",
                    amb_protocol::convert::display(name, Some(v))
                )),
            }
        }
        let small = raw::downscale(&self.full, 512);
        let develop = self.develop(&settings, lens);
        let stats = amb_engine::stats::measure(&pipeline::render(&small, &develop));
        let mut out = format!(
            "{} ({}{}), {} x {}; as shot {:.0} K, tint {:+.0}
",
            info.name,
            info.camera,
            if info.rendered {
                ", a JPEG/TIFF, not a raw"
            } else {
                ""
            },
            info.width,
            info.height,
            info.as_shot_temperature,
            info.as_shot_tint,
        );
        out.push_str(&if set.is_empty() {
            "Settings: all at defaults\n".to_string()
        } else {
            format!("Settings: {}\n", set.join(" "))
        });
        if !other.is_empty() {
            out.push_str(&format!("Also has: {}\n", other.join(", ")));
        }
        out.push_str(&stats.to_brief());
        out
    }

    /// `cmds` with the as-shot white balance set first when they move
    /// Temperature or Tint from As Shot, so `temp+=500` starts from the
    /// camera's value (the protocol can't read it from the raw).
    pub fn white_balance_from_as_shot(&self, cmds: Vec<Command>) -> Vec<Command> {
        let settings = self.state.settings();
        let moves_wb = cmds.iter().any(|c| match c {
            Command::SetParam { name, .. } | Command::AdjustParam { name, .. } => {
                matches!(name.as_str(), "Temperature" | "Tint")
            }
            _ => false,
        });
        if !moves_wb || settings.contains_key("Temperature") || self.full.rendered {
            return cmds;
        }
        let info = self.info();
        let mut start = vec![Command::SetParam {
            name: "Temperature".into(),
            value: serde_json::json!(info.as_shot_temperature.round()),
        }];
        if !settings.contains_key("Tint") {
            start.push(Command::SetParam {
                name: "Tint".into(),
                value: serde_json::json!(info.as_shot_tint.round()),
            });
        }
        start.extend(cmds);
        start
    }

    /// Guided Upright: the commands that make `guides` (each `[u1, v1, u2,
    /// v2]`, fractions of the upright, uncropped photo) vertical or
    /// horizontal with the Transform sliders, keep the guides, and turn
    /// Upright off. Also the largest angle a guide is still off by.
    pub fn guided_upright(&self, guides: &[[f64; 4]]) -> Result<(Vec<Command>, f64), String> {
        use amb_engine::upright::{self, Segment};
        let o = self.full.metadata.orientation;
        let stored: Vec<Segment> = guides
            .iter()
            .map(|g| Segment {
                a: amb_export::geometry::stored_fraction(o, g[0], g[1]),
                b: amb_export::geometry::stored_fraction(o, g[2], g[3]),
            })
            .collect();
        let (develop, _) = DevelopSettings::from_settings(self.state.settings());
        let k = &develop.calibration.transform;
        let found = upright::guided(&stored, self.full.width, self.full.height, k)
            .ok_or("draw a guide along a line that should be vertical or horizontal")?;
        let set = |name: &str, v: f64| {
            if v == 0.0 {
                Command::ResetParam { name: name.into() }
            } else {
                Command::SetParam {
                    name: name.into(),
                    value: serde_json::json!(v),
                }
            }
        };
        let text: Vec<String> = stored
            .iter()
            .map(|s| format!("{:.5},{:.5},{:.5},{:.5}", s.a.0, s.a.1, s.b.0, s.b.1))
            .collect();
        Ok((
            vec![
                Command::ResetParam {
                    name: "PerspectiveUpright".into(),
                },
                set("PerspectiveVertical", found.vertical),
                set("PerspectiveHorizontal", found.horizontal),
                set("PerspectiveRotate", found.rotate),
                Command::SetParam {
                    name: "amb:UprightGuides".into(),
                    value: serde_json::json!(text.join(";")),
                },
            ],
            found.worst_degrees,
        ))
    }

    /// The guides kept by Guided Upright, as fractions of the upright,
    /// uncropped photo.
    pub fn upright_guides(&self) -> Vec<[f64; 4]> {
        let o = self.full.metadata.orientation;
        let Some(Value::Text(t)) = self.state.settings().get("amb:UprightGuides") else {
            return Vec::new();
        };
        t.split(';')
            .filter_map(|g| {
                let v: Vec<f64> = g.split(',').filter_map(|x| x.trim().parse().ok()).collect();
                let [x1, y1, x2, y2] = v[..] else { return None };
                let a = amb_export::geometry::upright_fraction(o, x1, y1);
                let b = amb_export::geometry::upright_fraction(o, x2, y2);
                Some([a.0, a.1, b.0, b.1])
            })
            .collect()
    }

    /// The film negative analysis of this photo (film base, levels, midtone).
    pub fn negative_analysis(&self) -> amb_engine::negative::Analysis {
        let kind = amb_library::negative::film_type(self.state.settings());
        amb_library::negative::analyze_image(&self.full, kind)
    }

    /// The crop to a scan's frame (without rebate or holder edges), as
    /// commands; `None` when no border is found.
    pub fn negative_frame(&self) -> Option<Vec<Command>> {
        let kind = amb_library::negative::film_type(self.state.settings());
        let (l, t, r, b) = amb_engine::negative::detect_frame(
            &self.full.rgb,
            self.full.width,
            self.full.height,
            kind,
            &amb_engine::negative::DEFAULT_STRENGTHS,
        )?;
        let set = |name: &str, v: serde_json::Value| Command::SetParam {
            name: name.into(),
            value: v,
        };
        let round = |v: f64| (v * 10_000.0).round() / 10_000.0;
        Some(vec![
            set("HasCrop", serde_json::json!(true)),
            set("CropLeft", serde_json::json!(round(l))),
            set("CropTop", serde_json::json!(round(t))),
            set("CropRight", serde_json::json!(round(r))),
            set("CropBottom", serde_json::json!(round(b))),
            Command::ResetParam {
                name: "CropAngle".into(),
            },
        ])
    }

    /// Lightroom-style Auto for the photo: tone and/or white balance.
    pub fn auto(
        &mut self,
        tone: bool,
        white: bool,
        lens: &LensSources,
    ) -> amb_engine::matching::MatchResult {
        use amb_engine::matching;
        let small = raw::downscale(&self.full, amb_library::look::EDGE);
        let base = self.state.settings().clone();
        let base_dev = self.develop(&base, lens);
        let develop = |s: &matching::Settings| {
            let mut d = matching::develop_plain(s);
            d.lens.profile = base_dev.lens.profile.clone();
            d.camera_profile = base_dev.camera_profile.clone();
            d.transform.upright = base_dev.transform.upright;
            d
        };
        matching::auto(&small, &base, tone, white, &develop)
    }

    /// The settings that bring the photo's look to `target`, searched on a
    /// small copy, with this photo's lens and camera profiles and Upright.
    pub fn match_look(
        &mut self,
        target: &amb_engine::matching::LookStats,
        parts: Vec<amb_engine::matching::Part>,
        lens: &LensSources,
    ) -> amb_engine::matching::MatchResult {
        use amb_engine::matching;
        let small = raw::downscale(&self.full, amb_library::look::EDGE);
        let base = self.state.settings().clone();
        let base_dev = self.develop(&base, lens);
        let develop = |s: &matching::Settings| {
            let mut d = matching::develop_plain(s);
            d.lens.profile = base_dev.lens.profile.clone();
            d.camera_profile = base_dev.camera_profile.clone();
            d.transform.upright = base_dev.transform.upright;
            d
        };
        let opts = matching::MatchOptions {
            parts,
            ..Default::default()
        };
        matching::solve(&small, &base, target, &opts, &develop)
    }

    /// The photo developed with its current settings, full size and upright
    /// (working space), e.g. for a collage cell.
    pub fn render_upright(&mut self, lens: &LensSources) -> amb_engine::buffer::Image {
        let current = self.state.settings().clone();
        let develop = self.develop(&current, lens);
        let developed = pipeline::render(&self.full, &develop);
        amb_export::geometry::orient(&developed, self.full.metadata.orientation)
    }

    /// What a plugin with these permissions may see of this photo.
    pub fn plugin_context(
        &self,
        permissions: &[amb_plugin::Permission],
        seed: u64,
    ) -> amb_plugin::Context {
        use amb_plugin::Permission;
        let settings = self.state.settings();
        amb_plugin::Context {
            settings: amb_plugin::Context::settings_json(settings),
            photo: permissions
                .contains(&Permission::PhotoInfo)
                .then(|| amb_plugin::photo::info(&self.full)),
            stats: permissions
                .contains(&Permission::PhotoStats)
                .then(|| amb_plugin::photo::stats(&self.full, settings)),
            seed,
        }
    }

    /// Full-resolution render and export.
    pub fn export(
        &mut self,
        out: &Path,
        settings: &amb_export::ExportSettings,
        lens: &LensSources,
    ) -> Result<amb_export::ExportReport, String> {
        let current = self.state.settings().clone();
        let develop = self.develop(&current, lens);
        let developed = pipeline::render(&self.full, &develop);
        amb_export::export(&developed, &self.full.metadata, settings, out)
            .map_err(|e| format!("{}: {e}", out.display()))
    }
}

fn to_srgb8(img: &amb_engine::buffer::Image) -> Vec<u8> {
    amb_engine::output::to_srgb(img)
        .par_iter()
        .map(|v| (v * 255.0).round().clamp(0.0, 255.0) as u8)
        .collect()
}

/// A render (linear ProPhoto) as display sRGB bytes, soft-proofed when
/// `proof` is on, and the pixels to mark as beyond the proof's gamut.
fn display(
    proof: Option<(amb_color::space::OutputSpace, bool)>,
    img: &amb_engine::buffer::Image,
) -> (Vec<u8>, Option<Vec<bool>>) {
    match proof {
        None => (to_srgb8(img), None),
        Some((space, warn)) => {
            let mut proofed = img.clone();
            let out = space.soft_proof(&mut proofed.rgb);
            (to_srgb8(&proofed), warn.then_some(out))
        }
    }
}

/// Soft proofing's gamut warning: colours the proof's space can't hold, in
/// magenta (red and blue are the clipping warning's).
fn mark_gamut(bytes: &mut [u8], out: &[bool]) {
    bytes
        .par_chunks_mut(3)
        .zip(out.par_iter())
        .filter(|(_, o)| **o)
        .for_each(|(p, _)| p.copy_from_slice(&[230, 40, 210]));
}

/// Lightroom-style clipping warning: clipped highlights red, crushed shadows blue.
fn mark_clipping(bytes: &mut [u8]) {
    bytes.par_chunks_mut(3).for_each(|p| {
        if p.iter().any(|c| *c >= 254) {
            p.copy_from_slice(&[255, 40, 40]);
        } else if p.iter().all(|c| *c <= 1) {
            p.copy_from_slice(&[40, 110, 255]);
        }
    });
}

fn encode_jpeg(bytes: &[u8], w: usize, h: usize, quality: u8) -> Vec<u8> {
    let mut jpeg = Vec::new();
    let mut enc = jpeg_encoder::Encoder::new(&mut jpeg, quality);
    enc.set_sampling_factor(jpeg_encoder::SamplingFactor::R_4_2_0);
    let _ = enc.encode(bytes, w as u16, h as u16, jpeg_encoder::ColorType::Rgb);
    jpeg
}

#[derive(Debug, Clone, Serialize)]
pub struct PresetEntry {
    pub name: String,
    pub group: String,
    pub path: String,
}

/// Settings groups a saved preset leaves out: they belong to one photo.
const PHOTO_SPECIFIC_GROUPS: [&str; 6] = [
    "crop",
    "local",
    "retouch",
    "transform",
    "metadata",
    "lens_blur",
];

/// Save `settings` as a Lightroom-style XMP preset `dir/<group>/<name>.xmp`
/// (crop, masks, retouching and geometry left out). Returns its path.
pub fn save_preset(
    dir: &Path,
    name: &str,
    group: &str,
    settings: &BTreeMap<String, Value>,
) -> Result<PathBuf, String> {
    let name = name.trim();
    let group = if group.trim().is_empty() {
        "Yours"
    } else {
        group.trim()
    };
    if name.is_empty() {
        return Err("give the preset a name".into());
    }
    let safe = |s: &str| -> String {
        s.chars()
            .map(|c| {
                if c.is_alphanumeric() || " -_".contains(c) {
                    c
                } else {
                    '-'
                }
            })
            .collect::<String>()
            .trim()
            .to_string()
    };
    let catalog = amb_protocol::catalog::Catalog::builtin();
    let mut out: BTreeMap<String, Value> = settings
        .iter()
        .filter(|(k, _)| {
            catalog
                .lookup(k)
                .is_none_or(|t| !PHOTO_SPECIFIC_GROUPS.contains(&t.group.as_str()))
        })
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    if out.is_empty() {
        return Err("this photo has no edits to save".into());
    }
    let uuid: String = {
        use sha2::{Digest, Sha256};
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        Sha256::digest(format!("{group}/{name}/{stamp}"))
            .iter()
            .take(16)
            .map(|b| format!("{b:02X}"))
            .collect()
    };
    let alt = |s: &str| Value::Alt(vec![("x-default".into(), s.to_string())]);
    out.insert("PresetType".into(), Value::Text("Normal".into()));
    out.insert("Name".into(), alt(name));
    out.insert("Group".into(), alt(group));
    out.insert("UUID".into(), Value::Text(uuid));
    out.insert("HasSettings".into(), Value::Text("True".into()));
    out.insert("SupportsAmount".into(), Value::Text("True".into()));
    let folder = dir.join(safe(group));
    std::fs::create_dir_all(&folder).map_err(|e| format!("{}: {e}", folder.display()))?;
    let path = folder.join(format!("{}.xmp", safe(name)));
    let text = amb_presets::xmp_writer::write_packet(&out, amb_presets::is_signed_tag);
    amb_protocol::sidecar::write_atomic(&path, &text).map_err(|e| e.to_string())?;
    Ok(path)
}

/// Lightroom/Camera Raw's own preset folders, when present (read in place).
pub fn default_preset_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(appdata) = std::env::var("APPDATA") {
        dirs.push(PathBuf::from(appdata).join("Adobe/CameraRaw/Settings"));
    }
    if let Ok(home) = std::env::var("HOME") {
        dirs.push(
            PathBuf::from(&home).join("Library/Application Support/Adobe/CameraRaw/Settings"),
        );
        dirs.push(PathBuf::from(&home).join(".local/share/ambrotype/presets"));
    }
    dirs.into_iter().filter(|d| d.is_dir()).collect()
}

/// All presets (.xmp, .lrtemplate) under `dirs`, sorted by group and name.
pub fn scan_presets(dirs: &[PathBuf]) -> Vec<PresetEntry> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>, depth: usize) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() && depth < 6 {
                walk(&p, out, depth + 1);
            } else if p.extension().and_then(|x| x.to_str()).is_some_and(|x| {
                x.eq_ignore_ascii_case("xmp") || x.eq_ignore_ascii_case("lrtemplate")
            }) {
                out.push(p);
            }
        }
    }
    let mut files = Vec::new();
    for d in dirs {
        walk(d, &mut files, 0);
    }
    let mut presets: Vec<PresetEntry> = files
        .par_iter()
        .filter_map(|p| {
            let preset = amb_presets::Preset::from_path(p).ok()?;
            let stem = p.file_stem()?.to_string_lossy().into_owned();
            let parent = p
                .parent()
                .and_then(|d| d.file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            Some(PresetEntry {
                name: preset.name().map(str::to_string).unwrap_or(stem),
                group: preset.group().map(str::to_string).unwrap_or(parent),
                path: p.display().to_string(),
            })
        })
        .collect();
    presets.sort_by(|a, b| {
        (a.group.to_lowercase(), a.name.to_lowercase())
            .cmp(&(b.group.to_lowercase(), b.name.to_lowercase()))
    });
    presets
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edit_copies_are_named_like_lightrooms() {
        let dir = std::env::temp_dir().join(format!("amb-gui-editin-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let raw = dir.join("IMG_0001.NEF");
        std::fs::write(&raw, b"x").unwrap();
        assert_eq!(edit_copy_path(&raw), dir.join("IMG_0001-Edit.tif"));
        std::fs::write(dir.join("IMG_0001-Edit.tif"), b"x").unwrap();
        assert_eq!(edit_copy_path(&raw), dir.join("IMG_0001-Edit-2.tif"));
        // A virtual copy's goes next to its photo, by the photo's name.
        let copy = dir.join("IMG_0001.copy-2.xmp");
        std::fs::write(&copy, "<x/>").unwrap();
        assert_eq!(edit_copy_path(&copy), dir.join("IMG_0001-Edit-2.tif"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A synthetic DNG in its own file: tests run in parallel, and two of them
    /// writing the same path would read each other's half-written file.
    fn synthetic(name: &str) -> PathBuf {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("amb-gui-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("{name}-{n}.dng"));
        amb_testkit::dng::write(
            &path,
            &amb_testkit::patterns::generate(name).unwrap(),
            &Default::default(),
        )
        .unwrap();
        path
    }

    #[test]
    fn open_edit_preview_undo_save_export() {
        let path = synthetic("colorchecker");
        let lens = LensSources::default();
        let mut s = Session::open(&path).unwrap();
        let info = s.info();
        assert_eq!((info.width, info.height), (1368, 920));
        assert!(!info.sidecar_loaded);

        let base = s.preview(512, &BTreeMap::new(), false, false, &lens);
        // Integer area averaging: the long edge is at most 512 (here 1368 / 3).
        assert_eq!(base.width, 456);
        assert_eq!(
            base.histogram[1].iter().sum::<u32>() as usize,
            base.width * base.height
        );
        assert_eq!(&base.jpeg[..2], &[0xFF, 0xD8]);

        // A drag override brightens without touching history.
        let over = BTreeMap::from([("Exposure2012".to_string(), Value::Number(1.0))]);
        let bright = s.preview(512, &over, false, false, &lens);
        let mean = |p: &Preview| {
            p.histogram[1]
                .iter()
                .enumerate()
                .map(|(i, n)| i as f64 * f64::from(*n))
                .sum::<f64>()
                / (p.width * p.height) as f64
        };
        assert!(mean(&bright) > mean(&base) + 10.0);
        assert!(!s.state_info().can_undo);

        // A crop renders a crop-sized preview; the photo info stays uncropped.
        let crop = BTreeMap::from([
            ("HasCrop".to_string(), Value::Bool(true)),
            ("CropLeft".to_string(), Value::Number(0.25)),
            ("CropRight".to_string(), Value::Number(0.75)),
            ("CropAngle".to_string(), Value::Number(2.0)),
        ]);
        let cropped = s.preview(512, &crop, false, false, &lens);
        // About half the photo's width, rendered from a larger source so the
        // crop still fills about `max_edge` (integer downscale: 1368 / 2).
        assert!((300..=512).contains(&cropped.width), "{}", cropped.width);
        assert!(cropped.height > cropped.width, "{}", cropped.height);
        assert_eq!((s.info().width, s.info().height), (1368, 920));

        // Committing a command is undoable.
        s.execute(
            &[Command::SetParam {
                name: "Exposure2012".into(),
                value: serde_json::json!(1.0),
            }],
            None,
        )
        .unwrap();
        let info = s.state_info();
        assert!(info.can_undo && info.settings["Exposure2012"] == serde_json::json!(1.0));
        s.execute(&[Command::Undo], None).unwrap();
        assert!(s.state.settings().is_empty());
        s.execute(&[Command::Redo], None).unwrap();

        // "Before" ignores the edits.
        let before = s.preview(512, &BTreeMap::new(), true, false, &lens);
        assert!((mean(&before) - mean(&base)).abs() < 0.5);

        let saved = s.save_sidecar().unwrap();
        let text = std::fs::read_to_string(&saved).unwrap();
        assert!(text.contains("crs:Exposure2012=\"+1\""), "{text}");
        let reopened = Session::open(&path).unwrap();
        assert!(reopened.info().sidecar_loaded);
        assert_eq!(reopened.state.settings().len(), 1);

        let out = path.with_extension("jpg");
        let mut settings = amb_export::ExportSettings::new(amb_export::Format::Jpeg);
        settings.resize = amb_export::Resize::LongEdge(800);
        let report = s.export(&out, &settings, &lens).unwrap();
        assert_eq!(report.width, 800);
    }

    #[test]
    fn region_pick_white_and_clipping() {
        let path = synthetic("colorchecker");
        let lens = LensSources::default();
        let mut s = Session::open(&path).unwrap();
        let r = s.region(0.5, 0.5, 300, 200, false, false, &lens);
        assert_eq!((r.width, r.height), (300, 200));
        assert_eq!((r.full_width, r.full_height), (1368, 920));
        assert_eq!((r.x, r.y), (684 - 150, 460 - 100));
        // A corner request is clamped inside the image.
        let c = s.region(0.0, 1.0, 300, 200, false, true, &lens);
        assert_eq!((c.x, c.y), (0, 920 - 200));
        // Picking the light grey patch (row 4, second patch) gives D50: ~5000 K, and a tint
        // near +10 because D50 sits above the Planckian locus (Lightroom shows the same).
        let (patch, gap) = (200.0, 24.0);
        let u = (gap + (patch + gap) + patch / 2.0) / 1368.0;
        let v = (gap + 3.0 * (patch + gap) + patch / 2.0) / 920.0;
        let [("Temperature", t), ("Tint", tint)] = s.pick_white(u, v).expect("grey patch") else {
            panic!("a raw picks Kelvin");
        };
        assert!(
            (4800.0..5300.0).contains(&t) && (5.0..15.0).contains(&tint),
            "{t} {tint}"
        );
        let ctx = s.feedback_context(true);
        assert!(ctx.contains("Camera:") && ctx.contains("Settings:"));
    }

    #[test]
    fn region_tiles_match_the_full_render_in_any_orientation() {
        let path = synthetic("colorchecker");
        let lens = LensSources::default();
        for orientation in [1u16, 6, 3] {
            let mut s = Session::open(&path).unwrap();
            s.full.metadata.orientation = orientation;
            let set = |name: &str, v: f64| Command::SetParam {
                name: name.into(),
                value: v.into(),
            };
            let edit = [
                set("Clarity2012", 40.0),
                set("Shadows2012", 50.0),
                set("ColorNoiseReduction", 0.0),
            ];
            s.execute(&edit, None).unwrap();
            let settings = s.effective_settings(&BTreeMap::new(), false);
            let develop = s.develop(&settings, &lens);
            let full = to_srgb8(&amb_export::geometry::orient(
                &pipeline::render(&s.full, &develop),
                orientation,
            ));
            // A first crop, then a pan that needs new tiles.
            for (cx, cy) in [(0.3, 0.4), (0.6, 0.55)] {
                let r = s.region(cx, cy, 300, 200, false, false, &lens);
                let cache = s.region_cache.as_ref().unwrap();
                let worst = (r.y..r.y + r.height)
                    .flat_map(|y| {
                        (r.x * 3..(r.x + r.width) * 3).map(move |i| y * r.full_width * 3 + i)
                    })
                    .map(|i| cache.srgb[i].abs_diff(full[i]))
                    .max()
                    .unwrap();
                assert!(worst <= 1, "orientation {orientation}: {worst}");
            }
        }
    }

    #[test]
    fn crop_tool_round_trips_through_one_undo_step() {
        let path = synthetic("colorchecker");
        let lens = LensSources::default();
        let mut s = Session::open(&path).unwrap();
        let none = s.crop_state();
        assert!(!none.enabled);
        assert_eq!(
            none.rect,
            CropRect {
                cx: 684.0,
                cy: 460.0,
                width: 1368.0,
                height: 920.0,
                angle: 0.0
            }
        );
        let rect = CropRect {
            cx: 700.0,
            cy: 450.0,
            width: 800.0,
            height: 500.0,
            angle: 4.5,
        };
        let cmds = s.crop_commands(&rect);
        assert_eq!(cmds.len(), 6);
        s.execute(&cmds, Some("crop")).unwrap();
        let info = s.state_info();
        assert_eq!(info.history, vec!["crop".to_string()]);
        let back = s.crop_state();
        assert!(back.enabled);
        for (a, b) in [
            (back.rect.cx, rect.cx),
            (back.rect.cy, rect.cy),
            (back.rect.width, rect.width),
            (back.rect.height, rect.height),
            (back.rect.angle, rect.angle),
        ] {
            assert!((a - b).abs() < 0.01, "{back:?} vs {rect:?}");
        }
        // The preview renders the crop's size; with `HasCrop` overridden off
        // (the crop tool's view) it shows the whole photo.
        let p = s.preview(1368, &BTreeMap::new(), false, false, &lens);
        assert_eq!((p.width, p.height), (800, 500));
        let off = BTreeMap::from([("HasCrop".to_string(), Value::Bool(false))]);
        let whole = s.preview(1368, &off, false, false, &lens);
        assert_eq!((whole.width, whole.height), (1368, 920));
        // The whole photo with no angle (the tool's Reset) turns the crop off.
        let reset = s.crop_commands(&none.rect);
        s.execute(&reset, Some("reset crop")).unwrap();
        assert!(!s.crop_state().enabled);
        assert!(!s.state.settings().contains_key("CropLeft"));
        s.execute(&[Command::Undo], None).unwrap();
        assert!(s.crop_state().enabled);
    }

    #[test]
    fn rating_and_keywords_are_written_at_once_without_unsaved_edits() {
        use amb_protocol::sidecar::PhotoInfo;
        let path = synthetic("ramp");
        let mut s = Session::open(&path).unwrap();
        assert_eq!(s.info, PhotoInfo::default());
        // An unsaved edit...
        s.execute(
            &[Command::SetParam {
                name: "Exposure2012".into(),
                value: serde_json::json!(1.0),
            }],
            None,
        )
        .unwrap();
        // ...is not written by rating the photo.
        let info = PhotoInfo {
            rating: Some(3),
            keywords: vec!["test".into()],
            label: Some("Red".into()),
            flag: Some(amb_protocol::sidecar::Flag::Pick),
            ..Default::default()
        };
        assert_eq!(s.set_info(&info).unwrap(), info);
        let on_disk = std::fs::read_to_string(&s.sidecar).unwrap();
        assert!(!on_disk.contains("Exposure2012"), "{on_disk}");
        // Saving the edit keeps the rating; reopening reads both.
        assert!(s.has_unsaved_edits());
        s.save_sidecar().unwrap();
        assert!(!s.has_unsaved_edits());
        let again = Session::open(&path).unwrap();
        assert_eq!(again.info, info);
        assert_eq!(again.state.settings().len(), 1);
    }

    #[test]
    fn presets_are_found_with_names_and_groups() {
        let dir = std::env::temp_dir().join(format!("amb-gui-presets-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("Warm")).unwrap();
        std::fs::write(
            dir.join("Warm/sunny.xmp"),
            r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/" crs:Temperature="6500" crs:Exposure2012="+0.3"/></rdf:RDF></x:xmpmeta>"#,
        )
        .unwrap();
        std::fs::write(dir.join("notes.txt"), "not a preset").unwrap();
        let found = scan_presets(std::slice::from_ref(&dir));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "sunny");
        assert_eq!(found[0].group, "Warm");
    }

    #[test]
    fn saves_a_preset_without_the_crop() {
        let dir = std::env::temp_dir().join(format!("amb-save-preset-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut settings = BTreeMap::new();
        settings.insert("Exposure2012".to_string(), Value::Text("+0.50".into()));
        settings.insert("GrainAmount".to_string(), Value::Text("25".into()));
        settings.insert("CropTop".to_string(), Value::Text("0.1".into()));
        settings.insert("HasCrop".to_string(), Value::Text("True".into()));
        let path = save_preset(&dir, "Faded street", "", &settings).unwrap();
        assert!(
            path.ends_with("Yours/Faded street.xmp"),
            "{}",
            path.display()
        );
        let preset = amb_presets::Preset::from_path(&path).unwrap();
        assert_eq!(preset.name(), Some("Faded street"));
        assert!(preset.settings.contains_key("GrainAmount"));
        assert!(
            !preset.settings.contains_key("CropTop") && !preset.settings.contains_key("HasCrop")
        );
        // It shows up in the presets list, in its group.
        let found = scan_presets(std::slice::from_ref(&dir));
        assert_eq!(
            (found[0].name.as_str(), found[0].group.as_str()),
            ("Faded street", "Yours")
        );
        assert!(save_preset(&dir, " ", "", &settings).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
