// SPDX-License-Identifier: GPL-3.0-or-later
//! DNG opcode lists (`OpcodeList2`, `OpcodeList3`), as the DNG specification
//! describes them: the corrections a camera or converter wrote into the raw
//! file itself. Phones use them for lens shading (`GainMap`) and distortion
//! (`WarpRectilinear`); converted Micro Four Thirds and Fujifilm raws carry
//! their built-in lens profiles as `WarpRectilinear` and `FixVignetteRadial`.
//!
//! We read those three. `GainMap` is applied when the photo is loaded
//! ([`apply_gain_maps`]); the warp and the vignette become a lens
//! [`Correction`] of the photo's own ([`correction`]), which the lens pass
//! applies before any profile.

use amb_lens::{Correction, Distortion, Frame, Source, Tca, Vignetting};

/// One opcode we understand, or the id of one we skip.
#[derive(Debug, Clone, PartialEq)]
pub enum Opcode {
    /// Per plane: `[kr0, kr1, kr2, kr3, kt0, kt1]`; the centre as fractions
    /// of the image (x, y).
    WarpRectilinear {
        planes: Vec<[f64; 6]>,
        centre: (f64, f64),
    },
    FixVignetteRadial {
        k: [f64; 5],
        centre: (f64, f64),
    },
    GainMap(GainMap),
    Other {
        id: u32,
    },
}

/// A `GainMap`: a grid of gains over an area of the raw image, for the
/// pixels at the given pitch (one map per CFA colour on a Bayer sensor).
#[derive(Debug, Clone, PartialEq)]
pub struct GainMap {
    pub top: u32,
    pub left: u32,
    pub bottom: u32,
    pub right: u32,
    pub plane: u32,
    pub planes: u32,
    pub row_pitch: u32,
    pub col_pitch: u32,
    pub points_v: u32,
    pub points_h: u32,
    /// Spacing and origin of the grid, as fractions of the image.
    pub spacing_v: f64,
    pub spacing_h: f64,
    pub origin_v: f64,
    pub origin_h: f64,
    pub map_planes: u32,
    /// `points_v × points_h × map_planes`, plane fastest.
    pub gains: Vec<f32>,
}

impl GainMap {
    /// The gain at a position given as fractions of the image (v down, h
    /// across), for map plane `p`; bilinear between grid points, clamped at
    /// the edges.
    pub fn at(&self, v: f64, h: f64, p: usize) -> f32 {
        let (nv, nh) = (self.points_v as usize, self.points_h as usize);
        let mp = (self.map_planes as usize).max(1);
        let p = p.min(mp - 1);
        let grid = |pos: f64, origin: f64, spacing: f64, n: usize| {
            let f = if spacing > 0.0 {
                (pos - origin) / spacing
            } else {
                0.0
            };
            let f = f.clamp(0.0, (n - 1) as f64);
            let i = (f.floor() as usize).min(n.saturating_sub(2));
            (i, (f - i as f64) as f32, n > 1)
        };
        let (i, fv, two_v) = grid(v, self.origin_v, self.spacing_v, nv);
        let (j, fh, two_h) = grid(h, self.origin_h, self.spacing_h, nh);
        let g = |a: usize, b: usize| self.gains[(a * nh + b) * mp + p];
        let (i1, j1) = (if two_v { i + 1 } else { i }, if two_h { j + 1 } else { j });
        let top = g(i, j) + (g(i, j1) - g(i, j)) * fh;
        let bottom = g(i1, j) + (g(i1, j1) - g(i1, j)) * fh;
        top + (bottom - top) * fv
    }
}

/// Read an opcode list (always big-endian). Opcodes we can't parse are
/// returned as [`Opcode::Other`]; a truncated list stops early.
pub fn parse(data: &[u8]) -> Vec<Opcode> {
    let mut r = Reader { data, at: 0 };
    let mut out = Vec::new();
    let Some(count) = r.u32() else { return out };
    for _ in 0..count.min(1024) {
        let (Some(id), Some(_version), Some(_flags), Some(size)) =
            (r.u32(), r.u32(), r.u32(), r.u32())
        else {
            break;
        };
        let Some(body) = r.bytes(size as usize) else {
            break;
        };
        out.push(parse_one(id, body).unwrap_or(Opcode::Other { id }));
    }
    out
}

fn parse_one(id: u32, body: &[u8]) -> Option<Opcode> {
    let mut r = Reader { data: body, at: 0 };
    Some(match id {
        1 => {
            let n = r.u32()? as usize;
            if !(1..=4).contains(&n) {
                return None;
            }
            let planes = (0..n)
                .map(|_| Some([r.f64()?, r.f64()?, r.f64()?, r.f64()?, r.f64()?, r.f64()?]))
                .collect::<Option<Vec<_>>>()?;
            Opcode::WarpRectilinear {
                planes,
                centre: (r.f64()?, r.f64()?),
            }
        }
        3 => Opcode::FixVignetteRadial {
            k: [r.f64()?, r.f64()?, r.f64()?, r.f64()?, r.f64()?],
            centre: (r.f64()?, r.f64()?),
        },
        9 => {
            let mut m = GainMap {
                top: r.u32()?,
                left: r.u32()?,
                bottom: r.u32()?,
                right: r.u32()?,
                plane: r.u32()?,
                planes: r.u32()?,
                row_pitch: r.u32()?,
                col_pitch: r.u32()?,
                points_v: r.u32()?,
                points_h: r.u32()?,
                spacing_v: r.f64()?,
                spacing_h: r.f64()?,
                origin_v: r.f64()?,
                origin_h: r.f64()?,
                map_planes: r.u32()?,
                gains: Vec::new(),
            };
            let n = (m.points_v as usize)
                .checked_mul(m.points_h as usize)?
                .checked_mul(m.map_planes.max(1) as usize)?;
            if n == 0 || n > 1 << 24 {
                return None;
            }
            m.gains = (0..n).map(|_| r.f32()).collect::<Option<_>>()?;
            Opcode::GainMap(m)
        }
        _ => return None,
    })
}

struct Reader<'a> {
    data: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn bytes(&mut self, n: usize) -> Option<&'a [u8]> {
        let b = self.data.get(self.at..self.at.checked_add(n)?)?;
        self.at += n;
        Some(b)
    }
    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_be_bytes(self.bytes(4)?.try_into().ok()?))
    }
    fn f32(&mut self) -> Option<f32> {
        Some(f32::from_be_bytes(self.bytes(4)?.try_into().ok()?))
    }
    fn f64(&mut self) -> Option<f64> {
        Some(f64::from_be_bytes(self.bytes(8)?.try_into().ok()?))
    }
}

/// Where the developed photo sits in the image the opcodes describe: that
/// image's size (the raw's active area) and the photo's offset in it (the
/// default crop).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    pub stage: (usize, usize),
    pub origin: (usize, usize),
}

/// Apply the gain maps to a developed photo (`width × height` camera RGB,
/// interleaved). `cfa_colour(row, col)` gives the colour (0 red, 1 green,
/// 2 blue) of a raw pixel for a Bayer image, or `None` for one already in
/// RGB. Maps of the same colour are averaged (the two greens), since the
/// photo is already demosaiced.
pub fn apply_gain_maps(
    rgb: &mut [f32],
    width: usize,
    height: usize,
    maps: &[&GainMap],
    at: Placement,
    cfa_colour: Option<&dyn Fn(usize, usize) -> usize>,
) {
    use rayon::prelude::*;
    if maps.is_empty() || width == 0 {
        return;
    }
    // For each map, the photo channels it covers and its map plane for each.
    let mut by_channel: [Vec<(&GainMap, usize)>; 3] = Default::default();
    for m in maps {
        match cfa_colour {
            Some(colour) if m.row_pitch > 1 || m.col_pitch > 1 => {
                let c = colour(m.top as usize, m.left as usize).min(2);
                by_channel[c].push((m, 0));
            }
            // Every pixel, or an RGB image's planes.
            _ => {
                let first = if cfa_colour.is_some() { 0 } else { m.plane };
                let planes = if cfa_colour.is_some() { 3 } else { m.planes };
                for (i, c) in (first..first + planes.max(1)).enumerate() {
                    if let Some(list) = by_channel.get_mut(c as usize) {
                        list.push((m, i));
                    }
                }
            }
        }
    }
    let (sw, sh) = (at.stage.0.max(1) as f64, at.stage.1.max(1) as f64);
    let cfa = cfa_colour.is_some();
    rgb.par_chunks_mut(width * 3)
        .enumerate()
        .take(height)
        .for_each(|(y, row)| {
            let sy = y + at.origin.1;
            let v = (sy as f64 + 0.5) / sh;
            for (x, p) in row.chunks_mut(3).enumerate() {
                let sx = x + at.origin.0;
                let h = (sx as f64 + 0.5) / sw;
                for (c, list) in by_channel.iter().enumerate() {
                    let (mut sum, mut n) = (0.0f32, 0u32);
                    for (m, plane) in list {
                        // A CFA map's first row and column are its colour's
                        // phase: every demosaiced pixel near them is in.
                        let (pv, ph) = if cfa {
                            (m.row_pitch.max(1) - 1, m.col_pitch.max(1) - 1)
                        } else {
                            (0, 0)
                        };
                        let inside = sy + pv as usize >= m.top as usize
                            && sy < m.bottom as usize
                            && sx + ph as usize >= m.left as usize
                            && sx < m.right as usize;
                        if inside {
                            sum += m.at(v, h, *plane);
                            n += 1;
                        }
                    }
                    if n > 0 {
                        p[c] *= sum / n as f32;
                    }
                }
            }
        });
}

/// The lens correction the warp and vignette opcodes describe, for the
/// photo placed `at` in their image, or `None` without any.
pub fn correction(
    ops: &[Opcode],
    at: Placement,
    width: usize,
    height: usize,
) -> Option<Correction> {
    let frame = |(cx, cy): (f64, f64)| {
        let (sw, sh) = (
            at.stage.0.max(1) as f64 - 1.0,
            at.stage.1.max(1) as f64 - 1.0,
        );
        let (px, py) = (cx * sw, cy * sh);
        // r = 1 at the corner of their image furthest from the centre.
        let m = [(0.0, 0.0), (sw, 0.0), (0.0, sh), (sw, sh)]
            .iter()
            .map(|(x, y)| (x - px).hypot(y - py))
            .fold(0.0, f64::max)
            .max(1.0);
        let (w, h) = ((width.max(2) - 1) as f64, (height.max(2) - 1) as f64);
        Frame::Dng {
            cx: (px - at.origin.0 as f64) / w,
            cy: (py - at.origin.1 as f64) / h,
            unit: m / w.max(h),
        }
    };
    let mut c = Correction {
        source: Source::Embedded,
        description: "Built-in (DNG)".into(),
        file: None,
        distortion: None,
        tca: None,
        vignetting: None,
    };
    for op in ops {
        match op {
            Opcode::WarpRectilinear { planes, centre } => {
                let f = frame(*centre);
                if planes.len() >= 3 {
                    c.distortion = Some((Distortion::Dng { k: planes[1] }, f));
                    c.tca = Some((
                        Tca::Dng {
                            red: planes[0],
                            green: planes[1],
                            blue: planes[2],
                        },
                        f,
                    ));
                } else {
                    c.distortion = Some((Distortion::Dng { k: planes[0] }, f));
                }
            }
            Opcode::FixVignetteRadial { k, centre } => {
                c.vignetting = Some((Vignetting::DngGain { k: *k }, frame(*centre)));
            }
            _ => {}
        }
    }
    (c.distortion.is_some() || c.vignetting.is_some()).then_some(c)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(ops: &[(u32, Vec<u8>)]) -> Vec<u8> {
        let mut out = (ops.len() as u32).to_be_bytes().to_vec();
        for (id, body) in ops {
            for v in [*id, 0x0103_0000, 1, body.len() as u32] {
                out.extend_from_slice(&v.to_be_bytes());
            }
            out.extend_from_slice(body);
        }
        out
    }

    fn f64s(v: &[f64]) -> Vec<u8> {
        v.iter().flat_map(|x| x.to_be_bytes()).collect()
    }

    fn gain_map(top: u32, left: u32, gains: &[f32]) -> Vec<u8> {
        let mut b = Vec::new();
        for v in [top, left, 4, 4, 0, 1, 2, 2, 2, 2] {
            b.extend_from_slice(&v.to_be_bytes());
        }
        b.extend(f64s(&[1.0, 1.0, 0.0, 0.0]));
        b.extend_from_slice(&1u32.to_be_bytes());
        b.extend(gains.iter().flat_map(|g| g.to_be_bytes()));
        b
    }

    #[test]
    fn reads_warp_vignette_and_gain_maps() {
        let mut warp = 3u32.to_be_bytes().to_vec();
        for k0 in [0.99, 1.0, 1.01] {
            warp.extend(f64s(&[k0, -0.02, 0.0, 0.0, 0.0, 0.0]));
        }
        warp.extend(f64s(&[0.5, 0.5]));
        let vignette = f64s(&[0.3, 0.1, 0.0, 0.0, 0.0, 0.5, 0.5]);
        let data = list(&[
            (1, warp),
            (3, vignette),
            (9, gain_map(0, 0, &[1.0, 2.0, 3.0, 4.0])),
            (7, vec![0; 6]),
        ]);
        let ops = parse(&data);
        assert_eq!(ops.len(), 4);
        assert!(matches!(&ops[0], Opcode::WarpRectilinear { planes, .. } if planes.len() == 3));
        assert!(matches!(ops[3], Opcode::Other { id: 7 }));
        let Opcode::GainMap(m) = &ops[2] else {
            panic!()
        };
        // Corners of the grid, and halfway between them.
        assert_eq!(m.at(0.0, 0.0, 0), 1.0);
        assert_eq!(m.at(1.0, 1.0, 0), 4.0);
        assert!((m.at(0.5, 0.5, 0) - 2.5).abs() < 1e-6);
        // A truncated list stops without panicking.
        assert!(parse(&data[..30]).len() <= 1);

        let at = Placement {
            stage: (101, 101),
            origin: (0, 0),
        };
        let c = correction(&ops, at, 101, 101).unwrap();
        assert!(matches!(c.tca, Some((Tca::Dng { .. }, _))));
        // r = 1 at the corner: half the diagonal over the longest side.
        let Some((_, Frame::Dng { cx, unit, .. })) = c.distortion else {
            panic!()
        };
        assert!((cx - 0.5).abs() < 1e-9);
        assert!((unit - 0.5f64.hypot(0.5)).abs() < 1e-9, "{unit}");
    }

    #[test]
    fn gain_maps_go_to_their_cfa_colour() {
        // An RGGB pattern: red maps at (0, 0), blue at (1, 1), both greens 2×.
        let data = list(&[
            (9, gain_map(0, 0, &[2.0; 4])),
            (9, gain_map(0, 1, &[2.0; 4])),
            (9, gain_map(1, 0, &[2.0; 4])),
            (9, gain_map(1, 1, &[0.5; 4])),
        ]);
        let ops = parse(&data);
        let maps: Vec<&GainMap> = ops
            .iter()
            .filter_map(|o| match o {
                Opcode::GainMap(m) => Some(m),
                _ => None,
            })
            .collect();
        let rggb = |r: usize, c: usize| [[0, 1], [1, 2]][r % 2][c % 2];
        let mut rgb = vec![1.0f32; 4 * 4 * 3];
        let at = Placement {
            stage: (4, 4),
            origin: (0, 0),
        };
        apply_gain_maps(&mut rgb, 4, 4, &maps, at, Some(&rggb));
        assert_eq!(&rgb[..3], &[2.0, 2.0, 0.5]);
        assert_eq!(&rgb[45..], &[2.0, 2.0, 0.5]);
    }
}
