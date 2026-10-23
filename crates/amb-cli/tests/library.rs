// SPDX-License-Identifier: GPL-3.0-or-later
//! The library commands end to end, on copies of a synthetic DNG: tag and
//! info, places from a GPX track, duplicates, rename, sort, keywords,
//! collections, missing photos and the trash. Each test has its own folder,
//! catalogue and config folder.

use std::path::PathBuf;
use std::process::Command;

struct Lib {
    dir: PathBuf,
}

impl Lib {
    fn new(name: &str) -> Lib {
        let dir =
            std::env::temp_dir().join(format!("amb-cli-library-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("p")).unwrap();
        let lib = Lib { dir };
        lib.amb(&["testimage", "colorchecker", "p/IMG_1.dng"]);
        for n in [2, 3] {
            std::fs::copy(
                lib.dir.join("p/IMG_1.dng"),
                lib.dir.join(format!("p/IMG_{n}.dng")),
            )
            .unwrap();
        }
        lib
    }

    /// Run `amb` in the folder; stdout, or a panic with stderr.
    fn amb(&self, args: &[&str]) -> String {
        let out = Command::new(env!("CARGO_BIN_EXE_amb"))
            .args(args)
            .args(["--catalog", "catalog.sqlite"].iter().filter(|_| {
                !matches!(
                    args[0],
                    "testimage" | "info" | "tag" | "place" | "collection"
                )
            }))
            .current_dir(&self.dir)
            .env("XDG_CONFIG_HOME", self.dir.join("config"))
            .env("XDG_DATA_HOME", self.dir.join("data"))
            .env("HOME", &self.dir)
            .env("APPDATA", self.dir.join("config"))
            .output()
            .expect("amb runs");
        assert!(
            out.status.success(),
            "amb {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    fn json(&self, args: &[&str]) -> serde_json::Value {
        let mut a = args.to_vec();
        a.push("--json");
        serde_json::from_str(&self.amb(&a)).unwrap()
    }

    fn has(&self, p: &str) -> bool {
        self.dir.join(p).exists()
    }
}

impl Drop for Lib {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn tag_many_photos_then_read_them_back() {
    let lib = Lib::new("tag");
    lib.amb(&[
        "tag",
        "p",
        "--rating",
        "4",
        "--add",
        "beach, sun",
        "--label",
        "Red",
        "--place",
        "52.37,4.89",
        "--creator",
        "A. Person",
    ]);
    lib.amb(&[
        "tag",
        "p/IMG_2.dng",
        "--remove",
        "sun",
        "--flag",
        "reject",
        "--place",
        "none",
    ]);
    let info = lib.json(&["info", "p"]);
    let all = info.as_array().unwrap();
    assert_eq!(all.len(), 3);
    let two = all
        .iter()
        .find(|e| e["path"].as_str().unwrap().ends_with("IMG_2.dng"))
        .unwrap();
    assert_eq!(two["info"]["keywords"], serde_json::json!(["beach"]));
    assert_eq!(two["info"]["flag"], "reject");
    assert!(two["info"].get("gps").is_none(), "{two}");
    let one = all
        .iter()
        .find(|e| e["path"].as_str().unwrap().ends_with("IMG_1.dng"))
        .unwrap();
    assert_eq!(one["info"]["rating"], 4);
    assert_eq!(one["info"]["gps"], "52.370000,4.890000");
    assert_eq!(one["info"]["creator"], "A. Person");
    // A dry run changes nothing.
    lib.amb(&["tag", "p", "--rating", "1", "--dry-run"]);
    assert_eq!(lib.json(&["info", "p/IMG_1.dng"])[0]["info"]["rating"], 4);
}

#[test]
fn places_come_from_a_gpx_track() {
    let lib = Lib::new("place");
    // The synthetic DNG has no capture time: nothing to place by.
    std::fs::write(
        lib.dir.join("t.gpx"),
        r#"<gpx><trk><trkseg><trkpt lat="10" lon="20"><time>2025-01-01T00:00:00Z</time></trkpt></trkseg></trk></gpx>"#,
    )
    .unwrap();
    let r = lib.json(&["place", "p", "--gpx", "t.gpx"]);
    assert_eq!(r["unplaced"], 3);
}

#[test]
fn duplicates_rename_sort_and_trash() {
    let lib = Lib::new("files");
    // Three copies of one file: one group of the same file.
    let groups = lib.json(&["duplicates", "p", "--reject"]);
    assert_eq!(groups.as_array().unwrap().len(), 1, "{groups}");
    assert_eq!(groups[0]["kind"], "same");
    let info = lib.json(&["info", "p"]);
    let rejected = info
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["info"]["flag"] == "reject")
        .count();
    assert_eq!(rejected, 2);
    // Rename: shown first, then done, with the sidecars.
    lib.amb(&["rename", "p", "--name", "trip_{seq}", "--dry-run"]);
    assert!(lib.has("p/IMG_1.dng"));
    lib.amb(&["rename", "p", "--name", "trip_{seq}"]);
    for n in 1..=3 {
        assert!(lib.has(&format!("p/trip_000{n}.dng")) && lib.has(&format!("p/trip_000{n}.xmp")));
    }
    // Sort: no capture time, so the file's date.
    lib.amb(&["sort", "p", "--into", "sorted", "--by", "{yyyy}"]);
    let year = std::fs::read_dir(lib.dir.join("sorted"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    assert_eq!(std::fs::read_dir(&year).unwrap().count(), 6);
    // Trash: the photo and its sidecar. Only on Linux, where the trash is a
    // folder the test can give (macOS and Windows use the system's own).
    if cfg!(target_os = "linux") {
        let photo = year.join("trip_0001.dng");
        lib.amb(&["trash", photo.to_str().unwrap()]);
        assert!(!photo.exists() && !year.join("trip_0001.xmp").exists());
        assert!(lib.has("data/Trash/files/trip_0001.dng"));
    }
}

#[test]
fn keywords_collections_and_missing_photos() {
    let lib = Lib::new("keywords");
    lib.amb(&["tag", "p", "--add", "beach"]);
    lib.amb(&["catalog", "scan", "p"]);
    let words = lib.json(&["keywords"]);
    assert_eq!(words, serde_json::json!([["beach", 3]]));
    lib.amb(&["keywords", "rename", "beach", "--to", "sea"]);
    assert_eq!(lib.json(&["keywords"]), serde_json::json!([["sea", 3]]));
    lib.amb(&["collection", "add", "Trip", "p/IMG_1.dng", "p/IMG_2.dng"]);
    assert_eq!(lib.json(&["collection"])[0]["count"], 2);
    // Moved outside the app: found again by name and size, and the
    // collection follows.
    std::fs::create_dir_all(lib.dir.join("elsewhere")).unwrap();
    std::fs::rename(
        lib.dir.join("p/IMG_1.dng"),
        lib.dir.join("elsewhere/IMG_1.dng"),
    )
    .unwrap();
    assert_eq!(
        lib.json(&["catalog", "missing"]).as_array().unwrap().len(),
        1
    );
    lib.amb(&["catalog", "relink", "elsewhere"]);
    assert!(
        lib.json(&["catalog", "missing"])
            .as_array()
            .unwrap()
            .is_empty()
    );
    let shown = lib.json(&["collection", "show", "Trip"]);
    let moved = PathBuf::from(shown[0].as_str().unwrap());
    assert!(
        moved.is_absolute() && moved.ends_with("elsewhere/IMG_1.dng"),
        "{shown}"
    );
}

#[test]
fn slideshow_starts_the_editor_with_its_options() {
    let lib = Lib::new("slideshow");
    let cmd = lib.amb(&[
        "slideshow",
        "p",
        "--seconds",
        "7",
        "--effect",
        "random",
        "--shuffle",
        "--dry-run",
    ]);
    assert!(
        cmd.contains("--slideshow --seconds 7 --effect random --shuffle"),
        "{cmd}"
    );
    assert!(
        cmd.trim_end().ends_with("p"),
        "the folder, as a folder: {cmd}"
    );
    let bad = Command::new(env!("CARGO_BIN_EXE_amb"))
        .args(["slideshow", "p", "--effect", "wipe", "--dry-run"])
        .current_dir(&lib.dir)
        .output()
        .unwrap();
    assert!(!bad.status.success());
    assert!(String::from_utf8_lossy(&bad.stderr).contains("one of none, fade"));
}
