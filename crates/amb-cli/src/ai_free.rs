// SPDX-License-Identifier: GPL-3.0-or-later
//! `amb ai-free`: show, turn on or turn off AI-free mode, the switch that
//! keeps every AI model off the user's photos (`amb_presets::ai_free`). The
//! setting is saved for the editor and `amb` alike.

use crate::*;

fn show(on: bool, json: bool) -> Result<(), String> {
    if json {
        return print_json(&serde_json::json!({ "ai_free": on }));
    }
    println!(
        "AI-free mode is {}.",
        if on {
            "on: no AI model runs on your photos"
        } else {
            "off"
        }
    );
    Ok(())
}

/// `amb ai-free`: the saved setting.
pub(crate) fn status(json: bool) -> Result<(), String> {
    show(amb_library::ai_free::load(), json)
}

/// `amb ai-free on` / `off`: save it (and use it for the rest of this run).
pub(crate) fn set(on: bool, json: bool) -> Result<(), String> {
    amb_library::ai_free::save(on)?;
    amb_presets::ai_free::set(on);
    show(on, json)
}
