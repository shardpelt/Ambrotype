// SPDX-License-Identifier: GPL-3.0-or-later
//! Tests for the hub.

use super::*;

fn hub_url() -> String {
    format!("file://{}/data/hub/index.json", env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn the_built_in_hub_copy_installs_whole() {
    let hub = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("data/hub");
    assert!(check_hub(&hub));
}

#[test]
fn a_hub_item_that_wont_install_fails_the_check() {
    let hub = temp_dir("bad-hub");
    std::fs::create_dir_all(hub.join("looks/themes")).unwrap();
    let theme = br##"{"format": "ambrotype-theme/1", "name": "Bad", "tokens": {"bg": "url(x)"}}"##;
    std::fs::write(hub.join("looks/themes/bad.json"), theme).unwrap();
    let index = serde_json::json!({"format": "ambrotype-hub/1", "items": [{
        "id": "theme-bad", "kind": "theme", "name": "Bad", "path": "looks/themes/bad.json",
        "sha256": sha256_hex(theme),
    }]});
    std::fs::write(hub.join("index.json"), index.to_string()).unwrap();
    assert!(!check_hub(&hub));
    let _ = std::fs::remove_dir_all(&hub);
}

fn temp_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("amb-hub-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The hub items the app ships (data/hub/) are clean and current, and its
/// own looks show as built in.
#[test]
fn the_bundled_hub_items_are_valid() {
    let entries = browse(&hub_url(), Mine::default()).unwrap();
    assert!(entries.len() >= 9);
    for e in &entries {
        assert!(e.error.is_none(), "{}: {:?}", e.item.id, e.error);
        assert!(e.data.is_some());
        if e.item.kind == "preset" {
            assert!(e.installed.is_none());
        } else {
            let stem = e.item.id.split_once('-').unwrap().1;
            assert_eq!(e.installed.as_deref(), Some(&*format!("builtin:{stem}")));
            assert!(!e.update);
        }
    }
}

#[test]
fn install_update_and_checksums() {
    let dir = temp_dir("install");
    let url = hub_url();
    let (kind, id) = install(&dir, &url, "theme-light").unwrap();
    assert_eq!((kind, id.as_str()), (Kind::Theme, "user:light"));
    let entries = browse(
        &url,
        Mine {
            looks: Some(&dir),
            ..Default::default()
        },
    )
    .unwrap();
    let light = entries.iter().find(|e| e.item.id == "theme-light").unwrap();
    assert_eq!(light.installed.as_deref(), Some("user:light"));
    assert!(!light.update);
    // Installing again updates the same look rather than making a copy.
    assert_eq!(install(&dir, &url, "theme-light").unwrap().1, "user:light");
    assert!(install(&dir, &url, "theme-nope").is_err());

    // A file changed after it was listed is refused.
    let copy = temp_dir("tampered");
    std::fs::create_dir_all(copy.join("looks/themes")).unwrap();
    let index = std::fs::read_to_string(format!(
        "{}/data/hub/index.json",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap();
    std::fs::write(copy.join("index.json"), index).unwrap();
    std::fs::write(copy.join("looks/themes/dark.json"), "{}").unwrap();
    let tampered = format!("file://{}/index.json", copy.display());
    let entries = browse(&tampered, Mine::default()).unwrap();
    let dark = entries.iter().find(|e| e.item.id == "theme-dark").unwrap();
    assert!(dark.error.as_deref().unwrap().contains("checksum"));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&copy);
}

#[test]
fn plugins_install_from_the_hub_checked() {
    let hub = temp_dir("plugin-hub");
    let src = format!("{}/../../plugins/random-look", env!("CARGO_MANIFEST_DIR"));
    let wasm = std::fs::read(format!("{src}/plugin.wasm")).unwrap();
    let text = std::fs::read_to_string(format!("{src}/plugin.json")).unwrap();
    let mut manifest: Value = serde_json::from_str(&text).unwrap();
    manifest["id"] = "dice".into();
    let manifest = serde_json::to_vec(&manifest).unwrap();
    std::fs::create_dir_all(hub.join("plugins/dice")).unwrap();
    std::fs::write(hub.join("plugins/dice/plugin.json"), &manifest).unwrap();
    std::fs::write(hub.join("plugins/dice/plugin.wasm"), &wasm).unwrap();
    let index = serde_json::json!({ "format": INDEX_FORMAT, "items": [{
        "id": "plugin-dice", "kind": "plugin", "name": "Dice",
        "path": "plugins/dice/plugin.json", "sha256": sha256_hex(&manifest),
        "files": { "plugin.wasm": sha256_hex(&wasm) },
    }]});
    std::fs::write(hub.join("index.json"), index.to_string()).unwrap();
    let url = format!("file://{}/index.json", hub.display());
    let mine = temp_dir("plugin-mine");
    let entries = browse(
        &url,
        Mine {
            plugins: Some(&mine),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(entries[0].error.is_none(), "{:?}", entries[0].error);
    assert_eq!(entries[0].data.as_ref().unwrap()["id"], "dice");
    assert!(entries[0].installed.is_none());
    assert_eq!(install_plugin(&mine, &url, "plugin-dice").unwrap(), "dice");
    assert!(mine.join("dice/plugin.wasm").is_file());
    let entries = browse(
        &url,
        Mine {
            plugins: Some(&mine),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(entries[0].installed.as_deref(), Some("dice"));
    assert!(!entries[0].update);
    // A module changed after it was listed is refused.
    std::fs::write(hub.join("plugins/dice/plugin.wasm"), b"\0asm").unwrap();
    let err = install_plugin(&mine, &url, "plugin-dice").unwrap_err();
    assert!(err.contains("checksum"), "{err}");
    let _ = std::fs::remove_dir_all(&hub);
    let _ = std::fs::remove_dir_all(&mine);
}

#[test]
fn interfaces_install_from_the_hub_checked() {
    let hub = temp_dir("ui-hub");
    let files: [(&str, &[u8]); 3] = [
        ("index.html", b"<script src=\"js/app.js\"></script>"),
        ("js/app.js", b"// hi"),
        ("look.css", b"body{}"),
    ];
    let manifest = br#"{"name": "Tiny", "author": "Me"}"#;
    let base = hub.join("interfaces/tiny");
    std::fs::create_dir_all(base.join("js")).unwrap();
    std::fs::write(base.join("interface.json"), manifest).unwrap();
    for (name, bytes) in files {
        std::fs::write(base.join(name), bytes).unwrap();
    }
    let item = |files: Value| {
        serde_json::json!({ "format": INDEX_FORMAT, "items": [{
            "id": "interface-tiny", "kind": "interface", "name": "Tiny",
            "path": "interfaces/tiny/interface.json", "sha256": sha256_hex(manifest),
            "files": files,
        }]})
        .to_string()
    };
    let listed: serde_json::Map<String, Value> = files
        .iter()
        .map(|(n, b)| (n.to_string(), sha256_hex(b).into()))
        .collect();
    std::fs::write(hub.join("index.json"), item(listed.clone().into())).unwrap();
    let url = format!("file://{}/index.json", hub.display());
    let mine = temp_dir("ui-mine");
    let browse_mine = || {
        browse(
            &url,
            Mine {
                interfaces: Some(&mine),
                ..Default::default()
            },
        )
        .unwrap()
    };
    let entries = browse_mine();
    assert!(entries[0].error.is_none(), "{:?}", entries[0].error);
    assert_eq!(entries[0].data.as_ref().unwrap()["name"], "Tiny");
    assert_eq!(entries[0].data.as_ref().unwrap()["files"][1], "js/app.js");
    let dir = install_interface(&mine, &url, "interface-tiny").unwrap();
    assert!(crate::interface::is_interface(&dir));
    assert!(dir.join("js/app.js").is_file());
    assert_eq!(browse_mine()[0].installed.as_deref(), Some("tiny"));
    // Changed after it was listed: refused, and the installed copy stays.
    std::fs::write(base.join("js/app.js"), b"// changed").unwrap();
    let err = install_interface(&mine, &url, "interface-tiny").unwrap_err();
    assert!(err.contains("checksum"), "{err}");
    assert!(dir.join("js/app.js").is_file());
    // Nothing outside its folder, and only what pages are made of.
    for bad in [
        "../evil.html",
        "js/../../x.js",
        ".hidden.js",
        "run.sh",
        "a\\b.js",
    ] {
        let mut files = listed.clone();
        files.insert(bad.into(), sha256_hex(b"x").into());
        std::fs::write(hub.join("index.json"), item(files.into())).unwrap();
        assert!(browse_mine()[0].error.is_some(), "{bad}");
        assert!(
            install_interface(&mine, &url, "interface-tiny").is_err(),
            "{bad}"
        );
    }
    let _ = std::fs::remove_dir_all(&hub);
    let _ = std::fs::remove_dir_all(&mine);
}

#[test]
fn preset_library_falls_back_to_the_built_in_presets() {
    let cache = temp_dir("plib-cache");
    let mine = temp_dir("plib-mine");
    let lib = preset_library("file:///nowhere/index.json", &cache, Some(&mine));
    assert_eq!(lib.len(), BUNDLED_PRESETS.len());
    assert!(lib.len() >= 20);
    assert!(
        lib.iter()
            .all(|p| !p.group.is_empty() && !p.description.is_empty() && !p.installed)
    );
    let bw = lib.iter().find(|p| p.id == "preset-bw-classic").unwrap();
    install_library_preset(&mine, &cache, &bw.id, Path::new(&bw.path)).unwrap();
    let again = preset_library("file:///nowhere/index.json", &cache, Some(&mine));
    assert!(again.iter().find(|p| p.id == bw.id).unwrap().installed);
    // Only the library's own copies can be installed.
    let elsewhere = mine.join("x.xmp");
    std::fs::copy(&bw.path, &elsewhere).unwrap();
    assert!(install_library_preset(&mine, &cache, "preset-x", &elsewhere).is_err());
    assert!(install_library_preset(&mine, &cache, "../x", Path::new(&bw.path)).is_err());
}

#[test]
fn presets_install_from_the_hub_checked() {
    let hub = temp_dir("preset-hub");
    let fixtures = format!(
        "{}/../amb-presets/tests/fixtures",
        env!("CARGO_MANIFEST_DIR")
    );
    let preset = std::fs::read(format!("{fixtures}/punchy-teal-orange.xmp")).unwrap();
    let sidecar = std::fs::read(format!("{fixtures}/sidecar.xmp")).unwrap();
    std::fs::create_dir_all(hub.join("presets")).unwrap();
    std::fs::write(hub.join("presets/punchy.xmp"), &preset).unwrap();
    std::fs::write(hub.join("presets/sidecar.xmp"), &sidecar).unwrap();
    let item = |id: &str, file: &str, bytes: &[u8]| {
        serde_json::json!({ "id": id, "kind": "preset", "name": id,
            "path": format!("presets/{file}"), "sha256": sha256_hex(bytes) })
    };
    let index = serde_json::json!({ "format": INDEX_FORMAT, "items": [
        item("preset-punchy", "punchy.xmp", &preset),
        item("preset-sidecar", "sidecar.xmp", &sidecar),
    ]});
    std::fs::write(hub.join("index.json"), index.to_string()).unwrap();
    let url = format!("file://{}/index.json", hub.display());
    let mine = temp_dir("preset-mine");
    let presets = Mine {
        presets: Some(&mine),
        ..Default::default()
    };
    let entries = browse(&url, presets).unwrap();
    let punchy = entries
        .iter()
        .find(|e| e.item.id == "preset-punchy")
        .unwrap();
    assert!(punchy.error.is_none(), "{:?}", punchy.error);
    assert!(punchy.data.as_ref().unwrap()["settings"].as_u64().unwrap() > 3);
    let side = entries
        .iter()
        .find(|e| e.item.id == "preset-sidecar")
        .unwrap();
    assert!(
        side.error
            .as_deref()
            .unwrap()
            .contains("not a develop preset")
    );
    let path = install_preset(&mine, &url, "preset-punchy").unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), preset);
    let entries = browse(&url, presets).unwrap();
    let punchy = entries
        .iter()
        .find(|e| e.item.id == "preset-punchy")
        .unwrap();
    assert!(punchy.installed.is_some() && !punchy.update);
    assert!(install_preset(&mine, &url, "preset-sidecar").is_err());
    let _ = std::fs::remove_dir_all(&hub);
    let _ = std::fs::remove_dir_all(&mine);
}

#[test]
fn collages_size_profiles_and_borders_install_from_the_hub() {
    let hub = temp_dir("data-hub");
    std::fs::create_dir_all(hub.join("collages")).unwrap();
    std::fs::create_dir_all(hub.join("sizes")).unwrap();
    std::fs::create_dir_all(hub.join("borders")).unwrap();
    let collage = serde_json::json!({ "format": amb_export::collage::FORMAT, "name": "Two up",
        "width": 1200, "height": 800, "cells": [[0, 0, 0.5, 1], [0.5, 0, 0.5, 1]] })
    .to_string();
    let sizes = serde_json::json!({ "format": SIZES_FORMAT, "name": "Etsy",
        "sizes": [{ "name": "Listing", "width": 2000, "height": 1600 },
                  { "name": "Banner", "width": 3360, "height": 840 }] })
    .to_string();
    std::fs::write(hub.join("collages/two-up.json"), &collage).unwrap();
    std::fs::write(hub.join("sizes/etsy.json"), &sizes).unwrap();
    let border = serde_json::json!({ "format": amb_export::border::FORMAT, "name": "Keyline",
        "width": 5, "color": "#ffffff", "keyline": { "width": 0.3, "color": "#000000" } })
    .to_string();
    std::fs::write(hub.join("borders/keyline.json"), &border).unwrap();
    let item = |id: &str, kind: &str, path: &str, text: &str| {
        serde_json::json!({ "id": id, "kind": kind, "name": id, "path": path,
            "sha256": sha256_hex(text.as_bytes()) })
    };
    let index = serde_json::json!({ "format": INDEX_FORMAT, "items": [
        item("collage-two-up", "collage", "collages/two-up.json", &collage),
        item("sizes-etsy", "sizes", "sizes/etsy.json", &sizes),
        item("border-keyline", "border", "borders/keyline.json", &border),
    ]});
    std::fs::write(hub.join("index.json"), index.to_string()).unwrap();
    let url = format!("file://{}/index.json", hub.display());
    let entries = browse(&url, Mine::default()).unwrap();
    assert!(entries.iter().all(|e| e.error.is_none()), "{entries:?}");

    let mut templates = Vec::new();
    let id = install_collage(&mut templates, &url, "collage-two-up").unwrap();
    // Installing again updates the same template.
    assert_eq!(
        install_collage(&mut templates, &url, "collage-two-up").unwrap(),
        id
    );
    assert_eq!(templates.len(), 1);
    let mut presets = Vec::new();
    assert_eq!(install_sizes(&mut presets, &url, "sizes-etsy").unwrap(), 2);
    assert_eq!(install_sizes(&mut presets, &url, "sizes-etsy").unwrap(), 2);
    assert_eq!(presets.len(), 2);
    assert!(presets.iter().all(|p| p.group == "Etsy"));
    let mut borders = Vec::new();
    let id = install_border(&mut borders, &url, "border-keyline").unwrap();
    assert_eq!(
        install_border(&mut borders, &url, "border-keyline").unwrap(),
        id
    );
    assert_eq!(borders.len(), 1);
    assert_eq!(borders[0].border.keyline.as_ref().unwrap().width, 0.3);
    let mine = Mine {
        collages: &templates,
        sizes: &presets,
        borders: &borders,
        ..Default::default()
    };
    let entries = browse(&url, mine).unwrap();
    assert!(entries.iter().all(|e| e.installed.is_some() && !e.update));
    let _ = std::fs::remove_dir_all(&hub);
}

#[test]
fn shared_links() {
    let raw = "https://raw.githubusercontent.com";
    assert_eq!(
        link_candidates("https://github.com/someone/warm-theme.git/").unwrap(),
        [
            format!("{raw}/someone/warm-theme/HEAD/theme.json"),
            format!("{raw}/someone/warm-theme/HEAD/layout.json"),
        ]
    );
    assert_eq!(
        link_candidates("https://github.com/a/b/blob/main/looks/x.json").unwrap(),
        [format!("{raw}/a/b/main/looks/x.json")]
    );
    assert_eq!(
        link_candidates(" https://example.com/t.json ").unwrap(),
        ["https://example.com/t.json"]
    );
    assert!(link_candidates("http://example.com/t.json").is_err());
    assert!(link_candidates("https://github.com/someone").is_err());

    let dir = temp_dir("link");
    let file = dir.join("t.json");
    std::fs::write(
        &file,
        r##"{"format":"ambrotype-theme/1","name":"Mint","tokens":{"bg":"#102020","text":"#e0f0f0","accent":"#40d0a0"}}"##,
    )
    .unwrap();
    // file:// stands in for a web address here.
    let link = format!("file://{}", file.display());
    let looks_dir = dir.join("looks");
    assert!(install_link(&looks_dir, &link).is_err(), "only https links");
    let mut data: Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    data["source"] = serde_json::json!({ "link": link });
    let id = looks::save(&looks_dir, Kind::Theme, None, data).unwrap();
    let theme = looks::all(Some(&looks_dir), Kind::Theme)
        .into_iter()
        .find(|l| l.id == id)
        .unwrap();
    assert!(theme.usable);
    assert_eq!(theme.data["tokens"]["muted"].as_str().unwrap().len(), 7);
}

#[test]
fn addresses_and_paths() {
    assert!(fetch("http://example.com/index.json", 10).is_err());
    assert!(fetch("ftp://example.com/index.json", 10).is_err());
    let base = "https://raw.githubusercontent.com/o/r/main/index.json";
    assert_eq!(
        item_url(base, "looks/themes/a.json").unwrap(),
        "https://raw.githubusercontent.com/o/r/main/looks/themes/a.json"
    );
    for bad in [
        "../x.json",
        "/etc/passwd",
        "a/../../b",
        "https://evil/x",
        "a\\b",
        "",
    ] {
        assert!(item_url(base, bad).is_err(), "{bad}");
    }
    assert_eq!(
        github_repo("https://raw.githubusercontent.com/o/r/main/hub/index.json"),
        Some(("o/r".into(), "main".into(), "hub/".into()))
    );
    assert_eq!(github_repo("https://example.com/index.json"), None);
}

#[test]
fn submit_link() {
    let data = serde_json::json!({
        "format": looks::THEME_FORMAT, "name": "My Warm Look!", "tokens": {},
        "source": { "hub": "x", "id": "y", "sha256": "z" }
    });
    let url = submit_url(DEFAULT_INDEX, Kind::Theme, &data).unwrap();
    assert!(url.starts_with("https://github.com/Ambrotypehq/ambrotype-hub/new/main?filename=looks%2Fthemes%2Fmy-warm-look.json&value="));
    assert!(
        !url.contains("source"),
        "the install record isn't submitted"
    );
    assert!(submit_url("file:///tmp/index.json", Kind::Theme, &data).is_err());
}
