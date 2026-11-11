// SPDX-License-Identifier: GPL-3.0-or-later
//! Insights: how someone shoots, from the photos' headers. How often each
//! aperture, focal length, shutter speed, ISO and hour of the day comes
//! up, which cameras and lenses, and how many of each were kept.
//!
//! Numbers go to the nearest usual stop (f/1.4, f/2, 35 mm, 1/250 s), on a
//! log scale, so f/1.6 counts as f/1.8 and a 37 mm zoom setting as
//! 35 mm. The buckets are shared with the editor's Insights view (it gets
//! them from here), so `amb insights` and the view always agree.

mod advice;
pub use advice::{Note, advice, challenges};

use crate::catalog::Record;
use amb_protocol::sidecar::Flag;
use serde::Serialize;

/// One bar: values above the previous bucket's `max` and up to this one.
#[derive(Debug, Clone, Serialize)]
pub struct Bucket {
    pub label: String,
    pub max: f64,
}

/// A number from the header, cut into buckets.
#[derive(Debug, Clone, Serialize)]
pub struct Field {
    /// "aperture", "focal", "shutter", "iso" or "hour".
    pub key: &'static str,
    pub name: &'static str,
    pub buckets: Vec<Bucket>,
}

/// Stops with their labels; a value goes to the nearest on a log scale.
fn stops(key: &'static str, name: &'static str, stops: &[(f64, String)]) -> Field {
    let buckets = stops
        .iter()
        .enumerate()
        .map(|(i, (v, label))| Bucket {
            label: label.clone(),
            // Halfway (geometrically) to the next stop.
            max: stops
                .get(i + 1)
                .map_or(f64::INFINITY, |(n, _)| (v * n).sqrt()),
        })
        .collect();
    Field { key, name, buckets }
}

/// "1/250" or "2 s", from the fraction's denominator.
fn shutter_label(d: f64) -> String {
    if d > 1.0 {
        format!("1/{d}")
    } else {
        format!("{} s", (1.0 / d).round())
    }
}

/// The number fields, in the order the view shows them.
pub fn fields() -> Vec<Field> {
    let f = |v: f64| format!("f/{v}");
    let apertures = [
        1.0, 1.2, 1.4, 1.8, 2.0, 2.8, 4.0, 5.6, 8.0, 11.0, 16.0, 22.0,
    ];
    let focals = [
        8.0, 14.0, 16.0, 20.0, 24.0, 28.0, 35.0, 50.0, 70.0, 85.0, 105.0, 135.0, 200.0, 300.0,
        400.0, 600.0,
    ];
    let shutters = [
        8000.0, 4000.0, 2000.0, 1000.0, 500.0, 250.0, 125.0, 60.0, 30.0, 15.0, 8.0, 4.0, 2.0, 1.0,
        0.5, 0.25, 0.125, 0.0333,
    ];
    let isos = [
        50.0, 100.0, 200.0, 400.0, 800.0, 1600.0, 3200.0, 6400.0, 12800.0, 25600.0, 51200.0,
    ];
    vec![
        stops("aperture", "Aperture", &apertures.map(|v| (v, f(v)))),
        stops(
            "focal",
            "Focal length",
            &focals.map(|v| (v, format!("{v} mm"))),
        ),
        stops(
            "shutter",
            "Shutter speed",
            &shutters.map(|d| (1.0 / d, shutter_label(d))),
        ),
        stops("iso", "ISO", &isos.map(|v| (v, format!("{v}")))),
        Field {
            key: "hour",
            name: "Time of day",
            buckets: (0..24)
                .map(|h| Bucket {
                    label: format!("{h:02}"),
                    max: f64::from(h) + 0.5,
                })
                .collect(),
        },
    ]
}

/// The field's value for a photo: the hour is read from its capture time.
pub fn value(key: &str, r: &Record) -> Option<f64> {
    let s = &r.shot;
    match key {
        "aperture" => s.aperture,
        "focal" => s.focal,
        "shutter" => s.shutter,
        "iso" => s.iso.map(f64::from),
        "hour" => s.captured.as_deref()?.get(11..13)?.parse().ok(),
        _ => None,
    }
    .filter(|v: &f64| (v.is_finite() && *v > 0.0) || key == "hour")
}

/// Which bucket `v` goes in.
pub fn bucket_of(field: &Field, v: f64) -> usize {
    field
        .buckets
        .iter()
        .position(|b| v <= b.max)
        .unwrap_or(field.buckets.len() - 1)
}

/// Kept: picked, or three stars or more.
pub fn kept(r: &Record) -> bool {
    r.info.flag == Some(Flag::Pick) || r.info.rating.unwrap_or(0) >= 3
}

/// A bar's count and how many of those were kept.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Bar {
    pub label: String,
    pub count: usize,
    pub kept: usize,
}

/// One field over many photos; buckets nobody used at either end left out.
#[derive(Debug, Clone, Serialize)]
pub struct Tally {
    pub key: String,
    pub name: String,
    pub bars: Vec<Bar>,
    /// Photos whose header doesn't say.
    pub unknown: usize,
}

/// Count the photos by `key`: a number field, or "camera" or "lens" (the
/// most used first).
pub fn tally(key: &str, records: &[Record]) -> Option<Tally> {
    if key == "camera" || key == "lens" {
        let mut bars: Vec<Bar> = Vec::new();
        let mut unknown = 0;
        for r in records {
            let name = if key == "camera" {
                &r.shot.camera
            } else {
                &r.shot.lens
            };
            let Some(name) = name else {
                unknown += 1;
                continue;
            };
            let i = match bars.iter().position(|b| &b.label == name) {
                Some(i) => i,
                None => {
                    bars.push(Bar {
                        label: name.clone(),
                        ..Bar::default()
                    });
                    bars.len() - 1
                }
            };
            bars[i].count += 1;
            bars[i].kept += usize::from(kept(r));
        }
        bars.sort_by(|a, b| b.count.cmp(&a.count).then(a.label.cmp(&b.label)));
        let name = if key == "camera" { "Camera" } else { "Lens" };
        return Some(Tally {
            key: key.into(),
            name: name.into(),
            bars,
            unknown,
        });
    }
    let field = fields().into_iter().find(|f| f.key == key)?;
    let mut bars: Vec<Bar> = field
        .buckets
        .iter()
        .map(|b| Bar {
            label: b.label.clone(),
            ..Bar::default()
        })
        .collect();
    let mut unknown = 0;
    for r in records {
        match value(key, r) {
            Some(v) => {
                let b = &mut bars[bucket_of(&field, v)];
                b.count += 1;
                b.kept += usize::from(kept(r));
            }
            None => unknown += 1,
        }
    }
    // The hours stay whole; the others from the first to the last used.
    if key != "hour" {
        let last = bars.iter().rposition(|b| b.count > 0).map_or(0, |i| i + 1);
        bars.truncate(last);
        let first = bars.iter().position(|b| b.count > 0).unwrap_or(0);
        bars.drain(..first);
    }
    Some(Tally {
        key: key.into(),
        name: field.name.into(),
        bars,
        unknown,
    })
}

/// Every key `tally` takes, in the view's order.
pub const KEYS: [&str; 7] = [
    "aperture", "focal", "shutter", "iso", "hour", "camera", "lens",
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::Shot;

    fn shot(aperture: f64, focal: f64, rating: u8) -> Record {
        let mut r = Record {
            shot: Shot {
                aperture: Some(aperture),
                focal: Some(focal),
                shutter: Some(1.0 / 250.0),
                iso: Some(400),
                captured: Some("2026-05-04 18:30:00".into()),
                camera: Some("X-T5".into()),
                ..Shot::default()
            },
            ..Record::default()
        };
        r.info.rating = Some(rating);
        r
    }

    #[test]
    fn values_go_to_the_nearest_stop() {
        let all = fields();
        let ap = &all[0];
        let label = |f: &Field, v| f.buckets[bucket_of(f, v)].label.clone();
        assert_eq!(label(ap, 1.4), "f/1.4");
        assert_eq!(label(ap, 1.55), "f/1.4");
        assert_eq!(label(ap, 1.7), "f/1.8");
        assert_eq!(label(ap, 3.2), "f/2.8");
        assert_eq!(label(ap, 64.0), "f/22");
        assert_eq!(label(&all[1], 37.0), "35 mm");
        assert_eq!(label(&all[1], 45.0), "50 mm");
        assert_eq!(label(&all[2], 1.0 / 250.0), "1/250");
        assert_eq!(label(&all[2], 1.0 / 200.0), "1/250");
        assert_eq!(label(&all[2], 30.0), "30 s");
        assert_eq!(label(&all[3], 500.0), "400");
    }

    #[test]
    fn a_tally_counts_and_keeps_and_trims_the_ends() {
        let rs = [shot(1.4, 35.0, 4), shot(1.4, 35.0, 1), shot(2.8, 50.0, 0)];
        let t = tally("aperture", &rs).unwrap();
        let bars: Vec<_> = t
            .bars
            .iter()
            .map(|b| (b.label.as_str(), b.count, b.kept))
            .collect();
        assert_eq!(
            bars,
            [
                ("f/1.4", 2, 1),
                ("f/1.8", 0, 0),
                ("f/2", 0, 0),
                ("f/2.8", 1, 0)
            ]
        );
        assert_eq!(tally("hour", &rs).unwrap().bars[18].count, 3);
        let cams = tally("camera", &rs).unwrap();
        assert_eq!(
            (cams.bars[0].label.as_str(), cams.bars[0].count),
            ("X-T5", 3)
        );
        assert_eq!(tally("lens", &rs).unwrap().unknown, 3);
    }
}
