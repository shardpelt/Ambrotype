// SPDX-License-Identifier: GPL-3.0-or-later
//! `amb dust`: dust and hairs on a film scan, as heal spots.

use std::path::Path;

use crate::edit::edit_target;

/// Find dust on the photo's raw; with `apply`, add a heal spot for each as
/// one undo step ("Dust and hairs").
pub(crate) fn dust(photo: &Path, apply: bool, json: bool) -> Result<String, String> {
    let found = amb_library::dust::find_file(photo)?;
    let sidecar = edit_target(photo);
    let created = !sidecar.exists();
    let mut state = if created {
        amb_protocol::DevelopState::new()
    } else {
        amb_protocol::DevelopState::load(&sidecar).map_err(|e| e.to_string())?
    };
    let results = if found.commands.is_empty() {
        Vec::new()
    } else {
        state
            .execute_grouped("Dust and hairs", &found.commands)
            .map_err(|e| e.to_string())?
    };
    let written = if apply && !found.commands.is_empty() {
        state.save(&sidecar).map_err(|e| e.to_string())?;
        Some(sidecar.clone())
    } else {
        None
    };
    if json {
        return serde_json::to_string_pretty(&serde_json::json!({
            "specks": found.specks,
            "hairs": found.hairs,
            "covered": found.covered,
            "spots": found.commands,
            "written": written,
        }))
        .map_err(|e| e.to_string());
    }
    let report = amb_protocol::edit::EditReport {
        sidecar,
        created,
        results,
        written,
        dry_run: !apply,
    };
    let mut text = format!(
        "{} speck(s) and {} hair(s) to heal{}\n",
        found.specks,
        found.hairs,
        if found.covered > 0 {
            format!("; {} already under a spot", found.covered)
        } else {
            String::new()
        }
    );
    if !found.commands.is_empty() {
        text += &report
            .to_brief()
            .replace("dry run, not saved", "not applied (add --apply)");
    }
    Ok(text)
}
