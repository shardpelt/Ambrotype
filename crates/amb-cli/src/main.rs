// SPDX-License-Identifier: GPL-3.0-or-later
//! `amb`: Ambrotype command-line tool.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

mod mcp;
mod plugin;

use amb_presets::Preset;
use amb_presets::registry::Registry;
use amb_presets::report::{self, Report};

const USAGE: &str = "\
amb: Ambrotype command-line tool
Copyright (C) 2026 The Ambrotype contributors. Free software under the GNU GPL,
version 3 or later, with ABSOLUTELY NO WARRANTY: https://github.com/shardpelt/Ambrotype

USAGE:
    amb preset inspect <FILE>... [--json]   Show what a preset contains and how it maps to our support
    amb preset dump <FILE>                  Print the parsed settings as JSON
    amb preset census <PATH>... [--json]    Count tag usage across a preset collection (dirs are scanned recursively)
    amb sweep plan [--groups G,..]          List the Lightroom calibration sweeps
    amb sweep generate --out <DIR> [--groups G,..] [--synthetic P,..] [<RAW>...]
                                            Write a sweep kit for importing into Lightroom
                                            (docs/lightroom-trial-plan.md): raw copies + XMP sidecars,
                                            and/or synthetic DNG targets with embedded settings
    amb calibration defaults                Print the default model constants as calibration JSON
    amb testimage <PATTERN> <OUT.dng>       Write one synthetic target (ramp, hue, edges, colorchecker,
                                            negative: a scanned colour negative)
    amb render <RAW> <OUT.jpg|.png|.tif|.webp|.avif|.exr|.jxl> [--preset P] [--amount A]
                                            [--look default|linear] [--calibration fitted.json]
                                            [--lens-db DIR] [--lcp-dir DIR] [export options]
                                            Develop a raw with its own sidecar (or --preset, a preset or
                                            another sidecar), export it, and report which settings are not
                                            rendered yet
    amb render <RAW>... --out-dir DIR [--format jpeg|...] [--name TEMPLATE] [same options]
                                            The same for many raws (e.g. a folder: photos/*.NEF), with the
                                            same preset and export options; one failure doesn't stop the rest.
                                            --name names the files as Import does: {name} {yyyy} {mm} {dd}
                                            {hh} {min} {ss} {seq} {camera}, e.g. trip_{seq}
    amb do <PHOTO|SIDECAR.xmp> \"<EDITS>\" [--dry-run] [--json]
                                            Edit in the compact syntax as one undoable step and print
                                            only what changed, e.g. \"exp=+0.5 con+=10 hl=-40 reset:hsl
                                            preset:film.xmp@50\" (Name=v, Name+=d, Name-=d, reset:X,
                                            preset:P[@amount], snap:N, load:N, undo, redo; short names
                                            temp tint exp con hl sh wh bl tex cla deh vib sat sharp nr
                                            cnr vig grain; docs/ai/core.md)
    amb edit <SIDECAR.xmp> --commands <CMDS.json|-> [--out <NEW.xmp|.json>] [--dry-run] [--brief] [--json]
                                            Apply protocol commands (set_param, adjust_param, reset_param,
                                            reset_group, apply_preset, add_mask, set_mask, paint_mask,
                                            remove_mask, duplicate_mask, move_mask, add_spot, set_spot, remove_spot, undo, redo)
                                            as one transaction,
                                            print the diff and write the sidecar (other XMP metadata kept)
    amb match <PHOTO> <REF>... [--apply] [--parts wb,tone,color,grading,hsl] [--out PRESET.xmp] [--json]
                                            Find the settings that make the photo look like the reference
                                            images (JPEGs, or raws with their edits): white balance, tone,
                                            Vibrance/Saturation, Color Grading tints and the Color Mixer
                                            (HSL per colour), solved on small
                                            renders. Shows the changes; --apply makes them one undoable
                                            step, --out also writes them as a preset
    amb auto <PHOTO> [--tone] [--wb] [--apply] [--out PRESET.xmp] [--json]
                                            Lightroom-style Auto: the tone sliders (and Vibrance for dull
                                            colour) and the white balance (as Auto), from the photo's own
                                            look; --tone or --wb for one of them. Shows the changes;
                                            --apply makes them one undoable step
    amb stats <PHOTO> [--preset P] [--amount A] [--json]
                                            The photo developed small (with its edit, or a preset) as a
                                            few numbers instead of a picture: L* percentiles, clipped
                                            shadows and highlights, midtone colour cast, chroma
    amb query <SIDECAR.xmp> --queries <QUERIES.json|->
                                            Answer protocol queries (get_param, list_params, describe,
                                            list_groups, list_masks, list_spots) as JSON
    amb protocol schema [command|query]     Print the JSON Schema of commands and queries
    amb negative analyze <RAW>... [--slides] [--out ROLL.xmp] [--json]
                                            Estimate film base, levels and midtone of scanned negatives
                                            (the median over all frames, for a consistent roll) and write
                                            them as a preset with Auto off; --slides for slide film
    amb negative stocks [--json]            List the built-in film stock looks
    amb lens info <RAW> [--lens-db DIR] [--lcp-dir DIR]
                                            Show a raw's lens metadata and the profile it gets
    amb lens match --camera-make X --camera-model Y --lens-model M [--lens-make L] [--focal F]
                   [--aperture A] [--distance D] [--crop C] [--lens-db DIR] [--lcp-dir DIR]
                                            Find the lens profile for a shot (Adobe .lcp files you have,
                                            then the lensfun database; default locations or
                                            AMB_LCP_DIRS / AMB_LENSFUN_DB) and print the models
                                            interpolated to its focal length, aperture and distance
    amb lens fetch [--to DIR] [--from version_2.tar.bz2] [--force]
                                            Download lensfun's lens database (CC BY-SA 3.0) from its
                                            update server into ~/.local/share/lensfun/updates, where it is
                                            found by default; --from installs a downloaded copy
    amb import <SOURCE> <DEST> [--into \"{yyyy}/{yyyy}-{mm}-{dd}\"] [--rename \"{name}\"] [--start 1]
               [--keywords \"a, b\"] [--preset LOOK.xmp] [--backup DIR] [--no-subfolders] [--dry-run] [--json]
                                            Copy photos from a card or folder into DEST, sorted into
                                            folders and renamed from templates ({name} {yyyy} {yy} {mm}
                                            {dd} {hh} {min} {ss} {seq} {camera}); a raw's JPEG and .xmp
                                            keep its new name; photos already there are skipped. --preset
                                            gives photos without an edit of their own a look; --backup
                                            also copies the originals to DIR/Imported on YYYY-MM-DD
    amb collage list                        The built-in collage layouts
    amb collage <LAYOUT> <PHOTO>... --out OUT.jpg [--size WxH] [--background #fff] [--gap 1.5]
                [--margin 3] [export options]
                                            Several photos (each with its own edit) on one canvas, as
                                            the editor's collage maker does: LAYOUT is a built-in's
                                            name or number, or a layout .json. Photos fill the cells by
                                            shape; more photos than cells make pages, OUT-001.jpg, ...
    amb catalog scan <DIR>...               Catalogue the photos in DIR and its subfolders: what the
                                            library reads from each (rating, keywords, capture time,
                                            camera, lens, ...) is kept until the files change, so big
                                            folders open quickly. Deleting the catalogue loses nothing
    amb catalog find [--text T] [--camera C] [--lens L] [--keyword K] [--rating N]
                [--from DATE] [--until DATE] [--json]
                                            Search every catalogued photo, newest first
    amb catalog stats | forget <DIR>        How many photos it knows; forget a folder (files stay).
                                            --catalog FILE uses another catalogue than the editor's
    amb lrcat import <CATALOG.lrcat> [--apply] [--remap OLD=NEW] [--collections DIR] [--json]
                                            Move from Lightroom Classic: each photo's develop settings,
                                            rating, label, flag, keywords, stack, title and caption go into
                                            its XMP sidecar, virtual copies become ours, and collections
                                            (in their sets) the editor's. Shows the plan; --apply writes it.
                                            The catalogue is only read (close Lightroom first); sidecars
                                            already there are kept. --remap /Volumes/Photos/=/mnt/photos/
                                            finds photos that have moved
    amb merge hdr <RAW>... [--out OUT.dng] [--no-align] [--deghost low|medium|high] [--json]
                                            Merge bracketed raws of one scene into a floating-point DNG
                                            (default: <first>-HDR.dng next to them) that opens and edits
                                            like a raw, here and in Lightroom; Auto Tone it with
                                            amb plugin run plugins/auto-tone OUT.dng
    amb merge pano <RAW>... [--out OUT.dng] [--projection auto|spherical|cylindrical|perspective]
                   [--no-crop] [--no-lens] [--lens-db PATH] [--lcp-dir DIR] [--json]
                                            Stitch overlapping raws (camera turned about the lens) into
                                            a panorama DNG (default: <first>-Pano.dng next to them),
                                            each corrected with its lens profile first; --no-crop keeps
                                            the uncovered edges (black)
    amb mcp                                 Serve the Model Context Protocol over stdio, so your own AI app
                                            (Claude Desktop, Claude Code, ...) can edit, preview and export
                                            photos with Ambrotype's commands (docs/ai/mcp.md)
    amb profile list <RAW> [--dcp-dir DIR]  List the camera profiles (.dcp) on this computer for a raw's
                                            camera (Camera Raw's profile folders, AMB_DCP_DIRS, --dcp-dir);
                                            a preset's or sidecar's CameraProfile picks one by name
    amb ref [GROUP]                         The core card for AI clients (docs/ai/core.md: the compact
                                            syntax and the most used settings), or one group's settings
                                            one line each
    amb settings reference [--json]         Print the settings reference (docs/ai/settings-reference.md)
    amb plugin new <DIR> [--name \"My plugin\"]
                                            Start a plugin project (Rust, built to WebAssembly;
                                            docs/ai/plugins.md)
    amb plugin build <DIR>                  Build it (needs: rustup target add wasm32-unknown-unknown),
                                            then check it
    amb plugin check <DIR>                  Validate plugin.json and the module, and run it on a sample
    amb plugin run <DIR> <PHOTO|SIDECAR.xmp> [--params a=1,b=Film] [--dry-run] [--json]
                                            Run a plugin on a photo's edit and write its sidecar

EXPORT OPTIONS (amb render):
    --format jpeg|png|tiff|webp|avif|exr|jxl  Default: from the file extension
    --quality 1-100                         JPEG/AVIF quality (default 90)
    --bits 8|16|32                          PNG 8/16, TIFF 8/16/32 (float); default 8, TIFF 16
    --color-space srgb|p3|adobergb|prophoto|rec2020   Default srgb; an ICC profile is embedded
    --long-edge N | --short-edge N | --width N | --height N | --fit WxH | --megapixels M | --percent P
    --exact WxH                             Exactly W×H (e.g. 1080x1350): centre-cropped if the aspect differs
    --enlarge                              Allow upscaling (default: never enlarge)
    --sharpen screen|matte|glossy[:low|standard|high]   Output sharpening
    --metadata all|no-location|copyright|none           EXIF to include (default no-location)
    --max-size 500k|2M                      JPEG: lower the quality until the file fits
    --subsampling 444|422|420  --progressive  --avif-speed 1-10
    --rating 1-5  --keywords \"beach, sunset\"   Put a rating and keywords in the file (XMP, IPTC in JPEG,
                                            EXIF), with the all / no-location metadata policies
    --border 5%|40px[,#ffffff|auto] | border.json   A frame around the photo; a .json preset can add
                                            a deeper bottom, a keyline, rounded corners, and fit (with
                                            --exact: the whole photo inside the frame, not cropped)
    --border-caption \"{camera} · {exposure}\"   A line of text in the border's bottom (made three
                                            times the width deep if not set): {camera} {lens}
                                            {exposure} {focal} {aperture} {shutter} {iso} {date}
                                            {year} {title} {creator} {copyright}
    --watermark \"© Name\" | logo.png | watermark.json   Text or a logo (PNG, its transparency kept) on
                                            the photo; --watermark-at bottom-right (top-left, top, top-right,
                                            left, center, right, bottom-left, bottom), --watermark-size 20
                                            (% of the width), --watermark-opacity 70, --watermark-rotate 30
                                            (degrees), --watermark-tile (repeat it across the photo)

Supported inputs: .xmp presets and sidecars, .lrtemplate presets, and files with an embedded
XMP packet (e.g. .dng presets for Lightroom mobile). AI and script guide: docs/ai/README.md.";

/// Camera profiles: `--dcp-dir`, or `AMB_DCP_DIRS` and Camera Raw's folders.
fn profile_library(options: &BTreeMap<&str, &str>) -> amb_engine::profile::ProfileLibrary {
    use amb_engine::profile::ProfileLibrary;
    match options.get("--dcp-dir") {
        Some(dir) => ProfileLibrary::scan(&[PathBuf::from(dir)]),
        None => ProfileLibrary::load_default(),
    }
}

/// `amb profile list`: the profiles for a raw's camera.
fn profile_list(raw: &Path, options: &BTreeMap<&str, &str>) -> Result<(), String> {
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

/// `amb import`: plan, print, then copy (unless `dry_run`).
/// `amb collage`: photos, each developed with its own sidecar, on a layout's
/// canvas; pages when there are more photos than cells.
fn collage(
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
fn catalog(args: &[&str], options: &BTreeMap<&str, &str>, json: bool) -> Result<(), String> {
    use amb_library::catalog::{self, Catalog, Query};
    let path = match options.get("--catalog") {
        Some(p) => PathBuf::from(p),
        None => catalog::default_path().ok_or("no config folder: give --catalog FILE")?,
    };
    let cat = Catalog::open(&path)?;
    match args {
        ["scan", dirs @ ..] if !dirs.is_empty() => {
            let dirs: Vec<PathBuf> = dirs.iter().map(PathBuf::from).collect();
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
                    "{}  {:<5} {}  {}",
                    shot.captured.as_deref().unwrap_or("          -        "),
                    stars,
                    shot.camera.as_deref().unwrap_or("-"),
                    e.path.display()
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
            let n = cat.forget_folder(Path::new(dir));
            println!("forgot {n} photos (the files stay)");
            Ok(())
        }
        _ => Err("catalog: scan <DIR>... | find [--text T] [--camera C] [--lens L] [--keyword K] [--rating N] [--from DATE] [--until DATE] | stats | forget <DIR>".into()),
    }
}

/// `amb lrcat import`: a Lightroom Classic catalogue into sidecars and
/// collections.
fn lrcat_import(catalog: &Path, options: &BTreeMap<&str, &str>, json: bool) -> Result<(), String> {
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

fn import(
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
fn merge_hdr(raws: &[&str], options: &BTreeMap<&str, &str>, json: bool) -> Result<(), String> {
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
fn merge_pano(raws: &[&str], options: &BTreeMap<&str, &str>, json: bool) -> Result<(), String> {
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

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut json = false;
    let mut dry_run = false;
    let mut brief = false;
    let mut options: BTreeMap<&str, &str> = BTreeMap::new();
    let mut positional: Vec<&str> = Vec::new();
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--json" => json = true,
            "--dry-run" => dry_run = true,
            "--brief" => brief = true,
            "--progressive" | "--enlarge" | "--slides" | "--force" | "--no-subfolders"
            | "--no-align" | "--no-crop" | "--no-lens" | "--apply" | "--tone" | "--wb"
            | "--watermark-tile" => {
                options.insert(arg.as_str(), "true");
            }
            "--out"
            | "--groups"
            | "--synthetic"
            | "--preset"
            | "--amount"
            | "--look"
            | "--calibration"
            | "--commands"
            | "--queries"
            | "--camera-make"
            | "--camera-model"
            | "--lens-make"
            | "--lens-model"
            | "--focal"
            | "--aperture"
            | "--distance"
            | "--crop"
            | "--lens-db"
            | "--lcp-dir"
            | "--format"
            | "--quality"
            | "--bits"
            | "--color-space"
            | "--subsampling"
            | "--long-edge"
            | "--short-edge"
            | "--width"
            | "--height"
            | "--fit"
            | "--exact"
            | "--megapixels"
            | "--percent"
            | "--sharpen"
            | "--metadata"
            | "--max-size"
            | "--avif-speed"
            | "--border"
            | "--border-caption"
            | "--out-dir"
            | "--watermark"
            | "--watermark-at"
            | "--watermark-size"
            | "--watermark-opacity"
            | "--watermark-rotate"
            | "--rating"
            | "--keywords"
            | "--name"
            | "--params"
            | "--into"
            | "--rename"
            | "--start"
            | "--to"
            | "--from"
            | "--dcp-dir"
            | "--deghost"
            | "--projection"
            | "--parts"
            | "--remap"
            | "--backup"
            | "--size"
            | "--background"
            | "--gap"
            | "--margin"
            | "--collections"
            | "--camera"
            | "--lens"
            | "--keyword"
            | "--text"
            | "--until"
            | "--catalog" => match iter.next() {
                Some(value) => {
                    options.insert(arg.as_str(), value.as_str());
                }
                None => {
                    eprintln!("error: {arg} needs a value\n\n{USAGE}");
                    return ExitCode::from(2);
                }
            },
            other if other.starts_with("--") => {
                eprintln!("error: unknown option {other}\n\n{USAGE}");
                return ExitCode::from(2);
            }
            other => positional.push(other),
        }
    }
    let groups = options
        .get("--groups")
        .map(|g| g.split(',').map(str::trim).collect::<Vec<_>>());
    let result = match positional.as_slice() {
        ["preset", "inspect", files @ ..] if !files.is_empty() => inspect(files, json),
        ["preset", "dump", file] => dump(file),
        ["preset", "census", paths @ ..] if !paths.is_empty() => census(paths, json),
        ["sweep", "plan"] => sweep_plan(groups.as_deref()),
        ["sweep", "generate", raws @ ..] => match options.get("--out") {
            Some(out) => sweep_generate(
                raws,
                options.get("--synthetic").copied(),
                Path::new(out),
                groups.as_deref(),
            ),
            None => Err("sweep generate needs --out <DIR>".into()),
        },
        ["testimage", pattern, out] => testimage(pattern, Path::new(out)),
        ["profile", "list", raw] => profile_list(Path::new(raw), &options),
        ["mcp"] => mcp::serve(),
        ["collage", "list"] => {
            for (i, t) in amb_export::collage::builtin_templates().iter().enumerate() {
                println!("{:>2}  {} ({} photos)", i + 1, t.name, t.cells.len());
            }
            Ok(())
        }
        ["collage", layout, photos @ ..] if !photos.is_empty() => match options.get("--out") {
            Some(out) => collage(layout, photos, Path::new(out), &options),
            None => Err("collage needs --out <FILE>".into()),
        },
        ["lrcat", "import", catalog] => lrcat_import(Path::new(catalog), &options, json),
        ["catalog", rest @ ..] => catalog(rest, &options, json),
        ["merge", "hdr", raws @ ..] if raws.len() >= 2 => merge_hdr(raws, &options, json),
        ["merge", "pano", raws @ ..] if raws.len() >= 2 => merge_pano(raws, &options, json),
        ["import", source, dest] => {
            import(Path::new(source), Path::new(dest), &options, dry_run, json)
        }
        ["calibration", "defaults"] => {
            println!(
                "{}",
                amb_engine::calibration::Calibration::default().to_json()
            );
            Ok(())
        }
        ["render", inputs @ ..] if !inputs.is_empty() && options.contains_key("--out-dir") => {
            render_many(inputs, Path::new(options["--out-dir"]), &options)
        }
        ["render", input, out] => render(
            Path::new(input),
            Path::new(out),
            options.get("--preset").copied(),
            options.get("--amount").copied(),
            options.get("--look").copied(),
            options.get("--calibration").copied(),
            &options,
        ),
        ["edit", sidecar] => match options.get("--commands") {
            Some(commands) => edit(
                Path::new(sidecar),
                commands,
                options.get("--out").map(Path::new),
                dry_run,
                if json {
                    Output::Json
                } else if brief {
                    Output::Brief
                } else {
                    Output::Text
                },
            ),
            None => Err("edit needs --commands <file.json> (or - for stdin)".into()),
        },
        ["do", target, edits @ ..] if !edits.is_empty() => {
            do_edits(Path::new(target), &edits.join(" "), dry_run, json)
                .map(|text| println!("{text}"))
        }
        ["match", photo, refs @ ..] if !refs.is_empty() => match_look(
            Path::new(photo),
            refs,
            &options,
            options.contains_key("--apply"),
            json,
        )
        .map(|t| println!("{t}")),
        ["auto", photo] => auto(
            Path::new(photo),
            &options,
            options.contains_key("--apply"),
            json,
        )
        .map(|t| println!("{t}")),
        ["stats", photo] => stats(Path::new(photo), &options, json).map(|t| println!("{t}")),
        ["query", state] => match options.get("--queries") {
            Some(queries) => query(Path::new(state), queries),
            None => Err("query needs --queries <file.json> (or - for stdin)".into()),
        },
        ["negative", "analyze", raws @ ..] if !raws.is_empty() => negative_analyze(
            raws,
            options.get("--out").map(Path::new),
            options.contains_key("--slides"),
            json,
        ),
        ["negative", "stocks"] => {
            negative_stocks(json);
            Ok(())
        }
        ["lens", "info", raw] => lens_info(Path::new(raw), &options).map(|text| println!("{text}")),
        ["lens", "match"] => lens_match_output(&options).map(|text| println!("{text}")),
        ["lens", "fetch"] => lens_fetch(&options),
        ["protocol", "schema"] => print_json(&amb_protocol::schema::protocol_schema()),
        ["protocol", "schema", "command"] => print_json(&amb_protocol::schema::command_schema()),
        ["protocol", "schema", "query"] => print_json(&amb_protocol::schema::query_schema()),
        ["plugin", "new", dir] => plugin::new(Path::new(dir), options.get("--name").copied()),
        ["plugin", "build", dir] => plugin::build(Path::new(dir)),
        ["plugin", "check", dir] => plugin::check(Path::new(dir)),
        ["plugin", "run", dir, target] => plugin::run(
            Path::new(dir),
            Path::new(target),
            options.get("--params").copied(),
            dry_run,
            json,
        )
        .map(|text| println!("{text}")),
        ["ref"] => {
            print!("{}", amb_protocol::card::core());
            Ok(())
        }
        ["ref", group] => match amb_protocol::card::group(group) {
            Some(page) => {
                print!("{page}");
                Ok(())
            }
            None => Err(format!(
                "no group {group:?}; groups: {}",
                amb_protocol::catalog::Catalog::builtin()
                    .groups()
                    .iter()
                    .map(|g| g.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
        },
        ["settings", "reference"] => {
            if json {
                print_json(&amb_protocol::reference::json())
            } else {
                print!("{}", amb_protocol::reference::markdown());
                Ok(())
            }
        }
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("error: {msg}");
            ExitCode::FAILURE
        }
    }
}

fn print_json(value: &serde_json::Value) -> Result<(), String> {
    println!(
        "{}",
        serde_json::to_string_pretty(value).map_err(|e| e.to_string())?
    );
    Ok(())
}

/// Read a JSON argument file, or stdin for `-`.
fn read_input(path: &str) -> Result<String, String> {
    if path == "-" {
        let mut text = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut text)
            .map_err(|e| format!("stdin: {e}"))?;
        Ok(text)
    } else {
        std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))
    }
}

/// `amb edit`: returns what to print, so it can be tested without a process.
/// How an edit's report is printed.
#[derive(Clone, Copy, PartialEq)]
enum Output {
    Text,
    /// Net changes only, one line each.
    Brief,
    Json,
}

fn report_output(
    report: &amb_protocol::edit::EditReport,
    output: Output,
) -> Result<String, String> {
    match output {
        Output::Json => serde_json::to_string_pretty(&report.to_json()).map_err(|e| e.to_string()),
        Output::Brief => Ok(report.to_brief()),
        Output::Text => Ok(report.to_text().trim_end().to_string()),
    }
}

fn edit_output(
    sidecar: &Path,
    commands: &str,
    out: Option<&Path>,
    dry_run: bool,
    output: Output,
) -> Result<String, String> {
    let report = amb_protocol::edit::run(&amb_protocol::edit::EditRequest {
        sidecar,
        commands,
        out,
        dry_run,
    })
    .map_err(|e| e.to_string())?;
    report_output(&report, output)
}

fn edit(
    sidecar: &Path,
    commands: &str,
    out: Option<&Path>,
    dry_run: bool,
    output: Output,
) -> Result<(), String> {
    let commands = read_input(commands)?;
    println!("{}", edit_output(sidecar, &commands, out, dry_run, output)?);
    Ok(())
}

/// The sidecar `amb do` edits: an `.xmp` itself, or the photo's.
fn edit_target(target: &Path) -> PathBuf {
    let is_xmp = target
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("xmp"));
    if is_xmp {
        target.to_path_buf()
    } else {
        amb_engine::sidecar_path(target)
    }
}

/// `amb do`: a compact edit line, with a brief report.
fn do_edits(target: &Path, edits: &str, dry_run: bool, json: bool) -> Result<String, String> {
    let sidecar = edit_target(target);
    let report = amb_protocol::edit::run_compact(&amb_protocol::edit::EditRequest {
        sidecar: &sidecar,
        commands: edits,
        out: None,
        dry_run,
    })
    .map_err(|e| e.to_string())?;
    report_output(&report, if json { Output::Json } else { Output::Brief })
}

fn query(state: &Path, queries: &str) -> Result<(), String> {
    let queries = read_input(queries)?;
    let answers = amb_protocol::edit::run_queries(state, &queries).map_err(|e| e.to_string())?;
    print_json(&answers)
}

fn load(path: &Path) -> Result<Preset, String> {
    Preset::from_path(path).map_err(|e| format!("{}: {e}", path.display()))
}

fn inspect(files: &[&str], json: bool) -> Result<(), String> {
    let registry = Registry::builtin();
    let mut reports = Vec::new();
    for file in files {
        let preset = load(Path::new(file))?;
        let report = report::inspect(&preset, registry);
        if json {
            reports.push(serde_json::json!({ "file": file, "report": report }));
        } else {
            print_report(file, &report);
        }
    }
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&reports).map_err(|e| e.to_string())?
        );
    }
    Ok(())
}

fn print_report(file: &str, r: &Report) {
    println!("== {file}");
    println!(
        "   {:?} | name: {} | group: {} | process version: {} | amount slider: {}",
        r.format,
        r.name.as_deref().unwrap_or("-"),
        r.group.as_deref().unwrap_or("-"),
        r.process_version.as_deref().unwrap_or("-"),
        if r.supports_amount { "yes" } else { "no" },
    );
    let mut by_group: BTreeMap<&str, Vec<&report::TagEntry>> = BTreeMap::new();
    for t in &r.tags {
        by_group.entry(t.group.as_str()).or_default().push(t);
    }
    for (group, tags) in by_group {
        let items: Vec<String> = tags
            .iter()
            .map(|t| format!("{}={}", t.name, t.value))
            .collect();
        println!("   [{group}] {}", items.join("  "));
    }
    let counts: Vec<String> = r.counts.iter().map(|(k, v)| format!("{k}: {v}")).collect();
    println!("   status: {}", counts.join(", "));
    for w in &r.warnings {
        println!("   ! {w}");
    }
    println!();
}

fn dump(file: &str) -> Result<(), String> {
    let preset = load(Path::new(file))?;
    println!(
        "{}",
        serde_json::to_string_pretty(&preset).map_err(|e| e.to_string())?
    );
    Ok(())
}

fn collect_files(path: &Path, out: &mut Vec<PathBuf>) {
    if path.is_dir() {
        let Ok(entries) = std::fs::read_dir(path) else {
            return;
        };
        let mut entries: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
        entries.sort();
        for entry in entries {
            collect_files(&entry, out);
        }
    } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        if matches!(
            ext.to_ascii_lowercase().as_str(),
            "xmp" | "lrtemplate" | "dng"
        ) {
            out.push(path.to_path_buf());
        }
    }
}

fn census(paths: &[&str], json: bool) -> Result<(), String> {
    let registry = Registry::builtin();
    let mut files = Vec::new();
    for p in paths {
        collect_files(Path::new(p), &mut files);
    }
    let mut tag_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut format_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut pv_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut warning_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut failures = Vec::new();
    let mut parsed = 0usize;
    for file in &files {
        match Preset::from_path(file) {
            Ok(preset) => {
                parsed += 1;
                *format_counts
                    .entry(format!("{:?}", preset.format))
                    .or_default() += 1;
                *pv_counts
                    .entry(preset.process_version().unwrap_or("none").to_string())
                    .or_default() += 1;
                for name in preset.settings.keys() {
                    *tag_counts.entry(name.clone()).or_default() += 1;
                }
                for w in report::inspect(&preset, registry).warnings {
                    // Bucket warnings by their leading words so counts aggregate.
                    let key: String = w.split_whitespace().take(3).collect::<Vec<_>>().join(" ");
                    *warning_counts.entry(key).or_default() += 1;
                }
            }
            Err(e) => failures.push(format!("{}: {e}", file.display())),
        }
    }
    let mut ranked: Vec<(&String, &usize)> = tag_counts.iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));

    if json {
        let tags: Vec<serde_json::Value> = ranked
            .iter()
            .map(|(name, n)| {
                let info = registry.lookup(name);
                serde_json::json!({
                    "tag": name, "presets": n,
                    "group": info.as_ref().map(|i| i.group.clone()),
                    "status": info.map(|i| i.status),
                })
            })
            .collect();
        let out = serde_json::json!({
            "files": files.len(), "parsed": parsed, "formats": format_counts,
            "process_versions": pv_counts, "warnings": warning_counts,
            "tags": tags, "failures": failures,
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&out).map_err(|e| e.to_string())?
        );
        return Ok(());
    }

    println!(
        "files: {}  parsed: {parsed}  failed: {}",
        files.len(),
        failures.len()
    );
    println!("formats: {format_counts:?}");
    println!("process versions: {pv_counts:?}");
    println!(
        "\n{:>6}  {:>5}  {:<14} {:<18} tag",
        "count", "%", "group", "status"
    );
    for (name, n) in &ranked {
        let info = registry.lookup(name);
        let (group, status) = match &info {
            Some(i) => (i.group.clone(), format!("{:?}", i.status)),
            None => ("unknown".into(), "-".into()),
        };
        let pct = 100.0 * **n as f64 / parsed.max(1) as f64;
        println!("{n:>6}  {pct:>5.1}  {group:<14} {status:<18} {name}");
    }
    if !warning_counts.is_empty() {
        println!("\nwarnings:");
        for (w, n) in &warning_counts {
            println!("{n:>6}  {w}…");
        }
    }
    for f in &failures {
        eprintln!("failed: {f}");
    }
    Ok(())
}

fn selected_plan(groups: Option<&[&str]>) -> Result<Vec<amb_testkit::Sweep>, String> {
    let plan = amb_testkit::default_plan();
    let Some(groups) = groups else {
        return Ok(plan);
    };
    let known: Vec<&str> = plan.iter().map(|s| s.group).collect();
    if let Some(bad) = groups.iter().find(|g| !known.contains(g)) {
        let mut known = known;
        known.dedup();
        return Err(format!(
            "unknown sweep group {bad:?}; known: {}",
            known.join(", ")
        ));
    }
    Ok(plan
        .into_iter()
        .filter(|s| groups.contains(&s.group))
        .collect())
}

fn sweep_plan(groups: Option<&[&str]>) -> Result<(), String> {
    let plan = selected_plan(groups)?;
    let mut steps = 0;
    for sweep in &plan {
        let values: Vec<String> = sweep.values.iter().map(|v| format!("{v}")).collect();
        let extra: Vec<String> = sweep
            .extra
            .iter()
            .map(|(k, v)| {
                format!(
                    "{k}={}",
                    v.as_f64()
                        .map_or_else(|| v.as_str().unwrap_or("").to_string(), |n| n.to_string())
                )
            })
            .collect();
        println!(
            "{:<14} {:<28} [{}]{}",
            sweep.group,
            sweep.tag,
            values.join(", "),
            if extra.is_empty() {
                String::new()
            } else {
                format!("  with {}", extra.join(", "))
            }
        );
        steps += sweep.values.len();
    }
    println!(
        "\n{} sweeps, {steps} steps per input image (+1 baseline)",
        plan.len()
    );
    Ok(())
}

fn sweep_generate(
    raws: &[&str],
    synthetic: Option<&str>,
    out: &Path,
    groups: Option<&[&str]>,
) -> Result<(), String> {
    let plan = selected_plan(groups)?;
    let mut inputs: Vec<amb_testkit::Input> = raws
        .iter()
        .map(|r| amb_testkit::Input::Raw(PathBuf::from(r)))
        .collect();
    if let Some(patterns) = synthetic {
        inputs.extend(
            patterns
                .split(',')
                .map(|p| amb_testkit::Input::Synthetic(p.trim().to_string())),
        );
    }
    if inputs.is_empty() {
        return Err("give raw files and/or --synthetic <patterns>".into());
    }
    let summary = amb_testkit::generate(&inputs, &plan, out).map_err(|e| e.to_string())?;
    println!("wrote {} files to {}", summary.files, out.display());
    println!("manifest: {}", summary.manifest.display());
    println!(
        "next: import the folder into Lightroom Classic (Add, with 'read metadata'), then batch-export"
    );
    Ok(())
}

fn testimage(pattern: &str, out: &Path) -> Result<(), String> {
    let image = amb_testkit::patterns::generate(pattern).ok_or_else(|| {
        format!(
            "unknown pattern {pattern:?}; known: {}",
            amb_testkit::patterns::NAMES.join(", ")
        )
    })?;
    amb_testkit::dng::write(out, &image, &Default::default())
        .map_err(|e| format!("{}: {e}", out.display()))?;
    println!(
        "wrote {}x{} {pattern} target to {}",
        image.width,
        image.height,
        out.display()
    );
    Ok(())
}

fn render(
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
fn develop_settings(
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
fn develop_from(
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

/// `amb stats`: the photo developed small, as numbers.
fn stats(input: &Path, options: &BTreeMap<&str, &str>, json: bool) -> Result<String, String> {
    use amb_engine::raw;
    let source = amb_engine::source_path(input);
    let img = raw::load(&source).map_err(|e| format!("{}: {e}", source.display()))?;
    let mut img = raw::downscale(&img, 768);
    let sidecar = amb_engine::sidecar_path(input);
    let edit = options
        .get("--preset")
        .map(|p| p.to_string())
        .or_else(|| sidecar.is_file().then(|| sidecar.display().to_string()));
    let (develop, not_rendered, _) = develop_settings(
        &mut img,
        edit.as_deref(),
        options.get("--amount").copied(),
        None,
        None,
        options,
    )?;
    let stats = amb_engine::stats::measure(&amb_engine::pipeline::render(&img, &develop));
    if json {
        let mut v = serde_json::to_value(&stats).map_err(|e| e.to_string())?;
        v["not_rendered"] = serde_json::json!(not_rendered);
        return serde_json::to_string_pretty(&v).map_err(|e| e.to_string());
    }
    let mut text = stats.to_brief();
    if !not_rendered.is_empty() {
        text += &format!("\nnot rendered: {}", not_rendered.join(", "));
    }
    Ok(text)
}

/// `amb match`: the settings that make `photo` look like the references.
fn match_look(
    photo: &Path,
    refs: &[&str],
    options: &BTreeMap<&str, &str>,
    apply: bool,
    json: bool,
) -> Result<String, String> {
    use amb_engine::matching::{self, MatchOptions, Part};
    use amb_library::look;
    let parts = match options.get("--parts") {
        Some(list) => list
            .split(',')
            .map(|p| {
                Part::parse(p.trim())
                    .ok_or_else(|| format!("unknown part {p:?} (wb, tone, color, grading, hsl)"))
            })
            .collect::<Result<Vec<_>, _>>()?,
        None => Part::ALL.to_vec(),
    };
    let refs_paths: Vec<&Path> = refs.iter().map(Path::new).collect();
    let target = look::target_look(&refs_paths)?;
    let result = solve_on_photo(photo, options, |img, base, develop| {
        let opts = MatchOptions {
            parts,
            ..Default::default()
        };
        matching::solve(img, base, &target, &opts, develop)
    })?;
    let extra = serde_json::json!({ "references": refs });
    apply_result(photo, "Match look", &result, extra, options, apply, json)
}

/// `amb auto`: Lightroom-style Auto tone and white balance.
fn auto(
    photo: &Path,
    options: &BTreeMap<&str, &str>,
    apply: bool,
    json: bool,
) -> Result<String, String> {
    let (only_tone, only_wb) = (options.contains_key("--tone"), options.contains_key("--wb"));
    let (tone, white) = if only_tone || only_wb {
        (only_tone, only_wb)
    } else {
        (true, true)
    };
    let result = solve_on_photo(photo, options, |img, base, develop| {
        amb_engine::matching::auto(img, base, tone, white, develop)
    })?;
    apply_result(
        photo,
        "Auto",
        &result,
        serde_json::json!({}),
        options,
        apply,
        json,
    )
}

/// Run a look solver on the photo developed small with its edit, its lens
/// and camera profiles found once for every try.
fn solve_on_photo(
    photo: &Path,
    options: &BTreeMap<&str, &str>,
    solve: impl FnOnce(
        &amb_engine::raw::CameraImage,
        &amb_engine::matching::Settings,
        &dyn Fn(&amb_engine::matching::Settings) -> amb_engine::pipeline::DevelopSettings,
    ) -> amb_engine::matching::MatchResult,
) -> Result<amb_engine::matching::MatchResult, String> {
    use amb_engine::matching;
    use amb_library::look;
    let source = amb_engine::source_path(photo);
    let img = amb_engine::raw::load(&source).map_err(|e| format!("{}: {e}", source.display()))?;
    let mut img = amb_engine::raw::downscale(&img, look::EDGE);
    let base = look::photo_settings(photo)?;
    let (base_dev, _, _) = develop_from(&mut img, &base, None, None, options)?;
    let develop = |s: &matching::Settings| {
        let mut d = matching::develop_plain(s);
        d.lens.profile = base_dev.lens.profile.clone();
        d.camera_profile = base_dev.camera_profile.clone();
        d
    };
    Ok(solve(&img, &base, &develop))
}

/// Show a solver's changes, and with `apply` write them to the photo's
/// edit as one undo step called `label`; `--out` also saves them as a preset.
fn apply_result(
    photo: &Path,
    label: &str,
    result: &amb_engine::matching::MatchResult,
    mut extra: serde_json::Value,
    options: &BTreeMap<&str, &str>,
    apply: bool,
    json: bool,
) -> Result<String, String> {
    let commands = amb_library::look::commands(&result.changes);
    if let Some(out) = options.get("--out") {
        let mut preset = amb_protocol::DevelopState::new();
        preset.execute_all(&commands).map_err(|e| e.to_string())?;
        preset.save(Path::new(out)).map_err(|e| e.to_string())?;
    }
    let sidecar = edit_target(photo);
    let created = !sidecar.exists();
    let mut state = if created {
        amb_protocol::DevelopState::new()
    } else {
        amb_protocol::DevelopState::load(&sidecar).map_err(|e| e.to_string())?
    };
    let results = state
        .execute_grouped(label, &commands)
        .map_err(|e| e.to_string())?;
    let written = if apply && !commands.is_empty() {
        state.save(&sidecar).map_err(|e| e.to_string())?;
        Some(sidecar.clone())
    } else {
        None
    };
    let report = amb_protocol::edit::EditReport {
        sidecar,
        created,
        results,
        written,
        dry_run: !apply,
    };
    if json {
        extra["distance_before"] = serde_json::json!(result.before);
        extra["distance_after"] = serde_json::json!(result.after);
        extra["renders"] = serde_json::json!(result.evaluations);
        extra["edit"] = report.to_json();
        return serde_json::to_string_pretty(&extra).map_err(|e| e.to_string());
    }
    let mut text = format!(
        "look distance {:.1} → {:.1} ({} renders)\n",
        result.before, result.after, result.evaluations
    );
    text += &report
        .to_brief()
        .replace("dry run, not saved", "not applied (add --apply)");
    if let Some(out) = options.get("--out") {
        text += &format!("\nwrote preset {out}");
    }
    Ok(text)
}

/// Parse an optional numeric option.
fn number_option(options: &BTreeMap<&str, &str>, name: &str) -> Result<Option<f64>, String> {
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

fn negative_stocks(json: bool) {
    use amb_engine::negative::STOCKS;
    if json {
        let list: Vec<serde_json::Value> = STOCKS
            .iter()
            .map(|s| {
                serde_json::json!({
                    "id": s.id,
                    "name": s.name,
                    "black_and_white": s.black_and_white,
                    "description": s.description,
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&list).expect("serialises")
        );
        return;
    }
    for s in STOCKS {
        println!(
            "{:<22} {:<22} {}{}",
            s.id,
            s.name,
            if s.black_and_white { "[B&W] " } else { "" },
            s.description
        );
    }
    println!(
        "\nUse with: amb:NegativeConvert=True, amb:FilmStock=<id> (e.g. in a preset or `amb edit`)."
    );
}

/// `amb negative analyze`: per-frame analysis and the roll median, as `amb:` settings.
fn negative_analyze(
    raws: &[&str],
    out: Option<&Path>,
    slides: bool,
    json: bool,
) -> Result<(), String> {
    use amb_engine::negative::{self, Analysis, NegativeType};
    let kind = if slides {
        NegativeType::Positive
    } else {
        NegativeType::Color
    };
    let k = amb_engine::negative::DEFAULT_STRENGTHS;
    let mut frames: Vec<Analysis> = Vec::new();
    for raw in raws {
        let img = amb_engine::raw::load(Path::new(raw)).map_err(|e| format!("{raw}: {e}"))?;
        let a = negative::analyze_as(&img.rgb, img.width, img.height, kind, &k);
        if !json {
            println!(
                "{raw}: base {:.4} {:.4} {:.4} | shadow density {:.3} {:.3} {:.3} | highlight density {:.3} {:.3} {:.3} | midtone {:.3}",
                a.base[0],
                a.base[1],
                a.base[2],
                a.density_low[0],
                a.density_low[1],
                a.density_low[2],
                a.density_high[0],
                a.density_high[1],
                a.density_high[2],
                a.midtone
            );
        }
        frames.push(a);
    }
    let mut settings = amb_library::negative::frozen(&frames);
    if slides {
        settings.insert(
            "amb:NegativeType".into(),
            amb_presets::Value::Text("positive".into()),
        );
    }
    if json {
        let map: serde_json::Map<String, serde_json::Value> = settings
            .iter()
            .map(|(k, v)| {
                let value = match v {
                    amb_presets::Value::Bool(b) => serde_json::Value::Bool(*b),
                    other => serde_json::json!(other.as_f64()),
                };
                (k.clone(), value)
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&map).expect("serialises")
        );
    } else if frames.len() > 1 {
        println!("roll median over {} frames", frames.len());
    }
    if let Some(out) = out {
        settings.insert(
            "Name".into(),
            amb_presets::Value::Alt(vec![("x-default".into(), "Negative roll".into())]),
        );
        let packet = amb_presets::xmp_writer::write_packet(&settings, amb_presets::is_signed_tag);
        std::fs::write(out, packet).map_err(|e| format!("{}: {e}", out.display()))?;
        if !json {
            println!(
                "wrote {} (apply with `amb render <RAW> <OUT> --preset {}`; add amb:FilmStock to pick a look)",
                out.display(),
                out.display()
            );
        }
    }
    Ok(())
}

/// Export settings from the output path and the export options.
/// `amb render A.nef B.nef ... --out-dir DIR`: each raw to DIR/<name>.<ext>.
fn render_many(inputs: &[&str], dir: &Path, options: &BTreeMap<&str, &str>) -> Result<(), String> {
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

fn export_settings(
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

/// The lensfun database and the user's LCP files: `--lens-db` / `--lcp-dir`, else
/// the default locations.
fn lens_sources(
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
fn lens_fetch(options: &BTreeMap<&str, &str>) -> Result<(), String> {
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
fn lens_info(raw: &Path, options: &BTreeMap<&str, &str>) -> Result<String, String> {
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
fn lens_match_output(options: &BTreeMap<&str, &str>) -> Result<String, String> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edit_command_end_to_end() {
        let dir = std::env::temp_dir().join(format!("amb-cli-edit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let sidecar = dir.join("photo.xmp");
        std::fs::write(
            &sidecar,
            r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description xmlns:xmp="http://ns.adobe.com/xap/1.0/" xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/" xmp:Rating="5" crs:Exposure2012="+0.30"/></rdf:RDF></x:xmpmeta>"#,
        )
        .unwrap();
        let cmds = r#"[{"cmd":"adjust_param","name":"Exposure2012","delta":0.2},
                       {"cmd":"set_param","name":"Vibrance","value":150}]"#;
        let text = edit_output(&sidecar, cmds, None, false, Output::Text).unwrap();
        assert!(text.contains("Exposure2012: +0.3 -> +0.5"), "{text}");
        assert!(text.contains("clamped: Vibrance = 150"), "{text}");
        let written = std::fs::read_to_string(&sidecar).unwrap();
        assert!(written.contains(r#"crs:Exposure2012="+0.5""#), "{written}");
        assert!(written.contains(r#"xmp:Rating="5""#), "{written}");

        let json = edit_output(
            &sidecar,
            r#"{"cmd":"reset_param","name":"Vibrance"}"#,
            None,
            true,
            Output::Json,
        )
        .unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["dry_run"], serde_json::json!(true));
        assert_eq!(
            v["results"][0]["diff"][0]["before"],
            serde_json::json!(100.0)
        );

        let err = edit_output(
            &sidecar,
            r#"{"cmd":"set_param","name":"Exposur","value":1}"#,
            None,
            false,
            Output::Text,
        )
        .unwrap_err();
        assert!(err.contains("did you mean Exposure2012"), "{err}");

        // `amb do` on the photo finds its sidecar, and reports briefly.
        let photo = dir.join("photo.nef");
        let text = do_edits(&photo, "exp+=0.25 vib=10", false, false).unwrap();
        assert_eq!(
            text,
            "Exposure2012 +0.5→+0.75\nVibrance +100→+10\nsaved photo.xmp"
        );
        let text = do_edits(&sidecar, "undo", true, false).unwrap();
        assert!(text.ends_with("dry run, not saved"), "{text}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn lens_match_command() {
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../amb-lens/tests/fixtures");
        let lens_db = fixtures.join("lensfun").display().to_string();
        let lcp_dir = fixtures.join("lcp").display().to_string();
        let empty_dir = fixtures.join("lensfun").display().to_string();
        let mut options: BTreeMap<&str, &str> = BTreeMap::from([
            ("--camera-make", "Acme"),
            ("--camera-model", "Mini 2"),
            ("--lens-make", "Acme"),
            ("--lens-model", "Zoomar 24-70mm F2.8 Pro | A"),
            ("--focal", "47"),
            ("--aperture", "4"),
            ("--lens-db", lens_db.as_str()),
            ("--lcp-dir", lcp_dir.as_str()),
        ]);
        let text = lens_match_output(&options).unwrap();
        assert!(
            text.contains("crop 1.5 (lensfun camera database)"),
            "{text}"
        );
        assert!(
            text.contains("profile: Lcp | LCP: Adobe (Acme Zoomar"),
            "{text}"
        );
        assert!(
            text.contains(
                "lensfun  Acme Zoomar 24-70mm f/2.8 Pro | A (score 1.000, calibration crop 1.5)"
            ),
            "{text}"
        );
        assert!(text.contains("distortion  acm k=["), "{text}");

        // Without LCP profiles the lensfun calibration for the APS-C body is used.
        options.insert("--lcp-dir", empty_dir.as_str());
        let text = lens_match_output(&options).unwrap();
        assert!(text.contains("profile: Lensfun | lensfun: Acme Zoomar 24-70mm f/2.8 Pro | A (calibrated at crop 1.5)"), "{text}");
        // 47 mm is halfway between a poly3 (24 mm) and a ptlens (70 mm) calibration.
        assert!(
            text.contains("distortion  ptlens a=0.001 b=0.002 c=0.003"),
            "{text}"
        );
        assert!(
            text.contains("tca         linear kr=1.002 kb=0.9985"),
            "{text}"
        );

        options.insert("--focal", "wide");
        assert!(
            lens_match_output(&options)
                .unwrap_err()
                .contains("bad --focal")
        );
        options.remove("--lens-model");
        assert!(
            lens_match_output(&options)
                .unwrap_err()
                .contains("--lens-model")
        );
    }
}
