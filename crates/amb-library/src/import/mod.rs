// SPDX-License-Identifier: GPL-3.0-or-later
//! Import: copy photos from a card or folder into the library's folders,
//! sorted into folders and renamed from templates, as Lightroom's Import
//! "Copy" does. docs/design/library.md.
//!
//! - A photo travels with its companions: a raw with its same-name JPEG and
//!   its `.xmp` sidecar keep one name, so they stay a pair.
//! - Folder and name templates use the capture time from EXIF (the file's
//!   modified time, in UTC, when there is none) and a sequence number.
//! - Nothing is overwritten. A file already at the destination with the
//!   same bytes is skipped as imported; another file there with the same
//!   name makes the new one `name-2`.
//! - Each file is copied to a hidden temporary name, checked for size and
//!   then renamed, so an interrupted import leaves no half-copied photos.
//! - A develop preset can be applied as photos come in (not to one that
//!   brings its own edit in a sidecar), and a second copy of the originals,
//!   as they were named, can go to a backup folder.
//! - Move instead of copy deletes the card's files, but only once the copy
//!   (and the backup) is in place. Only some photos can be picked.
//! - An import that was stopped, or cut off, carries on when run again:
//!   photos already in place are skipped and half-copied leftovers cleared.
//! - `eject` hands a finished card to the system to unmount.

use std::collections::{BTreeMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

/// What happens to the photos on the card.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImportMode {
    /// Leave the originals where they are.
    #[default]
    Copy,
    /// Delete each original (and its companions) once its copy is checked.
    Move,
}

/// How to import.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ImportOptions {
    /// Subfolders under the destination, e.g. `{yyyy}/{yyyy}-{mm}-{dd}`;
    /// empty for the destination itself.
    pub folder: String,
    /// The new file name without extension, e.g. `{name}` (unchanged) or
    /// `{yyyy}{mm}{dd}_{seq}`.
    pub name: String,
    /// The first `{seq}` number.
    pub first_number: u32,
    /// Look in subfolders of the source too (a card's `DCIM/100XXXXX`).
    pub subfolders: bool,
    /// Keywords for every imported photo's sidecar.
    pub keywords: Vec<String>,
    /// A develop preset (`.xmp` or `.lrtemplate`) for every imported photo
    /// that doesn't bring its own edit.
    pub preset: Option<PathBuf>,
    /// A second copy of the originals, unrenamed, in
    /// `<backup>/Imported on YYYY-MM-DD/`.
    pub backup: Option<PathBuf>,
    /// Copy (the default) or move.
    pub mode: ImportMode,
    /// Only these photos: each a source file name (`IMG_1.NEF`, any case)
    /// or a full path. Empty for all.
    pub only: Vec<String>,
}

impl Default for ImportOptions {
    fn default() -> Self {
        ImportOptions {
            folder: "{yyyy}/{yyyy}-{mm}-{dd}".into(),
            name: "{name}".into(),
            first_number: 1,
            subfolders: true,
            keywords: Vec::new(),
            preset: None,
            backup: None,
            mode: ImportMode::Copy,
            only: Vec::new(),
        }
    }
}

/// The template tokens and what they stand for.
pub const TOKENS: &[(&str, &str)] = &[
    ("{name}", "the original file name, without extension"),
    ("{yyyy}", "capture year"),
    ("{yy}", "capture year, two digits"),
    ("{mm}", "capture month, 01-12"),
    ("{dd}", "capture day"),
    ("{hh}", "capture hour"),
    ("{min}", "capture minute"),
    ("{ss}", "capture second"),
    ("{seq}", "a sequence number, four digits, in capture order"),
    ("{camera}", "the camera model"),
];

/// One photo to import: its main file and its companions.
#[derive(Debug, Clone, Serialize)]
pub struct Planned {
    /// Source and destination of each file (the photo first).
    pub files: Vec<(PathBuf, PathBuf)>,
    /// "YYYY-MM-DD HH:MM:SS".
    pub captured: String,
    /// Why it isn't copied, if it isn't.
    pub skip: Option<String>,
}

/// What an import did.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Report {
    pub imported: usize,
    pub skipped: usize,
    pub errors: Vec<String>,
    /// The folders photos went into, in order.
    pub folders: Vec<PathBuf>,
    /// Stopped before the last photo; running the import again carries on.
    pub stopped: bool,
    /// Half-copied files of an earlier, cut-off import that were cleared.
    pub cleared: usize,
}

mod eject;
mod plan;
mod run;
mod source;
mod template;
#[cfg(test)]
mod tests;

pub use eject::eject;
pub use plan::plan;
use plan::same_file;
pub(crate) use run::copy_file;
use run::is_leftover;
pub use run::run;
pub(crate) use source::split_name;
use source::{Group, capture_time, group, scan};
pub(crate) use template::clean;
use template::{When, expand};
pub use template::{check_template, file_name};
