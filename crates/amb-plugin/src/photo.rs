// SPDX-License-Identifier: GPL-3.0-or-later
//! What a plugin may learn about a photo: camera facts (`photo-info`) and
//! statistics of a small render with the current edits (`photo-stats`).

use amb_engine::pipeline::{self, DevelopSettings};
use amb_engine::raw::CameraImage;
use amb_plugin_sdk::Stats;
use serde_json::Value;
use std::collections::BTreeMap;

/// The long edge of the render statistics are taken from.
const STATS_EDGE: usize = 512;

/// Camera and shot facts, as `amb_plugin_sdk::Photo` JSON.
pub fn info(img: &CameraImage) -> Value {
    let m = &img.metadata;
    let ratio = |r: Option<(u32, u32)>| {
        r.filter(|r| r.1 != 0)
            .map(|(n, d)| f64::from(n) / f64::from(d))
    };
    let swap = (5..=8).contains(&m.orientation);
    let (w, h) = if swap {
        (img.height, img.width)
    } else {
        (img.width, img.height)
    };
    serde_json::json!({
        "camera": format!("{} {}", img.make, img.model).trim(),
        "lens": img.shot.lens_model,
        "width": w,
        "height": h,
        "iso": m.iso,
        "shutter": ratio(m.exposure_time),
        "aperture": ratio(m.fnumber),
        "focal_length": ratio(m.focal_length),
        "as_shot_temperature": pipeline::as_shot_temp_tint(img).0.round(),
        "as_shot_tint": pipeline::as_shot_temp_tint(img).1.round(),
    })
}

/// Statistics of the photo rendered with `settings` at about 512 px (lens
/// corrections left out; they barely move a histogram).
pub fn stats(img: &CameraImage, settings: &BTreeMap<String, amb_presets::Value>) -> Stats {
    let step = img.width.max(img.height).div_ceil(STATS_EDGE).max(1);
    let (w, h) = ((img.width / step).max(1), (img.height / step).max(1));
    let mut rgb = Vec::with_capacity(w * h * 3);
    for y in 0..h {
        for x in 0..w {
            let i =
                ((y * step).min(img.height - 1) * img.width + (x * step).min(img.width - 1)) * 3;
            rgb.extend_from_slice(&img.rgb[i..i + 3]);
        }
    }
    let small = CameraImage {
        width: w,
        height: h,
        rgb,
        xyz_to_cam: img.xyz_to_cam,
        matrices: Vec::new(),
        as_shot_neutral: img.as_shot_neutral,
        clip: img.clip,
        rendered: img.rendered,
        make: img.make.clone(),
        model: img.model.clone(),
        shot: img.shot.clone(),
        metadata: img.metadata.clone(),
    };
    let (develop, _) = DevelopSettings::from_settings(settings);
    let developed = pipeline::render(&small, &develop);
    let upright = amb_export::geometry::orient(&developed, img.metadata.orientation);
    let bytes: Vec<u8> = amb_engine::output::to_srgb(&upright)
        .iter()
        .map(|v| (v * 255.0).round().clamp(0.0, 255.0) as u8)
        .collect();
    Stats::from_rgb8(&bytes, upright.width as u32, upright.height as u32)
}
