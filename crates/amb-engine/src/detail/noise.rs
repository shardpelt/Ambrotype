// SPDX-License-Identifier: GPL-3.0-or-later
//! The noise estimate both noise reductions use: per-tile powers, per band of
//! luminance.

use super::*;

impl From<TileSums> for TileAcc {
    fn from(t: TileSums) -> TileAcc {
        TileAcc {
            power: t.power,
            lum: t.lum,
            lo: t.lo,
            hi: t.hi,
            n: t.n,
        }
    }
}

/// Tile statistics of the Haar diagonal detail of the centred encoded
/// luminance (see [`haar_tiles`]).
pub(super) fn luminance_tiles(l: &[f32], w: usize, h: usize) -> Vec<TileStat> {
    haar_tiles(l, CENTRE, &[l], w, h, NOISE_TILE, false)
}

/// Powers of three detail signals of a 2×2 block `[a, b; c, d]`, each of
/// variance σ² for white noise of σ: the Haar diagonal `(a − b − c + d)/2`
/// (blind to horizontal and vertical structure and to gradients), and the
/// differences `(b − c)/√2` and `(a − d)/√2` along the two diagonals (blind to
/// structure along them). The smallest tile sum of the three reads noise but
/// not texture that runs along one of the four directions (stripes, weaves).
#[inline]
pub(super) fn haar_powers(v: [f32; 4]) -> [f32; 3] {
    let hv = 0.5 * (v[0] - v[1] - v[2] + v[3]);
    let (d1, d2) = (v[1] - v[2], v[0] - v[3]);
    [hv * hv, 0.5 * d1 * d1, 0.5 * d2 * d2]
}

#[inline]
pub(super) fn add3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

/// Add one 2×2 block with encoded luminance `lum` (minus `offset`) and noise
/// power `power` to a tile.
#[inline]
pub(super) fn add_haar_block(acc: &mut TileAcc, lum: [f32; 4], offset: f32, power: [f32; 3]) {
    let lo = lum[0].min(lum[1]).min(lum[2].min(lum[3])) + offset;
    let hi = lum[0].max(lum[1]).max(lum[2].max(lum[3])) + offset;
    let mean = 0.25 * (lum[0] + lum[1] + lum[2] + lum[3]) + offset;
    acc.add(power, mean, lo, hi);
}

/// Statistics over `tile`×`tile`-pixel tiles of the detail of 2×2 blocks,
/// its power summed over the `srcs` planes; banded and clipped by `lum`
/// (encoded luminance minus `offset`). The detail is the Haar diagonal, or
/// with `directional` the least of the three [`haar_powers`].
pub(super) fn haar_tiles(
    lum: &[f32],
    offset: f32,
    srcs: &[&[f32]],
    w: usize,
    h: usize,
    tile: usize,
    directional: bool,
) -> Vec<TileStat> {
    let (cw, ch) = (w / 2, h / 2);
    let t = (tile / 2).max(1);
    (0..ch.div_ceil(t))
        .into_par_iter()
        .flat_map_iter(|ty| {
            let mut acc = vec![TileAcc::default(); cw.div_ceil(t)];
            for cy in ty * t..((ty + 1) * t).min(ch) {
                let (i0, i1) = (2 * cy * w, (2 * cy + 1) * w);
                let block = |p: &[f32], cx: usize| {
                    [
                        p[i0 + 2 * cx],
                        p[i0 + 2 * cx + 1],
                        p[i1 + 2 * cx],
                        p[i1 + 2 * cx + 1],
                    ]
                };
                for cx in 0..cw {
                    let power = srcs
                        .iter()
                        .fold([0.0; 3], |a, p| add3(a, haar_powers(block(p, cx))));
                    let power = if directional { power } else { [power[0]; 3] };
                    add_haar_block(&mut acc[cx / t], block(lum, cx), offset, power);
                }
            }
            acc.into_iter().filter_map(TileAcc::finish)
        })
        .collect()
}

/// Tile side (pixels) of the noise estimate.
pub(super) const NOISE_TILE: usize = 16;

/// Luminance bands of the noise estimate.
pub(super) const NOISE_BANDS: usize = 8;

/// Fewest tiles that give a band its own estimate.
pub(super) const MIN_BAND_TILES: usize = 4;

/// Fewest samples that make a tile count.
pub(super) const MIN_TILE_SAMPLES: u32 = 4;

/// Tiles reaching outside this encoded range are skipped: clipping hides noise.
pub(super) const CLIP_LO: f32 = 0.002;

pub(super) const CLIP_HI: f32 = 0.99;

#[derive(Debug, Clone, Copy)]
pub(super) struct TileStat {
    /// Mean encoded luminance.
    pub(super) lum: f32,
    /// Mean noise power.
    pub(super) power: f32,
}

/// Tile accumulator; `power` holds up to three noise signals (identical when
/// there is one), of which the least is taken.
#[derive(Debug, Clone, Copy)]
pub(super) struct TileAcc {
    pub(super) power: [f32; 3],
    pub(super) lum: f32,
    pub(super) lo: f32,
    pub(super) hi: f32,
    pub(super) n: u32,
}

impl Default for TileAcc {
    fn default() -> Self {
        TileAcc {
            power: [0.0; 3],
            lum: 0.0,
            lo: f32::INFINITY,
            hi: f32::NEG_INFINITY,
            n: 0,
        }
    }
}

impl TileAcc {
    #[inline]
    pub(super) fn add(&mut self, power: [f32; 3], lum: f32, lo: f32, hi: f32) {
        self.power = add3(self.power, power);
        self.lum += lum;
        self.lo = self.lo.min(lo);
        self.hi = self.hi.max(hi);
        self.n += 1;
    }

    pub(super) fn finish(self) -> Option<TileStat> {
        let n = self.n as f32;
        let stat = TileStat {
            lum: self.lum / n,
            power: self.power[0].min(self.power[1]).min(self.power[2]) / n,
        };
        (self.n >= MIN_TILE_SAMPLES
            && self.lo >= CLIP_LO
            && self.hi <= CLIP_HI
            && stat.power.is_finite())
        .then_some(stat)
    }
}

/// Noise sigma (encoded) per band of encoded luminance.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct NoiseProfile {
    pub(super) sigma: [f32; NOISE_BANDS],
}

impl NoiseProfile {
    /// Per band, the square root of the `percentile` of its tile powers. Bands
    /// with too few tiles are interpolated from the nearest populated ones
    /// (constant beyond them); with no populated band all tiles are pooled, and
    /// with no tile at all the profile is zero (the floors then apply).
    pub(super) fn from_tiles(tiles: &[TileStat], percentile: f64) -> NoiseProfile {
        let q = slider(percentile, DEFAULT_STRENGTHS.noise_percentile, 0.0, 1.0);
        let pick = |v: &mut [f32]| -> f32 {
            let i = ((v.len() - 1) as f64 * q).round() as usize;
            let (_, p, _) = v.select_nth_unstable_by(i, f32::total_cmp);
            p.max(0.0).sqrt()
        };
        let mut bands: Vec<Vec<f32>> = vec![Vec::new(); NOISE_BANDS];
        for t in tiles {
            let b = ((t.lum * NOISE_BANDS as f32) as usize).min(NOISE_BANDS - 1);
            bands[b].push(t.power);
        }
        let known: Vec<(usize, f32)> = bands
            .iter_mut()
            .enumerate()
            .filter(|(_, v)| v.len() >= MIN_BAND_TILES)
            .map(|(i, v)| (i, pick(v)))
            .collect();
        if known.is_empty() {
            let mut all: Vec<f32> = tiles.iter().map(|t| t.power).collect();
            let sigma = if all.is_empty() { 0.0 } else { pick(&mut all) };
            return NoiseProfile {
                sigma: [sigma; NOISE_BANDS],
            };
        }
        let sigma = std::array::from_fn(|i| {
            let left = known.iter().rev().find(|(j, _)| *j <= i);
            let right = known.iter().find(|(j, _)| *j >= i);
            match (left, right) {
                (Some(&(a, va)), Some(&(b, vb))) if b > a => {
                    va + (vb - va) * (i - a) as f32 / (b - a) as f32
                }
                (Some(&(_, v)), _) | (None, Some(&(_, v))) => v,
                (None, None) => 0.0,
            }
        });
        NoiseProfile { sigma }
    }

    /// Sigma at encoded luminance `l` (linear between band centres).
    pub(super) fn at(&self, l: f32) -> f32 {
        let pos = (unit(l) * NOISE_BANDS as f32 - 0.5).clamp(0.0, (NOISE_BANDS - 1) as f32);
        let i = (pos as usize).min(NOISE_BANDS - 2);
        let f = pos - i as f32;
        self.sigma[i] + (self.sigma[i + 1] - self.sigma[i]) * f
    }
}
