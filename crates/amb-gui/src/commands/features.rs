// SPDX-License-Identifier: GPL-3.0-or-later
//! The features list ([docs/FEATURES.md](../../../../docs/FEATURES.md)) for
//! Search (Ctrl+K): each "- **what you want** → how" line, so typing a
//! problem ("whatsapp", "raw by mistake") finds the feature. Built into the
//! app, so the list in the docs and in the app are the same.

use serde::Serialize;

const FEATURES: &str = include_str!("../../../../docs/FEATURES.md");

#[derive(Debug, Clone, Serialize, PartialEq)]
pub(crate) struct Feature {
    pub(crate) section: String,
    /// What someone wants, as the list words it.
    pub(crate) want: String,
    /// Where it is and what to do, as plain text.
    pub(crate) how: String,
    /// `how` in pieces, `true` for a command to type (`amb ...`).
    pub(crate) parts: Vec<(bool, String)>,
    /// The names in bold in `how`: the buttons, menu items and settings
    /// that do it, for the app to offer.
    pub(crate) names: Vec<String>,
}

#[tauri::command]
pub(crate) fn features() -> Vec<Feature> {
    parse(FEATURES)
}

fn parse(text: &str) -> Vec<Feature> {
    let mut section = String::new();
    let mut out = Vec::new();
    for line in text.lines() {
        if let Some(s) = line.strip_prefix("## ") {
            section = s.trim().to_string();
            continue;
        }
        let Some(rest) = line.strip_prefix("- **") else {
            continue;
        };
        let Some((want, how)) = rest.split_once("** → ") else {
            continue;
        };
        let (parts, names) = pieces(how);
        let how = parts.iter().map(|(_, t)| t.as_str()).collect::<String>();
        out.push(Feature {
            section: section.clone(),
            want: want.trim().to_string(),
            how: how.trim().to_string(),
            parts,
            names,
        });
    }
    out
}

/// Markdown to plain text in pieces (links keep their words, no bold;
/// code spans their own pieces), and the bold names.
fn pieces(md: &str) -> (Vec<(bool, String)>, Vec<String>) {
    let mut parts: Vec<(bool, String)> = vec![(false, String::new())];
    let mut names = Vec::new();
    let mut rest = md;
    let text = |parts: &mut Vec<(bool, String)>, s: &str| match parts.last_mut() {
        Some((false, t)) => t.push_str(s),
        _ => parts.push((false, s.to_string())),
    };
    while !rest.is_empty() {
        if let Some(r) = rest.strip_prefix("**") {
            if let Some((name, after)) = r.split_once("**") {
                names.push(name.trim_end_matches('…').trim().to_string());
                text(&mut parts, name);
                rest = after;
                continue;
            }
        }
        if let Some(r) = rest.strip_prefix('[') {
            if let Some((words, after)) = r.split_once("](") {
                if let Some((_, after)) = after.split_once(')') {
                    text(&mut parts, words);
                    rest = after;
                    continue;
                }
            }
        }
        if let Some(r) = rest.strip_prefix('`') {
            if let Some((code, after)) = r.split_once('`') {
                parts.push((true, code.to_string()));
                rest = after;
                continue;
            }
        }
        let c = rest.chars().next().unwrap_or_default();
        text(&mut parts, &c.to_string());
        rest = &rest[c.len_utf8()..];
    }
    parts.retain(|(_, t)| !t.is_empty());
    (parts, names)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_features_list_reads_into_wants_and_hows() {
        let all = features();
        assert!(all.len() > 50, "{}", all.len());
        assert!(
            all.iter()
                .all(|f| !f.want.is_empty() && !f.how.is_empty() && !f.section.is_empty())
        );
        let convert = all
            .iter()
            .find(|f| f.want.contains("RAW by mistake"))
            .unwrap();
        assert!(convert.names.iter().any(|n| n == "Convert raws"));
        assert!(convert.how.contains("amb convert") && !convert.how.contains('`'));
        let (parts, names) = pieces("see [the guide](x.md), **Find dust**, `amb dust`.");
        assert_eq!(
            parts,
            [
                (false, "see the guide, Find dust, ".to_string()),
                (true, "amb dust".to_string()),
                (false, ".".to_string())
            ]
        );
        assert_eq!(names, ["Find dust"]);
    }
}
