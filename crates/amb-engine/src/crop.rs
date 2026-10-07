//! Crop and straighten: Camera Raw's `HasCrop`, `CropLeft`, `CropTop`,
//! `CropRight`, `CropBottom` and `CropAngle` (docs/design/crop.md).
//!
//! **Coordinate frame.** The edges are fractions (0..1) of the image *after*
//! the lens corrections, in the photo's stored (sensor) orientation: EXIF
//! orientation is applied later, at output. This is how darktable's Lightroom
//! importer (`src/develop/lightroom.c`) reads them.
//!
//! **Angle.** (`CropLeft`, `CropTop`) and (`CropRight`, `CropBottom`) are the
//! top-left and bottom-right corners of the crop rectangle in the unrotated
//! image. With a `CropAngle` the rectangle is rotated: these stay two opposite
//! corners, and the rectangle's sides follow the rotated axes. The output is
//! that rectangle, straightened, at the source's pixel scale, so its size is
//! the rectangle's size in pixels. darktable's importer reads a positive
//! `CropAngle` as a counter-clockwise rotation of the photo (in stored
//! orientation); we follow it. [`ANGLE_SIGN`] is the one place to flip if
//! Lightroom sweeps say otherwise.
//!
//! The crop is not resampled on its own: [`crate::lens::apply_camera_cropped`]
//! folds it into the lens-correction resampling pass (Catmull-Rom), so a
//! cropped and lens-corrected photo is interpolated once. Parts of the crop
//! that fall outside the image become blank areas, like the lens pass's.

/// Sign of the photo's rotation for a positive `CropAngle`: `+1` turns the
/// photo counter-clockwise as seen on screen (darktable's reading), `−1`
/// clockwise. Unverified against Lightroom renders.
pub const ANGLE_SIGN: f64 = 1.0;

/// Largest straighten angle Lightroom offers, in degrees.
pub const MAX_ANGLE: f64 = 45.0;

/// The crop settings, as stored.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Crop {
    /// `HasCrop`: the crop is active.
    pub enabled: bool,
    /// Top-left and bottom-right corners as fractions of the image width/height.
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    /// `CropAngle`, degrees (−45..45).
    pub angle: f64,
}

impl Default for Crop {
    fn default() -> Self {
        Crop {
            enabled: false,
            left: 0.0,
            top: 0.0,
            right: 1.0,
            bottom: 1.0,
            angle: 0.0,
        }
    }
}

impl Crop {
    /// The resampling geometry for an image of `width`×`height` (the frame the
    /// coordinates refer to). `None` when the crop is off, degenerate or the
    /// whole unrotated frame.
    pub fn geometry(&self, width: usize, height: usize) -> Option<CropGeometry> {
        let values = [self.left, self.top, self.right, self.bottom, self.angle];
        if !self.enabled || width == 0 || height == 0 || values.iter().any(|v| !v.is_finite()) {
            return None;
        }
        let (w, h) = (width as f64, height as f64);
        let theta = (ANGLE_SIGN * self.angle.clamp(-MAX_ANGLE, MAX_ANGLE)).to_radians();
        let (sin, cos) = theta.sin_cos();
        // The crop's x and y axes in frame pixels (y down): the photo turns by
        // θ counter-clockwise, so the crop frame turns by θ clockwise over it.
        let u = (cos, sin);
        let v = (-sin, cos);
        let tl = (self.left * w, self.top * h);
        let br = (self.right * w, self.bottom * h);
        let d = (br.0 - tl.0, br.1 - tl.1);
        let crop_w = d.0 * u.0 + d.1 * u.1;
        let crop_h = d.0 * v.0 + d.1 * v.1;
        if !(crop_w >= 0.5 && crop_h >= 0.5) {
            return None;
        }
        let (out_w, out_h) = (crop_w.round().max(1.0), crop_h.round().max(1.0));
        // Stretch by < half a pixel so the output pixels tile the crop exactly.
        let (sx, sy) = (crop_w / out_w, crop_h / out_h);
        let step_x = (u.0 * sx, u.1 * sx);
        let step_y = (v.0 * sy, v.1 * sy);
        // Output pixel centres sit at (i + ½, j + ½) in crop units; frame pixel
        // centres at integers (edges at −½).
        let origin = (
            tl.0 + 0.5 * (step_x.0 + step_y.0) - 0.5,
            tl.1 + 0.5 * (step_x.1 + step_y.1) - 0.5,
        );
        let g = CropGeometry {
            width: out_w as usize,
            height: out_h as usize,
            origin,
            step_x,
            step_y,
        };
        (!g.is_identity(width, height)).then_some(g)
    }
}

/// Output pixel → frame position map of a crop: an affine map (rotation and
/// a sub-pixel stretch) from output pixel indices to the uncropped frame's
/// pixel coordinates (pixel centres at integers).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CropGeometry {
    /// Output size in pixels.
    pub width: usize,
    pub height: usize,
    origin: (f64, f64),
    step_x: (f64, f64),
    step_y: (f64, f64),
}

impl CropGeometry {
    /// Frame position (pixel centres at integers) of output pixel (`x`, `y`).
    #[inline]
    pub fn source(&self, x: f64, y: f64) -> (f64, f64) {
        (
            self.origin.0 + x * self.step_x.0 + y * self.step_y.0,
            self.origin.1 + x * self.step_x.1 + y * self.step_y.1,
        )
    }

    /// The crop rectangle's corners in frame coordinates with pixel *edges*
    /// at integers (the frame spans 0..width × 0..height): top-left,
    /// top-right, bottom-right, bottom-left.
    pub fn corners(&self) -> [(f64, f64); 4] {
        let (w, h) = (self.width as f64, self.height as f64);
        // Output edges are at −½ and w − ½ in output pixel coordinates.
        [
            (-0.5, -0.5),
            (w - 0.5, -0.5),
            (w - 0.5, h - 0.5),
            (-0.5, h - 0.5),
        ]
        .map(|(x, y)| {
            let (sx, sy) = self.source(x, y);
            (sx + 0.5, sy + 0.5)
        })
    }

    fn is_identity(&self, width: usize, height: usize) -> bool {
        let near = |a: f64, b: f64| (a - b).abs() < 1e-9;
        self.width == width
            && self.height == height
            && near(self.origin.0, 0.0)
            && near(self.origin.1, 0.0)
            && near(self.step_x.0, 1.0)
            && near(self.step_x.1, 0.0)
            && near(self.step_y.0, 0.0)
            && near(self.step_y.1, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn crop(left: f64, top: f64, right: f64, bottom: f64, angle: f64) -> Crop {
        Crop {
            enabled: true,
            left,
            top,
            right,
            bottom,
            angle,
        }
    }

    #[test]
    fn off_full_frame_and_degenerate_crops_do_nothing() {
        assert!(Crop::default().geometry(300, 200).is_none());
        let full = Crop {
            enabled: true,
            ..Default::default()
        };
        assert!(full.geometry(300, 200).is_none());
        assert!(crop(0.6, 0.0, 0.4, 1.0, 0.0).geometry(300, 200).is_none());
        assert!(crop(0.2, 0.2, 0.2, 0.8, 0.0).geometry(300, 200).is_none());
        assert!(
            crop(0.0, 0.0, f64::NAN, 1.0, 0.0)
                .geometry(300, 200)
                .is_none()
        );
        // Disabled: the edges are ignored.
        let off = Crop {
            enabled: false,
            ..crop(0.1, 0.1, 0.5, 0.5, 3.0)
        };
        assert!(off.geometry(300, 200).is_none());
    }

    #[test]
    fn axis_aligned_crop_maps_onto_whole_pixels() {
        let g = crop(0.25, 0.1, 0.75, 0.6, 0.0)
            .geometry(400, 200)
            .expect("a crop");
        assert_eq!((g.width, g.height), (200, 100));
        assert_eq!(g.source(0.0, 0.0), (100.0, 20.0));
        assert_eq!(g.source(199.0, 99.0), (299.0, 119.0));
        let c = g.corners();
        assert_eq!(c[0], (100.0, 20.0));
        assert_eq!(c[2], (300.0, 120.0));
    }

    #[test]
    fn rotated_crop_keeps_the_stored_corners_and_turns_its_axes() {
        let (w, h) = (600, 400);
        let angle: f64 = 10.0;
        let (sin, cos) = (ANGLE_SIGN * angle).to_radians().sin_cos();
        // A crop rectangle of 300×150 frame pixels centred in the frame, its
        // axes turned so that the photo turns by `angle` (sign per ANGLE_SIGN).
        let (cw, ch) = (300.0, 150.0);
        let (cx, cy) = (300.0, 200.0);
        let tl = (
            cx - 0.5 * cw * cos + 0.5 * ch * sin,
            cy - 0.5 * cw * sin - 0.5 * ch * cos,
        );
        let br = (2.0 * cx - tl.0, 2.0 * cy - tl.1);
        let c = crop(
            tl.0 / w as f64,
            tl.1 / h as f64,
            br.0 / w as f64,
            br.1 / h as f64,
            angle,
        );
        let g = c.geometry(w, h).expect("a crop");
        assert_eq!((g.width, g.height), (300, 150));
        let corners = g.corners();
        let close = |a: (f64, f64), b: (f64, f64)| (a.0 - b.0).hypot(a.1 - b.1) < 1e-9;
        assert!(close(corners[0], tl), "{:?} vs {tl:?}", corners[0]);
        assert!(close(corners[2], br), "{:?} vs {br:?}", corners[2]);
        // Every corner lies inside the frame.
        for (x, y) in corners {
            assert!((0.0..=w as f64).contains(&x) && (0.0..=h as f64).contains(&y));
        }
        // Moving right along an output row moves along the turned x axis.
        let (a, b) = (g.source(0.0, 0.0), g.source(100.0, 0.0));
        assert!((b.0 - a.0 - 100.0 * cos).abs() < 1e-9);
        assert!((b.1 - a.1 - 100.0 * sin).abs() < 1e-9);
        // With ANGLE_SIGN = +1 (counter-clockwise photo rotation), a positive
        // angle makes an output row run downhill in the frame: the photo's
        // horizon that ran downhill comes out level.
        if ANGLE_SIGN > 0.0 {
            assert!(b.1 > a.1);
        }
        // The centre of the output is the centre of the crop.
        let mid = g.source(149.5, 74.5);
        assert!(close((mid.0 + 0.5, mid.1 + 0.5), (cx, cy)), "{mid:?}");
    }

    #[test]
    fn angle_is_clamped_to_lightroom_range() {
        let c = crop(0.4, 0.2, 0.6, 0.8, 80.0 * ANGLE_SIGN);
        let a = c.geometry(1000, 1000);
        let b = Crop {
            angle: MAX_ANGLE * ANGLE_SIGN,
            ..c
        }
        .geometry(1000, 1000);
        assert!(a.is_some());
        assert_eq!(a, b);
    }
}
