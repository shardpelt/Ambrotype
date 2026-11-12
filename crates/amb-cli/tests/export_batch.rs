// SPDX-License-Identifier: GPL-3.0-or-later
//! `amb render FOLDER --out-dir OUT --recursive`: the same tree, smaller,
//! and a second run with `--skip-existing` that does nothing.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn amb(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_amb"))
        .args(args)
        .output()
        .unwrap()
}

fn s(p: &Path) -> String {
    p.display().to_string()
}

/// JPEG size from its header (the SOF marker).
fn jpeg_size(path: &Path) -> (u32, u32) {
    let b = std::fs::read(path).unwrap();
    let mut i = 2;
    while i + 9 < b.len() {
        let marker = b[i + 1];
        if (0xC0..=0xC3).contains(&marker) {
            let h = u32::from(b[i + 5]) << 8 | u32::from(b[i + 6]);
            let w = u32::from(b[i + 7]) << 8 | u32::from(b[i + 8]);
            return (w, h);
        }
        i += 2 + (usize::from(b[i + 2]) << 8 | usize::from(b[i + 3]));
    }
    panic!("no size in {}", path.display());
}

#[test]
fn renders_a_tree_smaller_and_carries_on() {
    let root = std::env::temp_dir().join(format!("amb-cli-batch-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let (tree, out): (PathBuf, PathBuf) = (root.join("tree"), root.join("out"));
    for f in ["one/a.dng", "two/deep/b.dng", "two/c.dng"] {
        std::fs::create_dir_all(tree.join(f).parent().unwrap()).unwrap();
        let r = amb(&["testimage", "colorchecker", &s(&tree.join(f))]);
        assert!(r.status.success());
    }
    let (t, o) = (s(&tree), s(&out));
    let base = [
        "render",
        &t,
        "--out-dir",
        &o,
        "--recursive",
        "--long-edge",
        "256",
        "--format",
        "jpeg",
    ];
    // A dry run writes nothing.
    let dry = amb(&[&base[..], &["--dry-run"]].concat());
    assert!(dry.status.success());
    assert!(!out.exists());
    let first = amb(&base);
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    for f in ["one/a.jpg", "two/deep/b.jpg", "two/c.jpg"] {
        let (w, h) = jpeg_size(&out.join(f));
        assert_eq!(w.max(h), 256, "{f}");
    }
    let again = amb(&[&base[..], &["--skip-existing", "--json"]].concat());
    assert!(again.status.success());
    let report: serde_json::Value = serde_json::from_slice(&again.stdout).unwrap();
    assert_eq!(
        (report["done"].as_u64(), report["skipped"].as_u64()),
        (Some(0), Some(3))
    );
    let _ = std::fs::remove_dir_all(&root);
}
