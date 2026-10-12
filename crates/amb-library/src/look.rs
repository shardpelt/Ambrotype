// SPDX-License-Identifier: GPL-3.0-or-later
//! Look matching on files (`amb match`, the editor's Match…): reference
//! photos are developed small with their own edits and measured
//! ([`amb_engine::matching`]); the solved settings become edit commands.

use std::path::Path;

use amb_engine::matching::{self, LookStats};
use amb_presets::Value;
use amb_protocol::{Command, DevelopState};

/// Long edge of the renders that matching measures and searches on.
pub const EDGE: usize = 256;

/// A photo's own edit: its sidecar's settings, or none.
pub fn photo_settings(photo: &Path) -> Result<matching::Settings, String> {
    let is_xmp = photo
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("xmp"));
    let sidecar = if is_xmp {
        photo.to_path_buf()
    } else {
        amb_engine::sidecar_path(photo)
    };
    if !sidecar.is_file() {
        return Ok(Default::default());
    }
    Ok(DevelopState::load(&sidecar)
        .map_err(|e| format!("{}: {e}", sidecar.display()))?
        .settings()
        .clone())
}

/// The look of a reference photo (a JPEG, or a raw with its own edit).
pub fn reference_look(path: &Path) -> Result<LookStats, String> {
    let source = amb_engine::source_path(path);
    let img = amb_engine::raw::load(&source).map_err(|e| format!("{}: {e}", source.display()))?;
    let img = amb_engine::raw::downscale(&img, EDGE);
    let develop = matching::develop_plain(&photo_settings(path)?);
    Ok(matching::look_stats(&amb_engine::pipeline::render(
        &img, &develop,
    )))
}

/// The looks of several references, averaged into one target.
pub fn target_look(paths: &[&Path]) -> Result<LookStats, String> {
    let looks = paths
        .iter()
        .map(|p| reference_look(p))
        .collect::<Result<Vec<_>, _>>()?;
    matching::average(&looks).ok_or_else(|| "give at least one reference photo".to_string())
}

/// The solved changes as `set_param` commands, in order.
pub fn commands(changes: &[(String, Value)]) -> Vec<Command> {
    changes
        .iter()
        .map(|(name, v)| Command::SetParam {
            name: name.clone(),
            value: match v {
                Value::Number(n) => serde_json::json!(n),
                Value::Text(t) => serde_json::json!(t),
                Value::Bool(b) => serde_json::json!(b),
                other => serde_json::json!(other.as_str()),
            },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reference_with_its_edit_and_commands_from_changes() {
        let dir = std::env::temp_dir().join(format!("amb-look-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let raw = dir.join("ref.dng");
        amb_testkit::dng::write(
            &raw,
            &amb_testkit::patterns::generate("colorchecker").unwrap(),
            &Default::default(),
        )
        .unwrap();
        let plain = reference_look(&raw).unwrap();
        let mut state = DevelopState::new();
        state
            .execute(&Command::SetParam {
                name: "Exposure2012".into(),
                value: serde_json::json!(1.0),
            })
            .unwrap();
        state.save(&amb_engine::sidecar_path(&raw)).unwrap();
        let brighter = reference_look(&raw).unwrap();
        assert!(brighter.lightness[4] > plain.lightness[4] + 5.0);
        assert_eq!(target_look(&[raw.as_path()]).unwrap(), brighter);

        let cmds = commands(&[
            ("WhiteBalance".into(), Value::Text("Custom".into())),
            ("Exposure2012".into(), Value::Number(0.5)),
        ]);
        assert_eq!(
            serde_json::to_value(&cmds).unwrap(),
            serde_json::json!([
                {"cmd": "set_param", "name": "WhiteBalance", "value": "Custom"},
                {"cmd": "set_param", "name": "Exposure2012", "value": 0.5},
            ])
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
