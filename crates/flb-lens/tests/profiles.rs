//! Lens profile readers, matching, interpolation and resolution, on made-up
//! fixtures in the real lensfun and LCP formats.

use std::path::{Path, PathBuf};

use flb_lens::lcp::{LcpFile, apex_to_f_number};
use flb_lens::{
    Distortion, Frame, LcpLibrary, LensfunDb, ProfileHint, ShotInfo, Source, Tca, Vignetting,
    resolve,
};

const ZOOMAR_LCP: &str = "ACME FF-1 (Acme Zoomar 24-70mm f2.8 Pro A) - RAW.lcp";

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn db() -> LensfunDb {
    LensfunDb::load_dir(fixtures().join("lensfun")).unwrap()
}

fn lcp_lib() -> LcpLibrary {
    LcpLibrary::scan([fixtures().join("lcp")])
}

fn shot(camera: &str, lens_make: Option<&str>, lens: Option<&str>, focal: Option<f64>) -> ShotInfo {
    ShotInfo {
        camera_make: "Acme".into(),
        camera_model: camera.into(),
        lens_make: lens_make.map(Into::into),
        lens_model: lens.map(Into::into),
        focal_mm: focal,
        aperture: None,
        distance_m: None,
        crop_factor: None,
    }
}

fn zoomar(camera: &str, focal: f64) -> ShotInfo {
    shot(
        camera,
        Some("Acme"),
        Some("Zoomar 24-70mm F2.8 Pro | A"),
        Some(focal),
    )
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

fn assert_close(actual: &[f64], expected: &[f64]) {
    assert_eq!(actual.len(), expected.len());
    for (a, e) in actual.iter().zip(expected) {
        assert!(close(*a, *e), "{actual:?} != {expected:?}");
    }
}

// ---------------------------------------------------------------- lensfun

#[test]
fn lensfun_parses_and_reports_failures() {
    let db = db();
    assert_eq!(db.files.len(), 1);
    assert_eq!(db.failed.len(), 1);
    assert!(db.failed[0].0.ends_with("broken.xml"));
    assert_eq!(db.cameras.len(), 3);
    // The two Zoomar blocks merge into one lens with two calibrations.
    assert_eq!(db.lenses.len(), 4);
    let zoomar = &db.lenses[0];
    assert_eq!(zoomar.model(), "Acme Zoomar 24-70mm f/2.8 Pro | A");
    assert_eq!(zoomar.models.len(), 2);
    assert_eq!(zoomar.mounts, ["Testmount X"]);
    assert_eq!(zoomar.calibrations.len(), 2);
    let ff = &zoomar.calibrations[0];
    assert_eq!(ff.crop_factor, 1.0);
    assert_eq!(ff.aspect_ratio, 1.5);
    // The `acm` entry is not a lensfun model we read: skipped.
    assert_eq!(ff.distortion.len(), 2);
    assert_eq!(
        ff.tca[1].1,
        Tca::Poly3 {
            vr: 1.0004,
            vb: 0.9996,
            cr: 0.0,
            cb: 0.0,
            br: 0.0,
            bb: 0.0
        }
    );
    assert_eq!(ff.vignetting.len(), 5);
    assert!(ff.file.ends_with("acme.xml"));
    let apsc = &zoomar.calibrations[1];
    assert_eq!(apsc.crop_factor, 1.5);
    assert_eq!(apsc.tca[0].1, Tca::Linear { kr: 1.001, kb: 1.0 });

    // Calibration attributes override the lens's crop factor and aspect ratio.
    let bolt = &db.lenses[1];
    assert_eq!(bolt.calibrations[0].crop_factor, 2.0);
    assert!(close(bolt.calibrations[0].aspect_ratio, 4.0 / 3.0));
    assert_eq!(bolt.calibrations[0].distortion.len(), 1);

    let fisheye = &db.lenses[2];
    assert_eq!(fisheye.lens_type, "fisheye");
    assert!(!fisheye.calibrations[0].rectilinear);
    assert_eq!(db.lenses[3].model(), "Acme Pocket P5 & compatibles");
}

#[test]
fn camera_lookup_matches_name_variants() {
    let db = db();
    for (make, model) in [
        ("Acme", "FF-1"),
        ("ACME OPTICAL CORPORATION", "Acme FF-1"),
        ("acme", "ACME  FF-1"),
        ("Acme Optical Corporation", "FF-1"),
    ] {
        let cam = db
            .camera(make, model)
            .unwrap_or_else(|| panic!("{make} {model}"));
        assert_eq!(cam.model(), "Acme FF-1");
        assert_eq!(cam.mount.as_deref(), Some("Testmount X"));
    }
    assert_eq!(db.camera_crop("Acme", "Mini 2"), Some(1.5));
    assert_eq!(flb_lens::camera_crop(&db, "ACME", "Acme Mini 2"), Some(1.5));
    assert!(db.camera("Acme", "FF-2").is_none());
    assert!(db.camera("Other", "FF-1").is_none());
    assert!(db.camera("Acme", "").is_none());
}

#[test]
fn lens_matching() {
    let db = db();
    let m = db.find_lens(&zoomar("FF-1", 35.0)).unwrap();
    assert_eq!(m.lens.model(), "Acme Zoomar 24-70mm f/2.8 Pro | A");
    assert!(close(m.score, 1.0));
    // The maker inside the EXIF lens name is ignored, and so is spacing/case.
    let m = db
        .find_lens(&shot(
            "FF-1",
            None,
            Some("ACME ZOOMAR 24.0-70.0 mm F/2.8 PRO | A"),
            None,
        ))
        .unwrap();
    assert_eq!(m.lens.maker(), "Acme");
    // A lens on a compatible mount (Testmount Classic on Testmount X).
    let m = db
        .find_lens(&shot("FF-1", Some("Bolt"), Some("50mm F1.8 Classic"), None))
        .unwrap();
    assert_eq!(m.lens.model(), "Bolt 50mm f/1.8 Classic");

    // Negative cases: other focal length, other aperture and name, no lens name.
    for lens in [
        "Zoomar 24-105mm f/4 Pro | A",
        "Bolt 50mm f/1.4",
        "Zoomar Pro",
        "Wonder 35mm f/1.8 Classic",
    ] {
        assert!(
            db.find_lens(&shot("FF-1", None, Some(lens), Some(50.0)))
                .is_none(),
            "{lens}"
        );
    }
    assert!(
        db.find_lens(&shot("FF-1", None, None, Some(50.0)))
            .is_none()
    );

    // A fixed-lens camera finds its lens through its mount.
    let m = db
        .find_lens(&shot("Acme Pocket P5", None, None, Some(5.0)))
        .unwrap();
    assert_eq!(m.lens.model(), "Acme Pocket P5 & compatibles");
    assert!(close(m.image_crop, 4.8));
}

#[test]
fn calibration_follows_crop_factor() {
    let db = db();
    let crop_for = |camera: &str, crop: Option<f64>| {
        let mut s = zoomar(camera, 35.0);
        s.crop_factor = crop;
        db.find_lens(&s).unwrap().calibration.crop_factor
    };
    assert_eq!(crop_for("FF-1", None), 1.0);
    // The camera database gives the Mini 2's crop factor.
    assert_eq!(crop_for("Mini 2", None), 1.5);
    // Largest calibration crop not above the image's (2 % tolerance) ...
    assert_eq!(crop_for("FF-1", Some(1.3)), 1.0);
    assert_eq!(crop_for("FF-1", Some(1.48)), 1.5);
    assert_eq!(crop_for("FF-1", Some(2.0)), 1.5);
    // ... or the smallest when all are larger.
    assert_eq!(crop_for("FF-1", Some(0.79)), 1.0);
    // Unknown camera and crop: full frame.
    assert_eq!(crop_for("Unknown", None), 1.0);
    let bolt = db
        .find_lens(&shot("FF-1", Some("Bolt"), Some("50mm F1.8 Classic"), None))
        .unwrap();
    assert_eq!(bolt.calibration.crop_factor, 2.0);
}

#[test]
fn lensfun_interpolation() {
    let db = db();
    let mut s = zoomar("FF-1", 47.0);
    // Halfway between f/2.8 and f/5.6 in stops.
    s.aperture = Some(2.8 * 2f64.sqrt());
    let m = db.find_lens(&s).unwrap();
    let c = db.correction(&m, &s);
    assert_eq!(c.source, Source::Lensfun);
    assert_eq!(
        c.description,
        "lensfun: Acme Zoomar 24-70mm f/2.8 Pro | A (calibrated at crop 1.0)"
    );
    assert!(c.file.as_deref().unwrap().ends_with("acme.xml"));
    let hugin = Frame::HuginShortSide {
        cal_crop: 1.0,
        cal_aspect: 1.5,
    };
    let (d, f) = c.distortion.unwrap();
    assert_eq!(f, hugin);
    let Distortion::PtLens { a, b, c: cc } = d else {
        panic!("{d:?}")
    };
    assert_close(&[a, b, cc], &[0.02, -0.03, 0.01]);
    let (t, f) = c.tca.unwrap();
    assert_eq!(f, hugin);
    let Tca::Poly3 { vr, vb, br, bb, .. } = t else {
        panic!("{t:?}")
    };
    assert_close(&[vr, vb, br, bb], &[1.0003, 0.9997, 0.00005, 0.0]);
    let (v, f) = c.vignetting.unwrap();
    assert_eq!(f, Frame::HalfDiagonal { cal_crop: 1.0 });
    let Vignetting::Pa { k1, k2, k3 } = v else {
        panic!("{v:?}")
    };
    // f/2.8–f/5.6 midpoints: 24 mm (-0.3, 0.075, 0), 70 mm (-0.6, 0.15, -0.01).
    assert_close(&[k1, k2, k3], &[-0.45, 0.1125, -0.005]);

    // Outside the calibrated range: the nearest focal length.
    let c = db.correction(&m, &zoomar("FF-1", 10.0));
    assert_eq!(
        c.distortion.unwrap().0,
        Distortion::PtLens {
            a: 0.01,
            b: -0.04,
            c: 0.02
        }
    );
    // No aperture, no vignetting.
    assert!(c.vignetting.is_none());
    // Close focus picks the close-distance calibration.
    let mut near = zoomar("FF-1", 24.0);
    near.aperture = Some(2.8);
    near.distance_m = Some(0.8);
    let (v, _) = db.correction(&m, &near).vignetting.unwrap();
    assert_eq!(
        v,
        Vignetting::Pa {
            k1: -0.9,
            k2: 0.3,
            k3: 0.0
        }
    );
    // Unknown focal length on a zoom: no distortion or TCA.
    let c = db.correction(
        &m,
        &shot(
            "FF-1",
            Some("Acme"),
            Some("Zoomar 24-70mm F2.8 Pro | A"),
            None,
        ),
    );
    assert!(c.distortion.is_none() && c.tca.is_none());
}

#[test]
fn lensfun_mixed_kinds_take_the_nearest() {
    let db = db();
    let s = zoomar("Mini 2", 30.0);
    let m = db.find_lens(&s).unwrap();
    let c = db.correction(&m, &s);
    let (d, f) = c.distortion.unwrap();
    assert_eq!(d, Distortion::Poly3 { k1: -0.02 });
    assert_eq!(
        f,
        Frame::HuginShortSide {
            cal_crop: 1.5,
            cal_aspect: 1.5
        }
    );
    let c = db.correction(&m, &zoomar("Mini 2", 60.0));
    assert!(matches!(c.distortion.unwrap().0, Distortion::PtLens { .. }));
    let c = db.correction(&m, &zoomar("Mini 2", 47.0));
    let Tca::Linear { kr, kb } = c.tca.unwrap().0 else {
        panic!()
    };
    assert_close(&[kr, kb], &[1.002, 0.9985]);
}

#[test]
fn fisheye_and_prime() {
    let db = db();
    let mut s = shot("FF-1", Some("Acme"), Some("Fisheye 8mm f/3.5"), Some(8.0));
    s.aperture = Some(5.6);
    let m = db.find_lens(&s).unwrap();
    let c = db.correction(&m, &s);
    assert!(c.distortion.is_none(), "fisheye: no rectilinear distortion");
    assert_eq!(
        c.tca.unwrap().0,
        Tca::Linear {
            kr: 1.001,
            kb: 0.999
        }
    );
    assert!(c.vignetting.is_some());

    // A prime needs no focal length; the calibration's aspect ratio is used.
    let s = shot("FF-1", Some("Bolt"), Some("50mm F1.8 Classic"), None);
    let m = db.find_lens(&s).unwrap();
    let (d, f) = db.correction(&m, &s).distortion.unwrap();
    assert_eq!(
        d,
        Distortion::Poly5 {
            k1: -0.05,
            k2: 0.01
        }
    );
    let Frame::HuginShortSide {
        cal_crop,
        cal_aspect,
    } = f
    else {
        panic!()
    };
    assert_close(&[cal_crop, cal_aspect], &[2.0, 4.0 / 3.0]);
}

// ---------------------------------------------------------------- LCP

#[test]
fn lcp_element_syntax() {
    let f = LcpFile::parse(fixtures().join("lcp").join(ZOOMAR_LCP)).unwrap();
    assert_eq!(f.entries.len(), 4);
    let e = &f.entries[0];
    assert_eq!(e.author.as_deref(), Some("Flightbroom tests"));
    assert_eq!(e.make.as_deref(), Some("ACME"));
    assert_eq!(e.model.as_deref(), Some("ACME FF-1"));
    assert_eq!(e.unique_camera_model.as_deref(), Some("Acme FF-1"));
    assert_eq!(e.camera_raw_profile, Some(true));
    assert_eq!(
        e.lens_pretty_name.as_deref(),
        Some("Acme Zoomar 24-70mm f/2.8 Pro | A")
    );
    assert_eq!(e.lens.as_deref(), Some("Zoomar 24-70mm F2.8 Pro | A"));
    assert_eq!(e.lens_id.as_deref(), Some("4242"));
    assert_eq!(e.lens_info.as_deref(), Some("24/1 70/1 28/10 28/10"));
    assert_eq!(e.sensor_format_factor, Some(1.0));
    assert_eq!((e.focal_length, e.focus_distance), (Some(24.0), Some(10.0)));
    assert_eq!(
        (e.image_width, e.image_length),
        (Some(6000.0), Some(4000.0))
    );
    assert!(close(e.f_number().unwrap(), 2f64.powf(1.5)));
    let p = e.perspective.unwrap();
    assert_eq!(p.version, Some(2.0));
    assert_eq!((p.focal_length_x, p.focal_length_y), (Some(0.6), Some(0.6)));
    assert_eq!(
        (p.image_x_center, p.image_y_center),
        (Some(0.5), Some(0.334))
    );
    assert_eq!(p.radial, [-0.1, 0.02, 0.0]);
    assert_eq!(p.tangential, [0.001, -0.002]);
    assert_eq!(p.residual_mean_error, Some(0.0001));
    let red = p.chromatic_red_green.unwrap();
    assert_eq!(red.scale_factor, 1.0002);
    assert_eq!(red.radial, [0.001, 0.0, 0.0]);
    assert_eq!(red.tangential, [0.0, 0.0]);
    assert_eq!(p.chromatic_blue_green.unwrap().tangential, [0.0001, 0.0002]);
    let v = p.vignette.unwrap();
    assert_eq!(v.params, [-0.5, 0.2, -0.05]);
    assert_eq!(v.focal_length_x, None);
    assert_eq!(
        f.entries[2]
            .perspective
            .unwrap()
            .vignette
            .unwrap()
            .focal_length_x,
        Some(1.7)
    );
    assert!(e.fisheye.is_none());
    assert_eq!(f.lens_name(), Some("Acme Zoomar 24-70mm f/2.8 Pro | A"));
}

#[test]
fn lcp_attribute_syntax() {
    let f = LcpFile::parse(fixtures().join("lcp/sub/bolt-sport.lcp")).unwrap();
    assert_eq!(f.entries.len(), 3);
    // Attributes on rdf:li.
    let e = &f.entries[0];
    assert_eq!(e.make.as_deref(), Some("Acme"));
    assert_eq!(e.camera_raw_profile, Some(false));
    assert_eq!(
        e.profile_name.as_deref(),
        Some("Bolt 70-200mm f/4 Sport & friends")
    );
    assert_eq!(e.lens_id.as_deref(), Some("77"));
    assert_eq!(e.sensor_format_factor, Some(1.5));
    assert_eq!(e.f_number(), Some(4.0));
    let p = e.perspective.unwrap();
    assert_eq!((p.focal_length_x, p.focal_length_y), (Some(2.0), Some(2.1)));
    assert_eq!(p.radial, [0.01, 0.0, 0.0]);
    let red = p.chromatic_red_green.unwrap();
    assert_eq!((red.scale_factor, red.radial), (1.001, [0.002, 0.0, 0.0]));
    assert!(p.chromatic_blue_green.is_none());
    assert_eq!(p.vignette.unwrap().params, [-0.2, 0.01, 0.0]);
    // Attributes on rdf:Description children, nested the same way.
    let e = &f.entries[1];
    assert_eq!(e.focal_length, Some(135.0));
    let p = e.perspective.unwrap();
    assert_eq!(p.radial, [0.03, -0.002, 0.0]);
    assert_eq!(p.tangential, [0.0004, 0.0]);
    assert_eq!(p.chromatic_red_green.unwrap().scale_factor, 1.003);
    assert_eq!(p.chromatic_blue_green.unwrap().scale_factor, 0.998);
    assert_eq!(p.vignette.unwrap().params, [-0.4, 0.03, 0.0]);
    let e = &f.entries[2];
    assert_eq!(e.author, None);
    assert_eq!(e.focus_distance, None);
    assert_eq!(e.perspective.unwrap().focal_length_y, None);
}

#[test]
fn lcp_fisheye_and_errors() {
    let text = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
      <rdf:Description xmlns:photoshop="http://ns.adobe.com/photoshop/1.0/" xmlns:stCamera="http://ns.adobe.com/photoshop/1.0/camera-profile">
      <photoshop:CameraProfiles><rdf:Seq>
        <rdf:li stCamera:LensPrettyName="Acme Fisheye 8mm f/3.5" stCamera:ApertureValue="4">
          <stCamera:FisheyeModel stCamera:FocalLengthX="0.4" stCamera:RadialDistortParam1="-0.1">
            <stCamera:VignetteModel stCamera:VignetteModelParam1="-0.3"/>
          </stCamera:FisheyeModel>
        </rdf:li>
      </rdf:Seq></photoshop:CameraProfiles></rdf:Description></rdf:RDF></x:xmpmeta>"#;
    let f = LcpFile::parse_str(text, "fisheye.lcp").unwrap();
    // No FocalLength: a prime's name gives it.
    assert_eq!(f.entries[0].focal_length, Some(8.0));
    assert!(f.entries[0].fisheye.is_some() && f.entries[0].perspective.is_none());
    let mut s = shot("FF-1", None, Some("Fisheye 8mm f/3.5"), None);
    s.aperture = Some(4.0);
    let c = f.correction(&s).unwrap();
    assert!(c.distortion.is_none() && c.tca.is_none());
    assert_eq!(
        c.vignetting.unwrap().0,
        Vignetting::Acm {
            k1: -0.3,
            k2: 0.0,
            k3: 0.0
        }
    );
    assert!(LcpFile::parse_str("<x:xmpmeta xmlns:x=\"adobe:ns:meta/\"/>", "e.lcp").is_err());
    assert!(LcpFile::parse_str("not xml <", "e.lcp").is_err());
}

#[test]
fn apex_aperture() {
    assert!(close(apex_to_f_number(5.0), 2f64.powf(2.5)));
    assert!(close(apex_to_f_number(6.0), 8.0));
}

#[test]
fn lcp_interpolation() {
    let f = LcpFile::parse(fixtures().join("lcp").join(ZOOMAR_LCP)).unwrap();
    let s = zoomar("FF-1", 47.0);
    let c = f.correction(&s).unwrap();
    assert_eq!(c.source, Source::Lcp);
    assert!(
        c.description
            .contains("Adobe (Acme Zoomar 24-70mm f/2.8 Pro | A)")
    );
    // Unknown distance: the farthest calibration at each focal length.
    let (d, frame) = c.distortion.unwrap();
    let Distortion::Acm { k } = d else { panic!() };
    assert_close(&k, &[-0.025, 0.005, 0.0, 0.0005, -0.001]);
    let Frame::Acm {
        cx,
        cy,
        fx,
        fy,
        cal_crop,
    } = frame
    else {
        panic!()
    };
    assert_close(&[cx, cy, fx, fy, cal_crop], &[0.501, 0.335, 1.2, 1.2, 1.0]);
    let (t, _) = c.tca.unwrap();
    let Tca::Acm { red, blue } = t else { panic!() };
    assert_close(&red, &[1.0003, 0.002, 0.0, 0.0, 0.0, 0.0]);
    assert_close(&blue, &[0.9997, -0.0005, 0.0, 0.0, 0.00005, 0.0001]);
    assert!(c.vignetting.is_none(), "no aperture, no vignetting");

    // Close focus picks the close calibration at 24 mm.
    let mut near = s.clone();
    near.distance_m = Some(0.4);
    let Distortion::Acm { k } = f.correction(&near).unwrap().distortion.unwrap().0 else {
        panic!()
    };
    assert!(close(k[0], (-0.2 + 0.05) / 2.0));

    // Vignetting at 70 mm, f/4: halfway (in stops) between Av 3 and Av 5; the
    // vignette model's own focal length (1.7) blends with the perspective one (1.8).
    let mut v70 = zoomar("FF-1", 70.0);
    v70.aperture = Some(4.0);
    let (v, frame) = f.correction(&v70).unwrap().vignetting.unwrap();
    let Vignetting::Acm { k1, k2, k3 } = v else {
        panic!()
    };
    assert_close(&[k1, k2, k3], &[-0.2, 0.05, 0.0]);
    let Frame::Acm { fx, .. } = frame else {
        panic!()
    };
    assert!(close(fx, 1.75));

    // Attribute-syntax profile: defaults for missing centre and FocalLengthY.
    let f = LcpFile::parse(fixtures().join("lcp/sub/bolt-sport.lcp")).unwrap();
    let s = shot(
        "Mini 2",
        Some("Bolt"),
        Some("70-200mm F4 Sport"),
        Some(100.0),
    );
    let (d, frame) = f.correction(&s).unwrap().distortion.unwrap();
    let Distortion::Acm { k } = d else { panic!() };
    let t = 30.0 / 65.0;
    assert!(close(k[0], 0.01 + 0.02 * t));
    assert!(close(k[3], 0.0004 * t));
    let Frame::Acm { fy, cal_crop, .. } = frame else {
        panic!()
    };
    assert!(close(fy, 2.1 + (4.0 - 2.1) * t));
    assert_eq!(cal_crop, 1.5);
    let s200 = shot(
        "Mini 2",
        Some("Bolt"),
        Some("70-200mm F4 Sport"),
        Some(250.0),
    );
    let (_, frame) = f.correction(&s200).unwrap().distortion.unwrap();
    let Frame::Acm { cx, cy, fx, fy, .. } = frame else {
        panic!()
    };
    assert_close(&[cx, cy, fx, fy], &[0.5, 1.0 / 3.0, 6.0, 6.0]);
    // Only the red channel is profiled at 70 mm: blue is the identity.
    let s70 = shot(
        "Mini 2",
        Some("Bolt"),
        Some("70-200mm F4 Sport"),
        Some(70.0),
    );
    let Tca::Acm { red, blue } = f.correction(&s70).unwrap().tca.unwrap().0 else {
        panic!()
    };
    assert_close(&red, &[1.001, 0.002, 0.0, 0.0, 0.0, 0.0]);
    assert_close(&blue, &[1.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
}

#[test]
fn lcp_library_scans_and_matches() {
    let lib = lcp_lib();
    assert_eq!(lib.files.len(), 2, "{:?}", lib.files);
    assert!(
        lib.find_by_filename("acme ff-1 (acme zoomar 24-70mm f2.8 pro a) - raw.LCP")
            .is_some()
    );
    assert!(
        lib.find_by_filename(&format!("C:\\Profiles\\{ZOOMAR_LCP}"))
            .is_some()
    );
    assert!(lib.find_by_filename("other.lcp").is_none());

    // By file name (the focal length is in it).
    let m = lib.find(&zoomar("FF-1", 35.0)).unwrap();
    assert!(m.file.path.ends_with(ZOOMAR_LCP));
    assert!(m.score > 1.0, "camera and raw bonuses: {}", m.score);
    // By file contents (the file name has no focal length).
    let m = lib
        .find(&shot(
            "Mini 2",
            Some("Bolt"),
            Some("70-200mm F4 Sport"),
            None,
        ))
        .unwrap();
    assert!(m.file.path.ends_with("bolt-sport.lcp"));
    // Negative cases.
    assert!(
        lib.find(&shot("FF-1", None, Some("Zoomar 24-105mm F4"), None))
            .is_none()
    );
    assert!(lib.find(&shot("FF-1", None, None, None)).is_none());
    assert!(LcpLibrary::scan([fixtures().join("missing")]).is_empty());
}

// ---------------------------------------------------------------- resolve

#[test]
fn resolve_priority() {
    let db = db();
    let lib = lcp_lib();
    let s = zoomar("FF-1", 47.0);
    let none = ProfileHint::default();

    // 1. The LCP the edit names wins, even over a better name match.
    let hint = ProfileHint {
        name: Some("Bolt 70-200mm f/4 Sport & friends".into()),
        filename: Some("BOLT-SPORT.lcp".into()),
    };
    let c = resolve(&s, &hint, Some(&lib), Some(&db)).unwrap();
    assert_eq!(c.source, Source::Lcp);
    assert!(c.file.unwrap().ends_with("bolt-sport.lcp"));

    // 2. Then an LCP matched by lens name (also when the named file is missing).
    let missing = ProfileHint {
        name: None,
        filename: Some("gone.lcp".into()),
    };
    for hint in [&none, &missing] {
        let c = resolve(&s, hint, Some(&lib), Some(&db)).unwrap();
        assert_eq!(c.source, Source::Lcp);
        assert!(c.file.unwrap().ends_with(ZOOMAR_LCP));
    }

    // 3. Then lensfun.
    let c = resolve(&s, &none, None, Some(&db)).unwrap();
    assert_eq!(c.source, Source::Lensfun);
    let c = resolve(&s, &none, Some(&LcpLibrary::default()), Some(&db)).unwrap();
    assert_eq!(c.source, Source::Lensfun);

    // The crop factor comes from the camera database when the caller lacks it.
    let c = resolve(&zoomar("Mini 2", 47.0), &none, None, Some(&db)).unwrap();
    assert!(
        c.description.ends_with("(calibrated at crop 1.5)"),
        "{}",
        c.description
    );

    // 4. Nothing.
    let unknown = shot("FF-1", None, Some("Wonder 35mm f/1.8"), Some(35.0));
    assert!(resolve(&unknown, &none, Some(&lib), Some(&db)).is_none());
    assert!(resolve(&s, &none, None, None).is_none());
}
