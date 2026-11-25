// SPDX-License-Identifier: GPL-3.0-or-later
//! Will a photo be labelled "AI" on a social platform? What we can see of it:
//! the AI parts the edit holds, which an export would put in the picture
//! (AI-free mode off) or leave out (on).
//!
//! What we do not check is the source file's own markers (IPTC/XMP
//! `DigitalSourceType`, a C2PA manifest): an export is written with fresh
//! metadata only (`amb-export`'s `exif` and `embed`), nothing of the source's
//! XMP, IPTC or C2PA is copied, so a source's markers never reach the file.
//! Nor do we hide anything: we do not strip a marker, we do not write one. A
//! platform may still run its own checks on the pixels, so the verdict says
//! what is in the file, not what a platform will decide. See
//! `docs/design/ai-free.md`.

use std::collections::BTreeMap;
use std::path::Path;

use amb_presets::Value;

/// How a photo's edit stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// Nothing in the edit would be made by an AI model in the export.
    Clear,
    /// The edit holds AI parts, and AI-free mode is off: an export has them.
    AiInEdit,
}

/// What [`check`] found.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct AiLabel {
    pub verdict: Verdict,
    /// AI parts an export would carry (mode off), once each.
    pub edit_parts: Vec<String>,
    /// AI parts AI-free mode leaves out of an export (mode on): information;
    /// the verdict stays [`Verdict::Clear`].
    pub left_out: Vec<String>,
}

/// The verdict for an edit's `settings`, with AI-free mode as it is now.
pub fn check(settings: &BTreeMap<String, Value>) -> AiLabel {
    let parts = amb_presets::ai_free::left_out(settings);
    if amb_presets::ai_free::on() {
        return AiLabel {
            verdict: Verdict::Clear,
            edit_parts: Vec::new(),
            left_out: parts,
        };
    }
    AiLabel {
        verdict: if parts.is_empty() {
            Verdict::Clear
        } else {
            Verdict::AiInEdit
        },
        edit_parts: parts,
        left_out: Vec::new(),
    }
}

/// The same for `photo`'s own edit (its sidecar, or the Lightroom catalogue
/// it is linked to). No edit, or one that can't be read, is clear.
pub fn check_photo(photo: &Path) -> AiLabel {
    let settings = amb_engine::sidecar_text(photo)
        .and_then(|text| amb_presets::Preset::parse_xmp(&text).ok())
        .map(|p| p.settings)
        .unwrap_or_default();
    check(&settings)
}

/// The export warning for the parts AI-free mode left out, or `None`.
pub fn left_out_warning(left_out: &[String]) -> Option<String> {
    (!left_out.is_empty()).then(|| format!("left out for AI-free mode: {}", left_out.join(", ")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_ai() -> BTreeMap<String, Value> {
        let tag = "EnhanceDenoiseVersion";
        assert!(
            amb_presets::ai_free::needs_ai(tag),
            "{tag} is an AI setting"
        );
        BTreeMap::from([(tag.to_string(), Value::Number(1.0))])
    }

    #[test]
    fn an_edit_without_ai_is_clear() {
        let l = check(&BTreeMap::new());
        assert_eq!(l.verdict, Verdict::Clear);
        assert!(l.edit_parts.is_empty() && l.left_out.is_empty());
    }

    #[test]
    fn ai_in_the_edit_is_named_unless_the_mode_leaves_it_out() {
        // One test: the switch is process-wide.
        amb_presets::ai_free::set(false);
        let l = check(&with_ai());
        assert_eq!(l.verdict, Verdict::AiInEdit);
        assert_eq!(l.edit_parts.len(), 1);
        assert!(l.left_out.is_empty());
        amb_presets::ai_free::set(true);
        let l = check(&with_ai());
        amb_presets::ai_free::set(false);
        assert_eq!(l.verdict, Verdict::Clear);
        assert!(l.edit_parts.is_empty());
        assert_eq!(l.left_out.len(), 1);
    }

    #[test]
    fn the_warning_names_the_parts() {
        assert_eq!(left_out_warning(&[]), None);
        let w = left_out_warning(&["a".into(), "b".into()]).expect("a warning");
        assert_eq!(w, "left out for AI-free mode: a, b");
    }
}
