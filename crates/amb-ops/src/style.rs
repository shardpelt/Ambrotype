// SPDX-License-Identifier: GPL-3.0-or-later
//! Apply a style to photos: each photo's own settings are solved on a small
//! copy to reach the style's look, blended at the Amount, and kept in its
//! sidecar as one undo step called "Style: NAME".

use std::path::{Path, PathBuf};

use amb_engine::matching::{self, MatchOptions};
use amb_library::look;
use amb_library::styles::{self, Style};
use amb_protocol::DevelopState;
use serde::Serialize;

pub struct Options<'a> {
    pub photos: &'a [PathBuf],
    pub style: &'a Style,
    /// 1.0 is 100 %.
    pub amount: f64,
}

#[derive(Debug, Default, Serialize)]
pub struct Report {
    /// Photos whose sidecar now holds the style's moves.
    pub changed: Vec<PathBuf>,
    /// Photos that already had the look.
    pub unchanged: Vec<PathBuf>,
    pub errors: Vec<String>,
}

/// One photo; whether its edit changed.
fn apply_one(photo: &Path, style: &Style, amount: f64) -> Result<bool, String> {
    let source = amb_engine::source_path(photo);
    let full = amb_engine::raw::load(&source).map_err(|e| format!("{}: {e}", source.display()))?;
    let small = amb_engine::raw::downscale(&full, look::EDGE);
    let base = look::photo_settings(photo)?;
    let opts = MatchOptions {
        parts: style.focus.parts(),
        weights: style.focus.weights(),
        ..Default::default()
    };
    let result = matching::solve(&small, &base, &style.look, &opts, &|s| {
        matching::develop_plain(s)
    });
    let commands = styles::commands(&base, &result, amount);
    if commands.is_empty() {
        return Ok(false);
    }
    let sidecar = amb_engine::sidecar_path(photo);
    let mut state = if sidecar.is_file() || amb_presets::linked::get(&sidecar).is_some() {
        DevelopState::load(&sidecar).map_err(|e| e.to_string())?
    } else {
        DevelopState::from_settings(base)
    };
    state
        .execute_grouped(&format!("Style: {}", style.name), &commands)
        .map_err(|e| e.to_string())?;
    state.save(&sidecar).map_err(|e| e.to_string())?;
    Ok(true)
}

/// The style on each photo, one after another (each solve already uses all
/// cores); a photo that fails is reported and the rest go on.
pub fn apply(opts: &Options, progress: &(dyn Fn(usize, usize) + Sync)) -> Report {
    let mut report = Report::default();
    let total = opts.photos.len();
    for (i, photo) in opts.photos.iter().enumerate() {
        progress(i, total);
        match apply_one(photo, opts.style, opts.amount) {
            Ok(true) => report.changed.push(photo.clone()),
            Ok(false) => report.unchanged.push(photo.clone()),
            Err(e) => report.errors.push(e),
        }
    }
    progress(total, total);
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use amb_engine::matching::Focus;

    #[test]
    fn a_style_from_a_brighter_photo_raises_a_darker_one() {
        let dir = std::env::temp_dir().join(format!("amb-ops-style-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let img = amb_testkit::patterns::generate("colorchecker").unwrap();
        let (bright, dark) = (dir.join("bright.dng"), dir.join("dark.dng"));
        for p in [&bright, &dark] {
            amb_testkit::dng::write(p, &img, &Default::default()).unwrap();
        }
        let mut state = DevelopState::new();
        state
            .execute(&amb_protocol::Command::SetParam {
                name: "Exposure2012".into(),
                value: serde_json::json!(1.5),
            })
            .unwrap();
        state.save(&amb_engine::sidecar_path(&bright)).unwrap();

        let style = styles::from_photos("Bright", &[bright.as_path()], Focus::Tone).unwrap();
        let photos = [dark.clone()];
        let report = apply(
            &Options {
                photos: &photos,
                style: &style,
                amount: 1.0,
            },
            &|_, _| {},
        );
        assert_eq!(report.changed, photos, "{:?}", report.errors);
        let exposure = |p: &Path| {
            look::photo_settings(p).unwrap()["Exposure2012"]
                .as_f64()
                .unwrap()
        };
        assert!(exposure(&dark) > 0.5, "{}", exposure(&dark));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
