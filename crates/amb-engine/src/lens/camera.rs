// SPDX-License-Identifier: GPL-3.0-or-later
//! The camera-space corrections: the resampling pass and its interpolation.

use super::*;

/// Apply the camera-space part of the lens corrections. `crop_factor` is the
/// camera's, when known.
pub fn apply_camera(
    rgb: &[f32],
    width: usize,
    height: usize,
    crop_factor: Option<f64>,
    lc: &LensCorrections,
    k: &LensStrengths,
) -> CameraPass {
    apply_camera_cropped(rgb, width, height, crop_factor, lc, k, None, None)
}

/// [`apply_camera`], then Transform, crop and straighten in the same
/// resampling pass. `warp` maps the transformed frame into the lens-corrected
/// one ([`crate::transform::Transform::sampling_map`]); the output is
/// `crop`'s size, and `crop` maps it into the transformed frame. Parts
/// outside the source are blank. With Constrain Crop, the transformed frame
/// is zoomed until no blank areas remain.
#[allow(clippy::too_many_arguments)]
pub fn apply_camera_cropped(
    rgb: &[f32],
    width: usize,
    height: usize,
    crop_factor: Option<f64>,
    lc: &LensCorrections,
    k: &LensStrengths,
    crop: Option<&CropGeometry>,
    warp: Option<&Homography>,
) -> CameraPass {
    apply_camera_window(rgb, width, height, crop_factor, lc, k, crop, warp, None)
}

/// [`apply_camera_cropped`] for `window` of the output only (all of it when
/// `None`): the pass returns that rectangle, sampled exactly as the full
/// output would be there.
#[allow(clippy::too_many_arguments)]
pub fn apply_camera_window(
    rgb: &[f32],
    width: usize,
    height: usize,
    crop_factor: Option<f64>,
    lc: &LensCorrections,
    k: &LensStrengths,
    crop: Option<&CropGeometry>,
    warp: Option<&Homography>,
    window: Option<Rect>,
) -> CameraPass {
    let frame = ImageFrame {
        width,
        height,
        crop_factor,
    };
    let profile = lc.active_profile().map(|c| Geometry::new(c, &frame));
    let (full_w, full_h) = crop.map_or((width, height), |c| (c.width, c.height));
    let win = window.unwrap_or(Rect {
        x: 0,
        y: 0,
        width: full_w,
        height: full_h,
    });
    let mut src = Cow::Borrowed(rgb);

    // 1. Vignetting gains on the source pixels.
    let profile_vignette = profile
        .as_ref()
        .filter(|g| g.has_vignetting() && lc.vignetting_scale > 0.0);
    if profile_vignette.is_some() || lc.vignette_amount != 0.0 {
        let (cx, cy, half_diag) = centre(width, height);
        let vs = lc.vignetting_scale / 100.0;
        let start = k.manual_vignette_start_min
            + (k.manual_vignette_start_max - k.manual_vignette_start_min)
                * (lc.vignette_midpoint / 100.0).clamp(0.0, 1.0);
        let manual_stops = k.manual_vignette_stops * lc.vignette_amount / 100.0;
        src.to_mut()
            .par_chunks_mut(width * 3)
            .enumerate()
            .for_each(|(y, row)| {
                for x in 0..width {
                    let (fx, fy) = (x as f64, y as f64);
                    let mut gain = 1.0;
                    if let Some(g) = profile_vignette {
                        gain *= g.vignette_gain(fx, fy).powf(vs);
                    }
                    if manual_stops != 0.0 {
                        let r = (fx - cx).hypot(fy - cy) / half_diag;
                        gain *= (manual_stops * smoothstep(start, 1.0, r)).exp2();
                    }
                    let gain = gain as f32;
                    for v in &mut row[x * 3..x * 3 + 3] {
                        *v *= gain;
                    }
                }
            });
    }

    // 2. Lateral chromatic aberration model.
    let auto_ca = if lc.auto_ca {
        estimate_ca(&src, width, height, k.ca_max_shift)
    } else {
        None
    };
    let profile_tca = profile.as_ref().filter(|g| {
        let wanted = lc.auto_ca || k.profile_tca_always || lc.embedded.is_some();
        g.has_tca() && wanted && auto_ca.is_none()
    });

    // 3. Geometry.
    let profile_distortion = profile
        .as_ref()
        .filter(|g| g.has_distortion() && lc.distortion_scale != 0.0);
    let manual_k = -k.manual_distortion * lc.manual_distortion / 100.0;
    if profile_distortion.is_none()
        && manual_k == 0.0
        && auto_ca.is_none()
        && profile_tca.is_none()
        && crop.is_none()
        && warp.is_none()
    {
        let rgb = if win.width == width && win.height == height {
            src.into_owned()
        } else {
            let mut out = Vec::with_capacity(win.width * win.height * 3);
            for y in win.y..win.bottom() {
                out.extend_from_slice(&src[(y * width + win.x) * 3..(y * width + win.right()) * 3]);
            }
            out
        };
        return CameraPass {
            width: win.width,
            height: win.height,
            rgb,
            blank: None,
        };
    }
    let (cx, cy, half_diag) = centre(width, height);
    let ds = lc.distortion_scale / 100.0;
    let cs = lc.ca_scale / 100.0;
    // Output pixel → green source position, before zoom.
    let manual = move |x: f64, y: f64| -> (f64, f64) {
        if manual_k == 0.0 {
            return (x, y);
        }
        let (dx, dy) = (x - cx, y - cy);
        let r2 = (dx * dx + dy * dy) / (half_diag * half_diag);
        let m = (1.0 + manual_k * r2) / (1.0 + manual_k);
        (cx + dx * m, cy + dy * m)
    };
    let prof = |x: f64, y: f64| -> (f64, f64) {
        match profile_distortion {
            Some(g) => {
                let (px, py) = g.distort(x, y);
                (x + ds * (px - x), y + ds * (py - y))
            }
            None => (x, y),
        }
    };
    let zoomed = |z: f64, x: f64, y: f64| (cx + (x - cx) / z, cy + (y - cy) / z);
    let zoom = if lc.constrain_crop && (manual_k != 0.0 || profile_distortion.is_some()) {
        fill_zoom(width, height, |x, y| {
            let (mx, my) = manual(x, y);
            prof(mx, my)
        })
    } else if profile_distortion.is_some() {
        fill_zoom(width, height, prof)
    } else {
        1.0
    };
    let lens_map = |x: f64, y: f64| {
        let (zx, zy) = zoomed(zoom, x, y);
        let (mx, my) = manual(zx, zy);
        prof(mx, my)
    };
    // Transform, before the lens maps; Constrain Crop zooms the transformed
    // frame to fill.
    let warp_zoom = match warp {
        Some(h) if lc.constrain_crop => fill_zoom(width, height, |x, y| {
            let (tx, ty) = h.apply(x, y);
            lens_map(tx, ty)
        }),
        _ => 1.0,
    };
    let map_green = |x: f64, y: f64| match warp {
        Some(h) => {
            let (zx, zy) = zoomed(warp_zoom, x, y);
            let (tx, ty) = h.apply(zx, zy);
            lens_map(tx, ty)
        }
        None => lens_map(x, y),
    };
    let channel_positions = |gx: f64, gy: f64| -> [(f64, f64); 3] {
        if let Some([red, blue]) = &auto_ca {
            let (dx, dy) = (gx - cx, gy - cy);
            let rp = dx.hypot(dy);
            if rp < 1e-9 {
                return [(gx, gy); 3];
            }
            let r = rp / half_diag;
            let shift = |s: &RadialShift| {
                let f = 1.0 + s.at(r) / rp;
                (cx + dx * f, cy + dy * f)
            };
            [shift(red), (gx, gy), shift(blue)]
        } else if let Some(g) = profile_tca {
            let [r, b] = g.tca(gx, gy);
            let blend = |p: (f64, f64)| (gx + cs * (p.0 - gx), gy + cs * (p.1 - gy));
            [blend(r), (gx, gy), blend(b)]
        } else {
            [(gx, gy); 3]
        }
    };

    let (wf, hf) = (width as f64, height as f64);
    let inside = |x: f64, y: f64| x >= -0.5 && x <= wf - 0.5 && y >= -0.5 && y <= hf - 0.5;
    let (out_w, out_h) = (win.width, win.height);
    let mut out = vec![0.0f32; out_w * out_h * 3];
    let mut blank = vec![false; out_w * out_h];
    out.par_chunks_mut(out_w * 3)
        .zip(blank.par_chunks_mut(out_w))
        .enumerate()
        .for_each(|(y, (row, blank_row))| {
            for x in 0..out_w {
                // Output pixel → lens-corrected frame (crop) → source (lens).
                let (ox, oy) = ((win.x + x) as f64, (win.y + y) as f64);
                let (fx, fy) = crop.map_or((ox, oy), |c| c.source(ox, oy));
                let (gx, gy) = map_green(fx, fy);
                if !inside(fx, fy) || !inside(gx, gy) {
                    blank_row[x] = true;
                }
                if !(gx.is_finite() && gy.is_finite()) {
                    // Beyond the transformed photo's horizon.
                    continue;
                }
                let pos = channel_positions(gx, gy);
                if pos[0] == pos[1] && pos[2] == pos[1] {
                    let v = sample_bicubic_rgb(&src, width, height, gx, gy);
                    row[x * 3..x * 3 + 3].copy_from_slice(&v);
                } else {
                    for (c, (sx, sy)) in pos.iter().enumerate() {
                        row[x * 3 + c] = sample_bicubic(&src, width, height, c, *sx, *sy);
                    }
                }
            }
        });
    let any_blank = blank.iter().any(|b| *b);
    CameraPass {
        width: out_w,
        height: out_h,
        rgb: out,
        blank: any_blank.then_some(blank),
    }
}

/// Smallest zoom (≥ 0.5, ≤ 4) at which every border pixel of the output maps
/// inside the source image, for a mapping `f` from output to source pixels
/// (before zoom). Zooms out when the correction leaves room to spare.
fn fill_zoom(width: usize, height: usize, f: impl Fn(f64, f64) -> (f64, f64)) -> f64 {
    let (w, h) = (width as f64, height as f64);
    let (cx, cy) = ((w - 1.0) / 2.0, (h - 1.0) / 2.0);
    let n = 96;
    let border: Vec<(f64, f64)> = (0..=n)
        .flat_map(|i| {
            let t = i as f64 / n as f64;
            [
                (t * (w - 1.0), 0.0),
                (t * (w - 1.0), h - 1.0),
                (0.0, t * (h - 1.0)),
                (w - 1.0, t * (h - 1.0)),
            ]
        })
        .collect();
    let valid = |z: f64| {
        border.iter().all(|&(x, y)| {
            let (sx, sy) = f(cx + (x - cx) / z, cy + (y - cy) / z);
            sx >= -0.5 && sx <= w - 0.5 && sy >= -0.5 && sy <= h - 0.5
        })
    };
    let (mut lo, mut hi) = (0.5, 4.0);
    if valid(lo) {
        return lo;
    }
    if !valid(hi) {
        return hi;
    }
    for _ in 0..40 {
        let mid = 0.5 * (lo + hi);
        if valid(mid) {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    hi
}

fn catmull_rom(t: f32) -> [f32; 4] {
    let t2 = t * t;
    let t3 = t2 * t;
    [
        0.5 * (-t3 + 2.0 * t2 - t),
        0.5 * (3.0 * t3 - 5.0 * t2 + 2.0),
        0.5 * (-3.0 * t3 + 4.0 * t2 + t),
        0.5 * (t3 - t2),
    ]
}

/// Catmull-Rom sample of channel `c` at (x, y), clamping at the edges.
fn sample_bicubic(rgb: &[f32], w: usize, h: usize, c: usize, x: f64, y: f64) -> f32 {
    let x = x.clamp(0.0, (w - 1) as f64);
    let y = y.clamp(0.0, (h - 1) as f64);
    let (x0, y0) = (x.floor() as isize, y.floor() as isize);
    let (fx, fy) = ((x - x0 as f64) as f32, (y - y0 as f64) as f32);
    let (wx, wy) = (catmull_rom(fx), catmull_rom(fy));
    let mut acc = 0.0f32;
    for (j, wyj) in wy.iter().enumerate() {
        let yy = (y0 + j as isize - 1).clamp(0, h as isize - 1) as usize;
        let row = &rgb[yy * w * 3..(yy + 1) * w * 3];
        let mut racc = 0.0f32;
        for (i, wxi) in wx.iter().enumerate() {
            let xx = (x0 + i as isize - 1).clamp(0, w as isize - 1) as usize;
            racc += wxi * row[xx * 3 + c];
        }
        acc += wyj * racc;
    }
    acc
}

/// Catmull-Rom sample of all three channels at (x, y).
pub(crate) fn sample_bicubic_rgb(rgb: &[f32], w: usize, h: usize, x: f64, y: f64) -> [f32; 3] {
    let x = x.clamp(0.0, (w - 1) as f64);
    let y = y.clamp(0.0, (h - 1) as f64);
    let (x0, y0) = (x.floor() as isize, y.floor() as isize);
    let (wx, wy) = (
        catmull_rom((x - x0 as f64) as f32),
        catmull_rom((y - y0 as f64) as f32),
    );
    let xs: [usize; 4] =
        std::array::from_fn(|i| (x0 + i as isize - 1).clamp(0, w as isize - 1) as usize);
    let mut acc = [0.0f32; 3];
    for (j, wyj) in wy.iter().enumerate() {
        let yy = (y0 + j as isize - 1).clamp(0, h as isize - 1) as usize;
        let row = &rgb[yy * w * 3..(yy + 1) * w * 3];
        let mut racc = [0.0f32; 3];
        for (xx, wxi) in xs.iter().zip(wx) {
            let p = &row[xx * 3..xx * 3 + 3];
            racc[0] += wxi * p[0];
            racc[1] += wxi * p[1];
            racc[2] += wxi * p[2];
        }
        for c in 0..3 {
            acc[c] += wyj * racc[c];
        }
    }
    acc
}

/// Bilinear sample of channel `c` (clamped).
pub(super) fn sample_bilinear(rgb: &[f32], w: usize, h: usize, c: usize, x: f64, y: f64) -> f64 {
    let x = x.clamp(0.0, (w - 1) as f64);
    let y = y.clamp(0.0, (h - 1) as f64);
    let (x0, y0) = (x.floor() as usize, y.floor() as usize);
    let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
    let (fx, fy) = (x - x0 as f64, y - y0 as f64);
    let at = |xx: usize, yy: usize| f64::from(rgb[(yy * w + xx) * 3 + c]);
    let top = at(x0, y0) * (1.0 - fx) + at(x1, y0) * fx;
    let bottom = at(x0, y1) * (1.0 - fx) + at(x1, y1) * fx;
    top * (1.0 - fy) + bottom * fy
}

/// Paint the blank areas left by the geometric corrections.
pub fn fill_blank(img: &mut Image, blank: &[bool], k: &LensStrengths) {
    let v = k.blank_fill as f32;
    img.rgb
        .par_chunks_mut(3)
        .zip(blank.par_iter())
        .for_each(|(p, b)| {
            if *b {
                p.copy_from_slice(&[v, v, v]);
            }
        });
}
