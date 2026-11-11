// SPDX-License-Identifier: GPL-3.0-or-later
//! Clean up: the photos you may not want, each with why. Copies of the
//! same file and alike pictures (all but the best of each group), the
//! weaker frames of each burst (by focus and open eyes), blurry photos,
//! closed eyes, photos nearly black or blown out, and the ones already
//! rejected. Photos you picked or rated are left out: you chose them.
//!
//! It only finds them; moving them to the trash (or rejecting them) is
//! the caller's, after someone has looked.

use crate::burst::{self, Look};
use amb_library::catalog::Catalog;
use amb_library::duplicates::{self, Kind, Thumb};
use amb_protocol::sidecar::Flag;
use rayon::prelude::*;
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

/// Which tests to run.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Options {
    /// Copies of the same file, and alike pictures (fingerprints that
    /// differ in at most `alike_bits` of 64; 0: same files only).
    pub duplicates: bool,
    pub alike_bits: u32,
    /// Photos this close in time (seconds) are a burst; 0 leaves bursts out.
    pub burst_seconds: f64,
    pub blurry: bool,
    pub eyes: bool,
    /// Nearly black, or mostly blown out.
    pub exposure: bool,
    pub rejected: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            duplicates: true,
            alike_bits: duplicates::Options::default().max_bits,
            burst_seconds: 2.0,
            blurry: true,
            eyes: true,
            exposure: true,
            rejected: true,
        }
    }
}

/// Why a photo may go.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Reason {
    /// A copy of the same file as `keep`.
    Same {
        keep: PathBuf,
    },
    /// Alike `keep`, which is sharper.
    Alike {
        keep: PathBuf,
    },
    /// A weaker frame of the burst `keep` is the best of; `note` says how.
    Burst {
        keep: PathBuf,
        note: String,
    },
    Blurry,
    Eyes,
    Dark,
    Bright,
    Rejected,
}

impl Reason {
    /// In a few words, for people.
    pub fn text(&self) -> String {
        match self {
            Reason::Same { .. } => "a copy of the same file".into(),
            Reason::Alike { .. } => "alike a sharper photo".into(),
            Reason::Burst { note, .. } if note.is_empty() => "not the best of its burst".into(),
            Reason::Burst { note, .. } => format!("not the best of its burst ({note})"),
            Reason::Blurry => "blurry".into(),
            Reason::Eyes => "eyes closed".into(),
            Reason::Dark => "nearly black".into(),
            Reason::Bright => "blown out".into(),
            Reason::Rejected => "rejected".into(),
        }
    }

    /// The photo kept instead, for duplicates and bursts.
    pub fn keep(&self) -> Option<&Path> {
        match self {
            Reason::Same { keep } | Reason::Alike { keep } | Reason::Burst { keep, .. } => {
                Some(keep)
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Suspect {
    pub path: PathBuf,
    pub reasons: Vec<Reason>,
}

/// A photo blurrier than this share of the shoot's middle sharpness is
/// blurry (with enough photos to tell what the middle is).
const SOFT_SHARE: f32 = 0.25;
const SHOOT: usize = 6;
/// Below this sharpness (the variance of the Laplacian of a 1280-pixel
/// preview, at its sharpest part) a photo is blurry whatever the rest.
const BLURRY: f32 = 3.0e-4;
/// Eyes less open than this (0–1) look closed in a photo on its own; in a
/// burst the frames are compared instead.
const CLOSED: f32 = 0.3;

/// How a photo looks, for the tests.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Seen {
    pub look: Look,
    /// Share of pixels nearly black (all channels under 16) and nearly
    /// white (all over 245).
    pub black: f32,
    pub white: f32,
}

/// Look at `rgb` (an upright preview about 1280 pixels across).
pub fn see(rgb: &image::RgbImage) -> Seen {
    let n = (rgb.width() * rgb.height()).max(1) as f32;
    let (mut black, mut white) = (0usize, 0usize);
    for p in rgb.pixels() {
        if p.0.iter().all(|&v| v < 16) {
            black += 1;
        } else if p.0.iter().all(|&v| v > 245) {
            white += 1;
        }
    }
    Seen {
        look: burst::look(rgb),
        black: black as f32 / n,
        white: white as f32 / n,
    }
}

/// The photos among `photos` that may go, in the order given. `small` is a
/// thumbnail (256 pixels is plenty) and `big` a preview about 1280 pixels
/// across; `progress(done, total)` is called as photos are looked at.
pub fn find(
    cat: &Catalog,
    photos: &[PathBuf],
    options: Options,
    small: Thumb,
    big: &(dyn Fn(&Path) -> Option<image::RgbImage> + Sync),
    progress: &(dyn Fn(usize, usize) + Sync),
) -> Vec<Suspect> {
    let records = cat.records(photos);
    // Photos picked or rated were chosen; leave them be.
    let chosen = |i: usize| {
        let info = &records[i].info;
        info.flag == Some(Flag::Pick) || info.rating.is_some_and(|r| r > 0)
    };
    let mut reasons: HashMap<&Path, Vec<Reason>> = HashMap::new();
    let groups = if options.duplicates || options.burst_seconds > 0.0 {
        let find = duplicates::Options {
            burst_seconds: options.burst_seconds.max(0.0),
            max_bits: if options.duplicates {
                options.alike_bits.min(20)
            } else {
                0
            },
        };
        duplicates::find(cat, photos, find, small, &|_, _| {})
            .into_iter()
            .filter(|g| options.duplicates || g.kind == Kind::Burst)
            .filter(|g| options.burst_seconds > 0.0 || g.kind != Kind::Burst)
            .collect()
    } else {
        Vec::new()
    };
    // Looking takes the time: only when a test needs it.
    let looking = options.blurry
        || options.eyes
        || options.exposure
        || groups.iter().any(|g| g.kind == Kind::Burst);
    let done = AtomicUsize::new(0);
    let seen: Vec<Option<Seen>> = if looking {
        photos
            .par_iter()
            .map(|p| {
                let s = big(p).map(|img| see(&img));
                progress(done.fetch_add(1, Ordering::Relaxed) + 1, photos.len());
                s
            })
            .collect()
    } else {
        vec![None; photos.len()]
    };
    let index: HashMap<&Path, usize> = photos
        .iter()
        .enumerate()
        .map(|(i, p)| (p.as_path(), i))
        .collect();
    let mut in_burst = vec![false; photos.len()];
    for g in &groups {
        let at: Vec<usize> = g
            .photos
            .iter()
            .filter_map(|p| index.get(p.as_path()).copied())
            .collect();
        match g.kind {
            Kind::Same | Kind::Similar => {
                let keep = g.photos[0].clone();
                for p in &g.photos[1..] {
                    reasons.entry(p).or_default().push(if g.kind == Kind::Same {
                        Reason::Same { keep: keep.clone() }
                    } else {
                        Reason::Alike { keep: keep.clone() }
                    });
                }
            }
            Kind::Burst => {
                let looks: Vec<Look> = at
                    .iter()
                    .map(|&i| seen[i].map_or(NO_LOOK, |s| s.look))
                    .collect();
                for &i in &at {
                    in_burst[i] = true;
                }
                let ranked = burst::rank(&looks);
                let keep = photos[at[ranked[0].index]].clone();
                for r in &ranked[1..] {
                    reasons
                        .entry(&photos[at[r.index]])
                        .or_default()
                        .push(Reason::Burst {
                            keep: keep.clone(),
                            note: r.note.clone(),
                        });
                }
            }
        }
    }
    // The shoot's middle sharpness, for "blurrier than the rest".
    let mut sharps: Vec<f32> = seen.iter().flatten().map(|s| s.look.sharp).collect();
    sharps.sort_by(f32::total_cmp);
    let middle = (sharps.len() >= SHOOT).then(|| sharps[sharps.len() / 2]);
    for (i, s) in seen.iter().enumerate() {
        let Some(s) = s else { continue };
        let mine = reasons.entry(&photos[i]).or_default();
        // A black or white frame has nothing to be sharp; that is said instead.
        let (dark, bright) = (s.black > 0.9, s.white > 0.6);
        if options.exposure && dark {
            mine.push(Reason::Dark);
        } else if options.exposure && bright {
            mine.push(Reason::Bright);
        }
        let soft = s.look.sharp < BLURRY || middle.is_some_and(|m| s.look.sharp < m * SOFT_SHARE);
        if options.blurry && soft && !dark && !bright {
            mine.push(Reason::Blurry);
        }
        if options.eyes && !in_burst[i] && s.look.eyes.is_some_and(|e| e < CLOSED) {
            mine.push(Reason::Eyes);
        }
    }
    photos
        .iter()
        .enumerate()
        .filter(|&(i, _)| !chosen(i))
        .filter_map(|(i, p)| {
            let mut why = reasons.remove(p.as_path()).unwrap_or_default();
            if options.rejected && records[i].info.flag == Some(Flag::Reject) {
                why.push(Reason::Rejected);
            }
            (!why.is_empty()).then(|| Suspect {
                path: p.clone(),
                reasons: why,
            })
        })
        .collect()
}

const NO_LOOK: Look = Look {
    sharp: 0.0,
    eyes: None,
    faces: 0,
};

#[cfg(test)]
mod tests {
    use super::*;

    fn checks(w: u32, h: u32, size: u32) -> image::RgbImage {
        image::RgbImage::from_fn(w, h, |x, y| {
            let v = if (x / size + y / size) % 2 == 0 {
                210
            } else {
                40
            };
            image::Rgb([v, v, v / 2])
        })
    }

    #[test]
    fn black_and_white_frames_are_seen() {
        let black = image::RgbImage::from_pixel(64, 48, image::Rgb([3, 4, 2]));
        let white = image::RgbImage::from_pixel(64, 48, image::Rgb([255, 252, 250]));
        assert!(see(&black).black > 0.99);
        assert!(see(&white).white > 0.99);
        let fine = see(&checks(64, 48, 4));
        assert_eq!((fine.black, fine.white), (0.0, 0.0));
    }

    #[test]
    fn finds_the_blurry_dark_and_rejected_and_leaves_the_chosen() {
        let dir = std::env::temp_dir().join(format!("amb-cleanup-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let cat = Catalog::in_memory();
        let names = [
            "a", "b", "c", "d", "e", "f", "soft", "dark", "rejected", "picked",
        ];
        let photos: Vec<PathBuf> = names.iter().map(|n| dir.join(format!("{n}.jpg"))).collect();
        let pictures: HashMap<PathBuf, image::RgbImage> = photos
            .iter()
            .zip(names)
            .enumerate()
            .map(|(i, (p, n))| {
                // Each a different pattern, so none are alike.
                let sharp = checks(320, 200, 3 + i as u32);
                let img = match n {
                    "soft" => image::imageops::blur(&sharp, 6.0),
                    "dark" => image::RgbImage::from_pixel(320, 200, image::Rgb([2, 2, 2])),
                    _ => sharp,
                };
                img.save(p).unwrap();
                (p.clone(), img)
            })
            .collect();
        let flag = |n: &str, f: Flag| {
            let p = dir.join(format!("{n}.jpg"));
            let mut info = amb_library::catalog::read(&p).info;
            info.flag = Some(f);
            let text = amb_protocol::sidecar::with_info(None, &info).unwrap();
            amb_protocol::sidecar::write_atomic(&amb_engine::sidecar_path(&p), &text).unwrap();
        };
        flag("rejected", Flag::Reject);
        flag("picked", Flag::Pick);
        let options = Options {
            duplicates: false,
            burst_seconds: 0.0,
            ..Options::default()
        };
        let got = find(
            &cat,
            &photos,
            options,
            &|_| None,
            &|p| pictures.get(p).cloned(),
            &|_, _| {},
        );
        let why: HashMap<String, Vec<Reason>> = got
            .into_iter()
            .map(|s| {
                (
                    s.path.file_stem().unwrap().to_string_lossy().into_owned(),
                    s.reasons,
                )
            })
            .collect();
        assert_eq!(why.get("soft"), Some(&vec![Reason::Blurry]), "{why:?}");
        assert_eq!(why.get("dark"), Some(&vec![Reason::Dark]), "{why:?}");
        assert_eq!(
            why.get("rejected"),
            Some(&vec![Reason::Rejected]),
            "{why:?}"
        );
        assert!(
            !why.contains_key("picked") && !why.contains_key("a"),
            "{why:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Run with AMB_CLEANUP_CAL=photo.jpg to see a real photo's numbers.
    #[test]
    fn calibrate() {
        let Ok(p) = std::env::var("AMB_CLEANUP_CAL") else {
            return;
        };
        let img = image::open(p).unwrap().to_rgb8();
        for sigma in [0.0, 1.0, 2.0, 4.0] {
            let b = if sigma > 0.0 {
                image::imageops::blur(&img, sigma)
            } else {
                img.clone()
            };
            eprintln!("blur {sigma}: {:?}", see(&b));
        }
    }
}
