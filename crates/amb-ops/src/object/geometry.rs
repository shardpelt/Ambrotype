// SPDX-License-Identifier: GPL-3.0-or-later
//! Shapes shared by selecting and removing: overlaps, bounds, shadows, fills.

use super::{SNAP_EPS, SNAP_RADIUS};

/// Intersection over union of two masks of the same size.
pub(super) fn overlap(a: &[bool], b: &[bool]) -> f64 {
    let (mut both, mut either) = (0usize, 0usize);
    for (x, y) in a.iter().zip(b) {
        both += usize::from(*x && *y);
        either += usize::from(*x || *y);
    }
    both as f64 / either.max(1) as f64
}

/// The smallest and largest x and y set in `mask`.
pub(super) fn bounds(mask: &[bool], w: usize) -> ((usize, usize), (usize, usize)) {
    let (mut lo, mut hi) = ((usize::MAX, usize::MAX), (0, 0));
    for (i, _) in mask.iter().enumerate().filter(|(_, m)| **m) {
        let (x, y) = (i % w, i / w);
        lo = (lo.0.min(x), lo.1.min(y));
        hi = (hi.0.max(x), hi.1.max(y));
    }
    (lo, hi)
}

/// The shadow of `object` (`size` pixels across): what round where it
/// stands (its lowest quarter) is clearly darker than the photo a little further out, joined to it, and
/// within 20 % of its size. Nothing when that would be more than half the
/// object (a dark background, not a shadow).
pub(super) fn shadow_of(object: &[bool], rgb: &[f32], w: usize, h: usize, size: f64) -> Vec<bool> {
    let n = w * h;
    let none = Vec::new();
    if size < 8.0 {
        return none;
    }
    let lum: Vec<f32> = (0..n)
        .map(|i| 0.2126 * rgb[i * 3] + 0.7152 * rgb[i * 3 + 1] + 0.0722 * rgb[i * 3 + 2])
        .collect();
    let outside: Vec<bool> = object.iter().map(|m| !m).collect();
    // How far each pixel outside it is from it.
    let dist = distance_inside(&outside, w, h);
    let reach = (0.2 * size) as f32;
    // The photo round it, beyond where a shadow would be: its brightness
    // nearby, as a mean over just those pixels.
    let far: Vec<f32> = (0..n)
        .map(|i| f32::from(u8::from(outside[i] && dist[i] > 0.2 * size as f32)))
        .collect();
    let lit: Vec<f32> = (0..n).map(|i| lum[i] * far[i]).collect();
    let r = ((0.3 * size) as usize).max(4);
    let (sum, count) = (
        crate::snap::box_mean(&lit, w, h, r),
        crate::snap::box_mean(&far, w, h, r),
    );
    // A shadow it casts, lit from above, starts where it stands: near its
    // lowest quarter and below its middle; dark folds and corners beside
    // or above it are not its shadow.
    let (lo, hi) = bounds(object, w);
    let mid = lo.1 + (hi.1 - lo.1) / 2;
    let foot_y = lo.1 + (hi.1 - lo.1) * 3 / 4;
    let foot: Vec<bool> = (0..n).map(|i| object[i] && i / w >= foot_y).collect();
    let not_foot: Vec<bool> = foot.iter().map(|f| !f).collect();
    let from_foot = distance_inside(&not_foot, w, h);
    let place = |i: usize| i / w > mid && from_foot[i] <= reach;
    let dark = |i: usize| {
        if !outside[i] || !place(i) || count[i] < 0.05 {
            return false;
        }
        let around = sum[i] / count[i];
        lum[i] < 0.7 * around && lum[i] < around - 0.02
    };
    // Grown out from the object through dark pixels.
    let mut shadow = vec![false; n];
    let mut queue: std::collections::VecDeque<usize> = (0..n).filter(|&i| foot[i]).collect();
    while let Some(i) = queue.pop_front() {
        let (x, y) = ((i % w) as isize, (i / w) as isize);
        for (dx, dy) in [
            (-1, 0),
            (1, 0),
            (0, -1),
            (0, 1),
            (-1, -1),
            (1, -1),
            (-1, 1),
            (1, 1),
        ] {
            let (nx, ny) = (x + dx, y + dy);
            if nx < 0 || ny < 0 || nx as usize >= w || ny as usize >= h {
                continue;
            }
            let j = ny as usize * w + nx as usize;
            if !shadow[j] && dark(j) {
                shadow[j] = true;
                queue.push_back(j);
            }
        }
    }
    // Its specks and holes evened out.
    let soft: Vec<f32> = shadow.iter().map(|&s| f32::from(u8::from(s))).collect();
    let soft = crate::snap::box_mean(&soft, w, h, 2);
    let shadow: Vec<bool> = (0..n)
        .map(|i| outside[i] && place(i) && soft[i] >= 0.4)
        .collect();
    let (found, own) = (
        shadow.iter().filter(|s| **s).count(),
        object.iter().filter(|s| **s).count(),
    );
    // A few pixels are only the edge's own shading; more than half the
    // object is a dark background.
    if found * 50 < own || found * 2 > own {
        return none;
    }
    shadow
}

/// For each pixel of `mask`, about how far (pixels) it is from the nearest
/// pixel outside it (a two-pass chamfer distance).
pub(super) fn distance_inside(mask: &[bool], w: usize, h: usize) -> Vec<f32> {
    let big = (w + h) as f32;
    let mut d: Vec<f32> = mask.iter().map(|&m| if m { big } else { 0.0 }).collect();
    let (a, b) = (1.0f32, std::f32::consts::SQRT_2);
    let at = |d: &[f32], x: isize, y: isize| {
        if x < 0 || y < 0 || x as usize >= w || y as usize >= h {
            0.0
        } else {
            d[y as usize * w + x as usize]
        }
    };
    for y in 0..h as isize {
        for x in 0..w as isize {
            let i = y as usize * w + x as usize;
            if d[i] == 0.0 {
                continue;
            }
            let v = d[i]
                .min(at(&d, x - 1, y) + a)
                .min(at(&d, x, y - 1) + a)
                .min(at(&d, x - 1, y - 1) + b)
                .min(at(&d, x + 1, y - 1) + b);
            d[i] = v;
        }
    }
    for y in (0..h as isize).rev() {
        for x in (0..w as isize).rev() {
            let i = y as usize * w + x as usize;
            if d[i] == 0.0 {
                continue;
            }
            let v = d[i]
                .min(at(&d, x + 1, y) + a)
                .min(at(&d, x, y + 1) + a)
                .min(at(&d, x + 1, y + 1) + b)
                .min(at(&d, x - 1, y + 1) + b);
            d[i] = v;
        }
    }
    d
}

/// Set `mask` where `value` (at a pixel's centre) is at least a half,
/// within `area` (x0, y0, x1, y1 pixels).
pub(super) fn fill(
    mask: &mut [bool],
    w: usize,
    _h: usize,
    area: (usize, usize, usize, usize),
    value: impl Fn(f64, f64) -> f32,
) {
    let (x0, y0, x1, y1) = area;
    for y in y0..y1 {
        for x in x0..x1 {
            if value(x as f64 + 0.5, y as f64 + 0.5) >= 0.5 {
                mask[y * w + x] = true;
            }
        }
    }
}

/// Like `fill`, with the map's edge snapped to the photo's first: the
/// object selection model's map is coarse, so its outline would come out
/// in steps (`snap`). Only round where the map is, to keep it quick.
pub(super) fn fill_snapped(
    mask: &mut [bool],
    rgb: &[f32],
    w: usize,
    _h: usize,
    area: (usize, usize, usize, usize),
    value: impl Fn(f64, f64) -> f32,
) {
    let (x0, y0, x1, y1) = area;
    let (aw, ah) = (x1 - x0, y1 - y0);
    let soft: Vec<f32> = (0..aw * ah)
        .map(|i| value((x0 + i % aw) as f64 + 0.5, (y0 + i / aw) as f64 + 0.5))
        .collect();
    let (mut lo, mut hi) = ((aw, ah), (0, 0));
    for (i, _) in soft.iter().enumerate().filter(|(_, v)| **v >= 0.02) {
        let (x, y) = (i % aw, i / aw);
        lo = (lo.0.min(x), lo.1.min(y));
        hi = (hi.0.max(x), hi.1.max(y));
    }
    if lo.0 > hi.0 {
        return;
    }
    let grow = 3 * SNAP_RADIUS;
    let (cx0, cy0) = (lo.0.saturating_sub(grow), lo.1.saturating_sub(grow));
    let (cx1, cy1) = ((hi.0 + grow + 1).min(aw), (hi.1 + grow + 1).min(ah));
    let (cw, chh) = (cx1 - cx0, cy1 - cy0);
    let mut p = Vec::with_capacity(cw * chh);
    let mut colours = Vec::with_capacity(cw * chh * 3);
    for y in cy0..cy1 {
        p.extend_from_slice(&soft[y * aw + cx0..y * aw + cx1]);
        let row = (y0 + y) * w + x0;
        colours.extend_from_slice(&rgb[(row + cx0) * 3..(row + cx1) * 3]);
    }
    let q = crate::snap::guided(&p, &colours, cw, chh, SNAP_RADIUS, SNAP_EPS);
    for y in 0..chh {
        for x in 0..cw {
            if q[y * cw + x] >= 0.5 {
                mask[(y0 + cy0 + y) * w + x0 + cx0 + x] = true;
            }
        }
    }
}

/// The model's map at (u, v), fractions of the part it saw (bilinear).
pub(super) fn map_at(m: &amb_engine::subject::Map, u: f64, v: f64) -> f32 {
    let fx = (u * m.width as f64 - 0.5).clamp(0.0, (m.width - 1) as f64);
    let fy = (v * m.height as f64 - 0.5).clamp(0.0, (m.height - 1) as f64);
    let (x, y) = (fx as usize, fy as usize);
    let (x1, y1) = ((x + 1).min(m.width - 1), (y + 1).min(m.height - 1));
    let (tx, ty) = ((fx - x as f64) as f32, (fy - y as f64) as f32);
    let at = |x: usize, y: usize| m.data[y * m.width + x];
    let top = at(x, y) + (at(x1, y) - at(x, y)) * tx;
    let bottom = at(x, y1) + (at(x1, y1) - at(x, y1)) * tx;
    top + (bottom - top) * ty
}

/// Dab centres (pixels) of radius `r` that cover every pixel of `mask`: a
/// grid of step 0.7 r, a dab wherever the mask comes within 0.6 r (more
/// than half a step's diagonal, with rounding), so each masked pixel is
/// under a dab and nothing reaches more than about 1.6 r past the outline.
pub fn cover(mask: &[bool], w: usize, h: usize, r: f64) -> Vec<(f64, f64)> {
    let step = (0.7 * r).max(1.0);
    let reach = (0.6 * r).ceil() as isize + 1;
    let (mut lo_x, mut lo_y, mut hi_x, mut hi_y) = (w, h, 0, 0);
    for y in 0..h {
        for x in 0..w {
            if mask[y * w + x] {
                (lo_x, lo_y, hi_x, hi_y) = (lo_x.min(x), lo_y.min(y), hi_x.max(x), hi_y.max(y));
            }
        }
    }
    let mut dabs = Vec::new();
    if lo_x > hi_x {
        return dabs;
    }
    let near = |gx: f64, gy: f64| {
        let (cx, cy) = (gx.round() as isize, gy.round() as isize);
        for dy in -reach..=reach {
            for dx in -reach..=reach {
                if dx * dx + dy * dy > reach * reach {
                    continue;
                }
                let (x, y) = (cx + dx, cy + dy);
                if x >= 0
                    && y >= 0
                    && (x as usize) < w
                    && (y as usize) < h
                    && mask[y as usize * w + x as usize]
                {
                    return true;
                }
            }
        }
        false
    };
    let mut gy = lo_y as f64;
    while gy <= hi_y as f64 + step {
        let mut gx = lo_x as f64;
        while gx <= hi_x as f64 + step {
            if near(gx, gy) {
                dabs.push((gx, gy));
            }
            gx += step;
        }
        gy += step;
    }
    dabs
}
