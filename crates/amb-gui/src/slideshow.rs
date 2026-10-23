// SPDX-License-Identifier: GPL-3.0-or-later
//! `ambrotype --slideshow [--seconds N] [--effect E] [--shuffle] [--window]
//! PHOTO|DIR...` (what `amb slideshow` runs): the editor opens straight into a
//! slideshow of those photos (`ui/slideshow.js`).

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::Serialize;

use crate::folder::{self, FolderPhoto};

/// A slideshow asked for on the command line.
#[derive(Debug, Clone, PartialEq)]
pub struct Start {
    pub paths: Vec<String>,
    pub seconds: u32,
    pub effect: String,
    pub shuffle: bool,
    pub fullscreen: bool,
}

/// What the page gets: the photos, folders listed, and the options.
#[derive(Debug, Serialize)]
pub struct Show {
    pub photos: Vec<FolderPhoto>,
    pub seconds: u32,
    pub effect: String,
    pub shuffle: bool,
    pub fullscreen: bool,
}

/// Takes `--slideshow` and its options and paths out of `args`; `None`
/// (and `args` untouched) without `--slideshow`.
pub fn take(args: &mut Vec<String>) -> Option<Start> {
    if !args.iter().any(|a| a == "--slideshow") {
        return None;
    }
    let mut start = Start {
        paths: Vec::new(),
        seconds: 4,
        effect: "fade".into(),
        shuffle: false,
        fullscreen: true,
    };
    let mut it = std::mem::take(args).into_iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--slideshow" => {}
            "--shuffle" => start.shuffle = true,
            "--window" => start.fullscreen = false,
            "--seconds" => {
                if let Some(s) = it.next().and_then(|v| v.parse::<u32>().ok()) {
                    start.seconds = s.clamp(1, 600);
                }
            }
            "--effect" => {
                if let Some(e) = it.next() {
                    start.effect = e;
                }
            }
            _ if a.starts_with("--") => args.push(a),
            _ => start.paths.push(a),
        }
    }
    Some(start)
}

/// The slideshow asked for at start, once (the page asks when it loads).
pub static PENDING: Mutex<Option<Start>> = Mutex::new(None);

#[tauri::command]
pub(crate) async fn take_slideshow() -> Result<Option<Show>, String> {
    let Some(s) = PENDING.lock().map_err(|e| e.to_string())?.take() else {
        return Ok(None);
    };
    tauri::async_runtime::spawn_blocking(move || {
        let mut photos = Vec::new();
        let mut files = Vec::new();
        for p in &s.paths {
            let p = Path::new(p);
            if p.is_dir() {
                photos.extend(folder::describe(std::mem::take(&mut files)));
                photos.extend(folder::list_dir(p)?);
            } else {
                files.push(PathBuf::from(p));
            }
        }
        photos.extend(folder::describe(files));
        Ok(Some(Show {
            photos,
            seconds: s.seconds,
            effect: s.effect,
            shuffle: s.shuffle,
            fullscreen: s.fullscreen,
        }))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn options_and_paths_come_out() {
        let mut a = args(&[
            "--slideshow",
            "--seconds",
            "9",
            "--shuffle",
            "--effect",
            "slide",
            "--window",
            "/a",
            "/b.nef",
        ]);
        let s = take(&mut a).unwrap();
        assert!(a.is_empty());
        assert_eq!(
            s,
            Start {
                paths: args(&["/a", "/b.nef"]),
                seconds: 9,
                effect: "slide".into(),
                shuffle: true,
                fullscreen: false
            }
        );
        let mut plain = args(&["photo.nef"]);
        assert!(take(&mut plain).is_none());
        assert_eq!(plain, args(&["photo.nef"]));
    }
}
