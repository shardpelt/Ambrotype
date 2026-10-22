// SPDX-License-Identifier: GPL-3.0-or-later
//! Moving from Lightroom Classic: read a catalogue (`.lrcat`, an SQLite
//! file) and write what it knows about each photo into the photo's XMP
//! sidecar, and its collections as ours.
//!
//! The catalogue is only read, never changed: it is opened read-only and
//! `immutable`, so Lightroom can still use it afterwards. Close Lightroom
//! first, as changes it has not yet saved into the file are not seen.
//!
//! Per photo: its develop settings (the catalogue's own copy, which is
//! newer than any sidecar Lightroom wrote), rating, colour label, pick or
//! reject flag, keywords, stack, and the metadata kept in the catalogue's
//! XMP (title, caption, creator, copyright, …). Virtual copies become ours
//! (`IMG_0001.copy-2.xmp`). Existing sidecars are never overwritten.
//!
//! Read from the documented layout of the catalogue's tables
//! (`Adobe_images`, `AgLibraryFile`, `AgLibraryFolder`,
//! `AgLibraryRootFolder`, …), as other free tools that read catalogues
//! do; nothing of Adobe's is used.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use amb_presets::Value;
use amb_protocol::sidecar::{self, Flag, PhotoInfo, StackRef};
use rusqlite::{Connection, OpenFlags, types::Value as Sql};
use serde::Serialize;

/// A photo as the catalogue keeps it.
#[derive(Debug, Clone, Default)]
pub struct Photo {
    pub id: i64,
    /// The file, as the catalogue last saw it.
    pub path: String,
    /// For a virtual copy, the photo it is a copy of.
    pub master: Option<i64>,
    pub info: PhotoInfo,
    /// The catalogue's XMP for the photo (metadata, and an older copy of
    /// the develop settings).
    pub xmp: Option<String>,
    /// The develop settings, as a Lua table (`s = { Exposure2012 = 0.5, … }`).
    pub develop: Option<String>,
}

/// A collection, with its photos by catalogue id.
#[derive(Debug, Clone)]
pub struct LrCollection {
    pub name: String,
    /// The collection sets it is in, outermost first, joined with " / ".
    pub set: Option<String>,
    pub photos: Vec<i64>,
}

/// What a catalogue holds that we migrate.
#[derive(Debug, Clone, Default)]
pub struct Contents {
    /// `Adobe_DBVersion`, when the catalogue has it.
    pub version: Option<String>,
    pub photos: Vec<Photo>,
    pub collections: Vec<LrCollection>,
    /// Smart collections are rules, not lists; we name them, not import them.
    pub smart_collections: Vec<String>,
    /// Signs that Lightroom has the catalogue open.
    pub warnings: Vec<String>,
}

/// Read a catalogue.
pub fn read(catalog: &Path) -> Result<Contents, String> {
    if !catalog.is_file() {
        return Err(format!("{}: no such file", catalog.display()));
    }
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY
        | OpenFlags::SQLITE_OPEN_URI
        | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let db = Connection::open_with_flags(uri(catalog), flags)
        .map_err(|e| format!("{}: {e}", catalog.display()))?;
    let not_lr = |e: rusqlite::Error| {
        format!(
            "{}: not a Lightroom Classic catalogue we can read ({e})",
            catalog.display()
        )
    };
    let mut out = Contents {
        version: query(
            &db,
            "SELECT value FROM Adobe_variablesTable WHERE name = 'Adobe_DBVersion'",
        )
        .ok()
        .and_then(|rows| rows.into_iter().next())
        .and_then(|row| text(&row[0])),
        ..Contents::default()
    };
    for (suffix, what) in [
        ("lock", "Lightroom may have the catalogue open"),
        (
            "-wal",
            "Lightroom has changes not yet saved into the catalogue",
        ),
    ] {
        let mut name = catalog.as_os_str().to_owned();
        name.push(if suffix == "lock" { ".lock" } else { suffix });
        if Path::new(&name).exists() {
            out.warnings
                .push(format!("{what}: close it first for the latest edits"));
        }
    }

    // Columns beyond the file and folder are read where the catalogue has
    // them, so an older or newer layout still gives what it can.
    let images = columns(&db, "Adobe_images");
    let opt = |c: &str| {
        if images.contains(c) {
            format!("i.{c}")
        } else {
            "NULL".into()
        }
    };
    let sql = format!(
        "SELECT i.id_local, r.absolutePath, fo.pathFromRoot, f.baseName, f.extension,
                {}, {}, {}, {}
         FROM Adobe_images i
         JOIN AgLibraryFile f ON f.id_local = i.rootFile
         JOIN AgLibraryFolder fo ON fo.id_local = f.folder
         JOIN AgLibraryRootFolder r ON r.id_local = fo.rootFolder
         ORDER BY i.id_local",
        opt("rating"),
        opt("colorLabels"),
        opt("pick"),
        opt("masterImage"),
    );
    for row in query(&db, &sql).map_err(not_lr)? {
        let ext = text(&row[4]).unwrap_or_default();
        let name = text(&row[3]).unwrap_or_default();
        let path = format!(
            "{}{}{}{}",
            text(&row[1]).unwrap_or_default(),
            text(&row[2]).unwrap_or_default(),
            name,
            if ext.is_empty() {
                String::new()
            } else {
                format!(".{ext}")
            }
        );
        let pick = number(&row[7]).unwrap_or(0.0);
        out.photos.push(Photo {
            id: integer(&row[0]).unwrap_or_default(),
            path,
            master: integer(&row[8]),
            info: PhotoInfo {
                rating: number(&row[5])
                    .map(|r| r.round().clamp(0.0, 5.0) as u8)
                    .filter(|r| *r > 0),
                label: text(&row[6]).filter(|l| !l.trim().is_empty()),
                flag: if pick > 0.5 {
                    Some(Flag::Pick)
                } else if pick < -0.5 {
                    Some(Flag::Reject)
                } else {
                    None
                },
                ..PhotoInfo::default()
            },
            ..Photo::default()
        });
    }
    let index: HashMap<i64, usize> = out
        .photos
        .iter()
        .enumerate()
        .map(|(i, p)| (p.id, i))
        .collect();
    let mut each = |sql: &str, f: &mut dyn FnMut(&mut Photo, &[Sql])| {
        // An optional table the catalogue doesn't have gives nothing.
        for row in query(&db, sql).unwrap_or_default() {
            if let Some(&i) = integer(&row[0]).and_then(|id| index.get(&id)) {
                f(&mut out.photos[i], &row[1..]);
            }
        }
    };
    each(
        "SELECT image, xmp FROM Adobe_AdditionalMetadata",
        &mut |p, row| p.xmp = text(&row[0]).filter(|x| !x.trim().is_empty()),
    );
    each(
        "SELECT image, text FROM Adobe_imageDevelopSettings",
        &mut |p, row| p.develop = text(&row[0]).filter(|x| !x.trim().is_empty()),
    );
    each(
        "SELECT ki.image, k.name FROM AgLibraryKeywordImage ki
         JOIN AgLibraryKeyword k ON k.id_local = ki.tag ORDER BY ki.image, k.name",
        &mut |p, row| p.info.keywords.extend(text(&row[0])),
    );
    each(
        "SELECT image, stack, position FROM AgLibraryFolderStackImage",
        &mut |p, row| {
            if let Some(stack) = integer(&row[0]) {
                p.info.stack = Some(StackRef {
                    id: format!("lrcat-{stack}"),
                    top: number(&row[1]).is_some_and(|pos| pos <= 1.0),
                });
            }
        },
    );

    read_collections(&db, &mut out);
    Ok(out)
}

fn read_collections(db: &Connection, out: &mut Contents) {
    const COLLECTION: &str = "com.adobe.ag.library.collection";
    const SET: &str = "com.adobe.ag.library.group";
    const SMART: &str = "com.adobe.ag.library.smart_collection";
    let system = if columns(db, "AgLibraryCollection").contains("systemOnly") {
        "systemOnly"
    } else {
        "NULL"
    };
    let rows = query(
        db,
        &format!("SELECT id_local, name, creationId, parent, {system} FROM AgLibraryCollection"),
    )
    .unwrap_or_default();
    struct Node {
        name: String,
        kind: String,
        parent: Option<i64>,
        system: bool,
    }
    let nodes: BTreeMap<i64, Node> = rows
        .iter()
        .filter_map(|r| {
            Some((
                integer(&r[0])?,
                Node {
                    name: text(&r[1]).unwrap_or_default(),
                    kind: text(&r[2]).unwrap_or_default(),
                    parent: integer(&r[3]),
                    system: number(&r[4]).is_some_and(|s| s != 0.0),
                },
            ))
        })
        .collect();
    let order = if columns(db, "AgLibraryCollectionImage").contains("positionInCollection") {
        "positionInCollection, image"
    } else {
        "image"
    };
    let mut members: HashMap<i64, Vec<i64>> = HashMap::new();
    for r in query(
        db,
        &format!(
            "SELECT collection, image FROM AgLibraryCollectionImage ORDER BY collection, {order}"
        ),
    )
    .unwrap_or_default()
    {
        if let (Some(c), Some(i)) = (integer(&r[0]), integer(&r[1])) {
            members.entry(c).or_default().push(i);
        }
    }
    for (id, node) in &nodes {
        if node.kind == SMART {
            out.smart_collections.push(node.name.clone());
            continue;
        }
        if node.kind != COLLECTION {
            continue;
        }
        let photos = members.remove(id).unwrap_or_default();
        // The Quick Collection is a system one, kept when it has photos.
        let name = if node.system {
            if photos.is_empty() {
                continue;
            }
            "Quick Collection".to_string()
        } else {
            node.name.trim().to_string()
        };
        if name.is_empty() {
            continue;
        }
        let mut sets = Vec::new();
        let mut parent = node.parent;
        while let Some(p) = parent.and_then(|p| nodes.get(&p)) {
            if p.kind == SET && !p.name.trim().is_empty() {
                sets.push(p.name.trim().to_string());
            }
            parent = p.parent;
            if sets.len() > 64 {
                break; // a loop in a damaged catalogue
            }
        }
        sets.reverse();
        out.collections.push(LrCollection {
            name,
            set: (!sets.is_empty()).then(|| sets.join(" / ")),
            photos,
        });
    }
}

/// An SQLite URI for `path`, read-only and immutable: no lock is taken and
/// nothing (no journal) is written next to the catalogue.
fn uri(path: &Path) -> String {
    let abs = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let s = abs.to_string_lossy().replace('\\', "/");
    let mut out = String::from(if s.starts_with('/') {
        "file://"
    } else {
        "file:///"
    });
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"/-._~:".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out + "?immutable=1"
}

fn query(db: &Connection, sql: &str) -> rusqlite::Result<Vec<Vec<Sql>>> {
    let mut stmt = db.prepare(sql)?;
    let n = stmt.column_count();
    stmt.query_map([], |row| (0..n).map(|i| row.get::<_, Sql>(i)).collect())?
        .collect()
}

fn columns(db: &Connection, table: &str) -> HashSet<String> {
    query(db, &format!("PRAGMA table_info({table})"))
        .unwrap_or_default()
        .into_iter()
        .filter_map(|r| text(&r[1]))
        .collect()
}

fn text(v: &Sql) -> Option<String> {
    match v {
        Sql::Text(s) => Some(s.clone()),
        Sql::Blob(b) => String::from_utf8(b.clone()).ok(),
        Sql::Integer(i) => Some(i.to_string()),
        Sql::Real(r) => Some(r.to_string()),
        Sql::Null => None,
    }
}

fn number(v: &Sql) -> Option<f64> {
    match v {
        Sql::Integer(i) => Some(*i as f64),
        Sql::Real(r) => Some(*r),
        Sql::Text(s) => s.trim().parse().ok(),
        _ => None,
    }
}

fn integer(v: &Sql) -> Option<i64> {
    number(v).map(|n| n as i64)
}

/// How to migrate.
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// Path prefixes to replace, for photos that have moved since (another
    /// disk, another computer): `("/Volumes/Photos/", "/mnt/photos/")`.
    pub remap: Vec<(String, String)>,
}

/// What happens to one photo.
#[derive(Debug, Clone, Serialize)]
pub struct PlannedPhoto {
    /// The photo file (for a virtual copy, the photo it is a copy of).
    pub photo: PathBuf,
    /// The sidecar to write; for a virtual copy, the copy itself.
    pub sidecar: PathBuf,
    /// `write`, `keep` (a sidecar is already there) or `missing` (no photo).
    pub action: &'static str,
    /// A part that couldn't be read (the photo is still migrated).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
    #[serde(skip)]
    text: String,
}

/// A collection to make (or add to).
#[derive(Debug, Clone, Serialize)]
pub struct PlannedCollection {
    pub name: String,
    pub set: Option<String>,
    /// By the path the editor knows them by (a virtual copy by its sidecar).
    pub photos: Vec<PathBuf>,
}

/// Everything a migration would do; nothing is written until [`apply`].
#[derive(Debug, Clone, Serialize)]
pub struct Plan {
    pub version: Option<String>,
    pub photos: Vec<PlannedPhoto>,
    pub collections: Vec<PlannedCollection>,
    pub smart_collections: Vec<String>,
    pub warnings: Vec<String>,
}

impl Plan {
    pub fn count(&self, action: &str) -> usize {
        self.photos.iter().filter(|p| p.action == action).count()
    }
}

/// Read `catalog` and work out what to write.
pub fn plan(catalog: &Path, options: &Options) -> Result<Plan, String> {
    Ok(plan_contents(read(catalog)?, options))
}

pub fn plan_contents(contents: Contents, options: &Options) -> Plan {
    let remap = |p: &str| -> PathBuf {
        for (from, to) in &options.remap {
            if let Some(rest) = p.strip_prefix(from.as_str()) {
                return PathBuf::from(format!("{to}{rest}"));
            }
        }
        PathBuf::from(p)
    };
    let files: HashMap<i64, PathBuf> = contents
        .photos
        .iter()
        .map(|p| (p.id, remap(&p.path)))
        .collect();
    // Virtual copies are numbered from 2 in the catalogue's order, so a
    // second run finds the same names (and keeps them).
    let mut copies: HashMap<i64, u32> = HashMap::new();
    let mut known: HashMap<i64, PathBuf> = HashMap::new();
    let mut photos = Vec::new();
    for p in &contents.photos {
        let file = match p.master {
            Some(m) => files.get(&m).cloned().unwrap_or_else(|| remap(&p.path)),
            None => files[&p.id].clone(),
        };
        let sidecar = match p.master {
            Some(m) => {
                let n = copies.entry(m).or_insert(1);
                *n += 1;
                copy_path(&file, *n)
            }
            None => amb_engine::sidecar_path(&file),
        };
        known.insert(
            p.id,
            if p.master.is_some() {
                sidecar.clone()
            } else {
                file.clone()
            },
        );
        let (text, problem) = sidecar_text(p);
        let action = if !file.is_file() {
            "missing"
        } else if sidecar.exists() {
            "keep"
        } else {
            "write"
        };
        photos.push(PlannedPhoto {
            photo: file,
            sidecar,
            action,
            problem,
            text,
        });
    }

    // Two collections of one name (in different sets) would merge into one
    // of ours: name the later ones by their set.
    let mut seen = HashSet::new();
    let collections = contents
        .collections
        .iter()
        .map(|c| {
            let mut name = c.name.clone();
            if !seen.insert(name.to_lowercase()) {
                name = format!("{} ({})", c.name, c.set.as_deref().unwrap_or("Lightroom"));
                let mut n = 2;
                while !seen.insert(name.to_lowercase()) {
                    name = format!("{} ({n})", c.name);
                    n += 1;
                }
            }
            PlannedCollection {
                name,
                set: c.set.clone(),
                photos: c
                    .photos
                    .iter()
                    .filter_map(|id| known.get(id).cloned())
                    .collect(),
            }
        })
        .collect();
    Plan {
        version: contents.version,
        photos,
        collections,
        smart_collections: contents.smart_collections,
        warnings: contents.warnings,
    }
}

/// `IMG_0001.copy-2.xmp` for `IMG_0001.nef` (as
/// [`amb_engine::new_virtual_copy_path`] names them).
fn copy_path(photo: &Path, n: u32) -> PathBuf {
    let base = if amb_engine::raster::is_raster(photo) {
        photo.file_name()
    } else {
        photo.file_stem()
    }
    .unwrap_or_default()
    .to_string_lossy()
    .into_owned();
    photo.with_file_name(format!("{base}.copy-{n}.xmp"))
}

/// The sidecar for a photo: the catalogue's XMP (for its metadata), with
/// the develop settings and library state from the catalogue's tables.
fn sidecar_text(p: &Photo) -> (String, Option<String>) {
    let base = p.xmp.as_deref().unwrap_or("");
    let mut problem = None;
    let mut settings = p
        .develop
        .as_deref()
        .and_then(|lua| match develop_settings(lua) {
            Ok(s) => Some(s),
            Err(e) => {
                problem = Some(format!("develop settings: {e}"));
                None
            }
        });
    if settings.is_none() && !base.trim().is_empty() {
        settings = amb_presets::xmp::parse_crs(base).ok();
    }
    let settings = settings.unwrap_or_default();
    match sidecar::merge_with_info(base, &settings, Some(&p.info)) {
        Ok(text) => (text, problem),
        // The catalogue's XMP is damaged: the settings and state alone.
        Err(e) => (
            sidecar::merge_with_info("", &settings, Some(&p.info)).unwrap_or_default(),
            Some(format!("catalogue XMP: {e}")),
        ),
    }
}

/// The settings in an `Adobe_imageDevelopSettings` text: a Lua table of
/// Camera Raw settings (or, like a preset, one with `value.settings`).
fn develop_settings(lua: &str) -> Result<BTreeMap<String, Value>, String> {
    let top = amb_presets::lrtemplate::parse(lua).map_err(|e| e.to_string())?;
    let table = top.as_struct().ok_or("not a table")?;
    let inner = table
        .get("value")
        .and_then(Value::as_struct)
        .and_then(|v| v.get("settings"))
        .and_then(Value::as_struct);
    Ok(inner.unwrap_or(table).clone())
}

/// What [`apply`] did.
#[derive(Debug, Default, Serialize)]
pub struct Report {
    pub written: usize,
    pub kept: usize,
    pub missing: usize,
    pub collections: usize,
    pub errors: Vec<String>,
}

/// Write the planned sidecars and collections (into `collections_dir`).
/// A sidecar that has appeared since the plan is kept.
pub fn apply(plan: &Plan, collections_dir: &Path) -> Report {
    let mut report = Report::default();
    for p in &plan.photos {
        match p.action {
            "missing" => report.missing += 1,
            "write" if !p.sidecar.exists() => match sidecar::write_atomic(&p.sidecar, &p.text) {
                Ok(()) => report.written += 1,
                Err(e) => report.errors.push(e.to_string()),
            },
            _ => report.kept += 1,
        }
    }
    if !plan.collections.is_empty()
        && let Err(e) = std::fs::create_dir_all(collections_dir)
    {
        report
            .errors
            .push(format!("{}: {e}", collections_dir.display()));
        return report;
    }
    for c in &plan.collections {
        let done = crate::collections::add(collections_dir, &c.name, &c.photos).and_then(|_| {
            match &c.set {
                Some(set) => crate::collections::move_to_set(collections_dir, &c.name, set),
                None => Ok(crate::collections::Collection::new(&c.name)),
            }
        });
        match done {
            Ok(_) => report.collections += 1,
            Err(e) => report.errors.push(format!("collection {:?}: {e}", c.name)),
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A small catalogue in Lightroom Classic's layout (the tables and
    /// columns we read, as other catalogue readers document them).
    fn catalogue(dir: &Path, photos: &Path) -> PathBuf {
        let path = dir.join("Test Catalog.lrcat");
        let db = Connection::open(&path).unwrap();
        let root = format!("{}/", photos.to_string_lossy().replace('\\', "/"));
        db.execute_batch(
            &r#"
            CREATE TABLE Adobe_variablesTable (id_local INTEGER PRIMARY KEY, id_global, name, type, value);
            INSERT INTO Adobe_variablesTable VALUES (1, 'g1', 'Adobe_DBVersion', NULL, '1300025');
            CREATE TABLE AgLibraryRootFolder (id_local INTEGER PRIMARY KEY, id_global, absolutePath, name, relativePathFromCatalog);
            INSERT INTO AgLibraryRootFolder VALUES (10, 'r', '{root}', 'photos', NULL);
            CREATE TABLE AgLibraryFolder (id_local INTEGER PRIMARY KEY, id_global, parentId, pathFromRoot, rootFolder, visibility);
            INSERT INTO AgLibraryFolder VALUES (20, 'f', NULL, '2024/', 10, NULL);
            CREATE TABLE AgLibraryFile (id_local INTEGER PRIMARY KEY, id_global, baseName, errorMessage, errorTime,
                extension, externalModTime, folder, idx_filename, importHash, lc_idx_filename, lc_idx_filenameExtension,
                md5, modTime, originalFilename, sidecarExtensions);
            INSERT INTO AgLibraryFile (id_local, baseName, extension, folder, idx_filename) VALUES
                (30, 'IMG_0001', 'NEF', 20, 'IMG_0001.NEF'),
                (31, 'beach', 'jpg', 20, 'beach.jpg'),
                (32, 'gone', 'CR2', 20, 'gone.CR2'),
                (33, 'old', 'NEF', 20, 'old.NEF');
            CREATE TABLE Adobe_images (id_local INTEGER PRIMARY KEY, id_global, aspectRatioCache, bitDepth,
                captureTime, colorChannels, colorLabels, colorMode, copyCreationTime, copyName, copyReason,
                developSettingsIDCache, fileFormat, fileHeight, fileWidth, hasMissingSidecars, masterImage,
                orientation, originalCaptureTime, originalRootEntity, panningDistanceH, panningDistanceV,
                pick, positionInFolder, propertiesCache, pyramidIDCache, rating, rootFile, sidecarStatus,
                touchCount, touchTime);
            INSERT INTO Adobe_images (id_local, colorLabels, pick, rating, rootFile, masterImage, copyName) VALUES
                (40, 'Red', 1.0, 4.0, 30, NULL, NULL),
                (41, '', -1.0, NULL, 31, NULL, NULL),
                (42, NULL, 0, 2, 32, NULL, NULL),
                (43, 'Blue', 0, 3, 30, 40, 'Copy 1'),
                (44, NULL, 0, NULL, 33, NULL, NULL);
            CREATE TABLE Adobe_AdditionalMetadata (id_local INTEGER PRIMARY KEY, id_global, additionalInfoSet,
                embeddedXmp, externalXmpIsDirty, image, incrementalWhiteBalance, internalXmpDigest,
                isRawFile, lastSynchronizedHash, lastSynchronizedTimestamp, metadataPresetID,
                metadataVersion, monochrome, xmp);
            INSERT INTO Adobe_AdditionalMetadata (id_local, image, xmp) VALUES (50, 40,
                '<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description rdf:about="" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/" crs:Exposure2012="+9.00"><dc:title><rdf:Alt><rdf:li xml:lang="x-default">Harbour</rdf:li></rdf:Alt></dc:title></rdf:Description></rdf:RDF></x:xmpmeta>');
            CREATE TABLE Adobe_imageDevelopSettings (id_local INTEGER PRIMARY KEY, allowFastRender, beforeSettingsIDCache,
                croppedHeight, croppedWidth, digest, fileHeight, fileWidth, grayscale, hasDevelopAdjustments,
                hasDevelopAdjustmentsEx, historySettingsID, image, processVersion, settingsID, snapshotID,
                text, validatedForVersion, whiteBalance);
            INSERT INTO Adobe_imageDevelopSettings (id_local, image, text) VALUES
                (60, 40, 's = { Exposure2012 = 0.5, Contrast2012 = 20, ProcessVersion = "11.0", ToneCurvePV2012 = { 0, 0, 255, 255, }, }'),
                (61, 41, 's = { Exposure2012 = -1, }'),
                (62, 43, 's = { Exposure2012 = 1.25, ConvertToGrayscale = true, }'),
                (63, 44, 's = { broken');
            CREATE TABLE AgLibraryKeyword (id_local INTEGER PRIMARY KEY, id_global, dateCreated, genealogy,
                imageCountCache, includeOnExport, includeParents, includeSynonyms, keywordType, lastApplied,
                lc_name, name, parent);
            INSERT INTO AgLibraryKeyword (id_local, name) VALUES (70, NULL), (71, 'harbour'), (72, 'boats');
            CREATE TABLE AgLibraryKeywordImage (id_local INTEGER PRIMARY KEY, image, tag);
            INSERT INTO AgLibraryKeywordImage (image, tag) VALUES (40, 71), (40, 72), (41, 72);
            CREATE TABLE AgLibraryFolderStack (id_local INTEGER PRIMARY KEY, id_global, collapsed, text);
            INSERT INTO AgLibraryFolderStack VALUES (80, 's', 1, '');
            CREATE TABLE AgLibraryFolderStackImage (id_local INTEGER PRIMARY KEY, collapsed, image, position, stack);
            INSERT INTO AgLibraryFolderStackImage (image, position, stack) VALUES (40, 1, 80), (44, 2, 80);
            CREATE TABLE AgLibraryCollection (id_local INTEGER PRIMARY KEY, creationId, genealogy, imageCount,
                name, parent, systemOnly);
            INSERT INTO AgLibraryCollection (id_local, creationId, name, parent, systemOnly) VALUES
                (90, 'com.adobe.ag.library.group', 'Travel', NULL, 0),
                (91, 'com.adobe.ag.library.group', 'Italy', 90, 0),
                (92, 'com.adobe.ag.library.collection', 'Selects', 91, 0),
                (93, 'com.adobe.ag.library.collection', 'Selects', NULL, 0),
                (94, 'com.adobe.ag.library.smart_collection', 'Five stars', NULL, 0),
                (95, 'com.adobe.ag.library.collection', 'quick collection', NULL, 1),
                (96, 'com.adobe.ag.webGallery', 'Web', NULL, 0);
            CREATE TABLE AgLibraryCollectionImage (id_local INTEGER PRIMARY KEY, collection, image, pick,
                positionInCollection);
            INSERT INTO AgLibraryCollectionImage (collection, image, positionInCollection) VALUES
                (92, 43, 1), (92, 40, 2), (93, 41, 1), (93, 42, 2);
            "#
            .replace("{root}", &root),
        )
        .unwrap();
        path
    }

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("amb-lrcat-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("photos/2024")).unwrap();
        dir
    }

    #[test]
    fn migrates_a_catalogue() {
        let dir = temp("migrate");
        let photos = dir.join("photos");
        for f in ["IMG_0001.NEF", "beach.jpg", "old.NEF"] {
            std::fs::write(photos.join("2024").join(f), b"x").unwrap();
        }
        // Lightroom already wrote this one's sidecar: it stays.
        std::fs::write(photos.join("2024/old.xmp"), "<mine/>").unwrap();
        let catalog = catalogue(&dir, &photos);
        let before = std::fs::read(&catalog).unwrap();

        let plan = plan(&catalog, &Options::default()).unwrap();
        assert_eq!(plan.version.as_deref(), Some("1300025"));
        assert_eq!(plan.smart_collections, ["Five stars"]);
        let by = |name: &str| {
            plan.photos
                .iter()
                .find(|p| p.sidecar.ends_with(name))
                .unwrap_or_else(|| panic!("{name}"))
        };
        assert_eq!(by("2024/IMG_0001.xmp").action, "write");
        assert_eq!(by("2024/beach.jpg.xmp").action, "write");
        assert_eq!(by("2024/gone.xmp").action, "missing");
        assert_eq!(by("2024/old.xmp").action, "keep");
        assert!(by("2024/old.xmp").problem.is_some());
        let copy = by("2024/IMG_0001.copy-2.xmp");
        assert_eq!(copy.action, "write");
        assert!(copy.photo.ends_with("IMG_0001.NEF"));

        let names: Vec<_> = plan
            .collections
            .iter()
            .map(|c| (c.name.as_str(), c.set.as_deref(), c.photos.len()))
            .collect();
        assert_eq!(
            names,
            [
                ("Selects", Some("Travel / Italy"), 2),
                ("Selects (Lightroom)", None, 2),
            ]
        );
        // In Lightroom's order, the virtual copy first.
        assert!(plan.collections[0].photos[0].ends_with("IMG_0001.copy-2.xmp"));

        let collections = dir.join("collections");
        let report = apply(&plan, &collections);
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert_eq!((report.written, report.kept, report.missing), (3, 1, 1));
        assert_eq!(report.collections, 2);

        let main = std::fs::read_to_string(photos.join("2024/IMG_0001.xmp")).unwrap();
        let settings = amb_presets::xmp::parse_crs(&main).unwrap();
        assert_eq!(settings["Exposure2012"].as_f64(), Some(0.5));
        assert_eq!(settings["Contrast2012"].as_f64(), Some(20.0));
        assert!(settings.contains_key("ToneCurvePV2012"));
        let info = sidecar::read_info(&main);
        assert_eq!(info.rating, Some(4));
        assert_eq!(info.label.as_deref(), Some("Red"));
        assert_eq!(info.flag, Some(Flag::Pick));
        assert_eq!(info.keywords, ["boats", "harbour"]);
        assert_eq!(info.title.as_deref(), Some("Harbour"));
        assert_eq!(
            info.stack,
            Some(StackRef {
                id: "lrcat-80".into(),
                top: true
            })
        );

        let beach = std::fs::read_to_string(photos.join("2024/beach.jpg.xmp")).unwrap();
        let info = sidecar::read_info(&beach);
        assert_eq!(
            (info.rating, info.flag, info.label),
            (None, Some(Flag::Reject), None)
        );
        let copy = std::fs::read_to_string(photos.join("2024/IMG_0001.copy-2.xmp")).unwrap();
        let settings = amb_presets::xmp::parse_crs(&copy).unwrap();
        assert_eq!(settings["Exposure2012"].as_f64(), Some(1.25));
        assert_eq!(sidecar::read_info(&copy).label.as_deref(), Some("Blue"));
        assert_eq!(
            std::fs::read_to_string(photos.join("2024/old.xmp")).unwrap(),
            "<mine/>"
        );

        let selects = crate::collections::find(&collections, "Selects").unwrap().1;
        assert_eq!(selects.set.as_deref(), Some("Travel / Italy"));
        assert!(selects.photos[1].ends_with("IMG_0001.NEF"));

        // Again: everything is there already, and nothing is doubled.
        let again = super::plan(&catalog, &Options::default()).unwrap();
        assert_eq!(again.count("write"), 0);
        let report = apply(&again, &collections);
        assert_eq!(report.written, 0);
        let selects = crate::collections::find(&collections, "Selects").unwrap().1;
        assert_eq!(selects.photos.len(), 2);
        // The catalogue is untouched, and nothing was left next to it.
        assert_eq!(std::fs::read(&catalog).unwrap(), before);
        let mut left: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        left.sort();
        assert_eq!(left, ["Test Catalog.lrcat", "collections", "photos"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn moved_photos_are_found_by_remapping() {
        let dir = temp("remap");
        let photos = dir.join("photos");
        let catalog = catalogue(&dir, Path::new("/Volumes/Old Disk"));
        std::fs::write(photos.join("2024/beach.jpg"), b"x").unwrap();
        let options = Options {
            remap: vec![(
                "/Volumes/Old Disk/".into(),
                format!("{}/", photos.display()),
            )],
        };
        let plan = plan(&catalog, &options).unwrap();
        let beach = plan
            .photos
            .iter()
            .find(|p| p.photo.ends_with("beach.jpg"))
            .unwrap();
        assert_eq!(beach.photo, photos.join("2024/beach.jpg"));
        assert_eq!(beach.action, "write");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn refuses_what_is_not_a_catalogue() {
        let dir = temp("refuse");
        let not = dir.join("x.lrcat");
        std::fs::write(&not, b"not sqlite at all").unwrap();
        assert!(plan(&not, &Options::default()).is_err());
        assert!(plan(&dir.join("none.lrcat"), &Options::default()).is_err());
        let empty = dir.join("empty.lrcat");
        Connection::open(&empty)
            .unwrap()
            .execute_batch("CREATE TABLE t (a);")
            .unwrap();
        let e = plan(&empty, &Options::default()).unwrap_err();
        assert!(e.contains("not a Lightroom Classic catalogue"), "{e}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn damaged_catalogues_are_refused_or_read() {
        let dir = temp("damaged");
        let good = std::fs::read(catalogue(&dir, &dir.join("photos"))).unwrap();
        let path = dir.join("damaged.lrcat");
        let mut seed = 0x9e37_79b9_7f4a_7c15u64;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        for _ in 0..300 {
            let mut bytes = good.clone();
            for _ in 0..1 + next() % 16 {
                let i = (next() % bytes.len() as u64) as usize;
                bytes[i] = next() as u8;
            }
            if next() % 4 == 0 {
                bytes.truncate((next() % bytes.len() as u64) as usize);
            }
            std::fs::write(&path, &bytes).unwrap();
            if let Ok(plan) = plan(&path, &Options::default()) {
                let _ = serde_json::to_string(&plan);
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn uris_escape_paths() {
        let u = uri(Path::new("/a b/c%d#e?.lrcat"));
        // On Windows the path gets the current drive: file:///D:/a%20b/…
        assert!(u.starts_with("file:///"), "{u}");
        assert!(u.ends_with("/a%20b/c%25d%23e%3F.lrcat?immutable=1"), "{u}");
        if cfg!(windows) {
            let u = uri(Path::new(r"C:\a b\x.lrcat"));
            assert_eq!(u, "file:///C:/a%20b/x.lrcat?immutable=1");
        } else {
            assert_eq!(u, "file:///a%20b/c%25d%23e%3F.lrcat?immutable=1");
        }
    }
}
