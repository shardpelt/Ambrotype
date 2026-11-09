// SPDX-License-Identifier: GPL-3.0-or-later
//! Finding the frame of a scan inside its rebate or holder.

use super::analysis::downsample;
use super::*;

/// The frame of a scan: the photo without the film rebate around it (bare
/// film base on a negative, black unexposed film on a slide) or the
/// holder's dark or bright edges, as (left, top, right, bottom) fractions
/// of the scan. **Ours:** a pixel is border when it is near the film base,
/// nearly black or clipped; each side moves in while most of the row or
/// column there is border, then a little more for the soft edge. `None`
/// when there's no clear border.
pub fn detect_frame(
    rgb: &[f32],
    w: usize,
    h: usize,
    kind: NegativeType,
    k: &NegativeStrengths,
) -> Option<(f64, f64, f64, f64)> {
    let (blocks, bw, bh) = downsample(rgb, w, h, 250_000);
    if bw < 8 || bh < 8 {
        return None;
    }
    let base = (kind != NegativeType::Positive).then(|| estimate_base(rgb, w, h, kind, k));
    let bright = blocks
        .iter()
        .map(|p| p[0].max(p[1]).max(p[2]))
        .fold(0.0f64, f64::max)
        .max(1e-6);
    let border: Vec<bool> = blocks
        .iter()
        .map(|p| {
            let top = p[0].max(p[1]).max(p[2]);
            let dark = top < 0.02 * bright;
            let clipped = p.iter().all(|v| *v > 0.97);
            let bare = base.is_some_and(|b| (0..3).all(|c| (p[c] / b[c]).log10().abs() < 0.04));
            dark || clipped || bare
        })
        .collect();
    let row = |y: usize, x0: usize, x1: usize| {
        (x0..x1).filter(|&x| border[y * bw + x]).count() as f64 / (x1 - x0) as f64
    };
    let col = |x: usize, y0: usize, y1: usize| {
        (y0..y1).filter(|&y| border[y * bw + x]).count() as f64 / (y1 - y0) as f64
    };
    const MOSTLY: f64 = 0.6;
    let (max_x, max_y) = (bw / 4, bh / 4);
    // Two rounds, so a side's border doesn't count against the others.
    let (mut l, mut t, mut r, mut b) = (0usize, 0usize, bw, bh);
    for _ in 0..2 {
        let mut nt = 0;
        while nt < max_y && row(nt, l, r) > MOSTLY {
            nt += 1;
        }
        let mut nb = bh;
        while bh - nb < max_y && row(nb - 1, l, r) > MOSTLY {
            nb -= 1;
        }
        let mut nl = 0;
        while nl < max_x && col(nl, nt, nb) > MOSTLY {
            nl += 1;
        }
        let mut nr = bw;
        while bw - nr < max_x && col(nr - 1, nt, nb) > MOSTLY {
            nr -= 1;
        }
        (l, t, r, b) = (nl, nt, nr, nb);
    }
    if l == 0 && t == 0 && r == bw && b == bh {
        return None;
    }
    // Into the photo a little past the edge, which is soft on a scan.
    let pad = 0.005;
    let fx = |x: usize| x as f64 / bw as f64;
    let fy = |y: usize| y as f64 / bh as f64;
    let inset = |v: f64, edge: bool, dir: f64| if edge { v + dir * pad } else { v };
    Some((
        inset(fx(l), l > 0, 1.0),
        inset(fy(t), t > 0, 1.0),
        inset(fx(r), r < bw, -1.0),
        inset(fy(b), b < bh, -1.0),
    ))
}
