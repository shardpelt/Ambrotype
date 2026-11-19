// SPDX-License-Identifier: GPL-3.0-or-later
//! `amb render` end to end on copies of a synthetic DNG: how a batch names
//! its files, and the export options.

use std::path::PathBuf;
use std::process::Command;

struct Work {
    dir: PathBuf,
}

impl Work {
    fn new(name: &str) -> Work {
        let dir =
            std::env::temp_dir().join(format!("amb-cli-render-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let work = Work { dir };
        work.amb(&["testimage", "colorchecker", "a.dng"]);
        for n in ["b", "c"] {
            std::fs::copy(work.dir.join("a.dng"), work.dir.join(format!("{n}.dng"))).unwrap();
        }
        work
    }

    /// Run `amb` in the folder; stdout, or a panic with stderr.
    fn amb(&self, args: &[&str]) -> String {
        let out = Command::new(env!("CARGO_BIN_EXE_amb"))
            .args(args)
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

    fn size(&self, file: &str) -> u64 {
        std::fs::metadata(self.dir.join(file)).unwrap().len()
    }
}

impl Drop for Work {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn a_batch_names_each_photo_once() {
    let work = Work::new("batch");
    let out = work.amb(&[
        "render",
        "a.dng",
        "b.dng",
        "c.dng",
        "--out-dir",
        "out",
        "--name",
        "shot",
    ]);
    for n in ["shot.jpg", "shot-2.jpg", "shot-3.jpg"] {
        assert!(work.dir.join("out").join(n).exists(), "{n} missing");
    }
    for i in 1..=3 {
        assert!(out.contains(&format!("[{i}/3]")), "{out}");
    }
}

#[test]
fn export_options_shape_the_file() {
    let work = Work::new("options");
    work.amb(&["render", "a.dng", "edge.jpg", "--long-edge", "400"]);
    let (w, h) = image::image_dimensions(work.dir.join("edge.jpg")).unwrap();
    assert_eq!(w.max(h), 400);
    work.amb(&["render", "a.dng", "low.jpg", "--quality", "40"]);
    work.amb(&["render", "a.dng", "high.jpg", "--quality", "95"]);
    assert!(work.size("low.jpg") < work.size("high.jpg"));
}
