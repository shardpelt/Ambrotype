// SPDX-License-Identifier: GPL-3.0-or-later
//! Luminance noise reduction: self-guided smoothing scaled by the measured noise.

use super::*;

/// Luminance noise reduction as numbers (for other backends): self-guided
/// filters of radius `r_fine` (then `coarse`: radius and eps scale) with eps
/// from the measured noise ([`luminance_nr_tables`]); Detail returns
/// `restore` at most of the removed signal where its 3×3 energy clears
/// `threshold` times the noise; Contrast adds `contrast_gain` times a
/// `contrast_blur` of what is still removed.
#[derive(Debug, Clone, PartialEq)]
pub struct LuminanceNrPlan {
    pub r_fine: f32,
    pub coarse: Option<(f32, f32)>,
    pub restore: f32,
    pub threshold: f32,
    pub contrast_gain: f32,
    pub contrast_blur: crate::filters::GaussianPlan,
    pub max_ratio: f32,
    pub(super) gain: f32,
    pub(super) percentile: f64,
    pub(super) floor: f32,
}

/// [`LuminanceNrPlan`], or `None` when it does nothing.
pub fn luminance_nr_plan(p: &NoiseReduction, k: &DetailStrengths) -> Option<LuminanceNrPlan> {
    if !(p.luminance > 0.0 && p.luminance.is_finite()) {
        return None;
    }
    let s = p.luminance.min(100.0) / 100.0;
    let detail = (slider(p.luminance_detail, 50.0, 0.0, 100.0) / 100.0) as f32;
    let contrast = (slider(p.luminance_contrast, 0.0, 0.0, 100.0) / 100.0) as f32;
    let gain = (s * k.luminance_noise_gain.max(0.0)) as f32;
    if !gain.is_finite() || gain <= 0.0 {
        return None;
    }
    // Fractional radii (box windows with partial edge taps), so the result
    // changes continuously with the slider.
    let r_fine = (k.luminance_radius + k.luminance_radius_per_strength.max(0.0) * s)
        .clamp(1.0, MAX_LUMINANCE_RADIUS);
    let r_coarse = (r_fine * k.luminance_coarse_radius.max(0.0)).min(MAX_LUMINANCE_RADIUS);
    let coarse_eps = k.luminance_coarse_eps.max(0.0) as f32;
    Some(LuminanceNrPlan {
        r_fine: r_fine as f32,
        coarse: (r_coarse > 0.0 && coarse_eps > 0.0).then_some((r_coarse as f32, coarse_eps)),
        restore: k.luminance_detail_restore.max(0.0) as f32 * detail,
        threshold: k.luminance_detail_threshold.max(0.0) as f32,
        contrast_gain: k.luminance_contrast_restore.max(0.0) as f32 * contrast,
        contrast_blur: crate::filters::gaussian_plan(
            k.luminance_contrast_sigma.clamp(0.0, MAX_BLUR_SIGMA) as f32,
        ),
        max_ratio: k.max_luminance_ratio.max(1.0) as f32,
        gain,
        percentile: k.noise_percentile,
        floor: k.luminance_noise_floor.max(0.0) as f32,
    })
}

/// Luminance noise reduction's tables by encoded luminance ([`LUT_STEPS`] + 1
/// values on 0..1): the noise sigma and the guided filters' eps.
#[derive(Debug, Clone, PartialEq)]
pub struct LuminanceNrTables {
    pub sigma: Vec<f32>,
    pub eps: Vec<f32>,
}

/// The tables from the noise tiles a backend gathered (the Haar diagonal
/// detail of the centred encoded luminance over 16×16 tiles, as
/// `luminance_tiles` does), or `None` when they make it do nothing.
pub fn luminance_nr_tables(
    tiles: &[TileSums],
    plan: &LuminanceNrPlan,
) -> Option<LuminanceNrTables> {
    let tiles: Vec<TileStat> = tiles
        .iter()
        .filter_map(|t| TileAcc::from(*t).finish())
        .collect();
    let (sigma, eps) = luminance_luts(&tiles, plan)?;
    Some(LuminanceNrTables {
        sigma: sigma.0.to_vec(),
        eps: eps.0.to_vec(),
    })
}

pub(super) fn luminance_luts(tiles: &[TileStat], plan: &LuminanceNrPlan) -> Option<(Lut, Lut)> {
    let noise = NoiseProfile::from_tiles(tiles, plan.percentile);
    let sigma_n = Lut::from_fn(|v| noise.at(v) + plan.floor);
    let eps = Lut::from_fn(|v| (plan.gain * sigma_n.at(v)).powi(2));
    (eps.max() > 0.0 && eps.max().is_finite()).then_some((sigma_n, eps))
}

pub(super) fn luminance_noise_reduction(img: &mut Image, p: &NoiseReduction, k: &DetailStrengths) {
    let (w, h) = (img.width, img.height);
    let Some(plan) = luminance_nr_plan(p, k) else {
        return;
    };
    let t = transfer();
    let lw = luminance_weights();
    let max_ratio = plan.max_ratio;

    // 1. Noise level per luminance, then two-scale self-guided smoothing of the
    //    centred encoded luminance with eps set by it.
    let l = encoded_luminance(img, CENTRE);
    let Some((sigma_n, eps)) = luminance_luts(&luminance_tiles(&l, w, h), &plan) else {
        return;
    };
    let mut smooth = self_guided(&l, w, h, plan.r_fine, &eps, 1.0);
    if let Some((r_coarse, coarse_eps)) = plan.coarse {
        smooth = self_guided(&smooth, w, h, r_coarse, &eps, coarse_eps);
    }

    // 2. Detail: share of the removed signal returned per pixel, from its local
    //    energy against the noise level.
    let removed: Vec<f32> = l.par_iter().zip(&smooth).map(|(a, b)| a - b).collect();
    let (restore, threshold) = (plan.restore, plan.threshold);
    let kept: Option<Vec<f32>> = (restore > 0.0).then(|| {
        let sq: Vec<f32> = removed.par_iter().map(|v| v * v).collect();
        let mut energy = box_blur(&sq, w, h, 1);
        energy.par_iter_mut().zip(&smooth).for_each(|(e, &m)| {
            // Soft threshold: ~0 for noise-level energy, → `restore` for detail.
            let tau = threshold * sigma_n.at(m + CENTRE);
            *e = if tau > 0.0 {
                let q = *e / (tau * tau);
                restore * q * q / (1.0 + q * q)
            } else {
                restore
            };
        });
        energy
    });

    // 3. Contrast: low-pass of what is still removed after the Detail step.
    let contrast_gain = plan.contrast_gain;
    let lowpass: Option<Vec<f32>> = (contrast_gain > 0.0).then(|| {
        let rest: Vec<f32> = match &kept {
            Some(kept) => removed
                .par_iter()
                .zip(kept)
                .map(|(r, f)| r * (1.0 - f))
                .collect(),
            None => removed.clone(),
        };
        gaussian_by(&rest, w, h, &plan.contrast_blur)
    });

    img.rgb
        .par_chunks_mut(w * 3)
        .enumerate()
        .for_each(|(y, row)| {
            let span = y * w..(y + 1) * w;
            let (lrow, srow, rrow) = (
                &l[span.clone()],
                &smooth[span.clone()],
                &removed[span.clone()],
            );
            let krow = kept.as_ref().map(|v| &v[span.clone()]);
            let prow = lowpass.as_ref().map(|v| &v[span.clone()]);
            for (x, px) in row.chunks_exact_mut(3).enumerate() {
                let mut v = srow[x];
                if let Some(krow) = krow {
                    v += rrow[x] * krow[x];
                }
                if let Some(prow) = prow {
                    v += contrast_gain * prow[x];
                }
                let lv = lrow[x];
                if v == lv {
                    continue;
                }
                let y_old = dot(lw, px);
                if !y_old.is_finite() {
                    continue;
                }
                let y_new =
                    (y_old + t.decode(v + CENTRE) - t.decode(lv + CENTRE)).max(y_old.min(0.0));
                set_luminance(px, y_old, y_new, max_ratio);
            }
        });
}

/// Self-guided filter (guide = source): edge-preserving smoothing where local
/// variance below `eps_scale · eps(window mean)` is flattened. Variance is
/// clamped at zero so `f32` rounding in the running sums can never make the
/// coefficient leave 0..1.
pub(super) fn self_guided(
    src: &[f32],
    w: usize,
    h: usize,
    r: f32,
    eps: &Lut,
    eps_scale: f32,
) -> Vec<f32> {
    let mean = box_blur_frac(src, w, h, r);
    let sq: Vec<f32> = src.par_iter().map(|v| v * v).collect();
    let corr = box_blur_frac(&sq, w, h, r);
    drop(sq);
    let (a, b): (Vec<f32>, Vec<f32>) = mean
        .par_iter()
        .zip(&corr)
        .map(|(&m, &c)| {
            let var = (c - m * m).max(0.0);
            let e = eps_scale * eps.at(m + CENTRE);
            let a = if var + e > 0.0 { var / (var + e) } else { 1.0 };
            (a, m * (1.0 - a))
        })
        .unzip();
    drop((mean, corr));
    let mean_a = box_blur_frac(&a, w, h, r);
    drop(a);
    let mean_b = box_blur_frac(&b, w, h, r);
    src.par_iter()
        .zip(&mean_a)
        .zip(&mean_b)
        .map(|((&i, &a), &b)| a * i + b)
        .collect()
}
