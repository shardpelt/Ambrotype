// SPDX-License-Identifier: GPL-3.0-or-later
//! `amb style` end to end: save a style from a brighter photo, list it,
//! apply it to a dark one, delete it. The editor's config folder is the
//! test's own.

use std::path::PathBuf;
use std::process::Command;

fn amb(dir: &PathBuf, args: &[&str]) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_amb"))
        .args(args)
        .current_dir(dir)
        .env("XDG_CONFIG_HOME", dir.join("config"))
        .env("HOME", dir)
        .env("APPDATA", dir.join("config"))
        .output()
        .expect("amb runs");
    assert!(
        out.status.success(),
        "amb {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn exposure(sidecar: &PathBuf) -> f64 {
    let text = std::fs::read_to_string(sidecar).unwrap_or_default();
    let at = text.find("Exposure2012=\"").map(|i| i + 14);
    at.and_then(|i| text[i..].split('"').next()?.parse().ok())
        .unwrap_or(0.0)
}

#[test]
fn a_style_made_from_a_bright_photo_brightens_a_dark_one() {
    let dir = std::env::temp_dir().join(format!("amb-cli-style-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    amb(&dir, &["testimage", "colorchecker", "bright.dng"]);
    amb(&dir, &["testimage", "colorchecker", "dark.dng"]);
    amb(&dir, &["do", "bright.dng", "exp=+1.5"]);

    amb(
        &dir,
        &[
            "style",
            "save",
            "Bright test",
            "bright.dng",
            "--focus",
            "tone",
        ],
    );
    let listed = amb(&dir, &["style", "list", "--json"]);
    let listed: serde_json::Value = serde_json::from_str(&listed).unwrap();
    assert_eq!(listed[0]["name"], "Bright test");
    assert_eq!(listed[0]["focus"], "tone");

    amb(&dir, &["style", "apply", "bright test", "dark.dng"]);
    assert!(exposure(&dir.join("dark.xmp")) > 0.5);

    amb(&dir, &["style", "delete", "Bright test"]);
    let after = amb(&dir, &["style", "list", "--json"]);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&after).unwrap(),
        serde_json::json!([])
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn convert_with_a_style_look_records_the_style_step() {
    let dir = std::env::temp_dir().join(format!("amb-cli-convstyle-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    amb(&dir, &["testimage", "colorchecker", "bright.dng"]);
    amb(&dir, &["testimage", "colorchecker", "dark.dng"]);
    amb(&dir, &["do", "bright.dng", "exp=+1.5"]);
    amb(
        &dir,
        &["style", "save", "Bright", "bright.dng", "--focus", "tone"],
    );
    amb(
        &dir,
        &[
            "convert",
            "dark.dng",
            "--to",
            "out",
            "--look",
            "style:Bright",
        ],
    );
    assert!(dir.join("out/dark.jpg").is_file());
    let xmp = std::fs::read_to_string(dir.join("dark.xmp")).unwrap();
    assert!(xmp.contains("Style: Bright"), "{xmp}");

    let bad = Command::new(env!("CARGO_BIN_EXE_amb"))
        .args([
            "convert",
            "dark.dng",
            "--to",
            "out2",
            "--look",
            "style:Nope",
        ])
        .current_dir(&dir)
        .env("XDG_CONFIG_HOME", dir.join("config"))
        .env("HOME", &dir)
        .env("APPDATA", dir.join("config"))
        .output()
        .unwrap();
    assert!(!bad.status.success());
    assert!(String::from_utf8_lossy(&bad.stderr).contains("No style called"));
    let _ = std::fs::remove_dir_all(&dir);
}
