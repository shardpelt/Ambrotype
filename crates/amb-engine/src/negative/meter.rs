// SPDX-License-Identifier: GPL-3.0-or-later
//! The part of the crop Auto measures, drawn by hand (`amb:NegativeMeter*`).

use super::*;

/// The least a metering box may span, in each direction, as a fraction.
const MIN_SPAN: f64 = 0.02;

/// The pixels of `scan` inside `meter` (left, top, right, bottom as
/// fractions), with their size. `None` when the box is the whole frame or
/// not valid (empty, inside out, under 2 % either way): then the whole scan
/// is measured, as without a box.
pub fn sub_scan(scan: Scan, meter: [f64; 4]) -> Option<(Vec<f32>, usize, usize)> {
    let [l, t, r, b] = meter.map(|v| {
        if v.is_finite() {
            v.clamp(0.0, 1.0)
        } else {
            f64::NAN
        }
    });
    if l.is_nan() || t.is_nan() || r.is_nan() || b.is_nan() {
        return None;
    }
    if (l, t, r, b) == (0.0, 0.0, 1.0, 1.0) || r - l < MIN_SPAN || b - t < MIN_SPAN {
        return None;
    }
    let (w, h) = (scan.width as f64, scan.height as f64);
    let x0 = ((l * w).round() as usize).min(scan.width.saturating_sub(1));
    let y0 = ((t * h).round() as usize).min(scan.height.saturating_sub(1));
    let x1 = ((r * w).round() as usize).clamp(x0 + 1, scan.width);
    let y1 = ((b * h).round() as usize).clamp(y0 + 1, scan.height);
    let mut rgb = Vec::with_capacity((x1 - x0) * (y1 - y0) * 3);
    for y in y0..y1 {
        rgb.extend_from_slice(&scan.rgb[(y * scan.width + x0) * 3..(y * scan.width + x1) * 3]);
    }
    Some((rgb, x1 - x0, y1 - y0))
}
