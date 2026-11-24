// SPDX-License-Identifier: GPL-3.0-or-later
//! Registry of known Camera Raw setting tags: grouping, value kind, best-known
//! range/default, and our implementation status. Stored as data
//! (`data/tags.json`) so it can be corrected without code changes.

use std::collections::HashMap;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    /// Rendering support is planned; nothing renders yet.
    Planned,
    /// Rendered with an approximation of Adobe's behaviour.
    Approx,
    /// Rendered to (near) pixel parity.
    Exact,
    /// Depends on the specific image (crop, as-shot WB, auto): presets should rarely carry it.
    ImageSpecific,
    /// Describes the preset itself rather than an edit.
    Metadata,
    /// Deliberately not applied from presets (e.g. spot-removal coordinates).
    IgnoredInPresets,
    /// Recognised but not on the roadmap yet (AI/cloud-only features).
    Unsupported,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagInfo {
    pub name: String,
    pub group: String,
    pub kind: String,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub default: Option<serde_json::Value>,
    pub status: Status,
    /// A non-default value needs a learned model (AI-free mode refuses it).
    #[serde(default, skip_serializing_if = "is_false")]
    pub ai: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
}

#[derive(Debug, Deserialize)]
struct PrefixRule {
    prefix: String,
    group: String,
    kind: String,
    status: Status,
}

#[derive(Debug, Deserialize)]
struct RegistryFile {
    prefix_rules: Vec<PrefixRule>,
    tags: Vec<TagInfo>,
    /// `add_mask` kinds that need a model.
    ai_mask_kinds: Vec<String>,
    /// Lightroom `MaskSubType`s of `Mask/Image` that need a model.
    ai_mask_subtypes: Vec<String>,
    /// `add_spot` kinds that need a model.
    ai_spot_kinds: Vec<String>,
}

pub struct Registry {
    tags: HashMap<String, TagInfo>,
    prefix_rules: Vec<PrefixRule>,
    ai_mask_kinds: Vec<String>,
    ai_mask_subtypes: Vec<String>,
    ai_spot_kinds: Vec<String>,
}

impl Registry {
    /// The built-in registry.
    pub fn builtin() -> &'static Registry {
        static REGISTRY: OnceLock<Registry> = OnceLock::new();
        REGISTRY.get_or_init(|| {
            let file: RegistryFile = serde_json::from_str(include_str!("../data/tags.json"))
                .expect("data/tags.json is valid");
            Registry {
                tags: file.tags.into_iter().map(|t| (t.name.clone(), t)).collect(),
                prefix_rules: file.prefix_rules,
                ai_mask_kinds: file.ai_mask_kinds,
                ai_mask_subtypes: file.ai_mask_subtypes,
                ai_spot_kinds: file.ai_spot_kinds,
            }
        })
    }

    /// Look up a tag by exact name, falling back to prefix rules (e.g. `Table_<digest>`).
    pub fn lookup(&self, name: &str) -> Option<TagInfo> {
        if let Some(info) = self.tags.get(name) {
            return Some(info.clone());
        }
        self.prefix_rules
            .iter()
            .find(|r| name.starts_with(&r.prefix))
            .map(|r| TagInfo {
                name: name.to_string(),
                group: r.group.clone(),
                kind: r.kind.clone(),
                min: None,
                max: None,
                default: None,
                status: r.status,
                ai: false,
            })
    }

    /// Does a non-default value of this setting need a learned model?
    pub fn needs_ai(&self, name: &str) -> bool {
        self.tags.get(name).is_some_and(|t| t.ai)
    }

    /// Does an `add_mask` kind (`subject`, `sky`, …) need a learned model?
    pub fn mask_kind_needs_ai(&self, kind: &str) -> bool {
        let kind = kind.trim().to_ascii_lowercase();
        self.ai_mask_kinds.contains(&kind)
    }

    /// Does a Lightroom `Mask/Image` `MaskSubType` need a learned model?
    pub fn mask_subtype_needs_ai(&self, sub: &str) -> bool {
        self.ai_mask_subtypes.iter().any(|s| s == sub)
    }

    /// Does an `add_spot` kind (`remove`, …) need a learned model?
    pub fn spot_kind_needs_ai(&self, kind: &str) -> bool {
        let kind = kind.trim().to_ascii_lowercase();
        self.ai_spot_kinds.contains(&kind)
    }

    pub fn len(&self) -> usize {
        self.tags.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tags.is_empty()
    }

    /// Numeric default for a tag, if the registry knows one.
    pub fn numeric_default(&self, name: &str) -> Option<f64> {
        self.tags.get(name)?.default.as_ref()?.as_f64()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_and_looks_up() {
        let r = Registry::builtin();
        assert!(r.len() > 200);
        let exp = r.lookup("Exposure2012").unwrap();
        assert_eq!(exp.group, "tone");
        assert_eq!((exp.min, exp.max), (Some(-5.0), Some(5.0)));
        assert_eq!(r.lookup("Table_0123ABCD").unwrap().group, "profile");
        assert!(r.lookup("NotARealTag").is_none());
        assert_eq!(r.numeric_default("ColorGradeBlending"), Some(50.0));
        assert!(r.needs_ai("EnhanceDenoiseVersion"));
        assert!(!r.needs_ai("Exposure2012"));
        assert!(r.mask_kind_needs_ai("Subject") && !r.mask_kind_needs_ai("sky"));
        assert!(r.mask_subtype_needs_ai("3") && !r.mask_subtype_needs_ai("2"));
        assert!(r.spot_kind_needs_ai("remove") && !r.spot_kind_needs_ai("heal"));
    }
}
