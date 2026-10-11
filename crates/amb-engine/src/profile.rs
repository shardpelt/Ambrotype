// SPDX-License-Identifier: GPL-3.0-or-later
//! Camera profiles: DNG Camera Profile (`.dcp`) files, read in place from
//! where Camera Raw installs them (and from folders the user names), never
//! copied or shipped (docs/design/camera-profiles.md).
//!
//! What a profile holds and how it is applied follows the DNG
//! specification's camera profile chapter, written here from the
//! specification:
//!
//! - **Colour matrices** for one or two calibration illuminants, mixed by
//!   the inverse of the white balance's colour temperature;
//! - **forward matrices** (white-balanced camera RGB → XYZ D50), used
//!   instead of inverting the colour matrix when present;
//! - a **hue/saturation/value map** (two are blended by temperature, as
//!   the DNG SDK does) and a **look table**, applied in linear ProPhoto HSV;
//! - a **tone curve** (linear in, linear out), which replaces our own base
//!   look; and a **baseline exposure offset**.

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use amb_color::matrix::{
    D50_XYZ, Mat3, PROPHOTO_TO_XYZ_D50, apply3, bradford, invert3, mul3, xy_to_xyz,
};
use amb_color::temperature::xy_to_temp_tint;

/// A camera profile as read from a `.dcp` file.
#[derive(Debug, Clone, PartialEq)]
pub struct Dcp {
    /// `ProfileName`, e.g. "Adobe Standard" or "Camera Neutral".
    pub name: String,
    /// `UniqueCameraModel`, e.g. "Nikon D750".
    pub camera: String,
    /// (illuminant temperature in kelvin, colour matrix, forward matrix).
    pub illuminants: Vec<(f64, Mat3, Option<Mat3>)>,
    /// `ProfileHueSatMapData1/2`, each with its illuminant's temperature,
    /// coolest first; see [`Dcp::hue_sat_map`].
    pub hue_sat_maps: Vec<(f64, HsvTable)>,
    pub look_table: Option<HsvTable>,
    pub tone_curve: Option<ToneCurve>,
    /// `BaselineExposureOffset`, in stops.
    pub exposure_offset: f64,
    pub copyright: Option<String>,
}

/// A hue/saturation/value table (`ProfileHueSatMap*`, `ProfileLookTable*`).
#[derive(Debug, Clone, PartialEq)]
pub struct HsvTable {
    pub hues: usize,
    pub sats: usize,
    pub vals: usize,
    /// (hue shift in degrees, saturation scale, value scale), value-major,
    /// then hue, then saturation.
    pub data: Vec<[f32; 3]>,
    /// Look the value up on sRGB-encoded values (encoding 1) or linear (0).
    pub srgb_values: bool,
}

/// `ProfileToneCurve`: linear in, linear out, as a lookup table.
#[derive(Debug, Clone, PartialEq)]
pub struct ToneCurve {
    lut: Vec<f32>,
}

// ---------------------------------------------------------------- reading

const UNIQUE_CAMERA_MODEL: u16 = 50708;
const COLOR_MATRIX_1: u16 = 50721;
const COLOR_MATRIX_2: u16 = 50722;
const CALIBRATION_ILLUMINANT_1: u16 = 50778;
const CALIBRATION_ILLUMINANT_2: u16 = 50779;
const PROFILE_NAME: u16 = 50936;
const HUE_SAT_MAP_DIMS: u16 = 50937;
const HUE_SAT_MAP_DATA_1: u16 = 50938;
const HUE_SAT_MAP_DATA_2: u16 = 50939;
const TONE_CURVE: u16 = 50940;
const PROFILE_COPYRIGHT: u16 = 50942;
const FORWARD_MATRIX_1: u16 = 50964;
const FORWARD_MATRIX_2: u16 = 50965;
const LOOK_TABLE_DIMS: u16 = 50981;
const LOOK_TABLE_DATA: u16 = 50982;
const HUE_SAT_MAP_ENCODING: u16 = 51107;
const LOOK_TABLE_ENCODING: u16 = 51108;
const BASELINE_EXPOSURE_OFFSET: u16 = 51109;

/// One IFD entry's values, decoded.
#[derive(Debug, Clone)]
enum Field {
    Text(String),
    Numbers(Vec<f64>),
}

struct Reader<'a> {
    b: &'a [u8],
    le: bool,
}

impl Reader<'_> {
    fn u16(&self, at: usize) -> Option<u16> {
        let s: [u8; 2] = self.b.get(at..at + 2)?.try_into().ok()?;
        Some(if self.le {
            u16::from_le_bytes(s)
        } else {
            u16::from_be_bytes(s)
        })
    }

    fn u32(&self, at: usize) -> Option<u32> {
        let s: [u8; 4] = self.b.get(at..at + 4)?.try_into().ok()?;
        Some(if self.le {
            u32::from_le_bytes(s)
        } else {
            u32::from_be_bytes(s)
        })
    }

    fn u64(&self, at: usize) -> Option<u64> {
        let s: [u8; 8] = self.b.get(at..at + 8)?.try_into().ok()?;
        Some(if self.le {
            u64::from_le_bytes(s)
        } else {
            u64::from_be_bytes(s)
        })
    }

    /// The IFD entry at `at`: its tag and values.
    fn entry(&self, at: usize) -> Option<(u16, Field)> {
        let tag = self.u16(at)?;
        let kind = self.u16(at + 2)?;
        let count = self.u32(at + 4)? as usize;
        let size: usize = match kind {
            1 | 2 | 6 | 7 => 1,
            3 | 8 => 2,
            4 | 9 | 11 => 4,
            5 | 10 | 12 => 8,
            _ => return None,
        };
        let total = size.checked_mul(count)?;
        if total > self.b.len() {
            return None;
        }
        let data = if total <= 4 {
            at + 8
        } else {
            self.u32(at + 8)? as usize
        };
        self.b.get(data..data + total)?;
        let field = match kind {
            2 => Field::Text(
                String::from_utf8_lossy(&self.b[data..data + total])
                    .trim_end_matches('\0')
                    .trim()
                    .to_string(),
            ),
            _ => Field::Numbers(
                (0..count)
                    .map(|i| {
                        let p = data + i * size;
                        match kind {
                            1 | 7 => f64::from(self.b[p]),
                            6 => f64::from(self.b[p] as i8),
                            3 => f64::from(self.u16(p).unwrap_or(0)),
                            8 => f64::from(self.u16(p).unwrap_or(0) as i16),
                            4 => f64::from(self.u32(p).unwrap_or(0)),
                            9 => f64::from(self.u32(p).unwrap_or(0) as i32),
                            11 => f64::from(f32::from_bits(self.u32(p).unwrap_or(0))),
                            12 => f64::from_bits(self.u64(p).unwrap_or(0)),
                            5 => {
                                let (n, d) =
                                    (self.u32(p).unwrap_or(0), self.u32(p + 4).unwrap_or(1));
                                if d == 0 {
                                    0.0
                                } else {
                                    f64::from(n) / f64::from(d)
                                }
                            }
                            _ => {
                                let n = self.u32(p).unwrap_or(0) as i32;
                                let d = self.u32(p + 4).unwrap_or(1) as i32;
                                if d == 0 {
                                    0.0
                                } else {
                                    f64::from(n) / f64::from(d)
                                }
                            }
                        }
                    })
                    .collect(),
            ),
        };
        Some((tag, field))
    }
}

/// Correlated colour temperature of an EXIF light source code (DNG
/// `CalibrationIlluminant`), in kelvin.
pub fn illuminant_temperature(code: u16) -> Option<f64> {
    Some(match code {
        17 | 3 => 2856.0,
        18 => 4874.0,
        19 => 6774.0,
        20 => 5503.0,
        21 | 1 | 4 | 9 => 6504.0,
        22 => 7504.0,
        23 => 5003.0,
        24 => 3200.0,
        10 => 6500.0,
        11 => 7500.0,
        12 => 6430.0,
        13 => 5000.0,
        14 => 4150.0,
        15 => 3525.0,
        2 => 4150.0,
        _ => return None,
    })
}

fn mat(v: &[f64]) -> Option<Mat3> {
    (v.len() >= 9).then(|| [[v[0], v[1], v[2]], [v[3], v[4], v[5]], [v[6], v[7], v[8]]])
}

fn table(dims: &[f64], data: &[f64], srgb: bool) -> Option<HsvTable> {
    let (h, s, v) = match dims {
        [h, s] => (*h as usize, *s as usize, 1),
        [h, s, v, ..] => (*h as usize, *s as usize, (*v as usize).max(1)),
        _ => return None,
    };
    let n = h.checked_mul(s)?.checked_mul(v)?;
    if h == 0 || s < 2 || data.len() < n * 3 {
        return None;
    }
    Some(HsvTable {
        hues: h,
        sats: s,
        vals: v,
        data: data
            .chunks_exact(3)
            .take(n)
            .map(|c| [c[0] as f32, c[1] as f32, c[2] as f32])
            .collect(),
        srgb_values: srgb,
    })
}

impl Dcp {
    /// Parse a `.dcp` file's bytes.
    pub fn parse(b: &[u8]) -> Result<Dcp, String> {
        let le = match b.get(..4) {
            Some(b"IIRC") => true,
            Some(b"MMCR") => false,
            // A DNG or TIFF with an embedded profile reads the same way.
            Some(b"II*\0") => true,
            Some(b"MM\0*") => false,
            _ => return Err("not a camera profile (.dcp)".into()),
        };
        let r = Reader { b, le };
        let ifd = r.u32(4).ok_or("truncated")? as usize;
        let n = r.u16(ifd).ok_or("truncated")? as usize;
        let mut f: HashMap<u16, Field> = HashMap::new();
        for i in 0..n {
            if let Some((tag, field)) = r.entry(ifd + 2 + i * 12) {
                f.insert(tag, field);
            }
        }
        let text = |t: u16| match f.get(&t) {
            Some(Field::Text(s)) => Some(s.clone()),
            _ => None,
        };
        let nums = |t: u16| match f.get(&t) {
            Some(Field::Numbers(v)) => Some(v.as_slice()),
            _ => None,
        };
        let first = |t: u16| nums(t).and_then(|v| v.first().copied());
        let hsm_srgb = first(HUE_SAT_MAP_ENCODING) == Some(1.0);
        let look_srgb = first(LOOK_TABLE_ENCODING) == Some(1.0);
        let mut illuminants = Vec::new();
        let mut hue_sat_maps = Vec::new();
        for (cm, fm, ill, map) in [
            (
                COLOR_MATRIX_1,
                FORWARD_MATRIX_1,
                CALIBRATION_ILLUMINANT_1,
                HUE_SAT_MAP_DATA_1,
            ),
            (
                COLOR_MATRIX_2,
                FORWARD_MATRIX_2,
                CALIBRATION_ILLUMINANT_2,
                HUE_SAT_MAP_DATA_2,
            ),
        ] {
            let Some(c) = nums(cm).and_then(mat) else {
                continue;
            };
            let temp = first(ill)
                .and_then(|code| illuminant_temperature(code as u16))
                .unwrap_or(5003.0);
            illuminants.push((temp, c, nums(fm).and_then(mat)));
            // Each map belongs to its illuminant.
            if let Some(t) = nums(HUE_SAT_MAP_DIMS).zip(nums(map)) {
                hue_sat_maps.extend(table(t.0, t.1, hsm_srgb).map(|t| (temp, t)));
            }
        }
        if illuminants.is_empty() {
            return Err("the profile has no colour matrix".into());
        }
        illuminants.sort_by(|a, b| a.0.total_cmp(&b.0));
        hue_sat_maps.sort_by(|a, b| a.0.total_cmp(&b.0));
        let tone_curve = nums(TONE_CURVE).and_then(ToneCurve::from_points);
        Ok(Dcp {
            name: text(PROFILE_NAME).unwrap_or_default(),
            camera: text(UNIQUE_CAMERA_MODEL).unwrap_or_default(),
            illuminants,
            hue_sat_maps,
            look_table: nums(LOOK_TABLE_DIMS)
                .zip(nums(LOOK_TABLE_DATA))
                .and_then(|(d, v)| table(d, v, look_srgb)),
            tone_curve,
            exposure_offset: first(BASELINE_EXPOSURE_OFFSET).unwrap_or(0.0),
            copyright: text(PROFILE_COPYRIGHT),
        })
    }

    pub fn read(path: &Path) -> Result<Dcp, String> {
        let b = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Dcp::parse(&b).map_err(|e| format!("{}: {e}", path.display()))
    }

    // ------------------------------------------------------------ colour

    /// The colour and forward matrices for a white of `temp` kelvin, mixed
    /// by inverse temperature between the two illuminants.
    pub fn matrices(&self, temp: f64) -> (Mat3, Option<Mat3>) {
        let (lo, hi) = (&self.illuminants[0], self.illuminants.last().expect("one"));
        if self.illuminants.len() == 1 || lo.0 == hi.0 {
            return (lo.1, lo.2);
        }
        let inv = |t: f64| 1.0 / t.max(1.0);
        let w = ((inv(temp) - inv(hi.0)) / (inv(lo.0) - inv(hi.0))).clamp(0.0, 1.0);
        let mix = |a: &Mat3, b: &Mat3| {
            let mut m = [[0.0; 3]; 3];
            for r in 0..3 {
                for c in 0..3 {
                    m[r][c] = w * a[r][c] + (1.0 - w) * b[r][c];
                }
            }
            m
        };
        let fm = match (&lo.2, &hi.2) {
            (Some(a), Some(b)) => Some(mix(a, b)),
            _ => None,
        };
        (mix(&lo.1, &hi.1), fm)
    }

    /// The hue/saturation map for a white of `temp` kelvin. With two maps,
    /// they are blended entry by entry with the cooler one's weight 1 at or
    /// below its temperature, 0 at or above the warmer one's, and linear in
    /// inverse temperature between, as the DNG SDK does.
    pub fn hue_sat_map(&self, temp: f64) -> Option<Cow<'_, HsvTable>> {
        let (lo, hi) = match self.hue_sat_maps.as_slice() {
            [] => return None,
            [(_, a), (_, b)] if a.same_shape(b) => (&self.hue_sat_maps[0], &self.hue_sat_maps[1]),
            [(_, a), ..] => return Some(Cow::Borrowed(a)),
        };
        if !(lo.0 > 0.0 && hi.0 > lo.0) {
            return Some(Cow::Borrowed(&lo.1));
        }
        let inv = |t: f64| 1.0 / t.max(1.0);
        let w = ((inv(temp) - inv(hi.0)) / (inv(lo.0) - inv(hi.0))).clamp(0.0, 1.0) as f32;
        if w >= 1.0 {
            return Some(Cow::Borrowed(&lo.1));
        }
        if w <= 0.0 {
            return Some(Cow::Borrowed(&hi.1));
        }
        let mut t = lo.1.clone();
        for (a, b) in t.data.iter_mut().zip(&hi.1.data) {
            for c in 0..3 {
                a[c] = w * a[c] + (1.0 - w) * b[c];
            }
        }
        Some(Cow::Owned(t))
    }

    /// The colour temperature of white balance `neutral`, in kelvin.
    pub fn white_temperature(&self, neutral: [f64; 3]) -> f64 {
        let (x, y) = self.xy_for_neutral(neutral);
        xy_to_temp_tint(x, y).temperature
    }

    /// Camera RGB of a white with chromaticity (x, y), green = 1.
    pub fn neutral_for_xy(&self, x: f64, y: f64) -> [f64; 3] {
        let temp = xy_to_temp_tint(x, y).temperature;
        let (cm, _) = self.matrices(temp);
        let n = apply3(&cm, xy_to_xyz(x, y));
        [n[0] / n[1], 1.0, n[2] / n[1]]
    }

    /// Chromaticity of a camera-space white: the matrices depend on it, so
    /// it is found by iteration, as the DNG specification describes.
    pub fn xy_for_neutral(&self, neutral: [f64; 3]) -> (f64, f64) {
        let (mut x, mut y) = (0.3457, 0.3585);
        for _ in 0..30 {
            let temp = xy_to_temp_tint(x, y).temperature;
            let (cm, _) = self.matrices(temp);
            let xyz = apply3(&invert3(&cm), neutral);
            let sum = xyz[0] + xyz[1] + xyz[2];
            if !(sum.is_finite() && sum > 0.0) {
                break;
            }
            let (nx, ny) = (xyz[0] / sum, xyz[1] / sum);
            let done = (nx - x).abs() + (ny - y).abs() < 1e-7;
            (x, y) = (nx, ny);
            if done {
                break;
            }
        }
        (x, y)
    }

    /// Camera RGB → linear ProPhoto for white balance `neutral`, scaled so
    /// the neutral maps to (1, 1, 1).
    pub fn camera_to_prophoto(&self, neutral: [f64; 3]) -> Mat3 {
        let (cm, fm) = self.matrices(self.white_temperature(neutral));
        let cam_to_xyz = match fm {
            // White-balanced camera RGB → XYZ D50.
            Some(fm) => mul3(
                &fm,
                &[
                    [1.0 / neutral[0], 0.0, 0.0],
                    [0.0, 1.0 / neutral[1], 0.0],
                    [0.0, 0.0, 1.0 / neutral[2]],
                ],
            ),
            None => {
                let to_xyz = invert3(&cm);
                let w = apply3(&to_xyz, neutral);
                let w = [w[0] / w[1], 1.0, w[2] / w[1]];
                mul3(&bradford(w, D50_XYZ), &to_xyz)
            }
        };
        let m = mul3(&invert3(&PROPHOTO_TO_XYZ_D50), &cam_to_xyz);
        let n = apply3(&m, neutral);
        let k = 1.0 / n[1];
        m.map(|row| row.map(|v| v * k))
    }
}

// ---------------------------------------------------------------- applying

fn srgb_encode(v: f32) -> f32 {
    if v <= 0.003_130_8 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    }
}

impl HsvTable {
    fn same_shape(&self, other: &HsvTable) -> bool {
        (self.hues, self.sats, self.vals, self.data.len())
            == (other.hues, other.sats, other.vals, other.data.len())
    }

    /// Apply to one linear ProPhoto pixel (DNG's HSV: hue 0..6, value = max).
    pub fn apply(&self, rgb: &mut [f32]) {
        let (r, g, b) = (rgb[0], rgb[1], rgb[2]);
        let v = r.max(g).max(b);
        if v <= 0.0 {
            return;
        }
        let lo = r.min(g).min(b);
        let delta = v - lo;
        let s = delta / v;
        let h = if delta <= 0.0 {
            0.0
        } else if v == r {
            ((g - b) / delta).rem_euclid(6.0)
        } else if v == g {
            (b - r) / delta + 2.0
        } else {
            (r - g) / delta + 4.0
        };
        // Look up (trilinear; hue wraps around).
        let hf = h * self.hues as f32 / 6.0;
        let h0 = hf.floor();
        let ht = hf - h0;
        let h0 = (h0 as isize).rem_euclid(self.hues as isize) as usize;
        let h1 = (h0 + 1) % self.hues;
        let sf = (s.clamp(0.0, 1.0)) * (self.sats - 1) as f32;
        let s0 = (sf.floor() as usize).min(self.sats - 2);
        let st = sf - s0 as f32;
        let (v0, v1, vt) = if self.vals > 1 {
            let vv = if self.srgb_values {
                srgb_encode(v.min(1.0))
            } else {
                v.min(1.0)
            };
            let vf = vv * (self.vals - 1) as f32;
            let v0 = (vf.floor() as usize).min(self.vals - 2);
            (v0, v0 + 1, vf - v0 as f32)
        } else {
            (0, 0, 0.0)
        };
        let at =
            |vi: usize, hi: usize, si: usize| self.data[(vi * self.hues + hi) * self.sats + si];
        let mut e = [0.0f32; 3];
        for (vi, wv) in [(v0, 1.0 - vt), (v1, vt)] {
            if wv == 0.0 {
                continue;
            }
            for (hi, wh) in [(h0, 1.0 - ht), (h1, ht)] {
                for (si, ws) in [(s0, 1.0 - st), (s0 + 1, st)] {
                    let w = wv * wh * ws;
                    let d = at(vi, hi, si);
                    for c in 0..3 {
                        e[c] += w * d[c];
                    }
                }
            }
        }
        let h = (h + e[0] * 6.0 / 360.0).rem_euclid(6.0);
        let s = (s * e[1]).clamp(0.0, 1.0);
        let v = v * e[2];
        // Back to RGB.
        let i = (h.floor() as usize).min(5);
        let f = h - i as f32;
        let p = v * (1.0 - s);
        let q = v * (1.0 - s * f);
        let t = v * (1.0 - s * (1.0 - f));
        let out = match i {
            0 => [v, t, p],
            1 => [q, v, p],
            2 => [p, v, t],
            3 => [p, q, v],
            4 => [t, p, v],
            _ => [v, p, q],
        };
        rgb.copy_from_slice(&out);
    }
}

impl ToneCurve {
    const SIZE: usize = 4096;

    /// From `ProfileToneCurve`'s (input, output) pairs, linear 0..1.
    pub fn from_points(v: &[f64]) -> Option<ToneCurve> {
        let pts: Vec<(f64, f64)> = v.chunks_exact(2).map(|p| (p[0], p[1])).collect();
        if pts.len() < 2 || pts.windows(2).any(|w| w[1].0 <= w[0].0) {
            return None;
        }
        let lut = (0..Self::SIZE)
            .map(|i| {
                let x = i as f64 / (Self::SIZE - 1) as f64;
                let j = pts.partition_point(|p| p.0 <= x).clamp(1, pts.len() - 1);
                let (a, b) = (pts[j - 1], pts[j]);
                let t = ((x - a.0) / (b.0 - a.0)).clamp(0.0, 1.0);
                (a.1 + t * (b.1 - a.1)) as f32
            })
            .collect();
        Some(ToneCurve { lut })
    }

    /// One value through the curve (above 1 stays at the curve's end).
    #[inline]
    pub fn eval(&self, x: f32) -> f32 {
        let pos = x.clamp(0.0, 1.0) * (Self::SIZE - 1) as f32;
        let i = (pos as usize).min(Self::SIZE - 2);
        let f = pos - i as f32;
        self.lut[i] * (1.0 - f) + self.lut[i + 1] * f
    }

    /// The curve on an RGB pixel without shifting its hue: the largest and
    /// smallest channels go through the curve and the middle one keeps its
    /// place between them.
    pub fn apply_rgb(&self, c: [f32; 3]) -> [f32; 3] {
        let mut idx = [0usize, 1, 2];
        idx.sort_by(|a, b| c[*b].total_cmp(&c[*a]));
        let (hi, mid, lo) = (c[idx[0]], c[idx[1]], c[idx[2]]);
        let (hi2, lo2) = (self.eval(hi), self.eval(lo));
        let mid2 = if hi > lo {
            lo2 + (hi2 - lo2) * (mid - lo) / (hi - lo)
        } else {
            hi2
        };
        let mut out = [0.0; 3];
        out[idx[0]] = hi2;
        out[idx[1]] = mid2;
        out[idx[2]] = lo2;
        out
    }
}

// ---------------------------------------------------------------- finding

/// The camera profiles on this computer: `.dcp` files in the folders
/// Camera Raw installs them in and folders the user names.
#[derive(Debug, Default)]
pub struct ProfileLibrary {
    pub files: Vec<PathBuf>,
    /// path → (camera, name), read once.
    index: Mutex<HashMap<PathBuf, Option<(String, String)>>>,
    loaded: Mutex<HashMap<PathBuf, Arc<Dcp>>>,
}

/// A profile found for a camera.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Found {
    pub name: String,
    pub path: PathBuf,
}

/// Lower case, single spaces: how camera names are compared.
fn norm(s: &str) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Whether a profile's `UniqueCameraModel` names this camera.
pub fn same_camera(profile_camera: &str, make: &str, model: &str) -> bool {
    let p = norm(profile_camera);
    let (make, model) = (norm(make), norm(model));
    if p.is_empty() || model.is_empty() {
        return false;
    }
    let first = make.split(' ').next().unwrap_or("");
    p == model
        || p == format!("{make} {model}")
        || (!first.is_empty() && p == format!("{first} {model}"))
}

impl ProfileLibrary {
    /// `AMB_DCP_DIRS` (OS path-list syntax), then Camera Raw's profile
    /// folders.
    pub fn default_dirs() -> Vec<PathBuf> {
        let mut dirs: Vec<PathBuf> = std::env::var_os("AMB_DCP_DIRS")
            .map(|v| {
                std::env::split_paths(&v)
                    .filter(|p| !p.as_os_str().is_empty())
                    .collect()
            })
            .unwrap_or_default();
        const SUB: &str = "Adobe/CameraRaw/CameraProfiles";
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

    /// The `.dcp` files under `dirs`, recursively.
    pub fn scan(dirs: &[PathBuf]) -> ProfileLibrary {
        fn walk(dir: &Path, out: &mut Vec<PathBuf>, depth: usize) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() && depth < 8 {
                    walk(&p, out, depth + 1);
                } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("dcp")) {
                    out.push(p);
                }
            }
        }
        let mut files = Vec::new();
        for d in dirs {
            walk(d, &mut files, 0);
        }
        files.sort();
        files.dedup();
        ProfileLibrary {
            files,
            ..Default::default()
        }
    }

    pub fn load_default() -> ProfileLibrary {
        ProfileLibrary::scan(&ProfileLibrary::default_dirs())
    }

    fn header(&self, path: &Path) -> Option<(String, String)> {
        let mut index = self.index.lock().ok()?;
        index
            .entry(path.to_path_buf())
            .or_insert_with(|| Dcp::read(path).ok().map(|d| (d.camera, d.name)))
            .clone()
    }

    /// The profiles for a camera, by name.
    pub fn for_camera(&self, make: &str, model: &str) -> Vec<Found> {
        let mut out: Vec<Found> = self
            .files
            .iter()
            .filter_map(|p| {
                let (camera, name) = self.header(p)?;
                same_camera(&camera, make, model).then(|| Found {
                    name: if name.is_empty() {
                        p.file_stem()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned()
                    } else {
                        name
                    },
                    path: p.clone(),
                })
            })
            .collect();
        out.sort_by_key(|a| a.name.to_lowercase());
        out.dedup_by(|a, b| a.name.eq_ignore_ascii_case(&b.name));
        out
    }

    /// The profile called `name` for a camera, loaded.
    pub fn find(&self, make: &str, model: &str, name: &str) -> Option<Arc<Dcp>> {
        let found = self
            .for_camera(make, model)
            .into_iter()
            .find(|f| f.name.eq_ignore_ascii_case(name.trim()))?;
        let mut loaded = self.loaded.lock().ok()?;
        if let Some(d) = loaded.get(&found.path) {
            return Some(d.clone());
        }
        let d = Arc::new(Dcp::read(&found.path).ok()?);
        loaded.insert(found.path, d.clone());
        Some(d)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A little-endian `.dcp` with the given (tag, type, values) entries.
    pub(crate) fn write_dcp(entries: &[(u16, u16, Vec<f64>, Option<&str>)]) -> Vec<u8> {
        let mut entries: Vec<_> = entries.to_vec();
        entries.sort_by_key(|e| e.0);
        let mut out = b"IIRC".to_vec();
        out.extend(8u32.to_le_bytes());
        let n = entries.len();
        out.extend((n as u16).to_le_bytes());
        let mut data: Vec<u8> = Vec::new();
        let data_start = 8 + 2 + n * 12 + 4;
        let mut ifd = Vec::new();
        for (tag, kind, vals, text) in &entries {
            let bytes: Vec<u8> = match (kind, text) {
                (2, Some(t)) => {
                    let mut b = t.as_bytes().to_vec();
                    b.push(0);
                    b
                }
                (3, _) => vals
                    .iter()
                    .flat_map(|v| (*v as u16).to_le_bytes())
                    .collect(),
                (4, _) => vals
                    .iter()
                    .flat_map(|v| (*v as u32).to_le_bytes())
                    .collect(),
                (11, _) => vals
                    .iter()
                    .flat_map(|v| (*v as f32).to_le_bytes())
                    .collect(),
                (10, _) => vals
                    .iter()
                    .flat_map(|v| {
                        let mut b = ((*v * 10000.0).round() as i32).to_le_bytes().to_vec();
                        b.extend(10000i32.to_le_bytes());
                        b
                    })
                    .collect(),
                _ => unreachable!(),
            };
            let count = if *kind == 2 { bytes.len() } else { vals.len() };
            ifd.extend(tag.to_le_bytes());
            ifd.extend(kind.to_le_bytes());
            ifd.extend((count as u32).to_le_bytes());
            if bytes.len() <= 4 {
                let mut b = bytes.clone();
                b.resize(4, 0);
                ifd.extend(b);
            } else {
                ifd.extend(((data_start + data.len()) as u32).to_le_bytes());
                data.extend(&bytes);
                if data.len() % 2 == 1 {
                    data.push(0);
                }
            }
        }
        out.extend(ifd);
        out.extend(0u32.to_le_bytes());
        out.extend(data);
        out
    }

    pub(crate) const CM_A: [f64; 9] = [0.9, -0.3, -0.1, -0.4, 1.2, 0.2, -0.1, 0.2, 0.6];
    pub(crate) const CM_D65: [f64; 9] = [0.7, -0.2, -0.05, -0.45, 1.25, 0.2, -0.08, 0.15, 0.7];

    pub(crate) fn sample(extra: Vec<(u16, u16, Vec<f64>, Option<&'static str>)>) -> Vec<u8> {
        let mut e = vec![
            (UNIQUE_CAMERA_MODEL, 2, vec![], Some("Nikon D750")),
            (PROFILE_NAME, 2, vec![], Some("Test Standard")),
            (CALIBRATION_ILLUMINANT_1, 3, vec![17.0], None),
            (CALIBRATION_ILLUMINANT_2, 3, vec![21.0], None),
            (COLOR_MATRIX_1, 10, CM_A.to_vec(), None),
            (COLOR_MATRIX_2, 10, CM_D65.to_vec(), None),
        ];
        e.extend(extra);
        write_dcp(&e)
    }

    #[test]
    fn reads_names_matrices_and_tables() {
        // A 2×2 map that turns every hue by +30° and halves saturation.
        let hsm: Vec<f64> = (0..4).flat_map(|_| [30.0, 0.5, 1.0]).collect();
        let d = Dcp::parse(&sample(vec![
            (HUE_SAT_MAP_DIMS, 4, vec![2.0, 2.0, 1.0], None),
            (HUE_SAT_MAP_DATA_1, 11, hsm, None),
            (TONE_CURVE, 11, vec![0.0, 0.0, 0.5, 0.7, 1.0, 1.0], None),
            (BASELINE_EXPOSURE_OFFSET, 10, vec![-0.25], None),
        ]))
        .unwrap();
        assert_eq!(
            (d.name.as_str(), d.camera.as_str()),
            ("Test Standard", "Nikon D750")
        );
        assert_eq!(d.illuminants.len(), 2);
        assert_eq!(d.illuminants[0].0, 2856.0);
        assert!((d.illuminants[1].1[0][0] - 0.7).abs() < 1e-4);
        assert!((d.exposure_offset + 0.25).abs() < 1e-4);
        let t = d.tone_curve.as_ref().unwrap();
        assert!((t.eval(0.5) - 0.7).abs() < 1e-3 && (t.eval(0.25) - 0.35).abs() < 1e-3);
        let map = &d.hue_sat_maps[0].1;
        // Pure red → hue +30° (toward yellow), saturation halved.
        let mut px = [1.0f32, 0.0, 0.0];
        map.apply(&mut px);
        assert!(
            (px[0] - 1.0).abs() < 1e-5 && px[1] > px[2] && (px[2] - 0.5).abs() < 1e-4,
            "{px:?}"
        );
        // Greys stay grey.
        let mut grey = [0.4f32; 3];
        map.apply(&mut grey);
        assert!(grey.iter().all(|v| (v - 0.4).abs() < 1e-5), "{grey:?}");
        assert!(Dcp::parse(b"not a profile").is_err());
    }

    #[test]
    fn matrices_mix_by_temperature_and_white_stays_white() {
        let d = Dcp::parse(&sample(vec![])).unwrap();
        assert!((d.matrices(2856.0).0[0][0] - CM_A[0]).abs() < 1e-4);
        assert!((d.matrices(6504.0).0[0][0] - CM_D65[0]).abs() < 1e-4);
        let mid = d.matrices(4000.0).0[0][0];
        assert!(mid > CM_D65[0] && mid < CM_A[0]);
        // A white found from a neutral comes back to the same neutral.
        let n = d.neutral_for_xy(0.3457, 0.3585);
        let (x, y) = d.xy_for_neutral(n);
        assert!(
            (x - 0.3457).abs() < 1e-4 && (y - 0.3585).abs() < 1e-4,
            "{x} {y}"
        );
        let m = d.camera_to_prophoto(n);
        let w = apply3(&m, n);
        assert!(w.iter().all(|v| (v - 1.0).abs() < 1e-9), "{w:?}");
    }

    #[test]
    fn two_hue_sat_maps_blend_by_inverse_temperature() {
        // Map 1 (illuminant A, 2856 K) turns hues +20°, map 2 (D65) by −20°.
        // They are written warm-first to check each stays with its light.
        let map = |deg: f64| -> Vec<f64> { (0..4).flat_map(|_| [deg, 1.0, 1.0]).collect() };
        let d = Dcp::parse(&sample(vec![
            (HUE_SAT_MAP_DIMS, 4, vec![2.0, 2.0, 1.0], None),
            (HUE_SAT_MAP_DATA_1, 11, map(20.0), None),
            (HUE_SAT_MAP_DATA_2, 11, map(-20.0), None),
        ]))
        .unwrap();
        let shift = |temp: f64| d.hue_sat_map(temp).unwrap().data[0][0];
        assert_eq!(shift(2000.0), 20.0);
        assert_eq!(shift(2856.0), 20.0);
        assert_eq!(shift(9000.0), -20.0);
        // Halfway in inverse temperature is halfway between the maps.
        let mid = 2.0 / (1.0 / 2856.0 + 1.0 / 6504.0);
        assert!(shift(mid).abs() < 1e-3, "{}", shift(mid));
        // One map: used for every white.
        let one = Dcp::parse(&sample(vec![
            (HUE_SAT_MAP_DIMS, 4, vec![2.0, 2.0, 1.0], None),
            (HUE_SAT_MAP_DATA_1, 11, map(20.0), None),
        ]))
        .unwrap();
        assert_eq!(one.hue_sat_map(9000.0).unwrap().data[0][0], 20.0);
        assert!(
            Dcp::parse(&sample(vec![]))
                .unwrap()
                .hue_sat_map(5000.0)
                .is_none()
        );
    }

    #[test]
    fn rgb_tone_keeps_hue() {
        let t = ToneCurve::from_points(&[0.0, 0.0, 0.25, 0.4, 1.0, 1.0]).unwrap();
        let out = t.apply_rgb([0.5, 0.25, 0.1]);
        // Middle keeps its place between the others.
        let place = |c: [f32; 3]| (c[1] - c[2]) / (c[0] - c[2]);
        assert!(
            (place(out) - place([0.5, 0.25, 0.1])).abs() < 1e-5,
            "{out:?}"
        );
        assert!(ToneCurve::from_points(&[0.5, 0.0, 0.2, 1.0]).is_none());
    }

    #[test]
    fn library_finds_profiles_by_camera_and_name() {
        let dir = std::env::temp_dir().join(format!("amb-dcp-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Camera/Nikon D750")).unwrap();
        std::fs::write(dir.join("Camera/Nikon D750/test.dcp"), sample(vec![])).unwrap();
        std::fs::write(dir.join("broken.dcp"), b"nope").unwrap();
        let lib = ProfileLibrary::scan(std::slice::from_ref(&dir));
        assert_eq!(lib.files.len(), 2);
        let found = lib.for_camera("Nikon", "D750");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "Test Standard");
        assert!(
            lib.find("NIKON CORPORATION", "D750", "test standard")
                .is_some()
        );
        assert!(lib.find("Nikon", "D850", "Test Standard").is_none());
        assert!(same_camera("Fujifilm X-T3", "FUJIFILM", "X-T3"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
