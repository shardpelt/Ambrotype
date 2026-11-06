// SPDX-License-Identifier: GPL-3.0-or-later
//! Edits kept somewhere other than the photo's sidecar: a Lightroom Classic
//! catalogue opened in place (`amb_library::lightroom`). The program that
//! opens one sets the [`Source`], asked by sidecar path; everything that
//! reads a sidecar reads it through [`text`] (`amb_engine::sidecar_text`
//! for a photo, `amb_protocol::DevelopState::load` for edits).
//!
//! The last change wins: a sidecar written after the catalogue last
//! changed (by Ambrotype, or Lightroom itself) is read; else the
//! catalogue's copy. So editing a photo here writes its sidecar as usual,
//! and editing it in Lightroom again brings the catalogue's back.

use std::path::Path;
use std::sync::{Arc, RwLock};

/// What a linked source knows of a photo.
#[derive(Clone, Debug)]
pub struct Linked {
    /// The photo's edits and library state as sidecar XMP.
    pub xmp: String,
    /// When the source last changed (nanoseconds since 1970).
    pub modified: i64,
}

/// Asked for each sidecar read (by its path, whether or not it exists).
pub type Source = dyn Fn(&Path) -> Option<Linked> + Send + Sync;

static SOURCE: RwLock<Option<Arc<Source>>> = RwLock::new(None);

/// Set (or with `None` clear) where photos' edits are found besides their
/// sidecars.
pub fn set_source(source: Option<Arc<Source>>) {
    *SOURCE.write().unwrap_or_else(|e| e.into_inner()) = source;
}

/// What the linked source knows of the photo whose sidecar is `sidecar`.
pub fn get(sidecar: &Path) -> Option<Linked> {
    let source = SOURCE.read().unwrap_or_else(|e| e.into_inner()).clone()?;
    source(sidecar)
}

/// A sidecar's text: the file (an empty one counts as none), or the linked
/// source's when that changed after the file.
pub fn text(sidecar: &Path) -> Option<String> {
    let own = || {
        std::fs::read_to_string(sidecar)
            .ok()
            .filter(|t| !t.trim().is_empty())
    };
    match get(sidecar) {
        None => own(),
        Some(l) => match file_stamp(sidecar) {
            Some((_, modified)) if modified >= l.modified => own().or(Some(l.xmp)),
            _ => Some(l.xmp),
        },
    }
}

/// The size and modified time (nanoseconds) of what [`text`] reads, to
/// tell when it changed; `None` when there's nothing.
pub fn stamp(sidecar: &Path) -> Option<(i64, i64)> {
    let own = file_stamp(sidecar);
    match (get(sidecar), own) {
        (Some(l), Some((_, modified))) if modified < l.modified => {
            Some((l.xmp.len() as i64, l.modified))
        }
        (Some(l), None) => Some((l.xmp.len() as i64, l.modified)),
        (_, own) => own,
    }
}

/// A file's size and modified time (nanoseconds).
pub fn file_stamp(path: &Path) -> Option<(i64, i64)> {
    let m = std::fs::metadata(path).ok()?;
    let modified = m
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_nanos();
    Some((m.len() as i64, modified as i64))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_newer_of_sidecar_and_catalogue_is_read() {
        let dir = std::env::temp_dir().join(format!("amb-linked-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (known, other) = (dir.join("a.xmp"), dir.join("b.xmp"));
        let k = known.clone();
        let then = std::time::SystemTime::now() - std::time::Duration::from_secs(100);
        let at = |t: std::time::SystemTime| {
            t.duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos() as i64
        };
        let changed = at(then);
        set_source(Some(Arc::new(move |p: &Path| {
            (p == k).then(|| Linked {
                xmp: "<lightroom/>".into(),
                modified: changed,
            })
        })));
        // No sidecar: the catalogue's.
        assert_eq!(text(&known).as_deref(), Some("<lightroom/>"));
        assert_eq!(text(&other), None);
        // A sidecar written since: it wins.
        std::fs::write(&known, "<ours/>").unwrap();
        assert_eq!(text(&known).as_deref(), Some("<ours/>"));
        // One older than the catalogue's last change: the catalogue's.
        let f = std::fs::File::options().append(true).open(&known).unwrap();
        f.set_modified(then - std::time::Duration::from_secs(10))
            .unwrap();
        assert_eq!(text(&known).as_deref(), Some("<lightroom/>"));
        assert_eq!(stamp(&known), Some((12, changed)));
        set_source(None);
        assert_eq!(text(&known).as_deref(), Some("<ours/>"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
