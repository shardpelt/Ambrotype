// SPDX-License-Identifier: GPL-3.0-or-later
//! A photo's rating, label, flag, keywords and text, read from and written to its sidecar.
//!
//! The sidecar is read where it is: the `.xmp` beside the photo, or a
//! Lightroom catalogue opened in place when it knows the photo
//! (`amb_engine::sidecar_text`). Writing keeps everything else in it.

use amb_protocol::sidecar::{self, PhotoInfo};
use serde::Deserialize;
use std::path::Path;

/// A photo's info as its sidecar says (the default when it has none).
pub fn read_info(photo: &Path) -> PhotoInfo {
    amb_engine::sidecar_text(photo)
        .as_deref()
        .map(sidecar::read_info)
        .unwrap_or_default()
}

/// Write `info` into the photo's sidecar, keeping everything else in it, and return what was written.
pub fn write_info(photo: &Path, info: &PhotoInfo) -> Result<PhotoInfo, String> {
    let path = amb_engine::sidecar_path(photo);
    // From a Lightroom catalogue opened in place, when it knows the photo.
    let on_disk = amb_engine::sidecar_text(photo);
    let text = sidecar::with_info(on_disk.as_deref(), info).map_err(|e| e.to_string())?;
    sidecar::write_atomic(&path, &text).map_err(|e| e.to_string())?;
    Ok(sidecar::read_info(&text))
}

/// A change to many photos' rating, label and keywords. Fields left out
/// stay as they are.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct InfoChange {
    /// 1–5, or 0 to clear.
    #[serde(default)]
    pub rating: Option<u8>,
    /// A colour label, or "" to clear.
    #[serde(default)]
    pub label: Option<String>,
    /// "pick", "reject", or "" to clear.
    #[serde(default)]
    pub flag: Option<String>,
    #[serde(default)]
    pub add_keywords: Vec<String>,
    #[serde(default)]
    pub remove_keywords: Vec<String>,
    /// Title, caption, creator and copyright: the text, or "" to clear.
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub caption: Option<String>,
    #[serde(default)]
    pub creator: Option<String>,
    #[serde(default)]
    pub copyright: Option<String>,
    /// Put in a stack, or out of one (an empty id).
    #[serde(default)]
    pub stack: Option<sidecar::StackRef>,
    /// Set where it was taken ("latitude,longitude"), or "" to clear it.
    #[serde(default)]
    pub gps: Option<String>,
    /// Turn it this many quarter turns clockwise (negative: anticlockwise),
    /// as it is seen.
    #[serde(default)]
    pub rotate: Option<i32>,
    /// Mirror it left to right, as it is seen.
    #[serde(default)]
    pub flip: bool,
}

impl InfoChange {
    /// Whether it turns or mirrors the photo.
    pub fn turns(&self) -> bool {
        self.rotate.is_some_and(|r| r.rem_euclid(4) != 0) || self.flip
    }

    /// `info` ready for [`InfoChange::apply`]: for a turn, with the
    /// camera's orientation filled in when the sidecar doesn't say. A
    /// virtual copy turns with its photo, so it can't turn alone.
    pub fn base(&self, mut info: PhotoInfo, photo: &Path) -> Result<PhotoInfo, String> {
        if self.turns() {
            if amb_engine::virtual_copy_of(photo).is_some() {
                return Err("A virtual copy turns with its photo: turn the photo itself.".into());
            }
            info.orientation
                .get_or_insert_with(|| amb_engine::orientation::camera(photo));
        }
        Ok(info)
    }

    pub fn apply(&self, info: &PhotoInfo) -> PhotoInfo {
        let mut out = info.clone();
        if self.turns() {
            out.orientation = Some(amb_engine::orientation::turned(
                out.orientation.unwrap_or(1),
                self.rotate.unwrap_or(0),
                self.flip,
            ));
        }
        if let Some(r) = self.rating {
            out.rating = (1..=5).contains(&r).then_some(r);
        }
        if let Some(l) = &self.label {
            out.label = Some(l.clone()).filter(|l| !l.trim().is_empty());
        }
        if let Some(f) = &self.flag {
            out.flag = sidecar::Flag::parse(f);
        }
        if let Some(g) = &self.gps {
            out.gps = Some(g.clone());
        }
        for (field, change) in [
            (&mut out.title, &self.title),
            (&mut out.caption, &self.caption),
            (&mut out.creator, &self.creator),
            (&mut out.copyright, &self.copyright),
        ] {
            if let Some(v) = change {
                *field = Some(v.clone());
            }
        }
        if let Some(s) = &self.stack {
            out.stack = Some(s.clone());
        }
        let remove: Vec<String> = self
            .remove_keywords
            .iter()
            .map(|k| k.trim().to_lowercase())
            .collect();
        out.keywords
            .retain(|k| !remove.contains(&k.trim().to_lowercase()));
        for k in &self.add_keywords {
            if !out
                .keywords
                .iter()
                .any(|x| x.eq_ignore_ascii_case(k.trim()))
            {
                out.keywords.push(k.trim().to_string());
            }
        }
        out.cleaned()
    }
}

/// Write a change to one photo's sidecar (not the open photo: the session
/// owns that one). Returns the new info.
pub fn change_info(photo: &Path, change: &InfoChange) -> Result<PhotoInfo, String> {
    let info = change.apply(&change.base(read_info(photo), photo)?);
    write_info(photo, &info)
}

/// Put `other` in the stack `original` is in (top of a new one when it is in
/// none, with `other` under it). `original`'s info is the caller's, for a
/// photo a session owns: it gets back the info to save when the stack is new.
pub fn stack_with(original: &PhotoInfo, other: &Path) -> Result<Option<PhotoInfo>, String> {
    let (id, joined) = match &original.stack {
        Some(s) if !s.id.is_empty() => (s.id.clone(), None),
        _ => {
            let millis = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_millis());
            let id = format!("s{millis:x}");
            let top = sidecar::StackRef {
                id: id.clone(),
                top: true,
            };
            let info = PhotoInfo {
                stack: Some(top),
                ..original.clone()
            };
            (id, Some(info))
        }
    };
    let mut info = read_info(other);
    info.stack = Some(sidecar::StackRef { id, top: false });
    write_info(other, &info)?;
    Ok(joined)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_photo_is_stacked_with_another_and_joins_an_existing_stack() {
        let dir = std::env::temp_dir().join(format!("amb-stack-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (a, b, c) = (dir.join("a.dng"), dir.join("b.dng"), dir.join("c.dng"));
        let ramp = amb_testkit::patterns::generate("ramp").unwrap();
        for p in [&a, &b, &c] {
            amb_testkit::dng::write(p, &ramp, &Default::default()).unwrap();
        }
        let new = stack_with(&read_info(&a), &b)
            .unwrap()
            .expect("a new stack");
        let top = new.stack.clone().unwrap();
        assert!(top.top && !top.id.is_empty());
        let under = read_info(&b).stack.unwrap();
        assert_eq!((under.id.as_str(), under.top), (top.id.as_str(), false));
        // The original is in the stack now: the next photo joins it.
        assert!(stack_with(&new, &c).unwrap().is_none());
        assert_eq!(read_info(&c).stack.unwrap().id, top.id);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_change_is_written_and_read_back() {
        let dir = std::env::temp_dir().join(format!("amb-info-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let photo = dir.join("photo.dng");
        amb_testkit::dng::write(
            &photo,
            &amb_testkit::patterns::generate("ramp").unwrap(),
            &Default::default(),
        )
        .unwrap();
        let add = InfoChange {
            rating: Some(4),
            add_keywords: vec!["beach".into()],
            ..InfoChange::default()
        };
        change_info(&photo, &add).unwrap();
        let back = read_info(&photo);
        assert_eq!(back.rating, Some(4));
        assert_eq!(back.keywords, vec!["beach".to_string()]);
        let remove = InfoChange {
            remove_keywords: vec!["Beach".into()],
            ..InfoChange::default()
        };
        change_info(&photo, &remove).unwrap();
        assert!(read_info(&photo).keywords.is_empty());
        assert_eq!(read_info(&photo).rating, Some(4));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
