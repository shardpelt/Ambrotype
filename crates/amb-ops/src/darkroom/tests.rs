// SPDX-License-Identifier: GPL-3.0-or-later
//! Tests of the darkroom aids.

use super::*;

fn negative_photo(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("amb-darkroom-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("neg.dng");
    amb_testkit::dng::write(
        &path,
        &amb_testkit::patterns::negative(),
        &Default::default(),
    )
    .unwrap();
    path
}

fn on() -> BTreeMap<String, Value> {
    BTreeMap::from([(FLAG_NEGATIVE.to_string(), Value::Bool(true))])
}

fn mean(img: &RgbImage, c: &Cell, channel: usize) -> f64 {
    let mut sum = 0.0;
    for y in c.y..c.y + c.h {
        for x in c.x..c.x + c.w {
            sum += f64::from(img.get_pixel(x, y)[channel]);
        }
    }
    sum / f64::from(c.w * c.h)
}

#[test]
fn exposure_follows_how_the_photo_prints() {
    let mut s = BTreeMap::new();
    assert_eq!(exposure_setting(&s), "Exposure2012");
    s.insert(FLAG_NEGATIVE.into(), Value::Bool(true));
    assert_eq!(exposure_setting(&s), "amb:NegativeExposure");
    s.insert(FLAG_PAPER.into(), Value::Bool(true));
    assert_eq!(exposure_setting(&s), "amb:NegativeExposure");
    s.insert(FLAG_SPLIT.into(), Value::Bool(true));
    assert_eq!(exposure_setting(&s), "amb:PaperSoftExposure");
}

#[test]
fn labels_are_thirds_of_a_stop() {
    assert_eq!(delta_label(1.0 / 3.0, true), "+1/3");
    assert_eq!(delta_label(-2.0 / 3.0, true), "-2/3");
    assert_eq!(delta_label(4.0 / 3.0, true), "+1 1/3");
    assert_eq!(delta_label(0.0, true), "As now");
    assert_eq!(delta_label(15.0, false), "+15");
}

#[test]
fn a_strip_has_a_band_per_step_and_stops_at_the_range() {
    let photo = negative_photo("strip");
    let o = StripOptions {
        size: 160,
        ..Default::default()
    };
    let aid = test_strip(&photo, &on(), &o, &|_, _| {}).unwrap();
    assert_eq!(aid.cells.len(), 7);
    assert_eq!(aid.cells[3].label, "As now");
    assert_eq!(aid.cells[0].label, "-1");
    assert_eq!(aid.cells[6].changes["amb:NegativeExposure"], 1.0);
    let covered: u32 = aid.cells.iter().map(|c| c.w).sum();
    assert_eq!(covered, aid.width);
    // At the top of the range the steps beyond it are dropped.
    let mut s = on();
    s.insert("amb:NegativeExposure".into(), Value::Number(3.9));
    let aid = test_strip(&photo, &s, &o, &|_, _| {}).unwrap();
    assert!(aid.cells.len() < 7);
    assert!(
        aid.cells
            .iter()
            .all(|c| c.changes.values().all(|v| *v <= 4.0))
    );
}

#[test]
fn a_ring_moves_each_colour_the_way_it_says() {
    let photo = negative_photo("ring");
    let o = RingOptions {
        size: 120,
        ..Default::default()
    };
    let aid = ring_around(&photo, &on(), &o, &|_, _| {}).unwrap();
    assert_eq!(aid.cells.len(), 9);
    let by = |label: &str| aid.cells.iter().find(|c| c.label == label).unwrap();
    let mid = by("As now");
    let m = |label: &str, ch: usize| mean(&aid.image, by(label), ch);
    let now = |ch: usize| mean(&aid.image, mid, ch);
    assert!(m("More red", 0) > now(0), "{} {}", m("More red", 0), now(0));
    assert!(m("More green", 1) > now(1));
    assert!(m("More blue", 2) > now(2));
    assert!(m("More cyan", 0) < now(0));
    assert!(m("More magenta", 1) < now(1));
    assert!(m("More yellow", 2) < now(2));
    let luma = |label: &str| (0..3).map(|c| m(label, c)).sum::<f64>();
    let centre: f64 = (0..3).map(now).sum();
    assert!(luma("Lighter") > centre && luma("Darker") < centre);
    assert!(by("Lighter").changes["amb:NegativeExposure"] > 0.0);
}

#[test]
fn lighter_is_lighter_on_paper_too() {
    let photo = negative_photo("paper");
    let mut s = on();
    s.insert(FLAG_PAPER.into(), Value::Bool(true));
    s.insert(FLAG_SPLIT.into(), Value::Bool(true));
    let o = RingOptions {
        size: 120,
        ..Default::default()
    };
    let aid = ring_around(&photo, &s, &o, &|_, _| {}).unwrap();
    let luma = |label: &str| {
        let c = aid.cells.iter().find(|c| c.label == label).unwrap();
        (0..3).map(|ch| mean(&aid.image, c, ch)).sum::<f64>()
    };
    assert!(
        luma("Lighter") > luma("As now"),
        "{} {}",
        luma("Lighter"),
        luma("As now")
    );
    assert!(luma("Darker") < luma("As now"));
    let lighter = aid.cells.iter().find(|c| c.label == "Lighter").unwrap();
    assert!(lighter.changes["amb:PaperSoftExposure"] < 0.0);
}

#[test]
fn a_ring_needs_a_converted_negative() {
    let err = ring_around(
        Path::new("x.dng"),
        &BTreeMap::new(),
        &RingOptions::default(),
        &|_, _| {},
    )
    .unwrap_err();
    assert!(err.contains("switch on Film Negative"), "{err}");
}
