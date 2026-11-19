// SPDX-License-Identifier: GPL-3.0-or-later
//! Fingerprints and search vectors kept in the catalogue.

use super::{Catalog, key};
use amb_presets::linked::file_stamp;
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// What finding duplicates measured of a photo's file (`crate::duplicates`),
/// kept while the file's size and modified time stay the same.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Print {
    /// The file's SHA-256, hex; measured only when another file is as big.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    /// A 64-bit difference hash of the picture: alike pictures differ in few bits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dhash: Option<u64>,
    /// How sharp the picture looks (higher is sharper; only for comparing
    /// photos of one scene).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sharpness: Option<f32>,
}

impl Catalog {
    /// The prints kept for `photos` (`None` where none is, or the file changed).
    pub fn prints(&self, photos: &[PathBuf]) -> Vec<Option<Print>> {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        let Ok(mut q) =
            db.prepare_cached("SELECT size, modified, print FROM prints WHERE path = ?1")
        else {
            return vec![None; photos.len()];
        };
        photos
            .iter()
            .map(|p| {
                let (size, modified) = file_stamp(&amb_engine::source_path(p))?;
                let (s, m, json) = q
                    .query_row([key(p)], |r| {
                        Ok((
                            r.get::<_, i64>(0)?,
                            r.get::<_, i64>(1)?,
                            r.get::<_, String>(2)?,
                        ))
                    })
                    .optional()
                    .ok()??;
                ((s, m) == (size, modified))
                    .then(|| serde_json::from_str(&json).ok())
                    .flatten()
            })
            .collect()
    }

    /// Keep `prints` for their photos' files as they are now.
    pub(crate) fn store_prints(&self, prints: &[(PathBuf, Print)]) {
        let mut db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        let Ok(tx) = db.transaction() else { return };
        {
            let Ok(mut q) = tx.prepare_cached(
                "INSERT OR REPLACE INTO prints (path, size, modified, print) VALUES (?1, ?2, ?3, ?4)",
            ) else {
                return;
            };
            for (p, print) in prints {
                let Some((size, modified)) = file_stamp(&amb_engine::source_path(p)) else {
                    continue;
                };
                if let Ok(json) = serde_json::to_string(print) {
                    let _ = q.execute(params![key(p), size, modified, json]);
                }
            }
        }
        let _ = tx.commit();
    }
}

impl Catalog {
    /// The smart search vectors of `photos` from search model `model`
    /// (`None` where there is none yet, or the file changed).
    pub fn vectors(&self, model: &str, photos: &[PathBuf]) -> Vec<Option<Vec<f32>>> {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        let Ok(mut q) = db.prepare_cached(
            "SELECT size, modified, vector FROM vectors WHERE path = ?1 AND model = ?2",
        ) else {
            return vec![None; photos.len()];
        };
        photos
            .iter()
            .map(|p| {
                let (size, modified) = file_stamp(&amb_engine::source_path(p))?;
                let (s, m, blob) = q
                    .query_row(params![key(p), model], |r| {
                        Ok((
                            r.get::<_, i64>(0)?,
                            r.get::<_, i64>(1)?,
                            r.get::<_, Vec<u8>>(2)?,
                        ))
                    })
                    .optional()
                    .ok()??;
                ((s, m) == (size, modified)).then(|| {
                    blob.chunks_exact(4)
                        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                        .collect()
                })
            })
            .collect()
    }

    /// Keep the vectors of photos from search model `model`.
    pub(crate) fn store_vectors(&self, model: &str, vectors: &[(PathBuf, Vec<f32>)]) {
        let mut db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        let Ok(tx) = db.transaction() else { return };
        {
            let Ok(mut q) = tx.prepare_cached(
                "INSERT OR REPLACE INTO vectors (path, model, size, modified, vector) VALUES (?1, ?2, ?3, ?4, ?5)",
            ) else {
                return;
            };
            for (p, v) in vectors {
                let Some((size, modified)) = file_stamp(&amb_engine::source_path(p)) else {
                    continue;
                };
                let blob: Vec<u8> = v.iter().flat_map(|x| x.to_le_bytes()).collect();
                let _ = q.execute(params![key(p), model, size, modified, blob]);
            }
        }
        let _ = tx.commit();
    }
}
