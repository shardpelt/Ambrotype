// SPDX-License-Identifier: GPL-3.0-or-later
//! Lightroom Classic catalogues opened in place: no import, nothing
//! written. The library shows a catalogue's folders and collections, and
//! its photos with their Lightroom edits, ratings, labels, flags and
//! keywords, read from the catalogue (`lrcat`) through
//! [`amb_presets::linked`]: where a photo has no sidecar, or one older than
//! the catalogue's last change, the catalogue's copy is what is read.
//!
//! It stays in sync both ways, the last change winning: editing a photo
//! here writes its sidecar as usual (starting from the catalogue's edit,
//! with its metadata), and a later change in Lightroom brings the
//! catalogue's back. The catalogue is read again when its file changes
//! (looked at no more than every two seconds), and never written.
//!
//! The list is the user's, beside the library folders:
//!
//! ```json
//! { "format": "ambrotype-lightroom-catalogues/1", "catalogues": ["/Users/me/Pictures/Lightroom/Lightroom Catalog.lrcat"] }
//! ```
//!
//! Virtual copies aren't shown in place (they are files of ours, so they
//! need From Lightroom…), and smart collections are only named.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

pub const FORMAT: &str = "ambrotype-lightroom-catalogues/1";

#[derive(Debug, Default, Serialize, Deserialize)]
struct File {
    format: String,
    catalogues: Vec<PathBuf>,
}

/// Where the editor keeps the list: its config folder.
pub fn default_path() -> Option<PathBuf> {
    Some(
        crate::collections::editor_dir()?
            .parent()?
            .join("lightroom-catalogues.json"),
    )
}

/// The catalogues opened in place, as listed in `file`.
pub fn load(file: &Path) -> Vec<PathBuf> {
    std::fs::read_to_string(file)
        .ok()
        .and_then(|t| serde_json::from_str::<File>(&t).ok())
        .map(|f| f.catalogues)
        .unwrap_or_default()
}

fn save(file: &Path, catalogues: &[PathBuf]) -> Result<(), String> {
    let text = serde_json::to_string_pretty(&File {
        format: FORMAT.into(),
        catalogues: catalogues.to_vec(),
    })
    .map_err(|e| e.to_string())?;
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let tmp = file.with_extension("json.tmp");
    std::fs::write(&tmp, text).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, file).map_err(|e| format!("{}: {e}", file.display()))
}

/// A folder of a catalogue's photos, with how many it has there.
#[derive(Debug, Clone, Serialize)]
pub struct Folder {
    pub path: PathBuf,
    pub photos: usize,
}

/// A collection of a catalogue's, by its photos' paths.
#[derive(Debug, Clone, Serialize)]
pub struct Collection {
    pub name: String,
    pub set: Option<String>,
    pub photos: Vec<PathBuf>,
}

/// What a catalogue opened in place holds, as last read.
#[derive(Debug, Clone, Serialize)]
pub struct Opened {
    pub catalogue: PathBuf,
    /// Its file name without `.lrcat`.
    pub name: String,
    pub version: Option<String>,
    /// Photos found where the catalogue says (virtual copies not counted).
    pub photos: usize,
    /// Photos the catalogue lists that aren't there (moved, or a drive
    /// not plugged in).
    pub missing: usize,
    pub folders: Vec<Folder>,
    pub collections: Vec<Collection>,
    pub smart_collections: Vec<String>,
    /// Signs that Lightroom has it open, or that it couldn't be read.
    pub warnings: Vec<String>,
    /// The catalogue file's modified time (nanoseconds) when read.
    #[serde(skip)]
    modified: i64,
    /// Each photo's sidecar text, by its sidecar's path.
    #[serde(skip)]
    sidecars: HashMap<PathBuf, String>,
}

/// Read `catalogue` for showing in place.
pub fn open(catalogue: &Path) -> Result<Opened, String> {
    let modified = amb_presets::linked::file_stamp(catalogue)
        .ok_or_else(|| format!("{}: no such file", catalogue.display()))?
        .1;
    let plan = crate::lrcat::plan(catalogue, &Default::default())?;
    let mut sidecars = HashMap::new();
    let mut folders: BTreeMap<PathBuf, usize> = BTreeMap::new();
    let (mut photos, mut missing) = (0, 0);
    for p in plan.photos {
        // Virtual copies are sidecars of ours: not shown in place.
        if p.sidecar != amb_engine::sidecar_path(&p.photo) {
            continue;
        }
        if p.action == "missing" {
            missing += 1;
            continue;
        }
        photos += 1;
        if let Some(dir) = p.photo.parent() {
            *folders.entry(dir.to_path_buf()).or_default() += 1;
        }
        sidecars.insert(p.sidecar, p.text);
    }
    let name = catalogue
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    Ok(Opened {
        catalogue: catalogue.to_path_buf(),
        name,
        version: plan.version,
        photos,
        missing,
        folders: folders
            .into_iter()
            .map(|(path, photos)| Folder { path, photos })
            .collect(),
        collections: plan
            .collections
            .into_iter()
            .map(|c| Collection {
                name: c.name,
                set: c.set,
                photos: c.photos.into_iter().filter(|p| p.is_file()).collect(),
            })
            .collect(),
        smart_collections: plan.smart_collections,
        warnings: plan.warnings,
        modified,
        sidecars,
    })
}

/// How often a catalogue's file is looked at for changes, at most.
const LOOK: Duration = Duration::from_secs(2);

struct Kept {
    looked: Instant,
    opened: Result<Arc<Opened>, String>,
}

/// The catalogues opened in place, read when first asked and again when
/// their files change.
pub struct Catalogues {
    file: PathBuf,
    kept: Mutex<HashMap<PathBuf, Kept>>,
    /// The list, as last read from `file`.
    list: Mutex<(Option<Instant>, Vec<PathBuf>)>,
}

impl Catalogues {
    /// The catalogues listed in `file` (see [`default_path`]).
    pub fn new(file: PathBuf) -> Arc<Catalogues> {
        Arc::new(Catalogues {
            file,
            kept: Mutex::new(HashMap::new()),
            list: Mutex::new((None, Vec::new())),
        })
    }

    /// Open `catalogue` in place (it must be one Lightroom can read).
    pub fn add(&self, catalogue: &Path) -> Result<Arc<Opened>, String> {
        let catalogue =
            crate::lrcat::full_path(catalogue).unwrap_or_else(|_| catalogue.to_path_buf());
        let opened = Arc::new(open(&catalogue)?);
        let mut list = load(&self.file);
        if !list.contains(&catalogue) {
            list.push(catalogue.clone());
            save(&self.file, &list)?;
            self.changed();
        }
        self.kept.lock().unwrap_or_else(|e| e.into_inner()).insert(
            catalogue,
            Kept {
                looked: Instant::now(),
                opened: Ok(opened.clone()),
            },
        );
        Ok(opened)
    }

    /// Stop showing `catalogue`; its photos' sidecars stay as they are.
    pub fn remove(&self, catalogue: &Path) -> Result<(), String> {
        let full = crate::lrcat::full_path(catalogue).unwrap_or_else(|_| catalogue.to_path_buf());
        let mut list = load(&self.file);
        list.retain(|c| c != catalogue && *c != full);
        save(&self.file, &list)?;
        self.changed();
        let mut kept = self.kept.lock().unwrap_or_else(|e| e.into_inner());
        kept.remove(catalogue);
        kept.remove(&full);
        Ok(())
    }

    /// The catalogues listed (the file read again now and then).
    pub fn listed(&self) -> Vec<PathBuf> {
        let mut list = self.list.lock().unwrap_or_else(|e| e.into_inner());
        if list.0.is_none_or(|t| t.elapsed() >= LOOK) {
            *list = (Some(Instant::now()), load(&self.file));
        }
        list.1.clone()
    }

    fn changed(&self) {
        self.list.lock().unwrap_or_else(|e| e.into_inner()).0 = None;
    }

    /// Each catalogue listed, as read now (or why it couldn't be).
    pub fn all(&self) -> Vec<(PathBuf, Result<Arc<Opened>, String>)> {
        self.listed()
            .into_iter()
            .map(|c| {
                let opened = self.get(&c);
                (c, opened)
            })
            .collect()
    }

    /// One catalogue, read again if its file changed.
    pub fn get(&self, catalogue: &Path) -> Result<Arc<Opened>, String> {
        let mut kept = self.kept.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(k) = kept.get_mut(catalogue) {
            if k.looked.elapsed() < LOOK {
                return k.opened.clone();
            }
            k.looked = Instant::now();
            let now = amb_presets::linked::file_stamp(catalogue).map(|s| s.1);
            if matches!((&k.opened, now), (Ok(o), Some(t)) if o.modified == t) {
                return k.opened.clone();
            }
        }
        let opened = open(catalogue).map(Arc::new);
        kept.insert(
            catalogue.to_path_buf(),
            Kept {
                looked: Instant::now(),
                opened: opened.clone(),
            },
        );
        opened
    }

    /// What the catalogues know of the photo whose sidecar is `sidecar`.
    pub fn lookup(&self, sidecar: &Path) -> Option<amb_presets::linked::Linked> {
        for (_, opened) in self.all() {
            let Ok(o) = opened else { continue };
            if let Some(xmp) = o.sidecars.get(sidecar) {
                return Some(amb_presets::linked::Linked {
                    xmp: xmp.clone(),
                    modified: o.modified,
                });
            }
        }
        None
    }

    /// Make these the catalogues every sidecar read asks
    /// ([`amb_presets::linked::set_source`]).
    pub fn install(self: &Arc<Self>) {
        let me = self.clone();
        amb_presets::linked::set_source(Some(Arc::new(move |sidecar: &Path| me.lookup(sidecar))));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_catalogue_opened_in_place_shows_its_edits_until_one_is_made_here() {
        let dir = std::env::temp_dir().join(format!("amb-lightroom-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let photos = dir.join("photos");
        std::fs::create_dir_all(photos.join("2024")).unwrap();
        for f in ["IMG_0001.NEF", "beach.jpg", "old.NEF"] {
            std::fs::write(photos.join("2024").join(f), b"x").unwrap();
        }
        let catalog = crate::lrcat::tests::catalogue(&dir, &photos);
        let before = std::fs::read(&catalog).unwrap();
        let list = Catalogues::new(dir.join("lightroom-catalogues.json"));
        let opened = list.add(&catalog).unwrap();
        // Three photos there, one gone; the copy isn't shown.
        assert_eq!((opened.photos, opened.missing), (3, 1));
        assert_eq!(opened.folders.len(), 1);
        assert_eq!(opened.folders[0].photos, 3);
        let selects: Vec<_> = opened
            .collections
            .iter()
            .filter(|c| c.name.starts_with("Selects"))
            .collect();
        assert_eq!(selects.len(), 2);
        assert_eq!(load(&dir.join("lightroom-catalogues.json")).len(), 1);

        list.install();
        let nef = photos.join("2024/IMG_0001.NEF");
        // Read through the sidecar hook, with no sidecar written.
        let text = amb_engine::sidecar_text(&nef).expect("the catalogue's edit");
        let info = amb_protocol::sidecar::read_info(&text);
        assert_eq!(info.rating, Some(4));
        let state = amb_protocol::DevelopState::load(&amb_engine::sidecar_path(&nef)).unwrap();
        assert_eq!(
            state
                .settings()
                .get("Exposure2012")
                .and_then(|v| v.as_f64()),
            Some(0.5)
        );
        assert!(!amb_engine::sidecar_path(&nef).exists());

        // An edit here goes through the protocol into a sidecar, keeping
        // the catalogue's edit and metadata; from then on the sidecar wins.
        let sidecar = amb_engine::sidecar_path(&nef);
        amb_protocol::edit::run_compact(&amb_protocol::edit::EditRequest {
            sidecar: &sidecar,
            commands: "con=+40",
            out: None,
            dry_run: false,
        })
        .unwrap();
        let state = amb_protocol::DevelopState::load(&sidecar).unwrap();
        assert_eq!(
            state
                .settings()
                .get("Exposure2012")
                .and_then(|v| v.as_f64()),
            Some(0.5)
        );
        assert_eq!(
            state
                .settings()
                .get("Contrast2012")
                .and_then(|v| v.as_f64()),
            Some(40.0)
        );
        let ours = amb_engine::sidecar_text(&nef).unwrap();
        assert_eq!(amb_protocol::sidecar::read_info(&ours).rating, Some(4));
        assert!(ours.contains("Harbour"), "the catalogue's title kept");

        // The catalogue is never written.
        assert_eq!(std::fs::read(&catalog).unwrap(), before);
        amb_presets::linked::set_source(None);
        list.remove(&catalog).unwrap();
        assert!(load(&dir.join("lightroom-catalogues.json")).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
