//! `flb`: Flightbroom command-line tool.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use flb_presets::Preset;
use flb_presets::registry::Registry;
use flb_presets::report::{self, Report};

const USAGE: &str = "\
flb: Flightbroom command-line tool

USAGE:
    flb preset inspect <FILE>... [--json]   Show what a preset contains and how it maps to our support
    flb preset dump <FILE>                  Print the parsed settings as JSON
    flb preset census <PATH>... [--json]    Count tag usage across a preset collection (dirs are scanned recursively)
    flb sweep plan [--groups G,..]          List the Lightroom calibration sweeps
    flb sweep generate --out <DIR> [--groups G,..] [--synthetic P,..] [<RAW>...]
                                            Write a sweep kit for importing into Lightroom
                                            (docs/lightroom-trial-plan.md): raw copies + XMP sidecars,
                                            and/or synthetic DNG targets with embedded settings
    flb calibration defaults                Print the default model constants as calibration JSON
    flb testimage <PATTERN> <OUT.dng>       Write one synthetic target (ramp, hue, edges, colorchecker,
                                            negative: a scanned colour negative)
    flb render <RAW> <OUT.jpg|.png|.tif|.webp|.avif|.exr> [--preset P] [--amount A]
                                            [--look default|linear] [--calibration fitted.json]
                                            [--lens-db DIR] [--lcp-dir DIR] [export options]
                                            Develop a raw with a preset or sidecar, export it, and report
                                            which settings are not rendered yet
    flb edit <SIDECAR.xmp> --commands <CMDS.json|-> [--out <NEW.xmp|.json>] [--dry-run] [--json]
                                            Apply protocol commands (set_param, adjust_param, reset_param,
                                            reset_group, apply_preset, undo, redo) as one transaction,
                                            print the diff and write the sidecar (other XMP metadata kept)
    flb query <SIDECAR.xmp> --queries <QUERIES.json|->
                                            Answer protocol queries (get_param, list_params, describe,
                                            list_groups) as JSON
    flb protocol schema [command|query]     Print the JSON Schema of commands and queries
    flb negative analyze <RAW>... [--out ROLL.xmp] [--json]
                                            Estimate film base, levels and midtone of scanned negatives
                                            (the median over all frames, for a consistent roll) and write
                                            them as a preset with Auto off
    flb negative stocks [--json]            List the built-in film stock looks
    flb lens info <RAW> [--lens-db DIR] [--lcp-dir DIR]
                                            Show a raw's lens metadata and the profile it gets
    flb lens match --camera-make X --camera-model Y --lens-model M [--lens-make L] [--focal F]
                   [--aperture A] [--distance D] [--crop C] [--lens-db DIR] [--lcp-dir DIR]
                                            Find the lens profile for a shot (Adobe .lcp files you have,
                                            then the lensfun database; default locations or
                                            FLB_LCP_DIRS / FLB_LENSFUN_DB) and print the models
                                            interpolated to its focal length, aperture and distance
    flb settings reference [--json]         Print the settings reference (docs/ai/settings-reference.md)

EXPORT OPTIONS (flb render):
    --format jpeg|png|tiff|webp|avif|exr    Default: from the file extension
    --quality 1-100                         JPEG/AVIF quality (default 90)
    --bits 8|16|32                          PNG 8/16, TIFF 8/16/32 (float); default 8, TIFF 16
    --color-space srgb|p3|adobergb|prophoto|rec2020   Default srgb; an ICC profile is embedded
    --long-edge N | --short-edge N | --width N | --height N | --fit WxH | --megapixels M | --percent P
    --enlarge                               Allow upscaling (default: never enlarge)
    --sharpen screen|matte|glossy[:low|standard|high]   Output sharpening
    --metadata all|no-location|copyright|none           EXIF to include (default no-location)
    --max-size 500k|2M                      JPEG: lower the quality until the file fits
    --subsampling 444|422|420  --progressive  --avif-speed 1-10

Supported inputs: .xmp presets and sidecars, .lrtemplate presets, and files with an embedded
XMP packet (e.g. .dng presets for Lightroom mobile). AI and script guide: docs/ai/README.md.";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut json = false;
    let mut dry_run = false;
    let mut options: BTreeMap<&str, &str> = BTreeMap::new();
    let mut positional: Vec<&str> = Vec::new();
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--json" => json = true,
            "--dry-run" => dry_run = true,
            "--progressive" | "--enlarge" => {
                options.insert(arg.as_str(), "true");
            }
            "--out" | "--groups" | "--synthetic" | "--preset" | "--amount" | "--look"
            | "--calibration" | "--commands" | "--queries" | "--camera-make" | "--camera-model"
            | "--lens-make" | "--lens-model" | "--focal" | "--aperture" | "--distance"
            | "--crop" | "--lens-db" | "--lcp-dir" | "--format" | "--quality" | "--bits"
            | "--color-space" | "--subsampling" | "--long-edge" | "--short-edge" | "--width"
            | "--height" | "--fit" | "--megapixels" | "--percent" | "--sharpen" | "--metadata"
            | "--max-size" | "--avif-speed" => match iter.next() {
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
        ["calibration", "defaults"] => {
            println!(
                "{}",
                flb_engine::calibration::Calibration::default().to_json()
            );
            Ok(())
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
                json,
            ),
            None => Err("edit needs --commands <file.json> (or - for stdin)".into()),
        },
        ["query", state] => match options.get("--queries") {
            Some(queries) => query(Path::new(state), queries),
            None => Err("query needs --queries <file.json> (or - for stdin)".into()),
        },
        ["negative", "analyze", raws @ ..] if !raws.is_empty() => {
            negative_analyze(raws, options.get("--out").map(Path::new), json)
        }
        ["negative", "stocks"] => {
            negative_stocks(json);
            Ok(())
        }
        ["lens", "info", raw] => lens_info(Path::new(raw), &options).map(|text| println!("{text}")),
        ["lens", "match"] => lens_match_output(&options).map(|text| println!("{text}")),
        ["protocol", "schema"] => print_json(&flb_protocol::schema::protocol_schema()),
        ["protocol", "schema", "command"] => print_json(&flb_protocol::schema::command_schema()),
        ["protocol", "schema", "query"] => print_json(&flb_protocol::schema::query_schema()),
        ["settings", "reference"] => {
            if json {
                print_json(&flb_protocol::reference::json())
            } else {
                print!("{}", flb_protocol::reference::markdown());
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

/// `flb edit`: returns what to print, so it can be tested without a process.
fn edit_output(
    sidecar: &Path,
    commands: &str,
    out: Option<&Path>,
    dry_run: bool,
    json: bool,
) -> Result<String, String> {
    let report = flb_protocol::edit::run(&flb_protocol::edit::EditRequest {
        sidecar,
        commands,
        out,
        dry_run,
    })
    .map_err(|e| e.to_string())?;
    if json {
        serde_json::to_string_pretty(&report.to_json()).map_err(|e| e.to_string())
    } else {
        Ok(report.to_text().trim_end().to_string())
    }
}

fn edit(
    sidecar: &Path,
    commands: &str,
    out: Option<&Path>,
    dry_run: bool,
    json: bool,
) -> Result<(), String> {
    let commands = read_input(commands)?;
    println!("{}", edit_output(sidecar, &commands, out, dry_run, json)?);
    Ok(())
}

fn query(state: &Path, queries: &str) -> Result<(), String> {
    let queries = read_input(queries)?;
    let answers = flb_protocol::edit::run_queries(state, &queries).map_err(|e| e.to_string())?;
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

fn selected_plan(groups: Option<&[&str]>) -> Result<Vec<flb_testkit::Sweep>, String> {
    let plan = flb_testkit::default_plan();
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
    let mut inputs: Vec<flb_testkit::Input> = raws
        .iter()
        .map(|r| flb_testkit::Input::Raw(PathBuf::from(r)))
        .collect();
    if let Some(patterns) = synthetic {
        inputs.extend(
            patterns
                .split(',')
                .map(|p| flb_testkit::Input::Synthetic(p.trim().to_string())),
        );
    }
    if inputs.is_empty() {
        return Err("give raw files and/or --synthetic <patterns>".into());
    }
    let summary = flb_testkit::generate(&inputs, &plan, out).map_err(|e| e.to_string())?;
    println!("wrote {} files to {}", summary.files, out.display());
    println!("manifest: {}", summary.manifest.display());
    println!(
        "next: import the folder into Lightroom Classic (Add, with 'read metadata'), then batch-export"
    );
    Ok(())
}

fn testimage(pattern: &str, out: &Path) -> Result<(), String> {
    let image = flb_testkit::patterns::generate(pattern).ok_or_else(|| {
        format!(
            "unknown pattern {pattern:?}; known: {}",
            flb_testkit::patterns::NAMES.join(", ")
        )
    })?;
    flb_testkit::dng::write(out, &image, &Default::default())
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
    use flb_engine::{pipeline, raw};
    let start = std::time::Instant::now();
    let mut img = raw::load(input).map_err(|e| format!("{}: {e}", input.display()))?;
    let (t, tint) = pipeline::as_shot_temp_tint(&img);
    println!(
        "{} {} | {}x{} | as shot: {:.0} K, tint {:+.0}",
        img.make, img.model, img.width, img.height, t, tint
    );
    let mut settings = std::collections::BTreeMap::new();
    if let Some(preset) = preset {
        let preset = load(Path::new(preset))?;
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
            flb_presets::amount::apply(&settings, &preset.settings, amount, Registry::builtin());
        println!(
            "preset: {} at {:.0}%",
            preset.name().unwrap_or("(unnamed)"),
            amount * 100.0
        );
    }
    let (mut develop, not_rendered) = pipeline::DevelopSettings::from_settings(&settings);
    develop.base_look = match look.unwrap_or("default") {
        "default" => pipeline::BaseLook::Default,
        "linear" => pipeline::BaseLook::Linear,
        other => return Err(format!("unknown --look {other:?} (default, linear)")),
    };
    if let Some(path) = calibration {
        develop.calibration =
            flb_engine::calibration::Calibration::from_json_file(Path::new(path))?;
    }
    if develop.lens.profile_enable || develop.lens.auto_ca {
        let (db, lcp) = lens_sources(options)?;
        if img.shot.crop_factor.is_none() {
            img.shot.crop_factor = db
                .as_ref()
                .and_then(|db| db.camera_crop(&img.shot.camera_make, &img.shot.camera_model));
        }
        let hint = flb_lens::ProfileHint {
            name: develop.lens.profile_name.clone(),
            filename: develop.lens.profile_filename.clone(),
        };
        let lens = img.shot.lens_model.as_deref().unwrap_or("unknown lens");
        match flb_lens::resolve(&img.shot, &hint, Some(&lcp), db.as_ref()) {
            Some(c) => {
                println!("lens profile: {}", c.description);
                develop.lens.profile = Some(c);
            }
            None if develop.lens.profile_enable => println!(
                "lens profile: none found for {lens} ({}); profile corrections skipped",
                if db.is_none() && lcp.files.is_empty() {
                    "no lensfun database or LCP files found, see `flb lens match`"
                } else {
                    "not in the lensfun database or your LCP files"
                }
            ),
            None => {}
        }
    }
    let export = export_settings(out, options)?;
    let developed = pipeline::render(&img, &develop);
    let report = flb_export::export(&developed, &img.metadata, &export, out)
        .map_err(|e| format!("{}: {e}", out.display()))?;
    println!(
        "wrote {} ({}, {}x{}, {}, {}, {:.1} MB) in {:.2}s",
        out.display(),
        match report.format {
            flb_export::Format::Jpeg | flb_export::Format::Avif => format!(
                "{} q{}",
                report.format.extension().to_uppercase(),
                report.quality
            ),
            f => f.extension().to_uppercase(),
        },
        report.width,
        report.height,
        match report.bit_depth {
            flb_export::BitDepth::Eight => "8-bit",
            flb_export::BitDepth::Sixteen => "16-bit",
            flb_export::BitDepth::Float32 => "32-bit float",
        },
        report
            .color_space
            .profile_name()
            .trim_end_matches(" (Flightbroom)"),
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
    use flb_engine::negative::STOCKS;
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
        "\nUse with: flb:NegativeConvert=True, flb:FilmStock=<id> (e.g. in a preset or `flb edit`)."
    );
}

/// `flb negative analyze`: per-frame analysis and the roll median, as `flb:` settings.
fn negative_analyze(raws: &[&str], out: Option<&Path>, json: bool) -> Result<(), String> {
    use flb_engine::negative::{self, Analysis};
    let k = flb_engine::negative::DEFAULT_STRENGTHS;
    let mut frames: Vec<Analysis> = Vec::new();
    for raw in raws {
        let img = flb_engine::raw::load(Path::new(raw)).map_err(|e| format!("{raw}: {e}"))?;
        let a = negative::analyze(&img.rgb, img.width, img.height, &k);
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
    let median = |f: &dyn Fn(&Analysis) -> f64| -> f64 {
        let mut v: Vec<f64> = frames.iter().map(f).collect();
        v.sort_by(f64::total_cmp);
        let n = v.len();
        if n % 2 == 1 {
            v[n / 2]
        } else {
            0.5 * (v[n / 2 - 1] + v[n / 2])
        }
    };
    let round = |v: f64| (v * 10_000.0).round() / 10_000.0;
    let mut settings = BTreeMap::new();
    let mut put = |k: &str, v: flb_presets::Value| {
        settings.insert(format!("flb:{k}"), v);
    };
    put("NegativeConvert", flb_presets::Value::Bool(true));
    put("NegativeAuto", flb_presets::Value::Bool(false));
    for (c, name) in ["Red", "Green", "Blue"].iter().enumerate() {
        put(
            &format!("FilmBase{name}"),
            flb_presets::Value::Number(round(median(&|a| a.base[c]))),
        );
        put(
            &format!("NegativeDensityLow{name}"),
            flb_presets::Value::Number(round(median(&|a| a.density_low[c]))),
        );
        put(
            &format!("NegativeDensityHigh{name}"),
            flb_presets::Value::Number(round(median(&|a| a.density_high[c]))),
        );
    }
    put(
        "NegativeMidtone",
        flb_presets::Value::Number(round(median(&|a| a.midtone))),
    );
    if json {
        let map: serde_json::Map<String, serde_json::Value> = settings
            .iter()
            .map(|(k, v)| {
                let value = match v {
                    flb_presets::Value::Bool(b) => serde_json::Value::Bool(*b),
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
            flb_presets::Value::Alt(vec![("x-default".into(), "Negative roll".into())]),
        );
        let packet = flb_presets::xmp_writer::write_packet(&settings, flb_presets::is_signed_tag);
        std::fs::write(out, packet).map_err(|e| format!("{}: {e}", out.display()))?;
        if !json {
            println!(
                "wrote {} (apply with `flb render <RAW> <OUT> --preset {}`; add flb:FilmStock to pick a look)",
                out.display(),
                out.display()
            );
        }
    }
    Ok(())
}

/// Export settings from the output path and the export options.
fn export_settings(
    out: &Path,
    options: &BTreeMap<&str, &str>,
) -> Result<flb_export::ExportSettings, String> {
    use flb_export::{BitDepth, ExportSettings, Format, MetadataPolicy, Resize, Subsampling};
    let format = match options.get("--format") {
        Some(f) => Format::from_name(f).ok_or_else(|| format!("unknown --format {f:?}"))?,
        None => Format::from_path(out).ok_or_else(|| {
            format!(
                "{}: unknown output type (use .jpg, .png, .tif, .webp, .avif or .exr, or --format)",
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
        s.color_space = flb_color::space::OutputSpace::from_id(cs).ok_or_else(|| {
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
    ];
    let mut chosen = sizes.into_iter().flatten();
    if let Some(r) = chosen.next() {
        if chosen.next().is_some() {
            return Err("use only one of --long-edge, --short-edge, --width, --height, --fit, --megapixels, --percent".into());
        }
        s.resize = r;
    }
    if let Some(sh) = options.get("--sharpen") {
        s.sharpen = Some(flb_export::parse_sharpen(sh).ok_or_else(|| {
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
    Ok(s)
}

/// The lensfun database and the user's LCP files: `--lens-db` / `--lcp-dir`, else
/// the default locations.
fn lens_sources(
    options: &BTreeMap<&str, &str>,
) -> Result<
    (
        Option<flb_lens::lensfun::LensfunDb>,
        flb_lens::lcp::LcpLibrary,
    ),
    String,
> {
    use flb_lens::{LcpLibrary, LensfunDb};
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

/// `flb lens info <RAW>`: the raw's lens metadata, then the same report as `flb lens match`.
fn lens_info(raw: &Path, options: &BTreeMap<&str, &str>) -> Result<String, String> {
    let shot = flb_engine::raw::read_shot(raw).map_err(|e| format!("{}: {e}", raw.display()))?;
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

/// `flb lens match`: returns what to print, so it can be tested without a process.
fn lens_match_output(options: &BTreeMap<&str, &str>) -> Result<String, String> {
    use flb_lens::{ProfileHint, ShotInfo};
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
                "lensfun: no database found (install lensfun, set FLB_LENSFUN_DB or pass --lens-db)"
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

    let Some(c) = flb_lens::resolve(&shot, &ProfileHint::default(), Some(&lcp), db.as_ref()) else {
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
        let dir = std::env::temp_dir().join(format!("flb-cli-edit-{}", std::process::id()));
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
        let text = edit_output(&sidecar, cmds, None, false, false).unwrap();
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
            true,
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
            false,
        )
        .unwrap_err();
        assert!(err.contains("did you mean Exposure2012"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn lens_match_command() {
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../flb-lens/tests/fixtures");
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
