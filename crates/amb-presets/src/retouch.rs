// SPDX-License-Identifier: GPL-3.0-or-later
//! Spot removal: a typed view of Camera Raw's retouch tags.
//!
//! Like masks, spots stay plain settings and round-trip untouched; this module
//! only reads and builds them. Facts from Lightroom-written files (see
//! docs/design/spot-removal.md):
//! - `RetouchAreas` (current) is a list of areas. Each has `SpotType`
//!   (`heal`, `clone`, or `heal_patchmatch` for content-aware, with
//!   `fill_method="firefly"` when generative), `SourceState`
//!   (`sourceAutoComputed` / `sourceSetExplicitly`), the donor's centre as
//!   absolute fractions in `SourceX` and (despite its name) `OffsetY`,
//!   `Feather` 0..1, `Opacity` 0..1, and its shape in `Masks`: a
//!   `Mask/Ellipse` (`X`, `Y` the centre; `SizeX`, `SizeY` half-extents as
//!   fractions of the frame's width, so a round spot has equal sizes) or
//!   `Mask/Paint` brush strokes.
//! - `RetouchInfo` (pre-2018) is a list of strings,
//!   `centerX = 0.5, centerY = 0.4, radius = 0.02, sourceState = …,
//!   sourceX = 0.6, sourceY = 0.4, spotType = heal`.
//! - Coordinates are fractions of the uncropped photo in stored orientation.

use std::collections::BTreeMap;

use crate::masks::{boolean, new_sync_id, num, number, text};
use crate::value::Value;

pub const RETOUCH_TAG: &str = "RetouchAreas";
pub const LEGACY_TAG: &str = "RetouchInfo";
pub const ENABLE_TAG: &str = "EnableRetouch";

/// How a spot is filled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpotKind {
    /// Copy the donor, then match it to the surroundings.
    Heal,
    /// Copy the donor as it is.
    Clone,
    /// Lightroom's content-aware fill: no donor is stored. We heal from a
    /// donor we search for.
    ContentAware,
    /// Lightroom's generative fill (Firefly). Its pixels are not in the file;
    /// we heal from a searched donor instead.
    Generative,
}

impl SpotKind {
    pub fn of(area: &BTreeMap<String, Value>) -> Option<SpotKind> {
        match text(area, "SpotType")?.as_str() {
            "heal" => Some(SpotKind::Heal),
            "clone" => Some(SpotKind::Clone),
            "heal_patchmatch" if text(area, "fill_method").as_deref() == Some("firefly") => {
                Some(SpotKind::Generative)
            }
            "heal_patchmatch" => Some(SpotKind::ContentAware),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            SpotKind::Heal => "heal",
            SpotKind::Clone => "clone",
            SpotKind::ContentAware => "content-aware",
            SpotKind::Generative => "generative",
        }
    }

    /// Whether Lightroom made up the pixels (we can only approximate).
    pub fn synthesised(self) -> bool {
        matches!(self, SpotKind::ContentAware | SpotKind::Generative)
    }
}

/// The areas of `RetouchAreas`.
pub fn areas(settings: &BTreeMap<String, Value>) -> &[Value] {
    match settings.get(RETOUCH_TAG) {
        Some(Value::Seq(items)) => items,
        _ => &[],
    }
}

/// An area's shape components (`Masks`).
pub fn shape_components(area: &BTreeMap<String, Value>) -> Vec<&BTreeMap<String, Value>> {
    match area.get("Masks") {
        Some(Value::Seq(items)) => items
            .iter()
            .filter_map(|v| match v {
                Value::Struct(m) => Some(m),
                _ => None,
            })
            .collect(),
        Some(Value::Struct(m)) => vec![m],
        _ => Vec::new(),
    }
}

/// The ellipse of an area: centre (fractions) and half-extents (fractions of
/// the frame width).
pub fn ellipse(area: &BTreeMap<String, Value>) -> Option<((f64, f64), (f64, f64))> {
    let e = shape_components(area)
        .into_iter()
        .find(|c| text(c, "What").as_deref() == Some("Mask/Ellipse"))?;
    Some((
        (num(e, "X")?, num(e, "Y")?),
        (num(e, "SizeX")?.abs(), num(e, "SizeY")?.abs()),
    ))
}

/// The donor centre, when the area names one.
pub fn source(area: &BTreeMap<String, Value>) -> Option<(f64, f64)> {
    Some((num(area, "SourceX")?, num(area, "OffsetY")?))
}

/// A pre-2018 spot.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LegacySpot {
    pub centre: (f64, f64),
    /// Fraction of the frame width (assumed, as for the current format).
    pub radius: f64,
    pub source: Option<(f64, f64)>,
    pub kind: SpotKind,
}

/// Parse one `RetouchInfo` string.
pub fn parse_legacy(s: &str) -> Option<LegacySpot> {
    let mut fields: BTreeMap<&str, &str> = BTreeMap::new();
    for part in s.split(',') {
        let (k, v) = part.split_once('=')?;
        fields.insert(k.trim(), v.trim());
    }
    let n =
        |k: &str| -> Option<f64> { fields.get(k)?.parse().ok().filter(|v: &f64| v.is_finite()) };
    let kind = match fields.get("spotType").copied().unwrap_or("heal") {
        "clone" => SpotKind::Clone,
        _ => SpotKind::Heal,
    };
    let source = match (n("sourceX"), n("sourceY")) {
        (Some(x), Some(y)) => Some((x, y)),
        _ => None,
    };
    Some(LegacySpot {
        centre: (n("centerX")?, n("centerY")?),
        radius: n("radius")?.abs(),
        source,
        kind,
    })
}

/// A new round area in Lightroom's layout. `radius` is a fraction of the
/// frame width; `feather` and `opacity` are 0..1.
pub fn new_area(
    kind: SpotKind,
    centre: (f64, f64),
    radius: f64,
    source: Option<(f64, f64)>,
    source_auto: bool,
    feather: f64,
    opacity: f64,
) -> BTreeMap<String, Value> {
    let mut a: BTreeMap<String, Value> = BTreeMap::new();
    let spot_type = match kind {
        SpotKind::Clone => "clone",
        SpotKind::Heal => "heal",
        SpotKind::ContentAware | SpotKind::Generative => "heal_patchmatch",
    };
    a.insert("SpotType".into(), Value::Text(spot_type.into()));
    let state = if source.is_none() || source_auto {
        "sourceAutoComputed"
    } else {
        "sourceSetExplicitly"
    };
    a.insert("SourceState".into(), Value::Text(state.into()));
    a.insert("Method".into(), Value::Text("gaussian".into()));
    if let Some((x, y)) = source {
        a.insert("SourceX".into(), number(x));
        a.insert("OffsetY".into(), number(y));
    }
    a.insert("Opacity".into(), number(opacity));
    a.insert("Feather".into(), number(feather));
    let mut e: BTreeMap<String, Value> = BTreeMap::new();
    e.insert("What".into(), Value::Text("Mask/Ellipse".into()));
    e.insert("MaskActive".into(), boolean(true));
    e.insert("MaskBlendMode".into(), Value::Text("0".into()));
    e.insert("MaskInverted".into(), boolean(false));
    e.insert("MaskSyncID".into(), Value::Text(new_sync_id()));
    e.insert("MaskValue".into(), number(1.0));
    e.insert("X".into(), number(centre.0));
    e.insert("Y".into(), number(centre.1));
    e.insert("SizeX".into(), number(radius));
    e.insert("SizeY".into(), number(radius));
    a.insert("Masks".into(), Value::Seq(vec![Value::Struct(e)]));
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_strings() {
        let s = parse_legacy(
            "centerX = 0.512, centerY = 0.4, radius = 0.0123, sourceState = sourceSetExplicitly, sourceX = 0.6, sourceY = 0.45, spotType = clone",
        )
        .unwrap();
        assert_eq!(s.centre, (0.512, 0.4));
        assert_eq!(s.radius, 0.0123);
        assert_eq!(s.source, Some((0.6, 0.45)));
        assert_eq!(s.kind, SpotKind::Clone);
        assert!(parse_legacy("nonsense").is_none());
    }

    #[test]
    fn new_areas_read_back() {
        let a = new_area(
            SpotKind::Heal,
            (0.3, 0.4),
            0.02,
            Some((0.5, 0.4)),
            false,
            0.5,
            1.0,
        );
        assert_eq!(SpotKind::of(&a), Some(SpotKind::Heal));
        assert_eq!(ellipse(&a), Some(((0.3, 0.4), (0.02, 0.02))));
        assert_eq!(source(&a), Some((0.5, 0.4)));
        assert_eq!(
            text(&a, "SourceState").as_deref(),
            Some("sourceSetExplicitly")
        );
        let mut g = a.clone();
        g.insert("SpotType".into(), Value::Text("heal_patchmatch".into()));
        g.insert("fill_method".into(), Value::Text("firefly".into()));
        assert_eq!(SpotKind::of(&g), Some(SpotKind::Generative));
    }
}
