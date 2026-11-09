// SPDX-License-Identifier: GPL-3.0-or-later
//! Tests for the mask commands.

use super::*;

fn obj(v: serde_json::Value) -> Map<String, serde_json::Value> {
    v.as_object().unwrap().clone()
}

#[test]
fn background_is_the_subject_inverted_and_people_its_own_kind() {
    let mut s = BTreeMap::new();
    let mut clamped = Vec::new();
    add(&mut s, "background", None, None, None, &mut clamped).unwrap();
    add(&mut s, "people", None, None, None, &mut clamped).unwrap();
    let Value::Seq(list) = &s[MASK_TAG] else {
        panic!()
    };
    let part = |i: usize| -> Fields {
        let Value::Struct(c) = &list[i] else { panic!() };
        let Some(Value::Seq(parts)) = c.get("CorrectionMasks") else {
            panic!()
        };
        let Value::Struct(p) = &parts[0] else {
            panic!()
        };
        p.clone()
    };
    let (b, p) = (part(0), part(1));
    assert_eq!(masks::text(&b, "MaskSubType").as_deref(), Some("1"));
    assert_eq!(b.get("MaskInverted"), Some(&boolean(true)));
    assert_eq!(masks::text(&p, "MaskSubType").as_deref(), Some("3"));
    assert_eq!(p.get("MaskInverted"), Some(&boolean(false)));
}

#[test]
fn add_set_paint_and_remove() {
    let mut s = BTreeMap::new();
    let mut clamped = Vec::new();
    add(
        &mut s,
        "linear",
        None,
        None,
        Some(&obj(json!({"ZeroY": 0.6, "FullY": 0.3}))),
        &mut clamped,
    )
    .unwrap();
    set(
        &mut s,
        0,
        None,
        &obj(json!({"Exposure": -0.5, "LocalHighlights2012": -30, "name": "Sky"})),
        &mut clamped,
    )
    .unwrap();
    add(
        &mut s,
        "luminance",
        Some(0),
        Some("intersect"),
        Some(&obj(json!({"LumRange": [0.3, 0.5, 1, 1]}))),
        &mut clamped,
    )
    .unwrap();
    paint(
        &mut s,
        Some(0),
        None,
        Some("subtract"),
        &Stroke {
            dabs: None,
            points: &[[0.1, 0.1], [0.2, 0.1]],
            size: 0.05,
            feather: 50.0,
            flow: 100.0,
            density: 100.0,
            erase: false,
        },
        &mut clamped,
    )
    .unwrap();
    assert!(clamped.is_empty(), "{clamped:?}");

    let Value::Seq(list) = &s[MASK_TAG] else {
        panic!()
    };
    let Value::Struct(c) = &list[0] else { panic!() };
    // Stored as Lightroom stores them.
    assert_eq!(
        masks::text(c, "LocalExposure2012").as_deref(),
        Some("-0.125")
    );
    assert_eq!(
        masks::text(c, "LocalHighlights2012").as_deref(),
        Some("-0.3")
    );
    let comps = masks::components(c);
    let Value::Struct(range) = &comps[1] else {
        panic!()
    };
    assert_eq!(masks::text(range, "MaskBlendMode").as_deref(), Some("1"));
    assert_eq!(masks::text(range, "MaskValue").as_deref(), Some("0"));
    assert_eq!(masks::flag(range, "MaskInverted"), Some(true));
    assert_eq!(
        masks::range_mask(range).and_then(|r| masks::flag(r, "Invert")),
        Some(true)
    );

    let d = describe(&s);
    let m = &d["masks"][0];
    assert_eq!(m["name"], "Sky");
    assert_eq!(m["sliders"]["LocalExposure2012"], -0.5);
    assert_eq!(m["parts"][1]["mode"], "intersect");
    assert_eq!(m["parts"][2]["kind"], "brush");
    assert_eq!(m["parts"][2]["mode"], "subtract");
    // Dabs fill the gap between the two points.
    assert!(m["parts"][2]["dabs"].as_u64().unwrap() >= 8, "{m}");
    // Written in Lightroom's current layout: an aggregate holding strokes.
    let Value::Seq(list) = &s[MASK_TAG] else {
        panic!()
    };
    let Value::Struct(c) = &list[0] else { panic!() };
    let Value::Struct(brush) = &masks::components(c)[2] else {
        panic!()
    };
    assert_eq!(
        masks::text(brush, "What").as_deref(),
        Some("Mask/Aggregate")
    );
    assert_eq!(masks::text(brush, "MaskBlendMode").as_deref(), Some("1"));
    assert_eq!(masks::brush_strokes(&[brush]).len(), 1);

    // A second stroke joins the brush part; then erase in it.
    let stroke = |erase| Stroke {
        dabs: None,
        points: &[[0.3, 0.3]],
        size: 0.05,
        feather: 0.0,
        flow: 100.0,
        density: 100.0,
        erase,
    };
    paint(&mut s, Some(0), Some(2), None, &stroke(false), &mut clamped).unwrap();
    paint(&mut s, Some(0), Some(2), None, &stroke(true), &mut clamped).unwrap();
    assert_eq!(describe(&s)["masks"][0]["parts"][2]["strokes"], 3);

    set(
        &mut s,
        0,
        Some(2),
        &obj(json!({"mode": "add"})),
        &mut clamped,
    )
    .unwrap();
    let d = describe(&s);
    assert_eq!(d["masks"][0]["parts"][2]["mode"], "add");

    remove(&mut s, 0, Some(1)).unwrap();
    assert_eq!(
        describe(&s)["masks"][0]["parts"].as_array().unwrap().len(),
        2
    );
    remove(&mut s, 0, None).unwrap();
    assert!(!s.contains_key(MASK_TAG));
}

#[test]
fn mode_and_invert_keep_lightroom_mirrors() {
    let mut s = BTreeMap::new();
    let mut clamped = Vec::new();
    add(&mut s, "radial", None, None, None, &mut clamped).unwrap();
    set(
        &mut s,
        0,
        Some(0),
        &obj(json!({"invert": true, "Feather": 120})),
        &mut clamped,
    )
    .unwrap();
    assert_eq!(clamped.len(), 1, "Feather clamped");
    let Value::Seq(list) = &s[MASK_TAG] else {
        panic!()
    };
    let Value::Struct(c) = &list[0] else { panic!() };
    let Value::Struct(e) = &masks::components(c)[0] else {
        panic!()
    };
    assert_eq!(masks::flag(e, "MaskInverted"), Some(true));
    assert_eq!(masks::flag(e, "Flipped"), Some(false));
}

#[test]
fn duplicate_and_move() {
    let mut s = BTreeMap::new();
    let mut clamped = Vec::new();
    add(&mut s, "linear", None, None, None, &mut clamped).unwrap();
    let stroke = Stroke {
        dabs: None,
        points: &[[0.2, 0.2], [0.4, 0.2]],
        size: 0.05,
        feather: 50.0,
        flow: 100.0,
        density: 100.0,
        erase: false,
    };
    paint(&mut s, None, None, None, &stroke, &mut clamped).unwrap();
    paint(&mut s, Some(1), Some(0), None, &stroke, &mut clamped).unwrap();
    duplicate(&mut s, 1).unwrap();
    let names = |s: &BTreeMap<String, Value>| -> Vec<String> {
        let d = describe(s);
        d["masks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["name"].as_str().unwrap().to_string())
            .collect()
    };
    assert_eq!(names(&s), ["Mask 1", "Mask 2", "Mask 2 copy"]);
    let ids = |i: usize| -> (String, Vec<String>) {
        let Value::Seq(list) = &s[MASK_TAG] else {
            panic!()
        };
        let Value::Struct(c) = &list[i] else { panic!() };
        let strokes = masks::components(c)
            .iter()
            .filter_map(|e| match e {
                Value::Struct(e) => masks::text(e, "MaskSyncID"),
                _ => None,
            })
            .collect();
        (masks::text(c, "CorrectionSyncID").unwrap(), strokes)
    };
    let ((a, a_strokes), (b, b_strokes)) = (ids(1), ids(2));
    assert_ne!(a, b);
    assert_eq!(a_strokes.len(), b_strokes.len());
    assert!(a_strokes.iter().all(|id| !b_strokes.contains(id)));
    move_to(&mut s, 2, 0).unwrap();
    assert_eq!(names(&s), ["Mask 2 copy", "Mask 1", "Mask 2"]);
    move_to(&mut s, 0, 2).unwrap();
    assert_eq!(names(&s), ["Mask 1", "Mask 2", "Mask 2 copy"]);
    assert!(move_to(&mut s, 0, 3).is_err());
    assert!(duplicate(&mut s, 5).is_err());
}

#[test]
fn parts_move_within_their_mask() {
    let mut s = BTreeMap::new();
    let mut clamped = Vec::new();
    add(&mut s, "linear", None, None, None, &mut clamped).unwrap();
    add(
        &mut s,
        "radial",
        Some(0),
        Some("subtract"),
        None,
        &mut clamped,
    )
    .unwrap();
    add(
        &mut s,
        "luminance",
        Some(0),
        Some("add"),
        None,
        &mut clamped,
    )
    .unwrap();
    let order = |s: &BTreeMap<String, Value>| -> Vec<(String, String)> {
        describe(s)["masks"][0]["parts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| {
                (
                    p["kind"].as_str().unwrap().into(),
                    p["mode"].as_str().unwrap().into(),
                )
            })
            .collect()
    };
    let kinds = |s: &BTreeMap<String, Value>| order(s).into_iter().map(|p| p.0).collect::<Vec<_>>();
    assert_eq!(kinds(&s), ["linear", "radial", "luminance"]);
    move_part(&mut s, 0, 2, 0).unwrap();
    assert_eq!(kinds(&s), ["luminance", "linear", "radial"]);
    assert_eq!(order(&s)[2].1, "subtract");
    // A subtracting part can't come first.
    let err = move_part(&mut s, 0, 2, 0).unwrap_err().to_string();
    assert!(err.contains("first part"), "{err}");
    assert!(move_part(&mut s, 0, 1, 3).is_err());
    assert!(move_part(&mut s, 0, 5, 0).is_err());
}

#[test]
fn mistakes_are_explained() {
    let mut s = BTreeMap::new();
    let mut clamped = Vec::new();
    let err = set(&mut s, 0, None, &obj(json!({"Exposure": 1})), &mut clamped).unwrap_err();
    assert!(err.to_string().contains("no masks yet"), "{err}");
    add(&mut s, "radial", None, None, None, &mut clamped).unwrap();
    let err = set(
        &mut s,
        0,
        Some(0),
        &obj(json!({"ZeroX": 0.2})),
        &mut clamped,
    )
    .unwrap_err();
    assert!(
        err.to_string().contains("not a Radial Gradient setting"),
        "{err}"
    );
    let err = set(&mut s, 0, Some(0), &obj(json!({"Top": 0.9})), &mut clamped).unwrap_err();
    assert!(err.to_string().contains("Top < Bottom"), "{err}");
    let err = set(&mut s, 0, None, &obj(json!({"Exposur": 1})), &mut clamped).unwrap_err();
    assert!(err.to_string().contains("LocalExposure2012"), "{err}");
    let err = add(&mut s, "depth", None, None, None, &mut clamped).unwrap_err();
    assert!(err.to_string().contains("linear"), "{err}");
}

#[test]
fn preset_masks_join_and_scale() {
    let mut s = BTreeMap::new();
    let mut clamped = Vec::new();
    add(&mut s, "linear", None, None, None, &mut clamped).unwrap();
    set(&mut s, 0, None, &obj(json!({"Exposure": 1})), &mut clamped).unwrap();
    let incoming = s[MASK_TAG].clone();
    // Same id: replaced, scaled to half.
    merge(&mut s, &incoming, 0.5);
    let d = describe(&s);
    assert_eq!(d["masks"].as_array().unwrap().len(), 1);
    assert_eq!(d["masks"][0]["sliders"]["LocalExposure2012"], 0.5);
}

#[test]
fn given_dabs_keep_their_own_sizes() {
    let mut s = BTreeMap::new();
    let dabs = [[0.5, 0.5, 0.1], [0.62, 0.5, 0.1], [0.7, 0.5, 0.02]];
    paint(
        &mut s,
        None,
        None,
        None,
        &Stroke {
            dabs: Some(&dabs),
            points: &[],
            size: 0.05,
            feather: 0.0,
            flow: 100.0,
            density: 100.0,
            erase: false,
        },
        &mut Vec::new(),
    )
    .unwrap();
    let text = format!("{:?}", s);
    // Three dabs, the brush's radius the first's, one size change.
    assert_eq!(text.matches("d 0.").count(), 3, "{text}");
    assert!(text.contains("r 0.020000"), "{text}");
    assert!(!text.contains("r 0.100000"), "{text}");
}
