// SPDX-License-Identifier: GPL-3.0-or-later
//! Output colour spaces: primaries, white point, transfer function, the
//! conversion from our working space (linear ProPhoto, D50), and a matching
//! ICC profile.
//!
//! The ICC profiles are generated here (version 2.1 matrix/TRC display
//! profiles, the most widely supported kind), so no third-party profile files
//! are shipped. Colorants are Bradford-adapted to the D50 PCS, as the ICC
//! requires; the white point tag is the space's own white.

use crate::matrix::{
    D50_XYZ, Mat3, PROPHOTO_TO_XYZ_D50, apply3, bradford, invert3, mul3, xy_to_xyz,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OutputSpace {
    Srgb,
    DisplayP3,
    /// Same primaries, white and gamma as Adobe RGB (1998).
    AdobeRgbCompatible,
    ProPhoto,
    Rec2020,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Transfer {
    /// IEC 61966-2-1 (sRGB, Display P3).
    Srgb,
    /// Pure power law.
    Gamma(f64),
    /// ITU-R BT.2020 / BT.709 OETF.
    Bt2020,
}

impl Transfer {
    /// Linear (0..1) → encoded (0..1). Values outside 0..1 are extended
    /// symmetrically, so float outputs keep out-of-range data.
    pub fn encode(self, v: f64) -> f64 {
        let s = v.signum();
        let a = v.abs();
        s * match self {
            Transfer::Srgb => {
                if a <= 0.003_130_8 {
                    a * 12.92
                } else {
                    1.055 * a.powf(1.0 / 2.4) - 0.055
                }
            }
            Transfer::Gamma(g) => a.powf(1.0 / g),
            Transfer::Bt2020 => {
                if a < 0.018_053_968_510_807 {
                    4.5 * a
                } else {
                    1.099_296_826_809_44 * a.powf(0.45) - 0.099_296_826_809_44
                }
            }
        }
    }

    /// Encoded → linear (inverse of [`Transfer::encode`]).
    pub fn decode(self, e: f64) -> f64 {
        let s = e.signum();
        let a = e.abs();
        s * match self {
            Transfer::Srgb => {
                if a <= 0.040_45 {
                    a / 12.92
                } else {
                    ((a + 0.055) / 1.055).powf(2.4)
                }
            }
            Transfer::Gamma(g) => a.powf(g),
            Transfer::Bt2020 => {
                if a < 4.5 * 0.018_053_968_510_807 {
                    a / 4.5
                } else {
                    ((a + 0.099_296_826_809_44) / 1.099_296_826_809_44).powf(1.0 / 0.45)
                }
            }
        }
    }
}

const D65: (f64, f64) = (0.3127, 0.3290);
const D50: (f64, f64) = (0.3457, 0.3585);

impl OutputSpace {
    pub const ALL: [OutputSpace; 5] = [
        OutputSpace::Srgb,
        OutputSpace::DisplayP3,
        OutputSpace::AdobeRgbCompatible,
        OutputSpace::ProPhoto,
        OutputSpace::Rec2020,
    ];

    /// Short identifier, as used on the command line.
    pub fn id(self) -> &'static str {
        match self {
            OutputSpace::Srgb => "srgb",
            OutputSpace::DisplayP3 => "p3",
            OutputSpace::AdobeRgbCompatible => "adobergb",
            OutputSpace::ProPhoto => "prophoto",
            OutputSpace::Rec2020 => "rec2020",
        }
    }

    pub fn from_id(id: &str) -> Option<OutputSpace> {
        let id = id.to_ascii_lowercase().replace(['-', '_', ' ', '.'], "");
        Some(match id.as_str() {
            "srgb" => OutputSpace::Srgb,
            "p3" | "displayp3" => OutputSpace::DisplayP3,
            "adobergb" | "adobergb1998" | "argb" => OutputSpace::AdobeRgbCompatible,
            "prophoto" | "prophotorgb" | "romm" => OutputSpace::ProPhoto,
            "rec2020" | "bt2020" => OutputSpace::Rec2020,
            _ => return None,
        })
    }

    /// Description written into the ICC profile.
    pub fn profile_name(self) -> &'static str {
        match self {
            OutputSpace::Srgb => "sRGB (Ambrotype)",
            OutputSpace::DisplayP3 => "Display P3 (Ambrotype)",
            OutputSpace::AdobeRgbCompatible => "Compatible with Adobe RGB (1998) (Ambrotype)",
            OutputSpace::ProPhoto => "ProPhoto RGB (Ambrotype)",
            OutputSpace::Rec2020 => "Rec. 2020 (Ambrotype)",
        }
    }

    /// Red, green, blue primaries and white point (CIE xy).
    pub fn chromaticities(self) -> ([(f64, f64); 3], (f64, f64)) {
        match self {
            OutputSpace::Srgb => ([(0.64, 0.33), (0.30, 0.60), (0.15, 0.06)], D65),
            OutputSpace::DisplayP3 => ([(0.680, 0.320), (0.265, 0.690), (0.150, 0.060)], D65),
            OutputSpace::AdobeRgbCompatible => ([(0.64, 0.33), (0.21, 0.71), (0.15, 0.06)], D65),
            OutputSpace::ProPhoto => ([(0.7347, 0.2653), (0.1596, 0.8404), (0.0366, 0.0001)], D50),
            OutputSpace::Rec2020 => ([(0.708, 0.292), (0.170, 0.797), (0.131, 0.046)], D65),
        }
    }

    pub fn transfer(self) -> Transfer {
        match self {
            OutputSpace::Srgb | OutputSpace::DisplayP3 => Transfer::Srgb,
            OutputSpace::AdobeRgbCompatible => Transfer::Gamma(563.0 / 256.0),
            OutputSpace::ProPhoto => Transfer::Gamma(1.8),
            OutputSpace::Rec2020 => Transfer::Bt2020,
        }
    }

    /// Linear RGB → XYZ under the space's own white.
    pub fn to_xyz_native(self) -> Mat3 {
        let (prims, white) = self.chromaticities();
        rgb_to_xyz(prims, white)
    }

    /// Linear RGB → XYZ adapted to D50 (Bradford): the ICC colorants.
    pub fn to_xyz_d50(self) -> Mat3 {
        let (_, (wx, wy)) = self.chromaticities();
        let adapt = bradford(xy_to_xyz(wx, wy), D50_XYZ);
        mul3(&adapt, &self.to_xyz_native())
    }

    /// Linear working space (ProPhoto, D50) → this space's linear RGB.
    pub fn from_prophoto(self) -> Mat3 {
        mul3(&invert3(&self.to_xyz_d50()), &PROPHOTO_TO_XYZ_D50)
    }

    /// Soft proof: `rgb` (linear ProPhoto, interleaved) as it comes out in
    /// this space, colours beyond its gamut clipped to it as an export clips
    /// them, still in linear ProPhoto. Returns which pixels had a colour
    /// out of the gamut (bright whites past 1 are clipping, not gamut).
    pub fn soft_proof(self, rgb: &mut [f32]) -> Vec<bool> {
        let to = self.from_prophoto();
        let back = invert3(&to);
        rgb.chunks_mut(3)
            .map(|p| {
                let d = crate::matrix::apply3(&to, [p[0].into(), p[1].into(), p[2].into()]);
                let (lo, hi) = (d[0].min(d[1]).min(d[2]), d[0].max(d[1]).max(d[2]));
                const EPS: f64 = 1e-3;
                let out = lo < -EPS || (hi > 1.0 + EPS && lo < 1.0);
                if out || hi > 1.0 {
                    let r = crate::matrix::apply3(&back, d.map(|v| v.clamp(0.0, 1.0)));
                    for (slot, v) in p.iter_mut().zip(r) {
                        *slot = v as f32;
                    }
                }
                out
            })
            .collect()
    }

    /// An ICC v2.1 display profile describing this space.
    pub fn icc_profile(self) -> Vec<u8> {
        let (_, (wx, wy)) = self.chromaticities();
        icc_v2(
            self.profile_name(),
            &self.to_xyz_d50(),
            xy_to_xyz(wx, wy),
            self.transfer(),
        )
    }
}

/// RGB → XYZ matrix from primaries and white (white maps to Y = 1).
pub fn rgb_to_xyz(prims: [(f64, f64); 3], white: (f64, f64)) -> Mat3 {
    let cols: Vec<[f64; 3]> = prims.iter().map(|&(x, y)| xy_to_xyz(x, y)).collect();
    let m = [
        [cols[0][0], cols[1][0], cols[2][0]],
        [cols[0][1], cols[1][1], cols[2][1]],
        [cols[0][2], cols[1][2], cols[2][2]],
    ];
    let s = apply3(&invert3(&m), xy_to_xyz(white.0, white.1));
    m.map(|row| [row[0] * s[0], row[1] * s[1], row[2] * s[2]])
}

// ---------------------------------------------------------------- ICC writer

fn s15f16(v: f64) -> [u8; 4] {
    ((v * 65536.0).round() as i32).to_be_bytes()
}

fn xyz_tag(xyz: [f64; 3]) -> Vec<u8> {
    let mut t = b"XYZ \0\0\0\0".to_vec();
    for v in xyz {
        t.extend_from_slice(&s15f16(v));
    }
    t
}

fn curve_tag(transfer: Transfer) -> Vec<u8> {
    let mut t = b"curv\0\0\0\0".to_vec();
    match transfer {
        Transfer::Gamma(g) => {
            t.extend_from_slice(&1u32.to_be_bytes());
            // u8Fixed8Number.
            t.extend_from_slice(&((g * 256.0).round() as u16).to_be_bytes());
        }
        other => {
            // Encoded → linear table, as ICC TRCs map device values to linear.
            const N: usize = 1024;
            t.extend_from_slice(&(N as u32).to_be_bytes());
            for i in 0..N {
                let e = i as f64 / (N - 1) as f64;
                let v = (other.decode(e).clamp(0.0, 1.0) * 65535.0).round() as u16;
                t.extend_from_slice(&v.to_be_bytes());
            }
        }
    }
    t
}

fn desc_tag(text: &str) -> Vec<u8> {
    let ascii: Vec<u8> = text.bytes().filter(u8::is_ascii).collect();
    let mut t = b"desc\0\0\0\0".to_vec();
    t.extend_from_slice(&(ascii.len() as u32 + 1).to_be_bytes());
    t.extend_from_slice(&ascii);
    t.push(0);
    t.extend_from_slice(&[0; 4]); // Unicode language code
    t.extend_from_slice(&[0; 4]); // Unicode count
    t.extend_from_slice(&[0; 2]); // ScriptCode code
    t.push(0); // ScriptCode count
    t.extend_from_slice(&[0; 67]);
    t
}

fn text_tag(text: &str) -> Vec<u8> {
    let mut t = b"text\0\0\0\0".to_vec();
    t.extend(text.bytes().filter(u8::is_ascii));
    t.push(0);
    t
}

fn icc_v2(name: &str, colorants: &Mat3, white: [f64; 3], transfer: Transfer) -> Vec<u8> {
    let column = |c: usize| [colorants[0][c], colorants[1][c], colorants[2][c]];
    let trc = curve_tag(transfer);
    // (signature, data); identical data blocks are shared by offset.
    let tags: Vec<(&[u8; 4], Vec<u8>)> = vec![
        (b"desc", desc_tag(name)),
        (
            b"cprt",
            text_tag("No copyright, use freely. Generated by Ambrotype."),
        ),
        (b"wtpt", xyz_tag(white)),
        (b"rXYZ", xyz_tag(column(0))),
        (b"gXYZ", xyz_tag(column(1))),
        (b"bXYZ", xyz_tag(column(2))),
        (b"rTRC", trc.clone()),
        (b"gTRC", trc.clone()),
        (b"bTRC", trc),
    ];
    let header_len = 128 + 4 + 12 * tags.len();
    let mut table = Vec::new();
    let mut data: Vec<u8> = Vec::new();
    let mut placed: Vec<(Vec<u8>, u32)> = Vec::new();
    for (sig, block) in &tags {
        let offset = match placed.iter().find(|(b, _)| b == block) {
            Some((_, off)) => *off,
            None => {
                while data.len() % 4 != 0 {
                    data.push(0);
                }
                let off = (header_len + data.len()) as u32;
                data.extend_from_slice(block);
                placed.push((block.clone(), off));
                off
            }
        };
        table.extend_from_slice(*sig);
        table.extend_from_slice(&offset.to_be_bytes());
        table.extend_from_slice(&(block.len() as u32).to_be_bytes());
    }
    while data.len() % 4 != 0 {
        data.push(0);
    }
    let size = (header_len + data.len()) as u32;
    let mut out = Vec::with_capacity(size as usize);
    out.extend_from_slice(&size.to_be_bytes());
    out.extend_from_slice(&[0; 4]); // preferred CMM
    out.extend_from_slice(&[0x02, 0x10, 0, 0]); // version 2.1
    out.extend_from_slice(b"mntr");
    out.extend_from_slice(b"RGB ");
    out.extend_from_slice(b"XYZ ");
    // Creation date: 2026-01-01 00:00:00 (fixed, so profiles are reproducible).
    for v in [2026u16, 1, 1, 0, 0, 0] {
        out.extend_from_slice(&v.to_be_bytes());
    }
    out.extend_from_slice(b"acsp");
    out.extend_from_slice(&[0; 4]); // platform
    out.extend_from_slice(&[0; 4]); // flags
    out.extend_from_slice(&[0; 4]); // manufacturer
    out.extend_from_slice(&[0; 4]); // model
    out.extend_from_slice(&[0; 8]); // attributes
    out.extend_from_slice(&0u32.to_be_bytes()); // perceptual intent
    for v in D50_XYZ {
        out.extend_from_slice(&s15f16(v));
    }
    out.extend_from_slice(&[0; 4]); // creator
    out.extend_from_slice(&[0; 44]); // profile ID (v4) + reserved
    debug_assert_eq!(out.len(), 128);
    out.extend_from_slice(&(tags.len() as u32).to_be_bytes());
    out.extend_from_slice(&table);
    out.extend_from_slice(&data);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::matrix::SRGB_TO_XYZ_D50;

    #[test]
    fn transfers_round_trip() {
        for space in OutputSpace::ALL {
            let t = space.transfer();
            for i in 0..=100 {
                let v = i as f64 / 100.0;
                assert!((t.decode(t.encode(v)) - v).abs() < 1e-9, "{space:?} {v}");
            }
            assert!((t.encode(1.0) - 1.0).abs() < 1e-9);
        }
    }

    #[test]
    fn matrices_match_known_values() {
        let srgb = OutputSpace::Srgb.to_xyz_d50();
        for r in 0..3 {
            for c in 0..3 {
                assert!(
                    (srgb[r][c] - SRGB_TO_XYZ_D50[r][c]).abs() < 2e-4,
                    "{srgb:?}"
                );
            }
        }
        let pro = OutputSpace::ProPhoto.to_xyz_d50();
        for r in 0..3 {
            for c in 0..3 {
                assert!(
                    (pro[r][c] - PROPHOTO_TO_XYZ_D50[r][c]).abs() < 2e-3,
                    "{pro:?}"
                );
            }
        }
        // Working ProPhoto → ProPhoto output is (nearly) the identity, and every
        // space maps white to white.
        for space in OutputSpace::ALL {
            let w = apply3(&space.from_prophoto(), [1.0, 1.0, 1.0]);
            for c in w {
                assert!((c - 1.0).abs() < 2e-3, "{space:?} {w:?}");
            }
        }
    }

    #[test]
    fn soft_proof_clips_to_the_gamut() {
        // A saturated ProPhoto green is beyond sRGB, inside nothing smaller.
        let green = [0.1f32, 0.6, 0.05];
        let grey = [0.18f32, 0.18, 0.18];
        let mut px: Vec<f32> = green.iter().chain(&grey).copied().collect();
        let out = OutputSpace::Srgb.soft_proof(&mut px);
        assert_eq!(out, [true, false]);
        // The grey is untouched; the green now fits sRGB.
        assert_eq!(&px[3..], &grey);
        let srgb = apply3(
            &OutputSpace::Srgb.from_prophoto(),
            [px[0].into(), px[1].into(), px[2].into()],
        );
        assert!(
            srgb.iter().all(|v| (-1e-4..=1.0 + 1e-4).contains(v)),
            "{srgb:?}"
        );
        // ProPhoto itself holds it.
        let mut px = green.to_vec();
        assert_eq!(OutputSpace::ProPhoto.soft_proof(&mut px), [false]);
    }

    #[test]
    fn icc_profiles_are_well_formed() {
        for space in OutputSpace::ALL {
            let p = space.icc_profile();
            let size = u32::from_be_bytes([p[0], p[1], p[2], p[3]]) as usize;
            assert_eq!(size, p.len());
            assert_eq!(&p[36..40], b"acsp");
            let count = u32::from_be_bytes([p[128], p[129], p[130], p[131]]) as usize;
            assert_eq!(count, 9);
            for i in 0..count {
                let e = 132 + 12 * i;
                let off = u32::from_be_bytes([p[e + 4], p[e + 5], p[e + 6], p[e + 7]]) as usize;
                let len = u32::from_be_bytes([p[e + 8], p[e + 9], p[e + 10], p[e + 11]]) as usize;
                assert!(off % 4 == 0 && off + len <= p.len(), "{space:?} tag {i}");
            }
            assert_eq!(OutputSpace::from_id(space.id()), Some(space));
        }
        assert_eq!(
            OutputSpace::from_id("Display-P3"),
            Some(OutputSpace::DisplayP3)
        );
    }
}
