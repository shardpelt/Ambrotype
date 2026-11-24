// SPDX-License-Identifier: GPL-3.0-or-later
//! `amb ai-free` end to end: the setting is saved in the config folder, a
//! damaged file means on, and with it on the commands that would run a model
//! refuse with the mode's message.

use std::path::Path;
use std::process::{Command, Output};

fn amb(dir: &Path, config: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_amb"))
        .args(args)
        .current_dir(dir)
        .env("XDG_CONFIG_HOME", config)
        .env("AMB_MODELS", dir.join("models"))
        .output()
        .unwrap()
}

fn ok(dir: &Path, config: &Path, args: &[&str]) -> String {
    let out = amb(dir, config, args);
    assert!(
        out.status.success(),
        "amb {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn the_switch_is_saved_and_refuses_models() {
    let dir = std::env::temp_dir().join(format!("amb-cli-aifree-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let config = dir.join("config");
    std::fs::create_dir_all(&dir).unwrap();
    let file = config.join("org.ambrotype.editor/ai-free.json");
    let json = |args: &[&str]| -> serde_json::Value {
        serde_json::from_str(&ok(&dir, &config, args)).unwrap()
    };

    assert_eq!(json(&["ai-free", "--json"])["ai_free"], false);
    assert!(ok(&dir, &config, &["ai-free"]).contains("off"));
    assert_eq!(json(&["ai-free", "on", "--json"])["ai_free"], true);
    assert!(file.is_file(), "the setting is saved");
    assert_eq!(json(&["ai-free", "--json"])["ai_free"], true);

    // With it on, a model run is refused with the mode's message.
    ok(&dir, &config, &["testimage", "ramp", "p.dng"]);
    let out = amb(&dir, &config, &["denoise", "p.dng"]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("AI-free mode is on"), "{err}");
    let out = amb(&dir, &config, &["models", "install", "u2netp"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("AI-free mode is on"));
    std::fs::write(
        dir.join("cmds.json"),
        r#"{"cmd":"add_mask","kind":"subject"}"#,
    )
    .unwrap();
    let out = amb(
        &dir,
        &config,
        &["edit", "p.xmp", "--commands", "cmds.json", "--dry-run"],
    );
    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("AI-free mode is on"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    // A damaged file is on: trust beats convenience.
    std::fs::write(&file, "{ broken").unwrap();
    assert_eq!(json(&["ai-free", "--json"])["ai_free"], true);
    assert_eq!(json(&["ai-free", "off", "--json"])["ai_free"], false);
    assert_eq!(json(&["ai-free", "--json"])["ai_free"], false);
    let _ = std::fs::remove_dir_all(&dir);
}
