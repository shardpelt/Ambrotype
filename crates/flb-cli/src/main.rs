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
    flb testimage <PATTERN> <OUT.dng>       Write one synthetic target (ramp, hue, edges, colorchecker)
    flb render <RAW> <OUT.png|.jpg|.tif> [--preset P] [--amount A] [--look default|linear]
                                            Develop a raw (engine v0: white balance, Exposure, point
                                            curves) and report which preset settings were not rendered

Supported inputs: .xmp presets and sidecars, .lrtemplate presets, and files with an embedded
XMP packet (e.g. .dng presets for Lightroom mobile).";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut json = false;
    let mut options: BTreeMap<&str, &str> = BTreeMap::new();
    let mut positional: Vec<&str> = Vec::new();
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--json" => json = true,
            "--out" | "--groups" | "--synthetic" | "--preset" | "--amount" | "--look" => {
                match iter.next() {
                    Some(value) => {
                        options.insert(arg.as_str(), value.as_str());
                    }
                    None => {
                        eprintln!("error: {arg} needs a value\n\n{USAGE}");
                        return ExitCode::from(2);
                    }
                }
            }
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
        ["render", input, out] => render(
            Path::new(input),
            Path::new(out),
            options.get("--preset").copied(),
            options.get("--amount").copied(),
            options.get("--look").copied(),
        ),
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
) -> Result<(), String> {
    use flb_engine::{output, pipeline, raw};
    let start = std::time::Instant::now();
    let img = raw::load(input).map_err(|e| format!("{}: {e}", input.display()))?;
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
    let developed = pipeline::render(&img, &develop);
    output::write_srgb(&developed, out).map_err(|e| e.to_string())?;
    println!(
        "wrote {} in {:.2}s",
        out.display(),
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
