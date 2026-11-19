// SPDX-License-Identifier: GPL-3.0-or-later
//! AI fill: a spot filled by the inpainting model, with grain to match.

use super::{Placed, sample};
use crate::buffer::{Image, luminance_weights};

/// The side of the square an [`Inpainter`] takes and gives back.
pub const INPAINT_SIZE: usize = 512;

/// Fills a hole: `rgb` (sRGB-encoded 0..1, [`INPAINT_SIZE`]² pixels,
/// interleaved) where `mask` (one value a pixel, 1 = fill) is set; returns
/// the same layout, or why not.
pub type Inpainter = dyn Fn(&[f32], &[f32]) -> Result<Vec<f32>, String> + Send + Sync;

static INPAINTER: std::sync::RwLock<Option<std::sync::Arc<Inpainter>>> =
    std::sync::RwLock::new(None);

/// Register the inpainter content-aware spots are filled with (`amb-ai`
/// registers the AI Remove model when it is installed).
pub fn set_inpainter(f: std::sync::Arc<Inpainter>) {
    *INPAINTER.write().unwrap_or_else(|e| e.into_inner()) = Some(f);
}

pub(super) fn inpainter() -> Option<std::sync::Arc<Inpainter>> {
    INPAINTER.read().unwrap_or_else(|e| e.into_inner()).clone()
}

/// The context around a spot the inpainter sees: a square this many spot
/// radii from its centre to each side (inside what [`footprints`] reads).
pub(super) const FILL_CONTEXT: f64 = 3.0;

/// At least this much context each side of the centre, in the render's
/// pixels: the model sees the photo at its own scale (a small crop blown up
/// to the model's size looks nothing like a photo, and it fills it with
/// odd texture).
pub(crate) const FILL_MIN_HALF: f64 = INPAINT_SIZE as f64 / 2.0;

/// Fill spot `p` with `fill`. **Ours.** A square around the spot (three of
/// its radii each way, at least [`FILL_MIN_HALF`]) goes to the model at
/// [`INPAINT_SIZE`] (averaged down, never blown up when the photo is larger), with its
/// brightness scaled so its bright end sits at 1 and sRGB-encoded; the
/// fill comes back the same way, is laid in through the spot's feathered
/// alpha, and gets the grain of the ring around the spot (its fine detail's
/// spread per channel, as seeded noise), so it doesn't look smoother than
/// the photo.
pub(super) fn fill_one(img: &mut Image, p: &Placed, fill: &Inpainter) -> Result<(), String> {
    use crate::pipeline::{srgb_decode, srgb_encode};
    let n = INPAINT_SIZE;
    // A square box around the spot, slid inward at the photo's edges so it
    // stays square (a box cut short would be stretched for the model).
    let half = (p.radius * FILL_CONTEXT).max(FILL_MIN_HALF);
    let side = ((2.0 * half).ceil() as usize)
        .min(img.width)
        .min(img.height);
    if side < 8 {
        return Err("no room around the spot".into());
    }
    let start =
        |c: f64, len: usize| ((c - side as f64 / 2.0).round().max(0.0) as usize).min(len - side);
    let (bx0, by0) = (start(p.centre.0, img.width), start(p.centre.1, img.height));
    let (bx1, by1) = (bx0 + side, by0 + side);
    let side = side as f64;
    // Model pixel (u, v) <-> photo pixel.
    let s = n as f64 / side;
    let to_photo = |u: usize, v: usize| {
        (
            bx0 as f64 + (u as f64 + 0.5) / s - 0.5,
            by0 as f64 + (v as f64 + 0.5) / s - 0.5,
        )
    };
    // Each model pixel from the photo pixels it covers: averaged when the
    // box is larger than the model (no aliasing), else interpolated.
    let step = 1.0 / s;
    let read = |x: f64, y: f64| -> [f32; 3] {
        if step <= 1.0 {
            return sample(img, x, y);
        }
        let reach = step / 2.0;
        let (xa, xb) = (
            ((x - reach).ceil().max(bx0 as f64)) as usize,
            ((x + reach).floor() as usize).min(bx1 - 1),
        );
        let (ya, yb) = (
            ((y - reach).ceil().max(by0 as f64)) as usize,
            ((y + reach).floor() as usize).min(by1 - 1),
        );
        let mut sum = [0.0f32; 3];
        let mut k = 0.0f32;
        for yy in ya..=yb.max(ya) {
            for xx in xa..=xb.max(xa) {
                let c = img.get(xx, yy);
                for ch in 0..3 {
                    sum[ch] += c[ch];
                }
                k += 1.0;
            }
        }
        sum.map(|v| v / k.max(1.0))
    };
    let alpha_at = |x: f64, y: f64| -> f32 {
        let (i, j) = (
            x.round() as isize - p.x0 as isize,
            y.round() as isize - p.y0 as isize,
        );
        if i < 0 || j < 0 || i >= p.w as isize || j >= p.h as isize {
            0.0
        } else {
            p.alpha[j as usize * p.w + i as usize]
        }
    };
    // Brightness scale: the box's bright end (99.5th percentile) at 1.
    let lw = luminance_weights();
    let mut lum: Vec<f32> = (by0..by1)
        .flat_map(|y| (bx0..bx1).map(move |x| (x, y)))
        .map(|(x, y)| {
            let c = img.get(x, y);
            lw[0] * c[0] + lw[1] * c[1] + lw[2] * c[2]
        })
        .collect();
    lum.sort_by(f32::total_cmp);
    let scale = lum[(lum.len() - 1) * 995 / 1000].max(1e-4);
    let mut rgb = vec![0.0f32; n * n * 3];
    let mut mask = vec![0.0f32; n * n];
    let clamp = |x: f64, lo: usize, hi: usize| x.clamp(lo as f64, (hi - 1) as f64);
    for v in 0..n {
        for u in 0..n {
            let (x, y) = to_photo(u, v);
            let (x, y) = (clamp(x, bx0, bx1), clamp(y, by0, by1));
            let hole = alpha_at(x, y) > 0.01;
            let c = read(x, y);
            let k = v * n + u;
            mask[k] = hole as u8 as f32;
            for ch in 0..3 {
                rgb[k * 3 + ch] = if hole {
                    0.0
                } else {
                    srgb_encode((c[ch] / scale).clamp(0.0, 1.0))
                };
            }
        }
    }
    let out = fill(&rgb, &mask)?;
    if out.len() != n * n * 3 {
        return Err("the inpainter gave the wrong size".into());
    }
    // The ring's grain: spread of each channel's fine detail (pixel minus
    // its 3×3 mean) where the spot isn't.
    let grain = ring_grain(img, p, (bx0, by0, bx1, by1));
    let seed = p.centre.0.to_bits() ^ p.centre.1.to_bits().rotate_left(17);
    for j in 0..p.h {
        for i in 0..p.w {
            let a = p.alpha[j * p.w + i];
            if a <= 0.0 {
                continue;
            }
            let (x, y) = (p.x0 + i, p.y0 + j);
            let (u, v) = (
                (x as f64 - bx0 as f64 + 0.5) * s - 0.5,
                (y as f64 - by0 as f64 + 0.5) * s - 0.5,
            );
            let f = bilinear(&out, n, u, v);
            let t = img.get(x, y);
            let filled: [f32; 3] = std::array::from_fn(|c| {
                let noise = gaussian(seed, (x as u64) << 32 | y as u64, c as u64);
                (srgb_decode(f[c].clamp(0.0, 1.0)) * scale + grain[c] * noise).max(0.0)
            });
            img.set(x, y, [0, 1, 2].map(|c| t[c] + (filled[c] - t[c]) * a));
        }
    }
    Ok(())
}

/// Bilinear sample of an interleaved RGB `n × n` image at (u, v).
fn bilinear(rgb: &[f32], n: usize, u: f64, v: f64) -> [f32; 3] {
    let u = u.clamp(0.0, (n - 1) as f64);
    let v = v.clamp(0.0, (n - 1) as f64);
    let (x0, y0) = (u.floor() as usize, v.floor() as usize);
    let (x1, y1) = ((x0 + 1).min(n - 1), (y0 + 1).min(n - 1));
    let (fx, fy) = ((u - x0 as f64) as f32, (v - y0 as f64) as f32);
    let at = |x: usize, y: usize, c: usize| rgb[(y * n + x) * 3 + c];
    std::array::from_fn(|c| {
        let top = at(x0, y0, c) * (1.0 - fx) + at(x1, y0, c) * fx;
        let bottom = at(x0, y1, c) * (1.0 - fx) + at(x1, y1, c) * fx;
        top * (1.0 - fy) + bottom * fy
    })
}

/// Per channel, the spread of fine detail around the spot (outside it,
/// inside the box): the median of its size, as a standard deviation
/// (× 1.4826), so edges and texture in the ring (another object, a clone
/// next to it) don't count as grain, as they would in a plain spread.
fn ring_grain(img: &Image, p: &Placed, (x0, y0, x1, y1): (usize, usize, usize, usize)) -> [f32; 3] {
    let mut detail: [Vec<f32>; 3] = std::array::from_fn(|_| Vec::new());
    // Every second pixel each way is plenty, and four times quicker.
    for y in ((y0 + 1)..(y1 - 1)).step_by(2) {
        for x in ((x0 + 1)..(x1 - 1)).step_by(2) {
            let inside = x >= p.x0
                && y >= p.y0
                && x < p.x0 + p.w
                && y < p.y0 + p.h
                && p.alpha[(y - p.y0) * p.w + (x - p.x0)] > 0.0;
            if inside {
                continue;
            }
            let mut mean = [0.0f32; 3];
            for dy in 0..3 {
                for dx in 0..3 {
                    let c = img.get(x + dx - 1, y + dy - 1);
                    (0..3).for_each(|k| mean[k] += c[k] / 9.0);
                }
            }
            let c = img.get(x, y);
            (0..3).for_each(|k| detail[k].push((c[k] - mean[k]).abs()));
        }
    }
    // A 3×3 mean removes about a ninth of white noise's spread too.
    detail.map(|mut v| {
        if v.is_empty() {
            return 0.0;
        }
        let mid = v.len() / 2;
        let (_, m, _) = v.select_nth_unstable_by(mid, f32::total_cmp);
        *m * 1.4826 * (9.0f32 / 8.0).sqrt()
    })
}

/// A standard normal number from a seed and a position (deterministic, so
/// every render of the spot looks the same).
fn gaussian(seed: u64, at: u64, channel: u64) -> f32 {
    let mix = |mut z: u64| {
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    };
    let a = mix(seed ^ mix(at ^ channel.wrapping_mul(0x9e37_79b9_7f4a_7c15)));
    let b = mix(a);
    let u1 = ((a >> 11) as f64 + 0.5) / (1u64 << 53) as f64;
    let u2 = (b >> 11) as f64 / (1u64 << 53) as f64;
    ((-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos()) as f32
}
