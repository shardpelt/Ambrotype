// SPDX-License-Identifier: GPL-3.0-or-later
//! Photo Merge > HDR: bracketed raws of one scene merged into one
//! scene-linear image, with the highlights of the darker frames and the clean
//! shadows of the brighter ones.
//!
//! **Model (ours, not Adobe's).** Lightroom offers Auto Align, Auto Tone and
//! Deghost (None, Low, Medium, High) and writes a floating-point DNG
//! (docs/research/01 §Photo Merge). Here, on demosaiced camera RGB:
//!
//! 1. **Reference.** The middle exposure. The merge keeps its brightness
//!    (white = 1) and its colour data and metadata.
//! 2. **Alignment** (optional): a translation per frame, by median threshold
//!    bitmaps (Ward, 2003): each frame's luminance cut at its own median gives
//!    nearly the same bitmap whatever the exposure, so the shift that best
//!    matches the bitmaps is searched coarse to fine, up to 64 pixels.
//!    Pixels near the median are left out of the count (noise flips them).
//! 3. **Exposure.** Each frame's brightness relative to the reference is the
//!    median ratio over pixels well exposed in both (after alignment), so it
//!    is right even when the shutter's nominal times are not; the EXIF
//!    exposure (time × ISO / f-number²) is the fallback.
//! 4. **Merge.** Per pixel, every frame brought to the reference exposure,
//!    averaged with weights proportional to its exposure (more light, less
//!    noise) and falling to zero as any channel nears its clip level. Where
//!    every frame is clipped, the darkest one is used as it is.
//! 5. **Deghost** (optional): where a frame disagrees with the merge by more
//!    than the level allows (something moved), the area takes a single
//!    exposure: the reference, or the nearest darker frame where the
//!    reference is clipped. The map of such areas is widened and softened so
//!    the switch doesn't show.
//!
//! Rotation between frames is not corrected yet.

use rayon::prelude::*;

use crate::filters::box_blur;
use crate::raw::CameraImage;

/// How strongly moving things are removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Deghost {
    #[default]
    Off,
    Low,
    Medium,
    High,
}

impl Deghost {
    /// "off"/"none", "low", "medium", "high".
    pub fn parse(s: &str) -> Option<Deghost> {
        match s.trim().to_ascii_lowercase().as_str() {
            "off" | "none" | "0" => Some(Deghost::Off),
            "low" | "1" => Some(Deghost::Low),
            "medium" | "2" => Some(Deghost::Medium),
            "high" | "3" => Some(Deghost::High),
            _ => None,
        }
    }

    /// Largest difference (natural log of luminance) a frame may have from
    /// the merge before the area counts as moving.
    fn threshold(self) -> Option<f32> {
        match self {
            Deghost::Off => None,
            Deghost::Low => Some(0.6),
            Deghost::Medium => Some(0.35),
            Deghost::High => Some(0.2),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HdrOptions {
    /// Find and undo the shift between frames.
    pub align: bool,
    pub deghost: Deghost,
}

impl Default for HdrOptions {
    fn default() -> Self {
        HdrOptions {
            align: true,
            deghost: Deghost::Off,
        }
    }
}

/// An HDR merge and what was found on the way.
pub struct Merged {
    /// Camera RGB at the reference frame's exposure; values above 1 are the
    /// highlights the darker frames kept.
    pub image: CameraImage,
    /// Which input frame the merge keeps the brightness and metadata of.
    pub reference: usize,
    /// Each frame's exposure relative to the reference (2.0 = one stop more).
    pub exposures: Vec<f64>,
    /// Each frame's shift: its pixel (x + dx, y + dy) shows the reference's (x, y).
    pub shifts: Vec<(i32, i32)>,
    /// Share of the photo deghosting took from a single frame.
    pub ghost_share: f64,
}

/// Weights fall from 1 to 0 as a frame's most exposed channel goes from
/// this share of its clip level ...
const CLIP_SOFT: f32 = 0.80;
/// ... to this one.
const CLIP_HARD: f32 = 0.96;
/// Largest shift alignment finds: 2^levels − 1 pixels.
const ALIGN_LEVELS: usize = 6;

/// Merge bracketed frames of one scene. They must be the same size (the same
/// camera, unrotated).
pub fn hdr(frames: &[CameraImage], opts: &HdrOptions) -> Result<Merged, crate::Error> {
    let unsupported = |m: String| crate::Error::Unsupported(m);
    let first = frames
        .first()
        .ok_or_else(|| unsupported("HDR merge needs photos".into()))?;
    if frames.len() < 2 {
        return Err(unsupported("HDR merge needs at least two photos".into()));
    }
    let (w, h) = (first.width, first.height);
    if frames.iter().any(|f| (f.width, f.height) != (w, h)) {
        return Err(unsupported(
            "the photos differ in size; HDR merge needs frames from one camera".into(),
        ));
    }
    if frames.iter().any(|f| f.rendered) {
        return Err(unsupported(
            "HDR merge needs raw photos (JPEGs and other rendered images can't be merged yet)"
                .into(),
        ));
    }

    // 1. Order by exposure (EXIF, else mean brightness) and take the middle.
    let nominal: Vec<Option<f64>> = frames.iter().map(nominal_exposure).collect();
    let brightness: Vec<f64> = if nominal.iter().all(Option::is_some) {
        nominal.iter().map(|e| e.unwrap_or(1.0)).collect()
    } else {
        frames.iter().map(mean_green).collect()
    };
    let mut order: Vec<usize> = (0..frames.len()).collect();
    order.sort_by(|&a, &b| brightness[a].total_cmp(&brightness[b]));
    let reference = order[frames.len() / 2];
    let r = &frames[reference];

    // 2. Alignment.
    let shifts: Vec<(i32, i32)> = (0..frames.len())
        .map(|i| {
            if i == reference || !opts.align {
                (0, 0)
            } else {
                mtb_shift(r, &frames[i])
            }
        })
        .collect();

    // 3. Exposure relative to the reference.
    let mut exposures = Vec::with_capacity(frames.len());
    for (i, f) in frames.iter().enumerate() {
        let measured = if i == reference {
            Some(1.0)
        } else {
            measured_ratio(r, f, shifts[i])
        };
        let nominal_ratio = match (nominal[i], nominal[reference]) {
            (Some(a), Some(b)) if b > 0.0 => Some(a / b),
            _ => None,
        };
        let ratio = measured.or(nominal_ratio).ok_or_else(|| {
            unsupported(format!(
                "can't tell how bright photo {} is against the others (no overlap and no exposure data)",
                i + 1
            ))
        })?;
        exposures.push(ratio);
    }

    // 4. Merge.
    let clip: Vec<[f32; 3]> = frames
        .iter()
        .map(|f| {
            f.clip
                .map(|c| if c.is_finite() && c > 0.0 { c } else { 1.0 })
        })
        .collect();
    let frame = Frames {
        frames,
        shifts: &shifts,
        exposures: exposures.iter().map(|&e| e as f32).collect(),
        clip,
        w,
        h,
    };
    let mut rgb = vec![0.0f32; w * h * 3];
    rgb.par_chunks_mut(w * 3).enumerate().for_each(|(y, row)| {
        for (x, out) in row.chunks_exact_mut(3).enumerate() {
            out.copy_from_slice(&frame.merged(x, y, &order));
        }
    });

    // 5. Deghost.
    let mut ghost_share = 0.0;
    if let Some(threshold) = opts.deghost.threshold() {
        let lum = |p: [f32; 3]| (0.25 * p[0] + 0.5 * p[1] + 0.25 * p[2]).max(1e-6);
        let mut ghost: Vec<f32> = rgb
            .par_chunks(3)
            .enumerate()
            .map(|(k, m)| {
                let (x, y) = (k % w, k / w);
                let merged = lum([m[0], m[1], m[2]]).ln();
                let moved = (0..frames.len()).any(|i| {
                    frame.sample(i, x, y).is_some_and(|(v, s)| {
                        // Only where the frame is well exposed (not clipped,
                        // above its own noise).
                        let own = lum(v);
                        s < CLIP_SOFT
                            && own > 0.02
                            && ((own / frame.exposures[i]).ln() - merged).abs() > threshold
                    })
                });
                f32::from(u8::from(moved))
            })
            .collect();
        // Widen, then soften the edge of the moving areas.
        let r = (w.max(h) / 300).max(2);
        ghost = box_blur(&ghost, w, h, r);
        ghost.par_iter_mut().for_each(|g| *g = (*g * 4.0).min(1.0));
        ghost = box_blur(&ghost, w, h, r);
        ghost_share = ghost.iter().filter(|g| **g > 0.5).count() as f64 / (w * h) as f64;
        let darker: Vec<usize> = order[..=frames.len() / 2].iter().rev().copied().collect();
        rgb.par_chunks_mut(w * 3).enumerate().for_each(|(y, row)| {
            for (x, out) in row.chunks_exact_mut(3).enumerate() {
                let g = ghost[y * w + x];
                if g <= 0.0 {
                    continue;
                }
                let single = frame.single(x, y, &darker);
                for c in 0..3 {
                    out[c] += (single[c] - out[c]) * g;
                }
            }
        });
    }

    // Where every frame clipped, the darkest one's clip shows, at its scale.
    let darkest = order[0];
    let image_clip = std::array::from_fn(|c| {
        let k = frames[darkest].clip[c];
        if k.is_finite() {
            k / exposures[darkest] as f32
        } else {
            f32::INFINITY
        }
    });
    let mut image = r.with_pixels(rgb, w, h);
    image.clip = image_clip;
    Ok(Merged {
        image,
        reference,
        exposures,
        shifts,
        ghost_share,
    })
}

/// The frames to merge, with their shifts, exposures and clip levels.
struct Frames<'a> {
    frames: &'a [CameraImage],
    shifts: &'a [(i32, i32)],
    exposures: Vec<f32>,
    clip: Vec<[f32; 3]>,
    w: usize,
    h: usize,
}

impl Frames<'_> {
    /// Frame `i` at reference pixel (x, y): its value and how close its most
    /// exposed channel is to clipping (1 = at the clip level). `None` where
    /// the shift takes it outside the frame.
    #[inline]
    fn sample(&self, i: usize, x: usize, y: usize) -> Option<([f32; 3], f32)> {
        let (dx, dy) = self.shifts[i];
        let (sx, sy) = (x as i64 + dx as i64, y as i64 + dy as i64);
        if sx < 0 || sy < 0 || sx >= self.w as i64 || sy >= self.h as i64 {
            return None;
        }
        let k = (sy as usize * self.w + sx as usize) * 3;
        let p = &self.frames[i].rgb[k..k + 3];
        let clip = self.clip[i];
        let s = (p[0] / clip[0]).max(p[1] / clip[1]).max(p[2] / clip[2]);
        Some(([p[0], p[1], p[2]], s))
    }

    /// The weighted merge at reference pixel (x, y); `order` is darkest first.
    #[inline]
    fn merged(&self, x: usize, y: usize, order: &[usize]) -> [f32; 3] {
        let mut acc = [0.0f32; 3];
        let mut total = 0.0f32;
        for (i, &e) in self.exposures.iter().enumerate() {
            let Some((v, s)) = self.sample(i, x, y) else {
                continue;
            };
            let w = (1.0 - smoothstep(CLIP_SOFT, CLIP_HARD, s)) * e;
            if w > 0.0 {
                for c in 0..3 {
                    acc[c] += w * v[c] / e;
                }
                total += w;
            }
        }
        if total > 1e-9 {
            return acc.map(|a| a / total);
        }
        // Clipped everywhere: the darkest frame that covers the pixel.
        order
            .iter()
            .find_map(|&i| {
                self.sample(i, x, y)
                    .map(|(v, _)| v.map(|c| c / self.exposures[i]))
            })
            .unwrap_or([0.0; 3])
    }

    /// One frame's value at (x, y): the first of `frames` (the reference,
    /// then darker ones) that isn't clipped there, else the last.
    fn single(&self, x: usize, y: usize, frames: &[usize]) -> [f32; 3] {
        let mut last = None;
        for &i in frames {
            if let Some((v, s)) = self.sample(i, x, y) {
                let v = v.map(|c| c / self.exposures[i]);
                if s < CLIP_HARD {
                    return v;
                }
                last = Some(v);
            }
        }
        last.unwrap_or_else(|| self.merged(x, y, frames))
    }
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Exposure from EXIF: time × ISO / f-number² (any common scale).
fn nominal_exposure(f: &CameraImage) -> Option<f64> {
    let m = &f.metadata;
    let ratio = |r: Option<(u32, u32)>| {
        r.filter(|r| r.1 != 0)
            .map(|r| f64::from(r.0) / f64::from(r.1))
    };
    let t = ratio(m.exposure_time)?;
    let n = ratio(m.fnumber).unwrap_or(1.0);
    let iso = f64::from(m.iso.unwrap_or(100));
    let e = t * iso / (n * n);
    (e.is_finite() && e > 0.0).then_some(e)
}

/// Mean green level over a sample grid.
fn mean_green(f: &CameraImage) -> f64 {
    let n = f.width * f.height;
    let step = (n / 100_000).max(1);
    let (sum, count) = (0..n).step_by(step).fold((0.0, 0usize), |(s, c), k| {
        (s + f64::from(f.rgb[k * 3 + 1]), c + 1)
    });
    sum / count.max(1) as f64
}

/// `frame`'s brightness against `reference` as the median ratio of green over
/// pixels well exposed in both; `None` with too few such pixels.
fn measured_ratio(reference: &CameraImage, frame: &CameraImage, shift: (i32, i32)) -> Option<f64> {
    let (w, h) = (reference.width, reference.height);
    let step = ((w * h) as f64 / 250_000.0).sqrt().ceil().max(1.0) as usize;
    let usable = |img: &CameraImage, k: usize| {
        let p = &img.rgb[k * 3..k * 3 + 3];
        let clip = img
            .clip
            .map(|c| if c.is_finite() && c > 0.0 { c } else { 1.0 });
        let s = (p[0] / clip[0]).max(p[1] / clip[1]).max(p[2] / clip[2]);
        (s < CLIP_SOFT && p[1] > 0.01).then_some(p[1])
    };
    let mut logs: Vec<f32> = Vec::new();
    for y in (0..h).step_by(step) {
        let sy = y as i64 + shift.1 as i64;
        if sy < 0 || sy >= h as i64 {
            continue;
        }
        for x in (0..w).step_by(step) {
            let sx = x as i64 + shift.0 as i64;
            if sx < 0 || sx >= w as i64 {
                continue;
            }
            let (Some(a), Some(b)) = (
                usable(reference, y * w + x),
                usable(frame, sy as usize * w + sx as usize),
            ) else {
                continue;
            };
            logs.push((b / a).ln());
        }
    }
    if logs.len() < 500 {
        return None;
    }
    let mid = logs.len() / 2;
    let (_, median, _) = logs.select_nth_unstable_by(mid, f32::total_cmp);
    Some(f64::from(*median).exp())
}

/// A frame's luminance pyramid for alignment: level 0 at full size, each
/// next one half the size.
fn pyramid(f: &CameraImage, levels: usize) -> Vec<(Vec<f32>, usize, usize)> {
    let lum: Vec<f32> = f
        .rgb
        .par_chunks(3)
        .map(|p| 0.25 * p[0] + 0.5 * p[1] + 0.25 * p[2])
        .collect();
    let mut out = vec![(lum, f.width, f.height)];
    for _ in 1..levels {
        let (prev, w, h) = out.last().expect("level 0");
        let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
        let next: Vec<f32> = (0..nw * nh)
            .into_par_iter()
            .map(|k| {
                let (x, y) = ((k % nw) * 2, (k / nw) * 2);
                let at = |xx: usize, yy: usize| prev[yy.min(h - 1) * w + xx.min(w - 1)];
                0.25 * (at(x, y) + at(x + 1, y) + at(x, y + 1) + at(x + 1, y + 1))
            })
            .collect();
        out.push((next, nw, nh));
    }
    out
}

/// Threshold and exclusion bitmaps of one pyramid level: above the median,
/// and clearly away from it (more than about 5% either way).
fn bitmaps(lum: &[f32]) -> (Vec<bool>, Vec<bool>) {
    let step = (lum.len() / 200_000).max(1);
    let mut sample: Vec<f32> = lum.iter().step_by(step).copied().collect();
    let mid = sample.len() / 2;
    let median = *sample.select_nth_unstable_by(mid, f32::total_cmp).1;
    let (lo, hi) = (median * 0.95, median * 1.05);
    lum.par_iter()
        .map(|&v| (v > median, v < lo || v > hi))
        .unzip()
}

/// The shift of `frame` against `reference` (median threshold bitmaps,
/// coarse to fine).
fn mtb_shift(reference: &CameraImage, frame: &CameraImage) -> (i32, i32) {
    let a = pyramid(reference, ALIGN_LEVELS);
    let b = pyramid(frame, ALIGN_LEVELS);
    let (mut sx, mut sy) = (0i32, 0i32);
    for level in (0..ALIGN_LEVELS).rev() {
        let (la, w, h) = &a[level];
        let (lb, _, _) = &b[level];
        let (ta, ea) = bitmaps(la);
        let (tb, eb) = bitmaps(lb);
        let (w, h) = (*w as i64, *h as i64);
        let (cx, cy) = (sx * 2, sy * 2);
        let mut best = (u64::MAX, cx, cy);
        for dy in -1..=1 {
            for dx in -1..=1 {
                let (ox, oy) = (i64::from(cx + dx), i64::from(cy + dy));
                let err: u64 = (0..h)
                    .into_par_iter()
                    .map(|y| {
                        let ys = y + oy;
                        if ys < 0 || ys >= h {
                            return 0;
                        }
                        let mut e = 0u64;
                        for x in 0..w {
                            let xs = x + ox;
                            if xs < 0 || xs >= w {
                                continue;
                            }
                            let (i, j) = ((y * w + x) as usize, (ys * w + xs) as usize);
                            if ta[i] != tb[j] && ea[i] && eb[j] {
                                e += 1;
                            }
                        }
                        e
                    })
                    .sum();
                // Prefer the smaller shift on ties.
                if err < best.0 || (err == best.0 && dx == 0 && dy == 0) {
                    best = (err, cx + dx, cy + dy);
                }
            }
        }
        (sx, sy) = (best.1, best.2);
    }
    (sx, sy)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::raw::PhotoMetadata;

    /// A scene with a range of about 14 stops: a dim gradient, a textured
    /// wall and a bright window, as scene-linear light.
    fn scene(x: f64, y: f64) -> [f32; 3] {
        let wall = 0.02 + 0.3 * (x * 0.09).sin().abs() * (y * 0.07).cos().abs();
        let window = if (x - 150.0).abs() < 30.0 && (y - 40.0).abs() < 20.0 {
            2.5 + 0.5 * ((x + y) * 0.3).sin()
        } else {
            0.0
        };
        let shadow = if x < 40.0 {
            0.002 + 0.002 * (y * 0.2).sin()
        } else {
            0.0
        };
        let l = if x < 40.0 { shadow } else { wall + window };
        [l as f32 * 0.9, l as f32, l as f32 * 0.8]
    }

    /// One frame: the scene at `exposure`, shifted by `shift`, clipped at 1,
    /// with deterministic noise.
    fn frame(
        exposure: f64,
        time: (u32, u32),
        shift: (f64, f64),
        w: usize,
        h: usize,
    ) -> CameraImage {
        let mut rgb = Vec::with_capacity(w * h * 3);
        for y in 0..h {
            for x in 0..w {
                let p = scene(x as f64 - shift.0, y as f64 - shift.1);
                let seed = (exposure * 1000.0) as usize;
                let n = (((x * 31 + y * 17 + seed) % 13) as f32 / 13.0 - 0.5) * 0.002;
                rgb.extend(p.map(|v| (v * exposure as f32 + n).clamp(0.0, 1.0)));
            }
        }
        CameraImage {
            width: w,
            height: h,
            rgb,
            xyz_to_cam: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            as_shot_neutral: [1.0; 3],
            clip: [1.0; 3],
            rendered: false,
            make: "Test".into(),
            model: "Bracket".into(),
            shot: Default::default(),
            metadata: PhotoMetadata {
                exposure_time: Some(time),
                fnumber: Some((8, 1)),
                iso: Some(100),
                ..Default::default()
            },
        }
    }

    fn brackets(shifts: [(f64, f64); 3]) -> Vec<CameraImage> {
        vec![
            frame(4.0, (1, 15), shifts[0], 240, 160),
            frame(0.25, (1, 250), shifts[1], 240, 160),
            frame(1.0, (1, 60), shifts[2], 240, 160),
        ]
    }

    #[test]
    fn merges_highlights_and_shadows_at_the_reference_exposure() {
        let frames = brackets([(0.0, 0.0); 3]);
        let m = hdr(&frames, &HdrOptions::default()).unwrap();
        assert_eq!(m.reference, 2, "the middle exposure");
        assert_eq!(m.shifts, vec![(0, 0); 3]);
        assert!((m.exposures[0] - 4.0).abs() < 0.05, "{:?}", m.exposures);
        assert!((m.exposures[1] - 0.25).abs() < 0.01, "{:?}", m.exposures);
        // The window, clipped in the reference, comes from the darkest frame.
        let at = |x: usize, y: usize| {
            let k = (y * 240 + x) * 3;
            [m.image.rgb[k], m.image.rgb[k + 1], m.image.rgb[k + 2]]
        };
        let window = at(150, 40);
        let truth = scene(150.0, 40.0);
        assert!(window[1] > 1.5, "highlights above white: {window:?}");
        assert!(
            (window[1] / truth[1] - 1.0).abs() < 0.02,
            "{window:?} vs {truth:?}"
        );
        // The wall is close to the scene.
        for (x, y) in [(80, 100), (200, 140), (120, 20)] {
            let (v, t) = (at(x, y), scene(x as f64, y as f64));
            assert!((v[1] - t[1]).abs() < 0.01, "({x}, {y}): {v:?} vs {t:?}");
        }
        // The deep shadow comes from the brightest frame: less noise than the
        // reference has there.
        let shadow: Vec<f32> = (0..160)
            .map(|y| at(20, y)[1] - scene(20.0, y as f64)[1])
            .collect();
        let err = shadow.iter().map(|d| d.abs()).fold(0.0f32, f32::max);
        assert!(err < 0.0006, "{err}");
    }

    #[test]
    fn aligns_shifted_frames() {
        let frames = brackets([(3.0, -2.0), (-5.0, 4.0), (0.0, 0.0)]);
        let m = hdr(&frames, &HdrOptions::default()).unwrap();
        assert_eq!(m.shifts, vec![(3, -2), (-5, 4), (0, 0)]);
        let off = hdr(
            &frames,
            &HdrOptions {
                align: false,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(off.shifts, vec![(0, 0); 3]);
    }

    #[test]
    fn deghost_takes_moving_areas_from_one_frame() {
        let mut frames = brackets([(0.0, 0.0); 3]);
        // Something bright passes through the brightest frame only.
        for y in 100..130 {
            for x in 60..100 {
                let k = (y * 240 + x) * 3;
                frames[0].rgb[k..k + 3].copy_from_slice(&[0.7, 0.7, 0.7]);
            }
        }
        let plain = hdr(&frames, &HdrOptions::default()).unwrap();
        let clean = hdr(
            &frames,
            &HdrOptions {
                deghost: Deghost::Medium,
                ..Default::default()
            },
        )
        .unwrap();
        let k = (115 * 240 + 80) * 3 + 1;
        let truth = scene(80.0, 115.0)[1];
        assert!(
            (plain.image.rgb[k] - truth).abs() > 0.03,
            "a ghost without deghosting"
        );
        assert!(
            (clean.image.rgb[k] - truth).abs() < 0.01,
            "{} vs {truth}",
            clean.image.rgb[k]
        );
        assert!(
            clean.ghost_share > 0.01 && clean.ghost_share < 0.2,
            "{}",
            clean.ghost_share
        );
        // Away from it nothing changes.
        let far = (40 * 240 + 200) * 3 + 1;
        assert!((clean.image.rgb[far] - plain.image.rgb[far]).abs() < 1e-6);
    }

    #[test]
    fn exposures_come_from_the_pixels_when_exif_is_wrong() {
        let mut frames = brackets([(0.0, 0.0); 3]);
        // EXIF says one stop apart; the frames are two.
        frames[0].metadata.exposure_time = Some((1, 30));
        let m = hdr(&frames, &HdrOptions::default()).unwrap();
        assert!((m.exposures[0] - 4.0).abs() < 0.05, "{:?}", m.exposures);
    }

    #[test]
    fn rejects_mismatched_frames() {
        let a = frame(1.0, (1, 60), (0.0, 0.0), 40, 30);
        let b = frame(1.0, (1, 60), (0.0, 0.0), 30, 40);
        assert!(
            hdr(
                &[a.with_pixels(a.rgb.clone(), 40, 30)],
                &HdrOptions::default()
            )
            .is_err()
        );
        assert!(hdr(&[a, b], &HdrOptions::default()).is_err());
    }
}
