// SPDX-License-Identifier: GPL-3.0-or-later
//! Drawing a mask: the frame, shapes, colour sampling and ranges, and corrections.

use super::*;

/// Output pixel ↔ uncropped frame pixel (pixel centres at integers).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Frame {
    pub(crate) width: f64,
    pub(crate) height: f64,
    /// frame = o + x·ex + y·ey
    o: (f64, f64),
    ex: (f64, f64),
    ey: (f64, f64),
}

impl Frame {
    pub(crate) fn new(frame: (usize, usize), crop: Option<&CropGeometry>) -> Frame {
        let (o, ex, ey) = match crop {
            Some(g) => {
                let o = g.source(0.0, 0.0);
                let a = g.source(1.0, 0.0);
                let b = g.source(0.0, 1.0);
                (o, (a.0 - o.0, a.1 - o.1), (b.0 - o.0, b.1 - o.1))
            }
            None => ((0.0, 0.0), (1.0, 0.0), (0.0, 1.0)),
        };
        Frame {
            width: frame.0 as f64,
            height: frame.1 as f64,
            o,
            ex,
            ey,
        }
    }

    /// The same map for a buffer holding `tile` of the output: its pixel
    /// (0, 0) is output pixel (`tile.x`, `tile.y`).
    pub(crate) fn in_tile(self, tile: &Tile) -> Frame {
        let (tx, ty) = (tile.x as f64, tile.y as f64);
        Frame {
            o: (
                self.o.0 + tx * self.ex.0 + ty * self.ey.0,
                self.o.1 + tx * self.ex.1 + ty * self.ey.1,
            ),
            ..self
        }
    }

    /// Frame pixel coordinates (centres at integers) of output pixel (x, y).
    #[inline]
    pub(crate) fn source(&self, x: f64, y: f64) -> (f64, f64) {
        (
            self.o.0 + x * self.ex.0 + y * self.ey.0,
            self.o.1 + x * self.ex.1 + y * self.ey.1,
        )
    }

    /// Output pixel of a frame position.
    pub(crate) fn target(&self, fx: f64, fy: f64) -> (f64, f64) {
        let det = self.ex.0 * self.ey.1 - self.ey.0 * self.ex.1;
        let (dx, dy) = (fx - self.o.0, fy - self.o.1);
        (
            (dx * self.ey.1 - dy * self.ey.0) / det,
            (self.ex.0 * dy - self.ex.1 * dx) / det,
        )
    }

    /// Image fraction → frame pixel coordinates.
    pub(crate) fn pixel(&self, u: f64, v: f64) -> (f64, f64) {
        (u * self.width - 0.5, v * self.height - 0.5)
    }

    /// Output pixels per frame pixel (crops never scale much).
    pub(crate) fn scale(&self) -> f64 {
        1.0 / self.ex.0.hypot(self.ex.1).max(1e-9)
    }
}

/// What a correction's mask needs from the image.
pub(super) struct Reference<'a> {
    /// Perceptual luminance 0..1 per output pixel.
    pub(super) luminance: Option<&'a [f32]>,
    /// OkLab per output pixel.
    pub(super) lab: Option<&'a [[f32; 3]]>,
    /// Tone-encoded sRGB per output pixel (for the subject model).
    pub(super) srgb: Option<&'a [f32]>,
    pub(super) width: usize,
    pub(super) height: usize,
    pub(super) strengths: MaskStrengths,
    /// The buffer's place in the full output.
    pub(super) tile: Tile,
}

/// The colours colour-range masks select, one list per colour shape in
/// drawing order: measured on the image, or given (a region render takes
/// them from the whole photo, since its samples may lie outside the region).
#[derive(Default)]
pub(super) struct Targets<'a> {
    pub(super) given: Option<&'a [Vec<[f32; 3]>]>,
    pub(super) measured: Vec<Vec<[f32; 3]>>,
    /// Sky maps, as colour targets: given to a region render, measured on
    /// a whole one.
    pub(super) sky_given: Option<&'a [SkyMap]>,
    pub(super) sky_measured: Vec<SkyMap>,
}

/// Draw one shape into a plane of the output's size.
pub(super) fn draw_shape(
    shape: &Shape,
    frame: &Frame,
    r: &Reference,
    targets: &mut Targets,
) -> Vec<f32> {
    let (w, h) = (r.width, r.height);
    let mut plane = vec![0.0f32; w * h];
    let per_pixel = |plane: &mut Vec<f32>, f: &(dyn Fn(f64, f64) -> f64 + Sync)| {
        plane.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
            for (x, v) in row.iter_mut().enumerate() {
                let (fx, fy) = frame.source(x as f64, y as f64);
                *v = f(fx, fy) as f32;
            }
        });
    };
    match shape {
        Shape::Linear { zero, full } => {
            let z = frame.pixel(zero.0, zero.1);
            let f = frame.pixel(full.0, full.1);
            let d = (f.0 - z.0, f.1 - z.1);
            let len2 = d.0 * d.0 + d.1 * d.1;
            if len2 < 1e-12 {
                return plane;
            }
            per_pixel(&mut plane, &|fx, fy| {
                let t = ((fx - z.0) * d.0 + (fy - z.1) * d.1) / len2;
                smoothstep(0.0, 1.0, t)
            });
        }
        Shape::Radial {
            top,
            left,
            bottom,
            right,
            angle,
            feather,
        } => {
            let c = frame.pixel((left + right) / 2.0, (top + bottom) / 2.0);
            let a = ((right - left).abs() / 2.0 * frame.width).max(1e-6);
            let b = ((bottom - top).abs() / 2.0 * frame.height).max(1e-6);
            // Positive angles turn the ellipse counter-clockwise on screen (y down).
            let t = angle.to_radians();
            let (cos, sin) = (t.cos(), t.sin());
            let inner = 1.0 - feather / 100.0;
            per_pixel(&mut plane, &|fx, fy| {
                let (dx, dy) = (fx - c.0, fy - c.1);
                let u = dx * cos - dy * sin;
                let v = dx * sin + dy * cos;
                let d = ((u / a).powi(2) + (v / b).powi(2)).sqrt();
                1.0 - smoothstep(inner, 1.0, d)
            });
        }
        Shape::Brush(strokes) => {
            // Dabs are circles in pixels, sized in frame widths.
            let centre = |u: f64, v: f64| {
                let (fx, fy) = frame.pixel(u, v);
                frame.target(fx, fy)
            };
            return brush::draw(w, h, strokes, centre, frame.width * frame.scale());
        }
        Shape::Luminance(range) => {
            if let Some(l) = r.luminance {
                let [a, b, c, d] = *range;
                plane.par_iter_mut().zip(l.par_iter()).for_each(|(v, &y)| {
                    let y = y as f64;
                    // Equal edges are hard: in from `a`, out after `d`.
                    let rise = if b <= a {
                        f64::from(u8::from(y >= a))
                    } else {
                        smoothstep(a, b, y)
                    };
                    let fall = if d <= c {
                        f64::from(u8::from(y <= d))
                    } else {
                        1.0 - smoothstep(c, d, y)
                    };
                    *v = (rise * fall) as f32;
                });
            }
        }
        Shape::Color { samples, amount } => {
            if let Some(lab) = r.lab {
                let i = targets.measured.len();
                let colours = match targets.given.and_then(|g| g.get(i)) {
                    Some(t) => t.clone(),
                    None => sample_colours(lab, w, h, frame, samples),
                };
                draw_color_range(&mut plane, lab, &colours, *amount, &r.strengths);
                targets.measured.push(colours);
            }
        }
        Shape::Sky => {
            if let Some(lab) = r.lab {
                let i = targets.sky_measured.len();
                let map = match targets.sky_given.and_then(|g| g.get(i)) {
                    Some(m) => m.clone(),
                    None => sky_map(lab, w, h),
                };
                let t = &r.tile;
                let (fw, fh) = (t.full_width.max(1) as f64, t.full_height.max(1) as f64);
                plane.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
                    let v = (t.y + y) as f64 / fh;
                    for (x, out) in row.iter_mut().enumerate() {
                        let u = (t.x + x) as f64 / fw;
                        // The grid says where; each pixel's colour keeps
                        // branches and edges against the sky sharp.
                        *out = map.at(u, v) * sky_colour(lab[y * w + x]);
                    }
                });
                targets.sky_measured.push(map);
            }
        }
        Shape::Model(kind) => {
            {
                let i = targets.sky_measured.len();
                // Maps are listed in drawing order, sky and subject alike, so
                // a tile of the 1:1 view uses the whole photo's.
                let map = match (targets.sky_given.and_then(|g| g.get(i)), r.srgb) {
                    (Some(m), _) => Some(m.clone()),
                    // Only the whole photo shows what the subject is.
                    (None, Some(_)) if !r.tile.is_whole(w, h) => None,
                    (None, None) => None,
                    (None, Some(rgb)) => match crate::subject::find(*kind, rgb, w, h) {
                        Ok(m) => Some(SkyMap {
                            width: m.width,
                            height: m.height,
                            data: m.data,
                        }),
                        Err(e) => {
                            eprintln!("{} mask: {e}", kind.task());
                            None
                        }
                    },
                };
                if let Some(map) = map {
                    let t = &r.tile;
                    let (fw, fh) = (t.full_width.max(1) as f64, t.full_height.max(1) as f64);
                    plane.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
                        let v = (t.y + y) as f64 / fh;
                        for (x, out) in row.iter_mut().enumerate() {
                            *out = map.at((t.x + x) as f64 / fw, v);
                        }
                    });
                    // The model sees a shrunk photo, so its edge is soft;
                    // the photo's own edges sharpen it (a guided filter over
                    // about a cell and a half of the model's grid).
                    if let Some(rgb) = r.srgb {
                        plane = refine_edges(&plane, rgb, w, h, t, map.width.max(map.height));
                    }
                    targets.sky_measured.push(map);
                }
            }
        }
        Shape::Unsupported => {}
    }
    plane
}

/// The mean colour around each sample (5×5 pixels).
pub(super) fn sample_colours(
    lab: &[[f32; 3]],
    w: usize,
    h: usize,
    frame: &Frame,
    samples: &[(f64, f64)],
) -> Vec<[f32; 3]> {
    if w == 0 || h == 0 {
        return Vec::new();
    }
    samples
        .iter()
        .map(|&(u, v)| {
            let (fx, fy) = frame.pixel(u, v);
            let (x, y) = frame.target(fx, fy);
            let x = x.round().clamp(0.0, (w - 1) as f64) as usize;
            let y = y.round().clamp(0.0, (h - 1) as f64) as usize;
            let mut sum = [0.0f32; 3];
            let mut n = 0.0f32;
            for yy in y.saturating_sub(2)..(y + 3).min(h) {
                for xx in x.saturating_sub(2)..(x + 3).min(w) {
                    let p = lab[yy * w + xx];
                    for c in 0..3 {
                        sum[c] += p[c];
                    }
                    n += 1.0;
                }
            }
            sum.map(|s| s / n)
        })
        .collect()
}

/// A colour range mask selecting `targets`.
pub(super) fn draw_color_range(
    plane: &mut [f32],
    lab: &[[f32; 3]],
    targets: &[[f32; 3]],
    amount: f64,
    k: &MaskStrengths,
) {
    let tol =
        (k.color_tolerance_min + (k.color_tolerance_max - k.color_tolerance_min) * amount) as f32;
    plane.par_iter_mut().zip(lab.par_iter()).for_each(|(v, p)| {
        let mut best = 0.0f32;
        for t in targets {
            let d = ((0.5 * (p[0] - t[0])).powi(2) + (p[1] - t[1]).powi(2) + (p[2] - t[2]).powi(2))
                .sqrt();
            let m = 1.0 - smoothstep(0.5 * tol as f64, tol as f64, d as f64) as f32;
            best = best.max(m);
        }
        *v = best;
    });
}

/// A correction's final alpha plane.
pub(super) fn draw_correction(
    c: &Correction,
    frame: &Frame,
    r: &Reference,
    targets: &mut Targets,
) -> Vec<f32> {
    let mut alpha = vec![0.0f32; r.width * r.height];
    for part in &c.parts {
        let mut p = draw_shape(&part.shape, frame, r, targets);
        let (_, _, inverted) = part.combination.stored();
        if inverted {
            p.par_iter_mut().for_each(|v| *v = 1.0 - *v);
        }
        match part.combination.mode {
            Mode::Add => {
                let o = part.opacity as f32;
                alpha
                    .par_iter_mut()
                    .zip(p.par_iter())
                    .for_each(|(a, &v)| *a = a.max(v * o));
            }
            Mode::Subtract | Mode::Intersect => {
                alpha
                    .par_iter_mut()
                    .zip(p.par_iter())
                    .for_each(|(a, &v)| *a *= 1.0 - v);
            }
        }
    }
    if let Some((shape, invert)) = &c.range {
        let p = draw_shape(shape, frame, r, targets);
        alpha.par_iter_mut().zip(p.par_iter()).for_each(|(a, &v)| {
            *a *= if *invert { 1.0 - v } else { v };
        });
    }
    alpha
}
