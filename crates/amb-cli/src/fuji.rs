// SPDX-License-Identifier: GPL-3.0-or-later
//! `amb fuji`: a Fujifilm-style camera recipe, pasted as text, recreated as
//! develop settings (`amb_plugin::fuji`).

use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;

use amb_presets::Value;

use crate::edit::edit_target;

/// Read the recipe (a file, or `-` for stdin) and show what it becomes; with
/// `--out`, write it as a preset in the Fuji group; with a photo and `--apply`, put
/// it on the photo as one undo step.
pub(crate) fn fuji(
    recipe: &str,
    photo: Option<&Path>,
    options: &BTreeMap<&str, &str>,
    json: bool,
) -> Result<String, String> {
    let text = if recipe == "-" {
        let mut t = String::new();
        std::io::stdin()
            .read_to_string(&mut t)
            .map_err(|e| e.to_string())?;
        t
    } else {
        std::fs::read_to_string(recipe).map_err(|e| format!("{recipe}: {e}"))?
    };
    let sensor = match options.get("--sensor") {
        Some(s) => Some(amb_plugin::fuji::Sensor::from_name(s).ok_or_else(|| {
            format!("--sensor: {s:?} isn't a sensor (X-Trans I to V, Bayer, GFX) or a camera model")
        })?),
        None => None,
    };
    let made = amb_plugin::fuji::recreate(&text, sensor)?;
    let name = options
        .get("--name")
        .map(|n| n.to_string())
        .or(made.recipe.name.clone())
        .unwrap_or_else(|| "Fuji recipe".into());
    if let Some(out) = options.get("--out") {
        write_preset(Path::new(out), &name, &made.settings)?;
    }
    let apply = photo.filter(|_| options.contains_key("--apply"));
    if let Some(photo) = apply {
        let sidecar = edit_target(photo);
        let mut state = if sidecar.exists() {
            amb_protocol::DevelopState::load(&sidecar).map_err(|e| e.to_string())?
        } else {
            amb_protocol::DevelopState::new()
        };
        state
            .execute_grouped(&format!("Fuji recipe: {name}"), &made.commands)
            .map_err(|e| e.to_string())?;
        state.save(&sidecar).map_err(|e| e.to_string())?;
    }
    let settings = amb_plugin::Context::settings_json(&made.settings);
    if json {
        return serde_json::to_string_pretty(&serde_json::json!({
            "name": name,
            "sensor": made.recipe.sensor.name(),
            "params": made.recipe.params,
            "notes": made.recipe.notes,
            "settings": settings,
        }))
        .map_err(|e| e.to_string());
    }
    let mut text = format!(
        "{name} ({}): {}\n",
        made.recipe.sensor.name(),
        made.message.unwrap_or_default()
    );
    for note in &made.recipe.notes {
        text += &format!("  note: {note}\n");
    }
    for (k, v) in &settings {
        text += &format!("  {k} = {v}\n");
    }
    if let Some(out) = options.get("--out") {
        text += &format!("wrote {out}\n");
    }
    match (photo, apply) {
        (_, Some(p)) => text += &format!("applied to {}\n", p.display()),
        (Some(_), None) => text += "(not applied: add --apply)\n",
        _ => {}
    }
    Ok(text.trim_end().to_string())
}

/// A Lightroom-style preset in the Fuji group.
fn write_preset(path: &Path, name: &str, settings: &BTreeMap<String, Value>) -> Result<(), String> {
    let mut out = settings.clone();
    let alt = |s: &str| Value::Alt(vec![("x-default".into(), s.to_string())]);
    out.insert("PresetType".into(), Value::Text("Normal".into()));
    out.insert("Name".into(), alt(name));
    out.insert("Group".into(), alt("Fuji"));
    out.insert("HasSettings".into(), Value::Text("True".into()));
    out.insert("SupportsAmount".into(), Value::Text("True".into()));
    let text = amb_presets::xmp_writer::write_packet(&out, amb_presets::is_signed_tag);
    amb_protocol::sidecar::write_atomic(path, &text).map_err(|e| e.to_string())
}
