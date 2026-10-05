//! Evaluating a [`Correction`] on a concrete image.
//!
//! Pixel coordinates have pixel centres at integers; the image centre is
//! ((w − 1)/2, (h − 1)/2). Physical scale comes from the crop factor: a sensor
//! with crop factor c has a diagonal of 43.27 mm / c. When the camera's crop
//! factor is unknown we assume the profile's calibration sensor.

use crate::model::{Correction, Distortion, Frame, Tca, Vignetting};

/// Diagonal of a 36 × 24 mm sensor.
const FULL_FRAME_DIAGONAL_MM: f64 = 43.266_615_305_567_87;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImageFrame {
    pub width: usize,
    pub height: usize,
    /// Crop factor of the camera that took the image, when known.
    pub crop_factor: Option<f64>,
}

/// A model frame mapped onto image pixels: normalised = (pixel − centre) / unit.
#[derive(Debug, Clone, Copy)]
struct PixelFrame {
    cx: f64,
    cy: f64,
    ux: f64,
    uy: f64,
}

impl PixelFrame {
    fn new(frame: &Frame, img: &ImageFrame) -> PixelFrame {
        let (w, h) = (img.width.max(1) as f64, img.height.max(1) as f64);
        let (gcx, gcy) = ((w - 1.0) / 2.0, (h - 1.0) / 2.0);
        let cal_crop = match *frame {
            Frame::HuginShortSide { cal_crop, .. }
            | Frame::HalfDiagonal { cal_crop }
            | Frame::Acm { cal_crop, .. } => cal_crop,
        };
        let cal_crop = if cal_crop > 0.0 { cal_crop } else { 1.0 };
        let img_crop = img
            .crop_factor
            .filter(|c| c.is_finite() && *c > 0.0)
            .unwrap_or(cal_crop);
        // Millimetres per pixel on this image's sensor.
        let pitch = FULL_FRAME_DIAGONAL_MM / img_crop / w.hypot(h);
        match *frame {
            Frame::HuginShortSide { cal_aspect, .. } => {
                let aspect = if cal_aspect > 0.0 { cal_aspect } else { 1.5 };
                let unit_mm = FULL_FRAME_DIAGONAL_MM / cal_crop / aspect.hypot(1.0) / 2.0;
                let u = unit_mm / pitch;
                PixelFrame {
                    cx: gcx,
                    cy: gcy,
                    ux: u,
                    uy: u,
                }
            }
            Frame::HalfDiagonal { .. } => {
                let u = FULL_FRAME_DIAGONAL_MM / cal_crop / 2.0 / pitch;
                PixelFrame {
                    cx: gcx,
                    cy: gcy,
                    ux: u,
                    uy: u,
                }
            }
            Frame::Acm { cx, cy, fx, fy, .. } => {
                // Profile values are fractions of the calibration image's longest
                // side. A smaller sensor (larger crop) sees the same focal length
                // as more pixels per millimetre of field.
                let dmax = w.max(h);
                let ratio = img_crop / cal_crop;
                let same_sensor = (ratio - 1.0).abs() < 0.03;
                let (ccx, ccy) = if same_sensor {
                    (cx * dmax - 0.5, cy * dmax - 0.5)
                } else {
                    (gcx, gcy)
                };
                PixelFrame {
                    cx: ccx,
                    cy: ccy,
                    ux: fx * dmax * ratio,
                    uy: fy * dmax * ratio,
                }
            }
        }
    }

    fn to_norm(self, x: f64, y: f64) -> (f64, f64) {
        ((x - self.cx) / self.ux, (y - self.cy) / self.uy)
    }

    fn to_px(self, x: f64, y: f64) -> (f64, f64) {
        (self.cx + x * self.ux, self.cy + y * self.uy)
    }
}

/// A correction bound to one image size: maps output pixels to source pixels.
#[derive(Debug, Clone)]
pub struct Geometry {
    distortion: Option<(Distortion, PixelFrame, f64)>,
    tca: Option<(Tca, PixelFrame)>,
    vignetting: Option<(Vignetting, PixelFrame)>,
}

impl Geometry {
    pub fn new(c: &Correction, img: &ImageFrame) -> Geometry {
        Geometry {
            distortion: c.distortion.map(|(d, f)| {
                let s = d.centre_scale();
                let s = if s.abs() > 1e-6 { s } else { 1.0 };
                (d, PixelFrame::new(&f, img), s)
            }),
            tca: c.tca.map(|(t, f)| (t, PixelFrame::new(&f, img))),
            vignetting: c.vignetting.map(|(v, f)| (v, PixelFrame::new(&f, img))),
        }
    }

    pub fn has_distortion(&self) -> bool {
        self.distortion.is_some()
    }

    pub fn has_tca(&self) -> bool {
        self.tca.is_some()
    }

    pub fn has_vignetting(&self) -> bool {
        self.vignetting.is_some()
    }

    /// Source position (in the uncorrected photo) of output pixel (x, y), for
    /// the green channel. Identity without a distortion model.
    pub fn distort(&self, x: f64, y: f64) -> (f64, f64) {
        match &self.distortion {
            Some((d, f, s)) => {
                let (nx, ny) = f.to_norm(x, y);
                let (dx, dy) = d.apply(nx / s, ny / s);
                f.to_px(dx, dy)
            }
            None => (x, y),
        }
    }

    /// Red and blue source positions for a green source position.
    pub fn tca(&self, x: f64, y: f64) -> [(f64, f64); 2] {
        match &self.tca {
            Some((t, f)) => {
                let (nx, ny) = f.to_norm(x, y);
                let [r, b] = t.apply(nx, ny);
                [f.to_px(r.0, r.1), f.to_px(b.0, b.1)]
            }
            None => [(x, y), (x, y)],
        }
    }

    /// Gain that removes the lens vignetting at source pixel (x, y): 1 at the
    /// centre, above 1 towards darker corners.
    pub fn vignette_gain(&self, x: f64, y: f64) -> f64 {
        match &self.vignetting {
            Some((v, f)) => {
                let (nx, ny) = f.to_norm(x, y);
                1.0 / v.falloff(nx * nx + ny * ny).max(0.05)
            }
            None => 1.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Source;

    fn correction(
        distortion: Option<(Distortion, Frame)>,
        vignetting: Option<(Vignetting, Frame)>,
    ) -> Correction {
        Correction {
            source: Source::Lensfun,
            description: "test".into(),
            file: None,
            distortion,
            tca: None,
            vignetting,
        }
    }

    const IMG: ImageFrame = ImageFrame {
        width: 3001,
        height: 2001,
        crop_factor: Some(1.0),
    };

    #[test]
    fn hugin_unit_is_half_the_short_side() {
        // Full frame 3:2, calibration on full frame: r = 1 at half the image height.
        let f = PixelFrame::new(
            &Frame::HuginShortSide {
                cal_crop: 1.0,
                cal_aspect: 1.5,
            },
            &IMG,
        );
        assert!((f.uy - 1000.5).abs() < 1.0, "{}", f.uy);
        // The same lens on an APS-C body (crop 1.5): the image is 1.5× smaller in mm.
        let apsc = ImageFrame {
            crop_factor: Some(1.5),
            ..IMG
        };
        let g = PixelFrame::new(
            &Frame::HuginShortSide {
                cal_crop: 1.0,
                cal_aspect: 1.5,
            },
            &apsc,
        );
        assert!((g.uy / f.uy - 1.5).abs() < 1e-9);
    }

    #[test]
    fn vignetting_frame_is_half_the_diagonal() {
        let f = PixelFrame::new(&Frame::HalfDiagonal { cal_crop: 1.0 }, &IMG);
        assert!((f.ux - 3001f64.hypot(2001.0) / 2.0).abs() < 1.0);
        let v = correction(
            None,
            Some((
                Vignetting::Pa {
                    k1: -0.3,
                    k2: 0.0,
                    k3: 0.0,
                },
                Frame::HalfDiagonal { cal_crop: 1.0 },
            )),
        );
        let g = Geometry::new(&v, &IMG);
        assert!((g.vignette_gain(1500.0, 1000.0) - 1.0).abs() < 1e-9);
        // Corner: r ≈ 1, falloff 0.7, gain 1/0.7.
        assert!((g.vignette_gain(0.0, 0.0) - 1.0 / 0.7).abs() < 2e-3);
    }

    #[test]
    fn barrel_correction_samples_further_out_after_centre_normalisation() {
        // ptlens barrel profile; with lensfun's centre normalisation the centre
        // keeps scale 1 and the corners map inside the frame.
        let c = correction(
            Some((
                Distortion::PtLens {
                    a: 0.0,
                    b: -0.02,
                    c: 0.0,
                },
                Frame::HuginShortSide {
                    cal_crop: 1.0,
                    cal_aspect: 1.5,
                },
            )),
            None,
        );
        let g = Geometry::new(&c, &IMG);
        let (x, y) = g.distort(1500.0, 1000.0);
        assert!((x - 1500.0).abs() < 1e-9 && (y - 1000.0).abs() < 1e-9);
        // Near the centre the mapping is (almost) the identity.
        let (x1, _) = g.distort(1510.0, 1000.0);
        assert!((x1 - 1510.0).abs() < 0.05, "{x1}");
        // At the corner a barrel lens's content sits further in (smaller source radius
        // relative to the centre scale); the source stays within the frame.
        let (xc, yc) = g.distort(0.0, 0.0);
        assert!(xc > 0.0 && yc > 0.0, "({xc}, {yc})");
    }

    #[test]
    fn acm_frame_uses_longest_side() {
        let f = PixelFrame::new(
            &Frame::Acm {
                cx: 0.5,
                cy: 1.0 / 3.0,
                fx: 0.8,
                fy: 0.8,
                cal_crop: 1.0,
            },
            &IMG,
        );
        assert!((f.ux - 0.8 * 3001.0).abs() < 1e-9);
        assert!((f.cx - 1500.0).abs() < 1e-9);
        assert!((f.cy - (3001.0 / 3.0 - 0.5)).abs() < 1e-9);
    }
}
