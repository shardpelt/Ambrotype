// SPDX-License-Identifier: GPL-3.0-or-later
//! Colour noise reduction: chroma smoothing with the luminance held fixed.

use super::*;

/// Colour noise reduction as numbers (for other backends too): blended in
/// by `mix`; the chroma is smoothed by a guided filter on a copy downscaled
/// by `factor` (box radius `r_low` there); with Detail above 0 a garrote
/// keeps residuals above `detail_factor` times the measured noise.
#[derive(Debug, Clone, PartialEq)]
pub struct ColorNrPlan {
    pub mix: f32,
    pub factor: usize,
    pub r_low: f32,
    pub eps: f32,
    pub max_slope: f32,
    pub detail_factor: Option<f32>,
    /// The garrote's exponent.
    pub garrote_power: f64,
    pub(super) percentile: f64,
    pub(super) ceiling: f32,
    pub(super) floor: f32,
}

/// [`ColorNrPlan`] for a `w × h` image, or `None` when it does nothing.
pub fn color_nr_plan(
    p: &NoiseReduction,
    k: &DetailStrengths,
    w: usize,
    h: usize,
) -> Option<ColorNrPlan> {
    if !(p.color > 0.0 && p.color.is_finite()) || w == 0 || h == 0 {
        return None;
    }
    let s = p.color.min(100.0) / 100.0;
    let detail = slider(p.color_detail, 50.0, 0.0, 100.0) / 100.0;
    let smoothness = slider(p.color_smoothness, 50.0, 0.0, 100.0) / 100.0;
    let mix = (s / k.color_full_strength.max(1e-6)).min(1.0) as f32;
    if !mix.is_finite() || mix <= 0.0 {
        return None;
    }
    // Scale: a fixed downscale factor and the full-resolution radius divided by
    // it (fractional), so the output changes continuously with the sliders.
    let radius = (k.color_radius
        + k.color_radius_per_strength.max(0.0) * s
        + k.color_radius_per_smoothness.max(0.0) * smoothness)
        .clamp(1.0, MAX_COLOR_RADIUS);
    let factor = k
        .color_max_downscale
        .round()
        .clamp(1.0, MAX_DOWNSCALE)
        .min(w.min(h) as f64) as usize;
    Some(ColorNrPlan {
        mix,
        factor,
        r_low: (radius / factor as f64) as f32,
        eps: k.color_guide_eps.max(1e-9) as f32,
        max_slope: k.color_guide_max_slope.max(0.0) as f32,
        detail_factor: color_detail_factor(detail, s, k),
        garrote_power: k.color_garrote_power,
        percentile: k.noise_percentile,
        ceiling: k.color_noise_ceiling.max(0.0) as f32,
        floor: k.color_noise_floor.max(0.0) as f32,
    })
}

/// The low-resolution half of colour noise reduction: the chroma's
/// guided-filter coefficients (per channel `a` then `b`, on the `sw × sh`
/// grid) and, with Detail, the noise tiles of the low-resolution chroma.
pub struct ColorNrLow {
    pub coefs: Vec<(Vec<f32>, Vec<f32>)>,
    pub sw: usize,
    pub sh: usize,
    pub(super) low_tiles: Option<Vec<TileStat>>,
}

/// [`ColorNrLow`] from the image downscaled by `plan.factor` (as
/// `downscale` makes it: non-finite pixels left out, channels clamped).
pub fn color_nr_low(small: &[f32], sw: usize, sh: usize, plan: &ColorNrPlan) -> ColorNrLow {
    let t = transfer();
    let lw = luminance_weights();
    // Centred encoded luminance (the guide) and chroma.
    let planes: Vec<[f32; 4]> = small
        .par_chunks_exact(3)
        .map(|p| {
            let le = t.encode(dot(lw, p));
            [
                le - CENTRE,
                t.encode(p[0]) - le,
                t.encode(p[1]) - le,
                t.encode(p[2]) - le,
            ]
        })
        .collect();
    let plane = |c: usize| -> Vec<f32> { planes.par_iter().map(|v| v[c]).collect() };
    let guide = plane(0);
    let chroma = [plane(1), plane(2), plane(3)];
    drop(planes);
    // Haar tiles of the low-resolution chroma (for the noise ceiling).
    let low_tiles = plan.detail_factor.map(|_| {
        let tile = ((NOISE_TILE / plan.factor) & !1).max(2);
        let srcs = [&chroma[0][..], &chroma[1][..], &chroma[2][..]];
        haar_tiles(&guide, CENTRE, &srcs, sw, sh, tile, true)
    });
    let coefs = guided_coefficients(
        &guide,
        &chroma,
        sw,
        sh,
        plan.r_low,
        plan.eps,
        plan.max_slope,
    );
    ColorNrLow {
        coefs,
        sw,
        sh,
        low_tiles,
    }
}

/// Raw sums over one noise tile, as a backend gathers them (see [`TileAcc`]).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TileSums {
    pub power: [f32; 3],
    pub lum: f32,
    pub lo: f32,
    pub hi: f32,
    pub n: u32,
}

/// The garrote threshold by encoded luminance ([`LUT_STEPS`] + 1 values on
/// 0..1), from the residual-energy tiles and the directional chroma detail
/// tiles at full resolution, or `None` without Detail.
pub fn color_nr_tau(
    low: &ColorNrLow,
    energy: &[TileSums],
    fine: &[TileSums],
    plan: &ColorNrPlan,
) -> Option<Vec<f32>> {
    let finish = |t: &[TileSums]| -> Vec<TileStat> {
        t.iter()
            .filter_map(|t| TileAcc::from(*t).finish())
            .collect()
    };
    let tau = tau_lut(low, &finish(energy), &finish(fine), plan)?;
    Some(tau.0.to_vec())
}

pub(super) fn tau_lut(
    low: &ColorNrLow,
    tiles: &[TileStat],
    fine_tiles: &[TileStat],
    plan: &ColorNrPlan,
) -> Option<Lut> {
    let f = plan.detail_factor?;
    let q = plan.percentile;
    let noise = NoiseProfile::from_tiles(tiles, q);
    // Ceiling from the directional chroma detail at full and low resolution
    // (white noise of σ reads σ/factor on the downscaled chroma).
    let fine = NoiseProfile::from_tiles(fine_tiles, q);
    let coarse = NoiseProfile::from_tiles(low.low_tiles.as_deref().unwrap_or(&[]), q);
    let factor = plan.factor as f32;
    Some(Lut::from_fn(|v| {
        let cap = plan.ceiling * fine.at(v).max(factor * coarse.at(v));
        f * (noise.at(v).min(cap) + plan.floor)
    }))
}

pub(super) fn color_noise_reduction(img: &mut Image, p: &NoiseReduction, k: &DetailStrengths) {
    let (w, h) = (img.width, img.height);
    let Some(plan) = color_nr_plan(p, k, w, h) else {
        return;
    };
    let mix = plan.mix;
    let factor = plan.factor;
    let t = transfer();
    let lw = luminance_weights();

    // 1-2. Low resolution: guide, chroma and their guided-filter coefficients.
    let (small, sw, sh) = downscale(img, factor);
    let low = color_nr_low(&small, sw, sh, &plan);
    drop(small);
    let grids = CoefGrids::new(&low.coefs, w, sw, sh, factor);

    // 3. Detail: residual energy per pixel, and the garrote threshold per
    //    luminance from its noise level.
    let detail_test: Option<(Vec<f32>, Lut)> = plan.detail_factor.and_then(|_| {
        let (energy, tiles, fine_tiles) = residual_energy(img, &grids);
        let tau = tau_lut(&low, &tiles, &fine_tiles, &plan)?;
        Some((energy, tau))
    });
    let garrote = Garrote::new(plan.garrote_power);

    // 4. Full resolution: denoised chroma = smoothed + garrote(residual), blended
    //    in by `mix`; luminance kept.
    img.rgb.par_chunks_mut(w * 3).enumerate().for_each_init(
        || vec![0.0f32; 6 * sw],
        |rows, (y, row)| {
            grids.interpolate_rows(y, rows);
            for (x, px) in row.chunks_exact_mut(3).enumerate() {
                let Some(c) = chroma_residual(px, &grids, rows, x) else {
                    continue;
                };
                let r2 = c.r[0] * c.r[0] + c.r[1] * c.r[1] + c.r[2] * c.r[2];
                let keep = detail_test.as_ref().map_or(0.0, |(energy, tau)| {
                    let tv = tau.at(c.le);
                    garrote.keep(tv * tv / neighbour_energy(energy, w, h, x, y))
                });
                // New chroma `smoothed + keep·r`, blended by `mix`: a change of
                // `mix·(keep − 1)·r` to each encoded channel.
                let f = mix * (keep - 1.0);
                if f == 0.0 || r2 == 0.0 {
                    continue;
                }
                let mut out = [0.0f32; 3];
                for (o, (&e, &r)) in out.iter_mut().zip(c.e.iter().zip(&c.r)) {
                    // Never push a channel below zero that was not negative already.
                    *o = t.decode((e + f * r).max(e.min(0.0)));
                }
                let y_new = dot(lw, &out);
                if y_new > MIN_SCALE_Y {
                    let scale = c.y / y_new;
                    for (v, o) in px.iter_mut().zip(out) {
                        *v = o * scale;
                    }
                }
            }
        },
    );
}

/// Mean residual energy of the 8 neighbours of `(x, y)` (edges clamped): the
/// evidence that the pixel's residual is detail. Edges and texture show it on
/// both sides; an isolated speckle, however strong, does not.
#[inline]
pub(super) fn neighbour_energy(energy: &[f32], w: usize, h: usize, x: usize, y: usize) -> f32 {
    let (xm, xp) = (x.saturating_sub(1), (x + 1).min(w - 1));
    let row = |yy: usize| &energy[yy * w..(yy + 1) * w];
    let (up, mid, down) = (row(y.saturating_sub(1)), row(y), row((y + 1).min(h - 1)));
    (up[xm] + up[x] + up[xp] + mid[xm] + mid[xp] + down[xm] + down[x] + down[xp]) / 8.0
}

/// Garrote threshold factor (in noise sigmas) for a Detail value in 0..1 and a
/// strength in 0..1; `None` (keep no residual) at Detail 0.
pub(super) fn color_detail_factor(detail: f64, s: f64, k: &DetailStrengths) -> Option<f32> {
    if detail.is_nan() || detail <= 0.0 {
        return None;
    }
    let at_100 = k.color_detail_threshold.max(0.0);
    let falloff = detail.powf(-k.color_detail_power.max(0.0));
    let f = at_100 * falloff * (1.0 + k.color_threshold_per_strength.max(0.0) * s);
    f.is_finite().then_some(f as f32)
}

/// Share `max(0, 1 − (τ/|r|)^p)` of a residual kept by the garrote, from
/// `q = τ²/|r|²` (fast paths for the common exponents).
#[derive(Debug, Clone, Copy)]
pub(super) enum Garrote {
    Two,
    Four,
    Power(f32),
}

impl Garrote {
    pub(super) fn new(p: f64) -> Garrote {
        let p = slider(p, DEFAULT_STRENGTHS.color_garrote_power, 0.1, 64.0);
        if p == 2.0 {
            Garrote::Two
        } else if p == 4.0 {
            Garrote::Four
        } else {
            Garrote::Power((p / 2.0) as f32)
        }
    }

    #[inline]
    pub(super) fn keep(self, q: f32) -> f32 {
        // q ≥ 1 (|r| ≤ τ) and NaN (r = τ = 0) keep nothing.
        if q.is_nan() || q >= 1.0 {
            return 0.0;
        }
        1.0 - match self {
            Garrote::Two => q,
            Garrote::Four => q * q,
            Garrote::Power(half) => q.powf(half),
        }
    }
}

/// Upsampling of the low-resolution guided-filter coefficients: per output row
/// the six grids (`a` then `b` for R, G, B) are interpolated vertically into a
/// row buffer once, then horizontally per pixel.
pub(super) struct CoefGrids<'a> {
    pub(super) grids: [&'a [f32]; 6],
    pub(super) sw: usize,
    pub(super) sh: usize,
    pub(super) factor: usize,
    pub(super) columns: Vec<(usize, usize, f32)>,
}

impl<'a> CoefGrids<'a> {
    pub(super) fn new(
        coefs: &'a [(Vec<f32>, Vec<f32>)],
        w: usize,
        sw: usize,
        sh: usize,
        factor: usize,
    ) -> Self {
        CoefGrids {
            grids: [
                &coefs[0].0,
                &coefs[1].0,
                &coefs[2].0,
                &coefs[0].1,
                &coefs[1].1,
                &coefs[2].1,
            ],
            sw,
            sh,
            factor,
            columns: (0..w).map(|x| lerp_position(x, factor, sw)).collect(),
        }
    }

    /// Vertical interpolation of every grid for full-resolution row `y` into `rows` (6 × sw).
    pub(super) fn interpolate_rows(&self, y: usize, rows: &mut [f32]) {
        let sw = self.sw;
        let (y0, y1, wy) = lerp_position(y, self.factor, self.sh);
        for (g, dst) in self.grids.iter().zip(rows.chunks_exact_mut(sw)) {
            let (p0, p1) = (&g[y0 * sw..(y0 + 1) * sw], &g[y1 * sw..(y1 + 1) * sw]);
            for ((d, &a), &b) in dst.iter_mut().zip(p0).zip(p1) {
                *d = a + (b - a) * wy;
            }
        }
    }

    /// Coefficients at column `x` from rows filled by [`Self::interpolate_rows`].
    #[inline]
    pub(super) fn at(&self, rows: &[f32], x: usize) -> [f32; 6] {
        let (x0, x1, wx) = self.columns[x];
        std::array::from_fn(|g| {
            let r = &rows[g * self.sw..(g + 1) * self.sw];
            r[x0] + (r[x1] - r[x0]) * wx
        })
    }
}

/// One pixel's encoded channels and its chroma residual against the smoothed
/// chroma (`a · (le − CENTRE) + b` per channel).
pub(super) struct ChromaResidual {
    /// Linear luminance.
    pub(super) y: f32,
    /// Encoded luminance.
    pub(super) le: f32,
    /// Encoded R, G, B.
    pub(super) e: [f32; 3],
    /// Chroma minus smoothed chroma.
    pub(super) r: [f32; 3],
}

#[inline]
pub(super) fn chroma_residual(
    px: &[f32],
    grids: &CoefGrids,
    rows: &[f32],
    x: usize,
) -> Option<ChromaResidual> {
    let t = transfer();
    let y = dot(luminance_weights(), px);
    // Too dark to rescale, or not finite (left untouched).
    if !(y > MIN_SCALE_Y && y.is_finite() && px.iter().all(|v| v.is_finite())) {
        return None;
    }
    let le = t.encode(y);
    let lc = le - CENTRE;
    let coef = grids.at(rows, x);
    let e = [t.encode(px[0]), t.encode(px[1]), t.encode(px[2])];
    let r = std::array::from_fn(|c| e[c] - le - (coef[c] * lc + coef[3 + c]));
    Some(ChromaResidual { y, le, e, r })
}

/// The colour-NR residual energy `|r|²` (summed over channels) per pixel (0
/// where the pixel is too dark to scale or not finite), its tile statistics,
/// and from the same pass the tile statistics of the directional detail of
/// the chroma (the least of the three [`haar_powers`], summed over channels).
pub(super) fn residual_energy(
    img: &Image,
    grids: &CoefGrids,
) -> (Vec<f32>, Vec<TileStat>, Vec<TileStat>) {
    let (w, h) = (img.width, img.height);
    let t = transfer();
    let lw = luminance_weights();
    let mut energy = vec![0.0f32; w * h];
    let (tiles, fine): (Vec<Vec<TileStat>>, Vec<Vec<TileStat>>) = energy
        .par_chunks_mut(NOISE_TILE * w)
        .enumerate()
        .map(|(ty, band)| {
            let mut rows = vec![0.0f32; 6 * grids.sw];
            let mut acc = vec![TileAcc::default(); w.div_ceil(NOISE_TILE)];
            let mut haar = vec![TileAcc::default(); w.div_ceil(NOISE_TILE)];
            // Encoded luminance and chroma of the previous and current rows.
            let (mut prev, mut cur) = (vec![[0.0f32; 4]; w], vec![[0.0f32; 4]; w]);
            for (dy, out) in band.chunks_exact_mut(w).enumerate() {
                let y = ty * NOISE_TILE + dy;
                grids.interpolate_rows(y, &mut rows);
                let row = &img.rgb[y * w * 3..(y + 1) * w * 3];
                for (x, ((o, px), lc)) in out
                    .iter_mut()
                    .zip(row.chunks_exact(3))
                    .zip(cur.iter_mut())
                    .enumerate()
                {
                    if let Some(c) = chroma_residual(px, grids, &rows, x) {
                        *o = c.r[0] * c.r[0] + c.r[1] * c.r[1] + c.r[2] * c.r[2];
                        acc[x / NOISE_TILE].add([*o; 3], c.le, c.le, c.le);
                        *lc = [c.le, c.e[0] - c.le, c.e[1] - c.le, c.e[2] - c.le];
                    } else {
                        *lc = [t.encode(bounded(dot(lw, px))), 0.0, 0.0, 0.0];
                    }
                }
                if dy % 2 == 1 {
                    for (cx, (p0, p1)) in prev.chunks_exact(2).zip(cur.chunks_exact(2)).enumerate()
                    {
                        let block = |i: usize| [p0[0][i], p0[1][i], p1[0][i], p1[1][i]];
                        let power = (1..4).fold([0.0; 3], |p, i| add3(p, haar_powers(block(i))));
                        add_haar_block(&mut haar[2 * cx / NOISE_TILE], block(0), 0.0, power);
                    }
                }
                std::mem::swap(&mut prev, &mut cur);
            }
            let finish = |a: Vec<TileAcc>| a.into_iter().filter_map(TileAcc::finish).collect();
            (finish(acc), finish(haar))
        })
        .unzip();
    (energy, tiles.concat(), fine.concat())
}
