// SPDX-License-Identifier: GPL-3.0-or-later
//! `amb models`: the AI models for masks (subject, background, people):
//! list, install, choose, add your own, remove.

use crate::*;

fn models_dir() -> Result<PathBuf, String> {
    amb_ai::default_dir().ok_or_else(|| "no home folder; set AMB_MODELS".into())
}

fn megabytes(bytes: u64) -> String {
    if bytes == 0 {
        "?".into()
    } else {
        format!("{:.0} MB", (bytes as f64 / 1e6).max(1.0))
    }
}

pub(crate) fn models_list(json: bool) -> Result<(), String> {
    let dir = models_dir()?;
    let all = amb_ai::list(&dir);
    if json {
        return print_json(&serde_json::json!({
            "tasks": amb_ai::TASKS.map(|(t, about)| serde_json::json!({"task": t, "for": about})),
            "models": all,
        }));
    }
    let speed = |s: f64| {
        if s <= 0.0 {
            "?".to_string()
        } else if s < 1.0 {
            "<1 s".to_string()
        } else {
            format!("{s:.0} s")
        }
    };
    let detail = |d: u8| {
        if d == 0 {
            "?".to_string()
        } else {
            format!(
                "{}{}",
                "●".repeat(d as usize),
                "○".repeat(5 - d.min(5) as usize)
            )
        }
    };
    for (task, about) in amb_ai::TASKS {
        println!("{task}: {about}");
        println!(
            "  {:<20} {:<16} {:>7} {:>6}  {:<6} {:<11}",
            "id", "name", "size", "time", "detail", "licence"
        );
        for m in all.iter().filter(|m| m.spec.task == task) {
            let s = &m.spec;
            let mark = match (m.chosen, m.installed) {
                (true, true) => "* in use",
                (true, false) => "* in use, downloads on first use",
                (false, true) => "  installed",
                (false, false) => "  not installed",
            };
            println!(
                "  {:<20} {:<16} {:>7} {:>6}  {:<6} {:<11} {}",
                s.id,
                s.name,
                megabytes(s.bytes),
                speed(s.seconds),
                detail(s.detail),
                s.licence,
                mark
            );
            if !s.about.is_empty() {
                println!("  {:<20} {}", "", s.about);
            }
            if !s.notes.is_empty() {
                println!("  {:<20} Note: {}", "", s.notes);
            }
        }
    }
    println!(
        "\nTime is per photo on a 4-core laptop; detail is how fine the edges are (●●●●● finest).\nInstall with `amb models install ID`, then `amb models use ID`."
    );
    Ok(())
}

pub(crate) fn models_install(id: &str) -> Result<(), String> {
    let dir = models_dir()?;
    let last = std::sync::atomic::AtomicU64::new(0);
    let spec = amb_ai::install(&dir, id, &|done, total| {
        // A line every 10%.
        let tenth = (done * 10).checked_div(total).unwrap_or(0);
        if tenth > last.swap(tenth, std::sync::atomic::Ordering::Relaxed) {
            eprintln!("{}0%", tenth);
        }
    })?;
    println!(
        "{} is installed ({}). Use it with `amb models use {}`.",
        spec.name,
        megabytes(spec.bytes),
        spec.id
    );
    Ok(())
}

pub(crate) fn models_use(id: &str) -> Result<(), String> {
    let spec = amb_ai::choose(&models_dir()?, id)?;
    println!("{} masks now use {}.", spec.task, spec.name);
    Ok(())
}

pub(crate) fn models_remove(id: &str) -> Result<(), String> {
    amb_ai::remove(&models_dir()?, id)?;
    println!("Removed {id}.");
    Ok(())
}

fn triple(options: &BTreeMap<&str, &str>, key: &str) -> Result<Option<[f32; 3]>, String> {
    let Some(v) = options.get(key) else {
        return Ok(None);
    };
    let n: Vec<f32> = v
        .split(',')
        .map(|x| x.trim().parse::<f32>())
        .collect::<Result<_, _>>()
        .map_err(|_| format!("{key} wants three numbers, like 0.485,0.456,0.406"))?;
    match n[..] {
        [a, b, c] => Ok(Some([a, b, c])),
        _ => Err(format!("{key} wants three numbers")),
    }
}

pub(crate) fn models_add(file: &Path, options: &BTreeMap<&str, &str>) -> Result<(), String> {
    let need = |k: &str| {
        options
            .get(k)
            .copied()
            .ok_or_else(|| format!("models add needs {k}"))
    };
    let size = need("--size")?
        .parse::<usize>()
        .map_err(|_| "--size is the model's square input side, in pixels".to_string())?;
    let mut m = amb_ai::CustomModel {
        task: need("--task")?.to_string(),
        name: need("--name")?.to_string(),
        size,
        mean: [0.485, 0.456, 0.406],
        std: [0.229, 0.224, 0.225],
        activation: if options.contains_key("--sigmoid") {
            amb_ai::Activation::Sigmoid
        } else {
            amb_ai::Activation::None
        },
        licence: options.get("--licence").unwrap_or(&"").to_string(),
        source: options.get("--source").unwrap_or(&"").to_string(),
    };
    if let Some(v) = triple(options, "--mean")? {
        m.mean = v;
    }
    if let Some(v) = triple(options, "--std")? {
        m.std = v;
    }
    let spec = amb_ai::add_custom(&models_dir()?, file, m)?;
    println!(
        "Added {} as {} ({} masks). Use it with `amb models use {}`.",
        spec.name, spec.id, spec.task, spec.id
    );
    Ok(())
}
