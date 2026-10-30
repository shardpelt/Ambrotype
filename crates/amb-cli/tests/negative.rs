// SPDX-License-Identifier: GPL-3.0-or-later
//! `amb negative sync` end to end: a roll of synthetic negatives gets one
//! conversion, written to each sidecar with Auto off.

use std::process::Command;

fn amb(dir: &std::path::Path, args: &[&str]) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_amb"))
        .args(args)
        .current_dir(dir)
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
fn a_roll_gets_one_conversion() {
    let dir = std::env::temp_dir().join(format!("amb-cli-roll-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for n in ["a", "b", "c"] {
        amb(&dir, &["testimage", "negative", &format!("{n}.dng")]);
    }
    let plan: serde_json::Value = serde_json::from_str(&amb(
        &dir,
        &["negative", "sync", "a.dng", "b.dng", "c.dng", "--json"],
    ))
    .unwrap();
    let frames = plan["frames"].as_array().unwrap();
    assert_eq!(frames.len(), 3);
    assert!(
        frames.iter().all(|f| f["scene"] == 0 && f["odd"] == false),
        "{plan}"
    );
    assert_eq!(frames[0]["settings"], frames[2]["settings"]);
    assert!(plan["written"].as_array().unwrap().is_empty());

    let text = amb(
        &dir,
        &[
            "negative",
            "sync",
            "a.dng",
            "b.dng",
            "c.dng",
            "--baseline",
            "b.dng",
            "--apply",
        ],
    );
    assert!(text.contains("wrote 3 sidecar(s)"), "{text}");
    for n in ["a", "b", "c"] {
        let xmp = std::fs::read_to_string(dir.join(format!("{n}.xmp"))).unwrap();
        assert!(xmp.contains("amb:NegativeAuto=\"False\""), "{xmp}");
        assert!(xmp.contains("amb:NegativeDensityHighGreen="), "{xmp}");
    }
    std::fs::remove_dir_all(&dir).unwrap();
}
