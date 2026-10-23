// SPDX-License-Identifier: GPL-3.0-or-later
//! Library commands: collages, the catalogue, import, Lightroom catalogues, HDR and panorama merges.

use crate::*;

/// `amb import`: plan, print, then copy (unless `dry_run`).
/// `amb collage`: photos, each developed with its own sidecar, on a layout's
/// canvas; pages when there are more photos than cells.
pub(crate) fn collage(
    layout: &str,
    photos: &[&str],
    out: &Path,
    options: &BTreeMap<&str, &str>,
) -> Result<(), String> {
    use amb_export::collage::{self, Canvas, Placement, Template};
    let builtins = collage::builtin_templates();
    let mut template = if layout.ends_with(".json") {
        let text = std::fs::read_to_string(layout).map_err(|e| format!("{layout}: {e}"))?;
        Template::from_json(&text).map_err(|e| format!("{layout}: {e}"))?
    } else if let Some(t) = layout
        .parse::<usize>()
        .ok()
        .and_then(|n| builtins.get(n.wrapping_sub(1)))
    {
        t.clone()
    } else {
        builtins
            .iter()
            .find(|t| t.name.eq_ignore_ascii_case(layout.trim()))
            .cloned()
            .ok_or_else(|| format!("no layout {layout:?}: see amb collage list, or give a .json"))?
    };
    if let Some(size) = options.get("--size") {
        let (w, h) = size
            .split_once(['x', 'X'])
            .and_then(|(w, h)| Some((w.trim().parse().ok()?, h.trim().parse().ok()?)))
            .ok_or_else(|| format!("bad --size {size:?} (e.g. 1080x1350)"))?;
        (template.width, template.height) = (w, h);
    }
    if let Some(bg) = options.get("--background") {
        template.background = bg.to_string();
    }
    for (flag, slot) in [
        ("--gap", &mut template.gap),
        ("--margin", &mut template.margin),
    ] {
        if let Some(v) = options.get(flag) {
            *slot = v
                .parse()
                .map_err(|_| format!("bad {flag} {v:?} (a percentage)"))?;
        }
    }
    template.validate()?;
    let mut export = export_settings(out, options)?;
    export.resize = amb_export::Resize::None;
    // Each photo developed with its own edit, upright.
    let develop = |path: &str| -> Result<amb_engine::buffer::Image, String> {
        let input = Path::new(path);
        let source = amb_engine::source_path(input);
        let mut img =
            amb_engine::raw::load(&source).map_err(|e| format!("{}: {e}", source.display()))?;
        let sidecar = amb_engine::sidecar_path(input);
        let own = sidecar.is_file().then(|| sidecar.display().to_string());
        let (settings, _, _) =
            develop_settings(&mut img, own.as_deref(), None, None, None, options)?;
        let developed = amb_engine::pipeline::render(&img, &settings);
        Ok(amb_export::geometry::orient(
            &developed,
            img.metadata.orientation,
        ))
    };
    let n = template.cells.len();
    let pages = if photos.len() < n {
        // Fewer photos than cells: in order.
        vec![(0..n).map(|i| (i < photos.len()).then_some(i)).collect()]
    } else {
        // The photos' shapes (a quick look at each, without its crop), so
        // they fill the cells that suit them; then page by page.
        let aspects = photos
            .iter()
            .map(|p| {
                let source = amb_engine::source_path(Path::new(p));
                let img = amb_engine::raw::load(&source)
                    .map_err(|e| format!("{}: {e}", source.display()))?;
                let (w, h) = (img.width as f64, img.height.max(1) as f64);
                Ok(if (5..=8).contains(&img.metadata.orientation) {
                    h / w
                } else {
                    w / h
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        collage::pages(&template, &aspects)
    };
    let stem = out.file_stem().unwrap_or_default().to_string_lossy();
    let ext = out.extension().unwrap_or_default().to_string_lossy();
    for (i, page) in pages.iter().enumerate() {
        let mut canvas = Canvas::new(&template);
        for (cell, photo) in page.iter().enumerate() {
            if let Some(p) = photo {
                canvas.place(cell, &develop(photos[*p])?, &Placement::default());
            }
        }
        let path = if pages.len() == 1 {
            out.to_path_buf()
        } else {
            out.with_file_name(format!("{stem}-{:03}.{ext}", i + 1))
        };
        amb_export::export(&canvas.image, &Default::default(), &export, &path)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        println!(
            "wrote {} ({} x {}, {} photo(s))",
            path.display(),
            template.width,
            template.height,
            page.iter().flatten().count()
        );
    }
    Ok(())
}

/// `amb catalog`: the editor's catalogue of what it read from each photo
/// (amb_library::catalog).
pub(crate) fn catalog(
    args: &[&str],
    options: &BTreeMap<&str, &str>,
    json: bool,
) -> Result<(), String> {
    use amb_library::catalog::{self, Catalog, Query};
    let path = match options.get("--catalog") {
        Some(p) => PathBuf::from(p),
        None => catalog::default_path().ok_or("no config folder: give --catalog FILE")?,
    };
    let cat = Catalog::open(&path)?;
    match args {
        ["scan", dirs @ ..] if !dirs.is_empty() => {
            // Absolute, as the editor keeps them.
            let dirs: Vec<PathBuf> = dirs.iter().map(|d| absolute(d)).collect();
            let start = std::time::Instant::now();
            let (photos, read) = cat.scan(&dirs)?;
            let ms = start.elapsed().as_millis();
            if json {
                return print_json(&serde_json::json!({ "photos": photos, "read": read, "ms": ms }));
            }
            println!("{photos} photos, {read} read from their files, in {ms} ms");
            Ok(())
        }
        ["find"] => {
            let query = Query {
                text: options.get("--text").map(|s| s.to_string()),
                camera: options.get("--camera").map(|s| s.to_string()),
                lens: options.get("--lens").map(|s| s.to_string()),
                keyword: options.get("--keyword").map(|s| s.to_string()),
                rating: options
                    .get("--rating")
                    .map(|r| r.parse().map_err(|_| format!("bad --rating {r:?}")))
                    .transpose()?,
                from: options.get("--from").map(|s| s.to_string()),
                until: options.get("--until").map(|s| s.to_string()),
            };
            let found = cat.find(&query);
            if json {
                return print_json(&serde_json::to_value(&found).map_err(|e| e.to_string())?);
            }
            for e in &found {
                let shot = &e.record.shot;
                let stars = "*".repeat(e.record.info.rating.unwrap_or(0).into());
                println!(
                    "{}  {:<5} {}  {}{}",
                    shot.captured.as_deref().unwrap_or("          -        "),
                    stars,
                    shot.camera.as_deref().unwrap_or("-"),
                    e.path.display(),
                    if e.offline { "  (offline)" } else { "" }
                );
            }
            eprintln!("{} found", found.len());
            Ok(())
        }
        ["stats"] => {
            let s = cat.stats();
            if json {
                return print_json(&serde_json::to_value(s).map_err(|e| e.to_string())?);
            }
            println!("{} photos in {} folders ({})", s.photos, s.folders, path.display());
            Ok(())
        }
        ["forget", dir] => {
            let n = cat.forget_folder(&absolute(dir));
            println!("forgot {n} photos (the files stay)");
            Ok(())
        }
        // The library folders the editor keeps indexed (amb_library::roots).
        ["add" | "remove", dir] => {
            let file = roots_file(options)?;
            let folders = if args[0] == "add" {
                amb_library::roots::add(&file, &absolute(dir))?
            } else {
                amb_library::roots::remove(&file, &absolute(dir))?
            };
            for f in &folders {
                println!("{}", f.display());
            }
            Ok(())
        }
        ["index"] => {
            let file = roots_file(options)?;
            let folders = amb_library::roots::load(&file);
            if folders.is_empty() {
                return Err("no library folders yet: amb catalog add <DIR>".into());
            }
            let mut out = Vec::new();
            for f in &folders {
                let r = cat.index(f, &|done, total| {
                    if !json {
                        eprint!("\r{}: {done}/{total}", f.display());
                    }
                })?;
                if !json {
                    eprintln!();
                    if r.offline {
                        println!("{}: can't be reached; its photos stay listed, offline", f.display());
                    } else {
                        println!(
                            "{}: {} photos, {} read from their files, {} gone",
                            f.display(),
                            r.photos,
                            r.read,
                            r.forgotten
                        );
                    }
                }
                out.push(serde_json::json!({ "folder": f, "result": r }));
            }
            if json {
                return print_json(&serde_json::Value::Array(out));
            }
            Ok(())
        }
        ["folders"] => {
            let folders = cat.folders();
            if json {
                return print_json(&serde_json::to_value(&folders).map_err(|e| e.to_string())?);
            }
            for f in &folders {
                let off = if f.offline { "  (offline)" } else { "" };
                println!("{:>7}  {}{off}", f.photos, f.folder.display());
            }
            Ok(())
        }
        _ => Err("catalog: scan <DIR>... | find [--text T] [--camera C] [--lens L] [--keyword K] [--rating N] [--from DATE] [--until DATE] | stats | folders | forget <DIR> | add <DIR> | remove <DIR> | index".into()),
    }
}

/// The library folders list: `--folders FILE`, or the editor's.
fn absolute(p: &str) -> PathBuf {
    std::path::absolute(p).unwrap_or_else(|_| PathBuf::from(p))
}

fn roots_file(options: &BTreeMap<&str, &str>) -> Result<PathBuf, String> {
    match options.get("--folders") {
        Some(p) => Ok(PathBuf::from(p)),
        None => amb_library::roots::default_path()
            .ok_or_else(|| "no config folder: give --folders FILE".into()),
    }
}

/// `amb lrcat import`: a Lightroom Classic catalogue into sidecars and
/// collections.
pub(crate) fn lrcat_import(
    catalog: &Path,
    options: &BTreeMap<&str, &str>,
    json: bool,
) -> Result<(), String> {
    use amb_library::lrcat;
    let mut opts = lrcat::Options::default();
    if let Some(r) = options.get("--remap") {
        let (from, to) = r
            .split_once('=')
            .ok_or_else(|| format!("bad --remap {r:?} (OLD=NEW)"))?;
        opts.remap.push((from.to_string(), to.to_string()));
    }
    let dir = match options.get("--collections") {
        Some(d) => PathBuf::from(d),
        None => amb_library::collections::editor_dir()
            .ok_or("no config folder for collections: give --collections DIR")?,
    };
    let apply = options.contains_key("--apply");
    let plan = lrcat::plan(catalog, &opts)?;
    if json && !apply {
        return print_json(&serde_json::to_value(&plan).map_err(|e| e.to_string())?);
    }
    if !json {
        for w in &plan.warnings {
            eprintln!("warning: {w}");
        }
        for p in &plan.photos {
            let why = match p.action {
                "keep" => " (a sidecar is already there)",
                "missing" => " (photo not found)",
                _ => "",
            };
            println!("{:<7} {}{why}", p.action, p.sidecar.display());
            if let Some(problem) = &p.problem {
                println!("        {problem}");
            }
        }
        for c in &plan.collections {
            let set = c
                .set
                .as_deref()
                .map(|s| format!(" in {s}"))
                .unwrap_or_default();
            println!("collection {:?}{set}: {} photo(s)", c.name, c.photos.len());
        }
        if !plan.smart_collections.is_empty() {
            println!(
                "smart collections not moved (make them again in the editor): {}",
                plan.smart_collections.join(", ")
            );
        }
        println!(
            "catalogue version {}: {} sidecar(s) to write, {} kept, {} photo(s) not found, {} collection(s){}",
            plan.version.as_deref().unwrap_or("unknown"),
            plan.count("write"),
            plan.count("keep"),
            plan.count("missing"),
            plan.collections.len(),
            if apply {
                String::new()
            } else {
                " (nothing written: --apply writes them)".into()
            }
        );
    }
    if !apply {
        return Ok(());
    }
    let report = lrcat::apply(&plan, &dir);
    if json {
        print_json(&serde_json::to_value(&report).map_err(|e| e.to_string())?)?;
    } else {
        println!(
            "wrote {} sidecar(s), kept {}, {} not found; {} collection(s) in {}",
            report.written,
            report.kept,
            report.missing,
            report.collections,
            dir.display()
        );
        for e in &report.errors {
            eprintln!("error: {e}");
        }
    }
    if report.errors.is_empty() {
        Ok(())
    } else {
        Err(format!("{} problem(s), see above", report.errors.len()))
    }
}

pub(crate) fn import(
    source: &Path,
    dest: &Path,
    options: &BTreeMap<&str, &str>,
    dry_run: bool,
    json: bool,
) -> Result<(), String> {
    use amb_library::import::{self, ImportOptions};
    let d = ImportOptions::default();
    let opts = ImportOptions {
        folder: options.get("--into").map_or(d.folder, |s| s.to_string()),
        name: options.get("--rename").map_or(d.name, |s| s.to_string()),
        first_number: match options.get("--start") {
            Some(n) => n
                .parse()
                .map_err(|_| format!("bad --start {n:?} (a whole number)"))?,
            None => d.first_number,
        },
        subfolders: !options.contains_key("--no-subfolders"),
        keywords: options
            .get("--keywords")
            .map(|k| k.split(',').map(|s| s.trim().to_string()).collect())
            .unwrap_or_default(),
        preset: options.get("--preset").map(PathBuf::from),
        backup: options.get("--backup").map(PathBuf::from),
    };
    let plan = import::plan(source, dest, &opts)?;
    if json && dry_run {
        return print_json(&serde_json::to_value(&plan).map_err(|e| e.to_string())?);
    }
    let new = plan.iter().filter(|p| p.skip.is_none()).count();
    if !json {
        for p in &plan {
            let (src, dst) = &p.files[0];
            let extra = p.files.len() - 1;
            let with = if extra > 0 {
                format!(" (+{extra})")
            } else {
                String::new()
            };
            match &p.skip {
                Some(why) => println!("skip  {}: {why}", src.display()),
                None => println!("copy  {}{with} -> {}", src.display(), dst.display()),
            }
        }
        println!(
            "{new} to import, {} already there{}",
            plan.len() - new,
            if dry_run {
                " (dry run: nothing copied)"
            } else {
                ""
            }
        );
    }
    if dry_run {
        return Ok(());
    }
    let report = import::run(&plan, &opts, &mut |_, _, _| {});
    if json {
        print_json(&serde_json::to_value(&report).map_err(|e| e.to_string())?)?;
    } else {
        println!("imported {}, skipped {}", report.imported, report.skipped);
        for e in &report.errors {
            eprintln!("error: {e}");
        }
    }
    if report.errors.is_empty() {
        Ok(())
    } else {
        Err(format!("{} problem(s), see above", report.errors.len()))
    }
}

/// `amb merge hdr`: bracketed raws into one floating-point DNG.
pub(crate) fn merge_hdr(
    raws: &[&str],
    options: &BTreeMap<&str, &str>,
    json: bool,
) -> Result<(), String> {
    use amb_library::merge::{self, Deghost, HdrOptions};
    let deghost = match options.get("--deghost") {
        Some(d) => Deghost::parse(d)
            .ok_or_else(|| format!("bad --deghost {d:?} (off, low, medium or high)"))?,
        None => Deghost::Off,
    };
    let opts = HdrOptions {
        align: !options.contains_key("--no-align"),
        deghost,
    };
    let photos: Vec<PathBuf> = raws.iter().map(PathBuf::from).collect();
    let report = merge::hdr(&photos, &opts, options.get("--out").map(Path::new))?;
    if json {
        return print_json(&serde_json::to_value(&report).map_err(|e| e.to_string())?);
    }
    for (i, p) in photos.iter().enumerate() {
        let (dx, dy) = report.shifts[i];
        let role = if *p == report.reference {
            "  (reference)"
        } else {
            ""
        };
        println!(
            "{}: {:+.2} EV, shifted {dx}, {dy} px{role}",
            p.display(),
            report.stops[i]
        );
    }
    if opts.deghost != Deghost::Off {
        println!(
            "deghosting took {:.1}% of the photo from one exposure",
            report.ghost_share * 100.0
        );
    }
    println!(
        "wrote {} ({} x {})",
        report.output.display(),
        report.width,
        report.height
    );
    Ok(())
}

/// `amb merge pano`: overlapping raws stitched into a panorama DNG.
pub(crate) fn merge_pano(
    raws: &[&str],
    options: &BTreeMap<&str, &str>,
    json: bool,
) -> Result<(), String> {
    use amb_library::merge::{self, Lenses, PanoOptions, Projection};
    let projection = match options.get("--projection") {
        Some(p) => Projection::parse(p).ok_or_else(|| {
            format!("bad --projection {p:?} (auto, spherical, cylindrical or perspective)")
        })?,
        None => Projection::Auto,
    };
    let opts = PanoOptions {
        projection,
        auto_crop: !options.contains_key("--no-crop"),
    };
    let (db, lcp) = lens_sources(options)?;
    let lenses = Lenses {
        lcp: Some(&lcp),
        db: db.as_ref(),
        correct: !options.contains_key("--no-lens"),
    };
    let photos: Vec<PathBuf> = raws.iter().map(PathBuf::from).collect();
    let report = merge::pano(&photos, &opts, options.get("--out").map(Path::new), lenses)?;
    if json {
        return print_json(&serde_json::to_value(&report).map_err(|e| e.to_string())?);
    }
    for (i, p) in photos.iter().enumerate() {
        let lens = report.lens_profiles[i]
            .as_deref()
            .unwrap_or("no lens profile");
        let role = if *p == report.reference {
            "  (reference)"
        } else {
            ""
        };
        println!(
            "{}: {lens}, brightness x{:.3}{role}",
            p.display(),
            report.gains[i]
        );
    }
    for &(a, b, n) in &report.overlaps {
        println!(
            "{} overlaps {}: {n} matched points",
            photos[a].display(),
            photos[b].display()
        );
    }
    println!(
        "{} projection, {:.0}° x {:.0}°, points meet within {:.2} px",
        report.projection.name(),
        report.field.0,
        report.field.1,
        report.rms
    );
    println!(
        "wrote {} ({} x {})",
        report.output.display(),
        report.width,
        report.height
    );
    Ok(())
}
