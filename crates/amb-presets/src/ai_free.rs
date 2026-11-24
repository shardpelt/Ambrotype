// SPDX-License-Identifier: GPL-3.0-or-later
//! AI-free mode: one process-wide switch. When it is on, no learned model
//! runs on the user's photos, by any path (editor, `amb`, MCP server,
//! plugins, preview, export, batch operations, search).
//!
//! It lives in the bottom crate so every crate that loads or runs a model can
//! ask it. The saved setting is `amb_library::ai_free`; each program reads it
//! at start-up and calls [`set`]. Every model path calls [`check`] first;
//! `crates/amb-cli/tests/code_health.rs` fails when one doesn't.
//!
//! An outside AI agent driving our commands is fine: it only moves the same
//! settings a person would. The switch is about models touching pixels.
//! Heuristic features that learn nothing (the Sky mask's shape, auto tone)
//! are not AI and stay.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::masks::{self, MaskKind, flag};
use crate::registry::Registry;
use crate::retouch::{self, SpotKind};
use crate::value::Value;

static ON: AtomicBool = AtomicBool::new(false);

/// What a refused call says.
pub const MESSAGE: &str = "AI-free mode is on: no AI model runs on your photos. Turn it off in Settings, or with `amb ai-free off`.";

/// The engine hook setters, each gated at call time (the hook stays
/// installed so the editor can switch at run time). A source guard in
/// `crates/amb-cli/tests/code_health.rs` checks that every `pub fn set_…` in
/// `amb-engine` is listed here (or is `plugin_effects::set_resolver`, which
/// resolves plugin effects and runs no model).
pub const GATED_HOOKS: &[&str] = &[
    "subject::set_finder",
    "subject::set_selector",
    "subject::set_face_finder",
    "retouch::set_inpainter",
    "raw_denoise::set_raw_denoiser",
];

/// Turn the mode on or off for this process.
pub fn set(on: bool) {
    ON.store(on, Ordering::SeqCst);
}

/// Is the mode on?
pub fn on() -> bool {
    ON.load(Ordering::SeqCst)
}

/// `Err(MESSAGE)` when the mode is on: the first line of anything that loads,
/// fetches or runs a model.
pub fn check() -> Result<(), String> {
    if on() {
        Err(MESSAGE.to_string())
    } else {
        Ok(())
    }
}

/// The AI parts `after` holds that `before` did not (or holds more of): what
/// a change would add. The command path refuses a change when this isn't
/// empty while the mode is on.
pub fn added(before: &BTreeMap<String, Value>, after: &BTreeMap<String, Value>) -> Vec<String> {
    let mut had = ai_parts(before);
    let mut out = Vec::new();
    for part in ai_parts(after) {
        match had.iter().position(|p| *p == part) {
            Some(i) => {
                had.swap_remove(i);
            }
            None => out.push(part),
        }
    }
    out.dedup();
    out
}

/// Does a non-default value of the setting `tag` need a learned model?
/// (The registry's `ai` flag, `data/tags.json`.)
pub fn needs_ai(tag: &str) -> bool {
    Registry::builtin().needs_ai(tag)
}

/// Is `v` the same as leaving an AI setting unset: off, zero or empty?
pub fn is_off(v: &Value) -> bool {
    match v {
        Value::Bool(b) => !b,
        Value::Number(n) => *n == 0.0,
        Value::Text(t) => {
            let t = t.trim();
            t.is_empty() || t == "0" || t.eq_ignore_ascii_case("false")
        }
        Value::Seq(s) => s.is_empty(),
        Value::Alt(a) => a.is_empty(),
        Value::Struct(m) => m.is_empty(),
    }
}

/// The AI parts an edit holds (an old sidecar, a preset, a Lightroom XMP),
/// in plain words, once each: what AI-free mode leaves out when it renders
/// or exports the edit. Empty when it holds none.
pub fn left_out(settings: &BTreeMap<String, Value>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for part in ai_parts(settings) {
        if !out.contains(&part) {
            out.push(part);
        }
    }
    out
}

/// The AI parts an edit holds the way [`left_out`] names them, one entry for
/// each part (so two Select Subject masks are two entries).
pub fn ai_parts(settings: &BTreeMap<String, Value>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut add = |s: String| out.push(s);
    let reg = Registry::builtin();
    for (name, v) in settings {
        if reg.needs_ai(name) && !is_off(v) {
            add(format!("{name} (an AI setting)"));
        }
    }
    let masks_on = settings
        .get(masks::MASK_ENABLE_TAG)
        .and_then(Value::as_bool)
        .unwrap_or(true);
    if let (true, Some(Value::Seq(items))) = (masks_on, settings.get(masks::MASK_TAG)) {
        for item in items {
            let Value::Struct(c) = item else { continue };
            if flag(c, "CorrectionActive") == Some(false) {
                continue;
            }
            let comps = masks::components(c);
            for range in masks::parts(comps) {
                let Some(Value::Struct(first)) = comps.get(range.start) else {
                    continue;
                };
                if flag(first, "MaskActive") == Some(false) {
                    continue;
                }
                if let MaskKind::Ai(sub) = MaskKind::of(first) {
                    if reg.mask_subtype_needs_ai(&sub) {
                        let kind = MaskKind::Ai(sub);
                        add(format!("{} mask", kind.label()));
                    }
                }
            }
        }
    }
    let spots_on = settings
        .get(retouch::ENABLE_TAG)
        .and_then(Value::as_bool)
        .unwrap_or(true);
    if spots_on {
        for area in retouch::areas(settings) {
            let Value::Struct(a) = area else { continue };
            let ai =
                SpotKind::of(a).is_some_and(|k| k.synthesised()) && retouch::source(a).is_none();
            if ai {
                add("AI Remove spot".to_string());
            }
        }
    }
    out
}

/// Serialises tests that flip the process-wide switch (tests run in
/// parallel). Hold the guard for the whole test; it turns the mode off again
/// when dropped.
#[doc(hidden)]
pub struct TestGuard {
    _lock: std::sync::MutexGuard<'static, ()>,
}

impl Drop for TestGuard {
    fn drop(&mut self) {
        set(false);
    }
}

/// Take the test lock with the mode off.
#[doc(hidden)]
pub fn test_lock() -> TestGuard {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    set(false);
    TestGuard { _lock: g }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_follows_the_switch() {
        let _g = test_lock();
        assert!(check().is_ok());
        set(true);
        assert_eq!(check(), Err(MESSAGE.to_string()));
        assert!(on());
    }

    #[test]
    fn names_the_ai_parts_an_edit_holds() {
        let mut s = BTreeMap::new();
        assert!(left_out(&s).is_empty());
        s.insert("Exposure2012".into(), Value::Number(1.0));
        s.insert(
            "EnhanceDenoiseAlreadyApplied".into(),
            Value::Text("False".into()),
        );
        assert!(
            left_out(&s).is_empty(),
            "an AI setting that is off holds nothing"
        );
        s.insert("EnhanceDenoiseVersion".into(), Value::Text("1".into()));
        assert_eq!(left_out(&s).len(), 1);
    }
}
