// SPDX-License-Identifier: GPL-3.0-or-later
//! Rendering an aligned panorama.
//!
//! - **Projection** (Lightroom's choices): spherical (longitude and
//!   latitude), cylindrical (longitude and height: verticals stay straight)
//!   or perspective (straight lines stay straight; only for narrower fields).
//!   Auto picks perspective under about 100° × 90°, cylindrical up to 100°
//!   high, spherical beyond.
//! - **Brightness:** a gain per frame so overlaps agree, solved over every
//!   overlap with a pull towards 1 (Brown & Lowe's gain compensation).
//! - **Blending** in two bands: the smooth part (a blur of about 0.6% of the
//!   frame) averaged with weights that fall to zero at each frame's edges,
//!   the detail taken from the frame whose weight is highest. Done in strips
//!   of rows, so only one strip of each frame is held at a time.
//! - **Auto crop:** the largest rectangle the frames cover.

use rayon::prelude::*;

use super::align::Aligned;
use super::linalg::{self, M3, V3};
use crate::filters::gaussian_blur;
use crate::lens::sample_bicubic_rgb;
use crate::raw::{self, CameraImage};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Projection {
    #[default]
    Auto,
    Spherical,
    Cylindrical,
    Perspective,
}

impl Projection {
    pub fn parse(s: &str) -> Option<Projection> {
        match s.trim().to_ascii_lowercase().as_str() {
            "auto" => Some(Projection::Auto),
            "spherical" => Some(Projection::Spherical),
            "cylindrical" => Some(Projection::Cylindrical),
            "perspective" => Some(Projection::Perspective),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Projection::Auto => "auto",
            Projection::Spherical => "spherical",
            Projection::Cylindrical => "cylindrical",
            Projection::Perspective => "perspective",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PanoOptions {
    pub projection: Projection,
    /// Cut to the largest rectangle the frames cover (otherwise the
    /// uncovered corners are black).
    pub auto_crop: bool,
}

impl Default for PanoOptions {
    fn default() -> Self {
        PanoOptions {
            projection: Projection::Auto,
            auto_crop: true,
        }
    }
}

/// A stitched panorama and how it was made.
pub struct Panorama {
    /// Camera RGB of the reference frame's camera, upright.
    pub image: CameraImage,
    /// The projection used (never `Auto`).
    pub projection: Projection,
    /// The frame whose colour data and metadata the result keeps.
    pub reference: usize,
    /// Field of view covered, degrees across and up.
    pub field: (f64, f64),
    /// Per frame: the brightness gain applied.
    pub gains: Vec<f64>,
    /// Alignment residual (pixels), and the overlapping pairs.
    pub rms: f64,
    pub links: Vec<(usize, usize, usize)>,
}

/// Largest canvas rendered, in pixels; bigger panoramas are scaled down.
const MAX_PIXELS: f64 = 300e6;
/// Rows per strip.
const STRIP: usize = 512;

#[derive(Debug, Clone, Copy)]
struct Projector {
    kind: Projection,
    /// Canvas pixels per radian at the centre.
    scale: f64,
}

impl Projector {
    fn forward(&self, w: V3) -> Option<(f64, f64)> {
        let s = self.scale;
        let flat = w[0].hypot(w[2]);
        match self.kind {
            Projection::Perspective => {
                (w[2] > 1e-6 * linalg::norm(w)).then(|| (s * w[0] / w[2], s * w[1] / w[2]))
            }
            Projection::Cylindrical => {
                (flat > 1e-9).then(|| (s * w[0].atan2(w[2]), s * w[1] / flat))
            }
            Projection::Spherical | Projection::Auto => {
                Some((s * w[0].atan2(w[2]), s * w[1].atan2(flat)))
            }
        }
    }

    fn inverse(&self, u: f64, v: f64) -> Option<V3> {
        let (a, b) = (u / self.scale, v / self.scale);
        match self.kind {
            Projection::Perspective => Some([a, b, 1.0]),
            Projection::Cylindrical => {
                (a.abs() <= std::f64::consts::PI).then(|| [a.sin(), b, a.cos()])
            }
            Projection::Spherical | Projection::Auto => (a.abs() <= std::f64::consts::PI
                && b.abs() <= std::f64::consts::FRAC_PI_2)
                .then(|| [a.sin() * b.cos(), b.sin(), a.cos() * b.cos()]),
        }
    }
}

/// A frame as the renderer sees it.
struct Camera<'a> {
    frame: &'a CameraImage,
    r: M3,
    rt: M3,
    focal: f64,
    cx: f64,
    cy: f64,
}

impl Camera<'_> {
    /// The frame pixel showing world direction `w`, if any.
    #[inline]
    fn pixel(&self, w: V3) -> Option<(f64, f64)> {
        let c = linalg::apply(&self.r, w);
        if c[2] <= 1e-9 {
            return None;
        }
        let (x, y) = (
            self.focal * c[0] / c[2] + self.cx,
            self.focal * c[1] / c[2] + self.cy,
        );
        let (w, h) = (self.frame.width as f64, self.frame.height as f64);
        (x >= 0.0 && y >= 0.0 && x <= w - 1.0 && y <= h - 1.0).then_some((x, y))
    }

    /// Blending weight: falls linearly to zero at each edge.
    #[inline]
    fn weight(&self, x: f64, y: f64) -> f32 {
        let (w, h) = (
            self.frame.width as f64 - 1.0,
            self.frame.height as f64 - 1.0,
        );
        ((1.0 - (2.0 * x / w - 1.0).abs()) * (1.0 - (2.0 * y / h - 1.0).abs())).max(0.0) as f32
    }

    /// World directions along the frame's border (and its centre).
    fn outline(&self) -> Vec<V3> {
        let (w, h) = (
            self.frame.width as f64 - 1.0,
            self.frame.height as f64 - 1.0,
        );
        let n = 48;
        let mut pts = vec![(self.cx, self.cy)];
        for k in 0..=n {
            let t = k as f64 / n as f64;
            pts.extend([(t * w, 0.0), (t * w, h), (0.0, t * h), (w, t * h)]);
        }
        pts.iter()
            .map(|&(x, y)| linalg::apply(&self.rt, [x - self.cx, y - self.cy, self.focal]))
            .collect()
    }
}

/// Bounds (u0, v0, u1, v1) of the frames' outlines under `p`; `None` when an
/// outline can't be projected (a perspective too wide).
fn bounds(cams: &[Camera], p: &Projector) -> Option<[f64; 4]> {
    let mut b = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    for c in cams {
        for w in c.outline() {
            if p.kind == Projection::Perspective && w[2] < 0.17 * linalg::norm(w) {
                return None; // more than 80° off the axis
            }
            let (u, v) = p.forward(w)?;
            b = [b[0].min(u), b[1].min(v), b[2].max(u), b[3].max(v)];
        }
    }
    Some(b)
}

/// Render the aligned `frames`.
pub fn render(
    frames: &[CameraImage],
    aligned: &Aligned,
    opts: &PanoOptions,
) -> Result<Panorama, crate::Error> {
    let unsupported = |m: &str| crate::Error::Unsupported(m.into());
    let cams: Vec<Camera> = frames
        .iter()
        .zip(&aligned.rotations)
        .map(|(frame, r)| Camera {
            frame,
            r: *r,
            rt: linalg::transpose(r),
            focal: aligned.focal,
            cx: (frame.width as f64 - 1.0) / 2.0,
            cy: (frame.height as f64 - 1.0) / 2.0,
        })
        .collect();

    // The field covered, measured spherically, picks the projection.
    let sphere = Projector {
        kind: Projection::Spherical,
        scale: 1.0,
    };
    let sb = bounds(&cams, &sphere).ok_or_else(|| unsupported("the frames can't be placed"))?;
    let field = ((sb[2] - sb[0]).to_degrees(), (sb[3] - sb[1]).to_degrees());
    let area: f64 = frames.iter().map(|f| (f.width * f.height) as f64).sum();
    let mut kind = match opts.projection {
        Projection::Auto if field.0 <= 100.0 && field.1 <= 90.0 => Projection::Perspective,
        Projection::Auto if field.1 <= 100.0 => Projection::Cylindrical,
        Projection::Auto => Projection::Spherical,
        k => k,
    };
    let mut proj = Projector {
        kind,
        scale: aligned.focal,
    };
    let mut b = bounds(&cams, &proj);
    // A perspective that stretches the edges too far falls back (or fails,
    // when asked for).
    let stretched =
        |b: &Option<[f64; 4]>| b.is_none_or(|b| (b[2] - b[0]) * (b[3] - b[1]) > 6.0 * area);
    if kind == Projection::Perspective && stretched(&b) {
        if opts.projection == Projection::Perspective {
            return Err(unsupported(
                "the panorama is too wide for Perspective; use Cylindrical or Spherical",
            ));
        }
        kind = Projection::Cylindrical;
        proj.kind = kind;
        b = bounds(&cams, &proj);
    }
    let b = b.ok_or_else(|| unsupported("the frames can't be projected"))?;
    let pixels = (b[2] - b[0]) * (b[3] - b[1]);
    if pixels > MAX_PIXELS {
        proj.scale *= (MAX_PIXELS / pixels).sqrt();
    }
    let k = proj.scale / aligned.focal;
    let (u0, v0) = (b[0] * k, b[1] * k);
    let width = ((b[2] - b[0]) * k).ceil() as usize + 1;
    let height = ((b[3] - b[1]) * k).ceil() as usize + 1;
    let canvas = Canvas {
        proj,
        u0: u0.floor(),
        v0: v0.floor(),
        width,
        height,
    };

    // Brightness gains and coverage, on a small render.
    let small = Survey::new(&cams, &canvas);
    let gains = small.gains();
    let crop = if opts.auto_crop {
        small.largest_rectangle()
    } else {
        Some((0, 0, width, height))
    };
    let (x0, y0, cw, ch) = crop.ok_or_else(|| unsupported("the frames cover no rectangle"))?;

    let rgb = blend(&cams, &canvas, &gains, (x0, y0, cw, ch));
    let reference = aligned.reference;
    let mut image = frames[reference].with_pixels(rgb, cw, ch);
    image.metadata.orientation = 1;
    Ok(Panorama {
        image,
        projection: kind,
        reference,
        field,
        gains,
        rms: aligned.rms,
        links: aligned.links.clone(),
    })
}

/// The output plane: canvas pixel (x, y) is projected point (u0 + x, v0 + y).
struct Canvas {
    proj: Projector,
    u0: f64,
    v0: f64,
    width: usize,
    height: usize,
}

impl Canvas {
    fn direction(&self, x: f64, y: f64) -> Option<V3> {
        self.proj.inverse(self.u0 + x, self.v0 + y)
    }

    /// A frame's bounding box on the canvas (x0, y0, x1, y1), a little wide.
    fn footprint(&self, cam: &Camera) -> (usize, usize, usize, usize) {
        let (mut a, mut b) = (
            (f64::INFINITY, f64::INFINITY),
            (f64::NEG_INFINITY, f64::NEG_INFINITY),
        );
        for w in cam.outline() {
            if let Some((u, v)) = self.proj.forward(w) {
                let (x, y) = (u - self.u0, v - self.v0);
                a = (a.0.min(x), a.1.min(y));
                b = (b.0.max(x), b.1.max(y));
            }
        }
        let clamp = |v: f64, max: usize| (v.max(0.0) as usize).min(max);
        (
            clamp(a.0 - 2.0, self.width),
            clamp(a.1 - 2.0, self.height),
            clamp(b.0 + 3.0, self.width),
            clamp(b.1 + 3.0, self.height),
        )
    }
}

/// A small render of the frames' luminance on the canvas, for gains and
/// coverage.
struct Survey {
    w: usize,
    h: usize,
    /// Per small pixel, each frame's luminance (NaN where it doesn't reach).
    lum: Vec<Vec<f32>>,
    /// Canvas pixels per small pixel.
    step: usize,
}

impl Survey {
    fn new(cams: &[Camera], canvas: &Canvas) -> Survey {
        let step = canvas.width.max(canvas.height).div_ceil(800).max(1);
        let (w, h) = (canvas.width.div_ceil(step), canvas.height.div_ceil(step));
        let lum = cams
            .par_iter()
            .map(|cam| {
                let small = raw::downscale(cam.frame, 600);
                let f = small.width as f64 / cam.frame.width as f64;
                (0..w * h)
                    .map(|k| {
                        let (x, y) = ((k % w * step) as f64, (k / w * step) as f64);
                        let Some(d) = canvas.direction(x, y) else {
                            return f32::NAN;
                        };
                        match cam.pixel(d) {
                            Some((px, py)) => {
                                let (sx, sy) = (
                                    ((px + 0.5) * f - 0.5).clamp(0.0, (small.width - 1) as f64),
                                    ((py + 0.5) * f - 0.5).clamp(0.0, (small.height - 1) as f64),
                                );
                                let i = (sy as usize * small.width + sx as usize) * 3;
                                let p = &small.rgb[i..i + 3];
                                0.25 * p[0] + 0.5 * p[1] + 0.25 * p[2]
                            }
                            None => f32::NAN,
                        }
                    })
                    .collect()
            })
            .collect();
        Survey { w, h, lum, step }
    }

    /// A gain per frame that makes overlaps agree, pulled towards 1.
    fn gains(&self) -> Vec<f64> {
        let n = self.lum.len();
        // Overlap pixel counts and mean luminances (Ī_ij: frame i over its
        // overlap with j).
        let mut count = vec![vec![0.0f64; n]; n];
        let mut mean = vec![vec![0.0f64; n]; n];
        for k in 0..self.w * self.h {
            for i in 0..n {
                let a = self.lum[i][k];
                if !a.is_finite() {
                    continue;
                }
                for j in 0..n {
                    let b = self.lum[j][k];
                    if i != j && b.is_finite() {
                        count[i][j] += 1.0;
                        mean[i][j] += f64::from(a);
                    }
                }
            }
        }
        let mut overall = (0.0, 0.0);
        for i in 0..n {
            for j in 0..n {
                if count[i][j] > 0.0 {
                    mean[i][j] /= count[i][j];
                    overall = (
                        overall.0 + mean[i][j] * count[i][j],
                        overall.1 + count[i][j],
                    );
                }
            }
        }
        if overall.1 == 0.0 {
            return vec![1.0; n];
        }
        let sigma_n = 0.1 * overall.0 / overall.1;
        let (sn2, sg2) = (sigma_n * sigma_n, 0.1f64 * 0.1);
        let mut a = vec![vec![0.0; n]; n];
        let mut b = vec![0.0; n];
        for i in 0..n {
            for j in 0..n {
                let c = count[i][j];
                if i == j || c == 0.0 {
                    continue;
                }
                a[i][i] += c * (mean[i][j] * mean[i][j] / sn2 + 1.0 / sg2);
                a[i][j] -= c * mean[i][j] * mean[j][i] / sn2;
                b[i] += c / sg2;
            }
            if a[i][i] == 0.0 {
                a[i][i] = 1.0;
                b[i] = 1.0;
            }
        }
        linalg::solve(a, b)
            .filter(|g| g.iter().all(|v| *v > 0.2 && *v < 5.0))
            .unwrap_or_else(|| vec![1.0; n])
    }

    /// The largest axis-aligned rectangle every one of whose small pixels a
    /// frame covers, in canvas pixels, one small pixel in from its edge.
    #[allow(clippy::needless_range_loop)]
    fn largest_rectangle(&self) -> Option<(usize, usize, usize, usize)> {
        let (w, h) = (self.w, self.h);
        let covered = |k: usize| self.lum.iter().any(|l| l[k].is_finite());
        let mut heights = vec![0usize; w];
        let mut best = (0usize, (0, 0, 0, 0));
        for y in 0..h {
            for x in 0..w {
                heights[x] = if covered(y * w + x) {
                    heights[x] + 1
                } else {
                    0
                };
            }
            // Largest rectangle under the histogram, with a stack.
            let mut stack: Vec<usize> = Vec::new();
            for x in 0..=w {
                let cur = if x < w { heights[x] } else { 0 };
                while let Some(&top) = stack.last() {
                    if heights[top] <= cur {
                        break;
                    }
                    stack.pop();
                    let left = stack.last().map_or(0, |&l| l + 1);
                    let area = heights[top] * (x - left);
                    if area > best.0 {
                        best = (area, (left, y + 1 - heights[top], x, y + 1));
                    }
                }
                stack.push(x);
            }
        }
        let (_, (x0, y0, x1, y1)) = best;
        // One small pixel in from each edge (coverage was sampled at corners).
        let (x0, y0) = ((x0 + 1) * self.step, (y0 + 1) * self.step);
        let (x1, y1) = (
            x1.saturating_sub(1) * self.step,
            y1.saturating_sub(1) * self.step,
        );
        (x1 > x0 + 16 && y1 > y0 + 16).then_some((x0, y0, x1 - x0, y1 - y0))
    }
}

/// Two-band blend of the frames into the canvas rectangle `(x0, y0, w, h)`.
fn blend(
    cams: &[Camera],
    canvas: &Canvas,
    gains: &[f64],
    (x0, y0, w, h): (usize, usize, usize, usize),
) -> Vec<f32> {
    let long = cams
        .iter()
        .map(|c| c.frame.width.max(c.frame.height))
        .max()
        .unwrap_or(1) as f64;
    let k = canvas.proj.scale / cams[0].focal;
    let sigma = (0.006 * long * k).max(2.0) as f32;
    let margin = (3.0 * sigma).ceil() as usize + 2;
    let footprints: Vec<_> = cams.iter().map(|c| canvas.footprint(c)).collect();
    let mut out = vec![0.0f32; w * h * 3];
    for (band, rows) in out.chunks_mut(STRIP * w * 3).enumerate() {
        let (b0, b1) = (y0 + band * STRIP, y0 + band * STRIP + rows.len() / (w * 3));
        let n = (b1 - b0) * w;
        let mut low = vec![0.0f32; n * 3];
        let mut wsum = vec![0.0f32; n];
        let mut high = vec![0.0f32; n * 3];
        let mut best = vec![0.0f32; n];
        for (i, cam) in cams.iter().enumerate() {
            let (fx0, fy0, fx1, fy1) = footprints[i];
            // The frame's part of the strip, with a margin for the blur.
            let (ry0, ry1) = (
                fy0.max(b0.saturating_sub(margin)),
                fy1.min(b1 + margin).min(canvas.height),
            );
            let (rx0, rx1) = (
                fx0.max(x0.saturating_sub(margin)),
                fx1.min(x0 + w + margin).min(canvas.width),
            );
            if ry0 >= ry1 || rx0 >= rx1 || fy1 <= b0 || fy0 >= b1 || fx1 <= x0 || fx0 >= x0 + w {
                continue;
            }
            let (rw, rh) = (rx1 - rx0, ry1 - ry0);
            let gain = gains[i] as f32;
            // Warp: colour, validity and weight per pixel.
            let warped: Vec<([f32; 3], f32, f32)> = (0..rw * rh)
                .into_par_iter()
                .map(|p| {
                    let (x, y) = ((rx0 + p % rw) as f64, (ry0 + p / rw) as f64);
                    canvas.direction(x, y).and_then(|d| cam.pixel(d)).map_or(
                        ([0.0; 3], 0.0, 0.0),
                        |(px, py)| {
                            let c = sample_bicubic_rgb(
                                &cam.frame.rgb,
                                cam.frame.width,
                                cam.frame.height,
                                px,
                                py,
                            );
                            (c.map(|v| v * gain), 1.0, cam.weight(px, py))
                        },
                    )
                })
                .collect();
            // The smooth band: a normalised blur (edges don't darken it).
            let valid: Vec<f32> = warped.iter().map(|p| p.1).collect();
            let vb = gaussian_blur(&valid, rw, rh, sigma);
            let smooth: Vec<Vec<f32>> = (0..3)
                .into_par_iter()
                .map(|c| {
                    let plane: Vec<f32> = warped.iter().map(|p| p.0[c] * p.1).collect();
                    gaussian_blur(&plane, rw, rh, sigma)
                })
                .collect();
            for y in b0.max(ry0)..b1.min(ry1) {
                for x in x0.max(rx0)..(x0 + w).min(rx1) {
                    let p = (y - ry0) * rw + (x - rx0);
                    let (c, v, wt) = warped[p];
                    if v == 0.0 || wt <= 0.0 {
                        continue;
                    }
                    let norm = vb[p].max(1e-6);
                    let s = [
                        smooth[0][p] / norm,
                        smooth[1][p] / norm,
                        smooth[2][p] / norm,
                    ];
                    let o = (y - b0) * w + (x - x0);
                    for ch in 0..3 {
                        low[o * 3 + ch] += wt * s[ch];
                    }
                    wsum[o] += wt;
                    if wt > best[o] {
                        best[o] = wt;
                        for ch in 0..3 {
                            high[o * 3 + ch] = c[ch] - s[ch];
                        }
                    }
                }
            }
        }
        rows.par_chunks_mut(3).enumerate().for_each(|(o, px)| {
            if wsum[o] > 0.0 {
                for ch in 0..3 {
                    px[ch] = (low[o * 3 + ch] / wsum[o] + high[o * 3 + ch]).max(0.0);
                }
            }
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::testing::{photograph, scene, turn};
    use super::super::{align, linalg};
    use super::*;

    fn sweep(f: f64) -> Vec<CameraImage> {
        [
            turn(-35.0, 1.0, 0.5),
            turn(0.0, 0.0, 0.0),
            turn(34.0, -1.0, -0.5),
        ]
        .iter()
        .map(|r| photograph(r, f, 480, 320))
        .collect()
    }

    #[test]
    fn stitches_a_sweep_into_the_scene() {
        let f = 420.0;
        let frames = sweep(f);
        let aligned = align(&frames, Some(f)).unwrap();
        for kind in [
            Projection::Cylindrical,
            Projection::Spherical,
            Projection::Perspective,
        ] {
            let p = render(
                &frames,
                &aligned,
                &PanoOptions {
                    projection: kind,
                    auto_crop: true,
                },
            )
            .unwrap();
            assert_eq!(p.projection, kind);
            let img = &p.image;
            assert!(
                img.width > 2 * img.height,
                "{kind:?}: {}x{}",
                img.width,
                img.height
            );
            assert!(p.field.0 > 100.0 && p.field.0 < 140.0, "{:?}", p.field);
            assert!(
                p.gains.iter().all(|g| (g - 1.0).abs() < 0.03),
                "{:?}",
                p.gains
            );
            assert!(img.rgb.iter().all(|v| v.is_finite()));
            // Compare with the scene itself, through the same projection:
            // canvas pixel → world direction → the true brightness.
            let (mut err, mut count) = (0.0f64, 0usize);
            let proj = Projector {
                kind,
                scale: aligned.focal,
            };
            // Re-derive the canvas origin from the frames' bounds.
            let cams: Vec<Camera> = frames
                .iter()
                .zip(&aligned.rotations)
                .map(|(frame, r)| Camera {
                    frame,
                    r: *r,
                    rt: linalg::transpose(r),
                    focal: aligned.focal,
                    cx: (frame.width as f64 - 1.0) / 2.0,
                    cy: (frame.height as f64 - 1.0) / 2.0,
                })
                .collect();
            let b = bounds(&cams, &proj).unwrap();
            let small = Survey::new(
                &cams,
                &Canvas {
                    proj,
                    u0: b[0].floor(),
                    v0: b[1].floor(),
                    width: (b[2] - b[0]).ceil() as usize + 1,
                    height: (b[3] - b[1]).ceil() as usize + 1,
                },
            );
            let (x0, y0, _, _) = small.largest_rectangle().unwrap();
            // The levelled world differs from the scene's by the reference
            // frame's turn: undo it with the reference's rotations.
            let to_scene = linalg::mul(
                &linalg::transpose(&turn(0.0, 0.0, 0.0)),
                &aligned.rotations[1],
            );
            for y in (8..img.height - 8).step_by(7) {
                for x in (8..img.width - 8).step_by(7) {
                    let d = proj
                        .inverse(
                            b[0].floor() + (x0 + x) as f64,
                            b[1].floor() + (y0 + y) as f64,
                        )
                        .unwrap();
                    let truth = f64::from(scene(linalg::apply(&to_scene, d)));
                    let got = f64::from(img.rgb[(y * img.width + x) * 3 + 1]);
                    err += (got - truth).abs();
                    count += 1;
                }
            }
            let mean = err / count as f64;
            assert!(mean < 0.02, "{kind:?}: mean error {mean}");
        }
    }

    #[test]
    fn auto_picks_by_field_and_crop_is_optional() {
        let f = 420.0;
        let frames = sweep(f);
        let aligned = align(&frames, Some(f)).unwrap();
        let auto = render(&frames, &aligned, &PanoOptions::default()).unwrap();
        // About 125° across: too wide for perspective.
        assert_eq!(auto.projection, Projection::Cylindrical);
        let uncropped = render(
            &frames,
            &aligned,
            &PanoOptions {
                projection: Projection::Cylindrical,
                auto_crop: false,
            },
        )
        .unwrap();
        assert!(
            uncropped.image.width > auto.image.width && uncropped.image.height > auto.image.height
        );
        // Uncovered corners are black.
        assert_eq!(&uncropped.image.rgb[..3], &[0.0, 0.0, 0.0]);
        assert!(
            render(
                &frames,
                &aligned,
                &PanoOptions {
                    projection: Projection::Perspective,
                    auto_crop: true
                }
            )
            .is_ok(),
            "125° still fits a perspective"
        );
    }
}
