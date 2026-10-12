// SPDX-License-Identifier: GPL-3.0-or-later
//! The short references for AI clients (`amb ref`): the core card
//! (`docs/ai/core.md`: the compact syntax and the most used settings, a few
//! hundred tokens) and one compact page per group. Generated from the tag
//! registry, so ranges can't drift.

use std::fmt::Write as _;

use amb_presets::registry::{Status, TagInfo};
use amb_presets::xmp_writer::format_number;

use crate::catalog::Catalog;
use crate::compact::ALIASES;

/// Command that regenerates the committed core card.
pub const REGENERATE: &str = "cargo run -p amb-cli -- ref > docs/ai/core.md";

/// The core card's budget, in tokens estimated at 4 characters each.
pub const CORE_BUDGET: usize = 400;

/// `lo..hi =default`, as short as it gets.
fn range(t: &TagInfo) -> String {
    let n = |v: f64| format_number(v, false);
    let mut s = match (t.kind.as_str(), t.min, t.max) {
        (_, Some(lo), Some(hi)) => format!("{}..{}", n(lo), n(hi)),
        ("bool", _, _) => "true|false".into(),
        ("curve", _, _) => "points 0..255".into(),
        _ => t.kind.clone(),
    };
    match &t.default {
        Some(serde_json::Value::Number(d)) => {
            let _ = write!(s, " ={}", n(d.as_f64().unwrap_or(0.0)));
        }
        Some(serde_json::Value::Bool(b)) => {
            let _ = write!(s, " ={b}");
        }
        Some(serde_json::Value::String(v)) if !v.is_empty() => {
            let _ = write!(s, " ={v}");
        }
        _ => {}
    }
    s
}

/// The core card. Deterministic; ends with a newline.
pub fn core() -> String {
    let c = Catalog::builtin();
    let mut out = String::new();
    let _ = writeln!(out, "# Ambrotype core card");
    let _ = writeln!(
        out,
        "Edit: `amb do PHOTO \"exp=+0.5 con+=10 reset:hsl\"` [--dry-run]; MCP edit_photo {{photo, do}}. One undo step; prints net changes."
    );
    let _ = writeln!(
        out,
        "Items: Name=v Name+=d Name-=d reset:Name|group preset:PATH[@amount] sky:Name=v,… (a sky mask) snap:N load:N undo redo"
    );
    let _ = writeln!(
        out,
        "Check: `amb stats PHOTO` (L*, clipping, cast) before a picture (`amb render PHOTO out.jpg --long-edge 800`)."
    );
    let _ = writeln!(
        out,
        "Match a look: `amb match PHOTO REF.jpg...` (shows changes; --apply)."
    );
    let groups: Vec<&str> = c.groups().iter().map(|g| g.name.as_str()).collect();
    let _ = writeln!(out, "More: `amb ref GROUP`: {}", groups.join(" "));
    let _ = writeln!(out, "Short name, setting, range =default:");
    for (short, full) in ALIASES {
        let Some(t) = c.lookup(full) else { continue };
        let mut r = range(&t);
        if t.name == "Temperature" {
            r += " K (as shot)";
        }
        let _ = writeln!(out, "{short} {full} {r}");
    }
    let _ = writeln!(
        out,
        "HSL: {{Hue,Saturation,Luminance}}Adjustment{{Red,Orange,Yellow,Green,Aqua,Blue,Purple,Magenta}} -100..100"
    );
    let _ = writeln!(
        out,
        "Grading: ColorGrade{{Shadow,Midtone,Highlight,Global}}{{Hue 0..359,Sat 0..100,Lum -100..100}}"
    );
    let _ = writeln!(out, "B&W: ConvertToGrayscale=true");
    out
}

/// One group's settings, one line each: `Name range =default [status]`.
pub fn group(name: &str) -> Option<String> {
    let c = Catalog::builtin();
    let g = c.group(name)?;
    let mut out = String::new();
    let _ = writeln!(out, "# {} ({})", g.label, g.name);
    for tag in &g.tags {
        let Some(t) = c.lookup(tag) else { continue };
        let status = match t.status {
            Status::Approx | Status::Exact => "",
            Status::Planned => " [not rendered]",
            Status::ImageSpecific => " [per photo]",
            Status::Metadata => " [metadata]",
            Status::IgnoredInPresets => " [not from presets]",
            Status::Unsupported => " [unsupported]",
        };
        let label = c
            .text(tag)
            .map(|x| x.label.as_str())
            .filter(|l| !l.is_empty() && !tag.starts_with(*l));
        let label = label.map_or(String::new(), |l| format!(" ({l})"));
        let _ = writeln!(out, "{tag} {}{label}{status}", range(&t));
    }
    Some(out)
}

/// Rough token count: 4 characters a token, as for English and code.
pub fn estimate_tokens(text: &str) -> usize {
    text.chars().count().div_ceil(4)
}

#[cfg(test)]
mod tests {
    use super::*;

    const COMMITTED: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/ai/core.md");

    #[test]
    fn committed_core_card_is_up_to_date_and_within_budget() {
        let fresh = core();
        if std::env::var_os("AMB_UPDATE_DOCS").is_some() {
            std::fs::write(COMMITTED, &fresh).unwrap();
        }
        let committed = std::fs::read_to_string(COMMITTED).unwrap_or_default();
        assert!(
            committed == fresh,
            "docs/ai/core.md is stale; regenerate with: {REGENERATE}\n(or run the test with AMB_UPDATE_DOCS=1)"
        );
        let tokens = estimate_tokens(&fresh);
        assert!(
            tokens <= CORE_BUDGET,
            "the core card is about {tokens} tokens, over its budget of {CORE_BUDGET}"
        );
    }

    #[test]
    fn the_card_names_real_settings() {
        let c = Catalog::builtin();
        for kind in ["Hue", "Saturation", "Luminance"] {
            for band in [
                "Red", "Orange", "Yellow", "Green", "Aqua", "Blue", "Purple", "Magenta",
            ] {
                assert!(c.lookup(&format!("{kind}Adjustment{band}")).is_some());
            }
        }
        for zone in ["Shadow", "Midtone", "Highlight", "Global"] {
            for (part, lo, hi) in [
                ("Hue", 0.0, 359.0),
                ("Sat", 0.0, 100.0),
                ("Lum", -100.0, 100.0),
            ] {
                let t = c.lookup(&format!("ColorGrade{zone}{part}")).unwrap();
                assert_eq!((t.min, t.max), (Some(lo), Some(hi)), "{}", t.name);
            }
        }
        assert!(c.lookup("ConvertToGrayscale").is_some());
    }

    #[test]
    fn group_pages_list_every_setting() {
        let tone = group("tone").unwrap();
        assert!(tone.contains("Exposure2012 -5..5 =0"), "{tone}");
        for g in Catalog::builtin().groups() {
            let page = group(&g.name).unwrap();
            assert_eq!(page.lines().count(), g.tags.len() + 1, "{}", g.name);
        }
        assert!(group("nope").is_none());
    }
}
