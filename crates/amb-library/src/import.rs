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

use std::collections::{BTreeMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use amb_protocol::sidecar;

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
}

impl Default for ImportOptions {
    fn default() -> Self {
        ImportOptions {
            folder: "{yyyy}/{yyyy}-{mm}-{dd}".into(),
            name: "{name}".into(),
            first_number: 1,
            subfolders: true,
            keywords: Vec::new(),
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
}

/// Capture time parts.
#[derive(Debug, Clone, Copy, PartialEq)]
struct When {
    y: i64,
    mo: u32,
    d: u32,
    h: u32,
    mi: u32,
    s: u32,
}

impl When {
    fn text(&self) -> String {
        format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            self.y, self.mo, self.d, self.h, self.mi, self.s
        )
    }

    /// EXIF's "YYYY:MM:DD HH:MM:SS".
    fn from_exif(t: &str) -> Option<When> {
        let n: Vec<i64> = t
            .split(|c: char| !c.is_ascii_digit())
            .filter(|p| !p.is_empty())
            .map(|p| p.parse().ok())
            .collect::<Option<_>>()?;
        let [y, mo, d, h, mi, s] = n.get(..6)?.try_into().ok()?;
        (y > 1900 && (1..=12).contains(&mo) && (1..=31).contains(&d)).then_some(When {
            y,
            mo: mo as u32,
            d: d as u32,
            h: h as u32,
            mi: mi as u32,
            s: s as u32,
        })
    }

    /// Seconds since 1970 (UTC) as a civil date.
    fn from_unix(secs: i64) -> When {
        let days = secs.div_euclid(86_400);
        let rem = secs.rem_euclid(86_400);
        // Howard Hinnant's days-to-civil.
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
        let mo = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
        let y = yoe + era * 400 + i64::from(mo <= 2);
        When {
            y,
            mo,
            d,
            h: (rem / 3600) as u32,
            mi: (rem % 3600 / 60) as u32,
            s: (rem % 60) as u32,
        }
    }
}

/// Check a template: every `{...}` must be a known token.
pub fn check_template(template: &str) -> Result<(), String> {
    let mut rest = template;
    while let Some(i) = rest.find('{') {
        let end = rest[i..]
            .find('}')
            .ok_or_else(|| format!("{template:?}: a {{ without its }}"))?;
        let token = &rest[i..i + end + 1];
        if !TOKENS.iter().any(|(t, _)| *t == token) {
            let known: Vec<&str> = TOKENS.iter().map(|(t, _)| *t).collect();
            return Err(format!(
                "{template:?}: unknown {token}; use {}",
                known.join(" ")
            ));
        }
        rest = &rest[i + end + 1..];
    }
    Ok(())
}

/// A file or folder name part: no separators or characters Windows refuses.
fn clean(part: &str) -> String {
    let s: String = part
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            c if c.is_control() => '-',
            c => c,
        })
        .collect();
    s.trim().trim_matches('.').to_string()
}

fn expand(template: &str, name: &str, when: &When, seq: u32, camera: &str) -> String {
    template
        .replace("{name}", name)
        .replace("{yyyy}", &format!("{:04}", when.y))
        .replace("{yy}", &format!("{:02}", when.y.rem_euclid(100)))
        .replace("{mm}", &format!("{:02}", when.mo))
        .replace("{dd}", &format!("{:02}", when.d))
        .replace("{hh}", &format!("{:02}", when.h))
        .replace("{min}", &format!("{:02}", when.mi))
        .replace("{ss}", &format!("{:02}", when.s))
        .replace("{seq}", &format!("{seq:04}"))
        .replace("{camera}", camera)
}

fn is_photo(path: &Path) -> bool {
    amb_engine::raw::is_raw_path(path) || amb_engine::raster::is_raster(path)
}

fn ext_of(path: &Path) -> String {
    path.extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

fn hidden(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.starts_with('.'))
}

/// The photo files and sidecars under `dir`.
fn scan(dir: &Path, subfolders: bool, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut entries: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if hidden(&p) {
            continue;
        }
        if p.is_dir() {
            if subfolders {
                scan(&p, true, out)?;
            }
        } else if is_photo(&p) || ext_of(&p) == "xmp" {
            out.push(p);
        }
    }
    Ok(())
}

/// A photo and its companions in the source.
struct Group {
    main: PathBuf,
    others: Vec<PathBuf>,
}

/// A file's photo name and the rest of its name: `IMG_1` and `.CR2` for
/// `IMG_1.CR2`, `.xmp` for its sidecar, `.copy-2.xmp` for a virtual copy,
/// `.jpg.xmp` for a JPEG's sidecar `IMG_1.jpg.xmp`.
fn split_name(f: &Path) -> (String, String) {
    let name = f
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let mut base = match name.len().checked_sub(4) {
        Some(i) if name.is_char_boundary(i) && name[i..].eq_ignore_ascii_case(".xmp") => {
            name[..i].to_string()
        }
        _ => {
            let stem = f.file_stem().unwrap_or_default().to_string_lossy();
            return (stem.to_string(), name[stem.len()..].to_string());
        }
    };
    if let Some((b, n)) = base.rsplit_once(".copy-") {
        if !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()) {
            base = b.to_string();
        }
    }
    if amb_engine::raster::is_raster(Path::new(&base)) {
        if let Some((b, _)) = base.rsplit_once('.') {
            base = b.to_string();
        }
    }
    let rest = name[base.len()..].to_string();
    (base, rest)
}

/// Group files by folder and photo name: the raw (or else the first photo)
/// leads; JPEGs, sidecars and virtual copies follow it.
fn group(files: Vec<PathBuf>) -> Vec<Group> {
    let mut by_stem: BTreeMap<(PathBuf, String), Vec<PathBuf>> = BTreeMap::new();
    for f in files {
        let dir = f.parent().map(Path::to_path_buf).unwrap_or_default();
        let stem = split_name(&f).0.to_lowercase();
        by_stem.entry((dir, stem)).or_default().push(f);
    }
    by_stem
        .into_values()
        .filter_map(|mut files| {
            let lead = files
                .iter()
                .position(|f| amb_engine::raw::is_raw_path(f))
                .or_else(|| files.iter().position(|f| is_photo(f)))?;
            let main = files.remove(lead);
            Some(Group {
                main,
                others: files,
            })
        })
        .collect()
}

fn capture_time(path: &Path) -> (When, String) {
    let meta = amb_engine::raw::read_metadata(path).ok();
    let camera = meta
        .as_ref()
        .map(|(_, m)| m.model.trim().to_string())
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| "camera".into());
    let exif = meta
        .and_then(|(_, m)| m.date_time_original)
        .and_then(|t| When::from_exif(&t));
    let when = exif.unwrap_or_else(|| {
        let secs = std::fs::metadata(path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_secs() as i64);
        When::from_unix(secs)
    });
    (when, clean(&camera))
}

/// Whether two files have the same bytes.
fn same_file(a: &Path, b: &Path) -> bool {
    let (Ok(ma), Ok(mb)) = (std::fs::metadata(a), std::fs::metadata(b)) else {
        return false;
    };
    if ma.len() != mb.len() {
        return false;
    }
    let (Ok(mut fa), Ok(mut fb)) = (std::fs::File::open(a), std::fs::File::open(b)) else {
        return false;
    };
    let (mut ba, mut bb) = (vec![0u8; 1 << 16], vec![0u8; 1 << 16]);
    loop {
        let na = fa.read(&mut ba).unwrap_or(0);
        let nb = fb.read(&mut bb).unwrap_or(0);
        if na != nb || ba[..na] != bb[..nb] {
            return false;
        }
        if na == 0 {
            return true;
        }
    }
}

/// The files in `dir` by size (empty when it doesn't exist yet).
fn index_sizes(dir: &Path) -> BTreeMap<u64, Vec<PathBuf>> {
    let mut out: BTreeMap<u64, Vec<PathBuf>> = BTreeMap::new();
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        if let Some(m) = e.metadata().ok().filter(|m| m.is_file()) {
            out.entry(m.len()).or_default().push(e.path());
        }
    }
    out
}

/// What importing `source` into `dest` with `options` would do, in capture
/// order. Nothing is written.
pub fn plan(source: &Path, dest: &Path, options: &ImportOptions) -> Result<Vec<Planned>, String> {
    check_template(&options.folder)?;
    check_template(&options.name)?;
    if options.name.trim().is_empty() {
        return Err("the name template is empty".into());
    }
    let mut files = Vec::new();
    scan(source, options.subfolders, &mut files)?;
    let groups = group(files);
    // Capture times from the raw headers, in parallel.
    let mut timed: Vec<(Group, When, String)> = groups
        .into_par_iter()
        .map(|g| {
            let (when, camera) = capture_time(&g.main);
            (g, when, camera)
        })
        .collect();
    timed.sort_by(|a, b| {
        a.1.text()
            .cmp(&b.1.text())
            .then_with(|| a.0.main.cmp(&b.0.main))
    });

    let mut taken: HashSet<PathBuf> = HashSet::new();
    // Each destination folder's files by size, to find a photo that is
    // already there under another name (a sequence number that moved).
    let mut sizes: BTreeMap<PathBuf, BTreeMap<u64, Vec<PathBuf>>> = BTreeMap::new();
    let mut out = Vec::new();
    for (i, (g, when, camera)) in timed.into_iter().enumerate() {
        let seq = options.first_number.saturating_add(i as u32);
        let name = g
            .main
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let folder: PathBuf = expand(&options.folder, &name, &when, seq, &camera)
            .split(['/', '\\'])
            .map(clean)
            .filter(|p| !p.is_empty())
            .collect();
        let dir = dest.join(folder);
        let stem = clean(&expand(&options.name, &name, &when, seq, &camera));
        let stem = if stem.is_empty() { name.clone() } else { stem };
        let sources: Vec<&PathBuf> = std::iter::once(&g.main).chain(&g.others).collect();
        let target = |stem: &str, src: &Path| dir.join(format!("{stem}{}", split_name(src).1));
        // Already there, under any name?
        let by_size = sizes
            .entry(dir.clone())
            .or_insert_with(|| index_sizes(&dir));
        let len = std::fs::metadata(&g.main).map_or(u64::MAX, |m| m.len());
        let twin = by_size
            .get(&len)
            .and_then(|same| same.iter().find(|f| same_file(&g.main, f)))
            .cloned();
        // The first free name: nothing at the destination or taken by this
        // import, unless the photo is already there.
        let mut skip = twin.map(|t| format!("already imported as {}", t.display()));
        let mut chosen = stem.clone();
        for n in 1.. {
            if skip.is_some() {
                break;
            }
            let candidate = if n == 1 {
                stem.clone()
            } else {
                format!("{stem}-{n}")
            };
            let main = target(&candidate, &g.main);
            if main.exists() && same_file(&g.main, &main) {
                skip = Some(format!("already imported as {}", main.display()));
                chosen = candidate;
                break;
            }
            let free = sources.iter().all(|s| {
                let t = target(&candidate, s);
                !t.exists() && !taken.contains(&t)
            });
            if free {
                chosen = candidate;
                break;
            }
        }
        let files: Vec<(PathBuf, PathBuf)> = sources
            .iter()
            .map(|s| ((*s).clone(), target(&chosen, s)))
            .collect();
        if skip.is_none() {
            taken.extend(files.iter().map(|(_, d)| d.clone()));
        }
        out.push(Planned {
            files,
            captured: when.text(),
            skip,
        });
    }
    Ok(out)
}

/// Copy `from` to `to` through a hidden temporary file, keeping the
/// modified time.
fn copy_file(from: &Path, to: &Path) -> Result<(), String> {
    let err = |e: std::io::Error| format!("{}: {e}", to.display());
    if let Some(dir) = to.parent() {
        std::fs::create_dir_all(dir).map_err(err)?;
    }
    if to.exists() {
        return Err(format!("{}: already exists", to.display()));
    }
    let name = to.file_name().unwrap_or_default().to_string_lossy();
    let tmp = to.with_file_name(format!(".{name}.amb-import"));
    let copied = std::fs::copy(from, &tmp).map_err(err)?;
    let len = std::fs::metadata(from).map_err(err)?.len();
    if copied != len {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("{}: copied {copied} of {len} bytes", to.display()));
    }
    if let Ok(modified) = std::fs::metadata(from).and_then(|m| m.modified()) {
        if let Ok(f) = std::fs::File::options().write(true).open(&tmp) {
            let _ = f.set_modified(modified);
        }
    }
    std::fs::rename(&tmp, to).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        err(e)
    })
}

/// Add `keywords` to the sidecar of `photo` (making one if needed).
fn add_keywords(photo: &Path, keywords: &[String]) -> Result<(), String> {
    let path = amb_engine::sidecar_path(photo);
    let on_disk = std::fs::read_to_string(&path).ok();
    let mut info = on_disk
        .as_deref()
        .map(sidecar::read_info)
        .unwrap_or_default();
    info.keywords.extend(keywords.iter().cloned());
    let text = sidecar::with_info(on_disk.as_deref(), &info).map_err(|e| e.to_string())?;
    sidecar::write_atomic(&path, &text).map_err(|e| e.to_string())
}

/// Carry out a plan. One failed photo doesn't stop the rest.
/// `progress(done, total, name)` is called before each photo.
pub fn run(
    plan: &[Planned],
    options: &ImportOptions,
    progress: &mut dyn FnMut(usize, usize, &str),
) -> Report {
    let mut report = Report::default();
    let keywords: Vec<String> = options
        .keywords
        .iter()
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty())
        .collect();
    for (i, p) in plan.iter().enumerate() {
        let Some((main_src, main_dest)) = p.files.first() else {
            continue;
        };
        let name = main_src
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        progress(i, plan.len(), &name);
        if p.skip.is_some() {
            report.skipped += 1;
            continue;
        }
        match p
            .files
            .iter()
            .try_for_each(|(from, to)| copy_file(from, to))
        {
            Ok(()) => {
                report.imported += 1;
                if let Some(dir) = main_dest.parent() {
                    if !report.folders.iter().any(|f| f == dir) {
                        report.folders.push(dir.to_path_buf());
                    }
                }
                // The photo is in; a sidecar that won't take keywords is
                // reported but doesn't undo that.
                if !keywords.is_empty() {
                    if let Err(e) = add_keywords(main_dest, &keywords) {
                        report
                            .errors
                            .push(format!("{name}: keywords not added: {e}"));
                    }
                }
            }
            Err(e) => report.errors.push(format!("{name}: {e}")),
        }
    }
    progress(plan.len(), plan.len(), "");
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("amb-import-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn write(path: &Path, bytes: &[u8]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    #[test]
    fn dates_and_templates() {
        assert_eq!(When::from_unix(1_760_097_600).text(), "2025-10-10 12:00:00");
        assert_eq!(When::from_unix(0).text(), "1970-01-01 00:00:00");
        let w = When::from_exif("2026:05:04 07:08:09").unwrap();
        assert_eq!(
            expand(
                "{yyyy}/{yy}{mm}{dd}_{hh}{min}{ss}_{seq}_{name}",
                "IMG_1",
                &w,
                7,
                "X100V"
            ),
            "2026/260504_070809_0007_IMG_1"
        );
        assert!(When::from_exif("0000:00:00 00:00:00").is_none());
        assert!(
            check_template("{yyyy}-{month}")
                .unwrap_err()
                .contains("{month}")
        );
        assert!(check_template("{yyyy").is_err());
        assert_eq!(clean(" a:b/c. "), "a-b-c");
    }

    #[test]
    fn plans_groups_renames_and_skips_duplicates() {
        let root = temp("plan");
        let card = root.join("card/DCIM/100CANON");
        let lib = root.join("photos");
        write(&card.join("IMG_0002.CR2"), b"raw two");
        write(&card.join("IMG_0002.JPG"), b"jpeg two");
        write(
            &card.join("IMG_0002.xmp"),
            br#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description rdf:about="" xmlns:xmp="http://ns.adobe.com/xap/1.0/" xmp:Rating="3"/></rdf:RDF></x:xmpmeta>"#,
        );
        write(&card.join("IMG_0002.copy-2.xmp"), b"a virtual copy");
        write(&card.join("IMG_0001.CR2"), b"raw one");
        write(&card.join("phone.jpg"), b"phone photo");
        write(
            &card.join("phone.jpg.xmp"),
            br#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description rdf:about="" xmlns:xmp="http://ns.adobe.com/xap/1.0/" xmp:Rating="2"/></rdf:RDF></x:xmpmeta>"#,
        );
        write(&card.join("notes.txt"), b"not a photo");
        write(&card.join(".hidden.CR2"), b"hidden");
        // Same name already in the library, different photo: renamed.
        // Same name and bytes: skipped.
        let options = ImportOptions {
            folder: "shoot".into(),
            ..ImportOptions::default()
        };
        write(&lib.join("shoot/IMG_0001.CR2"), b"raw one");
        write(&lib.join("shoot/IMG_0002.CR2"), b"another photo");
        let plan = plan(&root.join("card"), &lib, &options).unwrap();
        assert_eq!(plan.len(), 3, "{plan:#?}");
        let phone = plan
            .iter()
            .find(|p| p.files[0].0.ends_with("phone.jpg"))
            .unwrap();
        assert!(
            phone.files[1].1.ends_with("shoot/phone.jpg.xmp"),
            "{phone:#?}"
        );
        let one = plan
            .iter()
            .find(|p| p.files[0].0.ends_with("IMG_0001.CR2"))
            .unwrap();
        assert!(one.skip.as_deref().unwrap().contains("already imported"));
        let two = plan
            .iter()
            .find(|p| p.files[0].0.ends_with("IMG_0002.CR2"))
            .unwrap();
        assert!(two.skip.is_none());
        let dests: Vec<String> = two
            .files
            .iter()
            .map(|(_, d)| d.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(dests.len(), 4);
        assert!(
            dests.contains(&"IMG_0002-2.copy-2.xmp".to_string()),
            "{dests:?}"
        );
        assert!(
            dests.iter().all(|d| d.starts_with("IMG_0002-2.")),
            "{dests:?}"
        );

        let report = run(
            &plan,
            &ImportOptions {
                keywords: vec!["trip".into()],
                ..options.clone()
            },
            &mut |_, _, _| {},
        );
        assert_eq!((report.imported, report.skipped), (2, 1), "{report:?}");
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        let shoot = lib.join("shoot");
        assert_eq!(
            std::fs::read(shoot.join("IMG_0002-2.JPG")).unwrap(),
            b"jpeg two"
        );
        let xmp = std::fs::read_to_string(shoot.join("IMG_0002-2.xmp")).unwrap();
        let phone =
            sidecar::read_info(&std::fs::read_to_string(shoot.join("phone.jpg.xmp")).unwrap());
        assert_eq!(
            (phone.keywords, phone.rating),
            (vec!["trip".to_string()], Some(2))
        );
        assert!(!shoot.join("phone.xmp").exists());
        let info = sidecar::read_info(&xmp);
        assert_eq!(
            (info.keywords, info.rating),
            (vec!["trip".to_string()], Some(3))
        );
        assert_eq!(report.folders, std::slice::from_ref(&shoot));
        // No temporary files left; a second run imports nothing new.
        assert!(
            std::fs::read_dir(&shoot)
                .unwrap()
                .flatten()
                .all(|e| !e.file_name().to_string_lossy().starts_with('.'))
        );
        let again = super::plan(&root.join("card"), &lib, &options).unwrap();
        assert!(again.iter().all(|p| p.skip.is_some()), "{again:#?}");
        // Numbered names: a new photo first in line moves every number, but
        // the photos already imported are still found.
        let numbered = ImportOptions {
            name: "s_{seq}".into(),
            ..options.clone()
        };
        let first = super::plan(&root.join("card"), &lib, &numbered).unwrap();
        run(&first, &numbered, &mut |_, _, _| {});
        write(&card.join("IMG_0000.CR2"), b"raw zero");
        let mtime = std::fs::metadata(card.join("IMG_0001.CR2"))
            .unwrap()
            .modified()
            .unwrap();
        std::fs::File::options()
            .write(true)
            .open(card.join("IMG_0000.CR2"))
            .unwrap()
            .set_modified(mtime - std::time::Duration::from_secs(3600))
            .unwrap();
        let next = super::plan(&root.join("card"), &lib, &numbered).unwrap();
        let fresh: Vec<_> = next.iter().filter(|p| p.skip.is_none()).collect();
        assert_eq!(fresh.len(), 1, "{next:#?}");
        assert!(fresh[0].files[0].0.ends_with("IMG_0000.CR2"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn sequence_names_follow_capture_order() {
        let root = temp("seq");
        let card = root.join("card");
        write(&card.join("b.nef"), b"b");
        write(&card.join("a.nef"), b"a");
        // Without EXIF the modified time orders them: b first.
        let early =
            std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
        let late = early + std::time::Duration::from_secs(60);
        for (name, t) in [("b.nef", early), ("a.nef", late)] {
            std::fs::File::options()
                .write(true)
                .open(card.join(name))
                .unwrap()
                .set_modified(t)
                .unwrap();
        }
        let options = ImportOptions {
            folder: "{yyyy}-{mm}".into(),
            name: "trip_{seq}".into(),
            first_number: 9,
            ..ImportOptions::default()
        };
        let plan = plan(&card, &root.join("lib"), &options).unwrap();
        let names: Vec<String> = plan
            .iter()
            .map(|p| {
                let (src, dst) = &p.files[0];
                format!(
                    "{} {}",
                    src.file_name().unwrap().to_string_lossy(),
                    dst.strip_prefix(root.join("lib")).unwrap().display()
                )
            })
            .collect();
        assert_eq!(
            names,
            ["b.nef 2023-11/trip_0009.nef", "a.nef 2023-11/trip_0010.nef"]
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
