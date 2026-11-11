// SPDX-License-Identifier: GPL-3.0-or-later
//! `amb place`: where photos were taken, from a GPS track (`--gpx`) or
//! guessed from the photos that have a place (`--guess`).

use std::path::{Path, PathBuf};

use crate::organise::{Options, photos, read_info, write_info};
use crate::print_json;

/// `amb place --gpx`: where photos were taken, from a GPS track.
pub(crate) fn place(
    args: &[&str],
    options: &Options,
    dry_run: bool,
    json: bool,
) -> Result<(), String> {
    let photos = photos(args)?;
    if options.contains_key("--guess") {
        return guess_places(photos, options, dry_run, json);
    }
    let gpx = options
        .get("--gpx")
        .ok_or("place needs --gpx TRACK.gpx or --guess")?;
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

/// `amb place --guess`: places from the photos that have one (by time, or
/// by look for photos the search model indexed); see amb_library::places.
fn guess_places(
    photos: Vec<PathBuf>,
    options: &Options,
    dry_run: bool,
    json: bool,
) -> Result<(), String> {
    let number = |key: &str, default: f64| -> Result<f64, String> {
        options.get(key).map_or(Ok(default), |v| {
            v.trim_start_matches('+')
                .replace('−', "-")
                .parse()
                .map_err(|_| format!("{key} {v}: a number"))
        })
    };
    let within = number("--within", 30.0)?;
    let clock = number("--clock", 0.0)?;
    let cat = match options.get("--catalog") {
        Some(p) => amb_library::catalog::Catalog::open(Path::new(p))?,
        None => amb_library::catalog::Catalog::open(
            &amb_library::catalog::default_path().ok_or("no config folder: give --catalog FILE")?,
        )?,
    };
    let records = cat.records(&photos);
    let model = crate::models::models_dir()
        .ok()
        .and_then(|d| amb_ai::chosen(&d, "search").ok())
        .map(|s| s.id);
    let vectors = match &model {
        Some(m) => cat.vectors(m, &photos),
        None => vec![None; photos.len()],
    };
    let list: Vec<_> = photos
        .into_iter()
        .zip(records)
        .zip(vectors)
        .map(|((p, r), v)| (p, r, v))
        .collect();
    let guesses = amb_library::places::guess(&list, within, clock * 60.0);
    if !dry_run {
        for g in &guesses {
            let mut info = read_info(&g.path);
            info.gps = Some(format!("{:.6},{:.6}", g.lat, g.lon));
            write_info(&g.path, &info)?;
        }
    }
    if json {
        return print_json(&serde_json::json!({ "guesses": guesses, "written": !dry_run }));
    }
    for g in &guesses {
        println!("{}  {:.5},{:.5}  {}", g.path.display(), g.lat, g.lon, g.why);
    }
    let without = list.iter().filter(|(_, r, _)| r.place().is_none()).count();
    eprintln!(
        "{} {} of {without} without a place",
        if dry_run { "Would place" } else { "Placed" },
        guesses.len()
    );
    Ok(())
}
