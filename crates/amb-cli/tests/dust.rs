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
fn an_area_or_a_stroke_looks_only_where_pointed() {
    let dir = std::env::temp_dir().join(format!("amb-cli-dust-area-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    amb(&dir, &["testimage", "dust", "slide.dng"]);
    // The speck at 250,500 of 1200x800, boxed with room round it; the wire
    // (row 120) and the other specks are outside.
    let area = "0.15,0.5,0.27,0.75";
    let found: serde_json::Value =
        serde_json::from_str(&amb(&dir, &["dust", "slide.dng", "--area", area, "--json"])).unwrap();
    assert_eq!(found["specks"], 1, "{found}");
    let spot = &found["spots"][0];
    let (x, y) = (
        spot["at"][0].as_f64().unwrap() * 1200.0,
        spot["at"][1].as_f64().unwrap() * 800.0,
    );
    assert!((x - 250.0).abs() < 5.0 && (y - 500.0).abs() < 5.0, "{spot}");
    // Along a stroke through the speck at 400,250, 2 % of the long side wide.
    let found: serde_json::Value = serde_json::from_str(&amb(
        &dir,
        &[
            "dust",
            "slide.dng",
            "--along",
            "0.28,0.31;0.38,0.31",
            "--radius",
            "0.02",
            "--json",
        ],
    ))
    .unwrap();
    assert_eq!(found["specks"], 1, "{found}");
    // It adds them as one step.
    let text = amb(&dir, &["dust", "slide.dng", "--area", area, "--apply"]);
    assert!(text.contains("saved"), "{text}");
    let sidecar = std::fs::read_to_string(dir.join("slide.xmp")).unwrap();
    assert_eq!(sidecar.matches("crs:SpotType=\"heal\"").count(), 1);
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
