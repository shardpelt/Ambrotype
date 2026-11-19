// SPDX-License-Identifier: GPL-3.0-or-later
//! Asking the catalogue for photos: the filters and what comes back.

use super::Record;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A photo in the catalogue, for searching across folders.
#[derive(Debug, Clone, Serialize)]
pub struct Entry {
    pub path: PathBuf,
    #[serde(flatten)]
    pub record: Record,
    /// Its folder can't be reached (a drive not plugged in): listed from
    /// what was read last time.
    pub offline: bool,
}

/// What to look for across the catalogue; every part given must match.
/// Text matches ignore case and match anywhere.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Query {
    /// In the file name, keywords, title or caption.
    pub text: Option<String>,
    pub camera: Option<String>,
    pub lens: Option<String>,
    /// A keyword, whole (ignoring case).
    pub keyword: Option<String>,
    /// At least this many stars.
    pub rating: Option<u8>,
    /// Taken on or after / before this date ("2026-07" or "2026-07-14").
    pub from: Option<String>,
    pub until: Option<String>,
}

impl Query {
    pub fn matches(&self, e: &Entry) -> bool {
        let has = |hay: Option<&str>, needle: &Option<String>| match needle {
            None => true,
            Some(n) => hay.is_some_and(|h| h.to_lowercase().contains(&n.to_lowercase())),
        };
        let info = &e.record.info;
        let shot = &e.record.shot;
        let text = self.text.as_ref().is_none_or(|t| {
            let t = t.to_lowercase();
            let name = e.path.file_name().unwrap_or_default().to_string_lossy();
            name.to_lowercase().contains(&t)
                || info.keywords.iter().any(|k| k.to_lowercase().contains(&t))
                || [&info.title, &info.caption]
                    .into_iter()
                    .flatten()
                    .any(|v| v.to_lowercase().contains(&t))
        });
        let keyword = self
            .keyword
            .as_ref()
            .is_none_or(|k| info.keywords.iter().any(|w| w.eq_ignore_ascii_case(k)));
        let rating = self.rating.is_none_or(|r| info.rating.unwrap_or(0) >= r);
        let date = shot.captured.as_deref();
        let from = self
            .from
            .as_ref()
            .is_none_or(|f| date.is_some_and(|d| d >= f.as_str()));
        // "2026-07" until: the whole month counts.
        let until = self.until.as_ref().is_none_or(|u| {
            date.is_some_and(|d| d.get(..u.len()).is_some_and(|p| p <= u.as_str()))
        });
        text && keyword
            && rating
            && from
            && until
            && has(shot.camera.as_deref(), &self.camera)
            && has(shot.lens.as_deref(), &self.lens)
    }
}
