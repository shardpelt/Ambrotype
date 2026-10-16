// SPDX-License-Identifier: GPL-3.0-or-later
//! Reading presets: `amb inspect`, `amb dump`, `amb query`, `amb preset census`.

use crate::*;

pub(crate) fn query(state: &Path, queries: &str) -> Result<(), String> {
    let queries = read_input(queries)?;
    let answers = amb_protocol::edit::run_queries(state, &queries).map_err(|e| e.to_string())?;
    print_json(&answers)
}

pub(crate) fn load(path: &Path) -> Result<Preset, String> {
    Preset::from_path(path).map_err(|e| format!("{}: {e}", path.display()))
}

pub(crate) fn inspect(files: &[&str], json: bool) -> Result<(), String> {
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

pub(crate) fn print_report(file: &str, r: &Report) {
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

pub(crate) fn dump(file: &str) -> Result<(), String> {
    let preset = load(Path::new(file))?;
    println!(
        "{}",
        serde_json::to_string_pretty(&preset).map_err(|e| e.to_string())?
    );
    Ok(())
}

pub(crate) fn collect_files(path: &Path, out: &mut Vec<PathBuf>) {
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

pub(crate) fn census(paths: &[&str], json: bool) -> Result<(), String> {
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
