//! One open photo: its decoded raw, the develop state (with undo), previews,
//! sidecar saving and export. No Tauri types here, so it is unit-testable;
//! `main.rs` exposes it to the web UI as commands.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use flb_engine::pipeline::{self, DevelopSettings};
use flb_engine::raw::{self, CameraImage};
use flb_lens::lcp::LcpLibrary;
use flb_lens::lensfun::LensfunDb;
use flb_presets::Value;
use flb_protocol::{Command, DevelopState, Outcome};
use rayon::prelude::*;
use serde::Serialize;

/// Lens profile sources, loaded once per app run.
#[derive(Default)]
pub struct LensSources {
    pub db: Option<LensfunDb>,
    pub lcp: Option<LcpLibrary>,
}

impl LensSources {
    pub fn load_default() -> LensSources {
        let lcp = LcpLibrary::scan_default();
        LensSources {
            db: LensfunDb::load_default(),
            lcp: (!lcp.files.is_empty()).then_some(lcp),
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
    /// XMP text of the sidecar we loaded, so saving keeps its other metadata.
    base_xmp: Option<String>,
    /// Resolved lens profile, keyed by (profile filename hint, auto CA or profile on).
    lens_cache: Option<(Option<String>, Option<flb_lens::Correction>)>,
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
}

#[derive(Debug, Clone, Serialize)]
pub struct StateInfo {
    pub settings: serde_json::Value,
    pub can_undo: bool,
    pub can_redo: bool,
    pub history: Vec<String>,
    /// Settings present but not rendered by this engine version.
    pub not_rendered: Vec<String>,
}

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
}

/// Area-average downscale of camera RGB so the long edge is at most `max_edge`.
pub fn downscale(img: &CameraImage, max_edge: usize) -> CameraImage {
    let long = img.width.max(img.height);
    let factor = long.div_ceil(max_edge.max(1)).max(1);
    if factor == 1 {
        return clone_camera(img, img.rgb.clone(), img.width, img.height);
    }
    let (w, h) = (img.width / factor, img.height / factor);
    let mut rgb = vec![0.0f32; w * h * 3];
    let n = (factor * factor) as f32;
    rgb.par_chunks_mut(w * 3).enumerate().for_each(|(y, row)| {
        for x in 0..w {
            let mut acc = [0.0f32; 3];
            for dy in 0..factor {
                let sy = y * factor + dy;
                for dx in 0..factor {
                    let i = (sy * img.width + x * factor + dx) * 3;
                    acc[0] += img.rgb[i];
                    acc[1] += img.rgb[i + 1];
                    acc[2] += img.rgb[i + 2];
                }
            }
            row[x * 3..x * 3 + 3].copy_from_slice(&acc.map(|v| v / n));
        }
    });
    clone_camera(img, rgb, w, h)
}

fn clone_camera(img: &CameraImage, rgb: Vec<f32>, width: usize, height: usize) -> CameraImage {
    CameraImage {
        width,
        height,
        rgb,
        xyz_to_cam: img.xyz_to_cam,
        as_shot_neutral: img.as_shot_neutral,
        make: img.make.clone(),
        model: img.model.clone(),
        shot: img.shot.clone(),
        metadata: img.metadata.clone(),
    }
}

/// Lightroom's sidecar convention: `photo.nef` → `photo.xmp`.
pub fn sidecar_path(photo: &Path) -> PathBuf {
    photo.with_extension("xmp")
}

fn ratio_text(r: Option<(u32, u32)>, f: impl Fn(f64) -> String) -> Option<String> {
    r.filter(|r| r.1 != 0)
        .map(|(n, d)| f(f64::from(n) / f64::from(d)))
}

impl Session {
    pub fn open(path: &Path) -> Result<Session, String> {
        let full = raw::load(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let sidecar = sidecar_path(path);
        let (state, base_xmp) = match std::fs::read_to_string(&sidecar) {
            Ok(text) => (
                DevelopState::from_xmp_str(&text).map_err(|e| e.to_string())?,
                Some(text),
            ),
            Err(_) => (DevelopState::new(), None),
        };
        Ok(Session {
            path: path.to_path_buf(),
            full,
            preview_src: None,
            state,
            sidecar,
            base_xmp,
            lens_cache: None,
        })
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
            name: self
                .path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
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
        }
    }

    pub fn state_info(&self) -> StateInfo {
        let (_, not_rendered) = DevelopSettings::from_settings(self.state.settings());
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
            not_rendered,
        }
    }

    pub fn execute(&mut self, cmds: &[Command]) -> Result<Vec<Outcome>, String> {
        self.state.execute_all(cmds).map_err(|e| e.to_string())
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
                    let hint = flb_lens::ProfileHint {
                        name: develop.lens.profile_name.clone(),
                        filename: key.clone(),
                    };
                    let p = flb_lens::resolve(&shot, &hint, lens.lcp.as_ref(), lens.db.as_ref());
                    self.lens_cache = Some((key, p.clone()));
                    p
                }
            };
            develop.lens.profile = profile;
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
        lens: &LensSources,
    ) -> Preview {
        let start = Instant::now();
        let max_edge = max_edge.clamp(256, 4096);
        if self.preview_src.as_ref().map(|(e, _)| *e as usize) != Some(max_edge) {
            self.preview_src = Some((max_edge as u32, downscale(&self.full, max_edge)));
        }
        let mut settings = if before {
            BTreeMap::new()
        } else {
            self.state.settings().clone()
        };
        if !before {
            for (k, v) in overrides {
                settings.insert(k.clone(), v.clone());
            }
            // A Temperature/Tint drag implies a custom white balance.
            if overrides.contains_key("Temperature") || overrides.contains_key("Tint") {
                settings.insert("WhiteBalance".into(), Value::Text("Custom".into()));
            }
        }
        let develop = self.develop(&settings, lens);
        let lens_profile = develop.lens.profile.as_ref().map(|c| c.description.clone());
        let src = &self.preview_src.as_ref().expect("set above").1;
        let developed = pipeline::render(src, &develop);
        let upright = flb_export::geometry::orient(&developed, self.full.metadata.orientation);
        let srgb = flb_engine::output::to_srgb(&upright);
        let bytes: Vec<u8> = srgb
            .par_iter()
            .map(|v| (v * 255.0).round().clamp(0.0, 255.0) as u8)
            .collect();
        let mut histogram = [vec![0u32; 256], vec![0u32; 256], vec![0u32; 256]];
        for p in bytes.chunks(3) {
            for c in 0..3 {
                histogram[c][p[c] as usize] += 1;
            }
        }
        let mut jpeg = Vec::new();
        let mut enc = jpeg_encoder::Encoder::new(&mut jpeg, 88);
        enc.set_sampling_factor(jpeg_encoder::SamplingFactor::R_4_2_0);
        let _ = enc.encode(
            &bytes,
            upright.width as u16,
            upright.height as u16,
            jpeg_encoder::ColorType::Rgb,
        );
        Preview {
            jpeg,
            width: upright.width,
            height: upright.height,
            histogram,
            millis: start.elapsed().as_millis(),
            lens_profile,
        }
    }

    /// Write the develop settings to the photo's XMP sidecar (keeping the
    /// sidecar's other metadata, like Lightroom's "Save Metadata to File").
    pub fn save_sidecar(&mut self) -> Result<String, String> {
        self.state
            .save_merged(&self.sidecar, self.base_xmp.as_deref())
            .map_err(|e| e.to_string())?;
        self.base_xmp = std::fs::read_to_string(&self.sidecar).ok();
        Ok(self.sidecar.display().to_string())
    }

    /// Full-resolution render and export.
    pub fn export(
        &mut self,
        out: &Path,
        settings: &flb_export::ExportSettings,
        lens: &LensSources,
    ) -> Result<flb_export::ExportReport, String> {
        let current = self.state.settings().clone();
        let develop = self.develop(&current, lens);
        let developed = pipeline::render(&self.full, &develop);
        flb_export::export(&developed, &self.full.metadata, settings, out)
            .map_err(|e| format!("{}: {e}", out.display()))
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PresetEntry {
    pub name: String,
    pub group: String,
    pub path: String,
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
        dirs.push(PathBuf::from(&home).join(".local/share/flightbroom/presets"));
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
            let preset = flb_presets::Preset::from_path(p).ok()?;
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

    fn synthetic(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("flb-gui-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("{name}.dng"));
        flb_testkit::dng::write(
            &path,
            &flb_testkit::patterns::generate(name).unwrap(),
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

        let base = s.preview(512, &BTreeMap::new(), false, &lens);
        // Integer area averaging: the long edge is at most 512 (here 1368 / 3).
        assert_eq!(base.width, 456);
        assert_eq!(
            base.histogram[1].iter().sum::<u32>() as usize,
            base.width * base.height
        );
        assert_eq!(&base.jpeg[..2], &[0xFF, 0xD8]);

        // A drag override brightens without touching history.
        let over = BTreeMap::from([("Exposure2012".to_string(), Value::Number(1.0))]);
        let bright = s.preview(512, &over, false, &lens);
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

        // Committing a command is undoable.
        s.execute(&[Command::SetParam {
            name: "Exposure2012".into(),
            value: serde_json::json!(1.0),
        }])
        .unwrap();
        let info = s.state_info();
        assert!(info.can_undo && info.settings["Exposure2012"] == serde_json::json!(1.0));
        s.execute(&[Command::Undo]).unwrap();
        assert!(s.state.settings().is_empty());
        s.execute(&[Command::Redo]).unwrap();

        // "Before" ignores the edits.
        let before = s.preview(512, &BTreeMap::new(), true, &lens);
        assert!((mean(&before) - mean(&base)).abs() < 0.5);

        let saved = s.save_sidecar().unwrap();
        let text = std::fs::read_to_string(&saved).unwrap();
        assert!(text.contains("crs:Exposure2012=\"+1\""), "{text}");
        let reopened = Session::open(&path).unwrap();
        assert!(reopened.info().sidecar_loaded);
        assert_eq!(reopened.state.settings().len(), 1);

        let out = path.with_extension("jpg");
        let mut settings = flb_export::ExportSettings::new(flb_export::Format::Jpeg);
        settings.resize = flb_export::Resize::LongEdge(800);
        let report = s.export(&out, &settings, &lens).unwrap();
        assert_eq!(report.width, 800);
    }

    #[test]
    fn presets_are_found_with_names_and_groups() {
        let dir = std::env::temp_dir().join(format!("flb-gui-presets-{}", std::process::id()));
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
}
