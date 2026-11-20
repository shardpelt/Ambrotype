// SPDX-License-Identifier: GPL-3.0-or-later
//! Library operations that work on folders of photos and their XMP
//! sidecars, shared by the `amb` tool and the editor. The files are the
//! library; the catalogue only remembers what was read from them.

pub mod catalog;
pub mod collections;
pub mod contact;
pub mod duplicates;
pub mod files;
pub mod folder;
pub mod gpx;
pub mod import;
pub mod info;
pub mod insights;
pub mod lensdb;
pub mod lightroom;
pub mod look;
pub mod lrcat;
pub mod places;
pub mod roots;
pub mod search;
pub mod smart;
pub mod styles;

use std::path::PathBuf;

/// The editor's config folder, shared by the editor and `amb`:
/// `~/.config/org.ambrotype.editor` on Linux, under
/// `~/Library/Application Support` on macOS and `%APPDATA%` on Windows.
pub fn editor_config_dir() -> Option<PathBuf> {
    let env = |v: &str| {
        std::env::var_os(v)
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
    };
    let config = if cfg!(windows) {
        env("APPDATA")?
    } else if cfg!(target_os = "macos") {
        env("HOME")?.join("Library/Application Support")
    } else {
        env("XDG_CONFIG_HOME").or_else(|| env("HOME").map(|h| h.join(".config")))?
    };
    Some(config.join("org.ambrotype.editor"))
}
