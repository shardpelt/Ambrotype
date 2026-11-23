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
//!
//! Names: [`names_at`] and [`add_names`] ask OpenStreetMap's Nominatim for a
//! place's city, state and country and write them into the sidecars, as
//! Lightroom keeps them; [`search`] finds a place by name. Only when asked.

use crate::catalog::Record;
use crate::gpx::local_seconds;
use amb_protocol::sidecar;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

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
pub(crate) const LOOK_ALIKE: f32 = 0.9;

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

// ---------------------------------------------------------------- names

/// Nominatim's usage policy: at most one request a second.
const NOMINATIM_GAP: Duration = Duration::from_secs(1);
/// Places this close (degrees, about 100 m) share one answer.
const SAME_PLACE_DEG: f64 = 0.001;
/// Requests that failed one after another before it stops: the network is
/// most likely down, and each one waits for its timeout.
const FAILURES_IN_A_ROW: usize = 3;
/// How long a request may take (seconds to connect, seconds in all).
const CONNECT_SECS: u64 = 10;
const REQUEST_SECS: u64 = 20;
/// The zoom Nominatim's reverse lookup answers at: 10 is a city.
const REVERSE_ZOOM: &str = "10";

/// A place found by name or address.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Found {
    pub name: String,
    pub lat: f64,
    pub lon: f64,
}

/// The names of a place, as OpenStreetMap gives them (in English).
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct PlaceNames {
    /// The city, town, village or municipality, first one present.
    pub city: Option<String>,
    pub state: Option<String>,
    pub country: Option<String>,
    /// Two letters, upper case.
    pub country_code: Option<String>,
}

impl PlaceNames {
    fn is_empty(&self) -> bool {
        self.city.is_none() && self.country.is_none()
    }
}

/// A GET to OpenStreetMap's Nominatim, `path` being `search` or `reverse`.
/// Asked only when someone asks (it tells openstreetmap.org what was
/// searched, or where a photo was taken).
fn nominatim_get(path: &str, query: &[(&str, &str)]) -> Result<String, String> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(CONNECT_SECS))
        .timeout(Duration::from_secs(REQUEST_SECS))
        .try_proxy_from_env(true)
        // Nominatim's policy asks for an application name.
        .user_agent(concat!(
            "Ambrotype/",
            env!("CARGO_PKG_VERSION"),
            " (photo editor; place search)"
        ))
        .build();
    let mut request = agent.get(&format!("https://nominatim.openstreetmap.org/{path}"));
    for (k, v) in query {
        request = request.query(k, v);
    }
    let response = request.call().map_err(|e| e.to_string())?;
    response.into_string().map_err(|e| e.to_string())
}

/// Places matching `query` ("Dam Square, Amsterdam"), at most eight.
pub fn search(query: &str) -> Result<Vec<Found>, String> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let text = nominatim_get(
        "search",
        &[("q", query), ("format", "jsonv2"), ("limit", "8")],
    )
    .map_err(|e| format!("Place search: {e}"))?;
    let rows: Vec<serde_json::Value> =
        serde_json::from_str(&text).map_err(|e| format!("Place search: {e}"))?;
    Ok(rows
        .iter()
        .filter_map(|v| {
            let num = |k: &str| v.get(k)?.as_str()?.parse::<f64>().ok();
            Some(Found {
                name: v.get("display_name")?.as_str()?.to_string(),
                lat: num("lat")?,
                lon: num("lon")?,
            })
        })
        .collect())
}

/// The names in Nominatim's reverse answer (`format=jsonv2`,
/// `addressdetails=1`). A place with no address (open sea) gives none.
fn names_from_json(text: &str) -> Result<PlaceNames, String> {
    let v: serde_json::Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let address = v.get("address");
    let get = |k: &str| {
        address?
            .get(k)?
            .as_str()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };
    Ok(PlaceNames {
        city: ["city", "town", "village", "municipality"]
            .iter()
            .find_map(|k| get(k)),
        state: get("state"),
        country: get("country"),
        country_code: get("country_code").map(|c| c.to_uppercase()),
    })
}

/// The city, state and country at a place, from OpenStreetMap.
pub fn names_at(lat: f64, lon: f64) -> Result<PlaceNames, String> {
    let (lat, lon) = (format!("{lat:.6}"), format!("{lon:.6}"));
    let text = nominatim_get(
        "reverse",
        &[
            ("lat", &lat),
            ("lon", &lon),
            ("format", "jsonv2"),
            ("zoom", REVERSE_ZOOM),
            ("addressdetails", "1"),
            ("accept-language", "en"),
        ],
    )
    .map_err(|e| format!("Place names: {e}"))?;
    names_from_json(&text).map_err(|e| format!("Place names: {e}"))
}

/// What to do when adding names.
#[derive(Debug, Clone, Default)]
pub struct NamesOptions {
    /// Look up photos that already have a City or Country too.
    pub replace: bool,
}

/// What adding names did.
#[derive(Debug, Default, Serialize)]
pub struct NamesReport {
    /// Photos whose sidecar now has names.
    pub named: usize,
    pub skipped_no_place: usize,
    /// Had a City or Country, and `replace` wasn't asked for.
    pub skipped_named: usize,
    pub failed: Vec<(PathBuf, String)>,
}

/// Write the city, state, country and country code of each photo's place
/// into its sidecar (`photoshop:City`, ...), from OpenStreetMap: one request
/// a second at most, and none for a place within about 100 m of the last.
/// `progress` gets (photos done, photos in all).
pub fn add_names(
    paths: &[PathBuf],
    opts: &NamesOptions,
    progress: &dyn Fn(usize, usize),
) -> Result<NamesReport, String> {
    let last = std::cell::Cell::new(None::<Instant>);
    add_names_with(
        paths,
        opts,
        progress,
        &crate::catalog::read,
        &mut |lat, lon| {
            if let Some(at) = last.get() {
                std::thread::sleep(NOMINATIM_GAP.saturating_sub(at.elapsed()));
            }
            let answer = names_at(lat, lon);
            last.set(Some(Instant::now()));
            answer
        },
    )
}

/// [`add_names`] with the reading of a photo and the lookup of a place as
/// parameters, so tests need no files of photos and no network.
fn add_names_with(
    paths: &[PathBuf],
    opts: &NamesOptions,
    progress: &dyn Fn(usize, usize),
    record: &dyn Fn(&Path) -> Record,
    lookup: &mut dyn FnMut(f64, f64) -> Result<PlaceNames, String>,
) -> Result<NamesReport, String> {
    let mut report = NamesReport::default();
    let mut known: Vec<([f64; 2], PlaceNames)> = Vec::new();
    let mut failures = 0;
    for (i, path) in paths.iter().enumerate() {
        progress(i, paths.len());
        let r = record(path);
        let Some(at) = r.place() else {
            report.skipped_no_place += 1;
            continue;
        };
        if !opts.replace && (r.info.city.is_some() || r.info.country.is_some()) {
            report.skipped_named += 1;
            continue;
        }
        let near = known.iter().find(|(k, _)| {
            (k[0] - at[0]).abs() < SAME_PLACE_DEG && (k[1] - at[1]).abs() < SAME_PLACE_DEG
        });
        let names = match near {
            Some((_, n)) => n.clone(),
            None => match lookup(at[0], at[1]) {
                Ok(n) => {
                    failures = 0;
                    known.push((at, n.clone()));
                    n
                }
                Err(e) => {
                    report.failed.push((path.clone(), e));
                    failures += 1;
                    if failures >= FAILURES_IN_A_ROW {
                        break;
                    }
                    continue;
                }
            },
        };
        if names.is_empty() {
            report.failed.push((
                path.clone(),
                "OpenStreetMap has no name for this place".into(),
            ));
            continue;
        }
        match write_names(path, &names) {
            Ok(()) => report.named += 1,
            Err(e) => report.failed.push((path.clone(), e)),
        }
    }
    progress(paths.len(), paths.len());
    Ok(report)
}

/// Set the names in the photo's sidecar, keeping everything else in it.
fn write_names(photo: &Path, names: &PlaceNames) -> Result<(), String> {
    let on_disk = amb_engine::sidecar_text(photo);
    let mut info = on_disk
        .as_deref()
        .map(sidecar::read_info)
        .unwrap_or_default();
    // What the place doesn't have is taken out, so a town's names don't
    // keep an old state.
    let text = |v: &Option<String>| Some(v.clone().unwrap_or_default());
    info.city = text(&names.city);
    info.state = text(&names.state);
    info.country = text(&names.country);
    info.country_code = text(&names.country_code);
    let text = sidecar::with_info(on_disk.as_deref(), &info).map_err(|e| e.to_string())?;
    sidecar::write_atomic(&amb_engine::sidecar_path(photo), &text).map_err(|e| e.to_string())
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

    // ---------------------------------------------------------- names

    fn placed(lat: f64, lon: f64, city: Option<&str>) -> Record {
        Record {
            info: amb_protocol::sidecar::PhotoInfo {
                gps: Some(format!("{lat:.6},{lon:.6}")),
                city: city.map(str::to_string),
                ..Default::default()
            },
            ..Record::default()
        }
    }

    #[test]
    fn reverse_answers_give_city_town_or_nothing() {
        let city = r#"{"address":{"city":"Paris","state":"Île-de-France","country":"France","country_code":"fr"}}"#;
        let n = names_from_json(city).unwrap();
        assert_eq!(n.city.as_deref(), Some("Paris"));
        assert_eq!(n.country_code.as_deref(), Some("FR"));
        // A town when there is no city, and a village after that.
        let town = r#"{"address":{"town":"Delft","village":"X","country":"Netherlands","country_code":"nl"}}"#;
        assert_eq!(
            names_from_json(town).unwrap().city.as_deref(),
            Some("Delft")
        );
        // No city at all (a country's coast), and no address (the sea).
        let none = r#"{"address":{"country":"Norway","country_code":"no"}}"#;
        let n = names_from_json(none).unwrap();
        assert_eq!((n.city, n.country.as_deref()), (None, Some("Norway")));
        assert!(
            names_from_json(r#"{"error":"Unable to geocode"}"#)
                .unwrap()
                .is_empty()
        );
        assert!(names_from_json("not json").is_err());
    }

    #[test]
    fn names_skip_unplaced_and_named_photos_and_share_near_answers() {
        let dir = std::env::temp_dir().join(format!("amb-names-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = |n: &str| dir.join(n);
        let paths: Vec<PathBuf> = ["a.dng", "b.dng", "c.dng", "d.dng", "e.dng"]
            .iter()
            .map(|n| p(n))
            .collect();
        let record = |path: &Path| match path.file_name().unwrap().to_str().unwrap() {
            "a.dng" => placed(48.8584, 2.2945, None),
            // Within 100 m of a: no second request.
            "b.dng" => placed(48.8588, 2.2949, None),
            "c.dng" => Record::default(),
            "d.dng" => placed(52.37, 4.89, Some("Amsterdam")),
            _ => placed(35.0, 139.0, None),
        };
        let mut asked = Vec::new();
        let names = |lat: f64, _: f64| {
            asked.push(lat);
            Ok(PlaceNames {
                city: Some("Paris".into()),
                country: Some("France".into()),
                country_code: Some("FR".into()),
                ..PlaceNames::default()
            })
        };
        let mut lookup = names;
        let report = add_names_with(
            &paths,
            &NamesOptions::default(),
            &|_, _| {},
            &record,
            &mut lookup,
        )
        .unwrap();
        assert_eq!(
            (
                report.named,
                report.skipped_no_place,
                report.skipped_named,
                report.failed.len()
            ),
            (3, 1, 1, 0)
        );
        assert_eq!(asked.len(), 2, "a and b share an answer: {asked:?}");
        let written =
            amb_protocol::sidecar::read_info(&std::fs::read_to_string(p("b.xmp")).unwrap());
        assert_eq!(written.city.as_deref(), Some("Paris"));
        assert_eq!(written.country_code.as_deref(), Some("FR"));
        assert!(!p("c.xmp").exists() && !p("d.xmp").exists());
        // With replace, the named photo is looked up too; failures are listed.
        let mut fail = |_: f64, _: f64| Err("offline".to_string());
        let again = NamesOptions { replace: true };
        let report = add_names_with(&paths[3..4], &again, &|_, _| {}, &record, &mut fail).unwrap();
        assert_eq!((report.named, report.failed.len()), (0, 1));
        std::fs::remove_dir_all(&dir).ok();
    }
}
