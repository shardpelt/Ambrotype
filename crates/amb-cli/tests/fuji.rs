// SPDX-License-Identifier: GPL-3.0-or-later
//! `amb fuji` end to end: a pasted recipe becomes a preset and goes on a photo.

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
fn a_recipe_becomes_a_preset_and_an_edit() {
    let dir = std::env::temp_dir().join(format!("amb-cli-fuji-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("recipe.txt"),
        "Quiet Street\nFilm Simulation: Classic Negative\nGrain Effect: Weak, Large\nWhite Balance: Daylight, +1 Red & -3 Blue\nClarity: -2\n",
    )
    .unwrap();
    amb(&dir, &["testimage", "ramp", "p.dng"]);
    let plan: serde_json::Value = serde_json::from_str(&amb(
        &dir,
        &[
            "fuji",
            "recipe.txt",
            "--sensor",
            "X-T3",
            "--out",
            "quiet.xmp",
            "--json",
        ],
    ))
    .unwrap();
    assert_eq!(plan["name"], "Quiet Street");
    assert_eq!(plan["sensor"], "X-Trans IV");
    assert_eq!(plan["params"]["look"], "Street Negative");
    assert_eq!(plan["settings"]["amb:WBShiftBlue"], -3.0);
    let preset = std::fs::read_to_string(dir.join("quiet.xmp")).unwrap();
    assert!(
        preset.contains("Fuji") && preset.contains("amb:WBShiftRed"),
        "{preset}"
    );

    amb(&dir, &["fuji", "recipe.txt", "p.dng", "--apply"]);
    let sidecar = std::fs::read_to_string(dir.join("p.xmp")).unwrap();
    assert!(sidecar.contains("GrainAmount"), "{sidecar}");
    let _ = std::fs::remove_dir_all(&dir);
}
