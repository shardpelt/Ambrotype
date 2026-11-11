// SPDX-License-Identifier: GPL-3.0-or-later
//! Guessing where photos without a place were taken, from the photos that
//! have one: a camera without GPS next to a phone that has it, or a day
//! where only some photos were placed.
//!
//! - **By time:** taken within `within` minutes of a placed photo, it was
//!   most likely there too. Between two placed photos (before and after) it
//!   goes on the line between them by time, as a GPS track would; the
//!   nearer one alone when only one is close.
//! - **By look:** otherwise, a placed photo that looks the same (the search
//!   model's likeness, for photos already indexed) lends its place.
//!
//! Nothing is written here: the guesses are shown to confirm first. Clocks
//! of different cameras can be apart (time zones); `offset` minutes are
//! added to the unplaced photos' times first.

use crate::catalog::Record;
use crate::gpx::local_seconds;
use serde::Serialize;
use std::path::PathBuf;

/// Where a photo was most likely taken, and why.
#[derive(Debug, Clone, Serialize)]
pub struct Guess {
    pub path: PathBuf,
    pub lat: f64,
    pub lon: f64,
    /// "time" or "look".
    pub by: &'static str,
    /// The placed photo it came from (the nearer one, between two).
    pub from: PathBuf,
    /// In words: "12 min after IMG_1.RAF", "looks like IMG_5.RAF (0.94)".
    pub why: String,
}

/// Likeness from which a placed photo lends its place by look (cosine of
/// the search model's vectors; the same scene is usually above 0.9).
pub const LOOK_ALIKE: f32 = 0.9;

fn name(p: &std::path::Path) -> String {
    p.file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if na == 0.0 || nb == 0.0 {
        0.0
    } else {
        dot / (na * nb)
    }
}

fn minutes_words(m: f64) -> String {
    if m < 1.0 {
        "under a minute".into()
    } else {
        format!("{} min", m.round())
    }
}

/// Guesses for the photos without a place among `photos` (with their
/// records and, if indexed, their search vectors).
pub fn guess(
    photos: &[(PathBuf, Record, Option<Vec<f32>>)],
    within_minutes: f64,
    offset_minutes: f64,
) -> Vec<Guess> {
    // Placed photos, by time.
    let mut placed: Vec<(i64, usize, [f64; 2])> = Vec::new();
    for (i, (_, r, _)) in photos.iter().enumerate() {
        if let (Some(at), Some(t)) = (
            r.place(),
            r.shot.captured.as_deref().and_then(local_seconds),
        ) {
            placed.push((t, i, at));
        }
    }
    placed.sort_by_key(|p| p.0);
    let window = (within_minutes * 60.0).round() as i64;
    let mut out = Vec::new();
    for (path, r, vector) in photos {
        if r.place().is_some() {
            continue;
        }
        let time = r
            .shot
            .captured
            .as_deref()
            .and_then(local_seconds)
            .map(|t| t + (offset_minutes * 60.0).round() as i64);
        if let Some(g) = time.and_then(|t| by_time(photos, &placed, path, t, window)) {
            out.push(g);
            continue;
        }
        if let Some(v) = vector {
            let best = photos
                .iter()
                .filter(|(_, q, _)| q.place().is_some())
                .filter_map(|(qp, q, qv)| Some((qp, q.place()?, cosine(v, qv.as_deref()?))))
                .max_by(|a, b| a.2.total_cmp(&b.2));
            if let Some((qp, at, like)) = best.filter(|b| b.2 >= LOOK_ALIKE) {
                out.push(Guess {
                    path: path.clone(),
                    lat: at[0],
                    lon: at[1],
                    by: "look",
                    from: qp.clone(),
                    why: format!("looks like {} ({like:.2})", name(qp)),
                });
            }
        }
    }
    out
}

fn by_time(
    photos: &[(PathBuf, Record, Option<Vec<f32>>)],
    placed: &[(i64, usize, [f64; 2])],
    path: &std::path::Path,
    t: i64,
    window: i64,
) -> Option<Guess> {
    let after = placed.partition_point(|p| p.0 <= t);
    let before = after
        .checked_sub(1)
        .map(|i| placed[i])
        .filter(|p| t - p.0 <= window);
    let next = placed.get(after).copied().filter(|p| p.0 - t <= window);
    let guess = |at: [f64; 2], from: usize, why: String| Guess {
        path: path.to_path_buf(),
        lat: (at[0] * 1e6).round() / 1e6,
        lon: (at[1] * 1e6).round() / 1e6,
        by: "time",
        from: photos[from].0.clone(),
        why,
    };
    match (before, next) {
        (Some(b), Some(n)) if n.0 > b.0 => {
            let k = (t - b.0) as f64 / (n.0 - b.0) as f64;
            let at = [
                b.2[0] + (n.2[0] - b.2[0]) * k,
                b.2[1] + (n.2[1] - b.2[1]) * k,
            ];
            let near = if k <= 0.5 { b.1 } else { n.1 };
            Some(guess(
                at,
                near,
                format!(
                    "between {} and {}, taken {} and {} apart",
                    name(&photos[b.1].0),
                    name(&photos[n.1].0),
                    minutes_words((t - b.0) as f64 / 60.0),
                    minutes_words((n.0 - t) as f64 / 60.0)
                ),
            ))
        }
        (Some(b), _) => Some(guess(
            b.2,
            b.1,
            format!(
                "{} after {}",
                minutes_words((t - b.0) as f64 / 60.0),
                name(&photos[b.1].0)
            ),
        )),
        (_, Some(n)) => Some(guess(
            n.2,
            n.1,
            format!(
                "{} before {}",
                minutes_words((n.0 - t) as f64 / 60.0),
                name(&photos[n.1].0)
            ),
        )),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::Shot;

    fn photo(
        name: &str,
        time: &str,
        gps: Option<[f64; 2]>,
        v: Option<Vec<f32>>,
    ) -> (PathBuf, Record, Option<Vec<f32>>) {
        let r = Record {
            shot: Shot {
                captured: Some(time.into()),
                camera_gps: gps,
                ..Shot::default()
            },
            ..Record::default()
        };
        (PathBuf::from(name), r, v)
    }

    #[test]
    fn by_time_near_one_between_two_and_by_look() {
        let photos = vec![
            photo(
                "phone1.jpg",
                "2026-07-14 10:00:00",
                Some([52.0, 4.0]),
                Some(vec![1.0, 0.0]),
            ),
            photo("cam1.raf", "2026-07-14 10:10:00", None, None),
            photo("phone2.jpg", "2026-07-14 10:20:00", Some([52.2, 4.2]), None),
            photo("cam2.raf", "2026-07-14 10:50:00", None, None),
            photo(
                "cam3.raf",
                "2026-07-15 09:00:00",
                None,
                Some(vec![0.99, 0.05]),
            ),
            photo(
                "cam4.raf",
                "2026-07-16 09:00:00",
                None,
                Some(vec![0.0, 1.0]),
            ),
        ];
        let g = guess(&photos, 30.0, 0.0);
        let by: Vec<_> = g.iter().map(|g| (name(&g.path), g.by)).collect();
        assert_eq!(
            by,
            [
                ("cam1.raf".into(), "time"),
                ("cam2.raf".into(), "time"),
                ("cam3.raf".into(), "look")
            ]
        );
        assert!(
            (g[0].lat - 52.1).abs() < 1e-9
                && g[0].why.starts_with("between phone1.jpg and phone2.jpg"),
            "{:?}",
            g[0]
        );
        assert_eq!(
            (g[1].lat, g[1].why.as_str()),
            (52.2, "30 min after phone2.jpg")
        );
        assert_eq!(g[2].lat, 52.0);
        // The camera's clock an hour behind: moved, cam2 falls out of reach.
        let later = guess(&photos, 30.0, 60.0);
        assert!(
            later
                .iter()
                .all(|g| name(&g.path) != "cam1.raf" || g.by != "time" || g.why.contains("after"))
        );
    }
}
