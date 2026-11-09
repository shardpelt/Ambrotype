// SPDX-License-Identifier: GPL-3.0-or-later
//! Best in a burst: of photos taken one after another, the one in focus
//! with eyes open.
//!
//! Each photo is looked at once, from its preview (`Look`): how sharp it
//! is where it matters (on the faces, or else in its sharpest part, where
//! the focus was) and, when the face finder is installed
//! (`amb_engine::subject::faces`), how open the eyes are (the face that
//! looks most closed counts, since one blink spoils a group). Then the
//! photos of a burst are compared with each other (`rank`): a burst is the
//! same people a moment apart, so an eye much less open than in the other
//! frames is a blink, where no fixed line would tell.

use serde::Serialize;

/// What one photo looks like, for ranking it among its burst.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Look {
    /// How sharp it is where it matters (the variance of its Laplacian).
    pub sharp: f32,
    /// How open the eyes are (0–1; the most closed face's), when there
    /// are faces big enough to tell.
    pub eyes: Option<f32>,
    /// The faces found.
    pub faces: usize,
}

/// A photo's place in its burst.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Ranked {
    /// Its index in the burst as given.
    pub index: usize,
    /// 0–1, best highest.
    pub score: f32,
    /// What stands out: "sharpest", "eyes closed", "soft", …
    pub note: String,
}

/// Faces with eyes closer than this (pixels of the preview) are too small
/// to tell open from closed.
const SMALLEST_EYES: f32 = 16.0;

/// How `rgb` (an upright preview, 1000 to 2000 pixels across is plenty)
/// looks.
pub fn look(rgb: &image::RgbImage) -> Look {
    let (w, h) = (rgb.width() as usize, rgb.height() as usize);
    let grey: Vec<f32> = rgb
        .pixels()
        .map(|p| {
            (0.2126 * f32::from(p[0]) + 0.7152 * f32::from(p[1]) + 0.0722 * f32::from(p[2])) / 255.0
        })
        .collect();
    let faces = if w > 0 && h > 0 {
        let f: Vec<f32> = rgb.as_raw().iter().map(|v| f32::from(*v) / 255.0).collect();
        amb_engine::subject::faces(&f, w, h).unwrap_or_default()
    } else {
        Vec::new()
    };
    let px = |v: f32, n: usize| (v * n as f32).clamp(0.0, n as f32 - 1.0) as usize;
    let boxes: Vec<(usize, usize, usize, usize)> = faces
        .iter()
        .map(|f| (px(f.x0, w), px(f.y0, h), px(f.x1, w) + 1, px(f.y1, h) + 1))
        .filter(|(x0, y0, x1, y1)| x1 - x0 >= 12 && y1 - y0 >= 12)
        .collect();
    let sharp = if boxes.is_empty() {
        sharpest_part(&grey, w, h)
    } else {
        boxes
            .iter()
            .map(|&b| laplacian_variance(&grey, w, b))
            .sum::<f32>()
            / boxes.len() as f32
    };
    let eyes = faces
        .iter()
        .filter_map(|f| {
            let e = f.eyes.map(|[x, y]| [x * w as f32, y * h as f32]);
            let d = (e[0][0] - e[1][0]).hypot(e[0][1] - e[1][1]);
            if d < SMALLEST_EYES {
                return None;
            }
            // Only eyes whose crop is in the picture (a face at the edge
            // may have one outside).
            let inside = |[x, y]: [f32; 2]| {
                x - 0.28 * d >= 0.0
                    && y - 0.16 * d >= 0.0
                    && x + 0.28 * d < w as f32
                    && y + 0.16 * d < h as f32
            };
            let seen: Vec<f32> = e
                .into_iter()
                .filter(|&p| inside(p))
                .map(|p| openness(rgb, p, d))
                .collect();
            (!seen.is_empty()).then(|| seen.iter().sum::<f32>() / seen.len() as f32)
        })
        .min_by(f32::total_cmp);
    Look {
        sharp,
        eyes,
        faces: faces.len(),
    }
}

/// The burst's photos, best first, each with its score and a note.
pub fn rank(looks: &[Look]) -> Vec<Ranked> {
    let top_sharp = looks
        .iter()
        .map(|l| l.sharp)
        .fold(0.0f32, f32::max)
        .max(1e-9);
    let top_eyes = looks.iter().filter_map(|l| l.eyes).fold(0.0f32, f32::max);
    // Eyes count when most photos have faces to tell by.
    let with_eyes = looks.iter().filter(|l| l.eyes.is_some()).count();
    let use_eyes = top_eyes > 0.0 && with_eyes * 2 >= looks.len();
    let mut out: Vec<Ranked> = looks
        .iter()
        .enumerate()
        .map(|(index, l)| {
            let sharp = l.sharp / top_sharp;
            let eyes = l.eyes.map(|e| e / top_eyes);
            let score = match (use_eyes, eyes) {
                (true, Some(e)) => 0.5 * sharp + 0.5 * e,
                // No face to tell by in this one: as if half closed.
                (true, None) => 0.5 * sharp + 0.25,
                (false, _) => sharp,
            };
            let mut notes = Vec::new();
            if use_eyes && eyes.is_some_and(|e| e < 0.6) {
                notes.push("eyes closed");
            }
            if sharp < 0.5 {
                notes.push("soft");
            }
            Ranked {
                index,
                score,
                note: notes.join(", "),
            }
        })
        .collect();
    out.sort_by(|a, b| b.score.total_cmp(&a.score));
    if let Some(best) = out.first_mut() {
        let sharpest = looks[best.index].sharp >= top_sharp;
        let open = use_eyes && looks[best.index].eyes == Some(top_eyes);
        best.note = match (sharpest, open) {
            (true, true) => "sharpest, eyes open".into(),
            (true, false) if use_eyes => "sharpest, eyes open enough".into(),
            (true, false) => "sharpest".into(),
            (false, true) => "eyes open, sharp enough".into(),
            (false, false) => "the best balance".into(),
        };
    }
    out
}

/// How open an eye at `at` is (0–1), with `d` pixels between the eyes. An
/// open eye is a round iris between whites: across it the brightness
/// changes as much as up and down, and there is white beside it. A closed
/// one is lashes and skin: one dark line, nearly all change up and down.
pub fn openness(rgb: &image::RgbImage, at: [f32; 2], d: f32) -> f32 {
    const W: usize = 56;
    const H: usize = 32;
    let (iw, ih) = (rgb.width() as f32, rgb.height() as f32);
    let (hw, hh) = (0.28 * d, 0.16 * d);
    // The crop round the eye, sampled bilinearly.
    let sample = |u: f32, v: f32| -> [f32; 3] {
        let x = (at[0] - hw + u * 2.0 * hw).clamp(0.0, iw - 1.0);
        let y = (at[1] - hh + v * 2.0 * hh).clamp(0.0, ih - 1.0);
        let (x0, y0) = (x as u32, y as u32);
        let (x1, y1) = (
            (x0 + 1).min(rgb.width() - 1),
            (y0 + 1).min(rgb.height() - 1),
        );
        let (tx, ty) = (x - x0 as f32, y - y0 as f32);
        let p = |x: u32, y: u32| rgb.get_pixel(x, y).0.map(|v| f32::from(v) / 255.0);
        let (a, b, c, e) = (p(x0, y0), p(x1, y0), p(x0, y1), p(x1, y1));
        [0, 1, 2].map(|k| {
            let top = a[k] + (b[k] - a[k]) * tx;
            top + (c[k] + (e[k] - c[k]) * tx - top) * ty
        })
    };
    let crop: Vec<[f32; 3]> = (0..W * H)
        .map(|i| {
            sample(
                ((i % W) as f32 + 0.5) / W as f32,
                ((i / W) as f32 + 0.5) / H as f32,
            )
        })
        .collect();
    let lum: Vec<f32> = crop.iter().map(|c| (c[0] + c[1] + c[2]) / 3.0).collect();
    // Change across and up-and-down in the middle of the crop.
    let (mut gx, mut gy) = (0.0f32, 0.0f32);
    for y in 8..24 {
        for x in 8..47 {
            gx += (lum[y * W + x + 1] - lum[y * W + x]).abs();
            gy += (lum[(y + 1) * W + x] - lum[y * W + x]).abs();
        }
    }
    let ratio = gx / gy.max(1e-4);
    // Whites: pale, nearly grey, brighter than the crop's middle tone.
    let mut sorted = lum.clone();
    sorted.sort_by(f32::total_cmp);
    let mid = sorted[sorted.len() / 2];
    let (mut white, mut n) = (0usize, 0usize);
    for y in 6..26 {
        for x in 4..52 {
            let c = crop[y * W + x];
            let (hi, lo) = (c[0].max(c[1]).max(c[2]), c[0].min(c[1]).min(c[2]));
            if (hi - lo) / hi.max(1e-3) < 0.25 && lum[y * W + x] > mid + 0.08 {
                white += 1;
            }
            n += 1;
        }
    }
    let white = white as f32 / n as f32;
    (ratio.min(2.0) / 2.0 * 0.7 + (white * 4.0).min(1.0) * 0.3).clamp(0.0, 1.0)
}

/// The variance of the Laplacian inside `area` (x0, y0, x1, y1).
fn laplacian_variance(grey: &[f32], w: usize, area: (usize, usize, usize, usize)) -> f32 {
    let (x0, y0, x1, y1) = area;
    let h = grey.len() / w.max(1);
    let (mut sum, mut sq, mut n) = (0.0f64, 0.0f64, 0usize);
    for y in y0.max(1)..y1.min(h - 1) {
        for x in x0.max(1)..x1.min(w - 1) {
            let i = y * w + x;
            let v =
                f64::from(4.0 * grey[i] - grey[i - 1] - grey[i + 1] - grey[i - w] - grey[i + w]);
            sum += v;
            sq += v * v;
            n += 1;
        }
    }
    if n == 0 {
        return 0.0;
    }
    let m = sum / n as f64;
    (sq / n as f64 - m * m) as f32
}

/// The sharpness of the sharpest part of the picture: the mean of its
/// three sharpest tiles (of 6 × 4), where the focus was.
fn sharpest_part(grey: &[f32], w: usize, h: usize) -> f32 {
    if w < 8 || h < 8 {
        return 0.0;
    }
    let mut tiles: Vec<f32> = (0..24)
        .map(|k| {
            let (tx, ty) = (k % 6, k / 6);
            laplacian_variance(
                grey,
                w,
                (tx * w / 6, ty * h / 4, (tx + 1) * w / 6, (ty + 1) * h / 4),
            )
        })
        .collect();
    tiles.sort_by(|a, b| b.total_cmp(a));
    tiles[..3].iter().sum::<f32>() / 3.0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An eye: skin, and either whites with a round dark iris or a dark
    /// line of lashes.
    fn eye(open: bool) -> image::RgbImage {
        image::RgbImage::from_fn(200, 120, |x, y| {
            let (dx, dy) = (x as f32 - 100.0, y as f32 - 60.0);
            let skin = image::Rgb([200, 150, 120]);
            if open {
                if dx.hypot(dy) < 14.0 {
                    image::Rgb([40, 30, 25])
                } else if (dx / 30.0).hypot(dy / 14.0) < 1.0 {
                    image::Rgb([235, 232, 228])
                } else {
                    skin
                }
            } else if dy.abs() < 2.5 && dx.abs() < 30.0 {
                image::Rgb([40, 30, 25])
            } else {
                skin
            }
        })
    }

    #[test]
    fn an_open_eye_is_more_open_than_a_closed_one() {
        // d: the eyes 100 pixels apart, so the crop is 56 × 32 pixels round it.
        let (open, closed) = (
            openness(&eye(true), [100.0, 60.0], 100.0),
            openness(&eye(false), [100.0, 60.0], 100.0),
        );
        assert!(open > closed * 1.6, "open {open}, closed {closed}");
    }

    #[test]
    fn the_sharpest_frame_ranks_first_and_a_blink_is_noted() {
        let looks = [
            Look {
                sharp: 0.4,
                eyes: Some(0.8),
                faces: 1,
            },
            Look {
                sharp: 1.0,
                eyes: Some(0.3),
                faces: 1,
            },
            Look {
                sharp: 0.9,
                eyes: Some(0.75),
                faces: 1,
            },
        ];
        let r = rank(&looks);
        assert_eq!(r[0].index, 2, "{r:?}");
        assert!(
            r.iter()
                .find(|x| x.index == 1)
                .unwrap()
                .note
                .contains("eyes closed")
        );
        assert!(
            r.iter()
                .find(|x| x.index == 0)
                .unwrap()
                .note
                .contains("soft")
        );
        // Without faces, sharpness alone.
        let plain = [
            Look {
                sharp: 0.2,
                eyes: None,
                faces: 0,
            },
            Look {
                sharp: 0.9,
                eyes: None,
                faces: 0,
            },
        ];
        let r = rank(&plain);
        assert_eq!((r[0].index, r[0].note.as_str()), (1, "sharpest"));
    }

    #[test]
    fn a_blurred_copy_is_less_sharp() {
        let sharp = image::RgbImage::from_fn(240, 160, |x, y| {
            let v = if (x / 6 + y / 6) % 2 == 0 { 220 } else { 30 };
            image::Rgb([v, v, v])
        });
        let soft = image::imageops::blur(&sharp, 3.0);
        assert!(look(&sharp).sharp > look(&soft).sharp * 3.0);
    }
}
