// SPDX-License-Identifier: GPL-3.0-or-later
//! Tests for the lens corrections.

use super::*;
use amb_lens::{Distortion, Frame, Source, Tca, Vignetting};

fn correction(
    distortion: Option<(Distortion, Frame)>,
    tca: Option<(Tca, Frame)>,
    vignetting: Option<(Vignetting, Frame)>,
) -> Correction {
    Correction {
        source: Source::Lensfun,
        description: "synthetic".into(),
        file: None,
        distortion,
        tca,
        vignetting,
    }
}

const HUGIN: Frame = Frame::HuginShortSide {
    cal_crop: 1.0,
    cal_aspect: 1.5,
};

#[test]
fn a_dng_correction_applies_without_profile_corrections() {
    let (w, h) = (41, 31);
    let rgb = vec![0.25f32; w * h * 3];
    // Gain 1 + r²: 2 at the corners.
    let c = Correction {
        source: Source::Embedded,
        description: "Built-in (DNG)".into(),
        file: None,
        distortion: None,
        tca: None,
        vignetting: Some((
            Vignetting::DngGain {
                k: [1.0, 0.0, 0.0, 0.0, 0.0],
            },
            Frame::Dng {
                cx: 0.5,
                cy: 0.5,
                unit: 25.0 / 40.0,
            },
        )),
    };
    let shot = amb_lens::ShotInfo {
        embedded: Some(c),
        ..Default::default()
    };
    let off = LensCorrections::default();
    assert!(!off.needs_camera_pass());
    let lc = off.for_image(&shot);
    assert!(lc.needs_camera_pass());
    let out = apply_camera(&rgb, w, h, None, &lc, &DEFAULT_STRENGTHS);
    let at = |x: usize, y: usize| out.rgb[(y * w + x) * 3];
    assert!((at(20, 15) - 0.25).abs() < 1e-4, "{}", at(20, 15));
    assert!((at(0, 0) - 0.5).abs() < 0.01, "{}", at(0, 0));
}

fn lens(profile: Correction) -> LensCorrections {
    LensCorrections {
        profile_enable: true,
        profile: Some(profile),
        ..Default::default()
    }
}

#[test]
fn neutral_settings_do_nothing() {
    let lc = LensCorrections::default();
    assert!(!lc.needs_camera_pass());
    let rgb = vec![0.5f32; 30 * 20 * 3];
    let out = apply_camera(&rgb, 30, 20, None, &lc, &DEFAULT_STRENGTHS);
    assert_eq!(out.rgb, rgb);
    assert!(out.blank.is_none());
}

#[test]
fn profile_vignetting_flattens_a_vignetted_field() {
    let (w, h) = (300, 200);
    let v = Vignetting::Pa {
        k1: -0.4,
        k2: 0.1,
        k3: 0.0,
    };
    let c = correction(None, None, Some((v, Frame::HalfDiagonal { cal_crop: 1.0 })));
    let geo = Geometry::new(
        &c,
        &ImageFrame {
            width: w,
            height: h,
            crop_factor: Some(1.0),
        },
    );
    // Flat 0.5 field, darkened by the lens.
    let mut rgb = vec![0.0f32; w * h * 3];
    for y in 0..h {
        for x in 0..w {
            let g = 0.5 / geo.vignette_gain(x as f64, y as f64);
            rgb[(y * w + x) * 3..(y * w + x) * 3 + 3].copy_from_slice(&[g as f32; 3]);
        }
    }
    let out = apply_camera(&rgb, w, h, Some(1.0), &lens(c.clone()), &DEFAULT_STRENGTHS);
    assert!(out.rgb.iter().all(|v| (v - 0.5).abs() < 1e-4));
    // Vignetting amount 0 leaves the falloff in.
    let half = LensCorrections {
        vignetting_scale: 0.0,
        ..lens(c)
    };
    let out = apply_camera(&rgb, w, h, Some(1.0), &half, &DEFAULT_STRENGTHS);
    assert_eq!(out.rgb, rgb);
}

/// A smooth test scene (no aliasing), as an RGB image seen through `distort`
/// (output pixel → scene position).
fn scene(x: f64, y: f64) -> f32 {
    (0.5 + 0.2 * (x / 9.0).sin() + 0.2 * (y / 7.0).cos()) as f32
}

fn render_scene(w: usize, h: usize, distort: impl Fn(f64, f64) -> (f64, f64)) -> Vec<f32> {
    let mut rgb = vec![0.0f32; w * h * 3];
    for y in 0..h {
        for x in 0..w {
            let (sx, sy) = distort(x as f64, y as f64);
            rgb[(y * w + x) * 3..(y * w + x) * 3 + 3].copy_from_slice(&[scene(sx, sy); 3]);
        }
    }
    rgb
}

/// Smallest mean error between `img` and the scene at any zoom about the
/// centre: 0 when `img` is an undistorted (scaled) view of the scene.
fn best_zoom_error(img: &[f32], w: usize, h: usize) -> f64 {
    let (cx, cy, _) = centre(w, h);
    let err = |z: f64| -> f64 {
        let (mut sum, mut n) = (0.0, 0);
        for y in (h / 10..h - h / 10).step_by(3) {
            for x in (w / 10..w - w / 10).step_by(3) {
                let expect = scene(cx + (x as f64 - cx) / z, cy + (y as f64 - cy) / z);
                sum += f64::from((img[(y * w + x) * 3 + 1] - expect).abs());
                n += 1;
            }
        }
        sum / n as f64
    };
    (0..=1200)
        .map(|i| err(0.7 + i as f64 * 0.0005))
        .fold(f64::MAX, f64::min)
}

#[test]
fn profile_distortion_undoes_the_lens() {
    let (w, h) = (360, 240);
    let d = Distortion::Poly5 { k1: -0.06, k2: 0.0 };
    let c = correction(Some((d, HUGIN)), None, None);
    let frame = ImageFrame {
        width: w,
        height: h,
        crop_factor: Some(1.0),
    };
    let geo = Geometry::new(&c, &frame);
    // The camera sees the scene through a barrel lens: the observed pixel at
    // source position s shows the scene point u with distort(u) = s.
    let invert = |sx: f64, sy: f64| {
        let (mut ux, mut uy) = (sx, sy);
        for _ in 0..40 {
            let (dx, dy) = geo.distort(ux, uy);
            ux += sx - dx;
            uy += sy - dy;
        }
        (ux, uy)
    };
    let observed = render_scene(w, h, invert);
    let before = best_zoom_error(&observed, w, h);
    let out = apply_camera(&observed, w, h, Some(1.0), &lens(c), &DEFAULT_STRENGTHS);
    assert!(out.blank.is_none(), "profile corrections fill the frame");
    let after = best_zoom_error(&out.rgb, w, h);
    assert!(before > 0.03, "test image is visibly distorted ({before})");
    assert!(
        after < 0.005,
        "undistorted after correction: {after} (before {before})"
    );
}

#[test]
fn manual_distortion_leaves_blanks_unless_constrained() {
    let (w, h) = (120, 80);
    let rgb = vec![0.5f32; w * h * 3];
    let lc = LensCorrections {
        manual_distortion: 60.0,
        ..Default::default()
    };
    let out = apply_camera(&rgb, w, h, None, &lc, &DEFAULT_STRENGTHS);
    let blank = out
        .blank
        .expect("correcting barrel pulls the edge midpoints in");
    assert!(blank[(h / 2) * w], "left edge midpoint is blank");
    assert!(!blank[0], "corners stay put");
    assert!(!blank[(h / 2) * w + w / 2], "centre is covered");
    let constrained = LensCorrections {
        constrain_crop: true,
        ..lc
    };
    let out = apply_camera(&rgb, w, h, None, &constrained, &DEFAULT_STRENGTHS);
    assert!(out.blank.is_none());
}

#[test]
fn crop_resamples_the_rotated_rectangle_and_blanks_outside() {
    use crate::crop::Crop;
    let (w, h) = (240, 160);
    let observed = render_scene(w, h, |x, y| (x, y));
    let lc = LensCorrections::default();
    // A straightened crop inside the frame: every output pixel is the
    // scene at its mapped position, and nothing is blank.
    let inside = Crop {
        enabled: true,
        left: 0.2,
        top: 0.2,
        right: 0.8,
        bottom: 0.8,
        angle: 5.0,
    };
    let g = inside.geometry(w, h).expect("a crop");
    let out = apply_camera_cropped(
        &observed,
        w,
        h,
        None,
        &lc,
        &DEFAULT_STRENGTHS,
        Some(&g),
        None,
    );
    assert_eq!((out.width, out.height), (g.width, g.height));
    assert_eq!(out.rgb.len(), g.width * g.height * 3);
    assert!(out.blank.is_none());
    let mut worst = 0.0f32;
    for y in 0..g.height {
        for x in 0..g.width {
            let (sx, sy) = g.source(x as f64, y as f64);
            let v = out.rgb[(y * g.width + x) * 3 + 1];
            worst = worst.max((v - scene(sx, sy)).abs());
        }
    }
    assert!(worst < 2e-3, "{worst}");
    // The full-frame corners with an angle: the rotated rectangle pokes
    // out of the frame, and those parts are blank.
    let full = Crop {
        left: 0.0,
        top: 0.0,
        right: 1.0,
        bottom: 1.0,
        ..inside
    };
    let g = full.geometry(w, h).expect("a crop");
    let out = apply_camera_cropped(
        &observed,
        w,
        h,
        None,
        &lc,
        &DEFAULT_STRENGTHS,
        Some(&g),
        None,
    );
    let blank = out.blank.expect("blank corners");
    let (cw, ch) = (g.width, g.height);
    assert!(
        blank[cw - 1] && blank[(ch - 1) * cw],
        "the other two corners"
    );
    assert!(!blank[(ch / 2) * cw + cw / 2]);
}

#[test]
fn manual_vignetting_lightens_corners_and_follows_midpoint() {
    let (w, h) = (90, 60);
    let rgb = vec![0.25f32; w * h * 3];
    let lc = LensCorrections {
        vignette_amount: 100.0,
        ..Default::default()
    };
    let out = apply_camera(&rgb, w, h, None, &lc, &DEFAULT_STRENGTHS);
    let at = |o: &CameraPass, x: usize, y: usize| o.rgb[(y * w + x) * 3];
    assert!(
        (at(&out, w / 2, h / 2) - 0.25).abs() < 1e-6,
        "centre unchanged"
    );
    assert!(at(&out, 0, 0) > 0.6, "corner lightened by ~1.5 stops");
    let late = LensCorrections {
        vignette_midpoint: 100.0,
        ..lc
    };
    let out_late = apply_camera(&rgb, w, h, None, &late, &DEFAULT_STRENGTHS);
    let mid = (w / 4, h / 4);
    assert!(at(&out_late, mid.0, mid.1) < at(&out, mid.0, mid.1));
}

/// Edges at many angles: a pattern of rings, with red scaled radially by
/// `kr` and blue by `kb` (lateral CA).
fn rings_with_ca(w: usize, h: usize, kr: f64, kb: f64) -> Vec<f32> {
    let (cx, cy, _) = centre(w, h);
    let pattern = |x: f64, y: f64| -> f32 {
        let r = (x - cx).hypot(y - cy);
        let angle = (y - cy).atan2(x - cx);
        let v = (r / 9.0 + 0.3 * (angle * 7.0).sin()).sin();
        if v > 0.0 { 0.85 } else { 0.08 }
    };
    let sample = |x: f64, y: f64, k: f64| {
        let mut acc = 0.0;
        for sy in 0..4 {
            for sx in 0..4 {
                let px = x + (sx as f64 - 1.5) / 4.0;
                let py = y + (sy as f64 - 1.5) / 4.0;
                // Channel content at radius r comes from scene radius r / k.
                acc += pattern(cx + (px - cx) / k, cy + (py - cy) / k);
            }
        }
        acc / 16.0
    };
    let mut rgb = vec![0.0f32; w * h * 3];
    rgb.par_chunks_mut(w * 3).enumerate().for_each(|(y, row)| {
        for x in 0..w {
            let (fx, fy) = (x as f64, y as f64);
            row[x * 3] = sample(fx, fy, kr);
            row[x * 3 + 1] = sample(fx, fy, 1.0);
            row[x * 3 + 2] = sample(fx, fy, kb);
        }
    });
    rgb
}

#[test]
fn auto_ca_estimates_and_removes_radial_shift() {
    let (w, h) = (600, 400);
    let (kr, kb) = (1.004, 0.996);
    let rgb = rings_with_ca(w, h, kr, kb);
    let [red, blue] = estimate_ca(&rgb, w, h, 4.0).expect("enough edges");
    let half_diag = centre(w, h).2;
    // Red is displaced outward by (kr − 1)·r_px at radius r_px.
    let expect_red = (kr - 1.0) * half_diag;
    let expect_blue = (kb - 1.0) * half_diag;
    assert!(
        (red.at(1.0) - expect_red).abs() < 0.25,
        "red {red:?} vs {expect_red}"
    );
    assert!(
        (blue.at(1.0) - expect_blue).abs() < 0.25,
        "blue {blue:?} vs {expect_blue}"
    );
    let lc = LensCorrections {
        auto_ca: true,
        ..Default::default()
    };
    let out = apply_camera(&rgb, w, h, None, &lc, &DEFAULT_STRENGTHS);
    let misfit = |img: &[f32]| -> f64 {
        img.chunks(3)
            .map(|p| f64::from((p[0] - p[1]).abs() + (p[2] - p[1]).abs()))
            .sum::<f64>()
    };
    assert!(
        misfit(&out.rgb) < 0.5 * misfit(&rgb),
        "fringes reduced: {} → {}",
        misfit(&rgb),
        misfit(&out.rgb)
    );
}

#[test]
fn auto_ca_finds_nothing_in_a_flat_image() {
    let rgb = vec![0.3f32; 200 * 150 * 3];
    assert!(estimate_ca(&rgb, 200, 150, 4.0).is_none());
}

#[test]
fn profile_tca_applies_with_remove_ca_only() {
    let (w, h) = (200, 140);
    let rgb = rings_with_ca(w, h, 1.0, 1.0);
    let tca = Tca::Linear {
        kr: 1.002,
        kb: 0.998,
    };
    let c = correction(None, Some((tca, HUGIN)), None);
    // Without Remove CA, a profile's TCA is not applied (by default).
    let out = apply_camera(&rgb, w, h, Some(1.0), &lens(c.clone()), &DEFAULT_STRENGTHS);
    assert_eq!(out.rgb, rgb);
    // On a flat field (no edges for auto CA), Remove CA falls back to the profile.
    let flat = vec![0.4f32; w * h * 3];
    let lc = LensCorrections {
        auto_ca: true,
        ..lens(c)
    };
    let out = apply_camera(&flat, w, h, Some(1.0), &lc, &DEFAULT_STRENGTHS);
    assert!(out.rgb.iter().all(|v| (v - 0.4).abs() < 1e-5));
}

#[test]
fn defringe_desaturates_purple_at_edges_only() {
    let (w, h) = (80, 40);
    let oklab = OkLab::new();
    // Purple (OkLab hue ~310°) at chroma 0.1, lightness 0.5.
    let hue = 310f32.to_radians();
    let purple = oklab.to_working([0.5, 0.1 * hue.cos(), 0.1 * hue.sin()]);
    let mut img = Image::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let v = if x < 20 {
                [0.02, 0.02, 0.02] // dark
            } else if x < 22 {
                purple // a 2 px fringe at the edge
            } else if x < 50 {
                [0.9, 0.9, 0.9] // bright
            } else {
                purple // a purple subject far from any edge
            };
            img.set(x, y, v);
        }
    }
    let d = Defringe {
        purple_amount: 10.0,
        ..Default::default()
    };
    apply_defringe(&mut img, &d, &DEFAULT_STRENGTHS);
    let chroma = |p: [f32; 3]| {
        let l = oklab.from_working(p);
        l[1].hypot(l[2])
    };
    assert!(chroma(img.get(20, 20)) < 0.03, "fringe removed");
    assert!(chroma(img.get(21, 20)) < 0.03, "fringe removed");
    assert!(chroma(img.get(70, 20)) > 0.09, "flat purple subject kept");
    // Green amount alone doesn't touch purple.
    let mut img2 = Image::new(w, h);
    for y in 0..h {
        for x in 0..w {
            img2.set(x, y, if x < 20 { [0.02; 3] } else { purple });
        }
    }
    let green_only = Defringe {
        green_amount: 10.0,
        ..Default::default()
    };
    apply_defringe(&mut img2, &green_only, &DEFAULT_STRENGTHS);
    assert!(chroma(img2.get(21, 20)) > 0.09);
}

#[test]
fn auto_defringe_finds_fringes_and_keeps_colours_that_belong() {
    let (w, h) = (120, 40);
    let oklab = OkLab::new();
    let at = |deg: f32, c: f32| {
        let r = deg.to_radians();
        oklab.to_working([0.5, c * r.cos(), c * r.sin()])
    };
    let (purple, green, sky) = (at(310.0, 0.1), at(150.0, 0.1), at(255.0, 0.1));
    let mut img = Image::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let v = match x {
                0..20 => [0.02; 3],  // dark branch
                20..22 => purple,    // its purple fringe
                22..40 => [0.9; 3],  // bright
                40..42 => green,     // a green fringe the other way
                42..60 => [0.02; 3], // dark
                _ => sky,            // a blue sky beside it
            };
            img.set(x, y, v);
        }
    }
    let d = Defringe {
        auto: true,
        ..Default::default()
    };
    apply_defringe(&mut img, &d, &DEFAULT_STRENGTHS);
    let chroma = |p: [f32; 3]| {
        let l = oklab.from_working(p);
        l[1].hypot(l[2])
    };
    assert!(
        chroma(img.get(21, 20)) < 0.04,
        "purple fringe: {}",
        chroma(img.get(21, 20))
    );
    assert!(
        chroma(img.get(40, 20)) < 0.04,
        "green fringe: {}",
        chroma(img.get(40, 20))
    );
    assert!(
        chroma(img.get(62, 20)) > 0.07,
        "the sky at the edge: {}",
        chroma(img.get(62, 20))
    );
    assert!(chroma(img.get(100, 20)) > 0.099, "the sky away from it");
}
