// SPDX-License-Identifier: GPL-3.0-or-later
//! Smart collections: photos chosen by rules, like Lightroom's, over the
//! folders you name (with their subfolders). They are worked out when
//! shown, from what the catalogue knows of each photo (its sidecar, and the
//! raw header's capture time, camera, lens and exposure).
//! Each is a small JSON file beside the ordinary collections:
//!
//! ```json
//! { "format": "ambrotype-smart-collection/1", "name": "Best of 2026",
//!   "folders": ["/photos/2026"], "match": "all",
//!   "rules": [ { "field": "rating", "op": ">=", "value": 4 },
//!              { "field": "captured", "op": "after", "value": "2026-06-01" } ] }
//! ```

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const FORMAT: &str = "ambrotype-smart-collection/1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Match {
    /// Every rule must hold.
    #[default]
    All,
    /// One rule is enough.
    Any,
}

/// One rule: a field, a test and a value.
///
/// | field | ops | value |
/// |---|---|---|
/// | `rating` | `=` `!=` `>=` `<=` | 0–5 (0: unrated) |
/// | `label` | `=` `!=` | "Red", ..., or "" for none |
/// | `flag` | `=` `!=` | "pick", "reject", or "" for none |
/// | `keyword` | `contains` `!contains` | text (any keyword containing it, any case) |
/// | `name` | `contains` `!contains` | text (in the file name, any case) |
/// | `edited` | `=` | true or false |
/// | `captured` | `after` `before` | "YYYY-MM-DD" (after: on or after; before: earlier) |
/// | `camera`, `lens` | `contains` `!contains` | text (any case) |
/// | `iso`, `focal` | `>=` `<=` | a number (focal length in mm) |
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rule {
    pub field: String,
    pub op: String,
    pub value: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SmartCollection {
    pub format: String,
    pub name: String,
    /// Where the photos are looked for, with their subfolders.
    pub folders: Vec<PathBuf>,
    #[serde(rename = "match", default)]
    pub matching: Match,
    pub rules: Vec<Rule>,
}

/// What the rules can ask about one photo.
#[derive(Debug, Clone, Default)]
pub struct Facts<'a> {
    pub name: &'a str,
    pub rating: Option<u8>,
    pub label: Option<&'a str>,
    /// "pick" or "reject".
    pub flag: Option<&'a str>,
    pub keywords: &'a [String],
    pub edited: bool,
    /// "YYYY-MM-DD HH:MM:SS".
    pub captured: Option<&'a str>,
    pub camera: Option<&'a str>,
    pub lens: Option<&'a str>,
    pub iso: Option<u32>,
    /// Millimetres.
    pub focal: Option<f64>,
}

const FIELDS: [(&str, &[&str]); 11] = [
    ("rating", &["=", "!=", ">=", "<="]),
    ("label", &["=", "!="]),
    ("flag", &["=", "!="]),
    ("keyword", &["contains", "!contains"]),
    ("name", &["contains", "!contains"]),
    ("edited", &["="]),
    ("captured", &["after", "before"]),
    ("camera", &["contains", "!contains"]),
    ("lens", &["contains", "!contains"]),
    ("iso", &[">=", "<="]),
    ("focal", &[">=", "<="]),
];

impl Rule {
    /// Why the rule can't be used, if it can't.
    pub fn check(&self) -> Result<(), String> {
        let ops = FIELDS
            .iter()
            .find(|(f, _)| *f == self.field)
            .map(|(_, ops)| *ops)
            .ok_or_else(|| {
                format!(
                    "unknown field {:?} ({})",
                    self.field,
                    FIELDS.map(|(f, _)| f).join(", ")
                )
            })?;
        if !ops.contains(&self.op.as_str()) {
            return Err(format!(
                "{} can't use {:?} ({})",
                self.field,
                self.op,
                ops.join(", ")
            ));
        }
        let ok = match self.field.as_str() {
            "rating" => self.value.as_u64().is_some_and(|v| v <= 5),
            "edited" => self.value.is_boolean(),
            "captured" => self.value.as_str().is_some_and(is_date),
            "iso" | "focal" => self.value.as_f64().is_some_and(|v| v >= 0.0),
            _ => self.value.is_string(),
        };
        if ok {
            Ok(())
        } else {
            Err(format!("{}: bad value {}", self.field, self.value))
        }
    }

    /// Does the photo meet the rule? A rule that can't be used never does.
    pub fn holds(&self, f: &Facts) -> bool {
        if self.check().is_err() {
            return false;
        }
        let text = self.value.as_str().unwrap_or("");
        let lower = text.to_lowercase();
        match (self.field.as_str(), self.op.as_str()) {
            ("rating", op) => {
                let (r, v) = (
                    f.rating.unwrap_or(0) as u64,
                    self.value.as_u64().unwrap_or(0),
                );
                match op {
                    "=" => r == v,
                    "!=" => r != v,
                    ">=" => r >= v,
                    _ => r <= v,
                }
            }
            ("label" | "flag", op) => {
                let have = if self.field == "label" {
                    f.label
                } else {
                    f.flag
                };
                let same = have.unwrap_or("").eq_ignore_ascii_case(text);
                (op == "=") == same
            }
            ("keyword", op) => {
                let has = f.keywords.iter().any(|k| k.to_lowercase().contains(&lower));
                (op == "contains") == has
            }
            ("name", op) => (op == "contains") == f.name.to_lowercase().contains(&lower),
            ("camera" | "lens", op) => {
                let have = if self.field == "camera" {
                    f.camera
                } else {
                    f.lens
                };
                // Unknown never matches "contains", and always "doesn't contain".
                let has = have.is_some_and(|h| h.to_lowercase().contains(&lower));
                (op == "contains") == has
            }
            ("iso" | "focal", op) => {
                let have = if self.field == "iso" {
                    f.iso.map(f64::from)
                } else {
                    f.focal
                };
                let v = self.value.as_f64().unwrap_or(0.0);
                have.is_some_and(|h| if op == ">=" { h >= v } else { h <= v })
            }
            ("edited", _) => self.value.as_bool() == Some(f.edited),
            ("captured", op) => match f.captured.map(|c| &c[..c.len().min(10)]) {
                // Dates as text compare in order.
                Some(day) if op == "after" => day >= text,
                Some(day) => day < text,
                None => false,
            },
            _ => false,
        }
    }
}

fn is_date(s: &str) -> bool {
    let b = s.as_bytes();
    s.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && s.chars()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
}

impl SmartCollection {
    pub fn new(name: &str) -> SmartCollection {
        SmartCollection {
            format: FORMAT.into(),
            name: name.trim().to_string(),
            folders: Vec::new(),
            matching: Match::All,
            rules: Vec::new(),
        }
    }

    /// Why it can't be used, if it can't.
    pub fn check(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("a smart collection needs a name".into());
        }
        if self.folders.is_empty() {
            return Err("choose at least one folder to look in".into());
        }
        if self.rules.is_empty() {
            return Err("add at least one rule".into());
        }
        self.rules.iter().try_for_each(Rule::check)
    }

    /// Does the photo belong?
    pub fn matches(&self, f: &Facts) -> bool {
        match self.matching {
            Match::All => self.rules.iter().all(|r| r.holds(f)),
            Match::Any => self.rules.iter().any(|r| r.holds(f)),
        }
    }
}

fn read(path: &Path) -> Option<SmartCollection> {
    let s: SmartCollection = serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
    (s.format == FORMAT).then_some(s)
}

fn files(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect()
}

/// Every smart collection in `dir`, by name.
pub fn all(dir: &Path) -> Vec<SmartCollection> {
    let mut out: Vec<SmartCollection> = files(dir).iter().filter_map(|p| read(p)).collect();
    out.sort_by_key(|c| c.name.to_lowercase());
    out
}

/// The smart collection called `name` (any case), and its file.
pub fn find(dir: &Path, name: &str) -> Option<(PathBuf, SmartCollection)> {
    let want = name.trim().to_lowercase();
    files(dir).into_iter().find_map(|p| {
        read(&p)
            .filter(|s| s.name.to_lowercase() == want)
            .map(|s| (p, s))
    })
}

/// Save `s`, replacing the one called `previous` (or its own name) if
/// there is one.
pub fn save(dir: &Path, s: &SmartCollection, previous: Option<&str>) -> Result<(), String> {
    s.check()?;
    let old = find(dir, previous.unwrap_or(&s.name));
    if let Some((_, other)) = find(dir, &s.name) {
        let renaming_onto_another = old.as_ref().is_none_or(|(_, o)| o.name != other.name);
        if renaming_onto_another {
            return Err(format!("there is already a smart collection {:?}", s.name));
        }
    }
    let path = match old {
        Some((p, _)) => p,
        None => {
            let slug: String = s
                .name
                .chars()
                .map(|c| {
                    if c.is_alphanumeric() {
                        c.to_ascii_lowercase()
                    } else {
                        '-'
                    }
                })
                .collect();
            let slug = slug.trim_matches('-').to_string();
            (1..)
                .map(|n| {
                    let suffix = if n == 1 {
                        String::new()
                    } else {
                        format!("-{n}")
                    };
                    dir.join(format!("smart-{slug}{suffix}.json"))
                })
                .find(|p| !p.exists())
                .expect("some name is free")
        }
    };
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let text = serde_json::to_string_pretty(s).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, text).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("{}: {e}", path.display()))
}

/// Delete a smart collection (not its photos).
pub fn delete(dir: &Path, name: &str) -> Result<(), String> {
    let (path, _) = find(dir, name).ok_or_else(|| format!("no smart collection {name:?}"))?;
    std::fs::remove_file(&path).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn rule(field: &str, op: &str, value: Value) -> Rule {
        Rule {
            field: field.into(),
            op: op.into(),
            value,
        }
    }

    #[test]
    fn rules_test_what_they_say() {
        let kw = vec!["Beach".to_string(), "family".to_string()];
        let f = Facts {
            name: "IMG_0042.NEF",
            rating: Some(4),
            label: Some("Red"),
            flag: Some("pick"),
            keywords: &kw,
            edited: true,
            captured: Some("2026-07-14 10:00:00"),
            camera: Some("Nikon D750"),
            lens: Some("SP 35mm f/1.8"),
            iso: Some(400),
            focal: Some(35.0),
        };
        let yes = [
            rule("rating", ">=", json!(4)),
            rule("rating", "<=", json!(4)),
            rule("rating", "!=", json!(2)),
            rule("label", "=", json!("red")),
            rule("flag", "=", json!("pick")),
            rule("keyword", "contains", json!("beach")),
            rule("keyword", "!contains", json!("work")),
            rule("name", "contains", json!("0042")),
            rule("edited", "=", json!(true)),
            rule("captured", "after", json!("2026-07-14")),
            rule("captured", "before", json!("2026-07-15")),
            rule("camera", "contains", json!("d750")),
            rule("lens", "!contains", json!("85mm")),
            rule("iso", ">=", json!(400)),
            rule("focal", "<=", json!(50)),
        ];
        for r in &yes {
            assert!(r.holds(&f), "{r:?}");
        }
        let no = [
            rule("rating", "=", json!(5)),
            rule("label", "=", json!("")),
            rule("flag", "!=", json!("pick")),
            rule("captured", "before", json!("2026-07-14")),
            rule("edited", "=", json!(false)),
            rule("camera", "contains", json!("canon")),
            rule("iso", "<=", json!(200)),
            rule("focal", ">=", json!(50.5)),
        ];
        for r in &no {
            assert!(!r.holds(&f), "{r:?}");
        }
        // An unrated, unlabelled photo with no date.
        let bare = Facts {
            name: "x.nef",
            ..Default::default()
        };
        assert!(rule("rating", "=", json!(0)).holds(&bare));
        assert!(rule("label", "=", json!("")).holds(&bare));
        assert!(!rule("captured", "after", json!("2000-01-01")).holds(&bare));
        // Unknown camera details: "contains" fails, "doesn't contain" holds,
        // numbers never match.
        assert!(!rule("camera", "contains", json!("")).holds(&bare));
        assert!(rule("lens", "!contains", json!("35")).holds(&bare));
        assert!(!rule("iso", ">=", json!(0)).holds(&bare));
    }

    #[test]
    fn bad_rules_are_refused() {
        for r in [
            rule("stars", ">=", json!(3)),
            rule("rating", "contains", json!(3)),
            rule("rating", ">=", json!(9)),
            rule("captured", "after", json!("July")),
            rule("edited", "=", json!("yes")),
            rule("iso", "=", json!(100)),
            rule("focal", ">=", json!("wide")),
        ] {
            assert!(r.check().is_err(), "{r:?}");
            assert!(!r.holds(&Facts::default()), "{r:?}");
        }
    }

    #[test]
    fn all_or_any_and_storage() {
        let dir = std::env::temp_dir().join(format!("amb-smart-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut s = SmartCollection::new("Best");
        assert!(s.check().is_err());
        s.folders = vec![PathBuf::from("/photos")];
        s.rules = vec![
            rule("rating", ">=", json!(4)),
            rule("flag", "=", json!("pick")),
        ];
        let f = Facts {
            rating: Some(5),
            ..Default::default()
        };
        assert!(!s.matches(&f));
        s.matching = Match::Any;
        assert!(s.matches(&f));

        save(&dir, &s, None).unwrap();
        // An ordinary collection beside it isn't read as one.
        crate::collections::add(&dir, "Plain", &[PathBuf::from("/a.nef")]).unwrap();
        assert_eq!(all(&dir), vec![s.clone()]);
        assert_eq!(crate::collections::list(&dir).len(), 1);
        // Rename in place; a clash is refused.
        let mut renamed = s.clone();
        renamed.name = "Top".into();
        save(&dir, &renamed, Some("Best")).unwrap();
        assert_eq!(all(&dir).len(), 1);
        assert_eq!(find(&dir, "top").unwrap().1.name, "Top");
        let mut other = s.clone();
        other.name = "Other".into();
        save(&dir, &other, None).unwrap();
        other.name = "Top".into();
        assert!(save(&dir, &other, Some("Other")).is_err());
        delete(&dir, "Top").unwrap();
        assert_eq!(all(&dir).len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
