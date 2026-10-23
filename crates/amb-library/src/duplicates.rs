// SPDX-License-Identifier: GPL-3.0-or-later
//! Finding photos that are the same, alike, or one burst, so all but the
//! best can be stacked or rejected.
//!
//! - **Same:** files with identical bytes (a copy imported twice). Only
//!   files of equal size are read whole and hashed (SHA-256).
//! - **Similar:** pictures whose difference hashes (a 64-bit fingerprint of
//!   a small greyscale copy, from the camera's embedded preview) differ in
//!   at most `Options::max_bits` bits: the same shot resized or re-saved, or
//!   frames of one scene.
//! - **Burst:** photos from one camera each taken within
//!   `Options::burst_seconds` of the one before.
//!
//! What's measured is kept in the catalogue while the file is unchanged, so
//! a second look is quick. Virtual copies are left out (they share their
//! photo's file). In each group the best photo comes first: a pick before
//! others and a reject last, then the highest rating, edited, the sharpest
//! picture, and the largest file.

use crate::catalog::{Catalog, Print, Record};
use rayon::prelude::*;
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Options {
    /// Photos this close in time (seconds) are a burst; 0 leaves bursts out.
    pub burst_seconds: f64,
    /// Pictures whose fingerprints differ in at most this many of 64 bits
    /// are alike; 0 finds only identical files.
    pub max_bits: u32,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            burst_seconds: 2.0,
            max_bits: 6,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Same,
    Similar,
    Burst,
}

#[derive(Debug, Clone, Serialize)]
pub struct Group {
    pub kind: Kind,
    /// Best first.
    pub photos: Vec<PathBuf>,
}

/// A small upright picture of a photo (any size; 256 pixels is plenty).
pub type Thumb<'a> = &'a (dyn Fn(&Path) -> Option<image::RgbImage> + Sync);

/// Above this many photos, alike pictures are looked for only among
/// neighbours in time, not across every pair.
const ALL_PAIRS: usize = 20_000;
const NEIGHBOURS: usize = 200;

/// The groups among `photos`, in time order. `progress(done, total)` is
/// called as files are measured.
pub fn find(
    catalog: &Catalog,
    photos: &[PathBuf],
    options: Options,
    thumb: Thumb,
    progress: &(dyn Fn(usize, usize) + Sync),
) -> Vec<Group> {
    let photos: Vec<PathBuf> = photos
        .iter()
        .filter(|p| amb_engine::virtual_copy_of(p).is_none())
        .cloned()
        .collect();
    let records = catalog.records(&photos);
    let sizes: Vec<u64> = photos
        .par_iter()
        .map(|p| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0))
        .collect();
    let mut by_size: HashMap<u64, usize> = HashMap::new();
    for &s in sizes.iter().filter(|&&s| s > 0) {
        *by_size.entry(s).or_default() += 1;
    }
    let need_sha: Vec<bool> = sizes.iter().map(|s| by_size.get(s) > Some(&1)).collect();
    // Measure what isn't kept yet.
    let mut prints: Vec<Print> = catalog
        .prints(&photos)
        .into_iter()
        .map(Option::unwrap_or_default)
        .collect();
    let todo: Vec<usize> = (0..photos.len())
        .filter(|&i| prints[i].dhash.is_none() || (need_sha[i] && prints[i].sha256.is_none()))
        .collect();
    let done = AtomicUsize::new(0);
    progress(0, todo.len());
    let measured: Vec<(usize, Print)> = todo
        .par_iter()
        .map(|&i| {
            let mut p = prints[i].clone();
            if p.dhash.is_none() {
                if let Some(img) = thumb(&photos[i]) {
                    let grey = image::imageops::grayscale(&img);
                    p.dhash = Some(dhash(&grey));
                    p.sharpness = Some(sharpness(&grey));
                }
            }
            if need_sha[i] && p.sha256.is_none() {
                p.sha256 = file_sha256(&amb_engine::source_path(&photos[i]));
            }
            progress(done.fetch_add(1, Ordering::Relaxed) + 1, todo.len());
            (i, p)
        })
        .collect();
    catalog.store_prints(
        &measured
            .iter()
            .map(|(i, p)| (photos[*i].clone(), p.clone()))
            .collect::<Vec<_>>(),
    );
    for (i, p) in measured {
        prints[i] = p;
    }
    group(&photos, &records, &prints, &sizes, options)
}

/// Group photos from what was measured (the part that needs no files).
fn group(
    photos: &[PathBuf],
    records: &[Record],
    prints: &[Print],
    sizes: &[u64],
    options: Options,
) -> Vec<Group> {
    let n = photos.len();
    // Same bytes: their own groups; then only the first of each takes part
    // in finding alike pictures and bursts.
    let mut by_sha: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, p) in prints.iter().enumerate() {
        if let Some(sha) = p.sha256.as_deref().filter(|_| sizes[i] > 0) {
            by_sha.entry(sha).or_default().push(i);
        }
    }
    let mut copy = vec![false; n];
    let mut same: Vec<Vec<usize>> = by_sha.into_values().filter(|g| g.len() > 1).collect();
    for g in &mut same {
        g.sort_by(|&a, &b| better(a, b, photos, records, prints, sizes));
        for &i in &g[1..] {
            copy[i] = true;
        }
    }
    let mut sets = Sets::new(n);
    let mut similar_edge = vec![false; n];
    let mut burst_edge = vec![false; n];
    // In time order, for bursts and for neighbours.
    let time = |i: usize| {
        records[i]
            .shot
            .captured
            .as_deref()
            .and_then(crate::gpx::local_seconds)
    };
    let mut order: Vec<usize> = (0..n).filter(|&i| !copy[i]).collect();
    order.sort_by_key(|&i| (time(i).unwrap_or(i64::MAX), photos[i].clone()));
    if options.max_bits > 0 {
        let hashed: Vec<usize> = order
            .iter()
            .copied()
            .filter(|&i| prints[i].dhash.is_some())
            .collect();
        let reach = if hashed.len() <= ALL_PAIRS {
            hashed.len()
        } else {
            NEIGHBOURS
        };
        for (k, &a) in hashed.iter().enumerate() {
            for &b in hashed.iter().skip(k + 1).take(reach) {
                let (Some(x), Some(y)) = (prints[a].dhash, prints[b].dhash) else {
                    continue;
                };
                if (x ^ y).count_ones() <= options.max_bits {
                    sets.join(a, b);
                    similar_edge[a] = true;
                    similar_edge[b] = true;
                }
            }
        }
    }
    if options.burst_seconds > 0.0 {
        for w in order.windows(2) {
            let (a, b) = (w[0], w[1]);
            let (Some(ta), Some(tb)) = (time(a), time(b)) else {
                continue;
            };
            let same_camera = records[a].shot.camera == records[b].shot.camera;
            if same_camera && (tb - ta) as f64 <= options.burst_seconds {
                sets.join(a, b);
                burst_edge[a] = true;
                burst_edge[b] = true;
            }
        }
    }
    let mut members: HashMap<usize, Vec<usize>> = HashMap::new();
    for &i in &order {
        members.entry(sets.root(i)).or_default().push(i);
    }
    let place = |i: usize| order.iter().position(|&j| j == i).unwrap_or(usize::MAX);
    let mut groups: Vec<(usize, Group)> = same
        .into_iter()
        .map(|g| {
            (
                place(g[0]),
                Group {
                    kind: Kind::Same,
                    photos: g.into_iter().map(|i| photos[i].clone()).collect(),
                },
            )
        })
        .collect();
    groups.extend(members.into_values().filter(|m| m.len() > 1).map(|mut m| {
        let at = place(m[0]);
        let kind = if m.iter().all(|&i| burst_edge[i]) || !m.iter().any(|&i| similar_edge[i]) {
            Kind::Burst
        } else {
            Kind::Similar
        };
        m.sort_by(|&a, &b| better(a, b, photos, records, prints, sizes));
        (
            at,
            Group {
                kind,
                photos: m.into_iter().map(|i| photos[i].clone()).collect(),
            },
        )
    }));
    groups.sort_by_key(|g| g.0);
    groups.into_iter().map(|g| g.1).collect()
}

/// Better photos first: a pick before others and a reject last, then the
/// highest rating, edited, the sharper picture, the larger file.
fn better(
    a: usize,
    b: usize,
    photos: &[PathBuf],
    records: &[Record],
    prints: &[Print],
    sizes: &[u64],
) -> std::cmp::Ordering {
    use amb_protocol::sidecar::Flag;
    let flag = |i: usize| match records[i].info.flag {
        Some(Flag::Pick) => 2,
        Some(Flag::Reject) => 0,
        None => 1,
    };
    let rating = |i: usize| records[i].info.rating.unwrap_or(0);
    let sharp = |i: usize| prints[i].sharpness.unwrap_or(0.0);
    let name_len = |i: usize| photos[i].file_name().map_or(0, |n| n.len());
    flag(b)
        .cmp(&flag(a))
        .then(rating(b).cmp(&rating(a)))
        .then(records[b].edited.cmp(&records[a].edited))
        .then(sharp(b).total_cmp(&sharp(a)))
        .then(sizes[b].cmp(&sizes[a]))
        // Then the plainer name ("IMG_1.nef" before "IMG_1-2.nef").
        .then(name_len(a).cmp(&name_len(b)))
        .then(a.cmp(&b))
}

/// 64-bit difference hash: a 9×8 greyscale copy, each bit whether a pixel
/// is brighter than its right neighbour.
pub fn dhash(grey: &image::GrayImage) -> u64 {
    let small = image::imageops::resize(grey, 9, 8, image::imageops::FilterType::Triangle);
    let mut h = 0u64;
    for y in 0..8 {
        for x in 0..8 {
            h <<= 1;
            h |= u64::from(small.get_pixel(x, y)[0] > small.get_pixel(x + 1, y)[0]);
        }
    }
    h
}

/// The spread (variance) of a 3×3 Laplacian over a copy at most 256 pixels
/// wide: blur and missed focus lower it. Only for comparing photos of one
/// scene.
pub fn sharpness(grey: &image::GrayImage) -> f32 {
    let (w, h) = grey.dimensions();
    let k = (256.0 / w.max(h) as f32).min(1.0);
    let small = image::imageops::resize(
        grey,
        ((w as f32 * k) as u32).max(3),
        ((h as f32 * k) as u32).max(3),
        image::imageops::FilterType::Triangle,
    );
    let (w, h) = small.dimensions();
    let at = |x: u32, y: u32| f32::from(small.get_pixel(x, y)[0]) / 255.0;
    let (mut sum, mut sum2, mut n) = (0.0f64, 0.0f64, 0.0f64);
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let l = 4.0 * at(x, y) - at(x - 1, y) - at(x + 1, y) - at(x, y - 1) - at(x, y + 1);
            sum += f64::from(l);
            sum2 += f64::from(l * l);
            n += 1.0;
        }
    }
    if n == 0.0 {
        return 0.0;
    }
    (sum2 / n - (sum / n).powi(2)) as f32
}

/// A JPEG (such as a cached thumbnail) as a picture.
pub fn decode_jpeg(bytes: &[u8]) -> Option<image::RgbImage> {
    image::load_from_memory_with_format(bytes, image::ImageFormat::Jpeg)
        .ok()
        .map(|i| i.to_rgb8())
}

fn file_sha256(path: &Path) -> Option<String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut f = std::fs::File::open(path).ok()?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Some(
        hasher
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
    )
}

/// Disjoint sets (union-find).
struct Sets(Vec<usize>);

impl Sets {
    fn new(n: usize) -> Sets {
        Sets((0..n).collect())
    }

    fn root(&mut self, mut i: usize) -> usize {
        while self.0[i] != i {
            self.0[i] = self.0[self.0[i]];
            i = self.0[i];
        }
        i
    }

    fn join(&mut self, a: usize, b: usize) {
        let (a, b) = (self.root(a), self.root(b));
        if a != b {
            self.0[a.max(b)] = a.min(b);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::Shot;

    fn record(captured: &str, rating: u8) -> Record {
        let mut r = Record {
            shot: Shot {
                captured: Some(captured.into()),
                camera: Some("Z 6".into()),
                ..Default::default()
            },
            ..Default::default()
        };
        r.info.rating = Some(rating).filter(|&r| r > 0);
        r
    }

    fn print(sha: Option<&str>, dhash: u64, sharp: f32) -> Print {
        Print {
            sha256: sha.map(str::to_string),
            dhash: Some(dhash),
            sharpness: Some(sharp),
        }
    }

    #[test]
    fn groups_same_files_alike_pictures_and_bursts() {
        let photos: Vec<PathBuf> = ["a", "a-copy", "b", "b-small", "c1", "c2", "c3", "lone"]
            .iter()
            .map(PathBuf::from)
            .collect();
        let records = vec![
            record("2026-07-01 10:00:00", 0),
            record("2026-07-01 10:00:00", 3),
            record("2026-07-02 09:00:00", 0),
            record("2026-07-05 12:00:00", 0),
            record("2026-07-03 15:00:00", 0),
            record("2026-07-03 15:00:01", 0),
            record("2026-07-03 15:00:02", 0),
            record("2026-07-04 08:00:00", 0),
        ];
        let prints = vec![
            print(Some("aa"), 0x00ff, 0.5),
            print(Some("aa"), 0x00ff, 0.5),
            print(None, 0xf0f0_f0f0, 0.5),
            print(None, 0xf0f0_f0f3, 0.4),
            print(None, 0x1111_0000_0000, 0.2),
            print(None, 0x2222_0000_0000_0000, 0.9),
            print(None, 0x4444_0000_0000, 0.5),
            print(None, 0x0f0f_0f0f_0f0f_0f0f, 0.5),
        ];
        let sizes = vec![10, 10, 20, 5, 30, 31, 32, 40];
        let groups = group(&photos, &records, &prints, &sizes, Options::default());
        let names = |g: &Group| {
            g.photos
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
        };
        assert_eq!(groups.len(), 3, "{groups:?}");
        assert_eq!(groups[0].kind, Kind::Same);
        // The rated copy is the better one.
        assert_eq!(names(&groups[0]), ["a-copy", "a"]);
        assert_eq!(groups[1].kind, Kind::Similar);
        // Equal otherwise: the sharper, then the larger file.
        assert_eq!(names(&groups[1]), ["b", "b-small"]);
        assert_eq!(groups[2].kind, Kind::Burst);
        assert_eq!(
            names(&groups[2])[0],
            "c2",
            "the sharpest of the burst first"
        );
        // Bursts off, alike pictures off: only the same files.
        let strict = group(
            &photos,
            &records,
            &prints,
            &sizes,
            Options {
                burst_seconds: 0.0,
                max_bits: 0,
            },
        );
        assert_eq!(strict.len(), 1);
    }

    #[test]
    fn fingerprints_see_alike_pictures() {
        let img = image::GrayImage::from_fn(200, 150, |x, y| {
            image::Luma([((x * 7 + y * 3) % 256) as u8])
        });
        let smaller = image::imageops::resize(&img, 100, 75, image::imageops::FilterType::Triangle);
        assert!((dhash(&img) ^ dhash(&smaller)).count_ones() <= 4);
        let other = image::GrayImage::from_fn(200, 150, |x, y| {
            image::Luma([((x * 3 + y * 11) % 256) as u8])
        });
        assert!((dhash(&img) ^ dhash(&other)).count_ones() > 12);
        let blurred = image::imageops::blur(&img, 3.0);
        assert!(sharpness(&img) > sharpness(&blurred));
    }
}
