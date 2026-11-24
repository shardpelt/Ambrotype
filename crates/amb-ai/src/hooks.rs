// SPDX-License-Identifier: GPL-3.0-or-later
//! Registers the engine's AI hooks (subject, objects, faces, inpainting, raw
//! denoising) so the editor and `amb` render with the same models.

use std::path::Path;
use std::sync::Arc;

/// Install every engine hook that a model in `models` serves. Models are
/// fetched or loaded on first use. The hooks stay installed in AI-free mode
/// (the editor switches it at run time): the engine acts as if none were
/// there while it is on, and each model path checks it too
/// (`amb_presets::ai_free`).
pub fn install(models: &Path) {
    let dir = models.to_path_buf();
    amb_engine::subject::set_finder(crate::finder(dir.clone()));
    amb_engine::subject::set_selector(crate::objects::selector(dir.clone()));
    amb_engine::subject::set_face_finder(crate::faces::finder(dir.clone()));
    amb_engine::retouch::set_inpainter(crate::remove::inpainter(dir.clone()));
    amb_engine::raw_denoise::set_raw_denoiser(
        Arc::new(crate::denoise::Denoiser::new(dir)).engine(),
    );
}
