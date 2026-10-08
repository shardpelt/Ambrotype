//! Reading the colour space of an image from its embedded ICC profile.
//!
//! Only RGB matrix/TRC profiles are read: three colorants and a tone curve
//! per channel. That covers what cameras, phones and editors embed (sRGB,
//! Adobe RGB, Display P3, ProPhoto, Rec. 2020). Table-based (LUT) profiles
//! are not read; the caller falls back to sRGB.

use crate::matrix::Mat3;

/// An RGB matrix/TRC profile: linear RGB → XYZ (D50 PCS), and each
/// channel's tone curve from encoded to linear.
#[derive(Debug, Clone, PartialEq)]
pub struct RgbProfile {
    pub to_xyz_d50: Mat3,
    pub curves: [Curve; 3],
}

/// An ICC tone curve, encoded (0..1) → linear (0..1).
#[derive(Debug, Clone, PartialEq)]
pub enum Curve {
    Gamma(f64),
    /// Sampled curve, evenly spaced over 0..1.
    Table(Vec<f64>),
    /// ICC parametric curve (function types 0–4) and its parameters
    /// `g, a, b, c, d, e, f`; unused ones are zero.
    Parametric(u16, [f64; 7]),
}

impl Curve {
    pub fn to_linear(&self, v: f64) -> f64 {
        let v = v.clamp(0.0, 1.0);
        match self {
            Curve::Gamma(g) => v.powf(*g),
            Curve::Table(t) => {
                let x = v * (t.len() - 1) as f64;
                let i = (x.floor() as usize).min(t.len() - 2);
                let f = x - i as f64;
                t[i] * (1.0 - f) + t[i + 1] * f
            }
            Curve::Parametric(kind, p) => {
                let [g, a, b, c, d, e, f] = *p;
                match kind {
                    0 => v.powf(g),
                    1 => {
                        if v >= -b / a {
                            (a * v + b).max(0.0).powf(g)
                        } else {
                            0.0
                        }
                    }
                    2 => {
                        if v >= -b / a {
                            (a * v + b).max(0.0).powf(g) + c
                        } else {
                            c
                        }
                    }
                    3 => {
                        if v >= d {
                            (a * v + b).max(0.0).powf(g)
                        } else {
                            c * v
                        }
                    }
                    _ => {
                        if v >= d {
                            (a * v + b).max(0.0).powf(g) + e
                        } else {
                            c * v + f
                        }
                    }
                }
            }
        }
    }
}

/// The sRGB profile, used when an image has none.
pub fn srgb() -> RgbProfile {
    RgbProfile {
        to_xyz_d50: crate::matrix::SRGB_TO_XYZ_D50,
        curves: std::array::from_fn(|_| srgb_curve()),
    }
}

fn srgb_curve() -> Curve {
    Curve::Parametric(
        3,
        [
            2.4,
            1.0 / 1.055,
            0.055 / 1.055,
            1.0 / 12.92,
            0.04045,
            0.0,
            0.0,
        ],
    )
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

fn s15f16(b: &[u8], at: usize) -> Option<f64> {
    Some(i32::from_be_bytes(b.get(at..at + 4)?.try_into().ok()?) as f64 / 65536.0)
}

/// Parse an ICC profile. `None` for anything that isn't an RGB matrix/TRC
/// profile, or is malformed.
pub fn parse(b: &[u8]) -> Option<RgbProfile> {
    if b.len() < 132 || b.get(36..40)? != b"acsp" || b.get(16..20)? != b"RGB " {
        return None;
    }
    let count = u32_at(b, 128)? as usize;
    let mut tags = std::collections::HashMap::new();
    for i in 0..count.min(256) {
        let at = 132 + i * 12;
        let sig: [u8; 4] = b.get(at..at + 4)?.try_into().ok()?;
        let (offset, size) = (u32_at(b, at + 4)? as usize, u32_at(b, at + 8)? as usize);
        tags.insert(sig, b.get(offset..offset.checked_add(size)?)?);
    }
    let xyz = |sig: &[u8; 4]| -> Option<[f64; 3]> {
        let t = tags.get(sig)?;
        if t.get(0..4)? != b"XYZ " {
            return None;
        }
        Some([s15f16(t, 8)?, s15f16(t, 12)?, s15f16(t, 16)?])
    };
    let (r, g, bl) = (xyz(b"rXYZ")?, xyz(b"gXYZ")?, xyz(b"bXYZ")?);
    let to_xyz_d50 = [
        [r[0], g[0], bl[0]],
        [r[1], g[1], bl[1]],
        [r[2], g[2], bl[2]],
    ];
    let curve = |sig: &[u8; 4]| -> Option<Curve> { parse_curve(tags.get(sig)?) };
    let curves = [curve(b"rTRC")?, curve(b"gTRC")?, curve(b"bTRC")?];
    Some(RgbProfile { to_xyz_d50, curves })
}

fn parse_curve(t: &[u8]) -> Option<Curve> {
    match t.get(0..4)? {
        b"curv" => {
            let n = u32_at(t, 8)? as usize;
            match n {
                0 => Some(Curve::Gamma(1.0)),
                1 => Some(Curve::Gamma(u16_at(t, 12)? as f64 / 256.0)),
                _ => {
                    let table = (0..n)
                        .map(|i| u16_at(t, 12 + 2 * i).map(|v| v as f64 / 65535.0))
                        .collect::<Option<Vec<_>>>()?;
                    Some(Curve::Table(table))
                }
            }
        }
        b"para" => {
            let kind = u16_at(t, 8)?;
            let n = match kind {
                0 => 1,
                1 => 3,
                2 => 4,
                3 => 5,
                4 => 7,
                _ => return None,
            };
            let mut p = [0.0; 7];
            for (i, slot) in p.iter_mut().take(n).enumerate() {
                *slot = s15f16(t, 12 + 4 * i)?;
            }
            Some(Curve::Parametric(kind, p))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::space::OutputSpace;

    #[test]
    fn reads_back_our_own_profiles() {
        for space in [
            OutputSpace::Srgb,
            OutputSpace::DisplayP3,
            OutputSpace::AdobeRgbCompatible,
            OutputSpace::ProPhoto,
            OutputSpace::Rec2020,
        ] {
            let p = parse(&space.icc_profile()).unwrap_or_else(|| panic!("{space:?}"));
            let want = space.to_xyz_d50();
            for (row, want) in p.to_xyz_d50.iter().zip(want) {
                for (v, w) in row.iter().zip(want) {
                    assert!((v - w).abs() < 1e-3, "{space:?}: {:?}", p.to_xyz_d50);
                }
            }
            // The tone curve inverts the space's own encoding.
            for v in [0.02, 0.2, 0.5, 0.9] {
                let lin = p.curves[1].to_linear(space.transfer().encode(v));
                assert!((lin - v).abs() < 2e-3, "{space:?} {v}: {lin}");
            }
        }
    }

    #[test]
    fn rejects_what_it_cannot_read() {
        assert_eq!(parse(b"not a profile"), None);
        let mut cmyk = OutputSpace::Srgb.icc_profile();
        cmyk[16..20].copy_from_slice(b"CMYK");
        assert_eq!(parse(&cmyk), None);
    }

    #[test]
    fn srgb_curve_matches_the_standard() {
        let c = srgb_curve();
        assert!((c.to_linear(0.5) - 0.214_041).abs() < 1e-5);
        assert!((c.to_linear(0.02) - 0.02 / 12.92).abs() < 1e-7);
    }
}
