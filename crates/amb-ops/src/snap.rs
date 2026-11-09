// SPDX-License-Identifier: GPL-3.0-or-later
//! Snapping a soft selection to the photo's edges: the object selection
//! model's map is coarse (256 × 256 for the whole photo), so its outline,
//! blown up, comes out in steps. A colour guided filter (He, Sun and
//! Tang, "Guided Image Filtering", 2010) redraws the map from the photo:
//! inside each small window the map becomes a linear function of the
//! colours, so its edge moves onto the photo's own edge where there is
//! one, and is smoothed where there isn't.

/// The soft map `p` (0-1, `w`×`h`) redrawn from `rgb` (0-1, interleaved,
/// the same size) with windows of radius `r` pixels; `eps` is how strong
/// a colour edge must be to be followed (smaller follows fainter edges).
pub fn guided(p: &[f32], rgb: &[f32], w: usize, h: usize, r: usize, eps: f32) -> Vec<f32> {
    let n = w * h;
    assert!(p.len() == n && rgb.len() == n * 3);
    let ch = |c: usize| -> Vec<f32> { (0..n).map(|i| rgb[i * 3 + c]).collect() };
    let (ir, ig, ib) = (ch(0), ch(1), ch(2));
    let mean = |v: &[f32]| box_mean(v, w, h, r);
    let prod = |a: &[f32], b: &[f32]| -> Vec<f32> { a.iter().zip(b).map(|(x, y)| x * y).collect() };
    let (mr, mg, mb, mp) = (mean(&ir), mean(&ig), mean(&ib), mean(p));
    let (mrp, mgp, mbp) = (
        mean(&prod(&ir, p)),
        mean(&prod(&ig, p)),
        mean(&prod(&ib, p)),
    );
    let (mrr, mrg, mrb) = (
        mean(&prod(&ir, &ir)),
        mean(&prod(&ir, &ig)),
        mean(&prod(&ir, &ib)),
    );
    let (mgg, mgb, mbb) = (
        mean(&prod(&ig, &ig)),
        mean(&prod(&ig, &ib)),
        mean(&prod(&ib, &ib)),
    );
    let (mut ar, mut ag, mut ab, mut bb) = (vec![0.0; n], vec![0.0; n], vec![0.0; n], vec![0.0; n]);
    for i in 0..n {
        // The colours' covariance in the window (plus eps), and theirs with p.
        let (s_rr, s_rg, s_rb) = (
            mrr[i] - mr[i] * mr[i] + eps,
            mrg[i] - mr[i] * mg[i],
            mrb[i] - mr[i] * mb[i],
        );
        let (s_gg, s_gb, s_bb) = (
            mgg[i] - mg[i] * mg[i] + eps,
            mgb[i] - mg[i] * mb[i],
            mbb[i] - mb[i] * mb[i] + eps,
        );
        let (cr, cg, cb) = (
            mrp[i] - mr[i] * mp[i],
            mgp[i] - mg[i] * mp[i],
            mbp[i] - mb[i] * mp[i],
        );
        // a = Σ⁻¹ c, by the symmetric 3 × 3 inverse.
        let i_rr = s_gg * s_bb - s_gb * s_gb;
        let i_rg = s_gb * s_rb - s_rg * s_bb;
        let i_rb = s_rg * s_gb - s_gg * s_rb;
        let i_gg = s_rr * s_bb - s_rb * s_rb;
        let i_gb = s_rb * s_rg - s_rr * s_gb;
        let i_bb = s_rr * s_gg - s_rg * s_rg;
        let det = s_rr * i_rr + s_rg * i_rg + s_rb * i_rb;
        let (x, y, z) = if det.abs() > 1e-12 {
            (
                (i_rr * cr + i_rg * cg + i_rb * cb) / det,
                (i_rg * cr + i_gg * cg + i_gb * cb) / det,
                (i_rb * cr + i_gb * cg + i_bb * cb) / det,
            )
        } else {
            (0.0, 0.0, 0.0)
        };
        ar[i] = x;
        ag[i] = y;
        ab[i] = z;
        bb[i] = mp[i] - x * mr[i] - y * mg[i] - z * mb[i];
    }
    let (ar, ag, ab, bb) = (mean(&ar), mean(&ag), mean(&ab), mean(&bb));
    (0..n)
        .map(|i| (ar[i] * ir[i] + ag[i] * ig[i] + ab[i] * ib[i] + bb[i]).clamp(0.0, 1.0))
        .collect()
}

/// The mean of `v` over a (2r + 1)² window round each pixel, the window
/// cut at the picture's edge (running sums, so any `r` costs the same).
pub fn box_mean(v: &[f32], w: usize, h: usize, r: usize) -> Vec<f32> {
    let mut rows = vec![0f32; w * h];
    let mut sums = vec![0f64; w.max(h) + 1];
    for y in 0..h {
        for x in 0..w {
            sums[x + 1] = sums[x] + f64::from(v[y * w + x]);
        }
        for x in 0..w {
            let (a, b) = (x.saturating_sub(r), (x + r + 1).min(w));
            rows[y * w + x] = ((sums[b] - sums[a]) / (b - a) as f64) as f32;
        }
    }
    let mut out = vec![0f32; w * h];
    for x in 0..w {
        for y in 0..h {
            sums[y + 1] = sums[y] + f64::from(rows[y * w + x]);
        }
        for y in 0..h {
            let (a, b) = (y.saturating_sub(r), (y + r + 1).min(h));
            out[y * w + x] = ((sums[b] - sums[a]) / (b - a) as f64) as f32;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn box_mean_averages_its_window() {
        let (w, h) = (5, 4);
        let v: Vec<f32> = (0..w * h).map(|i| i as f32).collect();
        let m = box_mean(&v, w, h, 1);
        // The middle of row 1 (x 2): rows 0-2, columns 1-3.
        let want = [1, 2, 3, 6, 7, 8, 11, 12, 13].iter().sum::<i32>() as f32 / 9.0;
        assert!((m[w + 2] - want).abs() < 1e-4);
        // A corner: its 2 × 2.
        assert!((m[0] - (0.0 + 1.0 + 5.0 + 6.0) / 4.0).abs() < 1e-4);
    }

    #[test]
    fn a_blocky_map_snaps_to_the_photo_edge() {
        // A red disc on grey; the map is the disc drawn in 16-pixel blocks.
        let (w, h) = (192, 192);
        let (cx, cy, rad) = (96.0, 96.0, 52.0);
        let inside = |x: usize, y: usize| (x as f32 + 0.5 - cx).hypot(y as f32 + 0.5 - cy) < rad;
        let mut rgb = vec![0.0f32; w * h * 3];
        let mut p = vec![0.0f32; w * h];
        for y in 0..h {
            for x in 0..w {
                let c = if inside(x, y) {
                    [0.8, 0.2, 0.1]
                } else {
                    [0.45, 0.45, 0.45]
                };
                rgb[(y * w + x) * 3..][..3].copy_from_slice(&c);
                let (bx, by) = (x / 16 * 16 + 8, y / 16 * 16 + 8);
                p[y * w + x] = if inside(bx, by) { 1.0 } else { 0.0 };
            }
        }
        let wrong = |m: &[f32]| {
            (0..w * h)
                .filter(|&i| (m[i] >= 0.5) != inside(i % w, i / w))
                .count()
        };
        let snapped = guided(&p, &rgb, w, h, 8, 1e-3);
        let (before, after) = (wrong(&p), wrong(&snapped));
        assert!(
            after * 4 < before,
            "{before} pixels wrong before, {after} after"
        );
    }
}
