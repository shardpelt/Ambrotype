// SPDX-License-Identifier: GPL-3.0-or-later
//! `amb lens`: lens profile sources, download and matching.

use crate::*;

/// The lensfun database and the user's LCP files: `--lens-db` / `--lcp-dir`, else
/// the default locations.
pub(crate) fn lens_sources(
    options: &BTreeMap<&str, &str>,
) -> Result<
    (
        Option<amb_lens::lensfun::LensfunDb>,
        amb_lens::lcp::LcpLibrary,
    ),
    String,
> {
    use amb_lens::{LcpLibrary, LensfunDb};
    let db = match options.get("--lens-db") {
        Some(dir) => {
            let path = Path::new(dir);
            let db = if path.is_file() {
                LensfunDb::load_files([path])
            } else {
                LensfunDb::load_dir(path).map_err(|e| format!("{dir}: {e}"))?
            };
            Some(db)
        }
        None => LensfunDb::load_default(),
    };
    let lcp = match options.get("--lcp-dir") {
        Some(dir) => LcpLibrary::scan([Path::new(dir)]),
        None => LcpLibrary::scan_default(),
    };
    Ok((db, lcp))
}

/// `amb lens fetch`: lensfun's database into the user's updates folder.
pub(crate) fn lens_fetch(options: &BTreeMap<&str, &str>) -> Result<(), String> {
    use amb_library::lensdb;
    let dir = match options.get("--to") {
        Some(d) => PathBuf::from(d),
        None => lensdb::user_dir().ok_or("no home folder; use --to DIR")?,
    };
    let got = match options.get("--from") {
        Some(file) => {
            let bytes = std::fs::read(file).map_err(|e| format!("{file}: {e}"))?;
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs());
            lensdb::install(&bytes, stamp, &dir)?
        }
        None => lensdb::fetch(Some(&dir), options.contains_key("--force"))?,
    };
    let db = amb_lens::lensfun::LensfunDb::load_dir(&got.dir).map_err(|e| e.to_string())?;
    println!(
        "lens database in {}: {} files, {} lenses",
        got.dir.display(),
        got.files,
        db.lenses.len()
    );
    Ok(())
}

/// `amb lens info <RAW>`: the raw's lens metadata, then the same report as `amb lens match`.
pub(crate) fn lens_info(raw: &Path, options: &BTreeMap<&str, &str>) -> Result<String, String> {
    let shot = amb_engine::raw::read_shot(raw).map_err(|e| format!("{}: {e}", raw.display()))?;
    let lens_model = shot
        .lens_model
        .clone()
        .ok_or_else(|| format!("{}: no lens model in the metadata", raw.display()))?;
    let num = |v: Option<f64>| v.map(|v| v.to_string());
    let owned: Vec<(&str, Option<String>)> = vec![
        ("--camera-make", Some(shot.camera_make.clone())),
        ("--camera-model", Some(shot.camera_model.clone())),
        ("--lens-make", shot.lens_make.clone()),
        ("--lens-model", Some(lens_model)),
        ("--focal", num(shot.focal_mm)),
        ("--aperture", num(shot.aperture)),
        ("--distance", num(shot.distance_m)),
    ];
    let mut merged = options.clone();
    for (key, value) in &owned {
        if let Some(v) = value {
            merged.insert(key, v.as_str());
        }
    }
    lens_match_output(&merged)
}

/// `amb lens match`: returns what to print, so it can be tested without a process.
pub(crate) fn lens_match_output(options: &BTreeMap<&str, &str>) -> Result<String, String> {
    use amb_lens::{ProfileHint, ShotInfo};
    use std::fmt::Write;

    let required = |name: &str| {
        options
            .get(name)
            .map(|v| v.to_string())
            .ok_or_else(|| format!("lens match needs {name}"))
    };
    let mut shot = ShotInfo {
        camera_make: required("--camera-make")?,
        camera_model: required("--camera-model")?,
        lens_make: options.get("--lens-make").map(|v| v.to_string()),
        lens_model: Some(required("--lens-model")?),
        focal_mm: number_option(options, "--focal")?,
        aperture: number_option(options, "--aperture")?,
        distance_m: number_option(options, "--distance")?,
        crop_factor: number_option(options, "--crop")?,
        embedded: None,
    };
    let mut out = String::new();

    let (db, lcp) = lens_sources(options)?;
    match &db {
        Some(db) => {
            let dirs: Vec<String> = db
                .files
                .iter()
                .filter_map(|f| f.parent().map(|p| p.display().to_string()))
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect();
            let _ = writeln!(
                out,
                "lensfun: {} files ({} failed), {} cameras, {} lenses from {}",
                db.files.len(),
                db.failed.len(),
                db.cameras.len(),
                db.lenses.len(),
                if dirs.is_empty() {
                    "-".into()
                } else {
                    dirs.join(", ")
                }
            );
            for (_, err) in &db.failed {
                let _ = writeln!(out, "  ! {err}");
            }
        }
        None => {
            let _ = writeln!(
                out,
                "lensfun: no database found (install lensfun, set AMB_LENSFUN_DB or pass --lens-db)"
            );
        }
    }
    let _ = writeln!(out, "LCP: {} profile files", lcp.files.len());

    let mut crop_note = "given";
    if shot.crop_factor.is_none() {
        shot.crop_factor = db
            .as_ref()
            .and_then(|db| db.camera_crop(&shot.camera_make, &shot.camera_model));
        crop_note = if shot.crop_factor.is_some() {
            "lensfun camera database"
        } else {
            "unknown"
        };
    }
    let opt = |v: Option<f64>, unit: &str| v.map_or("-".to_string(), |v| format!("{v}{unit}"));
    let _ = writeln!(
        out,
        "\nshot: {} {} | lens: {} {} | focal {} | f/{} | distance {} | crop {} ({crop_note})",
        shot.camera_make,
        shot.camera_model,
        shot.lens_make.as_deref().unwrap_or("-"),
        shot.lens_model.as_deref().unwrap_or("-"),
        opt(shot.focal_mm, " mm"),
        opt(shot.aperture, ""),
        opt(shot.distance_m, " m"),
        opt(shot.crop_factor, ""),
    );

    let _ = writeln!(out, "\ncandidates:");
    match lcp.find(&shot) {
        Some(m) => {
            let _ = writeln!(
                out,
                "  LCP      {} (score {:.3}) {}",
                m.file.lens_name().unwrap_or("-"),
                m.score,
                m.file.path.display()
            );
        }
        None => {
            let _ = writeln!(out, "  LCP      no match");
        }
    }
    match db.as_ref().and_then(|db| db.find_lens(&shot)) {
        Some(m) => {
            let _ = writeln!(
                out,
                "  lensfun  {} (score {:.3}, calibration crop {}) {}",
                m.lens.model(),
                m.score,
                m.calibration.crop_factor,
                m.calibration.file.display()
            );
        }
        None => {
            let _ = writeln!(out, "  lensfun  no match");
        }
    }

    let Some(c) = amb_lens::resolve(&shot, &ProfileHint::default(), Some(&lcp), db.as_ref()) else {
        let _ = write!(out, "\nprofile: none");
        return Ok(out);
    };
    let _ = writeln!(out, "\nprofile: {:?} | {}", c.source, c.description);
    let _ = writeln!(out, "  file: {}", c.file.as_deref().unwrap_or("-"));
    let line = |name: &str, model: Option<String>, why: &str| match model {
        Some(m) => format!("  {name:<11} {m}"),
        None => format!("  {name:<11} none{why}"),
    };
    let _ = writeln!(
        out,
        "{}",
        line(
            "distortion",
            c.distortion.map(|(d, f)| format!("{d}\n              {f}")),
            ""
        )
    );
    let _ = writeln!(
        out,
        "{}",
        line(
            "tca",
            c.tca.map(|(t, f)| format!("{t}\n              {f}")),
            ""
        )
    );
    let _ = write!(
        out,
        "{}",
        line(
            "vignetting",
            c.vignetting.map(|(v, f)| format!("{v}\n              {f}")),
            if shot.aperture.is_none() {
                " (needs --aperture)"
            } else {
                ""
            }
        )
    );
    Ok(out)
}
