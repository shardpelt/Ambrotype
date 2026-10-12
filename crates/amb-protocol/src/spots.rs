// SPDX-License-Identifier: GPL-3.0-or-later
//! Spot commands: `add_spot`, `set_spot`, `remove_spot`, and the `list_spots`
//! description. They edit `RetouchAreas` in Lightroom's own layout
//! ([`amb_presets::retouch`]). Spots are addressed by position from 0, as
//! `list_spots` shows them. Positions are fractions of the uncropped photo in
//! stored orientation; sizes are fractions of its width.

use std::collections::BTreeMap;

use amb_presets::Value;
use amb_presets::masks::{self, boolean, number};
use amb_presets::retouch::{self, LEGACY_TAG, RETOUCH_TAG, SpotKind};
use serde_json::{Map, json};

use crate::Error;
use crate::convert::{Clamp, json_number};

type Fields = BTreeMap<String, Value>;

fn invalid(name: impl Into<String>, reason: impl Into<String>) -> Error {
    Error::InvalidValue {
        name: name.into(),
        reason: reason.into(),
    }
}

fn clamp(name: &str, v: f64, lo: f64, hi: f64, clamped: &mut Vec<Clamp>) -> f64 {
    let c = v.clamp(lo, hi);
    if c != v {
        clamped.push(Clamp {
            name: name.to_string(),
            requested: v,
            applied: c,
            min: lo,
            max: hi,
        });
    }
    c
}

fn parse_kind(s: &str) -> Result<SpotKind, Error> {
    match s.trim().to_ascii_lowercase().as_str() {
        "heal" => Ok(SpotKind::Heal),
        "clone" => Ok(SpotKind::Clone),
        other => Err(invalid("kind", format!("{other:?} is not heal or clone"))),
    }
}

fn point(name: &str, v: &serde_json::Value) -> Result<(f64, f64), Error> {
    match v.as_array().map(|a| a.as_slice()) {
        Some([x, y]) => match (json_number(x), json_number(y)) {
            (Some(x), Some(y)) if x.is_finite() && y.is_finite() => Ok((x, y)),
            _ => Err(invalid(name, format!("{v} is not [x, y]"))),
        },
        _ => Err(invalid(name, format!("{v} is not [x, y]"))),
    }
}

fn spots_of(settings: &BTreeMap<String, Value>) -> Vec<Value> {
    retouch::areas(settings).to_vec()
}

fn store(next: &mut BTreeMap<String, Value>, list: Vec<Value>) {
    if list.is_empty() {
        next.remove(RETOUCH_TAG);
    } else {
        next.insert(RETOUCH_TAG.to_string(), Value::Seq(list));
    }
}

fn spot_mut(list: &mut [Value], spot: usize) -> Result<&mut Fields, Error> {
    let n = list.len();
    match list.get_mut(spot) {
        Some(Value::Struct(m)) => Ok(m),
        Some(_) => Err(invalid(format!("spot {spot}"), "is not a retouch area")),
        None => Err(invalid(
            format!("spot {spot}"),
            if n == 0 {
                "there are no spots yet; add one with add_spot".to_string()
            } else {
                format!("there are {n} spot(s), numbered from 0")
            },
        )),
    }
}

/// A new spot's settings, as the command gives them.
pub struct NewSpot<'a> {
    pub kind: Option<&'a str>,
    pub at: [f64; 2],
    pub radius: f64,
    pub source: Option<[f64; 2]>,
    pub source_auto: bool,
    /// 0-100.
    pub feather: f64,
    /// 0-100.
    pub opacity: f64,
    /// A brushed spot's stroke (`at` is then unused).
    pub points: Option<&'a [[f64; 2]]>,
}

/// `add_spot`: a round heal or clone spot, or with `points` a brushed one.
pub fn add(
    next: &mut BTreeMap<String, Value>,
    s: &NewSpot,
    clamped: &mut Vec<Clamp>,
) -> Result<(), Error> {
    let kind = s
        .kind
        .map(parse_kind)
        .transpose()?
        .unwrap_or(SpotKind::Heal);
    if s.at
        .iter()
        .chain(s.source.iter().flatten())
        .any(|v| !v.is_finite())
    {
        return Err(invalid("at", "positions must be finite numbers"));
    }
    let radius = clamp("radius", s.radius, 0.001, 0.5, clamped);
    let feather = clamp("feather", s.feather, 0.0, 100.0, clamped) / 100.0;
    let opacity = clamp("opacity", s.opacity, 1.0, 100.0, clamped) / 100.0;
    let mut area = retouch::new_area(
        kind,
        (s.at[0], s.at[1]),
        radius,
        s.source.map(|p| (p[0], p[1])),
        s.source_auto,
        feather,
        opacity,
    );
    if let Some(points) = s.points {
        if points.len() < 2 || points.iter().flatten().any(|v| !v.is_finite()) {
            return Err(invalid(
                "points",
                "a brushed spot needs two or more [x, y] points",
            ));
        }
        // The same stroke format as a brush mask; the spot's own Feather
        // gives the brush's softness.
        let dabs = crate::masks::dabs_along(points, radius)?;
        let mut e: Fields = BTreeMap::new();
        e.insert("What".into(), Value::Text("Mask/Paint".into()));
        e.insert("MaskActive".into(), boolean(true));
        e.insert("MaskBlendMode".into(), Value::Text("0".into()));
        e.insert("MaskInverted".into(), boolean(false));
        e.insert("MaskValue".into(), number(1.0));
        e.insert("Radius".into(), number(radius));
        e.insert("Flow".into(), number(1.0));
        e.insert("CenterWeight".into(), number(1.0 - feather));
        e.insert(
            "Dabs".into(),
            Value::Seq(
                dabs.iter()
                    .map(|&(x, y)| Value::Text(masks::format_dab(x, y)))
                    .collect(),
            ),
        );
        area.insert("Masks".into(), Value::Seq(vec![Value::Struct(e)]));
    }
    let mut list = spots_of(next);
    list.push(Value::Struct(area));
    store(next, list);
    Ok(())
}

const KEYS: &[&str] = &[
    "at", "radius", "source", "kind", "feather", "opacity", "active",
];

/// `set_spot`: move, resize, re-source or restyle a spot.
pub fn set(
    next: &mut BTreeMap<String, Value>,
    spot: usize,
    values: &Map<String, serde_json::Value>,
    clamped: &mut Vec<Clamp>,
) -> Result<(), Error> {
    let mut list = spots_of(next);
    let a = spot_mut(&mut list, spot)?;
    let at = format!("spot {spot}");
    for (key, v) in values {
        let name = format!("{at} {key}");
        let num = |v: &serde_json::Value| {
            json_number(v)
                .filter(|n| n.is_finite())
                .ok_or_else(|| invalid(&name, format!("expects a number, got {v}")))
        };
        match key.as_str() {
            "at" | "radius" => {
                let Some(Value::Seq(items)) = a.get_mut("Masks") else {
                    return Err(invalid(
                        &name,
                        "only a round spot can be moved or resized this way",
                    ));
                };
                let Some(Value::Struct(e)) = items.iter_mut().find(|c| {
                    matches!(c, Value::Struct(m) if masks::text(m, "What").as_deref() == Some("Mask/Ellipse"))
                }) else {
                    return Err(invalid(&name, "this spot is brushed; only a round spot can be moved or resized"));
                };
                if key == "at" {
                    let (x, y) = point(&name, v)?;
                    e.insert("X".into(), number(x));
                    e.insert("Y".into(), number(y));
                } else {
                    let r = clamp(&name, num(v)?, 0.001, 0.5, clamped);
                    e.insert("SizeX".into(), number(r));
                    e.insert("SizeY".into(), number(r));
                }
            }
            "source" => {
                if v.is_null() {
                    a.remove("SourceX");
                    a.remove("OffsetY");
                    a.insert(
                        "SourceState".into(),
                        Value::Text("sourceAutoComputed".into()),
                    );
                } else {
                    let (x, y) = point(&name, v)?;
                    a.insert("SourceX".into(), number(x));
                    a.insert("OffsetY".into(), number(y));
                    a.insert(
                        "SourceState".into(),
                        Value::Text("sourceSetExplicitly".into()),
                    );
                }
            }
            "kind" => {
                let k = parse_kind(
                    v.as_str()
                        .ok_or_else(|| invalid(&name, "expects heal or clone"))?,
                )?;
                a.insert("SpotType".into(), Value::Text(k.name().into()));
                a.remove("fill_method");
            }
            "feather" => {
                let f = clamp(&name, num(v)?, 0.0, 100.0, clamped);
                a.insert("Feather".into(), number(f / 100.0));
            }
            "opacity" => {
                let o = clamp(&name, num(v)?, 1.0, 100.0, clamped);
                a.insert("Opacity".into(), number(o / 100.0));
            }
            "active" => {
                let on = match v {
                    serde_json::Value::Bool(b) => *b,
                    _ => return Err(invalid(&name, "expects true or false")),
                };
                if let Some(Value::Seq(items)) = a.get_mut("Masks") {
                    for c in items {
                        if let Value::Struct(m) = c {
                            m.insert("MaskActive".into(), boolean(on));
                        }
                    }
                }
            }
            _ => {
                return Err(invalid(
                    &name,
                    format!("unknown spot value; use one of: {}", KEYS.join(", ")),
                ));
            }
        }
    }
    store(next, list);
    Ok(())
}

/// `remove_spot`.
pub fn remove(next: &mut BTreeMap<String, Value>, spot: usize) -> Result<(), Error> {
    let mut list = spots_of(next);
    spot_mut(&mut list, spot)?;
    list.remove(spot);
    store(next, list);
    Ok(())
}

/// The spots of `settings` as the editor and AI clients see them.
pub fn describe(settings: &BTreeMap<String, Value>) -> serde_json::Value {
    let spots: Vec<serde_json::Value> = retouch::areas(settings)
        .iter()
        .enumerate()
        .filter_map(|(i, v)| {
            let Value::Struct(a) = v else { return None };
            let kind = SpotKind::of(a)?;
            let mut d = json!({
                "index": i,
                "kind": kind.name(),
                "source_auto": masks::text(a, "SourceState").as_deref() != Some("sourceSetExplicitly"),
                "feather": (masks::num(a, "Feather").unwrap_or(0.5) * 100.0).round(),
                "opacity": (masks::num(a, "Opacity").unwrap_or(1.0) * 100.0).round(),
                "active": retouch::shape_components(a).iter().all(|c| masks::flag(c, "MaskActive") != Some(false)),
            });
            if !kind.synthesised() {
                if let Some((x, y)) = retouch::source(a) {
                    d["source"] = json!([x, y]);
                }
            }
            match retouch::ellipse(a) {
                Some(((x, y), (sx, sy))) => {
                    d["shape"] = json!("round");
                    d["at"] = json!([x, y]);
                    d["radius"] = json!(sx.max(sy));
                }
                None => d["shape"] = json!("brush"),
            }
            if kind.synthesised() {
                d["note"] = json!("Lightroom filled this spot itself; we heal it from a nearby area");
            }
            Some(d)
        })
        .collect();
    let mut out = json!({ "spots": spots });
    if let Some(Value::Seq(items)) = settings.get(LEGACY_TAG) {
        out["legacy"] = json!(items.len());
        out["note"] = json!(
            "older Lightroom spots (RetouchInfo) are rendered but can't be edited with these commands"
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obj(v: serde_json::Value) -> Map<String, serde_json::Value> {
        v.as_object().unwrap().clone()
    }

    fn spot(at: [f64; 2]) -> NewSpot<'static> {
        NewSpot {
            kind: None,
            at,
            radius: 0.02,
            source: None,
            source_auto: false,
            feather: 50.0,
            opacity: 100.0,
            points: None,
        }
    }

    #[test]
    fn a_brushed_spot_is_a_paint_stroke() {
        let mut s = BTreeMap::new();
        let mut clamped = Vec::new();
        let stroke = [[0.2, 0.3], [0.3, 0.3], [0.3, 0.4]];
        add(
            &mut s,
            &NewSpot {
                points: Some(&stroke),
                source: Some([0.5, 0.5]),
                ..spot([0.0, 0.0])
            },
            &mut clamped,
        )
        .unwrap();
        let d = describe(&s);
        assert_eq!(d["spots"][0]["shape"], "brush");
        let Value::Seq(areas) = &s[RETOUCH_TAG] else {
            panic!()
        };
        let Value::Struct(a) = &areas[0] else {
            panic!()
        };
        let comps = retouch::shape_components(a);
        assert_eq!(masks::text(comps[0], "What").as_deref(), Some("Mask/Paint"));
        // Dabs every 0.2 × the radius along the stroke.
        let Some(Value::Seq(dabs)) = comps[0].get("Dabs") else {
            panic!()
        };
        assert!(
            dabs.len() >= 2 * (0.1 / (0.2 * 0.02)) as usize,
            "{}",
            dabs.len()
        );
        assert!(
            add(
                &mut s,
                &NewSpot {
                    points: Some(&stroke[..1]),
                    ..spot([0.0, 0.0])
                },
                &mut clamped
            )
            .is_err()
        );
    }

    #[test]
    fn add_set_remove() {
        let mut s = BTreeMap::new();
        let mut clamped = Vec::new();
        add(&mut s, &spot([0.3, 0.4]), &mut clamped).unwrap();
        add(
            &mut s,
            &NewSpot {
                kind: Some("clone"),
                source: Some([0.5, 0.5]),
                ..spot([0.6, 0.6])
            },
            &mut clamped,
        )
        .unwrap();
        let d = describe(&s);
        assert_eq!(d["spots"][0]["kind"], "heal");
        assert_eq!(d["spots"][0]["source_auto"], true);
        assert_eq!(d["spots"][1]["source"], json!([0.5, 0.5]));
        assert_eq!(d["spots"][1]["source_auto"], false);

        set(
            &mut s,
            0,
            &obj(json!({"at": [0.35, 0.45], "radius": 0.9, "source": [0.2, 0.2], "kind": "clone"})),
            &mut clamped,
        )
        .unwrap();
        assert_eq!(clamped.len(), 1, "radius clamped");
        let d = describe(&s);
        assert_eq!(d["spots"][0]["at"], json!([0.35, 0.45]));
        assert_eq!(d["spots"][0]["radius"], 0.5);
        assert_eq!(d["spots"][0]["kind"], "clone");
        set(&mut s, 0, &obj(json!({"source": null})), &mut clamped).unwrap();
        assert_eq!(describe(&s)["spots"][0]["source_auto"], true);

        let err = set(&mut s, 0, &obj(json!({"size": 1})), &mut clamped).unwrap_err();
        assert!(err.to_string().contains("radius"), "{err}");
        remove(&mut s, 0).unwrap();
        remove(&mut s, 0).unwrap();
        assert!(!s.contains_key(RETOUCH_TAG));
        assert!(
            remove(&mut s, 0)
                .unwrap_err()
                .to_string()
                .contains("no spots yet")
        );
    }
}
