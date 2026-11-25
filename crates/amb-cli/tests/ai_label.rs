// SPDX-License-Identifier: GPL-3.0-or-later
//! `amb ai-label` end to end: a plain photo is clear; one whose edit holds an
//! AI part is "ai_in_edit"; with AI-free mode on the part is left out and the
//! verdict is clear, and `amb render --preset` says what it left out.

use std::path::Path;
use std::process::Command;

fn amb(dir: &Path, config: &Path, args: &[&str]) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_amb"))
        .args(args)
        .current_dir(dir)
        .env("XDG_CONFIG_HOME", config)
        .env("AMB_MODELS", dir.join("models"))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "amb {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn the_label_names_ai_parts_of_the_edit() {
    let dir = std::env::temp_dir().join(format!("amb-cli-ailabel-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let config = dir.join("config");
    std::fs::create_dir_all(&dir).unwrap();
    let label = |extra: &[&str]| -> serde_json::Value {
        let mut args = vec!["ai-label", "p.dng", "--json"];
        args.extend(extra);
        let v: serde_json::Value = serde_json::from_str(&amb(&dir, &config, &args)).unwrap();
        v[0]["label"].clone()
    };

    amb(&dir, &config, &["testimage", "ramp", "p.dng"]);
    assert_eq!(label(&[])["verdict"], "clear");

    std::fs::write(
        dir.join("cmds.json"),
        r#"{"cmd":"set_param","name":"EnhanceDenoiseVersion","value":"1"}"#,
    )
    .unwrap();
    amb(&dir, &config, &["edit", "p.xmp", "--commands", "cmds.json"]);
    let l = label(&[]);
    assert_eq!(l["verdict"], "ai_in_edit");
    assert_eq!(l["edit_parts"].as_array().unwrap().len(), 1);
    assert!(amb(&dir, &config, &["ai-label", "p.dng"]).contains("AI in the edit"));

    amb(&dir, &config, &["ai-free", "on"]);
    let l = label(&[]);
    assert_eq!(l["verdict"], "clear");
    assert_eq!(l["left_out"].as_array().unwrap().len(), 1);
    assert!(amb(&dir, &config, &["ai-label", "p.dng"]).contains("left out for AI-free mode"));

    // `amb render --preset`: the preset's AI part is left out, and said so.
    amb(&dir, &config, &["testimage", "ramp", "q.dng"]);
    let out = amb(
        &dir,
        &config,
        &["render", "--preset", "p.xmp", "q.dng", "q.jpg"],
    );
    assert!(out.contains("left out for AI-free mode"), "{out}");
    let _ = std::fs::remove_dir_all(&dir);
}
