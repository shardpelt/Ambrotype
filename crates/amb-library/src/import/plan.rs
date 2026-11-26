// SPDX-License-Identifier: GPL-3.0-or-later
//! Import's plan: what would be copied where, nothing written.

use super::*;

/// Whether two files have the same bytes.
pub(super) fn same_file(a: &Path, b: &Path) -> bool {
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

/// The files in `dir` by size (empty when it doesn't exist yet). A
/// half-copied leftover of a cut-off import isn't a photo that's there.
pub(super) fn index_sizes(dir: &Path) -> BTreeMap<u64, Vec<PathBuf>> {
    let mut out: BTreeMap<u64, Vec<PathBuf>> = BTreeMap::new();
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        if is_leftover(&e.file_name().to_string_lossy()) {
            continue;
        }
        if let Some(m) = e.metadata().ok().filter(|m| m.is_file()) {
            out.entry(m.len()).or_default().push(e.path());
        }
    }
    out
}

/// Whether `main` is among the photos picked (all, when none are named).
fn picked(only: &[String], main: &Path) -> bool {
    let name = main.file_name().unwrap_or_default().to_string_lossy();
    only.is_empty()
        || only
            .iter()
            .any(|o| o.eq_ignore_ascii_case(&name) || Path::new(o) == main)
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
    let groups: Vec<Group> = group(files)
        .into_iter()
        .filter(|g| picked(&options.only, &g.main))
        .collect();
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
