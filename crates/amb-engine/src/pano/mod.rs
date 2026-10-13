// SPDX-License-Identifier: GPL-3.0-or-later
//! Photo Merge > Panorama: overlapping frames of a camera turned about its
//! lens, stitched into one image (Brown & Lowe, 2007, "Automatic Panoramic
//! Image Stitching using Invariant Features").
//!
//! [`align`](align::align) finds each frame's rotation and the focal
//! length; [`render`](render::render) projects the frames (spherical,
//! cylindrical or perspective), evens out their brightness and blends them
//! in two bands: the smooth part averaged with weights falling to the frame
//! edges, the detail taken from the frame whose centre is nearest, so seams
//! neither ghost nor show a step.

pub mod align;
pub mod features;
pub mod linalg;
pub mod render;

pub use align::{Aligned, align, exif_focal};
pub use render::{PanoOptions, Panorama, Projection, render};

use crate::raw::CameraImage;

/// Align and render `frames` (upright, one camera and lens).
pub fn stitch(frames: &[CameraImage], opts: &PanoOptions) -> Result<Panorama, crate::Error> {
    if frames.iter().any(|f| f.rendered) {
        return Err(crate::Error::Unsupported(
            "panorama merge needs raw photos (JPEGs and other rendered images can't be merged yet)"
                .into(),
        ));
    }
    let hint = frames.iter().find_map(exif_focal);
    let aligned = align(frames, hint)?;
    render(frames, &aligned, opts)
}

#[doc(hidden)]
pub mod testing {
    //! A synthetic scene on a sphere and a camera that photographs it (for
    //! tests here and in other crates).
    use super::linalg::{self, M3};
    use crate::raw::{CameraImage, PhotoMetadata};

    /// Brightness of the scene in direction `w`: a soft gradient with
    /// rectangles of every size and brightness (corners to find).
    pub fn scene(w: [f64; 3]) -> f32 {
        let theta = w[0].atan2(w[2]);
        let phi = (w[1] / linalg::norm(w)).asin();
        let mut v = 0.15 + 0.1 * (theta * 1.3).sin() + 0.05 * (phi * 2.0).cos();
        let mut seed = 0x9e37_79b9_7f4a_7c15u64;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed >> 11) as f64 / (1u64 << 53) as f64
        };
        for _ in 0..700 {
            let (t0, p0) = (next() * 6.2 - 3.1, next() * 1.6 - 0.8);
            let (dt, dp) = (0.01 + next() * 0.08, 0.01 + next() * 0.06);
            let level = next() * 0.5 - 0.2;
            if (theta - t0).abs() < dt && (phi - p0).abs() < dp {
                v += level;
            }
        }
        v.clamp(0.02, 0.95) as f32
    }

    /// A `w × h` frame of the scene seen with world → camera rotation `r`
    /// and focal length `f` (pixels), 2×2 supersampled.
    pub fn photograph(r: &M3, f: f64, w: usize, h: usize) -> CameraImage {
        let (cx, cy) = ((w as f64 - 1.0) / 2.0, (h as f64 - 1.0) / 2.0);
        let rt = linalg::transpose(r);
        let mut rgb = Vec::with_capacity(w * h * 3);
        for y in 0..h {
            for x in 0..w {
                let mut acc = 0.0;
                for (sx, sy) in [(-0.25, -0.25), (0.25, -0.25), (-0.25, 0.25), (0.25, 0.25)] {
                    let c = [x as f64 + sx - cx, y as f64 + sy - cy, f];
                    acc += scene(linalg::apply(&rt, c));
                }
                let v = acc / 4.0;
                rgb.extend_from_slice(&[v * 0.9, v, v * 1.05]);
            }
        }
        CameraImage {
            width: w,
            height: h,
            rgb,
            xyz_to_cam: linalg::IDENTITY,
            matrices: Vec::new(),
            as_shot_neutral: [1.0; 3],
            clip: [f32::INFINITY; 3],
            rendered: false,
            make: "Test".into(),
            model: "Pano".into(),
            shot: Default::default(),
            metadata: PhotoMetadata::default(),
        }
    }

    /// World → camera rotation for a camera turned by `yaw` (right), `pitch`
    /// (up) and `roll`, in degrees.
    pub fn turn(yaw: f64, pitch: f64, roll: f64) -> M3 {
        let r = |w: [f64; 3]| linalg::rodrigues(w.map(f64::to_radians));
        // Camera to world: yaw about the vertical, then pitch, then roll.
        let c2w = linalg::mul(
            &linalg::mul(&r([0.0, yaw, 0.0]), &r([-pitch, 0.0, 0.0])),
            &r([0.0, 0.0, roll]),
        );
        linalg::transpose(&c2w)
    }
}

#[cfg(test)]
mod tests {
    use super::linalg;
    use super::testing::{photograph, turn};
    use super::*;

    /// Angle (degrees) of the rotation between two relative rotations.
    fn angle_between(a: &linalg::M3, b: &linalg::M3) -> f64 {
        let d = linalg::mul(a, &linalg::transpose(b));
        let tr = d[0][0] + d[1][1] + d[2][2];
        ((tr - 1.0) / 2.0).clamp(-1.0, 1.0).acos().to_degrees()
    }

    #[test]
    fn aligns_a_three_frame_sweep() {
        let f = 560.0;
        let truth = [
            turn(-38.0, 2.0, 1.0),
            turn(0.0, 0.0, 0.0),
            turn(37.0, -1.5, -1.0),
        ];
        let frames: Vec<_> = truth.iter().map(|r| photograph(r, f, 640, 427)).collect();
        // A hint 15% off: alignment finds the focal length itself.
        let a = align(&frames, Some(f * 1.15)).unwrap();
        // Focal length and rotations trade off against each other; with
        // about 50 matches per overlap at 1 px, these are what the frames
        // pin down (what stitching needs, points landing on each other, is
        // the rms below).
        assert!((a.focal / f - 1.0).abs() < 0.015, "focal {}", a.focal);
        assert_eq!(a.reference, 1, "the middle frame overlaps both others");
        assert!(a.rms < 1.5, "rms {}", a.rms);
        assert_eq!(a.links.len(), 2, "{:?}", a.links);
        for (i, j) in [(0, 1), (1, 2), (0, 2)] {
            let found = linalg::mul(&a.rotations[j], &linalg::transpose(&a.rotations[i]));
            let real = linalg::mul(&truth[j], &linalg::transpose(&truth[i]));
            let err = angle_between(&found, &real);
            // A focal length off by ε scales the angles between frames by
            // about ε: allow the same share as the focal check above.
            let turn = angle_between(&real, &linalg::IDENTITY);
            assert!(err < 0.015 * turn + 0.05, "{i}-{j}: {err}° of {turn}°");
        }
        // Levelled: every frame's horizontal axis is (nearly) level.
        for r in &a.rotations {
            assert!(r[0][1].abs() < 0.04, "camera x axis tilts: {:?}", r[0]);
        }
    }

    #[test]
    fn refuses_frames_that_do_not_overlap() {
        let f = 560.0;
        let frames = [
            photograph(&turn(0.0, 0.0, 0.0), f, 480, 320),
            photograph(&turn(120.0, 0.0, 0.0), f, 480, 320),
        ];
        let err = align(&frames, Some(f)).unwrap_err().to_string();
        assert!(err.contains("doesn't overlap"), "{err}");
    }
}
