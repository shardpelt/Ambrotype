// SPDX-License-Identifier: GPL-3.0-or-later
//! Registers the engine's AI hooks (subject, objects, faces, inpainting, raw
//! denoising) so the editor and `amb` render with the same models.

use std::path::Path;
use std::sync::Arc;

/// Install every engine hook that a model in `models` serves. Models are
/// fetched or loaded on first use.
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
