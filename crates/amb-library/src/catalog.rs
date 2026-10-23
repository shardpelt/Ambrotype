// SPDX-License-Identifier: GPL-3.0-or-later
//! The catalogue: an index of what the library knows about each photo, so
//! big folders open quickly and every photo seen can be searched at once.
//!
//! The photos' sidecars stay the library (docs/design/library.md). The
//! catalogue only remembers what was read from them and from the raw's
//! header (capture time, camera, lens, exposure), keyed by path and checked
//! against the file sizes and modified times of the photo and its sidecar:
//! anything changed, by us or by another app, is read again. So it can't go
//! out of date, and deleting it loses nothing; it is rebuilt as folders are
//! shown, and the library folders (`roots`) are indexed in the background.
//! Photos on a drive that isn't plugged in stay listed, marked offline.
//!
//! One SQLite file in the editor's config folder (`catalog.sqlite`).

use amb_protocol::sidecar::{self, PhotoInfo};
use rayon::prelude::*;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Bumped when what is stored changes; an older catalogue is emptied.
const VERSION: i64 = 2;

/// What the raw's header says about the shot.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Shot {
    /// "YYYY-MM-DD HH:MM:SS".
    pub captured: Option<String>,
    /// "NIKON D750": make and model, the make left out when the model
    /// starts with it.
    pub camera: Option<String>,
    pub lens: Option<String>,
    pub iso: Option<u32>,
    /// f-number.
    pub aperture: Option<f64>,
    /// Seconds.
    pub shutter: Option<f64>,
    /// Millimetres.
    pub focal: Option<f64>,
    /// Where the camera's GPS says it was taken: latitude, longitude.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub camera_gps: Option<[f64; 2]>,
}

/// What the library shows of a photo: its sidecar's rating, label,
/// keywords and so on, whether it has an edit, and the shot.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Record {
    pub info: PhotoInfo,
    pub edited: bool,
    pub shot: Shot,
}

/// Sizes and modified times (nanoseconds) of a photo and its sidecar (-1
/// when there is none); a record is current while these match.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stamp {
    pub size: i64,
    pub modified: i64,
    pub sidecar_size: i64,
    pub sidecar_modified: i64,
}

fn file_stamp(path: &Path) -> Option<(i64, i64)> {
    let m = std::fs::metadata(path).ok()?;
    let modified = m
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_nanos();
    Some((m.len() as i64, modified as i64))
}

fn stamp(photo: &Path) -> Option<Stamp> {
    let (size, modified) = file_stamp(&amb_engine::source_path(photo))?;
    let (sidecar_size, sidecar_modified) =
        file_stamp(&amb_engine::sidecar_path(photo)).unwrap_or((-1, -1));
    Some(Stamp {
        size,
        modified,
        sidecar_size,
        sidecar_modified,
    })
}

/// Read a photo's record from its files, without the catalogue.
pub fn read(photo: &Path) -> Record {
    let text = std::fs::read_to_string(amb_engine::sidecar_path(photo)).ok();
    let info = text.as_deref().map(sidecar::read_info).unwrap_or_default();
    let edited = text
        .as_deref()
        .and_then(|t| amb_presets::xmp::parse_crs(t).ok())
        .is_some_and(|s| !s.is_empty());
    let shot = amb_engine::raw::read_metadata(&amb_engine::source_path(photo))
        .map(|(_, m)| shot_of(&m))
        .unwrap_or_default();
    Record { info, edited, shot }
}

fn shot_of(m: &amb_engine::raw::PhotoMetadata) -> Shot {
    let ratio = |r: Option<(u32, u32)>| {
        r.filter(|r| r.1 != 0)
            .map(|(n, d)| f64::from(n) / f64::from(d))
    };
    let make = m.make.trim();
    let model = m.model.trim();
    let camera = if model.to_lowercase().starts_with(&make.to_lowercase()) || make.is_empty() {
        model.to_string()
    } else {
        format!("{make} {model}")
    };
    Shot {
        // EXIF writes "2026:05:04 12:30:00".
        captured: m.date_time_original.as_deref().map(|d| {
            let (date, time) = d.split_once(' ').unwrap_or((d, ""));
            format!("{} {time}", date.replace(':', "-"))
                .trim()
                .to_string()
        }),
        camera: Some(camera).filter(|c| !c.is_empty()),
        lens: m.lens_model.clone().filter(|l| !l.trim().is_empty()),
        iso: m.iso,
        aperture: ratio(m.fnumber),
        shutter: ratio(m.exposure_time),
        focal: ratio(m.focal_length),
        camera_gps: m.gps.as_ref().and_then(|g| {
            let deg = |(r, dms): &(String, [(u32, u32); 3]), neg: &str| {
                let [d, m, s] = dms.map(|v| ratio(Some(v)));
                let v = d? + m.unwrap_or(0.0) / 60.0 + s.unwrap_or(0.0) / 3600.0;
                Some(if r.trim().eq_ignore_ascii_case(neg) {
                    -v
                } else {
                    v
                })
            };
            let (lat, lon) = (deg(&g.latitude, "S")?, deg(&g.longitude, "W")?);
            // Cameras without a fix write zeros.
            (lat != 0.0 || lon != 0.0).then_some([lat, lon])
        }),
    }
}

impl Record {
    /// Where it was taken: a place set in the sidecar, or the camera's GPS.
    pub fn place(&self) -> Option<[f64; 2]> {
        self.info
            .place()
            .map(|(a, b)| [a, b])
            .or(self.shot.camera_gps)
    }
}

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

/// A folder the catalogue has photos in, for the folder tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FolderCount {
    pub folder: PathBuf,
    pub photos: usize,
    /// The folder can't be reached now (a drive not plugged in).
    pub offline: bool,
}

/// What indexing a library folder did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Indexed {
    pub photos: usize,
    /// Read from their files: new, or changed since last time.
    pub read: usize,
    /// Gone from the folder, so no longer listed.
    pub forgotten: usize,
    /// The folder couldn't be reached; nothing was changed.
    pub offline: bool,
}

/// How many photos are indexed at a time between progress reports.
const INDEX_CHUNK: usize = 500;

/// How many photos the catalogue knows, in how many folders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Stats {
    pub photos: usize,
    pub folders: usize,
}

pub struct Catalog {
    db: Mutex<Connection>,
}

/// Where the editor keeps its catalogue.
pub fn default_path() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("AMB_CATALOG").filter(|p| !p.is_empty()) {
        return Some(PathBuf::from(p));
    }
    Some(
        crate::collections::editor_dir()?
            .parent()?
            .join("catalog.sqlite"),
    )
}

impl Catalog {
    /// Open (or create) the catalogue at `path`.
    pub fn open(path: &Path) -> Result<Catalog, String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let db = Connection::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Self::setup(db)
    }

    /// A catalogue kept in memory only (tests, or when the file can't be opened).
    pub fn in_memory() -> Catalog {
        Self::setup(Connection::open_in_memory().expect("in-memory SQLite")).expect("schema")
    }

    fn setup(db: Connection) -> Result<Catalog, String> {
        let e = |e: rusqlite::Error| e.to_string();
        // Several app windows and the `amb` tool may share it.
        db.busy_timeout(std::time::Duration::from_secs(5))
            .map_err(e)?;
        let _ = db.pragma_update(None, "journal_mode", "WAL");
        db.pragma_update(None, "synchronous", "NORMAL").map_err(e)?;
        let version: i64 = db
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .map_err(e)?;
        if version != VERSION {
            // Only a cache: an older layout is simply started again.
            db.execute_batch("DROP TABLE IF EXISTS photos;")
                .map_err(e)?;
        }
        db.execute_batch(
            "CREATE TABLE IF NOT EXISTS photos (
                path TEXT PRIMARY KEY,
                folder TEXT NOT NULL,
                size INTEGER NOT NULL,
                modified INTEGER NOT NULL,
                sidecar_size INTEGER NOT NULL,
                sidecar_modified INTEGER NOT NULL,
                record TEXT NOT NULL,
                captured TEXT,
                camera TEXT,
                lens TEXT,
                rating INTEGER
            );
            CREATE INDEX IF NOT EXISTS photos_folder ON photos(folder);
            CREATE INDEX IF NOT EXISTS photos_captured ON photos(captured);",
        )
        .map_err(e)?;
        db.pragma_update(None, "user_version", VERSION).map_err(e)?;
        Ok(Catalog { db: Mutex::new(db) })
    }

    /// The records of `photos`, in order: from the catalogue where the files
    /// haven't changed, read again (in parallel) and stored where they have.
    /// A photo that can't be found gets an empty record.
    pub fn records(&self, photos: &[PathBuf]) -> Vec<Record> {
        self.records_counted(photos).0
    }

    /// As [`Catalog::records`], and how many were read from their files.
    pub fn records_counted(&self, photos: &[PathBuf]) -> (Vec<Record>, usize) {
        let stamps: Vec<Option<Stamp>> = photos.par_iter().map(|p| stamp(p)).collect();
        let known = self.lookup(photos);
        let mut out: Vec<Option<Record>> = photos
            .iter()
            .zip(&stamps)
            .map(|(p, s)| {
                let (have, record) = known.get(&key(p))?;
                (Some(*have) == *s).then(|| record.clone())
            })
            .collect();
        let missing: Vec<usize> = (0..photos.len()).filter(|&i| out[i].is_none()).collect();
        if missing.is_empty() {
            return (out.into_iter().map(Option::unwrap_or_default).collect(), 0);
        }
        let read_count = missing.len();
        let fresh: Vec<(usize, Record)> =
            missing.par_iter().map(|&i| (i, read(&photos[i]))).collect();
        self.store(
            fresh
                .iter()
                .filter_map(|(i, r)| Some((&photos[*i], stamps[*i]?, r))),
        );
        for (i, r) in fresh {
            out[i] = Some(r);
        }
        (
            out.into_iter().map(Option::unwrap_or_default).collect(),
            read_count,
        )
    }

    /// One photo's record (see [`Catalog::records`]).
    pub fn record(&self, photo: &Path) -> Record {
        self.records(&[photo.to_path_buf()])
            .pop()
            .unwrap_or_default()
    }

    fn lookup(&self, photos: &[PathBuf]) -> HashMap<String, (Stamp, Record)> {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        let Ok(mut q) = db.prepare_cached(
            "SELECT size, modified, sidecar_size, sidecar_modified, record FROM photos WHERE path = ?1",
        ) else {
            return HashMap::new();
        };
        photos
            .iter()
            .filter_map(|p| {
                let k = key(p);
                let row = q
                    .query_row([&k], |r| {
                        Ok((
                            Stamp {
                                size: r.get(0)?,
                                modified: r.get(1)?,
                                sidecar_size: r.get(2)?,
                                sidecar_modified: r.get(3)?,
                            },
                            r.get::<_, String>(4)?,
                        ))
                    })
                    .optional()
                    .ok()??;
                let record = serde_json::from_str(&row.1).ok()?;
                Some((k, (row.0, record)))
            })
            .collect()
    }

    fn store<'a>(&self, rows: impl Iterator<Item = (&'a PathBuf, Stamp, &'a Record)>) {
        let mut db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        let Ok(tx) = db.transaction() else { return };
        {
            let Ok(mut q) = tx.prepare_cached(
                "INSERT OR REPLACE INTO photos
                 (path, folder, size, modified, sidecar_size, sidecar_modified, record, captured, camera, lens, rating)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            ) else {
                return;
            };
            for (path, s, r) in rows {
                let folder = path
                    .parent()
                    .map(|f| f.display().to_string())
                    .unwrap_or_default();
                let Ok(json) = serde_json::to_string(r) else {
                    continue;
                };
                let _ = q.execute(params![
                    key(path),
                    folder,
                    s.size,
                    s.modified,
                    s.sidecar_size,
                    s.sidecar_modified,
                    json,
                    r.shot.captured,
                    r.shot.camera,
                    r.shot.lens,
                    r.info.rating,
                ]);
            }
        }
        let _ = tx.commit();
    }

    /// Every photo in the catalogue, newest first, as last read. The files
    /// aren't looked at (only whether each folder can be reached), so this
    /// stays quick for hundreds of thousands of photos; indexing and showing
    /// a folder keep the records current.
    pub fn all(&self) -> Vec<Entry> {
        let rows: Vec<(String, String, String)> = {
            let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
            let Ok(mut q) =
                db.prepare("SELECT path, folder, record FROM photos ORDER BY captured DESC, path")
            else {
                return Vec::new();
            };
            q.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
                .map(|rows| rows.flatten().collect())
                .unwrap_or_default()
        };
        let reachable = reachable(rows.iter().map(|r| r.1.as_str()));
        rows.into_par_iter()
            .map(|(path, folder, json)| Entry {
                path: PathBuf::from(path),
                record: serde_json::from_str(&json).unwrap_or_default(),
                offline: !reachable.contains(&folder),
            })
            .collect()
    }

    /// The folders with photos, each with how many, sorted by path.
    pub fn folders(&self) -> Vec<FolderCount> {
        let rows: Vec<(String, i64)> = {
            let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
            let Ok(mut q) =
                db.prepare("SELECT folder, COUNT(*) FROM photos GROUP BY folder ORDER BY folder")
            else {
                return Vec::new();
            };
            q.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
                .map(|rows| rows.flatten().collect())
                .unwrap_or_default()
        };
        let reachable = reachable(rows.iter().map(|r| r.0.as_str()));
        rows.into_iter()
            .map(|(folder, n)| FolderCount {
                offline: !reachable.contains(&folder),
                folder: PathBuf::from(folder),
                photos: n as usize,
            })
            .collect()
    }

    /// Bring a library folder (and its subfolders) up to date: new and
    /// changed photos are read, photos gone from it are forgotten.
    /// `progress(done, total)` follows along. A folder that can't be
    /// reached (an unplugged drive) is left as it was, its photos offline.
    pub fn index(&self, root: &Path, progress: &dyn Fn(usize, usize)) -> Result<Indexed, String> {
        if !root.is_dir() {
            return Ok(Indexed {
                offline: true,
                ..Default::default()
            });
        }
        let paths: Vec<PathBuf> = crate::folder::tree_paths(root)?
            .into_iter()
            .map(|(p, _)| p)
            .collect();
        let mut read = 0;
        progress(0, paths.len());
        for (i, chunk) in paths.chunks(INDEX_CHUNK).enumerate() {
            read += self.records_counted(chunk).1;
            progress(
                (i * INDEX_CHUNK + chunk.len()).min(paths.len()),
                paths.len(),
            );
        }
        let seen: HashSet<String> = paths.iter().map(|p| key(p)).collect();
        let gone: Vec<PathBuf> = self
            .paths_under(root)
            .into_iter()
            .filter(|p| !seen.contains(p))
            .map(PathBuf::from)
            .collect();
        self.forget(&gone);
        Ok(Indexed {
            photos: paths.len(),
            read,
            forgotten: gone.len(),
            offline: false,
        })
    }

    /// The sizes and times last seen for `photo`'s files, so caches keyed by
    /// them (thumbnails) still find it while its drive is unplugged.
    pub fn stamp_of(&self, photo: &Path) -> Option<Stamp> {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        db.query_row(
            "SELECT size, modified, sidecar_size, sidecar_modified FROM photos WHERE path = ?1",
            [key(photo)],
            |r| {
                Ok(Stamp {
                    size: r.get(0)?,
                    modified: r.get(1)?,
                    sidecar_size: r.get(2)?,
                    sidecar_modified: r.get(3)?,
                })
            },
        )
        .optional()
        .ok()
        .flatten()
    }

    /// The catalogued photos in `folder` and its subfolders.
    fn paths_under(&self, folder: &Path) -> Vec<String> {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        let (f, prefix) = folder_and_prefix(folder);
        let Ok(mut q) = db.prepare(
            "SELECT path FROM photos WHERE folder = ?1 OR substr(folder, 1, length(?2)) = ?2",
        ) else {
            return Vec::new();
        };
        q.query_map(params![f, prefix], |r| r.get::<_, String>(0))
            .map(|rows| rows.flatten().collect())
            .unwrap_or_default()
    }

    /// The photos in the catalogue that match `query`, newest first.
    pub fn find(&self, query: &Query) -> Vec<Entry> {
        self.all()
            .into_iter()
            .filter(|e| query.matches(e))
            .collect()
    }

    /// Catalogue the photos in `dirs` and their subfolders. Returns how many
    /// photos there are and how many had to be read from their files.
    pub fn scan(&self, dirs: &[PathBuf]) -> Result<(usize, usize), String> {
        let mut paths = Vec::new();
        for d in dirs {
            paths.extend(crate::folder::tree_paths(d)?.into_iter().map(|(p, _)| p));
        }
        let (records, read) = self.records_counted(&paths);
        Ok((records.len(), read))
    }

    /// Forget `photos` (their files stay).
    pub fn forget(&self, photos: &[PathBuf]) {
        if photos.is_empty() {
            return;
        }
        let mut db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        let Ok(tx) = db.transaction() else { return };
        for p in photos {
            let _ = tx.execute("DELETE FROM photos WHERE path = ?1", [key(p)]);
        }
        let _ = tx.commit();
    }

    /// Forget every photo in `folder` and its subfolders.
    pub fn forget_folder(&self, folder: &Path) -> usize {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        let (f, prefix) = folder_and_prefix(folder);
        db.execute(
            "DELETE FROM photos WHERE folder = ?1 OR substr(folder, 1, length(?2)) = ?2",
            params![f, prefix],
        )
        .unwrap_or(0)
    }

    pub fn stats(&self) -> Stats {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        let count = |sql: &str| {
            db.query_row(sql, [], |r| r.get::<_, i64>(0))
                .map_or(0, |n| n as usize)
        };
        Stats {
            photos: count("SELECT COUNT(*) FROM photos"),
            folders: count("SELECT COUNT(DISTINCT folder) FROM photos"),
        }
    }
}

fn key(photo: &Path) -> String {
    photo.display().to_string()
}

/// A folder as stored, and the start its subfolders share ("/a" and "/a/").
fn folder_and_prefix(folder: &Path) -> (String, String) {
    let f = folder.display().to_string();
    let sep = std::path::MAIN_SEPARATOR;
    let prefix = format!("{}{sep}", f.trim_end_matches(sep));
    (f, prefix)
}

/// Which of these folders can be reached now, each looked at once.
fn reachable<'a>(folders: impl Iterator<Item = &'a str>) -> HashSet<String> {
    let unique: HashSet<&str> = folders.collect();
    unique
        .into_par_iter()
        .filter(|f| Path::new(f).is_dir())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("amb-catalog-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(photo: &Path, info: &PhotoInfo) {
        let text = sidecar::with_info(None, info).unwrap();
        sidecar::write_atomic(&amb_engine::sidecar_path(photo), &text).unwrap();
    }

    fn info(rating: u8, keyword: &str) -> PhotoInfo {
        PhotoInfo {
            rating: Some(rating),
            keywords: vec![keyword.into()],
            ..Default::default()
        }
    }

    #[test]
    fn records_follow_the_files() {
        let dir = temp_dir("follow");
        let photo = dir.join("a.jpg");
        std::fs::write(&photo, b"not really a jpeg").unwrap();
        let cat = Catalog::in_memory();
        assert_eq!(cat.record(&photo).info, PhotoInfo::default());
        assert_eq!(
            cat.stats(),
            Stats {
                photos: 1,
                folders: 1
            }
        );

        // Another app writes the sidecar: the change is seen.
        write(&photo, &info(4, "sea"));
        let r = cat.record(&photo);
        assert_eq!(r.info.rating, Some(4));
        assert_eq!(r.info.keywords, ["sea"]);

        // Unchanged files come from the catalogue, not the disk: a record
        // planted in it is what comes back.
        let mut planted = r.clone();
        planted.info.rating = Some(1);
        cat.store(std::iter::once((&photo, stamp(&photo).unwrap(), &planted)));
        assert_eq!(cat.record(&photo).info.rating, Some(1));

        // Gone from its folder: forgotten when the folder is indexed.
        std::fs::remove_file(&photo).unwrap();
        let r = cat.index(&dir, &|_, _| {}).unwrap();
        assert_eq!((r.photos, r.forgotten), (0, 1));
        assert!(cat.all().is_empty());
        assert_eq!(cat.stats().photos, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_catalogue_survives_reopening_and_folders_can_be_forgotten() {
        let dir = temp_dir("reopen");
        let sub = dir.join("2026-07");
        std::fs::create_dir_all(&sub).unwrap();
        let photos = [dir.join("a.jpg"), sub.join("b.jpg")];
        for p in &photos {
            std::fs::write(p, b"x").unwrap();
            write(p, &info(3, "hill"));
        }
        let file = dir.join("catalog.sqlite");
        {
            let cat = Catalog::open(&file).unwrap();
            assert_eq!(cat.records(&photos)[1].info.rating, Some(3));
        }
        let cat = Catalog::open(&file).unwrap();
        assert_eq!(
            cat.stats(),
            Stats {
                photos: 2,
                folders: 2
            }
        );
        assert_eq!(cat.all().len(), 2);
        assert_eq!(cat.forget_folder(&sub), 1);
        assert_eq!(cat.forget_folder(&dir), 1);
        assert_eq!(cat.stats().photos, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn photos_on_an_unplugged_drive_stay_listed_as_offline() {
        let dir = temp_dir("offline");
        let drive = dir.join("drive");
        let shoot = drive.join("2026").join("07");
        std::fs::create_dir_all(&shoot).unwrap();
        for name in ["a.jpg", "b.jpg", "c.jpg"] {
            std::fs::write(shoot.join(name), b"x").unwrap();
        }
        write(&shoot.join("a.jpg"), &info(5, "lake"));
        std::fs::write(drive.join("d.jpg"), b"x").unwrap();
        let cat = Catalog::in_memory();
        let reports = std::sync::Mutex::new(Vec::new());
        let r = cat
            .index(&drive, &|done, total| {
                reports.lock().unwrap().push((done, total))
            })
            .unwrap();
        assert_eq!((r.photos, r.read, r.forgotten, r.offline), (4, 4, 0, false));
        assert_eq!(reports.lock().unwrap().last(), Some(&(4, 4)));
        let counts: Vec<(PathBuf, usize)> = cat
            .folders()
            .into_iter()
            .map(|f| (f.folder, f.photos))
            .collect();
        assert_eq!(counts, [(drive.clone(), 1), (shoot.clone(), 3)]);

        // The drive is unplugged: nothing is forgotten, all of it is offline.
        let away = dir.join("away");
        std::fs::rename(&drive, &away).unwrap();
        assert!(cat.index(&drive, &|_, _| {}).unwrap().offline);
        let all = cat.all();
        assert_eq!(all.len(), 4);
        assert!(all.iter().all(|e| e.offline));
        let a = all.iter().find(|e| e.path.ends_with("a.jpg")).unwrap();
        assert_eq!(a.record.info.rating, Some(5));
        let s = cat.stamp_of(&a.path).unwrap();
        assert_eq!(s.size, 1);
        assert!(s.sidecar_size > 0);
        assert!(cat.folders().iter().all(|f| f.offline));

        // Plugged back in, with one photo deleted meanwhile.
        std::fs::rename(&away, &drive).unwrap();
        std::fs::remove_file(shoot.join("c.jpg")).unwrap();
        let r = cat.index(&drive, &|_, _| {}).unwrap();
        assert_eq!((r.photos, r.read, r.forgotten), (3, 0, 1));
        assert!(cat.all().iter().all(|e| !e.offline));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
