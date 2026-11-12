// SPDX-License-Identifier: GPL-3.0-or-later
//! Batch export: a folder of photos, subfolders and all, to smaller copies
//! (or any other export) in another folder with the same structure. One
//! export setting for every photo, each with its own edit.
//!
//! - [`plan`] walks the sources and says what would be written where. It
//!   uses the library's own idea of a photo ([`amb_library::folder`]): a raw
//!   with a JPEG of the same name is one photo, the raw (as in the library
//!   and Convert raws); virtual copies are photos of their own.
//! - [`run`] renders the jobs a few at a time and carries on after a photo
//!   fails. A file is written under a hidden name and renamed when whole, so
//!   a stopped run never leaves a half file that `skip_existing` would take
//!   for done: stop it, run it again, and it carries on.
//!
//! The render of one photo is the caller's ([`Render`]): `amb` and the editor
//! each develop a photo with their own lens and profile sources, and the
//! editor's session keeps more (Upright, masks) than a bare file does.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Instant, SystemTime};

use amb_export::ExportSettings;
use amb_library::folder;

/// Photos exported at once: enough to keep reading, developing and encoding
/// busy, few enough for the memory (as [`crate::convert::AT_ONCE`]).
pub const AT_ONCE: usize = 3;

/// Name of a file while it is being written: hidden, so the library and a
/// second run don't see it.
const PARTIAL: &str = "partial";

#[derive(Debug, Clone)]
pub struct Options {
    /// Photos in the sources' subfolders too.
    pub recursive: bool,
    /// Keep a folder's subfolders under the output folder; off puts every
    /// file straight in it.
    pub keep_structure: bool,
    /// Leave alone an output that exists and is newer than its photo and
    /// the photo's sidecar.
    pub skip_existing: bool,
    /// How every photo is exported. A smaller copy beside each file
    /// (`small_copy`) is not made: the files are the smaller copies.
    pub export: ExportSettings,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            recursive: false,
            keep_structure: true,
            skip_existing: false,
            export: ExportSettings::new(amb_export::Format::Jpeg),
        }
    }
}

/// One photo and the file it becomes.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Job {
    pub source: PathBuf,
    pub out: PathBuf,
    /// The photo's folder below the source folder ("" for its own), for
    /// showing where a run is.
    pub folder: String,
}

/// What a run did.
#[derive(Debug, Default, serde::Serialize)]
pub struct Report {
    pub done: usize,
    /// Outputs that were there already and up to date.
    pub skipped: usize,
    /// Photos that could not be exported, with why.
    pub failed: Vec<(PathBuf, String)>,
    /// Size of the photos exported and of the files written.
    pub bytes_in: u64,
    pub bytes_out: u64,
    pub seconds: f64,
    /// The run was stopped before the last photo.
    pub stopped: bool,
}

/// Renders one photo with its own edit into a file, with these settings.
pub type Render<'a> = dyn Fn(&Path, &Path, &ExportSettings) -> Result<(), String> + Sync + 'a;

/// What would be written, for `sources` (photos and folders) into `out_dir`.
/// Names that two photos would share (`a/x.nef` and `a/x.dng`, or any two
/// with `keep_structure` off) get `-2`, `-3`… as in the editor's export,
/// the same on every run so `skip_existing` finds them.
pub fn plan(sources: &[PathBuf], out_dir: &Path, options: &Options) -> Result<Vec<Job>, String> {
    let ext = options.export.format.extension();
    let skip = canonical(out_dir);
    let mut jobs = Vec::new();
    let mut seen_dirs = HashSet::new();
    for source in sources {
        if source.is_dir() {
            if skip.is_some() && canonical(source) == skip {
                return Err(format!(
                    "{}: the output would replace the photo itself; choose another folder",
                    source.display()
                ));
            }
            let mut found = Vec::new();
            walk(
                source,
                "",
                options,
                skip.as_deref(),
                &mut seen_dirs,
                &mut found,
            )?;
            for (photo, rel) in found {
                let dir = if options.keep_structure {
                    out_dir.join(&rel)
                } else {
                    out_dir.to_path_buf()
                };
                jobs.push(job(photo, dir, ext, rel));
            }
        } else if source.is_file() {
            if !folder::is_photo(source) && amb_engine::virtual_copy_of(source).is_none() {
                return Err(format!("{}: not a photo", source.display()));
            }
            jobs.push(job(
                source.clone(),
                out_dir.to_path_buf(),
                ext,
                String::new(),
            ));
        } else {
            return Err(format!("{}: no such file or folder", source.display()));
        }
    }
    let mut taken: HashMap<String, usize> = HashMap::new();
    for j in &mut jobs {
        if same_file(&j.source, &j.out) {
            return Err(format!(
                "{}: the output would replace the photo itself; choose another folder",
                j.source.display()
            ));
        }
        // Case-insensitive, so the names stay apart on a Mac or Windows too.
        let key = j.out.to_string_lossy().to_lowercase();
        let n = taken.entry(key).or_insert(0);
        *n += 1;
        if *n > 1 {
            let stem = j.out.file_stem().unwrap_or_default().to_string_lossy();
            j.out = j.out.with_file_name(format!("{stem}-{n}.{ext}"));
        }
    }
    Ok(jobs)
}

fn job(source: PathBuf, dir: PathBuf, ext: &str, folder: String) -> Job {
    let stem = source.file_stem().unwrap_or_default().to_string_lossy();
    Job {
        out: dir.join(format!("{stem}.{ext}")),
        source,
        folder,
    }
}

/// `dir`'s photos, then (with `recursive`) its subfolders', each with its
/// folder below the one the walk started in. The output folder, hidden
/// folders and a folder already walked (a link back up) are left out.
fn walk(
    dir: &Path,
    rel: &str,
    options: &Options,
    skip: Option<&Path>,
    seen: &mut HashSet<PathBuf>,
    out: &mut Vec<(PathBuf, String)>,
) -> Result<(), String> {
    let here = canonical(dir).unwrap_or_else(|| dir.to_path_buf());
    if skip.is_some_and(|s| here == s) || !seen.insert(here) {
        return Ok(());
    }
    for photo in folder::photo_paths(dir)? {
        out.push((photo, rel.to_string()));
    }
    if options.recursive {
        for sub in folder::subfolders(dir) {
            let inner = if rel.is_empty() {
                sub.name.clone()
            } else {
                format!("{rel}/{}", sub.name)
            };
            // A folder that can't be read is skipped, not the whole tree.
            let _ = walk(&sub.path, &inner, options, skip, seen, out);
        }
    }
    Ok(())
}

fn canonical(p: &Path) -> Option<PathBuf> {
    std::fs::canonicalize(p).ok()
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (canonical(a), canonical(b)) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

/// Export `jobs`, `progress(done, total)` after each (skipped ones too).
/// Setting `stop` ends the run after the photos under way; the report says
/// so. A photo that fails is in the report and the run goes on.
pub fn run(
    jobs: &[Job],
    options: &Options,
    render: &Render,
    progress: &(dyn Fn(usize, usize) + Sync),
    stop: &AtomicBool,
) -> Report {
    let start = Instant::now();
    let mut export = options.export.clone();
    export.small_copy = None;
    let next = AtomicUsize::new(0);
    let finished = AtomicUsize::new(0);
    let report = Mutex::new(Report::default());
    std::thread::scope(|scope| {
        for _ in 0..AT_ONCE.min(jobs.len()) {
            scope.spawn(|| {
                loop {
                    if stop.load(Ordering::Relaxed) {
                        break;
                    }
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(job) = jobs.get(i) else { break };
                    let result = one(job, options.skip_existing, &export, render);
                    {
                        let mut r = report.lock().unwrap_or_else(|e| e.into_inner());
                        match result {
                            Outcome::Skipped => r.skipped += 1,
                            Outcome::Written(bytes_in, bytes_out) => {
                                r.done += 1;
                                r.bytes_in += bytes_in;
                                r.bytes_out += bytes_out;
                            }
                            Outcome::Failed(why) => r.failed.push((job.source.clone(), why)),
                        }
                    }
                    progress(finished.fetch_add(1, Ordering::Relaxed) + 1, jobs.len());
                }
            });
        }
    });
    let mut report = report.into_inner().unwrap_or_else(|e| e.into_inner());
    // In the photos' order, whichever finished first.
    report.failed.sort_by(|a, b| a.0.cmp(&b.0));
    report.stopped = finished.load(Ordering::Relaxed) < jobs.len();
    report.seconds = start.elapsed().as_secs_f64();
    report
}

enum Outcome {
    Skipped,
    /// Bytes read and bytes written.
    Written(u64, u64),
    Failed(String),
}

fn one(job: &Job, skip_existing: bool, export: &ExportSettings, render: &Render) -> Outcome {
    if skip_existing && up_to_date(job) {
        return Outcome::Skipped;
    }
    let Some(dir) = job.out.parent() else {
        return Outcome::Failed("no folder to write in".into());
    };
    if let Err(e) = std::fs::create_dir_all(dir) {
        return Outcome::Failed(format!("{}: {e}", dir.display()));
    }
    let partial = partial_path(&job.out);
    let written = render(&job.source, &partial, export).and_then(|()| {
        std::fs::rename(&partial, &job.out).map_err(|e| format!("{}: {e}", job.out.display()))
    });
    match written {
        Ok(()) => {
            let size = |p: &Path| std::fs::metadata(p).map_or(0, |m| m.len());
            Outcome::Written(size(&amb_engine::source_path(&job.source)), size(&job.out))
        }
        Err(e) => {
            let _ = std::fs::remove_file(&partial);
            Outcome::Failed(e)
        }
    }
}

/// `.name.partial.ext` beside `out`.
fn partial_path(out: &Path) -> PathBuf {
    let stem = out.file_stem().unwrap_or_default().to_string_lossy();
    let ext = out.extension().unwrap_or_default().to_string_lossy();
    out.with_file_name(format!(".{stem}.{PARTIAL}.{ext}"))
}

/// The output exists and is no older than the photo or its sidecar.
fn up_to_date(job: &Job) -> bool {
    let modified = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    let Some(out) = modified(&job.out) else {
        return false;
    };
    let newest = [
        amb_engine::source_path(&job.source),
        amb_engine::sidecar_path(&job.source),
        job.source.clone(),
    ]
    .iter()
    .filter_map(|p| modified(p))
    .max()
    .unwrap_or(SystemTime::UNIX_EPOCH);
    out >= newest
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("amb-batch-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn touch(path: &Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"photo").unwrap();
    }

    /// A tree: two JPEGs in the root, a raw with its JPEG and a PNG in
    /// `trip/day1`, a JPEG in `trip/day1/deep`, and a text file.
    fn tree(root: &Path) {
        for f in [
            "a.jpg",
            "b.JPG",
            "notes.txt",
            "trip/day1/c.nef",
            "trip/day1/c.jpg",
            "trip/day1/d.png",
            "trip/day1/deep/e.jpg",
        ] {
            touch(&root.join(f));
        }
    }

    fn outs(jobs: &[Job], out: &Path) -> Vec<String> {
        let mut v: Vec<String> = jobs
            .iter()
            .map(|j| {
                j.out
                    .strip_prefix(out)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect();
        v.sort();
        v
    }

    #[test]
    fn plans_a_tree_keeping_its_structure() {
        let (root, out) = (temp("tree"), temp("tree-out"));
        tree(&root);
        let options = Options {
            recursive: true,
            ..Options::default()
        };
        let jobs = plan(std::slice::from_ref(&root), &out, &options).unwrap();
        // The raw stands for its JPEG; the text file is no photo.
        assert_eq!(
            outs(&jobs, &out),
            [
                "a.jpg",
                "b.jpg",
                "trip/day1/c.jpg",
                "trip/day1/d.jpg",
                "trip/day1/deep/e.jpg"
            ]
        );
        assert!(jobs.iter().any(|j| j.folder == "trip/day1/deep"));
        assert!(jobs.iter().any(|j| j.source.ends_with("c.nef")));
        assert!(!jobs.iter().any(|j| j.source.ends_with("c.jpg")));
    }

    #[test]
    fn without_recursive_only_the_folders_own_photos() {
        let (root, out) = (temp("flat1"), temp("flat1-out"));
        tree(&root);
        let jobs = plan(&[root], &out, &Options::default()).unwrap();
        assert_eq!(outs(&jobs, &out), ["a.jpg", "b.jpg"]);
    }

    #[test]
    fn flat_puts_everything_in_one_folder_and_numbers_clashes() {
        let (root, out) = (temp("flat2"), temp("flat2-out"));
        touch(&root.join("x/photo.jpg"));
        touch(&root.join("y/photo.jpg"));
        touch(&root.join("y/Photo.png"));
        let options = Options {
            recursive: true,
            keep_structure: false,
            ..Options::default()
        };
        let jobs = plan(&[root], &out, &options).unwrap();
        assert_eq!(
            outs(&jobs, &out),
            ["Photo-3.jpg", "photo-2.jpg", "photo.jpg"]
        );
        // The same plan twice, so a second run finds the same names.
        assert_eq!(
            plan(&[jobs[0].source.parent().unwrap().into()], &out, &options)
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn leaves_out_an_output_folder_inside_the_source() {
        let root = temp("inside");
        let out = root.join("small");
        touch(&root.join("a.jpg"));
        touch(&out.join("old.jpg"));
        let options = Options {
            recursive: true,
            ..Options::default()
        };
        let jobs = plan(&[root], &out, &options).unwrap();
        assert_eq!(outs(&jobs, &out), ["a.jpg"]);
    }

    #[test]
    fn refuses_to_write_over_a_photo() {
        let root = temp("same");
        touch(&root.join("a.jpg"));
        let e = plan(std::slice::from_ref(&root), &root, &Options::default()).unwrap_err();
        assert!(e.contains("replace the photo"), "{e}");
    }

    #[test]
    fn a_single_file_goes_straight_into_the_folder() {
        let (root, out) = (temp("file"), temp("file-out"));
        tree(&root);
        let jobs = plan(&[root.join("a.jpg")], &out, &Options::default()).unwrap();
        assert_eq!(outs(&jobs, &out), ["a.jpg"]);
        assert!(plan(&[root.join("notes.txt")], &out, &Options::default()).is_err());
        assert!(plan(&[root.join("nope.jpg")], &out, &Options::default()).is_err());
    }

    /// Writes a file of `n` bytes; fails for a photo called "bad".
    fn fake(source: &Path, out: &Path, _: &ExportSettings) -> Result<(), String> {
        if source.file_stem().is_some_and(|s| s == "bad") {
            return Err("cannot read it".into());
        }
        std::fs::write(out, b"small").map_err(|e| e.to_string())
    }

    #[test]
    fn runs_reports_and_skips_what_is_done() {
        let (root, out) = (temp("run"), temp("run-out"));
        for f in ["a.jpg", "sub/b.jpg", "sub/bad.jpg"] {
            touch(&root.join(f));
        }
        let options = Options {
            recursive: true,
            skip_existing: true,
            ..Options::default()
        };
        let jobs = plan(std::slice::from_ref(&root), &out, &options).unwrap();
        let heard = AtomicUsize::new(0);
        let stop = AtomicBool::new(false);
        let report = run(
            &jobs,
            &options,
            &fake,
            &|_, _| {
                heard.fetch_add(1, Ordering::Relaxed);
            },
            &stop,
        );
        assert_eq!(
            (report.done, report.skipped, report.failed.len()),
            (2, 0, 1)
        );
        assert!(report.failed[0].0.ends_with("bad.jpg") && report.failed[0].1.contains("read"));
        assert_eq!((report.bytes_in, report.bytes_out), (10, 10));
        assert_eq!(heard.load(Ordering::Relaxed), 3);
        assert!(out.join("sub/b.jpg").is_file() && !out.join("sub/bad.jpg").exists());
        // No partial file is left behind.
        assert!(!out.join("sub/.bad.partial.jpg").exists());
        // Again: the two done are skipped, the failure is tried again.
        let again = run(&jobs, &options, &fake, &|_, _| {}, &stop);
        assert_eq!((again.done, again.skipped, again.failed.len()), (0, 2, 1));
    }

    #[test]
    fn a_changed_photo_or_edit_is_exported_again() {
        let (root, out) = (temp("fresh"), temp("fresh-out"));
        touch(&root.join("a.jpg"));
        let options = Options {
            skip_existing: true,
            ..Options::default()
        };
        let jobs = plan(std::slice::from_ref(&root), &out, &options).unwrap();
        let stop = AtomicBool::new(false);
        assert_eq!(run(&jobs, &options, &fake, &|_, _| {}, &stop).done, 1);
        assert!(up_to_date(&jobs[0]));
        // The edit changes after the export.
        std::thread::sleep(std::time::Duration::from_millis(50));
        std::fs::write(amb_engine::sidecar_path(&jobs[0].source), b"<x/>").unwrap();
        assert!(!up_to_date(&jobs[0]));
        assert_eq!(run(&jobs, &options, &fake, &|_, _| {}, &stop).done, 1);
    }

    #[test]
    fn stop_ends_the_run_early() {
        let (root, out) = (temp("stop"), temp("stop-out"));
        for i in 0..20 {
            touch(&root.join(format!("{i:02}.jpg")));
        }
        let options = Options::default();
        let jobs = plan(&[root], &out, &options).unwrap();
        let stop = AtomicBool::new(false);
        let report = run(
            &jobs,
            &options,
            &fake,
            &|done, _| {
                if done >= 4 {
                    stop.store(true, Ordering::Relaxed);
                }
            },
            &stop,
        );
        assert!(report.stopped);
        assert!(report.done >= 4 && report.done < 20, "{}", report.done);
    }
}
