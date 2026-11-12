// SPDX-License-Identifier: GPL-3.0-or-later
//! AI Denoise: a raw, denoised by the AI model (`amb_engine::raw_denoise`,
//! run by `amb_ai::denoise`), written as a new linear DNG beside it, as
//! Lightroom's Denoise does: `<name>-Denoised.dng`. The photo's edit comes
//! along, with luminance noise reduction off (the noise is gone).

use std::path::{Path, PathBuf};

use amb_presets::Value;

/// Where the denoised copy of `photo` goes: `<name>-Denoised.dng` beside
/// it, or `-Denoised-2` … when that (or its sidecar) is taken.
pub fn default_output(photo: &Path) -> Option<PathBuf> {
    let dir = photo.parent().unwrap_or(Path::new("."));
    let stem = photo.file_stem()?.to_string_lossy();
    (1..)
        .map(|n| match n {
            1 => dir.join(format!("{stem}-Denoised.dng")),
            n => dir.join(format!("{stem}-Denoised-{n}.dng")),
        })
        .find(|p| !p.exists() && !amb_engine::sidecar_path(p).exists())
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Report {
    pub output: PathBuf,
    pub width: usize,
    pub height: usize,
    pub seconds: f64,
}

/// How much of the denoised photo by default (0–100), as Lightroom's
/// Denoise starts: the rest is the photo as it was, so fine texture stays.
pub const AMOUNT: f32 = 70.0;

/// Denoise `photo` into `output` (or [`default_output`]), `amount` (0–100)
/// of it. The app must have registered a denoiser
/// (`amb_ai::denoise::Denoiser`).
pub fn denoise(
    photo: &Path,
    output: Option<&Path>,
    amount: f32,
    progress: &dyn Fn(usize, usize),
) -> Result<Report, String> {
    let start = std::time::Instant::now();
    let output = match output {
        Some(o) => o.to_path_buf(),
        None => default_output(photo).ok_or("no name for the denoised photo")?,
    };
    let img = amb_engine::raw_denoise::load(photo, amount / 100.0, progress)
        .map_err(|e| e.to_string())?;
    amb_export::dng::write_linear(&output, &img, None).map_err(|e| e.to_string())?;
    // The edit, without luminance noise reduction.
    if let Some(text) = amb_engine::sidecar_text(photo) {
        let mut settings = amb_presets::Preset::parse_xmp(&text)
            .map(|p| p.settings)
            .unwrap_or_default();
        settings.insert("LuminanceSmoothing".into(), Value::Number(0.0));
        let merged = amb_protocol::sidecar::merge(&text, &settings).map_err(|e| e.to_string())?;
        amb_protocol::sidecar::write_atomic(&amb_engine::sidecar_path(&output), &merged)
            .map_err(|e| e.to_string())?;
    }
    Ok(Report {
        output,
        width: img.width,
        height: img.height,
        seconds: start.elapsed().as_secs_f64(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_the_copy_after_the_photo_without_overwriting() {
        let dir = std::env::temp_dir().join(format!("amb-denoise-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let photo = dir.join("DSC_0001.NEF");
        assert_eq!(
            default_output(&photo).unwrap(),
            dir.join("DSC_0001-Denoised.dng")
        );
        std::fs::write(dir.join("DSC_0001-Denoised.dng"), b"").unwrap();
        assert_eq!(
            default_output(&photo).unwrap(),
            dir.join("DSC_0001-Denoised-2.dng")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
