// SPDX-License-Identifier: GPL-3.0-or-later
//! How fast the catalogue is on a big library: a synthetic one of tiny JPEGs
//! (100 000 by default, `AMB_SPEED_PHOTOS` to change) is indexed, re-indexed,
//! listed, filtered and searched, each step timed. Run by hand, see
//! docs/design/library.md; it checks counts, never a time.

use amb_library::catalog::{Catalog, Query};
use amb_library::info::write_info;
use amb_protocol::sidecar::PhotoInfo;
use rayon::prelude::*;
use std::path::PathBuf;
use std::time::Instant;

/// Photos per folder, as a camera card's folders run.
const PER_FOLDER: usize = 1000;
/// Of every ten photos, this many get a sidecar.
const SIDECARS_IN_TEN: usize = 3;
const KEYWORD: &str = "speedkw";
const MIN_RATING: u8 = 3;

fn tiny_jpeg() -> Vec<u8> {
    let img = image::RgbImage::from_fn(64, 48, |x, y| {
        image::Rgb([(x * 4) as u8, (y * 5) as u8, 90])
    });
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Jpeg)
        .expect("encoding a small image in memory");
    out.into_inner()
}

/// The rating of photo `i`, if it has a sidecar.
fn rating_of(i: usize) -> Option<u8> {
    (i % 10 < SIDECARS_IN_TEN).then(|| (i % 5) as u8 + 1)
}

#[test]
#[ignore = "slow: run by hand, see docs/design/library.md"]
fn library_speed() {
    let n: usize = std::env::var("AMB_SPEED_PHOTOS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000);
    let root = std::env::temp_dir().join(format!("amb-speed-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let lib = root.join("photos");

    let made = Instant::now();
    let jpeg = tiny_jpeg();
    let folders = n.div_ceil(PER_FOLDER);
    for f in 0..folders {
        std::fs::create_dir_all(lib.join(format!("2024/folder-{f:03}"))).unwrap();
    }
    let paths: Vec<PathBuf> = (0..n)
        .map(|i| lib.join(format!("2024/folder-{:03}/IMG_{i:06}.jpg", i / PER_FOLDER)))
        .collect();
    paths.par_iter().enumerate().for_each(|(i, p)| {
        std::fs::write(p, &jpeg).unwrap();
        if let Some(rating) = rating_of(i) {
            let info = PhotoInfo {
                rating: Some(rating),
                label: Some("Red".into()),
                keywords: vec![KEYWORD.into()],
                ..Default::default()
            };
            write_info(p, &info).unwrap();
        }
    });
    println!("made {n} photos in {:.1} s", made.elapsed().as_secs_f64());
    let with_sidecar = (0..n).filter(|&i| rating_of(i).is_some()).count();
    let rated = (0..n)
        .filter(|&i| rating_of(i).is_some_and(|r| r >= MIN_RATING))
        .count();

    let mut table: Vec<(&str, f64, String)> = Vec::new();
    let mut step = |name: &'static str, secs: f64, note: String| {
        println!("{name:<28} {secs:>9.3} s  {note}");
        table.push((name, secs, note));
    };

    let db = root.join("catalog.sqlite");
    let cat = Catalog::open(&db).unwrap();
    let t = Instant::now();
    let first = cat.index(&lib, &|_, _| {}).unwrap();
    let secs = t.elapsed().as_secs_f64();
    assert_eq!(first.photos, n);
    assert_eq!(first.read, n);
    step(
        "first index",
        secs,
        format!("{:.0} photos/s", n as f64 / secs),
    );
    drop(cat);

    let t = Instant::now();
    let cat = Catalog::open(&db).unwrap();
    let again = cat.index(&lib, &|_, _| {}).unwrap();
    let secs = t.elapsed().as_secs_f64();
    assert_eq!(again.photos, n);
    assert_eq!(again.read, 0, "nothing changed, nothing to read");
    step("re-index, nothing changed", secs, "open + index".into());

    let t = Instant::now();
    let folder_list = cat.folders();
    step(
        "folders()",
        t.elapsed().as_secs_f64(),
        format!("{} folders", folder_list.len()),
    );
    assert_eq!(folder_list.len(), folders);

    let t = Instant::now();
    let found = cat.find(&Query {
        rating: Some(MIN_RATING),
        ..Default::default()
    });
    step(
        "find rating >= 3",
        t.elapsed().as_secs_f64(),
        format!("{} found", found.len()),
    );
    assert_eq!(found.len(), rated);

    let t = Instant::now();
    let found = cat.find(&Query {
        text: Some(KEYWORD.into()),
        ..Default::default()
    });
    step(
        "find text",
        t.elapsed().as_secs_f64(),
        format!("{} found", found.len()),
    );
    assert_eq!(found.len(), with_sidecar);

    let t = Instant::now();
    let found = cat.find(&Query {
        keyword: Some(KEYWORD.into()),
        ..Default::default()
    });
    step(
        "find keyword",
        t.elapsed().as_secs_f64(),
        format!("{} found", found.len()),
    );
    assert_eq!(found.len(), with_sidecar);

    let t = Instant::now();
    let everything = cat.all();
    step(
        "all()",
        t.elapsed().as_secs_f64(),
        format!("{} photos", everything.len()),
    );
    assert_eq!(everything.len(), n);

    let cores = std::thread::available_parallelism().map_or(1, |c| c.get());
    println!(
        "\n{n} photos, {cores} cores, {} {}",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    println!("| Step | Seconds | Note |\n|---|---|---|");
    for (name, secs, note) in &table {
        println!("| {name} | {secs:.2} | {note} |");
    }
    let _ = std::fs::remove_dir_all(&root);
}
