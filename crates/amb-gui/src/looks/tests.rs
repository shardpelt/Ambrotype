// SPDX-License-Identifier: GPL-3.0-or-later
//! Tests for looks.

use super::check::*;
use super::setups::*;
use super::*;
use serde_json::json;

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("amb-looks-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn messages(p: &[Problem]) -> Vec<String> {
    p.iter().map(|p| p.message.clone()).collect()
}

#[test]
fn builtins_are_clean() {
    for kind in [Kind::Layout, Kind::Theme] {
        for look in builtins(kind) {
            assert!(
                look.problems.is_empty(),
                "{}: {:?}",
                look.id,
                messages(&look.problems)
            );
        }
    }
    // Every built-in theme sets every token, so switching themes changes everything.
    for theme in builtins(Kind::Theme) {
        let tokens = theme.data["tokens"].as_object().unwrap();
        for t in COLOR_TOKENS
            .iter()
            .copied()
            .chain(NUMBER_TOKENS.iter().map(|t| t.0))
            // Glass is a choice, off when left out.
            .filter(|t| *t != "glass")
        {
            assert!(tokens.contains_key(t), "{} misses {t}", theme.id);
        }
    }
}

#[test]
fn classic_shows_every_editor_control() {
    let classic = builtins(Kind::Layout).remove(0);
    assert_eq!(classic.id, DEFAULT_LAYOUT);
    let mut c = Checker::default();
    for side in ["left", "right"] {
        c.column(side, &classic.data[side]);
    }
    let wheel_tags = 12;
    assert_eq!(c.controls.len() + wheel_tags, catalog().kinds.len());
    assert_eq!(c.composites.len(), 2);
    // Quick fixes, the Retouch tools, presets, history and the
    // histogram; Rating & keywords is the library's.
    assert_eq!(c.blocks.len(), 5);
}

#[test]
fn layout_problems_are_specific() {
    let base = |items: Value| json!({ "format": LAYOUT_FORMAT, "name": "T", "right": { "width": 300, "items": items } });
    // Unknown names are warnings: the layout still works.
    let p = check_layout(&base(json!([
        "histogram", "filmstrip",
        { "title": "Mine", "items": [{ "setting": "NoSuchSetting" }, { "widget": "sparkles" }] }
    ])));
    assert!(p.iter().all(|p| !p.error), "{:?}", messages(&p));
    assert_eq!(p.len(), 3);
    // A widget of the wrong kind, a duplicate and a missing title are errors.
    let p = check_layout(&base(json!([
        { "title": "A", "items": [{ "setting": "Exposure2012", "widget": "toggle" }] },
        { "panel": "basic" },
        { "items": [] }
    ])));
    let m = messages(&p);
    assert!(
        m.iter()
            .any(|m| m.contains("can't use the \"toggle\" widget")),
        "{m:?}"
    );
    assert!(
        m.iter()
            .any(|m| m.contains("\"Exposure2012\" appears more than once")),
        "{m:?}"
    );
    assert!(m.iter().any(|m| m.contains("needs \"panel\"")), "{m:?}");
    // Two curves (the tone curve panel brings one) are an error.
    let p = check_layout(&base(json!([
        { "panel": "tone_curve" },
        { "title": "B", "items": [{ "widget": "point-curve" }] }
    ])));
    assert!(
        messages(&p)
            .iter()
            .any(|m| m.contains("point-curve widget appears more than once"))
    );
    // Hiding a setting frees it for another panel; includes count too.
    let p = check_layout(&base(json!([
        { "panel": "basic", "hide": ["Exposure2012"] },
        { "title": "C", "columns": 2, "items": [
            { "setting": "Exposure2012", "widget": "knob" },
            { "include": "detail/Sharpening" }
        ] },
        { "panel": "detail", "hide": [] }
    ])));
    let m = messages(&p);
    assert!(
        m.iter()
            .filter(|m| m.contains("appears more than once"))
            .count()
            >= 1,
        "{m:?}"
    );
    assert!(!m.iter().any(|m| m.contains("Exposure2012")), "{m:?}");
}

#[test]
fn theme_problems() {
    let p = check_theme(&json!({
        "format": THEME_FORMAT, "name": "x", "scheme": "dim",
        "tokens": { "bg": "red", "radius": 40, "sparkle": "#fff", "accent": "#ffcc00" }
    }));
    let m = messages(&p);
    assert_eq!(p.iter().filter(|p| p.error).count(), 3, "{m:?}");
    assert!(m.iter().any(|m| m.contains("unknown token \"sparkle\"")));
    assert!(is_hex_colour("#abc") && is_hex_colour("#aabbccdd") && !is_hex_colour("#ggg"));
}

#[test]
fn save_clone_import_delete() {
    let dir = temp_dir("store");
    let mut data = builtins(Kind::Layout)[0].data.clone();
    data["name"] = json!("My Look!");
    data["based_on"] = json!(DEFAULT_LAYOUT);
    let id = save(&dir, Kind::Layout, None, data.clone()).unwrap();
    assert_eq!(id, "user:my-look");
    // Saving over an existing id keeps the file; a new one gets a fresh name.
    data["density"] = json!("compact");
    assert_eq!(
        save(&dir, Kind::Layout, Some(&id), data.clone()).unwrap(),
        id
    );
    assert_eq!(
        save(&dir, Kind::Layout, None, data.clone()).unwrap(),
        "user:my-look-2"
    );
    let looks = list(Some(&dir), Some("user:my-look"), Some("user:nope"));
    assert_eq!(looks.active_layout, "user:my-look");
    assert_eq!(looks.active_theme, DEFAULT_THEME);
    let mine = looks.layouts.iter().find(|l| l.id == id).unwrap();
    assert!(!mine.builtin && mine.usable);
    assert_eq!(mine.based_on.as_deref(), Some(DEFAULT_LAYOUT));
    assert_eq!(mine.data["density"], "compact");

    // Errors aren't saved; nor is a theme saved as a layout.
    let mut bad = data.clone();
    bad["toolbar"] = json!("open");
    assert!(save(&dir, Kind::Layout, None, bad).is_err());
    assert!(save(&dir, Kind::Theme, None, data.clone()).is_err());

    // Export and import round trip.
    let file = dir.join("shared.json");
    std::fs::write(&file, export_text(Some(&dir), Kind::Layout, &id).unwrap()).unwrap();
    assert_eq!(
        import(&dir, &file).unwrap(),
        (Kind::Layout, "user:my-look-3".into())
    );

    // An unreadable file is listed, not fatal.
    std::fs::write(dir.join("layouts/broken.json"), "{ nope").unwrap();
    let broken = all(Some(&dir), Kind::Layout)
        .into_iter()
        .find(|l| l.id == "user:broken")
        .unwrap();
    assert!(!broken.usable);

    assert!(delete(&dir, Kind::Layout, DEFAULT_LAYOUT).is_err());
    assert!(delete(&dir, Kind::Layout, "user:../x").is_err());
    delete(&dir, Kind::Layout, &id).unwrap();
    assert!(!all(Some(&dir), Kind::Layout).iter().any(|l| l.id == id));
    let _ = std::fs::remove_dir_all(&dir);
}

/// docs/ai/ui-layouts.md is what AI assistants read: it must name everything a layout can use.
#[test]
fn format_reference_is_complete() {
    let doc = include_str!("../../../../docs/ai/ui-layouts.md");
    let mut names: Vec<String> = TOOLBAR
        .iter()
        .chain(&BLOCKS)
        .chain(&COMPOSITES)
        .chain(&DENSITIES)
        .chain(&COLOR_TOKENS)
        .copied()
        .chain(all_widgets())
        .chain(NUMBER_TOKENS.iter().map(|t| t.0))
        .map(|n| format!("`{n}`"))
        .collect();
    for p in &catalog().panels {
        names.push(format!("`{}`", p.id));
        names.extend(p.sections.iter().map(|s| format!("`{}`", s.label)));
    }
    let missing: Vec<_> = names.iter().filter(|n| !doc.contains(n.as_str())).collect();
    assert!(
        missing.is_empty(),
        "docs/ai/ui-layouts.md doesn't mention {missing:?}"
    );
}

#[test]
fn tabs_in_a_column() {
    let layout = |items: Value| json!({ "format": LAYOUT_FORMAT, "name": "T", "right": { "width": 300, "items": items } });
    let ok = check_layout(&layout(
        json!(["histogram", { "tab": "Tone" }, { "panel": "basic" }, { "tab": "Detail" }, { "panel": "detail" }]),
    ));
    assert!(ok.iter().all(|p| !p.error), "{ok:?}");
    let twice = check_layout(&layout(json!([{ "tab": "A" }, { "tab": "A" }])));
    assert!(
        twice.iter().any(|p| p.error && p.message.contains("twice")),
        "{twice:?}"
    );
    let empty = check_layout(&layout(json!([{ "tab": " " }])));
    assert!(empty.iter().any(|p| p.error), "{empty:?}");
}

#[test]
fn setups_bring_back_the_exact_looks() {
    let dir = std::env::temp_dir().join(format!("amb-setups-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    // A layout of the user's, kept in a setup with a built-in theme.
    let mut data = builtins(Kind::Layout)[0].data.clone();
    data["name"] = json!("Mine");
    let mine = save(&dir, Kind::Layout, None, data.clone()).unwrap();
    save_setup(&dir, "Work", &mine, "builtin:light").unwrap();
    assert_eq!(
        restore_setup(&dir, "Work").unwrap(),
        (mine.clone(), "builtin:light".to_string())
    );
    // The layout is changed afterwards: the setup brings back the copy.
    let mut changed = data.clone();
    changed["density"] = json!("compact");
    save(&dir, Kind::Layout, Some(&mine), changed).unwrap();
    let (layout, _) = restore_setup(&dir, "Work").unwrap();
    assert_ne!(layout, mine);
    let back = all(Some(&dir), Kind::Layout)
        .into_iter()
        .find(|l| l.id == layout)
        .unwrap();
    assert_eq!(back.data, data);
    // Restoring again reuses that copy rather than making another.
    assert_eq!(restore_setup(&dir, "Work").unwrap().0, layout);
    assert_eq!(setups(&dir).len(), 1);
    delete_setup(&dir, "Work").unwrap();
    assert!(setups(&dir).is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn slugs() {
    assert_eq!(slug("  Warm & Cosy  v2 "), "warm-cosy-v2");
    assert_eq!(slug("!!!"), "look");
}

#[test]
fn three_colours_make_a_theme() {
    // Derived from Dark's own three colours, the rest come out close to Dark.
    let dark: Value = serde_json::from_str(BUILTIN_THEMES[0].1).unwrap();
    let mut three = serde_json::json!({ "format": THEME_FORMAT, "name": "t", "tokens": {
        "bg": dark["tokens"]["bg"], "text": dark["tokens"]["text"], "accent": dark["tokens"]["accent"] } });
    complete_theme(&mut three);
    assert_eq!(three["scheme"], "dark");
    for token in COLOR_TOKENS {
        let a = parse_hex(three["tokens"][token].as_str().unwrap()).unwrap();
        let b = parse_hex(dark["tokens"][token].as_str().unwrap()).unwrap();
        let off = (0..3).map(|i| (a[i] - b[i]).abs()).fold(0.0, f64::max);
        assert!(off <= 8.0, "{token}: {a:?} vs {b:?}");
    }
    assert!(check_theme(&three).iter().all(|p| !p.error));
    // A light background makes a light theme; tokens given are kept.
    let mut light = serde_json::json!({ "format": THEME_FORMAT, "name": "l",
        "tokens": { "bg": "#fafafa", "text": "#222", "accent": "#0a6", "danger": "#f00" } });
    complete_theme(&mut light);
    assert_eq!(light["scheme"], "light");
    assert_eq!(light["tokens"]["danger"], "#f00");
    assert_eq!(light["tokens"]["text"], "#222");
    assert_eq!(
        light["tokens"].as_object().unwrap().len(),
        COLOR_TOKENS.len() + 1
    );
}
