//! Matches against a real lensfun database. Not run by default (the database
//! is not part of the repository):
//! `FLB_LENSFUN_DB=/usr/share/lensfun/version_2 cargo test -p flb-lens -- --ignored`

use flb_lens::{LensfunDb, ShotInfo};

fn shot(make: &str, model: &str, lens_make: &str, lens_model: &str, focal: f64) -> ShotInfo {
    ShotInfo {
        camera_make: make.into(),
        camera_model: model.into(),
        lens_make: Some(lens_make.into()),
        lens_model: Some(lens_model.into()),
        focal_mm: Some(focal),
        aperture: Some(5.6),
        distance_m: None,
        crop_factor: None,
    }
}

#[test]
#[ignore = "needs FLB_LENSFUN_DB pointing at a lensfun database directory"]
fn real_world_matches() {
    let dir = std::env::var("FLB_LENSFUN_DB").expect("set FLB_LENSFUN_DB");
    let db = LensfunDb::load_dir(&dir).expect("load database");
    println!(
        "loaded {} files ({} failed): {} cameras, {} lenses",
        db.files.len(),
        db.failed.len(),
        db.cameras.len(),
        db.lenses.len()
    );
    let cases = [
        (
            shot(
                "NIKON CORPORATION",
                "NIKON D750",
                "Tamron",
                "SP 35mm f/1.8 Di VC USD (F012)",
                35.0,
            ),
            "Tamron SP 35mm f/1.8 Di VC USD F012",
            1.0,
        ),
        (
            shot(
                "Canon",
                "Canon EOS R10",
                "Sigma",
                "150-600mm f/5-6.3 DG OS HSM | C",
                300.0,
            ),
            "Sigma 150-600mm f/5-6.3 DG OS HSM | C",
            1.613,
        ),
        (
            shot(
                "Nikon",
                "D750",
                "Nikon",
                "AF-S Zoom-Nikkor 24-70mm f/2.8G ED",
                50.0,
            ),
            "Nikon AF-S Zoom-Nikkor 24-70mm f/2.8G ED",
            1.0,
        ),
    ];
    for (shot, model, crop) in cases {
        let m = db
            .find_lens(&shot)
            .unwrap_or_else(|| panic!("no match for {shot:?}"));
        let c = db.correction(&m, &shot);
        println!(
            "{} {} + {:?} @ {:?} mm -> {} (score {:.3}, image crop {}, calibration crop {})\n  {}\n  distortion {:?}\n  tca {:?}\n  vignetting {:?}",
            shot.camera_make,
            shot.camera_model,
            shot.lens_model,
            shot.focal_mm,
            m.lens.model(),
            m.score,
            m.image_crop,
            m.calibration.crop_factor,
            c.description,
            c.distortion,
            c.tca,
            c.vignetting
        );
        assert_eq!(m.lens.model(), model);
        assert!((m.calibration.crop_factor - crop).abs() < 1e-9);
        assert!(c.distortion.is_some());
    }
}

/// Prints how common EXIF spellings match; for eyeballing, no assertions.
#[test]
#[ignore = "needs FLB_LENSFUN_DB pointing at a lensfun database directory"]
fn exif_name_variants() {
    let dir = std::env::var("FLB_LENSFUN_DB").expect("set FLB_LENSFUN_DB");
    let db = LensfunDb::load_dir(&dir).expect("load database");
    for (make, model, lens_model, focal) in [
        (
            "Canon",
            "Canon EOS 5D Mark III",
            "EF24-70mm f/2.8L II USM",
            35.0,
        ),
        (
            "Canon",
            "Canon EOS 80D",
            "EF-S18-55mm f/3.5-5.6 IS STM",
            35.0,
        ),
        ("SONY", "ILCE-7M3", "FE 24-70mm F2.8 GM", 35.0),
        ("FUJIFILM", "X-T3", "XF35mmF1.4 R", 35.0),
        (
            "NIKON CORPORATION",
            "NIKON D800",
            "24.0-70.0 mm f/2.8",
            35.0,
        ),
        (
            "NIKON CORPORATION",
            "NIKON D800",
            "AF-S Nikkor 24-70mm f/2.8E ED VR",
            35.0,
        ),
        ("Canon", "Canon PowerShot G7 X", "", 10.0),
        (
            "Canon",
            "Canon EOS R10",
            "RF-S18-150mm F3.5-6.3 IS STM",
            35.0,
        ),
        ("Canon", "Canon EOS R10", "Made-up 23mm f/1.1 Wonder", 23.0),
    ] {
        let shot = ShotInfo {
            camera_make: make.into(),
            camera_model: model.into(),
            lens_model: (!lens_model.is_empty()).then(|| lens_model.into()),
            focal_mm: Some(focal),
            ..ShotInfo::default()
        };
        match db.find_lens(&shot) {
            Some(m) => println!(
                "{model} + {lens_model:?} -> {} (score {:.3}, calibration crop {})",
                m.lens.model(),
                m.score,
                m.calibration.crop_factor
            ),
            None => println!("{model} + {lens_model:?} -> no match"),
        }
    }
}
