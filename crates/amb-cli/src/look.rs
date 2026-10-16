// SPDX-License-Identifier: GPL-3.0-or-later
//! Looking at a photo and solving for a look: `amb stats`, `amb match`, `amb auto`.

use crate::*;

/// `amb stats`: the photo developed small, as numbers.
pub(crate) fn stats(
    input: &Path,
    options: &BTreeMap<&str, &str>,
    json: bool,
) -> Result<String, String> {
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
pub(crate) fn match_look(
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
pub(crate) fn auto(
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
pub(crate) fn solve_on_photo(
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
pub(crate) fn apply_result(
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
