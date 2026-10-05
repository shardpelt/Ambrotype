//! The documented setting catalog: every registry tag in registry order, its
//! group, and the photographer-oriented description from
//! `data/descriptions.json`. Validation itself goes through
//! [`amb_presets::registry::Registry`] so the two can never disagree.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use amb_presets::registry::{Registry, TagInfo};
use serde::{Deserialize, Serialize};

/// Human-facing text for one tag.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct TagText {
    /// The control's name in the Lightroom UI, e.g. "Exposure".
    pub label: String,
    /// What the control does and what its range means.
    pub description: String,
    /// Units, when the value is not a plain slider position ("stops", "K", ...).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub units: Option<String>,
}

/// A registry group with its documentation.
#[derive(Debug, Clone, Serialize)]
pub struct GroupInfo {
    pub name: String,
    pub label: String,
    pub description: String,
    /// Tag names in registry order.
    pub tags: Vec<String>,
}

#[derive(Deserialize)]
struct GroupText {
    label: String,
    description: String,
}

#[derive(Deserialize)]
struct BandTemplate {
    prefix: String,
    label: String,
    description: String,
}

#[derive(Deserialize)]
struct DescriptionsFile {
    groups: BTreeMap<String, GroupText>,
    bands: Vec<String>,
    band_templates: Vec<BandTemplate>,
    tags: BTreeMap<String, TagText>,
}

#[derive(Deserialize)]
struct TagsFile {
    tags: Vec<TagInfo>,
}

pub struct Catalog {
    tags: Vec<TagInfo>,
    groups: Vec<GroupInfo>,
    texts: BTreeMap<String, TagText>,
}

impl Catalog {
    /// The built-in catalog (registry `tags.json` plus `descriptions.json`).
    pub fn builtin() -> &'static Catalog {
        static CATALOG: OnceLock<Catalog> = OnceLock::new();
        CATALOG.get_or_init(|| {
            // The registry does not expose iteration, so read the same data file
            // for ordering; lookups still go through `Registry`.
            let tags: TagsFile =
                serde_json::from_str(include_str!("../../amb-presets/data/tags.json"))
                    .expect("amb-presets/data/tags.json is valid");
            let file: DescriptionsFile =
                serde_json::from_str(include_str!("../data/descriptions.json"))
                    .expect("data/descriptions.json is valid");
            let mut texts = file.tags;
            for template in &file.band_templates {
                for band in &file.bands {
                    let fill = |s: &str| {
                        s.replace("{band}", band)
                            .replace("{band_lower}", &band.to_lowercase())
                    };
                    texts.insert(
                        format!("{}{band}", template.prefix),
                        TagText {
                            label: fill(&template.label),
                            description: fill(&template.description),
                            units: None,
                        },
                    );
                }
            }
            let mut groups: Vec<GroupInfo> = Vec::new();
            for tag in &tags.tags {
                match groups.iter_mut().find(|g| g.name == tag.group) {
                    Some(g) => g.tags.push(tag.name.clone()),
                    None => {
                        let text = file.groups.get(&tag.group);
                        groups.push(GroupInfo {
                            name: tag.group.clone(),
                            label: text.map_or_else(|| tag.group.clone(), |t| t.label.clone()),
                            description: text.map(|t| t.description.clone()).unwrap_or_default(),
                            tags: vec![tag.name.clone()],
                        });
                    }
                }
            }
            Catalog {
                tags: tags.tags,
                groups,
                texts,
            }
        })
    }

    /// All registry tags, in registry order.
    pub fn tags(&self) -> &[TagInfo] {
        &self.tags
    }

    /// All groups, in registry order.
    pub fn groups(&self) -> &[GroupInfo] {
        &self.groups
    }

    pub fn group(&self, name: &str) -> Option<&GroupInfo> {
        self.groups.iter().find(|g| g.name == name)
    }

    /// Registry information for a tag (exact name or prefix rule).
    pub fn lookup(&self, name: &str) -> Option<TagInfo> {
        Registry::builtin().lookup(name)
    }

    /// Description of a tag, if documented.
    pub fn text(&self, name: &str) -> Option<&TagText> {
        self.texts.get(name)
    }

    /// Up to five known tag names close to `name`, best first. Matches on the
    /// tag name and on its UI label ("Temp" finds `Temperature`, "Vignette"
    /// finds `PostCropVignetteAmount`).
    pub fn suggest(&self, name: &str) -> Vec<String> {
        let wanted = normalise(name);
        if wanted.is_empty() {
            return Vec::new();
        }
        let mut scored: Vec<(usize, usize, &str)> = Vec::new();
        for (order, tag) in self.tags.iter().enumerate() {
            let mut best = usize::MAX;
            let mut keys = vec![normalise(&tag.name)];
            if let Some(text) = self.texts.get(&tag.name) {
                keys.push(normalise(&text.label));
            }
            for key in &keys {
                let score = if *key == wanted {
                    0
                } else if key.starts_with(&wanted) || wanted.starts_with(key.as_str()) {
                    1 + key.len().abs_diff(wanted.len()).min(20) / 4
                } else if key.contains(&wanted) {
                    3 + key.len().abs_diff(wanted.len()).min(20) / 4
                } else {
                    let d = levenshtein(key, &wanted);
                    let limit = (wanted.len().max(key.len()) / 3).max(2);
                    if d <= limit { 4 + d } else { usize::MAX }
                };
                best = best.min(score);
            }
            if best != usize::MAX {
                scored.push((best, order, &tag.name));
            }
        }
        scored.sort();
        scored
            .into_iter()
            .take(5)
            .map(|(_, _, n)| n.to_string())
            .collect()
    }

    /// Close group names for an unknown group.
    pub fn suggest_group(&self, name: &str) -> Vec<String> {
        let wanted = normalise(name);
        let mut scored: Vec<(usize, &str)> = self
            .groups
            .iter()
            .filter_map(|g| {
                let key = normalise(&g.name);
                let label = normalise(&g.label);
                let d = if key.contains(&wanted) || label.contains(&wanted) {
                    0
                } else {
                    levenshtein(&key, &wanted)
                };
                (d <= (wanted.len() / 3).max(2)).then_some((d, g.name.as_str()))
            })
            .collect();
        scored.sort();
        scored.into_iter().take(3).map(|(_, n)| n.into()).collect()
    }
}

fn normalise(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

fn levenshtein(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0; b.len() + 1];
    for (i, ca) in a.chars().enumerate() {
        cur[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != *cb);
            cur[j + 1] = (prev[j] + cost).min(prev[j + 1] + 1).min(cur[j] + 1);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_follow_registry_order() {
        let c = Catalog::builtin();
        assert_eq!(c.groups()[0].name, "metadata");
        let tone = c.group("tone").unwrap();
        assert_eq!(tone.tags[0], "Exposure2012");
        assert_eq!(tone.label, "Basic: Tone");
        assert!(c.tags().len() > 200);
    }

    #[test]
    fn templated_band_descriptions() {
        let c = Catalog::builtin();
        let t = c.text("SaturationAdjustmentAqua").unwrap();
        assert_eq!(t.label, "Saturation: Aqua");
        assert!(t.description.contains("aqua colours"));
        assert!(c.text("GrayMixerMagenta").is_some());
    }

    #[test]
    fn required_groups_are_fully_documented() {
        let c = Catalog::builtin();
        let required = [
            "white_balance",
            "tone",
            "presence",
            "tone_curve",
            "hsl",
            "color_grading",
            "detail",
            "effects",
            "lens",
            "calibration",
        ];
        for group in required {
            for tag in &c.group(group).unwrap().tags {
                let text = c.text(tag).unwrap_or_else(|| panic!("{tag} undocumented"));
                assert!(
                    !text.label.is_empty() && text.description.len() > 20,
                    "{tag}"
                );
            }
        }
        // In fact every registry tag is documented.
        let missing: Vec<&str> = c
            .tags()
            .iter()
            .filter(|t| c.text(&t.name).is_none())
            .map(|t| t.name.as_str())
            .collect();
        assert!(missing.is_empty(), "undocumented: {missing:?}");
    }

    #[test]
    fn descriptions_only_name_registry_tags() {
        let c = Catalog::builtin();
        for name in c.texts.keys() {
            assert!(c.lookup(name).is_some(), "{name} is not a registry tag");
        }
    }

    #[test]
    fn suggestions() {
        let c = Catalog::builtin();
        assert_eq!(c.suggest("Exposure2021")[0], "Exposure2012");
        assert!(c.suggest("exposure").contains(&"Exposure2012".to_string()));
        assert_eq!(c.suggest("Temp")[0], "Temperature");
        assert!(c.suggest("Vibrence").contains(&"Vibrance".to_string()));
        assert!(
            c.suggest("vignette")
                .contains(&"PostCropVignetteAmount".to_string())
        );
        assert!(c.suggest("zzzzzzzzzzzz").is_empty());
        assert_eq!(c.suggest_group("colour_grading"), vec!["color_grading"]);
    }
}
