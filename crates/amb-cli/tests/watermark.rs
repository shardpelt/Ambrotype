// SPDX-License-Identifier: GPL-3.0-or-later
//! `amb watermark` end to end: save a mark by name, list it, render with it
//! by name, delete it. The editor's config folder is the test's own.

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

fn mean(path: &PathBuf) -> f64 {
    let img = image::open(path).expect("a picture").to_luma8();
    img.pixels().map(|p| f64::from(p.0[0])).sum::<f64>() / img.pixels().len() as f64
}

#[test]
fn a_saved_watermark_is_listed_rendered_by_name_and_deleted() {
    let dir = std::env::temp_dir().join(format!("amb-cli-watermark-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    amb(&dir, &["testimage", "ramp", "photo.dng"]);
    amb(
        &dir,
        &[
            "watermark",
            "save",
            "Me",
            "© Me",
            "--watermark-at",
            "center",
        ],
    );

    let listed = amb(&dir, &["watermark", "list", "--json"]);
    let listed: serde_json::Value = serde_json::from_str(&listed).unwrap();
    assert_eq!(listed[0]["name"], "Me");
    assert_eq!(listed[0]["mark"]["anchor"], "center");
    assert!(amb(&dir, &["watermark", "show", "me"]).contains("ambrotype-watermark/1"));

    amb(&dir, &["render", "photo.dng", "plain.jpg"]);
    amb(
        &dir,
        &["render", "photo.dng", "marked.jpg", "--watermark", "me"],
    );
    let (plain, marked) = (mean(&dir.join("plain.jpg")), mean(&dir.join("marked.jpg")));
    assert!(
        (plain - marked).abs() > 0.01,
        "the saved mark changes the photo ({plain} vs {marked})"
    );

    amb(&dir, &["watermark", "delete", "Me"]);
    let after = amb(&dir, &["watermark", "list", "--json"]);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&after).unwrap(),
        serde_json::json!([])
    );
    let _ = std::fs::remove_dir_all(&dir);
}
