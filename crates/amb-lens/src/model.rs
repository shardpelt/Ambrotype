// SPDX-License-Identifier: GPL-3.0-or-later
//! Lens correction models, as resolved for one shot.
//!
//! The polynomial models are the published ones used by lensfun/Hugin
//! (`poly3`, `poly5`, `ptlens`, TCA `linear`/`poly3`, vignetting `pa`) and the
//! Adobe Camera Model used by LCP files (radial + tangential distortion, R/B
//! relative to G for lateral CA, radial vignetting). All map *undistorted*
//! coordinates to *distorted* ones, i.e. "where in the original photo does this
//! output pixel come from", which is what resampling needs.

/// The coordinate frame a model's radius is measured in.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Frame {
    /// lensfun/Hugin distortion and TCA: r = 1 at half the short side of the
    /// calibration sensor. `cal_aspect` is long/short side (1.5 for 3:2).
    HuginShortSide { cal_crop: f64, cal_aspect: f64 },
    /// lensfun vignetting: r = 1 at half the calibration sensor's diagonal.
    HalfDiagonal { cal_crop: f64 },
    /// Adobe Camera Model: x = (u - cx) / fx, y = (v - cy) / fy, where the
    /// centre and focal lengths are fractions of the calibration image's longest
    /// side (LCP `ImageXCenter`, `ImageYCenter`, `FocalLengthX`, `FocalLengthY`).
    Acm {
        cx: f64,
        cy: f64,
        fx: f64,
        fy: f64,
        cal_crop: f64,
    },
    /// DNG opcodes (`WarpRectilinear`, `FixVignetteRadial`): r = 1 at the
    /// image corner furthest from the optical centre. The centre is a
    /// fraction of the photo's width and height, the unit a fraction of its
    /// longest side (both from pixel centre 0 to the last pixel's centre).
    Dng { cx: f64, cy: f64, unit: f64 },
}

impl Frame {
    /// Linear blend (for interpolating LCP frames between focal lengths).
    /// Returns `None` when the two frames differ in kind.
    pub fn lerp(&self, other: &Frame, t: f64) -> Option<Frame> {
        let l = |a: f64, b: f64| a + (b - a) * t;
        Some(match (*self, *other) {
            (
                Frame::HuginShortSide {
                    cal_crop: c0,
                    cal_aspect: a0,
                },
                Frame::HuginShortSide {
                    cal_crop: c1,
                    cal_aspect: a1,
                },
            ) => Frame::HuginShortSide {
                cal_crop: l(c0, c1),
                cal_aspect: l(a0, a1),
            },
            (Frame::HalfDiagonal { cal_crop: c0 }, Frame::HalfDiagonal { cal_crop: c1 }) => {
                Frame::HalfDiagonal {
                    cal_crop: l(c0, c1),
                }
            }
            (
                Frame::Acm {
                    cx,
                    cy,
                    fx,
                    fy,
                    cal_crop,
                },
                Frame::Acm {
                    cx: cx1,
                    cy: cy1,
                    fx: fx1,
                    fy: fy1,
                    cal_crop: c1,
                },
            ) => Frame::Acm {
                cx: l(cx, cx1),
                cy: l(cy, cy1),
                fx: l(fx, fx1),
                fy: l(fy, fy1),
                cal_crop: l(cal_crop, c1),
            },
            (
                Frame::Dng { cx, cy, unit },
                Frame::Dng {
                    cx: x,
                    cy: y,
                    unit: u,
                },
            ) => Frame::Dng {
                cx: l(cx, x),
                cy: l(cy, y),
                unit: l(unit, u),
            },
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Distortion {
    /// rd = ru · (1 − k1 + k1·ru²)
    Poly3 { k1: f64 },
    /// rd = ru · (1 + k1·ru² + k2·ru⁴)
    Poly5 { k1: f64, k2: f64 },
    /// rd = ru · (a·ru³ + b·ru² + c·ru + 1 − a − b − c)
    PtLens { a: f64, b: f64, c: f64 },
    /// Adobe Camera Model: radial k1..k3 plus tangential k4, k5 (lensfun's
    /// ordering; LCP `RadialDistortParam1..3`, `TangentialDistortParam1..2`):
    /// x_d = x·(1 + k1 r² + k2 r⁴ + k3 r⁶ + 2(k4 y + k5 x)) + k5 r²,
    /// y_d = y·(1 + k1 r² + k2 r⁴ + k3 r⁶ + 2(k4 y + k5 x)) + k4 r².
    Acm { k: [f64; 5] },
    /// DNG `WarpRectilinear` for one plane: radial kr0..kr3 and tangential
    /// kt0, kt1 (`[kr0, kr1, kr2, kr3, kt0, kt1]`), in a [`Frame::Dng`]:
    /// x_d = x·f + kt0·2xy + kt1·(r² + 2x²), y_d = y·f + kt1·2xy + kt0·(r² + 2y²),
    /// f = kr0 + kr1 r² + kr2 r⁴ + kr3 r⁶.
    Dng { k: [f64; 6] },
}

/// Lateral chromatic aberration: where red and blue sit relative to green,
/// applied to the green channel's (already distorted) position.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Tca {
    /// r_R = r · kr, r_B = r · kb
    Linear { kr: f64, kb: f64 },
    /// r_C = r · (b_C·r² + c_C·r + v_C)
    Poly3 {
        vr: f64,
        vb: f64,
        cr: f64,
        cb: f64,
        br: f64,
        bb: f64,
    },
    /// Adobe Camera Model per channel: `[scale, k1, k2, k3, k4, k5]`, the
    /// distortion polynomial of [`Distortion::Acm`] times the scale factor
    /// (LCP `ChromaticRedGreenModel` / `ChromaticBlueGreenModel`).
    Acm { red: [f64; 6], blue: [f64; 6] },
    /// DNG `WarpRectilinear` with a warp per plane ([`Distortion::Dng`]'s
    /// coefficients): red and blue come from their own warps of the output
    /// position the green one maps to.
    Dng {
        red: [f64; 6],
        green: [f64; 6],
        blue: [f64; 6],
    },
}

/// Vignetting: the observed image is the ideal one times
/// (1 + k1 r² + k2 r⁴ + k3 r⁶); correcting divides by it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Vignetting {
    /// lensfun `pa`, in the [`Frame::HalfDiagonal`] frame.
    Pa { k1: f64, k2: f64, k3: f64 },
    /// Adobe Camera Model `VignetteModelParam1..3`, in an [`Frame::Acm`] frame.
    Acm { k1: f64, k2: f64, k3: f64 },
    /// DNG `FixVignetteRadial`: the correcting gain is
    /// 1 + k0 r² + k1 r⁴ + k2 r⁶ + k3 r⁸ + k4 r¹⁰, in a [`Frame::Dng`].
    DngGain { k: [f64; 5] },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Lensfun,
    Lcp,
    /// Correction data embedded in the raw file (DNG `WarpRectilinear` and
    /// `FixVignetteRadial` opcodes; maker notes aren't read yet).
    Embedded,
}

/// A lens profile resolved for one shot: everything interpolated to its focal
/// length, aperture and focus distance.
#[derive(Debug, Clone, PartialEq)]
pub struct Correction {
    pub source: Source,
    /// Human-readable profile name, e.g. the lens model and calibration crop.
    pub description: String,
    /// The file the profile came from (a lensfun XML file or an .lcp).
    pub file: Option<String>,
    pub distortion: Option<(Distortion, Frame)>,
    pub tca: Option<(Tca, Frame)>,
    pub vignetting: Option<(Vignetting, Frame)>,
}

impl Distortion {
    /// Magnification at the centre (lensfun's `d`); dividing the undistorted
    /// radius by it keeps the centre's scale at 1, as lensfun does.
    pub fn centre_scale(&self) -> f64 {
        match *self {
            Distortion::Poly3 { k1 } => 1.0 - k1,
            Distortion::Poly5 { .. } | Distortion::Acm { .. } | Distortion::Dng { .. } => 1.0,
            Distortion::PtLens { a, b, c } => 1.0 - a - b - c,
        }
    }

    /// Distorted position of an undistorted point (x, y) in the model's frame.
    pub fn apply(&self, x: f64, y: f64) -> (f64, f64) {
        let r2 = x * x + y * y;
        match *self {
            Distortion::Poly3 { k1 } => {
                let f = 1.0 - k1 + k1 * r2;
                (x * f, y * f)
            }
            Distortion::Poly5 { k1, k2 } => {
                let f = 1.0 + k1 * r2 + k2 * r2 * r2;
                (x * f, y * f)
            }
            Distortion::PtLens { a, b, c } => {
                let r = r2.sqrt();
                let f = a * r2 * r + b * r2 + c * r + 1.0 - a - b - c;
                (x * f, y * f)
            }
            Distortion::Acm { k } => acm(&k, x, y),
            Distortion::Dng { k } => dng_warp(&k, x, y),
        }
    }

    /// Linear blend of the coefficients (for interpolating between focal
    /// lengths). Returns `None` when the two models differ in kind.
    pub fn lerp(&self, other: &Distortion, t: f64) -> Option<Distortion> {
        let l = |a: f64, b: f64| a + (b - a) * t;
        Some(match (*self, *other) {
            (Distortion::Poly3 { k1: a }, Distortion::Poly3 { k1: b }) => {
                Distortion::Poly3 { k1: l(a, b) }
            }
            (Distortion::Poly5 { k1: a1, k2: a2 }, Distortion::Poly5 { k1: b1, k2: b2 }) => {
                Distortion::Poly5 {
                    k1: l(a1, b1),
                    k2: l(a2, b2),
                }
            }
            (
                Distortion::PtLens { a, b, c },
                Distortion::PtLens {
                    a: a2,
                    b: b2,
                    c: c2,
                },
            ) => Distortion::PtLens {
                a: l(a, a2),
                b: l(b, b2),
                c: l(c, c2),
            },
            (Distortion::Acm { k: a }, Distortion::Acm { k: b }) => Distortion::Acm {
                k: std::array::from_fn(|i| l(a[i], b[i])),
            },
            (Distortion::Dng { k: a }, Distortion::Dng { k: b }) => Distortion::Dng {
                k: std::array::from_fn(|i| l(a[i], b[i])),
            },
            _ => return None,
        })
    }
}

/// DNG `WarpRectilinear` on one plane (see [`Distortion::Dng`]).
pub(crate) fn dng_warp(k: &[f64; 6], x: f64, y: f64) -> (f64, f64) {
    let r2 = x * x + y * y;
    let f = k[0] + r2 * (k[1] + r2 * (k[2] + r2 * k[3]));
    (
        x * f + k[4] * 2.0 * x * y + k[5] * (r2 + 2.0 * x * x),
        y * f + k[5] * 2.0 * x * y + k[4] * (r2 + 2.0 * y * y),
    )
}

/// Adobe Camera Model polynomial (radial + tangential), lensfun's form.
pub(crate) fn acm(k: &[f64; 5], x: f64, y: f64) -> (f64, f64) {
    let r2 = x * x + y * y;
    let common =
        1.0 + k[0] * r2 + k[1] * r2 * r2 + k[2] * r2 * r2 * r2 + 2.0 * (k[3] * y + k[4] * x);
    (x * common + k[4] * r2, y * common + k[3] * r2)
}

impl Tca {
    /// Red and blue positions for a green position (x, y) in the model's frame.
    pub fn apply(&self, x: f64, y: f64) -> [(f64, f64); 2] {
        match *self {
            Tca::Linear { kr, kb } => [(x * kr, y * kr), (x * kb, y * kb)],
            Tca::Poly3 {
                vr,
                vb,
                cr,
                cb,
                br,
                bb,
            } => {
                let r2 = x * x + y * y;
                let r = r2.sqrt();
                let fr = br * r2 + cr * r + vr;
                let fb = bb * r2 + cb * r + vb;
                [(x * fr, y * fr), (x * fb, y * fb)]
            }
            Tca::Acm { red, blue } => {
                let ch = |p: &[f64; 6]| {
                    let (u, v) = acm(&[p[1], p[2], p[3], p[4], p[5]], x, y);
                    (p[0] * u, p[0] * v)
                };
                [ch(&red), ch(&blue)]
            }
            Tca::Dng { red, green, blue } => {
                // The output position green's warp maps to (x, y): a few
                // fixed-point steps (the warp is close to the identity).
                let (mut u, mut v) = (x, y);
                for _ in 0..8 {
                    let (gx, gy) = dng_warp(&green, u, v);
                    u += x - gx;
                    v += y - gy;
                }
                [dng_warp(&red, u, v), dng_warp(&blue, u, v)]
            }
        }
    }

    pub fn lerp(&self, other: &Tca, t: f64) -> Option<Tca> {
        let l = |a: f64, b: f64| a + (b - a) * t;
        Some(match (*self, *other) {
            (Tca::Linear { kr, kb }, Tca::Linear { kr: kr2, kb: kb2 }) => Tca::Linear {
                kr: l(kr, kr2),
                kb: l(kb, kb2),
            },
            (
                Tca::Poly3 {
                    vr,
                    vb,
                    cr,
                    cb,
                    br,
                    bb,
                },
                Tca::Poly3 {
                    vr: vr2,
                    vb: vb2,
                    cr: cr2,
                    cb: cb2,
                    br: br2,
                    bb: bb2,
                },
            ) => Tca::Poly3 {
                vr: l(vr, vr2),
                vb: l(vb, vb2),
                cr: l(cr, cr2),
                cb: l(cb, cb2),
                br: l(br, br2),
                bb: l(bb, bb2),
            },
            (Tca::Acm { red, blue }, Tca::Acm { red: r2, blue: b2 }) => Tca::Acm {
                red: std::array::from_fn(|i| l(red[i], r2[i])),
                blue: std::array::from_fn(|i| l(blue[i], b2[i])),
            },
            (
                Tca::Dng { red, green, blue },
                Tca::Dng {
                    red: r2,
                    green: g2,
                    blue: b2,
                },
            ) => Tca::Dng {
                red: std::array::from_fn(|i| l(red[i], r2[i])),
                green: std::array::from_fn(|i| l(green[i], g2[i])),
                blue: std::array::from_fn(|i| l(blue[i], b2[i])),
            },
            _ => return None,
        })
    }
}

impl Vignetting {
    /// Falloff factor at squared radius r² (1 at the centre, below 1 in darker corners).
    pub fn falloff(&self, r2: f64) -> f64 {
        let (k1, k2, k3) = match *self {
            Vignetting::Pa { k1, k2, k3 } | Vignetting::Acm { k1, k2, k3 } => (k1, k2, k3),
            Vignetting::DngGain { k } => {
                let gain = 1.0 + r2 * (k[0] + r2 * (k[1] + r2 * (k[2] + r2 * (k[3] + r2 * k[4]))));
                return 1.0 / gain.max(1e-3);
            }
        };
        1.0 + k1 * r2 + k2 * r2 * r2 + k3 * r2 * r2 * r2
    }

    pub fn lerp(&self, other: &Vignetting, t: f64) -> Option<Vignetting> {
        let l = |a: f64, b: f64| a + (b - a) * t;
        Some(match (*self, *other) {
            (
                Vignetting::Pa { k1, k2, k3 },
                Vignetting::Pa {
                    k1: a,
                    k2: b,
                    k3: c,
                },
            ) => Vignetting::Pa {
                k1: l(k1, a),
                k2: l(k2, b),
                k3: l(k3, c),
            },
            (
                Vignetting::Acm { k1, k2, k3 },
                Vignetting::Acm {
                    k1: a,
                    k2: b,
                    k3: c,
                },
            ) => Vignetting::Acm {
                k1: l(k1, a),
                k2: l(k2, b),
                k3: l(k3, c),
            },
            _ => return None,
        })
    }
}

// Compact one-line forms for reports and the CLI, e.g. `ptlens a=0.01 b=-0.04 c=0.02`.

/// Up to 7 decimals, trailing zeros trimmed.
fn num(v: f64) -> String {
    let s = format!("{v:.7}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.into() }
}

fn nums(v: &[f64]) -> String {
    let parts: Vec<String> = v.iter().map(|x| num(*x)).collect();
    format!("[{}]", parts.join(", "))
}

impl std::fmt::Display for Frame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            Frame::HuginShortSide {
                cal_crop,
                cal_aspect,
            } => write!(
                f,
                "r=1 at half the short side (calibration crop {}, aspect {})",
                num(cal_crop),
                num(cal_aspect)
            ),
            Frame::HalfDiagonal { cal_crop } => write!(
                f,
                "r=1 at half the diagonal (calibration crop {})",
                num(cal_crop)
            ),
            Frame::Acm {
                cx,
                cy,
                fx,
                fy,
                cal_crop,
            } => write!(
                f,
                "ACM centre ({}, {}) focal ({}, {}) × longest side (calibration crop {})",
                num(cx),
                num(cy),
                num(fx),
                num(fy),
                num(cal_crop)
            ),
            Frame::Dng { cx, cy, unit } => write!(
                f,
                "DNG centre ({}, {}), r=1 at {} × longest side",
                num(cx),
                num(cy),
                num(unit)
            ),
        }
    }
}

impl std::fmt::Display for Distortion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            Distortion::Poly3 { k1 } => write!(f, "poly3 k1={}", num(k1)),
            Distortion::Poly5 { k1, k2 } => write!(f, "poly5 k1={} k2={}", num(k1), num(k2)),
            Distortion::PtLens { a, b, c } => {
                write!(f, "ptlens a={} b={} c={}", num(a), num(b), num(c))
            }
            Distortion::Acm { k } => write!(f, "acm k={}", nums(&k)),
            Distortion::Dng { k } => write!(f, "dng warp k={}", nums(&k)),
        }
    }
}

impl std::fmt::Display for Tca {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            Tca::Linear { kr, kb } => write!(f, "linear kr={} kb={}", num(kr), num(kb)),
            Tca::Poly3 {
                vr,
                vb,
                cr,
                cb,
                br,
                bb,
            } => write!(
                f,
                "poly3 vr={} vb={} cr={} cb={} br={} bb={}",
                num(vr),
                num(vb),
                num(cr),
                num(cb),
                num(br),
                num(bb)
            ),
            Tca::Acm { red, blue } => write!(f, "acm red={} blue={}", nums(&red), nums(&blue)),
            Tca::Dng { red, green, blue } => write!(
                f,
                "dng warp red={} green={} blue={}",
                nums(&red),
                nums(&green),
                nums(&blue)
            ),
        }
    }
}

impl std::fmt::Display for Vignetting {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (name, k1, k2, k3) = match *self {
            Vignetting::Pa { k1, k2, k3 } => ("pa", k1, k2, k3),
            Vignetting::Acm { k1, k2, k3 } => ("acm", k1, k2, k3),
            Vignetting::DngGain { k } => return write!(f, "dng gain k={}", nums(&k)),
        };
        write!(f, "{name} k1={} k2={} k3={}", num(k1), num(k2), num(k3))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dng_tca_finds_red_and_blue_through_the_green_warp() {
        let green = [1.0, -0.03, 0.01, 0.0, 0.001, -0.002];
        let red = [1.002, -0.03, 0.01, 0.0, 0.001, -0.002];
        let t = Tca::Dng {
            red,
            green,
            blue: green,
        };
        // The output point (0.6, -0.4): green comes from g, red from r.
        let g = dng_warp(&green, 0.6, -0.4);
        let r = dng_warp(&red, 0.6, -0.4);
        let [tr, tb] = t.apply(g.0, g.1);
        assert!(
            (tr.0 - r.0).abs() < 1e-9 && (tr.1 - r.1).abs() < 1e-9,
            "{tr:?} {r:?}"
        );
        assert!((tb.0 - g.0).abs() < 1e-9 && (tb.1 - g.1).abs() < 1e-9);
    }

    #[test]
    fn display_is_compact() {
        let t = Tca::Linear {
            kr: 1.0019999999999998,
            kb: 0.9985,
        };
        assert_eq!(t.to_string(), "linear kr=1.002 kb=0.9985");
        assert_eq!(
            Distortion::Acm {
                k: [-0.025, 0.0, -0.0, 1.4e-5, 2.0]
            }
            .to_string(),
            "acm k=[-0.025, 0, 0, 0.000014, 2]"
        );
        assert_eq!(
            Frame::HalfDiagonal { cal_crop: 1.0 }.to_string(),
            "r=1 at half the diagonal (calibration crop 1)"
        );
    }

    #[test]
    fn zero_coefficients_are_identity() {
        for d in [
            Distortion::Poly3 { k1: 0.0 },
            Distortion::Poly5 { k1: 0.0, k2: 0.0 },
            Distortion::PtLens {
                a: 0.0,
                b: 0.0,
                c: 0.0,
            },
            Distortion::Acm { k: [0.0; 5] },
        ] {
            assert_eq!(d.apply(0.3, -0.7), (0.3, -0.7));
        }
        let t = Tca::Poly3 {
            vr: 1.0,
            vb: 1.0,
            cr: 0.0,
            cb: 0.0,
            br: 0.0,
            bb: 0.0,
        };
        assert_eq!(t.apply(0.2, 0.1), [(0.2, 0.1), (0.2, 0.1)]);
    }

    #[test]
    fn barrel_pulls_corners_inward() {
        // Negative k1 = barrel: the distorted radius is smaller than the undistorted one.
        let d = Distortion::Poly5 { k1: -0.05, k2: 0.0 };
        let (x, _) = d.apply(1.0, 0.0);
        assert!(x < 1.0);
    }

    #[test]
    fn acm_tangential_matches_brown_conrady() {
        // x_d = x + 2·p1·x·y + p2·(r² + 2x²) with p1 = k4, p2 = k5 (radial terms zero).
        let (k4, k5) = (0.01, -0.02);
        let (x, y) = (0.3, -0.2);
        let r2 = x * x + y * y;
        let (xd, yd) = Distortion::Acm {
            k: [0.0, 0.0, 0.0, k4, k5],
        }
        .apply(x, y);
        assert!((xd - (x + 2.0 * k4 * x * y + k5 * (r2 + 2.0 * x * x))).abs() < 1e-12);
        assert!((yd - (y + k4 * (r2 + 2.0 * y * y) + 2.0 * k5 * x * y)).abs() < 1e-12);
    }

    #[test]
    fn lerp_blends_same_kind_only() {
        let a = Distortion::PtLens {
            a: 0.0,
            b: 0.0,
            c: 0.0,
        };
        let b = Distortion::PtLens {
            a: 0.02,
            b: -0.04,
            c: 0.06,
        };
        assert_eq!(
            a.lerp(&b, 0.5),
            Some(Distortion::PtLens {
                a: 0.01,
                b: -0.02,
                c: 0.03
            })
        );
        assert_eq!(a.lerp(&Distortion::Poly3 { k1: 0.1 }, 0.5), None);
    }
}
