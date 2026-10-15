// SPDX-License-Identifier: GPL-3.0-or-later
use std::fmt::Write;

fn main() {
    // The hub's starter presets ship with the app, for the Preset Library
    // when the hub can't be reached or lists none.
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("data/hub/presets");
    println!("cargo:rerun-if-changed={}", dir.display());
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .map(|d| d.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();
    files.retain(|p| p.extension().is_some_and(|e| e == "xmp"));
    files.sort();
    let mut out = String::from("&[\n");
    for p in files {
        let stem = p.file_stem().unwrap().to_string_lossy();
        let full = p.canonicalize().unwrap();
        writeln!(
            out,
            "    (\"preset-{stem}\", include_str!({:?})),",
            full.display()
        )
        .unwrap();
    }
    out.push(']');
    let dest = std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("bundled_presets.rs");
    std::fs::write(dest, out).unwrap();
    tauri_build::build()
}
