// SPDX-License-Identifier: GPL-3.0-or-later
//! `amb dust` end to end: on the synthetic dusty slide it finds the specks
//! and the hair but not the wire, adds them as heal spots, and a second run
//! adds nothing.

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
fn finds_dust_and_heals_it_once() {
    let dir = std::env::temp_dir().join(format!("amb-cli-dust-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    amb(&dir, &["testimage", "dust", "slide.dng"]);

    let found: serde_json::Value =
        serde_json::from_str(&amb(&dir, &["dust", "slide.dng", "--json"])).unwrap();
    let specks = found["specks"].as_u64().unwrap();
    assert_eq!(
        specks as usize,
        amb_testkit::patterns::DUST_SPECKS.len(),
        "{found}"
    );
    assert_eq!(found["hairs"], 1, "{found}");
    assert!(found["written"].is_null(), "only shown without --apply");
    // No spot lies on the wire.
    for spot in found["spots"].as_array().unwrap() {
        let y = spot["at"][1].as_f64().unwrap() * 800.0;
        assert!((y - 120.5).abs() > 4.0, "a spot on the wire: {spot}");
    }

    let text = amb(&dir, &["dust", "slide.dng", "--apply"]);
    assert!(text.contains("saved"), "{text}");
    let sidecar = std::fs::read_to_string(dir.join("slide.xmp")).unwrap();
    assert_eq!(
        sidecar.matches("crs:SpotType=\"heal\"").count(),
        9,
        "{sidecar}"
    );

    let again: serde_json::Value =
        serde_json::from_str(&amb(&dir, &["dust", "slide.dng", "--json"])).unwrap();
    assert_eq!(
        (again["specks"].as_u64(), again["hairs"].as_u64()),
        (Some(0), Some(0))
    );
    assert_eq!(again["covered"], 9);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_line_drawn_near_the_wire_moves_onto_it() {
    let dir = std::env::temp_dir().join(format!("amb-cli-dust-line-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    amb(&dir, &["testimage", "dust", "slide.dng"]);
    // The wire is on rows 120-121 of 800; drawn 4 rows off.
    let found: serde_json::Value = serde_json::from_str(&amb(
        &dir,
        &[
            "dust",
            "slide.dng",
            "--line",
            "0.1,0.155,0.9,0.155",
            "--json",
        ],
    ))
    .unwrap();
    let spots = found["spots"].as_array().unwrap();
    assert!(spots.len() >= 2, "{found}");
    for spot in spots {
        for p in spot["points"].as_array().unwrap() {
            let y = p[1].as_f64().unwrap() * 800.0;
            assert!((y - 121.0).abs() <= 1.0, "a point off the wire: {spot}");
        }
    }
    std::fs::remove_dir_all(&dir).unwrap();
}
