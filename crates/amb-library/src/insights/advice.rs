// SPDX-License-Identifier: GPL-3.0-or-later
//! What the numbers suggest, in plain words: each note says the number it
//! comes from, so it can be checked, and nothing is guessed from outside
//! the photos (no lens database, no brands). Then challenges: the far side
//! of how someone shoots, and things next to it, to try for a week.

use super::kept;
use crate::catalog::Record;
use serde::Serialize;
use std::collections::BTreeMap;

/// One line of advice or a challenge.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Note {
    /// "summary", "lens", "light", "habit", "keepers", "gear" or "challenge".
    pub kind: &'static str,
    pub title: String,
    pub text: String,
}

fn note(kind: &'static str, title: impl Into<String>, text: impl Into<String>) -> Note {
    Note {
        kind,
        title: title.into(),
        text: text.into(),
    }
}

fn pct(n: usize, of: usize) -> u32 {
    (100.0 * n as f64 / of.max(1) as f64).round() as u32
}

fn f_number(v: f64) -> String {
    if v.fract().abs() < 0.05 {
        format!("f/{v:.0}")
    } else {
        format!("f/{v:.1}")
    }
}

/// Enough photos for a number to mean something.
const ENOUGH: usize = 20;

/// Hours counted as night: from 20:00 and before 06:00.
const NIGHT_FROM: u32 = 20;
const NIGHT_UNTIL: u32 = 6;

/// A share of night photos (percent) from which the summary says "often at night".
const NIGHT_OFTEN: u32 = 25;

/// Everything Insights says about a set of photos, in the order it is
/// shown. The editor and `amb insights` both take it from [`notes`], so
/// they can't differ.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Notes {
    pub summary: Option<Note>,
    pub advice: Vec<Note>,
    pub gear: Vec<Note>,
    pub challenges: Vec<Note>,
}

/// The summary, advice, gear and challenges for `records`.
pub fn notes(records: &[Record]) -> Notes {
    Notes {
        summary: summary(records),
        advice: advice(records),
        gear: Vec::new(),
        challenges: challenges(records),
    }
}

/// The middle value (the upper one for an even count).
fn median(mut v: Vec<f64>) -> Option<f64> {
    v.sort_by(f64::total_cmp);
    v.get(v.len() / 2).copied()
}

/// The hour of day (0 to 23) of each photo that has a capture time.
fn hours(records: &[Record]) -> Vec<u32> {
    records
        .iter()
        .filter_map(|r| r.shot.captured.as_deref()?.get(11..13)?.parse().ok())
        .collect()
}

fn is_night(hour: u32) -> bool {
    hour >= NIGHT_FROM || hour < NIGHT_UNTIL
}

#[derive(Clone, Copy, PartialEq)]
enum FocalBand {
    Wide,
    Normal,
    ShortTele,
    Long,
}

#[derive(Clone, Copy, PartialEq)]
enum ApertureBand {
    Open,
    Middle,
    Stopped,
}

/// Focal bands: where each starts (mm) and the word the summary uses.
const FOCAL_BANDS: &[(FocalBand, f64, &str)] = &[
    (FocalBand::Wide, 0.0, "wide"),
    (FocalBand::Normal, 28.0, "normal"),
    (FocalBand::ShortTele, 60.0, "short tele"),
    (FocalBand::Long, 135.0, "long"),
];

/// Aperture bands: where each starts (f-number) and its words. f/2.8 and
/// wider is open, so the middle starts just past it (the next third-stop
/// is f/3.2); f/8 and up is stopped down.
const APERTURE_BANDS: &[(ApertureBand, f64, &str)] = &[
    (ApertureBand::Open, 0.0, "wide open"),
    (ApertureBand::Middle, 3.0, "at middle apertures"),
    (ApertureBand::Stopped, 8.0, "stopped down"),
];

/// What a usual focal length and aperture say someone shoots.
const STYLES: &[(FocalBand, ApertureBand, &str)] = &[
    (
        FocalBand::Wide,
        ApertureBand::Open,
        "close-in, in the middle of things",
    ),
    (
        FocalBand::Wide,
        ApertureBand::Middle,
        "travel, streets and places",
    ),
    (
        FocalBand::Wide,
        ApertureBand::Stopped,
        "landscapes and buildings",
    ),
    (FocalBand::Normal, ApertureBand::Open, "people and moments"),
    (
        FocalBand::Normal,
        ApertureBand::Middle,
        "the street, everyday life",
    ),
    (
        FocalBand::Normal,
        ApertureBand::Stopped,
        "whole scenes, sharp front to back",
    ),
    (FocalBand::ShortTele, ApertureBand::Open, "portraits"),
    (
        FocalBand::ShortTele,
        ApertureBand::Middle,
        "portraits and details, from a step back",
    ),
    (
        FocalBand::ShortTele,
        ApertureBand::Stopped,
        "far-off scenes, flattened and sharp",
    ),
    (
        FocalBand::Long,
        ApertureBand::Open,
        "wildlife, sport or details from afar",
    ),
    (
        FocalBand::Long,
        ApertureBand::Middle,
        "wildlife, sport or details from afar",
    ),
    (
        FocalBand::Long,
        ApertureBand::Stopped,
        "wildlife, sport or details from afar",
    ),
];

/// The band a value falls in: the last whose start is at or below it.
fn band<T: Copy>(table: &[(T, f64, &'static str)], v: f64) -> Option<(T, &'static str)> {
    table
        .iter()
        .rev()
        .find(|(_, from, _)| v >= *from)
        .map(|(b, _, word)| (*b, *word))
}

/// One paragraph on what the medians say, with the numbers. Needs
/// [`ENOUGH`] photos that have both a focal length and an aperture.
pub fn summary(records: &[Record]) -> Option<Note> {
    let both: Vec<(f64, f64)> = records
        .iter()
        .filter_map(|r| Some((r.shot.focal?, r.shot.aperture?)))
        .collect();
    if both.len() < ENOUGH {
        return None;
    }
    let focal = median(both.iter().map(|b| b.0).collect())?;
    let aperture = median(both.iter().map(|b| b.1).collect())?;
    let (fb, fword) = band(FOCAL_BANDS, focal)?;
    let (ab, aword) = band(APERTURE_BANDS, aperture)?;
    let phrase = STYLES
        .iter()
        .find(|(f, a, _)| *f == fb && *a == ab)
        .map(|s| s.2)?;
    let hours = hours(records);
    let light = if hours.len() < ENOUGH {
        ""
    } else if pct(hours.iter().filter(|h| is_night(**h)).count(), hours.len()) >= NIGHT_OFTEN {
        ", often at night"
    } else {
        ", in daylight"
    };
    Some(note(
        "summary",
        "What the numbers say you shoot",
        format!(
            "Mostly {fword} (median {focal:.0} mm), {aword} ({}){light}: the numbers say {phrase}.",
            f_number(aperture)
        ),
    ))
}

/// Advice from the photos: lenses (wide open, zoom ends), light, habits
/// and where the keepers come from. The most telling first, at most eight.
pub fn advice(records: &[Record]) -> Vec<Note> {
    let mut out = Vec::new();
    // Per lens: how it's used.
    let mut lenses: BTreeMap<&str, Vec<&Record>> = BTreeMap::new();
    for r in records {
        if let Some(l) = r.shot.lens.as_deref() {
            lenses.entry(l).or_default().push(r);
        }
    }
    let mut by_size: Vec<_> = lenses
        .into_iter()
        .filter(|(_, v)| v.len() >= ENOUGH)
        .collect();
    by_size.sort_by_key(|(_, v)| std::cmp::Reverse(v.len()));
    for (lens, rs) in by_size.iter().take(4) {
        out.extend(lens_notes(lens, rs));
    }
    out.extend(light_notes(records));
    out.extend(habit_notes(records));
    out.extend(keeper_notes(records));
    out.truncate(8);
    out
}

fn lens_notes(lens: &str, rs: &[&Record]) -> Vec<Note> {
    let mut out = Vec::new();
    let n = rs.len();
    let aps: Vec<f64> = rs.iter().filter_map(|r| r.shot.aperture).collect();
    if aps.len() >= ENOUGH {
        // Its widest is the widest it was used at: a guess, but the photos'.
        let widest = aps.iter().copied().fold(f64::INFINITY, f64::min);
        let open = aps.iter().filter(|a| **a <= widest * 1.12).count();
        let share = pct(open, aps.len());
        if widest <= 2.0 && share < 15 {
            let stopped = aps.iter().filter(|a| **a >= widest * 1.9).count();
            out.push(note(
                "lens",
                format!("{lens}: rarely wide open"),
                format!(
                    "It opens to {}, but only {share} % of its {} photos are there; {} % are two stops down or more. A lighter, slower lens would take most of these.",
                    f_number(widest),
                    aps.len(),
                    pct(stopped, aps.len())
                ),
            ));
        } else if widest <= 2.8 && share >= 50 {
            out.push(note(
                "lens",
                format!("{lens}: you love it wide open"),
                format!(
                    "{share} % of its {} photos are at {}, its widest. It earns its place.",
                    aps.len(),
                    f_number(widest)
                ),
            ));
        }
    }
    let focals: Vec<f64> = rs.iter().filter_map(|r| r.shot.focal).collect();
    if focals.len() >= ENOUGH {
        let lo = focals.iter().copied().fold(f64::INFINITY, f64::min);
        let hi = focals.iter().copied().fold(0.0, f64::max);
        // A zoom: used over a range.
        if hi / lo >= 1.5 {
            let at_lo = focals.iter().filter(|f| **f <= lo * 1.05).count();
            let at_hi = focals.iter().filter(|f| **f >= hi * 0.95).count();
            if pct(at_lo, n) >= 30 {
                out.push(note(
                    "lens",
                    format!("{lens}: often at its widest"),
                    format!(
                        "{} % of its photos are at {lo:.0} mm, the widest it goes. You'd likely use something wider.",
                        pct(at_lo, focals.len())
                    ),
                ));
            }
            if pct(at_hi, n) >= 30 {
                out.push(note(
                    "lens",
                    format!("{lens}: often at its longest"),
                    format!(
                        "{} % of its photos are at {hi:.0} mm, the longest it goes. You'd likely use something longer.",
                        pct(at_hi, focals.len())
                    ),
                ));
            }
        }
    }
    out
}

fn light_notes(records: &[Record]) -> Vec<Note> {
    let mut out = Vec::new();
    let known: Vec<&Record> = records
        .iter()
        .filter(|r| r.shot.iso.is_some() && r.shot.shutter.is_some())
        .collect();
    if known.len() < ENOUGH {
        return out;
    }
    let dark: Vec<&&Record> = known.iter().filter(|r| r.shot.iso >= Some(3200)).collect();
    if pct(dark.len(), known.len()) >= 15 {
        let slow = dark
            .iter()
            .filter(|r| r.shot.shutter.is_some_and(|s| s > 1.0 / 60.0))
            .count();
        let wide = dark
            .iter()
            .filter(|r| r.shot.aperture.is_some_and(|a| a <= 2.0))
            .count();
        let tip = if pct(wide, dark.len()) < 30 {
            " A faster lens (f/2 or wider) would let the ISO come down."
        } else {
            " You already open up; a camera that's cleaner at high ISO, or a tripod, is the next step."
        };
        out.push(note(
            "light",
            "You shoot in the dark a lot",
            format!(
                "{} % of your photos are at ISO 3200 or more, and {} % of those slower than 1/60 s.{tip}",
                pct(dark.len(), known.len()),
                pct(slow, dark.len())
            ),
        ));
    }
    // Slower than one over the focal length: the old rule for a steady hand.
    let risky = known
        .iter()
        .filter(|r| {
            matches!((r.shot.shutter, r.shot.focal), (Some(s), Some(f)) if f > 0.0 && s > 1.0 / f)
        })
        .count();
    if pct(risky, known.len()) >= 10 {
        out.push(note(
            "light",
            "Watch for shake",
            format!(
                "{} % of your photos are slower than 1 over the focal length (1/50 s at 50 mm), where a hand starts to shake. Check them at 100 %; stabilisation or a faster shutter helps.",
                pct(risky, known.len())
            ),
        ));
    }
    out
}

fn habit_notes(records: &[Record]) -> Vec<Note> {
    let mut out = Vec::new();
    let tally = |key| super::tally(key, records);
    if let Some(t) = tally("focal") {
        let total: usize = t.bars.iter().map(|b| b.count).sum();
        if total >= ENOUGH {
            if let Some(top) = t.bars.iter().max_by_key(|b| b.count) {
                if pct(top.count, total) >= 40 {
                    out.push(note(
                        "habit",
                        format!("You see the world at {}", top.label),
                        format!(
                            "{} % of your photos with a focal length are at {}.",
                            pct(top.count, total),
                            top.label
                        ),
                    ));
                }
            }
        }
    }
    if let Some(t) = tally("hour") {
        let total: usize = t.bars.iter().map(|b| b.count).sum();
        // 6–9 and 17–20 as golden-hour stand-ins (no location or season).
        let golden: usize = t
            .bars
            .iter()
            .enumerate()
            .filter(|(h, _)| matches!(h, 6..=8 | 17..=19))
            .map(|(_, b)| b.count)
            .sum();
        if total >= ENOUGH && pct(golden, total) < 10 {
            out.push(note(
                "habit",
                "Mostly midday light",
                format!(
                    "Only {} % of your photos are from early morning or evening, when the light is low and warm.",
                    pct(golden, total)
                ),
            ));
        }
    }
    out
}

fn keeper_notes(records: &[Record]) -> Vec<Note> {
    let all_kept = records.iter().filter(|r| kept(r)).count();
    if all_kept < 5 || records.len() < 50 {
        return Vec::new();
    }
    let overall = all_kept as f64 / records.len() as f64;
    let mut best: Option<(f64, String, usize, usize)> = None;
    for key in ["aperture", "focal", "lens"] {
        let Some(t) = super::tally(key, records) else {
            continue;
        };
        for b in t.bars.iter().filter(|b| b.count >= ENOUGH) {
            let rate = b.kept as f64 / b.count as f64;
            if rate >= overall * 1.5 && best.as_ref().is_none_or(|x| rate > x.0) {
                best = Some((rate, b.label.clone(), b.kept, b.count));
            }
        }
    }
    best.map(|(_, label, k, n)| {
        note(
            "keepers",
            format!("Your keepers come at {label}"),
            format!(
                "{} % of the photos at {label} were kept ({k} of {n}), against {} % overall.",
                pct(k, n),
                (overall * 100.0).round()
            ),
        )
    })
    .into_iter()
    .collect()
}

/// Challenges: the far side of the usual focal length, aperture and time
/// of day, and one next to it. Three at most.
pub fn challenges(records: &[Record]) -> Vec<Note> {
    let mut out = Vec::new();
    let focal = median(records.iter().filter_map(|r| r.shot.focal).collect());
    let aperture = median(records.iter().filter_map(|r| r.shot.aperture).collect());
    if let Some(f) = focal.filter(|_| records.len() >= ENOUGH) {
        out.push(if f >= 35.0 {
            note(
                "challenge",
                "A week wide",
                format!("You're usually around {f:.0} mm. Shoot a week at 24 mm or wider: get close, fill the frame, let the edges lean."),
            )
        } else {
            note(
                "challenge",
                "A week long",
                format!("You're usually around {f:.0} mm. Shoot a week at 85 mm or longer: pick out details, flatten the distance, step back."),
            )
        });
    }
    if let Some(a) = aperture.filter(|_| records.len() >= ENOUGH) {
        out.push(if a >= 4.0 {
            note(
                "challenge",
                "Wide open",
                format!("Your usual aperture is {}. Try a day at the widest your lens goes: one thing sharp, the rest soft.", f_number(a)),
            )
        } else {
            note(
                "challenge",
                "Everything sharp",
                format!("Your usual aperture is {}. Try a day at f/8 to f/11: front to back in focus, scenes instead of subjects.", f_number(a)),
            )
        });
    }
    let hours = hours(records);
    if hours.len() >= ENOUGH {
        let night = hours.iter().filter(|h| is_night(**h)).count();
        out.push(if pct(night, hours.len()) < 10 {
            note(
                "challenge",
                "After dark",
                "Hardly any of your photos are at night. Try the blue hour after sunset, then street lights and long exposures.",
            )
        } else {
            note(
                "challenge",
                "Early light",
                "You know the night already. Try the first hour after sunrise: long shadows, soft colour, empty streets.",
            )
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::Shot;

    fn shot(lens: &str, aperture: f64, focal: f64, iso: u32, shutter: f64, hour: u32) -> Record {
        Record {
            shot: Shot {
                lens: Some(lens.into()),
                aperture: Some(aperture),
                focal: Some(focal),
                iso: Some(iso),
                shutter: Some(shutter),
                captured: Some(format!("2026-05-04 {hour:02}:00:00")),
                ..Shot::default()
            },
            ..Record::default()
        }
    }

    #[test]
    fn a_fast_lens_kept_stopped_down_is_said() {
        let mut rs: Vec<Record> = (0..40)
            .map(|_| shot("35mm F1.4", 4.0, 35.0, 200, 1.0 / 500.0, 12))
            .collect();
        rs.push(shot("35mm F1.4", 1.4, 35.0, 200, 1.0 / 500.0, 12));
        let notes = advice(&rs);
        let lens = notes
            .iter()
            .find(|n| n.kind == "lens")
            .expect("a lens note");
        assert!(lens.title.contains("rarely wide open"), "{lens:?}");
        assert!(
            lens.text.contains("f/1.4") && lens.text.contains("2 %"),
            "{}",
            lens.text
        );
        assert!(
            notes
                .iter()
                .any(|n| n.title == "You see the world at 35 mm")
        );
        assert!(notes.iter().any(|n| n.title == "Mostly midday light"));
    }

    #[test]
    fn a_zoom_at_its_end_and_dark_slow_shots() {
        let rs: Vec<Record> = (0..40)
            .map(|i| {
                shot(
                    "24-70",
                    2.8,
                    if i % 2 == 0 { 24.0 } else { 50.0 },
                    6400,
                    1.0 / 20.0,
                    22,
                )
            })
            .collect();
        let titles: Vec<String> = advice(&rs).into_iter().map(|n| n.title).collect();
        assert!(
            titles.iter().any(|t| t.contains("often at its widest")),
            "{titles:?}"
        );
        assert!(
            titles.contains(&"You shoot in the dark a lot".to_string()),
            "{titles:?}"
        );
        assert!(
            titles.contains(&"Watch for shake".to_string()),
            "{titles:?}"
        );
        let ch: Vec<String> = challenges(&rs).into_iter().map(|n| n.title).collect();
        assert_eq!(ch, ["A week wide", "Everything sharp", "Early light"]);
    }

    #[test]
    fn wide_and_stopped_down_is_landscapes() {
        let rs: Vec<Record> = (0..30)
            .map(|_| shot("24mm", 8.0, 24.0, 100, 1.0 / 250.0, 12))
            .collect();
        let s = summary(&rs).expect("a summary");
        assert_eq!(s.kind, "summary");
        assert_eq!(
            s.text,
            "Mostly wide (median 24 mm), stopped down (f/8), in daylight: the numbers say landscapes and buildings."
        );
        assert_eq!(notes(&rs).summary, Some(s));
        let night: Vec<Record> = (0..30)
            .map(|_| shot("85mm", 1.8, 85.0, 3200, 1.0 / 125.0, 22))
            .collect();
        let s = summary(&night).expect("a summary");
        assert!(
            s.text.contains("short tele")
                && s.text.contains("often at night")
                && s.text.contains("portraits"),
            "{}",
            s.text
        );
    }

    #[test]
    fn every_band_pair_has_a_phrase() {
        for (f, _, _) in FOCAL_BANDS {
            for (a, _, _) in APERTURE_BANDS {
                assert!(STYLES.iter().any(|s| s.0 == *f && s.1 == *a));
            }
        }
    }

    #[test]
    fn few_photos_say_nothing() {
        let rs: Vec<Record> = (0..5)
            .map(|_| shot("x", 1.4, 35.0, 6400, 0.5, 12))
            .collect();
        assert!(advice(&rs).is_empty() && challenges(&rs).is_empty() && summary(&rs).is_none());
    }
}
