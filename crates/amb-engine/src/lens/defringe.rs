// SPDX-License-Identifier: GPL-3.0-or-later
//! Defringe: the manual and automatic passes on the working-space image.

use super::*;

/// Defringe's search radius in pixels on an output of `tile`'s full size.
pub fn defringe_radius(d: &Defringe, k: &LensStrengths, tile: &Tile) -> usize {
    let resolution = ((tile.full_width * tile.full_height) as f64 / 24e6).sqrt();
    // Auto searches as far as the sliders at 4 (of 20) do.
    let amount =
        d.purple_amount
            .max(d.green_amount)
            .max(if d.auto { AUTO_FRINGE_AMOUNT } else { 0.0 });
    ((1.0 + amount * k.defringe_radius) * resolution)
        .round()
        .max(1.0) as usize
}

/// Defringe on the working-space image (scene-linear ProPhoto): desaturate
/// purple and green fringes next to high-contrast edges.
pub fn apply_defringe(img: &mut Image, d: &Defringe, k: &LensStrengths) {
    let tile = Tile::of(img);
    apply_defringe_in(img, d, k, &tile);
}

/// [`apply_defringe`] on a part of the output (`tile`), with the search
/// radius of the full output.
pub fn apply_defringe_in(img: &mut Image, d: &Defringe, k: &LensStrengths, tile: &Tile) {
    apply_defringe_weighted(img, d, k, tile, None);
    if d.auto {
        apply_auto_defringe_in(img, d, k, tile);
    }
}

/// Auto's search reach, as if the Amount sliders were at this.
const AUTO_FRINGE_AMOUNT: f64 = 4.0;
/// OkLab hues (degrees) fringes come in: green to cyan, and blue through
/// purple to magenta. Reds, oranges and yellows are left alone (skin,
/// sunsets).
const FRINGE_HUES: [(f64, f64); 2] = [(115.0, 205.0), (235.0, 350.0)];

/// Automatic fringe removal (`amb:DefringeAuto`). **Ours.** Lateral
/// chromatic aberration is fixed by moving red and blue (Remove Chromatic
/// Aberration); what remains along high-contrast edges is a thin line of
/// colour: purple or blue on one side, green or cyan on the other. Such a
/// line's colour is not the colour of the area around it. So near edges,
/// a pixel in a fringe hue loses the part of its colour that stands out
/// from its neighbourhood's average colour (its excess chroma), and keeps
/// the rest: a blue sky or a green leaf at an edge stays blue or green.
pub fn apply_auto_defringe_in(img: &mut Image, d: &Defringe, k: &LensStrengths, tile: &Tile) {
    if !d.auto || img.width < 3 || img.height < 3 {
        return;
    }
    let (w, h) = (img.width, img.height);
    let oklab = OkLab::new();
    let positive = |p: &[f32]| [p[0].max(0.0), p[1].max(0.0), p[2].max(0.0)];
    let lab: Vec<[f32; 3]> = img
        .rgb
        .par_chunks(3)
        .map(|p| oklab.from_working(positive(p)))
        .collect();
    let radius = defringe_radius(d, k, tile);
    let lightness: Vec<f32> = lab.iter().map(|l| l[0]).collect();
    let contrast = local_range(&lightness, w, h, radius);
    let (clo, chi) = (k.defringe_contrast_lo, k.defringe_contrast_hi);
    let edge: Vec<f32> = contrast
        .iter()
        .map(|&c| smoothstep(clo, chi, f64::from(c)) as f32)
        .collect();
    // The surroundings' colour: a, b averaged over three times the reach,
    // from the flat pixels only (the fringe and its edge left out).
    let flat: Vec<f32> = edge.iter().map(|e| 1.0 - e).collect();
    let weighted = |c: usize| {
        let v: Vec<f32> = lab.iter().zip(&flat).map(|(l, f)| l[c] * f).collect();
        crate::filters::box_blur(&v, w, h, 3 * radius)
    };
    let (a_sum, b_sum, f_sum) = (
        weighted(1),
        weighted(2),
        crate::filters::box_blur(&flat, w, h, 3 * radius),
    );
    let mean = |i: usize| {
        if f_sum[i] > 1e-3 {
            (a_sum[i] / f_sum[i], b_sum[i] / f_sum[i])
        } else {
            (0.0, 0.0)
        }
    };
    let in_hues = |hue: f64| {
        FRINGE_HUES
            .iter()
            .map(|&(lo, hi)| {
                smoothstep(lo - 10.0, lo, hue) * (1.0 - smoothstep(hi, hi + 10.0, hue))
            })
            .fold(0.0, f64::max)
    };
    img.rgb.par_chunks_mut(3).enumerate().for_each(|(i, p)| {
        let edge = f64::from(edge[i]);
        if edge <= 0.0 {
            return;
        }
        let l = lab[i];
        let chroma = f64::from(l[1].hypot(l[2]));
        if chroma < 0.01 {
            return;
        }
        let hue = f64::from(l[2].atan2(l[1])).to_degrees().rem_euclid(360.0);
        let fringe = in_hues(hue);
        if fringe <= 0.0 {
            return;
        }
        // The share of the colour not explained by the surroundings:
        // the pixel's chroma beyond its projection on the mean colour.
        let (ma, mb) = mean(i);
        let (ma, mb) = (f64::from(ma), f64::from(mb));
        let along = (f64::from(l[1]) * ma + f64::from(l[2]) * mb) / chroma;
        let excess = ((chroma - along.max(0.0)) / chroma).clamp(0.0, 1.0);
        let wgt = (edge * fringe * excess) as f32;
        if wgt <= 0.0 {
            return;
        }
        let keep = 1.0 - wgt;
        let out = oklab.to_working([l[0], l[1] * keep, l[2] * keep]);
        p.copy_from_slice(&out);
    });
}

/// [`apply_defringe_in`] with its strength scaled per pixel by `weight`
/// (0..1; masks use it).
pub fn apply_defringe_weighted(
    img: &mut Image,
    d: &Defringe,
    k: &LensStrengths,
    tile: &Tile,
    weight: Option<&[f32]>,
) {
    let sliders = d.purple_amount > 0.0 || d.green_amount > 0.0;
    if !sliders || img.width < 3 || img.height < 3 {
        return;
    }
    let (w, h) = (img.width, img.height);
    let oklab = OkLab::new();
    let positive = |p: &[f32]| [p[0].max(0.0), p[1].max(0.0), p[2].max(0.0)];
    let lightness: Vec<f32> = img
        .rgb
        .par_chunks(3)
        .map(|p| oklab.from_working(positive(p))[0])
        .collect();
    let radius = defringe_radius(d, k, tile);
    let contrast = local_range(&lightness, w, h, radius);

    let band = |amount: f64, lo: f64, hi: f64, start: f64, span: f64| {
        let (lo, hi) = (lo.min(hi), lo.max(hi));
        (
            (amount / 4.0).clamp(0.0, 1.0),
            start + lo / 100.0 * span,
            start + hi / 100.0 * span,
        )
    };
    let purple = band(
        d.purple_amount,
        d.purple_hue_lo,
        d.purple_hue_hi,
        k.purple_hue_start,
        k.purple_hue_span,
    );
    let green = band(
        d.green_amount,
        d.green_hue_lo,
        d.green_hue_hi,
        k.green_hue_start,
        k.green_hue_span,
    );
    // Weight of hue (degrees) inside [lo, hi] with an 8° feather, on the circle.
    let hue_weight = |hue: f64, lo: f64, hi: f64| -> f64 {
        let centre = 0.5 * (lo + hi);
        let half = 0.5 * (hi - lo);
        let dist = ((hue - centre + 540.0).rem_euclid(360.0) - 180.0).abs();
        1.0 - smoothstep(half, half + 8.0, dist)
    };
    let (clo, chi) = (k.defringe_contrast_lo, k.defringe_contrast_hi);
    img.rgb
        .par_chunks_mut(3)
        .zip(contrast.par_iter())
        .enumerate()
        .for_each(|(i, (p, c))| {
            let edge = smoothstep(clo, chi, f64::from(*c))
                * weight.map_or(1.0, |w| f64::from(w[i].clamp(0.0, 1.0)));
            if edge <= 0.0 {
                return;
            }
            let l = oklab.from_working(positive(p));
            let chroma = f64::from(l[1].hypot(l[2]));
            if chroma < 0.01 {
                return;
            }
            let hue = f64::from(l[2].atan2(l[1])).to_degrees().rem_euclid(360.0);
            let wp = purple.0 * hue_weight(hue, purple.1, purple.2);
            let wg = green.0 * hue_weight(hue, green.1, green.2);
            let wgt = (edge * wp.max(wg)).clamp(0.0, 1.0) as f32;
            if wgt <= 0.0 {
                return;
            }
            let out = oklab.to_working([l[0], l[1] * (1.0 - wgt), l[2] * (1.0 - wgt)]);
            p.copy_from_slice(&out);
        });
}

/// max − min of `plane` over a (2r+1)² window, separable.
fn local_range(plane: &[f32], w: usize, h: usize, r: usize) -> Vec<f32> {
    // One pass along rows of length `width`, producing (max, min) planes.
    let pass = |max_src: &[f32], min_src: &[f32], width: usize| -> (Vec<f32>, Vec<f32>) {
        let mut max_out = vec![0.0f32; max_src.len()];
        let mut min_out = vec![0.0f32; min_src.len()];
        max_out
            .par_chunks_mut(width)
            .zip(min_out.par_chunks_mut(width))
            .zip(max_src.par_chunks(width).zip(min_src.par_chunks(width)))
            .for_each(|((mo, mi), (ms, ns))| {
                for x in 0..width {
                    let lo = x.saturating_sub(r);
                    let hi = (x + r).min(width - 1);
                    mo[x] = ms[lo..=hi].iter().copied().fold(f32::MIN, f32::max);
                    mi[x] = ns[lo..=hi].iter().copied().fold(f32::MAX, f32::min);
                }
            });
        (max_out, min_out)
    };
    let (max_h, min_h) = pass(plane, plane, w);
    let (max_v, min_v) = pass(&transpose(&max_h, w, h), &transpose(&min_h, w, h), h);
    let range: Vec<f32> = max_v.par_iter().zip(&min_v).map(|(a, b)| a - b).collect();
    transpose(&range, h, w)
}
