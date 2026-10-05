//! The lensfun database: cameras, lenses and their calibration data.
//!
//! We read the user's installed database (or a directory they point us at);
//! the data is CC BY-SA 3.0 and is not shipped with Ambrotype. Lens lookup is
//! fuzzy like lensfun's own, calibration sets are chosen by crop factor, and
//! the models are interpolated to the shot (see [`LensfunDb::correction`]).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::interp::{self, DEFAULT_DISTANCE_M, VigPoint};
use crate::model::{Correction, Distortion, Frame, Source, Tca, Vignetting};
use crate::names;
use crate::xml::Element;
use crate::{Error, ShotInfo};

/// A camera body from the database.
#[derive(Debug, Clone, PartialEq)]
pub struct Camera {
    /// Maker names: the default first, then language variants.
    pub makers: Vec<String>,
    /// Model names: the default first, then language variants.
    pub models: Vec<String>,
    pub mount: Option<String>,
    pub crop_factor: Option<f64>,
}

impl Camera {
    pub fn maker(&self) -> &str {
        self.makers.first().map_or("", String::as_str)
    }

    pub fn model(&self) -> &str {
        self.models.first().map_or("", String::as_str)
    }
}

/// A lens model. Several `<lens>` blocks with the same maker and model (one
/// per calibration crop factor) are merged into one.
#[derive(Debug, Clone, PartialEq)]
pub struct Lens {
    pub makers: Vec<String>,
    pub models: Vec<String>,
    pub mounts: Vec<String>,
    /// Projection: `rectilinear` (the default), `fisheye`, `equisolid`, …
    pub lens_type: String,
    pub calibrations: Vec<Calibration>,
    /// Name tokens of each model variant.
    tokens: Vec<Vec<String>>,
}

impl Lens {
    pub fn maker(&self) -> &str {
        self.makers.first().map_or("", String::as_str)
    }

    pub fn model(&self) -> &str {
        self.models.first().map_or("", String::as_str)
    }

    /// The calibration to use for an image of this crop factor: like lensfun,
    /// the largest calibration crop that is not larger than the image's (2 %
    /// tolerance); failing that, the smallest.
    pub fn calibration_for(&self, image_crop: f64) -> Option<&Calibration> {
        let fits = self
            .calibrations
            .iter()
            .filter(|c| c.crop_factor <= image_crop * 1.02)
            .max_by(|a, b| a.crop_factor.total_cmp(&b.crop_factor));
        fits.or_else(|| {
            self.calibrations
                .iter()
                .min_by(|a, b| a.crop_factor.total_cmp(&b.crop_factor))
        })
    }
}

/// One calibration set: the models measured on one sensor size.
#[derive(Debug, Clone, PartialEq)]
pub struct Calibration {
    pub crop_factor: f64,
    /// Long / short side of the calibration sensor.
    pub aspect_ratio: f64,
    /// Only rectilinear lenses get distortion correction.
    pub rectilinear: bool,
    /// (focal length, model), sorted by focal length.
    pub distortion: Vec<(f64, Distortion)>,
    pub tca: Vec<(f64, Tca)>,
    pub vignetting: Vec<VignettingSample>,
    /// The XML file the calibration came from.
    pub file: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VignettingSample {
    pub focal: f64,
    /// f-number.
    pub aperture: f64,
    /// Focus distance in metres.
    pub distance: f64,
    pub model: Vignetting,
}

/// A lens found for a shot, with the calibration chosen for its crop factor.
#[derive(Debug, Clone, Copy)]
pub struct LensMatch<'a> {
    pub lens: &'a Lens,
    pub calibration: &'a Calibration,
    /// Name-match score in 0..=1 (1 for a fixed-lens camera's own lens).
    pub score: f64,
    /// Crop factor of the image the calibration was chosen for.
    pub image_crop: f64,
}

#[derive(Debug, Clone, Default)]
pub struct LensfunDb {
    pub cameras: Vec<Camera>,
    pub lenses: Vec<Lens>,
    /// Files that loaded.
    pub files: Vec<PathBuf>,
    /// Files that failed to load, with the reason.
    pub failed: Vec<(PathBuf, String)>,
    /// Mount name → mounts whose lenses fit it.
    compat: HashMap<String, Vec<String>>,
    /// (maker, model) lowercased → index into `lenses`.
    lens_index: HashMap<(String, String), usize>,
}

impl LensfunDb {
    /// Load every `*.xml` file in a directory (not recursive). Files that fail
    /// to parse are skipped and listed in [`LensfunDb::failed`].
    pub fn load_dir(dir: impl AsRef<Path>) -> Result<LensfunDb, Error> {
        Ok(LensfunDb::load_files(xml_files(dir.as_ref())?))
    }

    /// Load the given files; failures are listed in [`LensfunDb::failed`].
    pub fn load_files<P: AsRef<Path>>(paths: impl IntoIterator<Item = P>) -> LensfunDb {
        let mut db = LensfunDb::default();
        for path in paths {
            let path = path.as_ref();
            let result = std::fs::read(path)
                .map_err(Error::from)
                .and_then(|bytes| db.load_str(&String::from_utf8_lossy(&bytes), path));
            match result {
                Ok(()) => db.files.push(path.to_path_buf()),
                Err(e) => db.failed.push((path.to_path_buf(), e.to_string())),
            }
        }
        db
    }

    /// Add the contents of one database file. `path` is recorded as the source.
    pub fn load_str(&mut self, xml: &str, path: &Path) -> Result<(), Error> {
        let root = Element::parse(xml).map_err(|message| Error::Parse {
            path: path.display().to_string(),
            message,
        })?;
        if root.name != "lensdatabase" {
            return Err(Error::Parse {
                path: path.display().to_string(),
                message: format!("expected <lensdatabase>, found <{}>", root.name),
            });
        }
        for el in &root.children {
            match el.name.as_str() {
                "mount" => self.add_mount(el),
                "camera" => {
                    if let Some(camera) = parse_camera(el) {
                        self.cameras.push(camera);
                    }
                }
                "lens" => {
                    if let Some(lens) = parse_lens(el, path) {
                        self.add_lens(lens);
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Where lensfun databases are usually installed, most specific first:
    /// `AMB_LENSFUN_DB`, the user's and system's lensfun-update directories,
    /// then the distribution's database.
    pub fn default_dirs() -> Vec<PathBuf> {
        let mut dirs = Vec::new();
        if let Some(p) = std::env::var_os("AMB_LENSFUN_DB").filter(|p| !p.is_empty()) {
            dirs.push(PathBuf::from(p));
        }
        if let Some(home) = home_dir() {
            dirs.push(home.join(".local/share/lensfun/updates/version_2"));
        }
        for d in [
            "/var/lib/lensfun-updates/version_2",
            "/usr/share/lensfun/version_2",
            "/usr/local/share/lensfun/version_2",
            "/opt/homebrew/share/lensfun/version_2",
            "/usr/share/lensfun",
            "/usr/local/share/lensfun",
        ] {
            dirs.push(PathBuf::from(d));
        }
        if cfg!(windows) {
            for var in ["PROGRAMDATA", "LOCALAPPDATA"] {
                if let Some(base) = std::env::var_os(var) {
                    dirs.push(PathBuf::from(base).join("lensfun").join("version_2"));
                }
            }
        }
        dirs
    }

    /// Load the first of [`LensfunDb::default_dirs`] that has XML files
    /// (`AMB_LENSFUN_DB` may also name a single file).
    pub fn load_default() -> Option<LensfunDb> {
        for dir in LensfunDb::default_dirs() {
            if dir.is_file() {
                return Some(LensfunDb::load_files([dir]));
            }
            match xml_files(&dir) {
                Ok(files) if !files.is_empty() => return Some(LensfunDb::load_files(files)),
                _ => {}
            }
        }
        None
    }

    fn add_mount(&mut self, el: &Element) {
        let Some(name) = el.child("name").map(|n| n.text().to_string()) else {
            return;
        };
        let compat = self.compat.entry(name).or_default();
        for c in el.children_named("compat") {
            compat.push(c.text().to_string());
        }
    }

    fn add_lens(&mut self, lens: Lens) {
        let key = (lens.maker().to_lowercase(), lens.model().to_lowercase());
        let Some(&i) = self.lens_index.get(&key) else {
            self.lens_index.insert(key, self.lenses.len());
            self.lenses.push(lens);
            return;
        };
        let existing = &mut self.lenses[i];
        for m in lens.mounts {
            if !existing.mounts.contains(&m) {
                existing.mounts.push(m);
            }
        }
        for c in lens.calibrations {
            push_calibration(&mut existing.calibrations, c);
        }
    }

    /// Camera by EXIF make and model, case-insensitive, matching any name
    /// variant and tolerating the maker in front of the model.
    pub fn camera(&self, make: &str, model: &str) -> Option<&Camera> {
        let make = make.trim();
        let model = model.trim();
        if model.is_empty() {
            return None;
        }
        self.cameras.iter().find(|cam| {
            let maker_ok = make.is_empty()
                || cam
                    .makers
                    .iter()
                    .any(|m| m.eq_ignore_ascii_case(make) || names::same_maker(m, make));
            if !maker_ok {
                return false;
            }
            let mut prefixes: Vec<&str> = cam.makers.iter().map(String::as_str).collect();
            prefixes.push(make);
            let shot = strip_maker(model, &prefixes);
            cam.models.iter().any(|m| strip_maker(m, &prefixes) == shot)
        })
    }

    /// The crop factor the database gives for a camera.
    pub fn camera_crop(&self, make: &str, model: &str) -> Option<f64> {
        self.camera(make, model).and_then(|c| c.crop_factor)
    }

    fn mount_fits(&self, camera_mount: &str, lens_mount: &str) -> bool {
        camera_mount.eq_ignore_ascii_case(lens_mount)
            || self
                .compat
                .get(camera_mount)
                .is_some_and(|c| c.iter().any(|m| m.eq_ignore_ascii_case(lens_mount)))
    }

    /// Find the lens for a shot by fuzzy name match, and its calibration for
    /// the shot's crop factor. Fixed-lens cameras match their own lens.
    pub fn find_lens(&self, shot: &ShotInfo) -> Option<LensMatch<'_>> {
        let camera = self.camera(&shot.camera_make, &shot.camera_model);
        let image_crop = shot
            .crop_factor
            .filter(|c| c.is_finite() && *c > 0.0)
            .or(camera.and_then(|c| c.crop_factor))
            .unwrap_or(1.0);
        let camera_mount = camera.and_then(|c| c.mount.as_deref());

        let mut best: Option<(&Lens, f64)> = None;
        if let Some(lens_model) = shot.lens_model.as_deref().filter(|m| !m.trim().is_empty()) {
            // The lens make is a maker word, dropped on both sides, so "make + model"
            // compares the same as the model alone.
            let shot_tokens = names::tokens(lens_model);
            for lens in &self.lenses {
                let makers = names::maker_words(
                    lens.makers
                        .iter()
                        .map(String::as_str)
                        .chain(shot.lens_make.as_deref())
                        .chain([shot.camera_make.as_str()]),
                );
                let Some(mut score) = lens
                    .tokens
                    .iter()
                    .filter_map(|t| names::score(t, &shot_tokens, &makers))
                    .max_by(f64::total_cmp)
                else {
                    continue;
                };
                if let Some(make) = shot.lens_make.as_deref().filter(|m| !m.trim().is_empty()) {
                    if !lens.makers.iter().any(|m| names::same_maker(m, make)) {
                        score *= 0.9;
                    }
                }
                if let Some(mount) = camera_mount {
                    if !lens.mounts.is_empty()
                        && !lens.mounts.iter().any(|m| self.mount_fits(mount, m))
                    {
                        score *= 0.9;
                    }
                }
                if score >= names::MIN_SCORE && best.is_none_or(|(_, s)| score > s) {
                    best = Some((lens, score));
                }
            }
        }
        // Compact cameras: their fixed lens has the camera's (lowercase) mount.
        if best.is_none() {
            if let Some(mount) = camera_mount.filter(|m| m.starts_with(|c: char| c.is_lowercase()))
            {
                best = self
                    .lenses
                    .iter()
                    .find(|l| l.mounts.iter().any(|m| m == mount))
                    .map(|l| (l, 1.0));
            }
        }
        let (lens, score) = best?;
        Some(LensMatch {
            lens,
            calibration: lens.calibration_for(image_crop)?,
            score,
            image_crop,
        })
    }

    /// Interpolate the matched calibration to the shot. Distortion and TCA are
    /// linear in focal length between the bracketing calibrated focals (clamped
    /// outside); vignetting also needs the aperture and uses the calibrated
    /// focus distance nearest the shot's (infinity when unknown).
    pub fn correction(&self, m: &LensMatch<'_>, shot: &ShotInfo) -> Correction {
        let cal = m.calibration;
        let hugin = Frame::HuginShortSide {
            cal_crop: cal.crop_factor,
            cal_aspect: cal.aspect_ratio,
        };
        let distortion = cal
            .rectilinear
            .then(|| {
                let focal = interp::pick_key(cal.distortion.iter().map(|e| e.0), shot.focal_mm)?;
                interp::interpolate(&cal.distortion, focal, |a, b, t| a.lerp(b, t))
            })
            .flatten();
        let tca = interp::pick_key(cal.tca.iter().map(|e| e.0), shot.focal_mm)
            .and_then(|focal| interp::interpolate(&cal.tca, focal, |a, b, t| a.lerp(b, t)));
        let vignetting = shot.aperture.and_then(|aperture| {
            let points: Vec<VigPoint<Vignetting>> = cal
                .vignetting
                .iter()
                .map(|v| VigPoint {
                    focal: v.focal,
                    aperture: v.aperture,
                    distance: v.distance,
                    value: v.model,
                })
                .collect();
            let distance = shot
                .distance_m
                .filter(|d| d.is_finite() && *d > 0.0)
                .unwrap_or(DEFAULT_DISTANCE_M);
            interp::interpolate_vignetting(&points, shot.focal_mm, aperture, distance, |a, b, t| {
                a.lerp(b, t)
            })
        });
        Correction {
            source: Source::Lensfun,
            description: format!(
                "lensfun: {} (calibrated at crop {})",
                m.lens.model(),
                fmt_crop(cal.crop_factor)
            ),
            file: Some(cal.file.display().to_string()),
            distortion: distortion.map(|d| (d, hugin)),
            tca: tca.map(|t| (t, hugin)),
            vignetting: vignetting.map(|v| {
                (
                    v,
                    Frame::HalfDiagonal {
                        cal_crop: cal.crop_factor,
                    },
                )
            }),
        }
    }
}

/// `1` → `1.0`, `1.613` → `1.613`.
pub(crate) fn fmt_crop(c: f64) -> String {
    if c.fract() == 0.0 {
        format!("{c:.1}")
    } else {
        format!("{c}")
    }
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .filter(|h| !h.is_empty())
        .map(PathBuf::from)
}

fn xml_files(dir: &Path) -> Result<Vec<PathBuf>, Error> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| e.eq_ignore_ascii_case("xml"))
        })
        .collect();
    files.sort();
    Ok(files)
}

/// Lowercase, collapse whitespace and drop a leading maker name.
fn strip_maker(name: &str, makers: &[&str]) -> String {
    let norm = |s: &str| {
        s.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    };
    let name = norm(name);
    for maker in makers {
        let maker = norm(maker);
        let first = maker.split(' ').next().unwrap_or_default().to_string();
        for prefix in [maker, first] {
            if prefix.is_empty() {
                continue;
            }
            if let Some(rest) = name.strip_prefix(&prefix) {
                if let Some(rest) = rest.strip_prefix(' ') {
                    return rest.to_string();
                }
            }
        }
    }
    name
}

fn texts(el: &Element, name: &str) -> Vec<String> {
    // The default (no `lang`) variant first.
    let mut out: Vec<(bool, String)> = el
        .children_named(name)
        .map(|c| (c.attr("lang").is_some(), c.text().to_string()))
        .filter(|(_, t)| !t.is_empty())
        .collect();
    out.sort_by_key(|(lang, _)| *lang);
    let mut names: Vec<String> = Vec::new();
    for (_, t) in out {
        if !names.contains(&t) {
            names.push(t);
        }
    }
    names
}

fn number(s: &str) -> Option<f64> {
    s.trim().parse::<f64>().ok().filter(|v| v.is_finite())
}

/// `"4:3"` → 4/3, `"1.5"` → 1.5; always ≥ 1 (long / short).
fn aspect_ratio(s: &str) -> Option<f64> {
    let v = match s.split_once(':') {
        Some((a, b)) => number(a)? / number(b)?,
        None => number(s)?,
    };
    (v.is_finite() && v > 0.0).then(|| if v < 1.0 { 1.0 / v } else { v })
}

fn parse_camera(el: &Element) -> Option<Camera> {
    let makers = texts(el, "maker");
    let models = texts(el, "model");
    if models.is_empty() {
        return None;
    }
    Some(Camera {
        makers,
        models,
        mount: el.child("mount").map(|m| m.text().to_string()),
        crop_factor: el.child("cropfactor").and_then(|c| number(c.text())),
    })
}

fn parse_lens(el: &Element, path: &Path) -> Option<Lens> {
    let makers = texts(el, "maker");
    let models = texts(el, "model");
    if models.is_empty() {
        return None;
    }
    let lens_type = el
        .child("type")
        .and_then(|t| t.text().split_whitespace().next().map(str::to_lowercase))
        .unwrap_or_else(|| "rectilinear".to_string());
    let crop = el
        .child("cropfactor")
        .and_then(|c| number(c.text()))
        .filter(|c| *c > 0.0)
        .unwrap_or(1.0);
    let aspect = el
        .child("aspect-ratio")
        .and_then(|a| aspect_ratio(a.text()))
        .unwrap_or(1.5);
    let mut calibrations = Vec::new();
    for cal in el.children_named("calibration") {
        let c = parse_calibration(
            cal,
            cal.attr("cropfactor")
                .and_then(number)
                .filter(|c| *c > 0.0)
                .unwrap_or(crop),
            cal.attr("aspect-ratio")
                .and_then(aspect_ratio)
                .unwrap_or(aspect),
            lens_type == "rectilinear",
            path,
        );
        push_calibration(&mut calibrations, c);
    }
    let tokens = models.iter().map(|m| names::tokens(m)).collect();
    Some(Lens {
        makers,
        mounts: el
            .children_named("mount")
            .map(|m| m.text().to_string())
            .collect(),
        models,
        lens_type,
        calibrations,
        tokens,
    })
}

/// Add a calibration, merging it into an existing one for the same sensor.
fn push_calibration(list: &mut Vec<Calibration>, c: Calibration) {
    let same = list.iter_mut().find(|e| {
        (e.crop_factor - c.crop_factor).abs() < 1e-3
            && (e.aspect_ratio - c.aspect_ratio).abs() < 1e-3
            && e.rectilinear == c.rectilinear
    });
    match same {
        Some(e) => {
            e.distortion.extend(c.distortion);
            e.tca.extend(c.tca);
            e.vignetting.extend(c.vignetting);
            sort_calibration(e);
        }
        None => list.push(c),
    }
}

fn sort_calibration(c: &mut Calibration) {
    c.distortion.sort_by(|a, b| a.0.total_cmp(&b.0));
    c.tca.sort_by(|a, b| a.0.total_cmp(&b.0));
    c.vignetting.sort_by(|a, b| {
        a.focal
            .total_cmp(&b.focal)
            .then(a.aperture.total_cmp(&b.aperture))
            .then(a.distance.total_cmp(&b.distance))
    });
}

fn parse_calibration(
    el: &Element,
    crop_factor: f64,
    aspect_ratio: f64,
    rectilinear: bool,
    path: &Path,
) -> Calibration {
    let mut c = Calibration {
        crop_factor,
        aspect_ratio,
        rectilinear,
        distortion: Vec::new(),
        tca: Vec::new(),
        vignetting: Vec::new(),
        file: path.to_path_buf(),
    };
    for e in &el.children {
        let num = |name: &str, default: f64| e.attr(name).and_then(number).unwrap_or(default);
        let Some(focal) = e.attr("focal").and_then(number) else {
            continue;
        };
        match (e.name.as_str(), e.attr("model").unwrap_or_default()) {
            ("distortion", "poly3") => c
                .distortion
                .push((focal, Distortion::Poly3 { k1: num("k1", 0.0) })),
            ("distortion", "poly5") => c.distortion.push((
                focal,
                Distortion::Poly5 {
                    k1: num("k1", 0.0),
                    k2: num("k2", 0.0),
                },
            )),
            ("distortion", "ptlens") => c.distortion.push((
                focal,
                Distortion::PtLens {
                    a: num("a", 0.0),
                    b: num("b", 0.0),
                    c: num("c", 0.0),
                },
            )),
            ("tca", "linear") => c.tca.push((
                focal,
                Tca::Linear {
                    kr: num("kr", 1.0),
                    kb: num("kb", 1.0),
                },
            )),
            ("tca", "poly3") => c.tca.push((
                focal,
                Tca::Poly3 {
                    vr: num("vr", 1.0),
                    vb: num("vb", 1.0),
                    cr: num("cr", 0.0),
                    cb: num("cb", 0.0),
                    br: num("br", 0.0),
                    bb: num("bb", 0.0),
                },
            )),
            ("vignetting", "pa") => {
                let Some(aperture) = e.attr("aperture").and_then(number).filter(|a| *a > 0.0)
                else {
                    continue;
                };
                c.vignetting.push(VignettingSample {
                    focal,
                    aperture,
                    distance: num("distance", DEFAULT_DISTANCE_M),
                    model: Vignetting::Pa {
                        k1: num("k1", 0.0),
                        k2: num("k2", 0.0),
                        k3: num("k3", 0.0),
                    },
                });
            }
            // Unknown or unsupported models (e.g. lensfun's own `acm`).
            _ => {}
        }
    }
    sort_calibration(&mut c);
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aspect_ratio_forms() {
        assert!((aspect_ratio("4:3").unwrap() - 4.0 / 3.0).abs() < 1e-12);
        assert_eq!(aspect_ratio("1.5"), Some(1.5));
        assert_eq!(aspect_ratio("2:3"), Some(1.5));
        assert_eq!(aspect_ratio("x"), None);
    }

    #[test]
    fn strips_maker_prefix() {
        assert_eq!(strip_maker("Nikon  D750", &["Nikon Corporation"]), "d750");
        assert_eq!(strip_maker("D750", &["Nikon"]), "d750");
        assert_eq!(fmt_crop(1.0), "1.0");
        assert_eq!(fmt_crop(1.613), "1.613");
    }
}
