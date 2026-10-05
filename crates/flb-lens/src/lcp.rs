//! Adobe Lens Correction Profiles (`.lcp`), read in place from the user's
//! Camera Raw / Lightroom install. We never copy or ship them.
//!
//! An LCP is RDF/XMP: `photoshop:CameraProfiles/rdf:Seq/rdf:li`, one `li` per
//! calibration shot (focal length, focus distance, aperture). Fields are in
//! the `stCamera` namespace and appear either as child elements or as
//! attributes (of the `li` itself or of an `rdf:Description` inside it); the
//! nested camera models use the same two forms. The models follow the Adobe
//! Camera Model (see [`crate::model`]).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::interp::{self, DEFAULT_DISTANCE_M, VigPoint};
use crate::model::{Correction, Distortion, Frame, Source, Tca, Vignetting};
use crate::names;
use crate::xml::Element;
use crate::{Error, ShotInfo};

/// f-number for an APEX aperture value: N = 2^(Av/2).
pub fn apex_to_f_number(av: f64) -> f64 {
    (av / 2.0).exp2()
}

/// Lateral CA of one channel relative to green.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChromaticModel {
    pub scale_factor: f64,
    pub radial: [f64; 3],
    pub tangential: [f64; 2],
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VignetteModel {
    pub focal_length_x: Option<f64>,
    pub focal_length_y: Option<f64>,
    pub params: [f64; 3],
}

/// `stCamera:PerspectiveModel` or `stCamera:FisheyeModel`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct CameraModel {
    pub version: Option<f64>,
    /// Fractions of the calibration image's longest side.
    pub focal_length_x: Option<f64>,
    pub focal_length_y: Option<f64>,
    pub image_x_center: Option<f64>,
    pub image_y_center: Option<f64>,
    pub radial: [f64; 3],
    pub tangential: [f64; 2],
    pub residual_mean_error: Option<f64>,
    pub chromatic_red_green: Option<ChromaticModel>,
    pub chromatic_blue_green: Option<ChromaticModel>,
    pub vignette: Option<VignetteModel>,
}

/// One calibration entry (`rdf:li`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LcpEntry {
    pub author: Option<String>,
    /// Camera make and model of the calibration.
    pub make: Option<String>,
    pub model: Option<String>,
    pub unique_camera_model: Option<String>,
    /// Calibrated on raw files (true) or JPEGs (false).
    pub camera_raw_profile: Option<bool>,
    pub lens_pretty_name: Option<String>,
    pub lens: Option<String>,
    pub lens_id: Option<String>,
    pub lens_info: Option<String>,
    pub profile_name: Option<String>,
    /// Crop factor of the calibration camera.
    pub sensor_format_factor: Option<f64>,
    /// mm.
    pub focal_length: Option<f64>,
    /// m.
    pub focus_distance: Option<f64>,
    /// APEX Av; see [`LcpEntry::f_number`].
    pub aperture_value: Option<f64>,
    pub image_width: Option<f64>,
    pub image_length: Option<f64>,
    pub perspective: Option<CameraModel>,
    pub fisheye: Option<CameraModel>,
}

impl LcpEntry {
    pub fn f_number(&self) -> Option<f64> {
        self.aperture_value.map(apex_to_f_number)
    }

    fn model(&self) -> Option<&CameraModel> {
        self.perspective.as_ref().or(self.fisheye.as_ref())
    }

    fn distance(&self) -> f64 {
        self.focus_distance
            .filter(|d| d.is_finite() && *d > 0.0)
            .unwrap_or(DEFAULT_DISTANCE_M)
    }

    /// The ACM frame for this entry's model, optionally with the vignette
    /// model's own focal lengths.
    fn frame(&self, m: &CameraModel, focal_xy: Option<(f64, f64)>, focal: f64) -> Frame {
        let crop = self
            .sensor_format_factor
            .filter(|c| c.is_finite() && *c > 0.0)
            .unwrap_or(1.0);
        // Defaults: the image centre, and the focal length over the longest side
        // of a 3:2 sensor of this crop.
        let (w, h) = match (self.image_width, self.image_length) {
            (Some(w), Some(h)) if w > 0.0 && h > 0.0 => (w, h),
            _ => (3.0, 2.0),
        };
        let long = w.max(h);
        let default_f = focal * crop / 36.0;
        let (fx, fy) = focal_xy.unwrap_or((
            m.focal_length_x.unwrap_or(default_f),
            m.focal_length_y.or(m.focal_length_x).unwrap_or(default_f),
        ));
        Frame::Acm {
            cx: m.image_x_center.unwrap_or(0.5 * w / long),
            cy: m.image_y_center.unwrap_or(0.5 * h / long),
            fx,
            fy,
            cal_crop: crop,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LcpFile {
    pub path: PathBuf,
    pub entries: Vec<LcpEntry>,
}

impl LcpFile {
    pub fn parse(path: impl AsRef<Path>) -> Result<LcpFile, Error> {
        let path = path.as_ref();
        let bytes = std::fs::read(path)?;
        LcpFile::parse_str(&String::from_utf8_lossy(&bytes), path)
    }

    pub fn parse_str(text: &str, path: impl AsRef<Path>) -> Result<LcpFile, Error> {
        let path = path.as_ref();
        let err = |message: String| Error::Parse {
            path: path.display().to_string(),
            message,
        };
        let root = Element::parse(text).map_err(err)?;
        let profiles = root
            .find("CameraProfiles")
            .ok_or_else(|| err("no photoshop:CameraProfiles".into()))?;
        let seq = profiles.find("Seq").unwrap_or(profiles);
        let mut entries: Vec<LcpEntry> = seq.children_named("li").map(parse_entry).collect();
        if entries.is_empty() {
            return Err(err("no calibration entries".into()));
        }
        // Entries without a focal length: a prime's name gives it.
        let prime = entries
            .iter()
            .find_map(|e| e.lens_pretty_name.as_deref().or(e.lens.as_deref()))
            .and_then(prime_focal);
        for e in &mut entries {
            if e.focal_length.is_none() {
                e.focal_length = prime;
            }
        }
        Ok(LcpFile {
            path: path.to_path_buf(),
            entries,
        })
    }

    fn first<'a>(&'a self, f: impl Fn(&'a LcpEntry) -> Option<&'a String>) -> Option<&'a str> {
        self.entries.iter().find_map(f).map(String::as_str)
    }

    /// The lens's display name.
    pub fn lens_name(&self) -> Option<&str> {
        self.first(|e| e.lens_pretty_name.as_ref())
            .or_else(|| self.first(|e| e.lens.as_ref()))
    }

    pub fn profile_name(&self) -> Option<&str> {
        self.first(|e| e.profile_name.as_ref())
    }

    /// How well this profile matches the shot: `None` below the name threshold;
    /// above it, the name score plus small bonuses for the calibration camera,
    /// a raw (not JPEG) profile and a matching crop factor.
    pub fn score(&self, shot: &ShotInfo) -> Option<f64> {
        let lens_model = shot.lens_model.as_deref()?;
        // The lens make is a maker word, dropped on both sides, so "make + model"
        // compares the same as the model alone.
        let shot_tokens = names::tokens(lens_model);
        let e = self.entries.first()?;
        let makers = names::maker_words(
            e.make
                .as_deref()
                .into_iter()
                .chain(shot.lens_make.as_deref())
                .chain([shot.camera_make.as_str()]),
        );
        let name_score = [&e.lens, &e.lens_pretty_name]
            .into_iter()
            .flatten()
            .filter_map(|n| names::score(&names::tokens(n), &shot_tokens, &makers))
            .max_by(f64::total_cmp)
            .filter(|s| *s >= names::MIN_SCORE)?;
        let mut bonus = 0.0;
        if let Some(make) = e.make.as_deref() {
            if names::same_maker(make, &shot.camera_make) {
                bonus += 0.03;
                let norm = |s: &str| {
                    s.split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" ")
                        .to_lowercase()
                };
                let models = [&e.model, &e.unique_camera_model];
                if models.into_iter().flatten().any(|m| {
                    let (m, s) = (norm(m), norm(&shot.camera_model));
                    m == s || m.ends_with(&format!(" {s}")) || s.ends_with(&format!(" {m}"))
                }) {
                    bonus += 0.03;
                }
            }
        }
        if e.camera_raw_profile != Some(false) {
            bonus += 0.02;
        }
        if let (Some(cal), Some(img)) = (e.sensor_format_factor, shot.crop_factor) {
            if cal > 0.0 && img > 0.0 && (cal / img - 1.0).abs() < 0.02 {
                bonus += 0.02;
            }
        }
        Some(name_score + bonus)
    }

    /// Interpolate the profile to the shot. `None` when nothing applies.
    ///
    /// Distortion and TCA: per calibrated focal length the entry whose focus
    /// distance is nearest the shot's (log space; the largest when unknown),
    /// then linear in focal length, frame included. Vignetting: over focal
    /// length and aperture (stops) at the nearest distance; needs the aperture.
    pub fn correction(&self, shot: &ShotInfo) -> Option<Correction> {
        let focal_keys = self.entries.iter().filter_map(|e| e.focal_length);
        let focal = interp::pick_key(focal_keys, shot.focal_mm);

        let distortion = focal.and_then(|f| {
            let samples = self.per_focal(shot, |e| {
                let m = e.perspective.as_ref()?;
                let k = [
                    m.radial[0],
                    m.radial[1],
                    m.radial[2],
                    m.tangential[0],
                    m.tangential[1],
                ];
                Some((Distortion::Acm { k }, e.frame(m, None, e.focal_length?)))
            });
            interp::interpolate(&samples, f, |a, b, t| {
                Some((a.0.lerp(&b.0, t)?, a.1.lerp(&b.1, t)?))
            })
        });
        let tca = focal.and_then(|f| {
            let samples = self.per_focal(shot, |e| {
                let m = e.model()?;
                if m.chromatic_red_green.is_none() && m.chromatic_blue_green.is_none() {
                    return None;
                }
                let ch = |c: Option<ChromaticModel>| {
                    let c = c.unwrap_or(ChromaticModel {
                        scale_factor: 1.0,
                        radial: [0.0; 3],
                        tangential: [0.0; 2],
                    });
                    [
                        c.scale_factor,
                        c.radial[0],
                        c.radial[1],
                        c.radial[2],
                        c.tangential[0],
                        c.tangential[1],
                    ]
                };
                let tca = Tca::Acm {
                    red: ch(m.chromatic_red_green),
                    blue: ch(m.chromatic_blue_green),
                };
                Some((tca, e.frame(m, None, e.focal_length?)))
            });
            interp::interpolate(&samples, f, |a, b, t| {
                Some((a.0.lerp(&b.0, t)?, a.1.lerp(&b.1, t)?))
            })
        });
        let vignetting = shot.aperture.and_then(|aperture| {
            let points: Vec<VigPoint<(Vignetting, Frame)>> = self
                .entries
                .iter()
                .filter_map(|e| {
                    let m = e.model()?;
                    let v = m.vignette?;
                    let focal = e.focal_length?;
                    let focal_xy = v
                        .focal_length_x
                        .map(|fx| (fx, v.focal_length_y.unwrap_or(fx)));
                    Some(VigPoint {
                        focal,
                        aperture: e.f_number().filter(|a| a.is_finite() && *a > 0.0)?,
                        distance: e.distance(),
                        value: (
                            Vignetting::Acm {
                                k1: v.params[0],
                                k2: v.params[1],
                                k3: v.params[2],
                            },
                            e.frame(m, focal_xy, focal),
                        ),
                    })
                })
                .collect();
            let distance = shot
                .distance_m
                .filter(|d| d.is_finite() && *d > 0.0)
                .unwrap_or(DEFAULT_DISTANCE_M);
            interp::interpolate_vignetting(&points, shot.focal_mm, aperture, distance, |a, b, t| {
                Some((a.0.lerp(&b.0, t)?, a.1.lerp(&b.1, t)?))
            })
        });
        if distortion.is_none() && tca.is_none() && vignetting.is_none() {
            return None;
        }
        let name = self
            .profile_name()
            .or(self.lens_name())
            .unwrap_or("unnamed profile");
        let file_name = self
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        Some(Correction {
            source: Source::Lcp,
            description: format!("LCP: {name} [{file_name}]"),
            file: Some(self.path.display().to_string()),
            distortion,
            tca,
            vignetting,
        })
    }

    /// One sample per calibrated focal length, from the entry whose focus
    /// distance is nearest the shot's; sorted by focal length.
    fn per_focal<T>(
        &self,
        shot: &ShotInfo,
        model: impl Fn(&LcpEntry) -> Option<T>,
    ) -> Vec<(f64, T)> {
        let mut by_focal: Vec<(f64, Vec<(f64, T)>)> = Vec::new();
        for e in &self.entries {
            let Some(focal) = e.focal_length else {
                continue;
            };
            let Some(v) = model(e) else {
                continue;
            };
            match by_focal.iter_mut().find(|(f, _)| (*f - focal).abs() < 1e-6) {
                Some((_, list)) => list.push((e.distance(), v)),
                None => by_focal.push((focal, vec![(e.distance(), v)])),
            }
        }
        let target = shot.distance_m.filter(|d| d.is_finite() && *d > 0.0);
        let mut out: Vec<(f64, T)> = by_focal
            .into_iter()
            .filter_map(|(focal, mut list)| {
                let distances: Vec<f64> = list.iter().map(|(d, _)| *d).collect();
                let i = interp::nearest_distance(&distances, target)?;
                Some((focal, list.swap_remove(i).1))
            })
            .collect();
        out.sort_by(|a, b| a.0.total_cmp(&b.0));
        out
    }
}

/// The single focal length in a prime lens's name (`35mm` but not `24-70mm`).
fn prime_focal(name: &str) -> Option<f64> {
    let mut focals = names::tokens(name)
        .into_iter()
        .filter(|t| names::is_focal(t));
    let first = focals.next()?;
    let n = first.strip_suffix("mm")?;
    if n.contains('-') || focals.next().is_some() {
        return None;
    }
    n.parse().ok()
}

/// The nodes that may carry a struct's fields: the element and its
/// `rdf:Description` children.
fn scopes(el: &Element) -> impl Iterator<Item = &Element> {
    std::iter::once(el).chain(el.children_named("Description"))
}

/// A field, as an attribute or a child element's text.
fn field<'a>(el: &'a Element, name: &str) -> Option<&'a str> {
    scopes(el).find_map(|s| {
        s.attr(name).or_else(|| {
            s.child(name).map(|c| {
                // A language alternative: take its first item.
                if c.text().is_empty() {
                    c.find("li").map_or("", Element::text)
                } else {
                    c.text()
                }
            })
        })
    })
}

fn sub<'a>(el: &'a Element, name: &str) -> Option<&'a Element> {
    scopes(el).find_map(|s| s.child(name))
}

fn string(el: &Element, name: &str) -> Option<String> {
    field(el, name)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn number(el: &Element, name: &str) -> Option<f64> {
    field(el, name)
        .and_then(|s| s.trim().parse::<f64>().ok())
        .filter(|v| v.is_finite())
}

fn params<const N: usize>(el: &Element, prefix: &str) -> [f64; N] {
    std::array::from_fn(|i| number(el, &format!("{prefix}{}", i + 1)).unwrap_or(0.0))
}

fn parse_chromatic(el: &Element) -> ChromaticModel {
    ChromaticModel {
        scale_factor: number(el, "ScaleFactor").unwrap_or(1.0),
        radial: params(el, "RadialDistortParam"),
        tangential: params(el, "TangentialDistortParam"),
    }
}

fn parse_model(el: &Element) -> CameraModel {
    CameraModel {
        version: number(el, "Version"),
        focal_length_x: number(el, "FocalLengthX"),
        focal_length_y: number(el, "FocalLengthY"),
        image_x_center: number(el, "ImageXCenter"),
        image_y_center: number(el, "ImageYCenter"),
        radial: params(el, "RadialDistortParam"),
        tangential: params(el, "TangentialDistortParam"),
        residual_mean_error: number(el, "ResidualMeanError"),
        chromatic_red_green: sub(el, "ChromaticRedGreenModel").map(parse_chromatic),
        chromatic_blue_green: sub(el, "ChromaticBlueGreenModel").map(parse_chromatic),
        vignette: sub(el, "VignetteModel").map(|v| VignetteModel {
            focal_length_x: number(v, "FocalLengthX"),
            focal_length_y: number(v, "FocalLengthY"),
            params: params(v, "VignetteModelParam"),
        }),
    }
}

fn parse_entry(li: &Element) -> LcpEntry {
    LcpEntry {
        author: string(li, "Author"),
        make: string(li, "Make"),
        model: string(li, "Model"),
        unique_camera_model: string(li, "UniqueCameraModel"),
        camera_raw_profile: field(li, "CameraRawProfile")
            .map(|s| s.trim().eq_ignore_ascii_case("true")),
        lens_pretty_name: string(li, "LensPrettyName"),
        lens: string(li, "Lens"),
        lens_id: string(li, "LensID"),
        lens_info: string(li, "LensInfo"),
        profile_name: string(li, "ProfileName"),
        sensor_format_factor: number(li, "SensorFormatFactor"),
        focal_length: number(li, "FocalLength"),
        focus_distance: number(li, "FocusDistance"),
        aperture_value: number(li, "ApertureValue"),
        image_width: number(li, "ImageWidth"),
        image_length: number(li, "ImageLength"),
        perspective: sub(li, "PerspectiveModel").map(parse_model),
        fisheye: sub(li, "FisheyeModel").map(parse_model),
    }
}

/// A profile found for a shot.
#[derive(Debug, Clone)]
pub struct LcpMatch {
    pub file: Arc<LcpFile>,
    pub score: f64,
}

/// The `.lcp` files in a set of directories, parsed lazily (an Adobe install
/// has thousands).
#[derive(Debug, Default)]
pub struct LcpLibrary {
    pub files: Vec<PathBuf>,
    cache: Mutex<HashMap<PathBuf, Arc<LcpFile>>>,
}

impl LcpLibrary {
    /// `FLB_LCP_DIRS` (OS path-list syntax), then Adobe's profile folders.
    pub fn default_dirs() -> Vec<PathBuf> {
        let mut dirs: Vec<PathBuf> = std::env::var_os("FLB_LCP_DIRS")
            .map(|v| {
                std::env::split_paths(&v)
                    .filter(|p| !p.as_os_str().is_empty())
                    .collect()
            })
            .unwrap_or_default();
        const SUB: &str = "Adobe/CameraRaw/LensProfiles/1.0";
        if cfg!(windows) {
            for var in ["PROGRAMDATA", "APPDATA"] {
                if let Some(base) = std::env::var_os(var) {
                    let mut p = PathBuf::from(base);
                    p.extend(SUB.split('/'));
                    dirs.push(p);
                }
            }
        }
        if cfg!(target_os = "macos") {
            dirs.push(Path::new("/Library/Application Support").join(SUB));
            if let Some(home) = std::env::var_os("HOME") {
                dirs.push(
                    PathBuf::from(home)
                        .join("Library/Application Support")
                        .join(SUB),
                );
            }
        }
        dirs
    }

    /// Collect `*.lcp` paths under the directories, recursively, without
    /// parsing them. Missing directories are skipped.
    pub fn scan<P: AsRef<Path>>(dirs: impl IntoIterator<Item = P>) -> LcpLibrary {
        let mut files = Vec::new();
        for dir in dirs {
            collect_lcp(dir.as_ref(), &mut files, 0);
        }
        files.sort();
        files.dedup();
        LcpLibrary {
            files,
            cache: Mutex::default(),
        }
    }

    pub fn scan_default() -> LcpLibrary {
        LcpLibrary::scan(LcpLibrary::default_dirs())
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// A profile by file name, case-insensitive (Lightroom records
    /// `crs:LensProfileFilename`, e.g. `NIKON D800E (Nikon AF-S NIKKOR 24-70mm
    /// f2.8G ED) - RAW.lcp`). A path in `name` is ignored.
    pub fn find_by_filename(&self, name: &str) -> Option<&Path> {
        let name = name.rsplit(['/', '\\']).next().unwrap_or(name).trim();
        if name.is_empty() {
            return None;
        }
        let want = name.to_lowercase();
        self.files
            .iter()
            .find(|p| {
                p.file_name()
                    .is_some_and(|f| f.to_string_lossy().to_lowercase() == want)
            })
            .map(PathBuf::as_path)
    }

    /// Parse a profile (cached).
    pub fn load(&self, path: &Path) -> Result<Arc<LcpFile>, Error> {
        if let Ok(cache) = self.cache.lock() {
            if let Some(f) = cache.get(path) {
                return Ok(f.clone());
            }
        }
        let file = Arc::new(LcpFile::parse(path)?);
        if let Ok(mut cache) = self.cache.lock() {
            cache.insert(path.to_path_buf(), file.clone());
        }
        Ok(file)
    }

    /// The best profile for the shot's lens. Candidates are prefiltered
    /// cheaply — by file name, else by file text — on the lens name's
    /// distinctive tokens (its focal length), then parsed and scored.
    pub fn find(&self, shot: &ShotInfo) -> Option<LcpMatch> {
        let lens_model = shot.lens_model.as_deref()?;
        let makers = names::maker_words(
            shot.lens_make
                .as_deref()
                .into_iter()
                .chain([shot.camera_make.as_str()]),
        );
        let tokens: Vec<String> = names::tokens(lens_model)
            .into_iter()
            .filter(|t| !makers.contains(t))
            .collect();
        let mut needles: Vec<String> = tokens
            .iter()
            .filter_map(|t| t.strip_suffix("mm").filter(|_| names::is_focal(t)))
            .map(str::to_string)
            .collect();
        if needles.is_empty() {
            needles.extend(
                tokens
                    .iter()
                    .filter(|t| t.chars().count() >= 3)
                    .max_by_key(|t| t.chars().count())
                    .cloned(),
            );
        }
        if needles.is_empty() {
            return None;
        }
        let has_all = |hay: &str| needles.iter().all(|n| hay.contains(n.as_str()));
        let mut candidates: Vec<&PathBuf> = self
            .files
            .iter()
            .filter(|p| {
                p.file_name()
                    .is_some_and(|f| has_all(&f.to_string_lossy().to_lowercase()))
            })
            .collect();
        if candidates.is_empty() {
            candidates = self
                .files
                .iter()
                .filter(|p| {
                    std::fs::read(p)
                        .is_ok_and(|b| has_all(&String::from_utf8_lossy(&b).to_lowercase()))
                })
                .collect();
        }
        let mut best: Option<LcpMatch> = None;
        for path in candidates {
            let Ok(file) = self.load(path) else {
                continue;
            };
            let Some(score) = file.score(shot) else {
                continue;
            };
            if best.as_ref().is_none_or(|b| score > b.score) {
                best = Some(LcpMatch { file, score });
            }
        }
        best
    }
}

fn collect_lcp(dir: &Path, out: &mut Vec<PathBuf>, depth: usize) {
    // Guard against symlink loops.
    if depth > 16 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_lcp(&path, out, depth + 1);
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("lcp"))
        {
            out.push(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apex() {
        assert!((apex_to_f_number(4.0) - 4.0).abs() < 1e-12);
        assert!((apex_to_f_number(3.0) - 2.828_427_124_746_19).abs() < 1e-12);
        assert!((apex_to_f_number(0.0) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn prime_focal_from_name() {
        assert_eq!(prime_focal("Sigma 35mm F1.4 DG HSM"), Some(35.0));
        assert_eq!(prime_focal("Nikon 24-70mm f/2.8"), None);
        assert_eq!(prime_focal("Fixed lens"), None);
    }
}
