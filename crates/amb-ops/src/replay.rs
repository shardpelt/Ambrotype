// SPDX-License-Identifier: GPL-3.0-or-later
//! Edit Replay (`amb replay`, File → Export Edit Replay…): a short video of
//! a photo's edit, step by step from its History: the photo as it was, then
//! each step easing in (sliders glide to their new value), named in a
//! caption, then the result. Encoded as AV1 in MP4 ([`amb_export::video`]),
//! the size people post to Reels and Shorts.
//!
//! Frames are developed like a contact sheet (the edit without lens or
//! camera profiles) from a copy of the raw at about the video's size, on the
//! GPU when there is one.

use std::collections::BTreeMap;
use std::path::Path;

use amb_engine::buffer::Image;
use amb_engine::matching;
use amb_export::video::{Encoder, Frame, VideoSettings};
use amb_presets::Value;
use amb_protocol::DevelopState;

#[derive(Debug, Clone, Copy)]
pub struct ReplayOptions {
    /// The video's long side in pixels.
    pub long_edge: usize,
    pub fps: u32,
    /// Name each step at the bottom.
    pub captions: bool,
}

impl Default for ReplayOptions {
    fn default() -> Self {
        ReplayOptions {
            long_edge: 1080,
            fps: 30,
            captions: true,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Replay {
    /// Where it was written (set by the caller that knows).
    pub path: Option<String>,
    pub steps: usize,
    pub frames: usize,
    pub seconds: f64,
    pub width: usize,
    pub height: usize,
    pub bytes: u64,
}

/// Seconds the photo shows as it was, and as it ends.
const HOLD_START: f64 = 1.0;
const HOLD_END: f64 = 1.5;
/// About how long all the steps take together, and the bounds per step.
const STEPS_TOTAL: f64 = 10.0;
const STEP_MIN: f64 = 0.2;
const STEP_MAX: f64 = 0.7;
/// The pause after each step, as a share of its time.
const STEP_PAUSE: f64 = 0.3;

/// One step of the History: its caption and the settings after it.
struct Step {
    caption: String,
    settings: BTreeMap<String, Value>,
}

/// The settings before the first step and after each one, from `state`'s
/// History (the steps undone are left out).
fn steps(state: &DevelopState) -> (BTreeMap<String, Value>, Vec<Step>) {
    let mut settings = state.settings().clone();
    let mut out = Vec::new();
    for entry in state.undo_stack().iter().rev() {
        out.push(Step {
            caption: caption(entry),
            settings: settings.clone(),
        });
        for c in &entry.changes {
            match &c.before {
                Some(v) => settings.insert(c.name.clone(), v.clone()),
                None => settings.remove(&c.name),
            };
        }
    }
    out.reverse();
    (settings, out)
}

/// "Exposure +0.7" for a step that set one setting; the step's own label
/// otherwise.
fn caption(entry: &amb_protocol::HistoryEntry) -> String {
    let catalog = amb_protocol::catalog::Catalog::builtin();
    let name = |tag: &str| {
        catalog
            .text(tag)
            .map(|t| t.label.clone())
            .filter(|l| !l.is_empty())
            .unwrap_or_else(|| tag.trim_start_matches("amb:").to_string())
    };
    match entry.changes.as_slice() {
        [c] if matches!(c.after, Some(Value::Number(_)) | Some(Value::Bool(_))) => {
            format!(
                "{} {}",
                name(&c.name),
                amb_protocol::convert::display(&c.name, c.after.as_ref())
            )
        }
        [c] => name(&c.name),
        _ => entry
            .label
            .strip_prefix("apply_preset ")
            .map(|p| {
                let file = p.rsplit(['/', '\\']).next().unwrap_or(p);
                format!("Preset {}", file.trim_end_matches(".xmp"))
            })
            .unwrap_or_else(|| entry.label.clone()),
    }
}

/// Ease in and out (smoothstep).
fn ease(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// `from` moved toward `to` by `t`: numbers glide, the rest switch halfway.
fn between(
    from: &BTreeMap<String, Value>,
    to: &BTreeMap<String, Value>,
    t: f64,
) -> BTreeMap<String, Value> {
    let mut out = if t < 0.5 { from.clone() } else { to.clone() };
    // A number unset on one side glides from (or to) its default.
    let default = |k: &str| {
        amb_presets::registry::Registry::builtin()
            .lookup(k)
            .and_then(|i| i.default)
            .and_then(|d| d.as_f64())
    };
    let number = |m: &BTreeMap<String, Value>, k: &str| match m.get(k) {
        Some(Value::Number(v)) => Some(*v),
        None => default(k),
        _ => None,
    };
    for k in from.keys().chain(to.keys()) {
        if let (Some(a), Some(b)) = (number(from, k), number(to, k)) {
            if a != b {
                out.insert(k.clone(), Value::Number(a + (b - a) * t));
            }
        }
    }
    out
}

/// Make the replay of `photo` (its sidecar's History) into `out`.
/// `progress(done, total)` hears the frames as they are made.
pub fn replay(
    photo: &Path,
    out: &Path,
    o: &ReplayOptions,
    gpu: Option<&amb_gpu::Gpu>,
    progress: &dyn Fn(usize, usize),
) -> Result<Replay, String> {
    let sidecar = amb_engine::sidecar_path(photo);
    let state = DevelopState::load(&sidecar).map_err(|e| format!("{}: {e}", sidecar.display()))?;
    let (original, steps) = steps(&state);
    if steps.is_empty() {
        return Err("this photo has no History to replay: edit it first".into());
    }
    let source = amb_engine::source_path(photo);
    let full = amb_engine::raw::load(&source).map_err(|e| format!("{}: {e}", source.display()))?;
    // About the video's size (larger when the edit crops), so each frame
    // develops quickly.
    let last_settings = matching::develop_plain(&steps[steps.len() - 1].settings);
    let (cw, ch) = amb_engine::pipeline::output_size(&full, &last_settings);
    let crop = full.width.max(full.height) as f64 / cw.max(ch).max(1) as f64;
    let img =
        amb_engine::raw::downscale(&full, (o.long_edge.max(64) as f64 * crop.max(1.0)) as usize);
    drop(full);
    let orientation = img.metadata.orientation;
    let develop = |s: &BTreeMap<String, Value>| -> Image {
        let d = matching::develop_plain(s);
        let (out, _) = amb_gpu::full::render(gpu, &img, &d);
        amb_export::geometry::orient(&out, orientation)
    };
    // The video's size: the result's shape, `long_edge` on its long side,
    // both sides even.
    let last = develop(&steps[steps.len() - 1].settings);
    let scale = o.long_edge as f64 / last.width.max(last.height) as f64;
    let even = |v: f64| ((v.round() as usize) / 2 * 2).max(16);
    let (w, h) = (
        even(last.width as f64 * scale),
        even(last.height as f64 * scale),
    );
    let fps = f64::from(o.fps.max(1));
    let per_step = (STEPS_TOTAL / steps.len() as f64).clamp(STEP_MIN, STEP_MAX);
    let ease_frames = ((per_step * (1.0 - STEP_PAUSE) * fps).round() as usize).max(1);
    let pause_frames = ((per_step * STEP_PAUSE * fps).round() as usize).max(1);
    let total =
        ((HOLD_START + HOLD_END) * fps) as usize + steps.len() * (ease_frames + pause_frames);
    let mut enc = Encoder::new(VideoSettings {
        width: w,
        height: h,
        fps: o.fps.max(1),
        ..Default::default()
    })?;
    let px = (h as f32 / 22.0).max(12.0);
    let mut done = 0;
    let mut frame = |settings: &BTreeMap<String, Value>, caption: &str, times: usize| {
        let developed = develop(settings);
        let mut rgb = fit(&developed, w, h);
        if o.captions {
            amb_export::video::caption(&mut rgb, w, h, caption, px);
        }
        for _ in 0..times {
            enc.push(Frame { rgb: &rgb })?;
            done += 1;
            progress(done, total);
        }
        Ok::<(), String>(())
    };
    frame(&original, "Before", (HOLD_START * fps) as usize)?;
    let mut from = &original;
    for step in &steps {
        for f in 1..=ease_frames {
            let t = ease(f as f64 / ease_frames as f64);
            frame(&between(from, &step.settings, t), &step.caption, 1)?;
        }
        frame(&step.settings, &step.caption, pause_frames)?;
        from = &step.settings;
    }
    frame(from, "After", (HOLD_END * fps) as usize)?;
    let bytes = enc.finish(out)?;
    Ok(Replay {
        path: None,
        steps: steps.len(),
        frames: done,
        seconds: done as f64 / fps,
        width: w,
        height: h,
        bytes,
    })
}

/// `img` (linear ProPhoto) as 8-bit sRGB `w × h`, fitted inside it and
/// centred on black (an earlier step can have another crop).
fn fit(img: &Image, w: usize, h: usize) -> Vec<u8> {
    let s = (w as f64 / img.width as f64).min(h as f64 / img.height as f64);
    let (fw, fh) = (
        ((img.width as f64 * s).round() as usize).clamp(1, w),
        ((img.height as f64 * s).round() as usize).clamp(1, h),
    );
    let small = amb_export::geometry::resize(img, fw, fh);
    let srgb = amb_engine::output::to_srgb(&small);
    let (x0, y0) = ((w - fw) / 2, (h - fh) / 2);
    let mut out = vec![0u8; w * h * 3];
    for y in 0..fh {
        for x in 0..fw {
            for c in 0..3 {
                let v = srgb[(y * fw + x) * 3 + c];
                out[((y0 + y) * w + x0 + x) * 3 + c] = (v * 255.0).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use amb_protocol::Command;

    #[test]
    fn history_becomes_the_settings_at_each_step() {
        let mut state = DevelopState::from_settings(BTreeMap::new());
        let set = |name: &str, v: f64| Command::SetParam {
            name: name.into(),
            value: serde_json::json!(v),
        };
        state.execute(&set("Exposure2012", 0.7)).unwrap();
        state.execute(&set("Contrast2012", 20.0)).unwrap();
        let (original, steps) = steps(&state);
        assert!(original.is_empty());
        assert_eq!(steps.len(), 2);
        assert_eq!(
            steps[0].settings.get("Exposure2012"),
            Some(&Value::Number(0.7))
        );
        assert!(!steps[0].settings.contains_key("Contrast2012"));
        assert_eq!(steps[1].settings.len(), 2);
        assert_eq!(steps[0].caption, "Exposure +0.7");
        // Halfway between the two: Contrast glides.
        let mid = between(&steps[0].settings, &steps[1].settings, 0.5);
        assert_eq!(mid.get("Contrast2012"), Some(&Value::Number(10.0)));
    }
}
