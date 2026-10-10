// SPDX-License-Identifier: GPL-3.0-or-later
//! Collections: named lists of photos from any folders, like Lightroom's
//! collections. With no catalogue, each is a small JSON file in a folder
//! (the editor uses its config folder):
//!
//! ```json
//! { "format": "ambrotype-collection/1", "name": "Portfolio", "photos": ["/photos/a.nef"] }
//! ```
//!
//! Photos are kept by full path, in the order they were added. A photo
//! that has been moved or deleted stays listed until it is removed, and
//! [`Collection::missing`] reports it.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const FORMAT: &str = "ambrotype-collection/1";

/// A collection as stored.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Collection {
    pub format: String,
    pub name: String,
    pub photos: Vec<PathBuf>,
}

/// A collection in a listing.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Summary {
    pub name: String,
    pub count: usize,
}

impl Collection {
    pub fn new(name: &str) -> Collection {
        Collection {
            format: FORMAT.into(),
            name: name.trim().to_string(),
            photos: Vec::new(),
        }
    }

    /// The photos that are no longer where they were added from.
    pub fn missing(&self) -> Vec<&Path> {
        self.photos
            .iter()
            .filter(|p| !p.is_file())
            .map(PathBuf::as_path)
            .collect()
    }
}

/// The file name for a collection name: letters, digits, `-` and `_`.
fn file_for(dir: &Path, name: &str) -> PathBuf {
    let slug: String = name
        .trim()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c.to_lowercase().next().unwrap_or(c)
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    dir.join(format!(
        "{}.json",
        if slug.is_empty() { "collection" } else { &slug }
    ))
}

fn check_name(name: &str) -> Result<&str, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("a collection needs a name".into());
    }
    if name.chars().count() > 100 {
        return Err("collection names are at most 100 characters".into());
    }
    Ok(name)
}

/// Every collection in `dir`, by name.
pub fn all(dir: &Path) -> Vec<Collection> {
    let mut out: Vec<Collection> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .filter_map(|p| std::fs::read_to_string(p).ok())
        .filter_map(|t| serde_json::from_str::<Collection>(&t).ok())
        .filter(|c| c.format == FORMAT)
        .collect();
    out.sort_by_key(|c| c.name.to_lowercase());
    out
}

/// The collections in `dir` and how many photos each has.
pub fn list(dir: &Path) -> Vec<Summary> {
    all(dir)
        .into_iter()
        .map(|c| Summary {
            name: c.name,
            count: c.photos.len(),
        })
        .collect()
}

/// The collection called `name` (any case), if there is one.
pub fn find(dir: &Path, name: &str) -> Option<(PathBuf, Collection)> {
    let want = name.trim().to_lowercase();
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .find_map(|p| {
            let c: Collection = serde_json::from_str(&std::fs::read_to_string(&p).ok()?).ok()?;
            (c.format == FORMAT && c.name.to_lowercase() == want).then_some((p, c))
        })
}

fn write(path: &Path, c: &Collection) -> Result<(), String> {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
    }
    let text = serde_json::to_string_pretty(c).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, text).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))
}

/// A free file for a new collection (another name may share the slug).
fn new_file(dir: &Path, name: &str) -> PathBuf {
    let base = file_for(dir, name);
    let stem = base
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    (1..)
        .map(|n| {
            if n == 1 {
                base.clone()
            } else {
                dir.join(format!("{stem}-{n}.json"))
            }
        })
        .find(|p| !p.exists())
        .expect("some name is free")
}

/// Add `photos` to the collection `name`, making it if needed. Photos
/// already in it stay where they are. Returns the collection.
pub fn add(dir: &Path, name: &str, photos: &[PathBuf]) -> Result<Collection, String> {
    let name = check_name(name)?;
    let (path, mut c) =
        find(dir, name).unwrap_or_else(|| (new_file(dir, name), Collection::new(name)));
    for p in photos {
        if !c.photos.contains(p) {
            c.photos.push(p.clone());
        }
    }
    write(&path, &c)?;
    Ok(c)
}

/// Take `photos` out of the collection `name`.
pub fn remove(dir: &Path, name: &str, photos: &[PathBuf]) -> Result<Collection, String> {
    let (path, mut c) = find(dir, name).ok_or_else(|| format!("no collection {name:?}"))?;
    c.photos.retain(|p| !photos.contains(p));
    write(&path, &c)?;
    Ok(c)
}

/// Rename a collection (its file keeps its name).
pub fn rename(dir: &Path, from: &str, to: &str) -> Result<Collection, String> {
    let to = check_name(to)?;
    if from.trim().to_lowercase() != to.to_lowercase() && find(dir, to).is_some() {
        return Err(format!("there is already a collection {to:?}"));
    }
    let (path, mut c) = find(dir, from).ok_or_else(|| format!("no collection {from:?}"))?;
    c.name = to.to_string();
    write(&path, &c)?;
    Ok(c)
}

/// Delete a collection (not its photos).
pub fn delete(dir: &Path, name: &str) -> Result<(), String> {
    let (path, _) = find(dir, name).ok_or_else(|| format!("no collection {name:?}"))?;
    std::fs::remove_file(&path).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("amb-coll-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn add_remove_rename_delete() {
        let dir = temp("crud");
        let (a, b) = (
            PathBuf::from("/photos/a.nef"),
            PathBuf::from("/other/b.nef"),
        );
        assert!(list(&dir).is_empty());
        add(&dir, "Best of 2026", std::slice::from_ref(&a)).unwrap();
        let c = add(&dir, "best of 2026", &[b.clone(), a.clone()]).unwrap();
        assert_eq!(c.photos, [a.clone(), b.clone()]);
        assert_eq!(c.name, "Best of 2026");
        assert!(dir.join("best-of-2026.json").is_file());
        // A name with the same slug gets its own file.
        add(&dir, "Best of: 2026", std::slice::from_ref(&b)).unwrap();
        assert_eq!(
            list(&dir),
            [
                Summary {
                    name: "Best of 2026".into(),
                    count: 2
                },
                Summary {
                    name: "Best of: 2026".into(),
                    count: 1
                },
            ]
        );
        assert_eq!(c.missing().len(), 2);
        let c = remove(&dir, "Best of 2026", std::slice::from_ref(&a)).unwrap();
        assert_eq!(c.photos, std::slice::from_ref(&b));
        assert!(
            rename(&dir, "Best of 2026", "best of: 2026")
                .unwrap_err()
                .contains("already")
        );
        rename(&dir, "Best of 2026", "Portfolio").unwrap();
        assert_eq!(find(&dir, "portfolio").unwrap().1.photos, [b]);
        delete(&dir, "Portfolio").unwrap();
        assert!(find(&dir, "Portfolio").is_none());
        assert!(add(&dir, "  ", &[]).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
