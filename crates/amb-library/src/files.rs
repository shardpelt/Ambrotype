// SPDX-License-Identifier: GPL-3.0-or-later
//! File care: moving, renaming and deleting photos together with what
//! belongs to them, so a raw never loses its sidecar, its JPEG twin or its
//! virtual copies.
//!
//! A photo's files are the files in its folder with its photo name
//! (`IMG_1.NEF`, `IMG_1.JPG`, `IMG_1.xmp`, `IMG_1.copy-2.xmp`, and for a
//! JPEG `IMG_1.jpg.xmp`), as Import groups them. A move or rename is
//! planned first, so the user sees every new name before anything happens;
//! nothing is overwritten (a taken name gets `-2`, `-3`, …). Deleting moves
//! the files to the system's trash. Photos moved outside the app can be
//! found again by name and size.

use crate::import;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// A photo's files: its own first, then the others in its folder with its
/// photo name. A virtual copy stands for its photo.
pub fn companions(photo: &Path) -> Vec<PathBuf> {
    let main = amb_engine::source_path(photo);
    let Some(dir) = main.parent() else {
        return vec![main];
    };
    let stem = import::split_name(&main).0.to_lowercase();
    let mut out = vec![main.clone()];
    let mut others: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p != &main && p.is_file() && import::split_name(p).0.to_lowercase() == stem)
        .filter(|p| {
            !p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with('.'))
        })
        .collect();
    others.sort();
    out.extend(others);
    out
}

/// Where photos go.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Target {
    /// Into a folder, names kept.
    Move { folder: PathBuf },
    /// New names from a template (Import's tokens), in their folders.
    Rename { template: String },
    /// Into dated folders under `root` from a folder template such as
    /// `{yyyy}/{yyyy}-{mm}-{dd}`, names kept.
    Dated { root: PathBuf, template: String },
}

/// One photo's planned move: each of its files, old and new.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Planned {
    /// The photo as listed (a virtual copy's sidecar, or its file).
    pub photo: PathBuf,
    /// Where the photo as listed ends up.
    pub to: PathBuf,
    pub files: Vec<(PathBuf, PathBuf)>,
}

/// What applying a plan did.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Done {
    pub photos: usize,
    pub files: usize,
    pub errors: Vec<String>,
    /// Every file moved, old and new: to follow them in the catalogue and
    /// collections, and to undo.
    pub moves: Vec<(PathBuf, PathBuf)>,
}

/// Plan moving `photos` to `target`. Photos already where they'd go are
/// left out; a name taken by another file gets `-2`, `-3`, …
pub fn plan(photos: &[PathBuf], target: &Target) -> Result<Vec<Planned>, String> {
    if let Target::Rename { template } | Target::Dated { template, .. } = target {
        import::check_template(template)?;
    }
    // Each photo once (a photo and its virtual copies move together), in
    // capture order for {seq}.
    let mut seen = HashSet::new();
    let mut groups: Vec<(PathBuf, Vec<PathBuf>)> = Vec::new();
    for p in photos {
        let main = amb_engine::source_path(p);
        if seen.insert(main.clone()) {
            groups.push((p.clone(), companions(p)));
        }
    }
    let captured = |p: &Path| {
        amb_engine::raw::read_metadata(&amb_engine::source_path(p))
            .ok()
            .and_then(|(_, m)| m.date_time_original)
            .unwrap_or_default()
    };
    if matches!(target, Target::Rename { .. }) {
        groups.sort_by_cached_key(|(p, _)| (captured(p), p.clone()));
    }
    let moving: HashSet<PathBuf> = groups.iter().flat_map(|(_, f)| f.iter().cloned()).collect();
    // Photo names taken in each folder (by files staying, or planned).
    let mut taken: HashMap<PathBuf, HashSet<String>> = HashMap::new();
    let mut out = Vec::new();
    for (seq, (photo, files)) in groups.into_iter().enumerate() {
        let main = &files[0];
        let (stem, _) = import::split_name(main);
        let (dir, new_stem) = match target {
            Target::Move { folder } => (folder.clone(), stem.clone()),
            Target::Rename { template } => (
                main.parent().unwrap_or(Path::new("")).to_path_buf(),
                import::file_name(template, main, seq as u32 + 1)?,
            ),
            Target::Dated { root, template } => {
                let mut dir = root.clone();
                for part in template.split('/').filter(|p| !p.trim().is_empty()) {
                    dir.push(import::file_name(part, main, seq as u32 + 1)?);
                }
                (dir, stem.clone())
            }
        };
        let names = taken.entry(dir.clone()).or_insert_with(|| {
            std::fs::read_dir(&dir)
                .into_iter()
                .flatten()
                .flatten()
                .map(|e| e.path())
                .filter(|p| !moving.contains(p))
                .map(|p| import::split_name(&p).0.to_lowercase())
                .collect()
        });
        let mut chosen = new_stem.clone();
        let mut n = 2;
        while names.contains(&chosen.to_lowercase()) {
            chosen = format!("{new_stem}-{n}");
            n += 1;
        }
        names.insert(chosen.to_lowercase());
        let files: Vec<(PathBuf, PathBuf)> = files
            .iter()
            .map(|f| {
                let rest = import::split_name(f).1;
                (f.clone(), dir.join(format!("{chosen}{rest}")))
            })
            .collect();
        if files.iter().all(|(a, b)| a == b) {
            continue;
        }
        let to = files
            .iter()
            .find(|(a, _)| a == &photo)
            .map(|(_, b)| b.clone())
            .unwrap_or_else(|| files[0].1.clone());
        out.push(Planned { photo, to, files });
    }
    Ok(out)
}

/// Move the files as planned. A photo whose files can't all be moved is
/// put back as it was.
pub fn apply(plan: &[Planned]) -> Done {
    let mut done = Done::default();
    for p in plan {
        let mut moved: Vec<(PathBuf, PathBuf)> = Vec::new();
        let mut failed = None;
        for (from, to) in &p.files {
            match move_file(from, to) {
                Ok(()) => moved.push((from.clone(), to.clone())),
                Err(e) => {
                    failed = Some(e);
                    break;
                }
            }
        }
        if let Some(e) = failed {
            for (from, to) in moved.iter().rev() {
                let _ = move_file(to, from);
            }
            done.errors.push(e);
            continue;
        }
        done.photos += 1;
        done.files += moved.len();
        done.moves.extend(moved);
    }
    done
}

/// Move one file, never over another: a rename, or across drives a checked
/// copy and then the original removed.
fn move_file(from: &Path, to: &Path) -> Result<(), String> {
    if to.exists() {
        return Err(format!("{}: already exists", to.display()));
    }
    if let Some(dir) = to.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    if std::fs::rename(from, to).is_ok() {
        return Ok(());
    }
    import::copy_file(from, to)?;
    std::fs::remove_file(from).map_err(|e| format!("{}: {e}", from.display()))
}

/// Move `photos` and all their files to the system's trash. Returns how
/// many files went.
pub fn trash(photos: &[PathBuf]) -> Result<usize, String> {
    let mut files: Vec<PathBuf> = Vec::new();
    let mut seen = HashSet::new();
    for p in photos {
        // A virtual copy alone goes on its own: its photo stays.
        let mine = if amb_engine::virtual_copy_of(p).is_some() {
            vec![p.clone()]
        } else {
            companions(p)
        };
        for f in mine {
            if seen.insert(f.clone()) {
                files.push(f);
            }
        }
    }
    files.retain(|f| f.exists());
    if files.is_empty() {
        return Ok(0);
    }
    trash_files(&files)?;
    Ok(files.len())
}

#[cfg(target_os = "linux")]
fn trash_files(files: &[PathBuf]) -> Result<(), String> {
    let home = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .ok_or("no home folder for the trash")?
        .join("Trash");
    for f in files {
        linux_trash(f, &home)?;
    }
    Ok(())
}

/// The freedesktop.org trash: the home trash on the home drive, otherwise
/// `.Trash-<uid>` at the top of the file's drive.
#[cfg(target_os = "linux")]
fn linux_trash(file: &Path, home_trash: &Path) -> Result<(), String> {
    use std::os::unix::fs::MetadataExt;
    let err = |e: std::io::Error| format!("{}: {e}", file.display());
    let file = std::fs::canonicalize(file).map_err(err)?;
    let meta = std::fs::metadata(&file).map_err(err)?;
    let home_dev = std::fs::create_dir_all(home_trash)
        .and_then(|_| std::fs::metadata(home_trash))
        .map(|m| m.dev());
    let trash = if home_dev.as_ref().is_ok_and(|d| *d == meta.dev()) {
        home_trash.to_path_buf()
    } else {
        // The top of the file's drive: the last parent on the same device.
        let mut top = file.parent().unwrap_or(Path::new("/")).to_path_buf();
        while let Some(up) = top.parent() {
            if std::fs::metadata(up).map(|m| m.dev()).ok() != Some(meta.dev()) {
                break;
            }
            top = up.to_path_buf();
        }
        let uid = std::fs::metadata(std::env::var_os("HOME").unwrap_or_default())
            .map(|m| m.uid())
            .unwrap_or(0);
        top.join(format!(".Trash-{uid}"))
    };
    let (files_dir, info_dir) = (trash.join("files"), trash.join("info"));
    std::fs::create_dir_all(&files_dir).map_err(err)?;
    std::fs::create_dir_all(&info_dir).map_err(err)?;
    let name = file
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let mut pick = name.clone();
    let mut n = 2;
    while files_dir.join(&pick).exists() || info_dir.join(format!("{pick}.trashinfo")).exists() {
        let (stem, ext) = match name.rsplit_once('.') {
            Some((s, e)) => (s.to_string(), format!(".{e}")),
            None => (name.clone(), String::new()),
        };
        pick = format!("{stem}.{n}{ext}");
        n += 1;
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let info = format!(
        "[Trash Info]\nPath={}\nDeletionDate={}\n",
        percent_path(&file),
        iso_time(now)
    );
    std::fs::write(info_dir.join(format!("{pick}.trashinfo")), info).map_err(err)?;
    std::fs::rename(&file, files_dir.join(&pick)).map_err(|e| {
        let _ = std::fs::remove_file(info_dir.join(format!("{pick}.trashinfo")));
        err(e)
    })
}

#[cfg(target_os = "linux")]
fn percent_path(p: &Path) -> String {
    use std::os::unix::ffi::OsStrExt;
    p.as_os_str()
        .as_bytes()
        .iter()
        .map(|&b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// Seconds since 1970 as "YYYY-MM-DDThh:mm:ss" (UTC).
#[cfg(target_os = "linux")]
fn iso_time(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// The Finder's trash, so "Put Back" works.
#[cfg(target_os = "macos")]
fn trash_files(files: &[PathBuf]) -> Result<(), String> {
    let list: Vec<String> = files
        .iter()
        .map(|f| {
            let s = f
                .display()
                .to_string()
                .replace('\\', "\\\\")
                .replace('"', "\\\"");
            format!("POSIX file \"{s}\"")
        })
        .collect();
    let script = format!(
        "tell application \"Finder\" to delete {{{}}}",
        list.join(", ")
    );
    let out = std::process::Command::new("osascript")
        .args(["-e", &script])
        .output()
        .map_err(|e| format!("osascript: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// The Recycle Bin.
#[cfg(target_os = "windows")]
fn trash_files(files: &[PathBuf]) -> Result<(), String> {
    let list: Vec<String> = files
        .iter()
        .map(|f| format!("'{}'", f.display().to_string().replace('\'', "''")))
        .collect();
    let script = format!(
        "Add-Type -AssemblyName Microsoft.VisualBasic; foreach ($f in @({})) {{ [Microsoft.VisualBasic.FileIO.FileSystem]::DeleteFile($f, 'OnlyErrorDialogs', 'SendToRecycleBin') }}",
        list.join(", ")
    );
    let out = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .map_err(|e| format!("powershell: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn trash_files(_files: &[PathBuf]) -> Result<(), String> {
    Err("there is no trash on this system".into())
}

/// Missing photos (path, size they had) found in `folder` or its
/// subfolders by name and size: (old path, new path). A name found more
/// than once with that size is left alone.
pub fn find_moved(missing: &[(PathBuf, i64)], folder: &Path) -> Vec<(PathBuf, PathBuf)> {
    let mut found: HashMap<(String, u64), Vec<PathBuf>> = HashMap::new();
    let mut stack = vec![folder.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let p = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            match entry.file_type() {
                Ok(t) if t.is_dir() => stack.push(p),
                Ok(t) if t.is_file() => {
                    let len = entry.metadata().map(|m| m.len()).unwrap_or(0);
                    found.entry((name.to_lowercase(), len)).or_default().push(p);
                }
                _ => {}
            }
        }
    }
    missing
        .iter()
        .filter_map(|(old, size)| {
            let name = old.file_name()?.to_string_lossy().to_lowercase();
            match found.get(&(name, *size as u64))?.as_slice() {
                [one] => Some((old.clone(), one.clone())),
                _ => None,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("amb-files-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn touch(dir: &Path, names: &[&str]) {
        for n in names {
            std::fs::write(dir.join(n), n.as_bytes()).unwrap();
        }
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        v.sort();
        v
    }

    #[test]
    fn a_photo_moves_with_its_sidecar_jpeg_and_copies() {
        let d = folder("move");
        touch(
            &d,
            &[
                "IMG_1.NEF",
                "IMG_1.JPG",
                "IMG_1.xmp",
                "IMG_1.copy-2.xmp",
                "IMG_10.NEF",
                "b.jpg",
                "b.jpg.xmp",
            ],
        );
        assert_eq!(companions(&d.join("IMG_1.NEF")).len(), 4);
        // A virtual copy stands for its photo.
        assert_eq!(
            companions(&d.join("IMG_1.copy-2.xmp"))[0],
            d.join("IMG_1.NEF")
        );
        let to = d.join("sorted");
        touch(&d, &[]);
        std::fs::create_dir_all(&to).unwrap();
        touch(&to, &["IMG_1.NEF"]);
        let photos = [
            d.join("IMG_1.NEF"),
            d.join("IMG_1.copy-2.xmp"),
            d.join("b.jpg"),
        ];
        let planned = plan(&photos, &Target::Move { folder: to.clone() }).unwrap();
        assert_eq!(planned.len(), 2, "a photo and its copy move once");
        // The name is taken there: -2.
        assert_eq!(planned[0].to, to.join("IMG_1-2.NEF"));
        let done = apply(&planned);
        assert!(done.errors.is_empty(), "{:?}", done.errors);
        assert_eq!(done.files, 6);
        assert_eq!(names(&d), ["IMG_10.NEF", "sorted"]);
        assert_eq!(
            names(&to),
            [
                "IMG_1-2.JPG",
                "IMG_1-2.NEF",
                "IMG_1-2.copy-2.xmp",
                "IMG_1-2.xmp",
                "IMG_1.NEF",
                "b.jpg",
                "b.jpg.xmp"
            ]
        );
        // Undo: the moves backwards.
        let back: Vec<Planned> = vec![Planned {
            photo: PathBuf::new(),
            to: PathBuf::new(),
            files: done
                .moves
                .iter()
                .map(|(a, b)| (b.clone(), a.clone()))
                .collect(),
        }];
        assert!(apply(&back).errors.is_empty());
        assert_eq!(names(&d).len(), 8);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn rename_and_dated_folders_use_imports_tokens() {
        let d = folder("rename");
        touch(&d, &["a.jpg", "a.jpg.xmp", "b.jpg"]);
        let photos = [d.join("a.jpg"), d.join("b.jpg")];
        let renamed = plan(
            &photos,
            &Target::Rename {
                template: "trip-{seq}".into(),
            },
        )
        .unwrap();
        let new: Vec<_> = renamed
            .iter()
            .map(|p| p.to.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(new, ["trip-0001.jpg", "trip-0002.jpg"]);
        assert_eq!(renamed[0].files[1].1, d.join("trip-0001.jpg.xmp"));
        // These files have no capture time: their modified date is used.
        let dated = plan(
            &photos,
            &Target::Dated {
                root: d.clone(),
                template: "{yyyy}/{mm}".into(),
            },
        )
        .unwrap();
        assert_eq!(
            dated[0]
                .to
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .parent()
                .unwrap(),
            d
        );
        assert!(plan_err("{nope}", &photos));
        let _ = std::fs::remove_dir_all(&d);
    }

    fn plan_err(t: &str, photos: &[PathBuf]) -> bool {
        plan(photos, &Target::Rename { template: t.into() }).is_err()
    }

    #[test]
    fn moved_photos_are_found_by_name_and_size() {
        let d = folder("missing");
        std::fs::create_dir_all(d.join("new/deep")).unwrap();
        touch(&d.join("new/deep"), &["IMG_1.NEF", "IMG_2.NEF"]);
        touch(&d.join("new"), &["IMG_2.NEF"]);
        let missing = [
            (d.join("old/IMG_1.NEF"), 9),
            (d.join("old/IMG_2.NEF"), 9),
            (d.join("old/IMG_3.NEF"), 9),
        ];
        let found = find_moved(&missing, &d);
        // IMG_2 is there twice: left alone.
        assert_eq!(
            found,
            [(d.join("old/IMG_1.NEF"), d.join("new/deep/IMG_1.NEF"))]
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn deleting_moves_to_the_trash_with_its_files() {
        let d = folder("trash");
        let data = d.join("data");
        let into = |photo: &Path| {
            for f in companions(photo) {
                linux_trash(&f, &data.join("Trash")).unwrap();
            }
        };
        touch(&d, &["IMG_1.NEF", "IMG_1.xmp", "IMG_2.NEF"]);
        into(&d.join("IMG_1.NEF"));
        assert_eq!(names(&d), ["IMG_2.NEF", "data"]);
        assert_eq!(names(&data.join("Trash/files")), ["IMG_1.NEF", "IMG_1.xmp"]);
        let info = std::fs::read_to_string(data.join("Trash/info/IMG_1.NEF.trashinfo")).unwrap();
        assert!(
            info.contains("Path=/") && info.contains("IMG_1.NEF") && info.contains("DeletionDate="),
            "{info}"
        );
        // The same name again gets a number.
        touch(&d, &["IMG_1.NEF"]);
        into(&d.join("IMG_1.NEF"));
        assert!(data.join("Trash/files/IMG_1.2.NEF").exists());
        let _ = std::fs::remove_dir_all(&d);
    }
}
