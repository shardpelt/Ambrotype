// SPDX-License-Identifier: GPL-3.0-or-later
//! `amb render`: develop and export one photo or many, and the camera profiles it uses.

use crate::*;

/// Camera profiles: `--dcp-dir`, or `AMB_DCP_DIRS` and Camera Raw's folders.
pub(crate) fn profile_library(
    options: &BTreeMap<&str, &str>,
) -> amb_engine::profile::ProfileLibrary {
    use amb_engine::profile::ProfileLibrary;
    match options.get("--dcp-dir") {
        Some(dir) => ProfileLibrary::scan(&[PathBuf::from(dir)]),
        None => ProfileLibrary::load_default(),
    }
}

/// `amb profile list`: the profiles for a raw's camera.
pub(crate) fn profile_list(raw: &Path, options: &BTreeMap<&str, &str>) -> Result<(), String> {
    let source = amb_engine::source_path(raw);
    let (_, meta) = amb_engine::raw::read_metadata(&source)
        .map_err(|e| format!("{}: {e}", source.display()))?;
    let lib = profile_library(options);
    let found = lib.for_camera(&meta.make, &meta.model);
    println!(
        "{} {}: {} profile(s) among {} .dcp file(s)",
        meta.make,
        meta.model,
        found.len(),
        lib.files.len()
    );
    for f in found {
        println!("  {}  ({})", f.name, f.path.display());
    }
    Ok(())
}

pub(crate) fn render(
    input: &Path,
    out: &Path,
    preset: Option<&str>,
    amount: Option<&str>,
    look: Option<&str>,
    calibration: Option<&str>,
    options: &BTreeMap<&str, &str>,
) -> Result<(), String> {
    use amb_engine::{pipeline, raw};
    let start = std::time::Instant::now();
    // A virtual copy (IMG.copy-2.xmp) renders its photo.
    let source = amb_engine::source_path(input);
    let mut img = raw::load(&source).map_err(|e| format!("{}: {e}", source.display()))?;
    let wb = if img.rendered {
        "rendered image: relative white balance (IncrementalTemperature/Tint)".to_string()
    } else {
        let (t, tint) = pipeline::as_shot_temp_tint(&img);
        format!("as shot: {t:.0} K, tint {tint:+.0}")
    };
    // Masks and the crop use the stored orientation, so say when it isn't upright.
    let turned = match img.metadata.orientation {
        2 => " | stored mirrored",
        3 => " | stored upside down",
        4 => " | stored mirrored upside down",
        5 => " | stored turned and mirrored",
        6 => " | stored turned left (shown turned 90° clockwise)",
        7 => " | stored turned and mirrored",
        8 => " | stored turned right (shown turned 90° counter-clockwise)",
        _ => "",
    };
    println!(
        "{} {} | {}x{} | {wb}{turned}",
        img.make, img.model, img.width, img.height
    );
    // The photo's own edit, as Lightroom exports it, unless a preset is given.
    let sidecar = amb_engine::sidecar_path(input);
    let own = (preset.is_none() && sidecar.is_file()).then(|| sidecar.display().to_string());
    let preset = preset.or(own.as_deref());
    let (develop, not_rendered, notes) =
        develop_settings(&mut img, preset, amount, look, calibration, options)?;
    for n in notes {
        println!("{n}");
    }
    let export = export_settings(out, options)?;
    let developed = pipeline::render(&img, &develop);
    let report = amb_export::export(&developed, &img.metadata, &export, out)
        .map_err(|e| format!("{}: {e}", out.display()))?;
    println!(
        "wrote {} ({}, {}x{}, {}, {}, {:.1} MB) in {:.2}s",
        out.display(),
        match report.format {
            amb_export::Format::Jpeg | amb_export::Format::Avif => format!(
                "{} q{}",
                report.format.extension().to_uppercase(),
                report.quality
            ),
            f => f.extension().to_uppercase(),
        },
        report.width,
        report.height,
        match report.bit_depth {
            amb_export::BitDepth::Eight => "8-bit",
            amb_export::BitDepth::Sixteen => "16-bit",
            amb_export::BitDepth::Float32 => "32-bit float",
        },
        report
            .color_space
            .profile_name()
            .trim_end_matches(" (Ambrotype)"),
        report.bytes as f64 / 1e6,
        start.elapsed().as_secs_f64()
    );
    if !not_rendered.is_empty() {
        println!(
            "not rendered yet ({}): {}",
            not_rendered.len(),
            not_rendered.join(", ")
        );
    }
    Ok(())
}

/// The develop settings for `img` from a preset or sidecar (at `amount`),
/// with its lens and camera profiles found; what wasn't rendered, and notes
/// on what was used.
#[allow(clippy::type_complexity)]
pub(crate) fn develop_settings(
    img: &mut amb_engine::raw::CameraImage,
    preset: Option<&str>,
    amount: Option<&str>,
    look: Option<&str>,
    calibration: Option<&str>,
    options: &BTreeMap<&str, &str>,
) -> Result<
    (
        amb_engine::pipeline::DevelopSettings,
        Vec<String>,
        Vec<String>,
    ),
    String,
> {
    let mut notes = Vec::new();
    let mut settings = std::collections::BTreeMap::new();
    if let Some(path) = preset {
        let preset = load(Path::new(path))?;
        let amount = match amount {
            Some(a) => {
                a.trim_end_matches('%')
                    .parse::<f64>()
                    .map_err(|_| format!("bad --amount {a:?}"))?
                    / 100.0
            }
            None => 1.0,
        };
        settings =
            amb_presets::amount::apply(&settings, &preset.settings, amount, Registry::builtin());
        let file = Path::new(path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        notes.push(match preset.name() {
            Some(name) => format!("preset: {name} at {:.0}%", amount * 100.0),
            None => format!("edit: {file} at {:.0}%", amount * 100.0),
        });
    }
    let (develop, not_rendered, more) = develop_from(img, &settings, look, calibration, options)?;
    notes.extend(more);
    Ok((develop, not_rendered, notes))
}

/// [`develop_settings`] from a settings map.
#[allow(clippy::type_complexity)]
pub(crate) fn develop_from(
    img: &mut amb_engine::raw::CameraImage,
    settings: &amb_engine::matching::Settings,
    look: Option<&str>,
    calibration: Option<&str>,
    options: &BTreeMap<&str, &str>,
) -> Result<
    (
        amb_engine::pipeline::DevelopSettings,
        Vec<String>,
        Vec<String>,
    ),
    String,
> {
    use amb_engine::pipeline;
    let mut notes = Vec::new();
    let (mut develop, not_rendered) = pipeline::DevelopSettings::from_settings(settings);
    develop.base_look = match look.unwrap_or("default") {
        "default" => pipeline::BaseLook::Default,
        "linear" => pipeline::BaseLook::Linear,
        other => return Err(format!("unknown --look {other:?} (default, linear)")),
    };
    if let Some(path) = calibration {
        develop.calibration =
            amb_engine::calibration::Calibration::from_json_file(Path::new(path))?;
    }
    if develop.lens.profile_enable || develop.lens.auto_ca {
        let (db, lcp) = lens_sources(options)?;
        if img.shot.crop_factor.is_none() {
            img.shot.crop_factor = db
                .as_ref()
                .and_then(|db| db.camera_crop(&img.shot.camera_make, &img.shot.camera_model));
        }
        let hint = amb_lens::ProfileHint {
            name: develop.lens.profile_name.clone(),
            filename: develop.lens.profile_filename.clone(),
        };
        let lens = img.shot.lens_model.as_deref().unwrap_or("unknown lens");
        match amb_lens::resolve(&img.shot, &hint, Some(&lcp), db.as_ref()) {
            Some(c) => {
                notes.push(format!("lens profile: {}", c.description));
                develop.lens.profile = Some(c);
            }
            None if develop.lens.profile_enable => notes.push(format!(
                "lens profile: none found for {lens} ({}); profile corrections skipped",
                if db.is_none() && lcp.files.is_empty() {
                    "no lensfun database or LCP files found, see `amb lens match`"
                } else {
                    "not in the lensfun database or your LCP files"
                }
            )),
            None => {}
        }
    }
    if let Some(name) = develop.profile_name.clone() {
        if !img.rendered {
            let lib = profile_library(options);
            develop.camera_profile = lib.find(&img.make, &img.model, &name);
        }
        match (&develop.camera_profile, develop.profile_note()) {
            (Some(p), _) => notes.push(format!("camera profile: {} ({})", p.name, p.camera)),
            (None, Some(note)) => notes.push(format!("camera profile: {note}")),
            _ => {}
        }
    }
    Ok((develop, not_rendered, notes))
}

/// Parse an optional numeric option.
pub(crate) fn number_option(
    options: &BTreeMap<&str, &str>,
    name: &str,
) -> Result<Option<f64>, String> {
    options
        .get(name)
        .map(|v| {
            v.trim()
                .parse::<f64>()
                .ok()
                .filter(|n| n.is_finite() && *n > 0.0)
                .ok_or_else(|| format!("bad {name} {v:?} (a positive number)"))
        })
        .transpose()
}

/// Export settings from the output path and the export options.
/// `amb render A.nef B.nef ... --out-dir DIR`: each raw to DIR/<name>.<ext>.
pub(crate) fn render_many(
    inputs: &[&str],
    dir: &Path,
    options: &BTreeMap<&str, &str>,
) -> Result<(), String> {
    let ext = match options.get("--format") {
        Some(f) => amb_export::Format::from_name(f)
            .ok_or_else(|| format!("unknown --format {f:?}"))?
            .extension(),
        None => "jpg",
    };
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let template = options.get("--name").copied();
    if let Some(t) = template {
        amb_library::import::check_template(t)?;
    }
    let mut failed = Vec::new();
    let mut taken: Vec<String> = Vec::new();
    for (i, input) in inputs.iter().enumerate() {
        let input = Path::new(input);
        let stem = match template {
            Some(t) => amb_library::import::file_name(t, input, i as u32 + 1)?,
            None => input
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("photo")
                .to_string(),
        };
        // Two photos with one name (a template without {name} or {seq}) both stay.
        let mut name = stem.clone();
        let mut n = 2;
        while taken.contains(&name) {
            name = format!("{stem}-{n}");
            n += 1;
        }
        taken.push(name.clone());
        let out = dir.join(format!("{name}.{ext}"));
        print!("[{}/{}] ", i + 1, inputs.len());
        let result = render(
            input,
            &out,
            options.get("--preset").copied(),
            options.get("--amount").copied(),
            options.get("--look").copied(),
            options.get("--calibration").copied(),
            options,
        );
        // One bad file doesn't stop the batch.
        if let Err(e) = result {
            println!("failed: {e}");
            failed.push(input.display().to_string());
        }
    }
    if failed.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{} of {} failed: {}",
            failed.len(),
            inputs.len(),
            failed.join(", ")
        ))
    }
}

pub(crate) fn export_settings(
    out: &Path,
    options: &BTreeMap<&str, &str>,
) -> Result<amb_export::ExportSettings, String> {
    use amb_export::{BitDepth, ExportSettings, Format, MetadataPolicy, Resize, Subsampling};
    let format = match options.get("--format") {
        Some(f) => Format::from_name(f).ok_or_else(|| format!("unknown --format {f:?}"))?,
        None => Format::from_path(out).ok_or_else(|| {
            format!(
                "{}: unknown output type (use .jpg, .png, .tif, .webp, .avif, .exr or .jxl, or --format)",
                out.display()
            )
        })?,
    };
    let mut s = ExportSettings::new(format);
    let int = |key: &str| -> Result<Option<u32>, String> {
        options
            .get(key)
            .map(|v| v.parse::<u32>().map_err(|_| format!("bad {key} {v:?}")))
            .transpose()
    };
    let float = |key: &str| -> Result<Option<f64>, String> {
        options
            .get(key)
            .map(|v| v.parse::<f64>().map_err(|_| format!("bad {key} {v:?}")))
            .transpose()
    };
    if let Some(q) = int("--quality")? {
        s.quality = q.clamp(1, 100) as u8;
    }
    if let Some(b) = int("--bits")? {
        s.bit_depth =
            Some(BitDepth::from_bits(b).ok_or_else(|| format!("bad --bits {b} (8, 16 or 32)"))?);
    }
    if let Some(cs) = options.get("--color-space") {
        s.color_space = amb_color::space::OutputSpace::from_id(cs).ok_or_else(|| {
            format!("unknown --color-space {cs:?} (srgb, p3, adobergb, prophoto, rec2020)")
        })?;
    }
    if let Some(ss) = options.get("--subsampling") {
        s.subsampling = Some(match ss.replace(':', "").as_str() {
            "444" => Subsampling::S444,
            "422" => Subsampling::S422,
            "420" => Subsampling::S420,
            _ => return Err(format!("bad --subsampling {ss:?} (444, 422, 420)")),
        });
    }
    s.progressive = options.contains_key("--progressive");
    s.enlarge = options.contains_key("--enlarge");
    let sizes = [
        int("--long-edge")?.map(Resize::LongEdge),
        int("--short-edge")?.map(Resize::ShortEdge),
        int("--width")?.map(Resize::Width),
        int("--height")?.map(Resize::Height),
        float("--megapixels")?.map(Resize::Megapixels),
        float("--percent")?.map(Resize::Percent),
        options
            .get("--fit")
            .map(|v| {
                v.split_once(['x', 'X'])
                    .and_then(|(w, h)| Some(Resize::Fit(w.parse().ok()?, h.parse().ok()?)))
                    .ok_or_else(|| format!("bad --fit {v:?} (e.g. 2048x2048)"))
            })
            .transpose()?,
        options
            .get("--exact")
            .map(|v| {
                v.split_once(['x', 'X'])
                    .and_then(|(w, h)| Some(Resize::Exact(w.parse().ok()?, h.parse().ok()?)))
                    .filter(|r| !matches!(r, Resize::Exact(0, _) | Resize::Exact(_, 0)))
                    .ok_or_else(|| format!("bad --exact {v:?} (e.g. 1080x1350)"))
            })
            .transpose()?,
    ];
    let mut chosen = sizes.into_iter().flatten();
    if let Some(r) = chosen.next() {
        if chosen.next().is_some() {
            return Err("use only one of --long-edge, --short-edge, --width, --height, --fit, --exact, --megapixels, --percent".into());
        }
        s.resize = r;
    }
    if let Some(sh) = options.get("--sharpen") {
        s.sharpen = Some(amb_export::parse_sharpen(sh).ok_or_else(|| {
            format!("bad --sharpen {sh:?} (screen|matte|glossy[:low|standard|high])")
        })?);
    }
    if let Some(m) = options.get("--metadata") {
        s.metadata = MetadataPolicy::from_id(m)
            .ok_or_else(|| format!("bad --metadata {m:?} (all, no-location, copyright, none)"))?;
    }
    if let Some(size) = options.get("--max-size") {
        let lower = size.to_ascii_lowercase();
        let (num, mult) = if let Some(n) = lower.strip_suffix(['k']) {
            (n, 1e3)
        } else if let Some(n) = lower.strip_suffix(['m']) {
            (n, 1e6)
        } else {
            (lower.as_str(), 1.0)
        };
        let bytes = num
            .trim_end_matches('b')
            .parse::<f64>()
            .map_err(|_| format!("bad --max-size {size:?} (e.g. 500k, 2M)"))?
            * mult;
        if format != Format::Jpeg {
            return Err("--max-size works with JPEG only".into());
        }
        s.max_bytes = Some(bytes as u64);
    }
    if let Some(speed) = int("--avif-speed")? {
        s.avif_speed = speed.clamp(1, 10) as u8;
    }
    if let Some(r) = options.get("--rating") {
        let r: u8 = r
            .parse()
            .ok()
            .filter(|r| (1..=5).contains(r))
            .ok_or_else(|| format!("bad --rating {r:?} (1 to 5)"))?;
        s.tags.rating = Some(r);
    }
    if let Some(k) = options.get("--keywords") {
        s.tags.keywords = k
            .split(',')
            .map(str::trim)
            .filter(|k| !k.is_empty())
            .map(String::from)
            .collect();
    }
    if let Some(w) = options.get("--watermark") {
        use amb_export::watermark::{Anchor, Watermark};
        let lower = w.to_lowercase();
        let mut mark = if lower.ends_with(".json") {
            let text = std::fs::read_to_string(w).map_err(|e| format!("{w}: {e}"))?;
            serde_json::from_str::<Watermark>(&text).map_err(|e| format!("{w}: {e}"))?
        } else if lower.ends_with(".png") {
            Watermark {
                image: Some(PathBuf::from(w)),
                ..Watermark::text("")
            }
        } else {
            Watermark::text(w)
        };
        if let Some(a) = options.get("--watermark-at") {
            mark.anchor =
                Anchor::from_name(a).ok_or_else(|| format!("unknown --watermark-at {a:?}"))?;
        }
        let number = |key: &str| -> Result<Option<f64>, String> {
            options
                .get(key)
                .map(|v| {
                    v.parse::<f64>()
                        .map_err(|_| format!("{key} {v:?} is not a number"))
                })
                .transpose()
        };
        if let Some(v) = number("--watermark-size")? {
            mark.size = v;
        }
        if let Some(v) = number("--watermark-opacity")? {
            mark.opacity = v;
        }
        if let Some(v) = number("--watermark-rotate")? {
            mark.rotate = v;
        }
        if options.contains_key("--watermark-tile") {
            mark.tile = true;
        }
        mark.validate()?;
        s.watermark = Some(mark);
    }
    if let Some(b) = options.get("--border") {
        s.border = Some(if b.ends_with(".json") {
            let text = std::fs::read_to_string(b).map_err(|e| format!("{b}: {e}"))?;
            amb_export::border::Border::from_json(&text).map_err(|e| format!("{b}: {e}"))?
        } else {
            amb_export::border::Border::from_spec(b)?
        });
    }
    if let Some(c) = options.get("--border-caption") {
        let b = s
            .border
            .as_mut()
            .ok_or("--border-caption needs a --border with room at the bottom")?;
        let text = if c.trim().is_empty() {
            amb_export::border::DEFAULT_CAPTION
        } else {
            c
        };
        b.caption = Some(amb_export::border::Caption::new(text));
        // A frame of one width gets a deeper bottom for the caption.
        if b.bottom.is_none() {
            b.bottom = Some(b.width * 3.0);
        }
        b.validate()?;
    }
    Ok(s)
}
