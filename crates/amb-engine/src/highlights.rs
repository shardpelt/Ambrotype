// SPDX-License-Identifier: GPL-3.0-or-later
//! Highlight reconstruction: where one or two of a pixel's channels clipped,
//! rebuild them from the channels that still hold detail.
//!
//! The idea is darktable's "inpaint opposed" (written here from its
//! description, not its code): on white-balanced values, a clipped channel
//! is replaced by the mean of the *other* two channels around the pixel,
//! times a colour correction: each channel's ratio to that mean, on
//! average, in the unclipped ring just outside the clipped areas (darktable
//! adds a difference; a ratio keeps the hue as the light gets brighter). So a
//! blown yellow flower or a sky gets back colour like its surroundings,
//! instead of a flat white or magenta patch. The rebuilt value is never
//! below the clipped one. Pixels with all three channels clipped hold no
//! colour to rebuild from and are left to [`crate::pipeline`]'s neutral
//! roll-off.
//!
//! Means are taken over 3×3 neighbours on cube roots (cubed back), so a
//! single bright pixel doesn't dominate.

use rayon::prelude::*;

/// A channel counts as clipped from this share of its clip level.
const AT: f32 = 0.97;
/// Pixels this far from a clipped one (and not clipped) form the ring the
/// colour correction is measured on.
const RING: usize = 2;

/// Rebuild the partly clipped pixels of camera RGB `rgb` (`w × h`,
/// interleaved), whose channels clip at `clip` (infinite: never), at the
/// as-shot white `neutral` (camera RGB of white, green 1). Returns how many
/// pixels were rebuilt.
pub fn reconstruct(
    rgb: &mut [f32],
    w: usize,
    h: usize,
    clip: [f32; 3],
    neutral: [f64; 3],
) -> usize {
    if w < 3 || h < 3 || clip.iter().all(|c| !c.is_finite()) {
        return 0;
    }
    let gain: [f32; 3] = std::array::from_fn(|c| {
        if neutral[c] > 1e-9 {
            (1.0 / neutral[c]) as f32
        } else {
            1.0
        }
    });
    let at: [f32; 3] = std::array::from_fn(|c| AT * clip[c]);
    // Which channels clipped, per pixel (bits 0..2).
    let mask: Vec<u8> = rgb
        .par_chunks_exact(3)
        .map(|p| (0..3).fold(0u8, |m, c| m | (u8::from(p[c] >= at[c]) << c)))
        .collect();
    if !mask.iter().any(|&m| m != 0 && m != 7) {
        return 0;
    }
    let src: &[f32] = rgb;
    // The mean of the two channels other than `c` around (x, y), balanced.
    let opposed = |x: usize, y: usize, c: usize| -> f32 {
        let (o1, o2) = ((c + 1) % 3, (c + 2) % 3);
        let mut sum = 0.0f32;
        let mut n = 0.0f32;
        for yy in y.saturating_sub(1)..(y + 2).min(h) {
            for xx in x.saturating_sub(1)..(x + 2).min(w) {
                let p = &src[(yy * w + xx) * 3..][..3];
                sum += (p[o1] * gain[o1]).max(0.0).cbrt() + (p[o2] * gain[o2]).max(0.0).cbrt();
                n += 2.0;
            }
        }
        (sum / n).powi(3)
    };
    // The colour correction, from the unclipped ring around clipped areas.
    let near_clipped = |x: usize, y: usize| {
        (y.saturating_sub(RING)..(y + RING + 1).min(h)).any(|yy| {
            (x.saturating_sub(RING)..(x + RING + 1).min(w)).any(|xx| mask[yy * w + xx] != 0)
        })
    };
    let (sum, n) = (0..h)
        .into_par_iter()
        .map(|y| {
            let mut sum = [0.0f64; 3];
            let mut n = 0usize;
            for x in 0..w {
                if mask[y * w + x] != 0 || !near_clipped(x, y) {
                    continue;
                }
                let p = &src[(y * w + x) * 3..][..3];
                // Dark pixels say little about colour.
                let o: [f32; 3] = std::array::from_fn(|c| opposed(x, y, c));
                if o.iter().any(|v| *v < 0.05) {
                    continue;
                }
                for c in 0..3 {
                    sum[c] += f64::from((p[c] * gain[c] / o[c]).ln());
                }
                n += 1;
            }
            (sum, n)
        })
        .reduce(
            || ([0.0; 3], 0),
            |a, b| {
                (
                    [a.0[0] + b.0[0], a.0[1] + b.0[1], a.0[2] + b.0[2]],
                    a.1 + b.1,
                )
            },
        );
    // The typical (geometric mean) ratio, within reason; too few ring pixels
    // to trust: the opposed mean alone.
    let chroma: [f32; 3] = std::array::from_fn(|c| {
        if n >= 16 {
            ((sum[c] / n as f64).exp() as f32).clamp(0.25, 4.0)
        } else {
            1.0
        }
    });
    let rebuilt: Vec<(usize, [f32; 3])> = (0..h)
        .into_par_iter()
        .flat_map_iter(|y| {
            let mask = &mask;
            let opposed = &opposed;
            (0..w).filter_map(move |x| {
                let i = y * w + x;
                let m = mask[i];
                if m == 0 || m == 7 {
                    return None;
                }
                let p = &src[i * 3..][..3];
                let mut out = [p[0], p[1], p[2]];
                for c in (0..3).filter(|c| m & (1 << c) != 0) {
                    let v = opposed(x, y, c) * chroma[c] / gain[c];
                    out[c] = out[c].max(v);
                }
                Some((i, out))
            })
        })
        .collect();
    for (i, v) in &rebuilt {
        rgb[i * 3..i * 3 + 3].copy_from_slice(v);
    }
    rebuilt.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A smooth orange-ish gradient whose red clips in a bright band.
    fn scene(w: usize, h: usize) -> (Vec<f32>, Vec<f32>) {
        let truth: Vec<f32> = (0..w * h)
            .flat_map(|i| {
                let x = (i % w) as f32 / w as f32;
                let level = 0.4 + 0.9 * x; // brighter to the right
                [level * 1.0, level * 0.6, level * 0.3]
            })
            .collect();
        let clipped = truth.iter().map(|v| v.min(1.0)).collect();
        (truth, clipped)
    }

    #[test]
    fn a_clipped_channel_comes_back_close_to_the_truth() {
        let (w, h) = (80, 40);
        let (truth, mut img) = scene(w, h);
        let n = reconstruct(&mut img, w, h, [1.0; 3], [1.0; 3]);
        assert!(n > 0);
        // Where red clipped, the rebuilt red is above the clip and near the
        // truth (the hue around is kept).
        let mut err = 0.0;
        let mut count = 0;
        for i in 0..w * h {
            if truth[i * 3] > 1.02 {
                assert!(img[i * 3] >= 1.0);
                err += (img[i * 3] - truth[i * 3]).abs() / truth[i * 3];
                count += 1;
            }
            // Unclipped channels are untouched.
            assert_eq!(img[i * 3 + 1], truth[i * 3 + 1].min(1.0));
        }
        assert!(count > 0);
        assert!(
            err / count as f32 <= 0.02,
            "mean relative error {}",
            err / count as f32
        );
    }

    #[test]
    fn nothing_clipped_or_all_clipped_is_left_alone() {
        let (w, h) = (20, 10);
        let mut img = vec![0.5f32; w * h * 3];
        assert_eq!(reconstruct(&mut img, w, h, [1.0; 3], [1.0; 3]), 0);
        let mut white = vec![1.0f32; w * h * 3];
        assert_eq!(reconstruct(&mut white, w, h, [1.0; 3], [1.0; 3]), 0);
        assert!(white.iter().all(|v| *v == 1.0));
        // No clip level known: nothing to do.
        assert_eq!(reconstruct(&mut img, w, h, [f32::INFINITY; 3], [1.0; 3]), 0);
    }
}
