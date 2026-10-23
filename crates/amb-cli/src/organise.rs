// SPDX-License-Identifier: GPL-3.0-or-later
//! The library from the command line, as the editor's library does it:
//! photo info and tagging many photos at once, places (GPX tracks),
//! duplicates and bursts, moving, renaming and deleting with companions,
//! keywords, collections, and photos moved outside the app. Everything goes
//! into the photos' XMP sidecars; the catalogue and collections follow
//! moves.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use amb_library::catalog::{self, Catalog};
use amb_library::collections;
use amb_library::files::{self, Target};
use amb_protocol::sidecar::{self, Flag, PhotoInfo};

use crate::print_json;

type Options<'a> = BTreeMap<&'a str, &'a str>;

/// The photos named: files as given, folders as the photos in them.
fn photos(args: &[&str]) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    for a in args {
        // Absolute, as the catalogue and collections keep them.
        let p = std::path::absolute(a).map_err(|e| format!("{a}: {e}"))?;
        if p.is_dir() {
            out.extend(amb_library::folder::photo_paths(&p)?);
        } else if p.is_file() {
            out.push(p);
        } else {
            return Err(format!("{a}: no such photo or folder"));
        }
    }
    if out.is_empty() {
        return Err("no photos".into());
    }
    Ok(out)
}

fn open_catalog(options: &Options) -> Result<Catalog, String> {
    let path = match options.get("--catalog") {
        Some(p) => PathBuf::from(p),
        None => catalog::default_path().ok_or("no config folder: give --catalog FILE")?,
    };
    Catalog::open(&path)
}

fn collections_dir(options: &Options) -> Result<PathBuf, String> {
    match options.get("--collections") {
        Some(d) => Ok(PathBuf::from(d)),
        None => collections::editor_dir()
            .ok_or_else(|| "no config folder: give --collections DIR".into()),
    }
}

fn read_info(photo: &Path) -> PhotoInfo {
    std::fs::read_to_string(amb_engine::sidecar_path(photo))
        .ok()
        .as_deref()
        .map(sidecar::read_info)
        .unwrap_or_default()
}

fn write_info(photo: &Path, info: &PhotoInfo) -> Result<PhotoInfo, String> {
    let path = amb_engine::sidecar_path(photo);
    let on_disk = std::fs::read_to_string(&path).ok();
    let text = sidecar::with_info(on_disk.as_deref(), info).map_err(|e| e.to_string())?;
    sidecar::write_atomic(&path, &text).map_err(|e| e.to_string())?;
    Ok(sidecar::read_info(&text))
}

/// "1 photo", "2 photos".
fn photos_n(n: usize) -> String {
    format!("{n} photo{}", if n == 1 { "" } else { "s" })
}

fn list(v: &str) -> Vec<String> {
    v.split(',')
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty())
        .collect()
}

/// `amb info`: what each photo's sidecar says.
pub(crate) fn info(args: &[&str], json: bool) -> Result<(), String> {
    let photos = photos(args)?;
    if json {
        let all: Vec<serde_json::Value> = photos
            .iter()
            .map(|p| serde_json::json!({ "path": p, "info": read_info(p) }))
            .collect();
        return print_json(&serde_json::Value::Array(all));
    }
    for p in &photos {
        let i = read_info(p);
        let mut parts = Vec::new();
        if let Some(r) = i.rating {
            parts.push("*".repeat(r.into()));
        }
        if let Some(l) = &i.label {
            parts.push(l.clone());
        }
        if let Some(f) = i.flag {
            parts.push(f.name().into());
        }
        if !i.keywords.is_empty() {
            parts.push(format!("[{}]", i.keywords.join(", ")));
        }
        for (name, v) in [
            ("title", &i.title),
            ("caption", &i.caption),
            ("creator", &i.creator),
            ("©", &i.copyright),
        ] {
            if let Some(v) = v.as_deref().filter(|v| !v.is_empty()) {
                parts.push(format!("{name}: {v}"));
            }
        }
        if let Some((lat, lon)) = i.place() {
            parts.push(format!("at {lat:.5},{lon:.5}"));
        }
        if let Some(s) = &i.stack {
            parts.push(format!(
                "stack {}{}",
                s.id,
                if s.top { " (top)" } else { "" }
            ));
        }
        println!("{}  {}", p.display(), parts.join("  "));
    }
    Ok(())
}

/// `amb tag`: one change to many photos' rating, label, flag, keywords,
/// title, caption, creator, copyright and place.
pub(crate) fn tag(
    args: &[&str],
    options: &Options,
    dry_run: bool,
    json: bool,
) -> Result<(), String> {
    let photos = photos(args)?;
    let rating = options
        .get("--rating")
        .map(|r| {
            r.parse::<u8>()
                .ok()
                .filter(|r| *r <= 5)
                .ok_or(format!("--rating {r}: 0 to 5"))
        })
        .transpose()?;
    let label = options
        .get("--label")
        .map(|l| match l.to_lowercase().as_str() {
            "none" | "" => None,
            _ => Some(l.to_string()),
        });
    let flag = options
        .get("--flag")
        .map(|f| match *f {
            "none" | "" => Ok(None),
            f => Flag::parse(f)
                .map(Some)
                .ok_or(format!("--flag {f}: pick, reject or none")),
        })
        .transpose()?;
    let place = options
        .get("--place")
        .map(|p| match *p {
            "none" | "" => Ok(String::new()),
            p => sidecar::parse_place(p)
                .map(|(a, b)| format!("{a:.6},{b:.6}"))
                .ok_or(format!("--place {p}: latitude,longitude or none")),
        })
        .transpose()?;
    let add = options.get("--add").map(|v| list(v)).unwrap_or_default();
    let remove: Vec<String> = options.get("--remove").map(|v| list(v)).unwrap_or_default();
    let changes = [
        rating.is_some(),
        label.is_some(),
        flag.is_some(),
        place.is_some(),
        !add.is_empty(),
        !remove.is_empty(),
    ]
    .iter()
    .any(|c| *c)
        || ["--title", "--caption", "--creator", "--copyright"]
            .iter()
            .any(|k| options.contains_key(k));
    if !changes {
        return Err("nothing to change: give --rating, --label, --flag, --add, --remove, --title, --caption, --creator, --copyright or --place".into());
    }
    let mut out = Vec::new();
    for p in &photos {
        let mut i = read_info(p);
        if let Some(r) = rating {
            i.rating = (r > 0).then_some(r);
        }
        if let Some(l) = &label {
            i.label = l.clone();
        }
        if let Some(f) = flag {
            i.flag = f;
        }
        if place.is_some() {
            i.gps = place.clone();
        }
        let lower: Vec<String> = remove.iter().map(|k| k.to_lowercase()).collect();
        i.keywords.retain(|k| !lower.contains(&k.to_lowercase()));
        for k in &add {
            if !i.keywords.iter().any(|x| x.eq_ignore_ascii_case(k)) {
                i.keywords.push(k.clone());
            }
        }
        for (key, field) in [
            ("--title", &mut i.title),
            ("--caption", &mut i.caption),
            ("--creator", &mut i.creator),
            ("--copyright", &mut i.copyright),
        ] {
            if let Some(v) = options.get(key) {
                *field = Some(v.to_string());
            }
        }
        let written = if dry_run {
            i.cleaned()
        } else {
            write_info(p, &i)?
        };
        out.push(serde_json::json!({ "path": p, "info": written }));
    }
    if json {
        return print_json(&serde_json::Value::Array(out));
    }
    eprintln!(
        "{} {}",
        if dry_run { "Would change" } else { "Changed" },
        photos_n(photos.len())
    );
    Ok(())
}

/// `amb place --gpx`: where photos were taken, from a GPS track.
pub(crate) fn place(
    args: &[&str],
    options: &Options,
    dry_run: bool,
    json: bool,
) -> Result<(), String> {
    let photos = photos(args)?;
    let gpx = options.get("--gpx").ok_or("place needs --gpx TRACK.gpx")?;
    let text = std::fs::read_to_string(gpx).map_err(|e| format!("{gpx}: {e}"))?;
    let track = amb_library::gpx::parse(&text)?;
    let hours: f64 = match options.get("--offset") {
        Some(o) => o
            .trim_start_matches('+')
            .parse()
            .map_err(|_| format!("--offset {o}: hours, e.g. 2 or -5"))?,
        None => 0.0,
    };
    let replace = options.contains_key("--replace");
    let mut placed = Vec::new();
    let (mut kept, mut unplaced) = (0, 0);
    for p in &photos {
        let mut info = read_info(p);
        let camera = amb_engine::raw::read_metadata(&amb_engine::source_path(p)).ok();
        let camera_gps = camera.as_ref().is_some_and(|(_, m)| m.gps.is_some());
        if !replace && (info.place().is_some() || camera_gps) {
            kept += 1;
            continue;
        }
        let captured = camera.and_then(|(_, m)| m.date_time_original).map(|t| {
            let (d, rest) = t.split_once(' ').unwrap_or((&t, ""));
            format!("{} {rest}", d.replace(':', "-"))
        });
        let Some((lat, lon)) =
            captured.and_then(|c| track.place(&c, (hours * 60.0).round() as i64))
        else {
            unplaced += 1;
            continue;
        };
        info.gps = Some(format!("{lat:.6},{lon:.6}"));
        if !dry_run {
            write_info(p, &info)?;
        }
        placed.push(serde_json::json!({ "path": p, "lat": lat, "lon": lon }));
    }
    if json {
        return print_json(
            &serde_json::json!({ "placed": placed, "kept": kept, "unplaced": unplaced }),
        );
    }
    for p in &placed {
        println!(
            "{}  {:.5},{:.5}",
            p["path"].as_str().unwrap_or(""),
            p["lat"],
            p["lon"]
        );
    }
    eprintln!(
        "{} {} · {kept} already had a place (--replace to redo) · {unplaced} taken while the track wasn't recording",
        if dry_run { "Would place" } else { "Placed" },
        placed.len()
    );
    Ok(())
}

/// `amb duplicates`: same files, alike pictures and bursts; best first.
pub(crate) fn duplicates(args: &[&str], options: &Options, json: bool) -> Result<(), String> {
    use amb_library::duplicates::{self, Options as Find};
    let photos = photos(args)?;
    let cat = open_catalog(options)?;
    let find = Find {
        max_bits: match options.get("--alike") {
            Some(b) => b.parse().map_err(|_| format!("--alike {b}: 0 to 20"))?,
            None => Find::default().max_bits,
        },
        burst_seconds: match options.get("--burst") {
            Some(s) => s.parse().map_err(|_| format!("--burst {s}: seconds"))?,
            None => Find::default().burst_seconds,
        },
    };
    let thumb = |p: &Path| amb_engine::raw::embedded_thumbnail(p, 256).ok().flatten();
    let groups = duplicates::find(&cat, &photos, find, &thumb, &|_, _| {});
    let (stack, reject) = (
        options.contains_key("--stack"),
        options.contains_key("--reject"),
    );
    for (i, g) in groups.iter().enumerate() {
        if stack {
            let id = format!("d{}{i}", std::process::id());
            for (k, p) in g.photos.iter().enumerate() {
                let mut info = read_info(p);
                info.stack = Some(sidecar::StackRef {
                    id: id.clone(),
                    top: k == 0,
                });
                write_info(p, &info)?;
            }
        }
        if reject {
            for (k, p) in g.photos.iter().enumerate() {
                let mut info = read_info(p);
                info.flag = Some(if k == 0 { Flag::Pick } else { Flag::Reject });
                write_info(p, &info)?;
            }
        }
    }
    if json {
        return print_json(&serde_json::to_value(&groups).map_err(|e| e.to_string())?);
    }
    for g in &groups {
        println!("{:?}:", g.kind);
        for (k, p) in g.photos.iter().enumerate() {
            println!("  {} {}", if k == 0 { "*" } else { " " }, p.display());
        }
    }
    let extra: usize = groups.iter().map(|g| g.photos.len() - 1).sum();
    eprintln!(
        "{} group{}, {} you may not need (* the best){}",
        groups.len(),
        if groups.len() == 1 { "" } else { "s" },
        photos_n(extra),
        match (stack, reject) {
            (true, true) => "; stacked, the rest rejected",
            (true, false) => "; stacked",
            (false, true) => "; the best picked, the rest rejected",
            _ => "",
        }
    );
    Ok(())
}

/// `amb move`, `amb rename`, `amb sort`: plan, show, then move each photo
/// with its sidecar, JPEG twin and virtual copies.
pub(crate) fn relocate(
    kind: &str,
    args: &[&str],
    options: &Options,
    dry_run: bool,
    json: bool,
) -> Result<(), String> {
    let photos = photos(args)?;
    let target = match kind {
        "move" => Target::Move {
            folder: PathBuf::from(options.get("--to").ok_or("move needs --to DIR")?),
        },
        "rename" => Target::Rename {
            template: options
                .get("--name")
                .ok_or("rename needs --name TEMPLATE, e.g. trip_{seq}")?
                .to_string(),
        },
        _ => Target::Dated {
            root: PathBuf::from(options.get("--into").ok_or("sort needs --into DIR")?),
            template: options
                .get("--by")
                .unwrap_or(&"{yyyy}/{yyyy}-{mm}-{dd}")
                .to_string(),
        },
    };
    let plan = files::plan(&photos, &target)?;
    let done = if dry_run {
        None
    } else {
        Some(files::apply(&plan))
    };
    if let Some(d) = &done {
        follow(&d.moves, options);
    }
    if json {
        return print_json(&serde_json::json!({ "plan": plan, "done": done }));
    }
    for p in &plan {
        println!("{} -> {}", p.photo.display(), p.to.display());
    }
    match done {
        None => eprintln!(
            "Would move {} ({} files); without --dry-run it happens",
            photos_n(plan.len()),
            plan.iter().map(|p| p.files.len()).sum::<usize>()
        ),
        Some(d) => {
            for e in &d.errors {
                eprintln!("error: {e}");
            }
            eprintln!("Moved {} ({} files)", photos_n(d.photos), d.files);
            if !d.errors.is_empty() {
                return Err(format!("{} couldn't be moved", d.errors.len()));
            }
        }
    }
    Ok(())
}

/// The catalogue and collections follow moved files.
fn follow(moves: &[(PathBuf, PathBuf)], options: &Options) {
    if let Ok(cat) = open_catalog(options) {
        cat.forget(&moves.iter().map(|m| m.0.clone()).collect::<Vec<_>>());
    }
    if let Ok(dir) = collections_dir(options) {
        collections::relink(&dir, moves);
    }
}

/// `amb trash`: photos with all their files to the system's trash.
pub(crate) fn trash(args: &[&str], options: &Options, dry_run: bool) -> Result<(), String> {
    let photos = photos(args)?;
    if dry_run {
        for p in &photos {
            for f in files::companions(p) {
                println!("{}", f.display());
            }
        }
        eprintln!("Would move these to the trash");
        return Ok(());
    }
    let n = files::trash(&photos)?;
    if let Ok(cat) = open_catalog(options) {
        cat.forget(&photos);
    }
    eprintln!("Moved {} ({n} files) to the trash", photos_n(photos.len()));
    Ok(())
}

/// `amb catalog missing` and `amb catalog relink <DIR>`.
pub(crate) fn missing(relink: Option<&str>, options: &Options, json: bool) -> Result<(), String> {
    let cat = open_catalog(options)?;
    let missing = cat.missing();
    let Some(dir) = relink else {
        if json {
            return print_json(
                &serde_json::to_value(missing.iter().map(|m| &m.0).collect::<Vec<_>>())
                    .map_err(|e| e.to_string())?,
            );
        }
        for (p, _) in &missing {
            println!("{}", p.display());
        }
        eprintln!(
            "{} not where the catalogue last saw them",
            photos_n(missing.len())
        );
        return Ok(());
    };
    // Absolute, as the catalogue and collections keep them.
    let dir = std::path::absolute(dir).map_err(|e| format!("{dir}: {e}"))?;
    let found = files::find_moved(&missing, &dir);
    follow(&found, options);
    cat.records(&found.iter().map(|f| f.1.clone()).collect::<Vec<_>>());
    if json {
        return print_json(&serde_json::to_value(&found).map_err(|e| e.to_string())?);
    }
    for (a, b) in &found {
        println!("{} -> {}", a.display(), b.display());
    }
    eprintln!(
        "Found {} of {} missing",
        found.len(),
        photos_n(missing.len())
    );
    Ok(())
}

/// `amb keywords`: the catalogue's keywords; rename, merge or remove them
/// everywhere.
pub(crate) fn keywords(
    args: &[&str],
    options: &Options,
    dry_run: bool,
    json: bool,
) -> Result<(), String> {
    let cat = open_catalog(options)?;
    let paths: Vec<PathBuf> = cat
        .all()
        .into_iter()
        .filter(|e| !e.offline)
        .map(|e| e.path)
        .collect();
    let records = cat.records(&paths);
    match args {
        [] | ["list"] => {
            let mut counts: BTreeMap<String, (String, usize)> = BTreeMap::new();
            for r in &records {
                for k in &r.info.keywords {
                    counts
                        .entry(k.to_lowercase())
                        .or_insert_with(|| (k.clone(), 0))
                        .1 += 1;
                }
            }
            if json {
                return print_json(
                    &serde_json::to_value(counts.values().collect::<Vec<_>>())
                        .map_err(|e| e.to_string())?,
                );
            }
            for (k, n) in counts.values() {
                println!("{n:>6}  {k}");
            }
            Ok(())
        }
        [verb @ ("rename" | "remove"), from @ ..] if !from.is_empty() => {
            let to = match *verb {
                "rename" => Some(
                    options
                        .get("--to")
                        .ok_or("keywords rename needs --to NEW")?
                        .to_string(),
                ),
                _ => None,
            };
            let wanted: Vec<String> = from.iter().map(|k| k.to_lowercase()).collect();
            let mut changed = Vec::new();
            for (p, r) in paths.iter().zip(&records) {
                if !r
                    .info
                    .keywords
                    .iter()
                    .any(|k| wanted.contains(&k.to_lowercase()))
                {
                    continue;
                }
                if !dry_run {
                    let mut info = read_info(p);
                    info.keywords
                        .retain(|k| !wanted.contains(&k.to_lowercase()));
                    if let Some(t) = &to {
                        if !info.keywords.iter().any(|k| k.eq_ignore_ascii_case(t)) {
                            info.keywords.push(t.clone());
                        }
                    }
                    write_info(p, &info)?;
                }
                changed.push(p);
            }
            if json {
                return print_json(&serde_json::to_value(&changed).map_err(|e| e.to_string())?);
            }
            eprintln!(
                "{} {}",
                if dry_run { "Would change" } else { "Changed" },
                photos_n(changed.len())
            );
            Ok(())
        }
        _ => Err("keywords [list] | rename <K>... --to <NEW> | remove <K>...".into()),
    }
}

/// `amb collection`: the editor's collections.
pub(crate) fn collection(args: &[&str], options: &Options, json: bool) -> Result<(), String> {
    let dir = collections_dir(options)?;
    match args {
        [] | ["list"] => {
            let all = collections::list(&dir);
            if json {
                return print_json(&serde_json::to_value(&all).map_err(|e| e.to_string())?);
            }
            for c in all {
                println!("{:>5}  {}{}", c.count, c.set.map(|s| format!("{s} / ")).unwrap_or_default(), c.name);
            }
            Ok(())
        }
        ["show", name] => {
            let (_, c) = collections::find(&dir, name).ok_or(format!("no collection {name:?}"))?;
            if json {
                return print_json(&serde_json::to_value(&c.photos).map_err(|e| e.to_string())?);
            }
            for p in &c.photos {
                println!("{}{}", p.display(), if p.exists() { "" } else { "  (missing)" });
            }
            Ok(())
        }
        ["add", name, rest @ ..] if !rest.is_empty() => {
            let c = collections::add(&dir, name, &photos(rest)?)?;
            eprintln!("{} in {:?}", photos_n(c.photos.len()), c.name);
            Ok(())
        }
        ["remove", name, rest @ ..] if !rest.is_empty() => {
            // Gone files can be taken out too, so they aren't checked.
            let paths: Vec<PathBuf> = rest
                .iter()
                .map(|p| std::path::absolute(p).unwrap_or_else(|_| PathBuf::from(p)))
                .collect();
            let c = collections::remove(&dir, name, &paths)?;
            eprintln!("{} in {:?}", photos_n(c.photos.len()), c.name);
            Ok(())
        }
        ["delete", name] => collections::delete(&dir, name),
        _ => Err("collection [list] | show <NAME> | add <NAME> <PHOTO>... | remove <NAME> <PHOTO>... | delete <NAME>".into()),
    }
}

/// The change effects a slideshow knows (the editor's `ui/slideshow.js`).
pub(crate) const SLIDE_EFFECTS: [&str; 6] = ["none", "fade", "slide", "zoom", "black", "random"];

/// `amb slideshow`: opens the editor straight into a slideshow of the photos
/// (folders as the photos in them), with the slideshow dialog's options.
pub(crate) fn slideshow(args: &[&str], options: &Options, dry_run: bool) -> Result<(), String> {
    let photos = photos(args)?;
    if photos.is_empty() {
        return Err("no photos to show".into());
    }
    let seconds = options.get("--seconds").copied().unwrap_or("4");
    if !seconds.parse::<u32>().is_ok_and(|s| (1..=600).contains(&s)) {
        return Err(format!("--seconds {seconds}: a whole number from 1 to 600"));
    }
    let effect = options.get("--effect").copied().unwrap_or("fade");
    if !SLIDE_EFFECTS.contains(&effect) {
        return Err(format!(
            "--effect {effect}: one of {}",
            SLIDE_EFFECTS.join(", ")
        ));
    }
    let mut gui_args = vec![
        "--slideshow".to_string(),
        "--seconds".into(),
        seconds.into(),
        "--effect".into(),
        effect.into(),
    ];
    for flag in ["--shuffle", "--window"] {
        if options.contains_key(flag) {
            gui_args.push(flag.into());
        }
    }
    // Folders stay folders, so the command line stays short.
    for a in args {
        let p = std::path::absolute(a).map_err(|e| format!("{a}: {e}"))?;
        gui_args.push(p.to_string_lossy().into_owned());
    }
    let editor = editor_path();
    if dry_run {
        println!("{} {}", editor.display(), gui_args.join(" "));
        eprintln!("Would show {}", photos_n(photos.len()));
        return Ok(());
    }
    std::process::Command::new(&editor)
        .args(&gui_args)
        .spawn()
        .map_err(|e| {
            format!(
                "couldn't start the editor ({}): {e}; set AMB_EDITOR to its path",
                editor.display()
            )
        })?;
    eprintln!("Showing {}", photos_n(photos.len()));
    Ok(())
}

/// The editor: AMB_EDITOR, else next to `amb` (where the installers put
/// both), else `ambrotype` on the PATH.
fn editor_path() -> PathBuf {
    if let Some(p) = std::env::var_os("AMB_EDITOR") {
        return PathBuf::from(p);
    }
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf))
    {
        for name in ["ambrotype", "Ambrotype"] {
            let p = dir.join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
            if p.is_file() {
                return p;
            }
        }
    }
    PathBuf::from("ambrotype")
}
