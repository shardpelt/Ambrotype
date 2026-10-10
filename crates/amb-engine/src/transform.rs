// SPDX-License-Identifier: GPL-3.0-or-later
//! Transform: Lightroom's manual Transform sliders (`PerspectiveVertical`,
//! `PerspectiveHorizontal`, `PerspectiveRotate`, `PerspectiveAspect`,
//! `PerspectiveScale`, `PerspectiveX`, `PerspectiveY`), docs/design/transform.md.
//!
//! **Model v1 (ours, not Adobe's).** The sliders build one homography on the
//! lens-corrected frame, in stored orientation, before the crop:
//!
//! 1. Vertical and Horizontal turn a virtual camera about the frame's
//!    horizontal and vertical axes (H = K·R·K⁻¹, K a pinhole camera with a
//!    fixed focal length), then the frame centre is put back in place at its
//!    old local size, so the sliders keystone the photo about its centre;
//! 2. Rotate turns it about the centre;
//! 3. Aspect stretches it vertically (positive) or horizontally (negative),
//!    keeping its area;
//! 4. Scale scales it about the centre;
//! 5. X and Y move it.
//!
//! The renderer needs the inverse: for each output position, where to sample
//! the lens-corrected frame. [`crate::lens::apply_camera_cropped`] composes
//! it into its single resampling pass: output pixel → crop → transformed
//! frame → this map → lens-corrected frame → lens maps → source.
//!
//! Strengths and signs are guesses, kept in [`TransformStrengths`] until
//! measured against Lightroom.

/// Strength and sign constants, fitted later against Lightroom sweeps.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct TransformStrengths {
    /// Virtual camera tilt at Vertical or Horizontal ±100, in degrees.
    pub max_tilt: f64,
    /// Focal length of the virtual camera, as a 35 mm equivalent (the frame's
    /// diagonal stands for 35 mm film's 43.3 mm).
    pub focal_35mm: f64,
    /// Aspect ±100: the ratio of the vertical to the horizontal stretch.
    pub max_aspect: f64,
    /// X or Y ±100: the move as a fraction of the frame's width or height.
    pub max_offset: f64,
    /// Signs: +1 keeps the sense described on each field of [`Transform`].
    pub vertical_sign: f64,
    pub horizontal_sign: f64,
    pub rotate_sign: f64,
    pub offset_y_sign: f64,
}

impl Default for TransformStrengths {
    fn default() -> Self {
        DEFAULT_STRENGTHS
    }
}

pub const DEFAULT_STRENGTHS: TransformStrengths = TransformStrengths {
    max_tilt: 30.0,
    focal_35mm: 35.0,
    max_aspect: 2.0,
    max_offset: 0.5,
    vertical_sign: 1.0,
    horizontal_sign: 1.0,
    rotate_sign: 1.0,
    offset_y_sign: 1.0,
};

/// The Transform panel's sliders, as stored.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform {
    /// `PerspectiveVertical` −100..100: negative widens the top (straightens
    /// verticals that lean together, as in a photo taken looking up).
    pub vertical: f64,
    /// `PerspectiveHorizontal` −100..100: positive makes the right side taller.
    pub horizontal: f64,
    /// `PerspectiveRotate` −10..10 degrees: positive turns the photo
    /// counter-clockwise on screen (like `CropAngle`).
    pub rotate: f64,
    /// `PerspectiveAspect` −100..100: positive stretches vertically.
    pub aspect: f64,
    /// `PerspectiveScale` 50..150 percent.
    pub scale: f64,
    /// `PerspectiveX`, `PerspectiveY` −100..100: positive moves the photo
    /// right and down.
    pub offset_x: f64,
    pub offset_y: f64,
}

impl Default for Transform {
    fn default() -> Self {
        Transform {
            vertical: 0.0,
            horizontal: 0.0,
            rotate: 0.0,
            aspect: 0.0,
            scale: 100.0,
            offset_x: 0.0,
            offset_y: 0.0,
        }
    }
}

impl Transform {
    pub fn is_identity(&self) -> bool {
        *self == Transform::default()
    }

    /// The output → lens-corrected frame map for a `width`×`height` frame,
    /// in pixel coordinates (pixel centres at integers). `None` when the
    /// sliders change nothing or are not finite.
    pub fn sampling_map(
        &self,
        width: usize,
        height: usize,
        k: &TransformStrengths,
    ) -> Option<Homography> {
        if self.is_identity() || width == 0 || height == 0 {
            return None;
        }
        let forward = self.forward(width, height, k)?;
        forward.inverse()
    }

    /// The lens-corrected frame → output map (pixel coordinates).
    pub fn forward(
        &self,
        width: usize,
        height: usize,
        k: &TransformStrengths,
    ) -> Option<Homography> {
        let values = [
            self.vertical,
            self.horizontal,
            self.rotate,
            self.aspect,
            self.scale,
            self.offset_x,
            self.offset_y,
        ];
        if values.iter().any(|v| !v.is_finite()) {
            return None;
        }
        let (w, h) = (width as f64, height as f64);
        // Normalised coordinates: centre at 0, the half diagonal = 1, y down.
        let d = 0.5 * w.hypot(h);
        let (cx, cy) = ((w - 1.0) / 2.0, (h - 1.0) / 2.0);
        let to_norm = Homography([
            [1.0 / d, 0.0, -cx / d],
            [0.0, 1.0 / d, -cy / d],
            [0.0, 0.0, 1.0],
        ]);
        let to_px = Homography([[d, 0.0, cx], [0.0, d, cy], [0.0, 0.0, 1.0]]);

        // 1. Keystone: turn a pinhole camera of focal length f (in half
        // diagonals) by α about the x axis and β about the y axis.
        let f = k.focal_35mm.max(1.0) / (0.5 * 43.267);
        // (A camera turned by a positive α about x, y down, sees the top of
        // the frame wider, so negative Vertical turns it by a positive α.)
        let alpha = -(k.vertical_sign * self.vertical.clamp(-100.0, 100.0) / 100.0 * k.max_tilt)
            .to_radians();
        let beta = (k.horizontal_sign * self.horizontal.clamp(-100.0, 100.0) / 100.0 * k.max_tilt)
            .to_radians();
        let mut m = Homography::IDENTITY;
        if alpha != 0.0 || beta != 0.0 {
            let (sa, ca) = alpha.sin_cos();
            let (sb, cb) = beta.sin_cos();
            let rx = Homography([[1.0, 0.0, 0.0], [0.0, ca, -sa], [0.0, sa, ca]]);
            let ry = Homography([[cb, 0.0, sb], [0.0, 1.0, 0.0], [-sb, 0.0, cb]]);
            let kk = Homography([[f, 0.0, 0.0], [0.0, f, 0.0], [0.0, 0.0, 1.0]]);
            let ki = Homography([[1.0 / f, 0.0, 0.0], [0.0, 1.0 / f, 0.0], [0.0, 0.0, 1.0]]);
            let p = kk.mul(&ry.mul(&rx)).mul(&ki);
            // Put the centre back, at its old local size.
            let (qx, qy) = p.apply(0.0, 0.0);
            let e = 1e-4;
            let (ax, ay) = p.apply(e, 0.0);
            let (bx, by) = p.apply(0.0, e);
            let det = ((ax - qx) * (by - qy) - (ay - qy) * (bx - qx)) / (e * e);
            if !(det.is_finite() && det > 1e-6) {
                return None;
            }
            let s = 1.0 / det.sqrt();
            let back = Homography([[s, 0.0, -s * qx], [0.0, s, -s * qy], [0.0, 0.0, 1.0]]);
            m = back.mul(&p);
        }

        // 2. Rotate: counter-clockwise on screen (y down) for a positive θ.
        let theta = (k.rotate_sign * self.rotate.clamp(-10.0, 10.0)).to_radians();
        let (st, ct) = theta.sin_cos();
        let rot = Homography([[ct, st, 0.0], [-st, ct, 0.0], [0.0, 0.0, 1.0]]);

        // 3. Aspect, keeping the area; 4. Scale.
        let g = k
            .max_aspect
            .max(1.0)
            .powf(self.aspect.clamp(-100.0, 100.0) / 100.0)
            .sqrt();
        let s = self.scale.clamp(50.0, 150.0) / 100.0;
        let stretch = Homography([[s / g, 0.0, 0.0], [0.0, s * g, 0.0], [0.0, 0.0, 1.0]]);

        // 5. Offsets, in fractions of the width and height.
        let ox = self.offset_x.clamp(-100.0, 100.0) / 100.0 * k.max_offset * w / d;
        let oy =
            k.offset_y_sign * self.offset_y.clamp(-100.0, 100.0) / 100.0 * k.max_offset * h / d;
        let shift = Homography([[1.0, 0.0, ox], [0.0, 1.0, oy], [0.0, 0.0, 1.0]]);

        let norm = shift.mul(&stretch).mul(&rot).mul(&m);
        Some(to_px.mul(&norm).mul(&to_norm))
    }
}

/// A plane projective map, row-major, acting on column vectors (x, y, 1).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Homography(pub [[f64; 3]; 3]);

impl Homography {
    pub const IDENTITY: Homography =
        Homography([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);

    /// The image of (`x`, `y`). Points sent to infinity come back as NaN.
    #[inline]
    pub fn apply(&self, x: f64, y: f64) -> (f64, f64) {
        let m = &self.0;
        let wz = m[2][0] * x + m[2][1] * y + m[2][2];
        if wz <= 1e-12 {
            return (f64::NAN, f64::NAN);
        }
        (
            (m[0][0] * x + m[0][1] * y + m[0][2]) / wz,
            (m[1][0] * x + m[1][1] * y + m[1][2]) / wz,
        )
    }

    /// `self` after `other`: (self · other)(p) = self(other(p)).
    pub fn mul(&self, other: &Homography) -> Homography {
        let (a, b) = (&self.0, &other.0);
        let mut out = [[0.0; 3]; 3];
        for (i, row) in out.iter_mut().enumerate() {
            for (j, v) in row.iter_mut().enumerate() {
                *v = (0..3).map(|k| a[i][k] * b[k][j]).sum();
            }
        }
        Homography(out)
    }

    pub fn inverse(&self) -> Option<Homography> {
        let m = &self.0;
        let c = |r0: usize, r1: usize, c0: usize, c1: usize| {
            m[r0][c0] * m[r1][c1] - m[r0][c1] * m[r1][c0]
        };
        let det = m[0][0] * c(1, 2, 1, 2) - m[0][1] * c(1, 2, 0, 2) + m[0][2] * c(1, 2, 0, 1);
        if !det.is_finite() || det.abs() < 1e-15 {
            return None;
        }
        let adj = [
            [c(1, 2, 1, 2), -c(0, 2, 1, 2), c(0, 1, 1, 2)],
            [-c(1, 2, 0, 2), c(0, 2, 0, 2), -c(0, 1, 0, 2)],
            [c(1, 2, 0, 1), -c(0, 2, 0, 1), c(0, 1, 0, 1)],
        ];
        let mut inv = adj.map(|row| row.map(|v| v / det));
        // Keep the last coefficient positive, so `apply` sees w > 0 in front
        // of the camera.
        if inv[2][2] < 0.0 {
            inv = inv.map(|row| row.map(|v| -v));
        }
        Some(Homography(inv))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const K: TransformStrengths = DEFAULT_STRENGTHS;
    const W: usize = 600;
    const H: usize = 400;

    fn t(f: impl FnOnce(&mut Transform)) -> Transform {
        let mut t = Transform::default();
        f(&mut t);
        t
    }

    fn close(a: (f64, f64), b: (f64, f64), tol: f64) -> bool {
        (a.0 - b.0).hypot(a.1 - b.1) < tol
    }

    #[test]
    fn defaults_and_bad_values_do_nothing() {
        assert!(Transform::default().sampling_map(W, H, &K).is_none());
        assert!(
            t(|t| t.vertical = f64::NAN)
                .sampling_map(W, H, &K)
                .is_none()
        );
    }

    #[test]
    fn inverse_undoes_forward() {
        let tr = t(|t| {
            t.vertical = -40.0;
            t.horizontal = 25.0;
            t.rotate = 3.0;
            t.aspect = 30.0;
            t.scale = 90.0;
            t.offset_x = 10.0;
            t.offset_y = -20.0;
        });
        let f = tr.forward(W, H, &K).unwrap();
        let g = tr.sampling_map(W, H, &K).unwrap();
        for p in [(0.0, 0.0), (599.0, 0.0), (300.0, 200.0), (17.0, 388.0)] {
            let (x, y) = f.apply(p.0, p.1);
            assert!(close(g.apply(x, y), p, 1e-6), "{p:?}");
        }
    }

    #[test]
    fn negative_vertical_widens_the_top_about_the_centre() {
        let f = t(|t| t.vertical = -50.0).forward(W, H, &K).unwrap();
        let c = ((W as f64 - 1.0) / 2.0, (H as f64 - 1.0) / 2.0);
        assert!(close(f.apply(c.0, c.1), c, 1e-9));
        let top = f.apply(599.0, 0.0).0 - f.apply(0.0, 0.0).0;
        let bottom = f.apply(599.0, 399.0).0 - f.apply(0.0, 399.0).0;
        assert!(top > 610.0 && bottom < 590.0, "top {top}, bottom {bottom}");
        // A vertical line through the centre stays vertical.
        assert!((f.apply(c.0, 0.0).0 - c.0).abs() < 1e-9);
    }

    #[test]
    fn positive_horizontal_makes_the_right_side_taller() {
        let f = t(|t| t.horizontal = 50.0).forward(W, H, &K).unwrap();
        let left = f.apply(0.0, 399.0).1 - f.apply(0.0, 0.0).1;
        let right = f.apply(599.0, 399.0).1 - f.apply(599.0, 0.0).1;
        assert!(right > 410.0 && left < 390.0, "left {left}, right {right}");
    }

    #[test]
    fn rotate_aspect_scale_and_offsets() {
        let c = ((W as f64 - 1.0) / 2.0, (H as f64 - 1.0) / 2.0);
        // Rotate: a point right of the centre moves up (counter-clockwise).
        let f = t(|t| t.rotate = 10.0).forward(W, H, &K).unwrap();
        assert!(f.apply(c.0 + 100.0, c.1).1 < c.1 - 10.0);
        // Aspect +100: twice as tall as wide, same area.
        let f = t(|t| t.aspect = 100.0).forward(W, H, &K).unwrap();
        let sx = f.apply(c.0 + 1.0, c.1).0 - c.0;
        let sy = f.apply(c.0, c.1 + 1.0).1 - c.1;
        assert!((sy / sx - 2.0).abs() < 1e-9 && (sx * sy - 1.0).abs() < 1e-9);
        // Scale 50: half size about the centre.
        let f = t(|t| t.scale = 50.0).forward(W, H, &K).unwrap();
        assert!(close(
            f.apply(c.0 + 100.0, c.1 + 50.0),
            (c.0 + 50.0, c.1 + 25.0),
            1e-9
        ));
        // X +100 moves right by half the width; Y +100 down by half the height.
        let f = t(|t| {
            t.offset_x = 100.0;
            t.offset_y = 100.0;
        })
        .forward(W, H, &K)
        .unwrap();
        assert!(close(f.apply(c.0, c.1), (c.0 + 300.0, c.1 + 200.0), 1e-9));
    }
}
