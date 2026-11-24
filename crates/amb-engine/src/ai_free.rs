// SPDX-License-Identifier: GPL-3.0-or-later
//! What AI-free mode leaves out of a photo ([`amb_presets::ai_free`] is the
//! switch). The engine itself acts as if no model were installed while it is
//! on: Select Subject, Background and People select nothing, AI Remove spots
//! are skipped, AI Denoise is refused. This module names what an edit holds
//! of that, for the export report and the editor to show.

use std::collections::BTreeMap;
use std::path::Path;

use amb_presets::Value;

/// The AI parts the edit `settings` holds, in plain words, once each
/// ([`amb_presets::ai_free::left_out`]).
pub fn left_out(settings: &BTreeMap<String, Value>) -> Vec<String> {
    amb_presets::ai_free::left_out(settings)
}

/// The same for `photo`'s own edit (its sidecar, or the Lightroom catalogue
/// it is linked to). Empty while AI-free mode is off: nothing is left out
/// then, so nothing is reported.
pub fn left_out_of(photo: &Path) -> Vec<String> {
    if !amb_presets::ai_free::on() {
        return Vec::new();
    }
    let Some(text) = crate::sidecar_text(photo) else {
        return Vec::new();
    };
    match amb_presets::Preset::parse_xmp(&text) {
        Ok(p) => left_out(&p.settings),
        Err(_) => Vec::new(),
    }
}
