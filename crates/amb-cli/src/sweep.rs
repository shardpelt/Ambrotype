// SPDX-License-Identifier: GPL-3.0-or-later
//! `amb sweep` (the Lightroom measurement kit) and `amb testimage`.

use crate::*;

pub(crate) fn selected_plan(groups: Option<&[&str]>) -> Result<Vec<amb_testkit::Sweep>, String> {
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

pub(crate) fn sweep_plan(groups: Option<&[&str]>) -> Result<(), String> {
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

pub(crate) fn sweep_generate(
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

pub(crate) fn testimage(pattern: &str, out: &Path) -> Result<(), String> {
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
