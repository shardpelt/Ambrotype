// SPDX-License-Identifier: GPL-3.0-or-later
//! `amb share` end to end: one HTML file holding the photo before and after.

use std::process::Command;

#[test]
fn a_page_holds_before_and_after() {
    let dir = std::env::temp_dir().join(format!("amb-cli-share-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let amb = |args: &[&str]| {
        let out = Command::new(env!("CARGO_BIN_EXE_amb"))
            .args(args)
            .current_dir(&dir)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "amb {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    };
    amb(&["testimage", "ramp", "p.dng"]);
    amb(&["do", "p.dng", "exp=+1"]);
    amb(&["share", "p.dng", "--out", "p.html", "--long-edge", "300"]);
    let page = std::fs::read_to_string(dir.join("p.html")).unwrap();
    assert_eq!(page.matches("data:image/jpeg;base64,").count(), 2);
    assert!(page.contains("<title>p: before and after</title>"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_contact_sheet_pages_a_roll() {
    let dir = std::env::temp_dir().join(format!("amb-cli-contact-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let amb = |args: &[&str]| {
        let out = Command::new(env!("CARGO_BIN_EXE_amb"))
            .args(args)
            .current_dir(&dir)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "amb {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    };
    for n in ["a", "b", "c"] {
        amb(&["testimage", "ramp", &format!("{n}.dng")]);
    }
    // Two across, a small page: more photos than fit make pages.
    let said = amb(&[
        "contact",
        "a.dng",
        "b.dng",
        "c.dng",
        "--out",
        "s.png",
        "--columns",
        "2",
        "--paper",
        "8x10",
        "--title",
        "Roll 1",
    ]);
    assert!(said.contains("3 photo(s)"), "{said}");
    assert!(
        dir.join("s.png").exists() || dir.join("s-001.png").exists(),
        "{said}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
