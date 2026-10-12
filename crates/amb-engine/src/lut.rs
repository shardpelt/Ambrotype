// SPDX-License-Identifier: GPL-3.0-or-later
//! `.cube` look-up tables (the Resolve/Adobe text format), applied as a look
//! after the colour and curve stage (`amb:LUT`, `amb:LUTAmount`).
//!
//! A `.cube` file maps display-encoded RGB (sRGB/Rec.709 transfer, 0..1 by
//! default) to RGB. It may hold a 1D table, a 3D table, or both (a 1D shaper
//! first). Lightroom has no LUT setting, so this is our own (`amb:`) tag;
//! Lightroom ignores it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

/// Largest table sizes read (a 3D table holds size³ entries).
const MAX_3D: usize = 129;
const MAX_1D: usize = 65_536;

/// A parsed `.cube` file.
#[derive(Clone, PartialEq)]
pub struct CubeLut {
    pub title: Option<String>,
    /// Input range of the 1D and 3D tables.
    domain_1d: [[f32; 3]; 2],
    domain_3d: [[f32; 3]; 2],
    one_d: Vec<[f32; 3]>,
    size_3d: usize,
    /// Red changes fastest, then green, then blue.
    three_d: Vec<[f32; 3]>,
}

impl std::fmt::Debug for CubeLut {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CubeLut")
            .field("title", &self.title)
            .field("1d", &self.one_d.len())
            .field("3d", &self.size_3d)
            .finish()
    }
}

fn floats<const N: usize>(rest: &str, what: &str) -> Result<[f32; N], String> {
    let v: Vec<f32> = rest
        .split_whitespace()
        .map(|t| {
            t.parse::<f32>()
                .map_err(|_| format!("{what}: {t:?} isn't a number"))
        })
        .collect::<Result<_, _>>()?;
    v.try_into()
        .map_err(|_| format!("{what}: expected {N} numbers"))
}

impl CubeLut {
    /// Read a `.cube` file's text.
    pub fn parse(text: &str) -> Result<CubeLut, String> {
        let mut title = None;
        let (mut n1, mut n3) = (0usize, 0usize);
        let mut domain = [[0.0f32; 3], [1.0; 3]];
        let mut range_1d = None;
        let mut range_3d = None;
        let mut data: Vec<[f32; 3]> = Vec::new();
        for (i, raw) in text.lines().enumerate() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let at = |e: String| format!("line {}: {e}", i + 1);
            let (key, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
            match key {
                "TITLE" => title = Some(rest.trim().trim_matches('"').to_string()),
                "LUT_1D_SIZE" => {
                    n1 = rest
                        .trim()
                        .parse()
                        .map_err(|_| at("bad LUT_1D_SIZE".into()))?
                }
                "LUT_3D_SIZE" => {
                    n3 = rest
                        .trim()
                        .parse()
                        .map_err(|_| at("bad LUT_3D_SIZE".into()))?
                }
                "DOMAIN_MIN" => domain[0] = floats(rest, "DOMAIN_MIN").map_err(at)?,
                "DOMAIN_MAX" => domain[1] = floats(rest, "DOMAIN_MAX").map_err(at)?,
                "LUT_1D_INPUT_RANGE" => {
                    let [lo, hi] = floats(rest, "LUT_1D_INPUT_RANGE").map_err(at)?;
                    range_1d = Some([[lo; 3], [hi; 3]]);
                }
                "LUT_3D_INPUT_RANGE" => {
                    let [lo, hi] = floats(rest, "LUT_3D_INPUT_RANGE").map_err(at)?;
                    range_3d = Some([[lo; 3], [hi; 3]]);
                }
                k if k.starts_with(|c: char| {
                    c.is_ascii_digit() || c == '-' || c == '.' || c == '+'
                }) =>
                {
                    data.push(floats(line, "a table entry").map_err(at)?);
                }
                // Other keywords (e.g. LUT_IN_VIDEO_RANGE) don't change the table.
                _ => {}
            }
        }
        if n1 == 0 && n3 == 0 {
            return Err("no LUT_1D_SIZE or LUT_3D_SIZE".into());
        }
        if n1 > MAX_1D || (n1 > 0 && n1 < 2) {
            return Err(format!("LUT_1D_SIZE {n1} is out of range (2..{MAX_1D})"));
        }
        if n3 > MAX_3D || (n3 > 0 && n3 < 2) {
            return Err(format!("LUT_3D_SIZE {n3} is out of range (2..{MAX_3D})"));
        }
        let want = n1 + n3 * n3 * n3;
        if data.len() != want {
            return Err(format!("{} table entries, expected {want}", data.len()));
        }
        let three_d = data.split_off(n1);
        Ok(CubeLut {
            title,
            domain_1d: range_1d.unwrap_or(domain),
            domain_3d: range_3d.unwrap_or(domain),
            one_d: data,
            size_3d: n3,
            three_d,
        })
    }

    /// Map one display-encoded RGB value.
    pub fn apply(&self, rgb: [f32; 3]) -> [f32; 3] {
        let mut c = rgb;
        if !self.one_d.is_empty() {
            let n = self.one_d.len();
            for (ch, v) in c.iter_mut().enumerate() {
                let t = unit(*v, self.domain_1d, ch) * (n - 1) as f32;
                let i = (t.floor() as usize).min(n - 2);
                let f = t - i as f32;
                *v = self.one_d[i][ch] * (1.0 - f) + self.one_d[i + 1][ch] * f;
            }
        }
        if self.size_3d > 0 {
            c = self.trilinear(c);
        }
        c
    }

    fn trilinear(&self, c: [f32; 3]) -> [f32; 3] {
        let n = self.size_3d;
        let pos = |ch: usize| {
            let t = unit(c[ch], self.domain_3d, ch) * (n - 1) as f32;
            let i = (t.floor() as usize).min(n - 2);
            (i, t - i as f32)
        };
        let ((r, fr), (g, fg), (b, fb)) = (pos(0), pos(1), pos(2));
        let at = |r: usize, g: usize, b: usize| self.three_d[r + n * (g + n * b)];
        let mut out = [0.0f32; 3];
        for (dr, wr) in [(0, 1.0 - fr), (1, fr)] {
            for (dg, wg) in [(0, 1.0 - fg), (1, fg)] {
                for (db, wb) in [(0, 1.0 - fb), (1, fb)] {
                    let w = wr * wg * wb;
                    if w == 0.0 {
                        continue;
                    }
                    let v = at(r + dr, g + dg, b + db);
                    for k in 0..3 {
                        out[k] += w * v[k];
                    }
                }
            }
        }
        out
    }
}

/// `v` placed in the domain of channel `ch`, clamped to 0..1.
fn unit(v: f32, domain: [[f32; 3]; 2], ch: usize) -> f32 {
    let (lo, hi) = (domain[0][ch], domain[1][ch]);
    if hi <= lo {
        return 0.0;
    }
    ((v - lo) / (hi - lo)).clamp(0.0, 1.0)
}

type Cache = Mutex<HashMap<PathBuf, (std::time::SystemTime, u64, Arc<CubeLut>)>>;

/// Load a `.cube` file, read again only when it changes.
pub fn load(path: &Path) -> Result<Arc<CubeLut>, String> {
    static CACHE: OnceLock<Cache> = OnceLock::new();
    let meta = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let stamp = (meta.modified().unwrap_or(std::time::UNIX_EPOCH), meta.len());
    let cache = CACHE.get_or_init(Default::default);
    if let Some((t, len, lut)) = cache.lock().ok().and_then(|c| c.get(path).cloned()) {
        if (t, len) == stamp {
            return Ok(lut);
        }
    }
    if meta.len() > 64 << 20 {
        return Err(format!("{}: larger than 64 MB", path.display()));
    }
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let lut = Arc::new(CubeLut::parse(&text).map_err(|e| format!("{}: {e}", path.display()))?);
    if let Ok(mut c) = cache.lock() {
        c.insert(path.to_path_buf(), (stamp.0, stamp.1, lut.clone()));
    }
    Ok(lut)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 3D table of `f` at `n`³ points.
    fn cube(n: usize, f: impl Fn([f32; 3]) -> [f32; 3]) -> String {
        let mut s = format!("TITLE \"test\"\n# comment\nLUT_3D_SIZE {n}\n");
        for b in 0..n {
            for g in 0..n {
                for r in 0..n {
                    let x = |i: usize| i as f32 / (n - 1) as f32;
                    let [o0, o1, o2] = f([x(r), x(g), x(b)]);
                    s += &format!("{o0} {o1} {o2}\n");
                }
            }
        }
        s
    }

    #[test]
    fn identity_and_swaps() {
        let id = CubeLut::parse(&cube(17, |c| c)).unwrap();
        assert_eq!(id.title.as_deref(), Some("test"));
        for c in [
            [0.0, 0.0, 0.0],
            [0.3, 0.6, 0.9],
            [1.0, 1.0, 1.0],
            [0.123, 0.5, 0.77],
        ] {
            let o = id.apply(c);
            assert!(o.iter().zip(c).all(|(a, b)| (a - b).abs() < 1e-5), "{o:?}");
        }
        // Out of range clamps.
        assert_eq!(id.apply([-1.0, 2.0, 0.5])[..2], [0.0, 1.0]);
        // Red and blue swapped, and a linear map is exact between points.
        let swap = CubeLut::parse(&cube(5, |[r, g, b]| [b, g, r])).unwrap();
        let o = swap.apply([0.1, 0.4, 0.8]);
        assert!(
            (o[0] - 0.8).abs() < 1e-5 && (o[2] - 0.1).abs() < 1e-5,
            "{o:?}"
        );
    }

    #[test]
    fn shaper_and_domain() {
        // A 1D table that squares, then a 3D identity over 0..2.
        let mut text =
            String::from("LUT_1D_SIZE 3\nLUT_3D_SIZE 2\nDOMAIN_MIN 0 0 0\nDOMAIN_MAX 2 2 2\n");
        text += "0 0 0\n0.5 0.5 0.5\n2 2 2\n";
        for b in 0..2 {
            for g in 0..2 {
                for r in 0..2 {
                    text += &format!("{} {} {}\n", r * 2, g * 2, b * 2);
                }
            }
        }
        let lut = CubeLut::parse(&text).unwrap();
        let o = lut.apply([1.0, 0.5, 2.0]);
        assert!(
            (o[0] - 0.5).abs() < 1e-5 && (o[1] - 0.25).abs() < 1e-5 && (o[2] - 2.0).abs() < 1e-5,
            "{o:?}"
        );
    }

    #[test]
    fn bad_files_say_why() {
        assert!(
            CubeLut::parse("TITLE x\n")
                .unwrap_err()
                .contains("LUT_3D_SIZE")
        );
        assert!(
            CubeLut::parse("LUT_3D_SIZE 2\n0 0 0\n")
                .unwrap_err()
                .contains("expected 8")
        );
        assert!(
            CubeLut::parse("LUT_3D_SIZE 300\n")
                .unwrap_err()
                .contains("out of range")
        );
        assert!(
            CubeLut::parse("LUT_3D_SIZE 2\n0 0 x\n")
                .unwrap_err()
                .contains("line 2")
        );
    }

    #[test]
    fn loads_and_reloads_when_changed() {
        let dir = std::env::temp_dir().join(format!("amb-lut-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("look.cube");
        std::fs::write(&p, cube(2, |c| c)).unwrap();
        let a = load(&p).unwrap();
        assert!(Arc::ptr_eq(&a, &load(&p).unwrap()));
        std::fs::write(&p, cube(3, |c| c)).unwrap();
        assert_eq!(load(&p).unwrap().size_3d, 3);
        assert!(load(&dir.join("missing.cube")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
