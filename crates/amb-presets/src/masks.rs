// SPDX-License-Identifier: GPL-3.0-or-later
//! Masks and local adjustments: a typed view of Camera Raw's correction tags.
//!
//! Lightroom keeps masks as plain settings, so they stay [`Value`]s in the
//! settings map and round-trip untouched. This module only reads and builds
//! them. Since Lightroom Classic 11 they are in `MaskGroupBasedCorrections`:
//! one *correction* per mask (its local sliders and `CorrectionMasks`), each
//! mask made of *components* combined in order. Older files use one tag per
//! tool (see [`LEGACY_TAGS`]) with the same correction layout.
//!
//! Facts below come from Lightroom-written files (see docs/design/masks.md):
//! - coordinates are fractions of the uncropped image in stored (sensor)
//!   orientation, like the crop corners;
//! - local sliders are stored as UI value / 100, Exposure as EV / 4;
//! - add is `MaskBlendMode` 0 with `MaskValue` 1; subtract is mode 1 with
//!   `MaskValue` 0; intersect is subtract with `MaskInverted` (A ∩ B = A − ¬B);
//! - a radial gradient's `Flipped` is always `!MaskInverted`, and a range
//!   mask's `Invert` always equals `MaskInverted`;
//! - a brush is one `Mask/Aggregate` component (it carries the combination)
//!   whose `Masks` list holds the `Mask/Paint` strokes; older files put the
//!   strokes straight into `CorrectionMasks`. Newer Lightroom versions may
//!   keep the strokes in the photo's `.acr` file instead (`MaskBrushTable`);
//! - a stroke's `Dabs` are tokens `r <radius>`, `f <flow>`, `h <hardness>`
//!   and `d <x> <y>` (a dab with the current state); the radius is a fraction
//!   of the frame's width, so a dab is a circle in pixels.

use std::collections::BTreeMap;
use std::ops::Range;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::value::Value;

/// The current mask tag.
pub const MASK_TAG: &str = "MaskGroupBasedCorrections";

/// Pre-2021 tags, one per tool, each with its panel switch.
pub const LEGACY_TAGS: [(&str, &str); 3] = [
    ("GradientBasedCorrections", "EnableGradientBasedCorrections"),
    (
        "CircularGradientBasedCorrections",
        "EnableCircularGradientBasedCorrections",
    ),
    ("PaintBasedCorrections", "EnablePaintBasedCorrections"),
];

/// Panel switch for [`MASK_TAG`].
pub const MASK_ENABLE_TAG: &str = "EnableMaskGroupBasedCorrections";

/// Our colour-range samples, kept inside `CorrectionRangeMask` (Lightroom's
/// own sample format is not known): a list of `"x y"` image positions.
pub const COLOR_SAMPLES: &str = "amb:ColorSamples";

/// One local slider of a mask.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LocalSlider {
    /// Field name in the correction, e.g. `LocalExposure2012`.
    pub tag: &'static str,
    pub label: &'static str,
    /// UI range and step.
    pub min: f64,
    pub max: f64,
    pub step: f64,
    /// Stored value = UI value / `scale`.
    pub scale: f64,
    /// Panel section in the editor.
    pub section: &'static str,
    /// Whether the engine renders it yet.
    pub rendered: bool,
}

const fn slider(
    tag: &'static str,
    label: &'static str,
    section: &'static str,
    rendered: bool,
) -> LocalSlider {
    LocalSlider {
        tag,
        label,
        min: -100.0,
        max: 100.0,
        step: 1.0,
        scale: 100.0,
        section,
        rendered,
    }
}

/// Lightroom's local sliders, in panel order.
pub const LOCAL_SLIDERS: &[LocalSlider] = &[
    slider("LocalTemperature", "Temp", "Color", true),
    slider("LocalTint", "Tint", "Color", true),
    LocalSlider {
        tag: "LocalExposure2012",
        label: "Exposure",
        min: -4.0,
        max: 4.0,
        step: 0.01,
        scale: 4.0,
        section: "Light",
        rendered: true,
    },
    slider("LocalContrast2012", "Contrast", "Light", true),
    slider("LocalHighlights2012", "Highlights", "Light", true),
    slider("LocalShadows2012", "Shadows", "Light", true),
    slider("LocalWhites2012", "Whites", "Light", true),
    slider("LocalBlacks2012", "Blacks", "Light", true),
    slider("LocalTexture", "Texture", "Effects", true),
    slider("LocalClarity2012", "Clarity", "Effects", true),
    slider("LocalDehaze", "Dehaze", "Effects", true),
    slider("LocalSaturation", "Saturation", "Color", true),
    LocalSlider {
        tag: "LocalToningHue",
        label: "Color hue",
        min: 0.0,
        max: 360.0,
        step: 1.0,
        scale: 1.0,
        section: "Color",
        rendered: true,
    },
    LocalSlider {
        tag: "LocalToningSaturation",
        label: "Color amount",
        min: 0.0,
        max: 100.0,
        step: 1.0,
        scale: 100.0,
        section: "Color",
        rendered: true,
    },
    slider("LocalSharpness", "Sharpness", "Detail", true),
    slider("LocalLuminanceNoise", "Noise", "Detail", true),
    slider("LocalMoire", "Moire", "Detail", false),
    slider("LocalDefringe", "Defringe", "Detail", true),
];

/// Correction fields that change the image but are not rendered yet (besides
/// the sliders marked so in [`LOCAL_SLIDERS`]).
pub const UNRENDERED_FIELDS: &[&str] = &["LocalHue", "LocalGrain", "LocalPointColors"];

pub fn local_slider(tag: &str) -> Option<&'static LocalSlider> {
    LOCAL_SLIDERS.iter().find(|s| s.tag == tag)
}

/// What a mask component is.
#[derive(Debug, Clone, PartialEq)]
pub enum MaskKind {
    Linear,
    Radial,
    Brush,
    Luminance,
    Color,
    Depth,
    /// An AI mask (`Mask/Image`) with its `MaskSubType`.
    Ai(String),
    Other(String),
}

impl MaskKind {
    pub fn of(entry: &BTreeMap<String, Value>) -> MaskKind {
        let what = text(entry, "What").unwrap_or_default();
        match what.as_str() {
            "Mask/Gradient" => MaskKind::Linear,
            "Mask/CircularGradient" => MaskKind::Radial,
            "Mask/Paint" | "Mask/Aggregate" => MaskKind::Brush,
            "Mask/RangeMask" => match range_mask(entry).and_then(|r| text(r, "Type")).as_deref() {
                Some("1") => MaskKind::Color,
                Some("2") => MaskKind::Luminance,
                Some("3") => MaskKind::Depth,
                _ => MaskKind::Other(what),
            },
            "Mask/Image" => MaskKind::Ai(text(entry, "MaskSubType").unwrap_or_default()),
            _ => MaskKind::Other(what),
        }
    }

    /// Short name used by commands and the editor.
    pub fn name(&self) -> &str {
        match self {
            MaskKind::Linear => "linear",
            MaskKind::Radial => "radial",
            MaskKind::Brush => "brush",
            MaskKind::Luminance => "luminance",
            MaskKind::Color => "color",
            MaskKind::Depth => "depth",
            MaskKind::Ai(_) => "ai",
            MaskKind::Other(_) => "other",
        }
    }

    /// A readable label, e.g. "Linear Gradient" or "Select Sky".
    pub fn label(&self) -> String {
        match self {
            MaskKind::Linear => "Linear Gradient".into(),
            MaskKind::Radial => "Radial Gradient".into(),
            MaskKind::Brush => "Brush".into(),
            MaskKind::Luminance => "Luminance Range".into(),
            MaskKind::Color => "Color Range".into(),
            MaskKind::Depth => "Depth Range".into(),
            MaskKind::Ai(sub) => match sub.as_str() {
                "1" => "Select Subject".into(),
                "2" => "Select Sky".into(),
                "3" => "People".into(),
                _ => "AI mask".into(),
            },
            MaskKind::Other(what) => what.clone(),
        }
    }

    /// Whether the engine can draw it.
    pub fn rendered(&self) -> bool {
        matches!(
            self,
            MaskKind::Linear
                | MaskKind::Radial
                | MaskKind::Brush
                | MaskKind::Luminance
                | MaskKind::Color
        ) || matches!(self, MaskKind::Ai(sub) if sub == "2")
    }
}

/// How a component joins the mask built so far.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Add,
    Subtract,
    Intersect,
}

impl Mode {
    pub fn name(self) -> &'static str {
        match self {
            Mode::Add => "add",
            Mode::Subtract => "subtract",
            Mode::Intersect => "intersect",
        }
    }

    pub fn parse(s: &str) -> Option<Mode> {
        match s.trim().to_ascii_lowercase().as_str() {
            "add" => Some(Mode::Add),
            "subtract" => Some(Mode::Subtract),
            "intersect" => Some(Mode::Intersect),
            _ => None,
        }
    }
}

/// A component's combination as the user sees it: a mode and whether its
/// shape is inverted. Subtracting an inverted shape is intersecting, so the
/// stored form has only three cases per inversion (see the module docs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Combination {
    pub mode: Mode,
    pub inverted: bool,
}

impl Combination {
    /// Read from a component, honouring the legacy mirrors (`Flipped`,
    /// `Invert`) when `MaskInverted` is absent.
    pub fn of(entry: &BTreeMap<String, Value>) -> Combination {
        let stored_inverted = shape_inverted(entry);
        let subtracting = num(entry, "MaskBlendMode").unwrap_or(0.0) == 1.0;
        match (subtracting, stored_inverted) {
            (false, inv) => Combination {
                mode: Mode::Add,
                inverted: inv,
            },
            (true, false) => Combination {
                mode: Mode::Subtract,
                inverted: false,
            },
            (true, true) => Combination {
                mode: Mode::Intersect,
                inverted: false,
            },
        }
    }

    /// `(MaskBlendMode, MaskValue, MaskInverted)` as Lightroom writes them.
    pub fn stored(self) -> (u8, f64, bool) {
        match (self.mode, self.inverted) {
            (Mode::Add, inv) => (0, 1.0, inv),
            (Mode::Subtract, false) | (Mode::Intersect, true) => (1, 0.0, false),
            (Mode::Intersect, false) | (Mode::Subtract, true) => (1, 0.0, true),
        }
    }
}

/// Whether the component's stored shape is inverted: `MaskInverted`, or for
/// files without it, a radial gradient with `Flipped` false (effect outside)
/// or a range mask with `Invert`.
pub fn shape_inverted(entry: &BTreeMap<String, Value>) -> bool {
    if let Some(b) = flag(entry, "MaskInverted") {
        return b;
    }
    match text(entry, "What").as_deref() {
        Some("Mask/CircularGradient") => !flag(entry, "Flipped").unwrap_or(true),
        Some("Mask/RangeMask") => range_mask(entry)
            .and_then(|r| flag(r, "Invert"))
            .unwrap_or(false),
        _ => false,
    }
}

/// The `CorrectionRangeMask` struct of a component or correction.
pub fn range_mask(entry: &BTreeMap<String, Value>) -> Option<&BTreeMap<String, Value>> {
    match entry.get("CorrectionRangeMask") {
        Some(Value::Struct(m)) => Some(m),
        _ => None,
    }
}

/// A correction's components (`CorrectionMasks`).
pub fn components(correction: &BTreeMap<String, Value>) -> &[Value] {
    match correction.get("CorrectionMasks") {
        Some(Value::Seq(items)) => items,
        _ => &[],
    }
}

/// Group components into *parts*, the editor's mask components. A
/// `Mask/Aggregate` brush is one part. In older files each brush stroke is its
/// own `Mask/Paint` entry, so consecutive strokes with the same `MaskSyncID`
/// (or none) form one part. Every other entry is a part on its own.
pub fn parts(components: &[Value]) -> Vec<Range<usize>> {
    let mut out: Vec<Range<usize>> = Vec::new();
    let mut last_paint_id: Option<String> = None;
    for (i, c) in components.iter().enumerate() {
        let Value::Struct(m) = c else {
            out.push(i..i + 1);
            last_paint_id = None;
            continue;
        };
        if text(m, "What").as_deref() == Some("Mask/Paint") {
            let id = text(m, "MaskSyncID").unwrap_or_default();
            if let (Some(prev), Some(r)) = (&last_paint_id, out.last_mut()) {
                if *prev == id && r.end == i {
                    r.end = i + 1;
                    continue;
                }
            }
            last_paint_id = Some(id);
        } else {
            last_paint_id = None;
        }
        out.push(i..i + 1);
    }
    out
}

/// The strokes of a brush part: an aggregate's `Masks`, or (old files) the
/// part's own `Mask/Paint` entries.
pub fn brush_strokes<'a>(
    entries: &[&'a BTreeMap<String, Value>],
) -> Vec<&'a BTreeMap<String, Value>> {
    match entries.first() {
        Some(first) if text(first, "What").as_deref() == Some("Mask/Aggregate") => {
            match first.get("Masks") {
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
        _ => entries.to_vec(),
    }
}

/// Whether a brush keeps its strokes in Lightroom's `.acr` file
/// (`MaskBrushTable`), which we don't read yet.
pub fn uses_brush_table(entry: &BTreeMap<String, Value>) -> bool {
    entry.keys().any(|k| k.starts_with("MaskBrushTable"))
}

/// One token of a stroke's `Dabs`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DabToken {
    /// `r`: the radius from here on (fraction of the frame width).
    Radius(f64),
    /// `f`: the flow from here on (0..1).
    Flow(f64),
    /// `h`: the hardness from here on (0..1, as `CenterWeight`).
    Hardness(f64),
    /// `d`: a dab at (x, y), image fractions.
    Dab(f64, f64),
}

pub fn parse_dab_token(s: &str) -> Option<DabToken> {
    let mut it = s.split_whitespace();
    let kind = it.next()?;
    let mut num = || -> Option<f64> { it.next()?.parse().ok().filter(|v: &f64| v.is_finite()) };
    let t = match kind {
        "r" => DabToken::Radius(num()?),
        "f" => DabToken::Flow(num()?),
        "h" => DabToken::Hardness(num()?),
        "d" => DabToken::Dab(num()?, num()?),
        _ => return None,
    };
    Some(t)
}

/// One brush dab, `"d x y"`, in image fractions.
pub fn parse_dab(s: &str) -> Option<(f64, f64)> {
    match parse_dab_token(s)? {
        DabToken::Dab(x, y) => Some((x, y)),
        _ => None,
    }
}

pub fn format_dab(x: f64, y: f64) -> String {
    format!("d {x:.6} {y:.6}")
}

/// A number as Lightroom writes mask fields: up to 6 decimals, no `+`.
pub fn number(v: f64) -> Value {
    let mut s = format!("{v:.6}");
    while s.contains('.') && (s.ends_with('0') || s.ends_with('.')) {
        s.pop();
    }
    if s == "-0" {
        s = "0".into();
    }
    Value::Text(s)
}

/// A boolean as Lightroom writes mask fields (`true` / `false`).
pub fn boolean(b: bool) -> Value {
    Value::Text(if b { "true" } else { "false" }.into())
}

/// A new `CorrectionSyncID` / `MaskSyncID`: 32 upper-case hex digits.
pub fn new_sync_id() -> String {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as u64);
    let half = |salt: u64| {
        let mut h = RandomState::new().build_hasher();
        h.write_u64(nanos);
        h.write_u64(n);
        h.write_u64(salt);
        h.finish()
    };
    format!("{:016X}{:016X}", half(1), half(2))
}

pub fn text(m: &BTreeMap<String, Value>, key: &str) -> Option<String> {
    match m.get(key)? {
        Value::Text(s) => Some(s.trim().to_string()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

pub fn num(m: &BTreeMap<String, Value>, key: &str) -> Option<f64> {
    m.get(key).and_then(Value::as_f64).filter(|v| v.is_finite())
}

pub fn flag(m: &BTreeMap<String, Value>, key: &str) -> Option<bool> {
    m.get(key).and_then(Value::as_bool)
}

/// The four `LumRange` values: no effect below the first, full from the
/// second to the third, none above the fourth.
pub fn lum_range(range: &BTreeMap<String, Value>) -> Option<[f64; 4]> {
    let s = text(range, "LumRange")?;
    let v: Vec<f64> = s
        .split_whitespace()
        .filter_map(|t| t.parse().ok())
        .collect();
    (v.len() == 4 && v.iter().all(|x| x.is_finite())).then(|| [v[0], v[1], v[2], v[3]])
}

pub fn format_lum_range(r: [f64; 4]) -> String {
    r.map(|v| format!("{v:.6}")).join(" ")
}

/// Our colour-range samples, as image fractions.
pub fn color_samples(range: &BTreeMap<String, Value>) -> Vec<(f64, f64)> {
    match range.get(COLOR_SAMPLES) {
        Some(Value::Seq(items)) => items
            .iter()
            .filter_map(|v| match v {
                Value::Text(s) => {
                    let mut it = s.split_whitespace().filter_map(|t| t.parse::<f64>().ok());
                    Some((it.next()?, it.next()?))
                }
                _ => None,
            })
            .filter(|(x, y)| x.is_finite() && y.is_finite())
            .collect(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(pairs: &[(&str, &str)]) -> Value {
        Value::Struct(
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), Value::Text(v.to_string())))
                .collect(),
        )
    }

    #[test]
    fn combinations_round_trip_through_lightroom_fields() {
        for mode in [Mode::Add, Mode::Subtract, Mode::Intersect] {
            for inverted in [false, true] {
                let c = Combination { mode, inverted };
                let (blend, value, inv) = c.stored();
                let e = entry(&[
                    ("What", "Mask/Gradient"),
                    ("MaskBlendMode", &blend.to_string()),
                    ("MaskValue", &value.to_string()),
                    ("MaskInverted", if inv { "true" } else { "false" }),
                ]);
                let Value::Struct(m) = e else { unreachable!() };
                let back = Combination::of(&m);
                // Subtracting an inverted shape reads back as intersecting.
                let expect = match (mode, inverted) {
                    (Mode::Subtract, true) => Combination {
                        mode: Mode::Intersect,
                        inverted: false,
                    },
                    (Mode::Intersect, true) => Combination {
                        mode: Mode::Subtract,
                        inverted: false,
                    },
                    _ => c,
                };
                assert_eq!(back, expect, "{c:?}");
            }
        }
    }

    #[test]
    fn legacy_radial_effect_is_outside_unless_flipped() {
        let Value::Struct(m) = entry(&[("What", "Mask/CircularGradient"), ("Flipped", "false")])
        else {
            unreachable!()
        };
        assert!(shape_inverted(&m));
        let Value::Struct(m) = entry(&[("What", "Mask/CircularGradient"), ("Flipped", "true")])
        else {
            unreachable!()
        };
        assert!(!shape_inverted(&m));
    }

    #[test]
    fn brush_strokes_group_into_parts() {
        let items = vec![
            entry(&[("What", "Mask/Gradient")]),
            entry(&[("What", "Mask/Paint"), ("MaskSyncID", "A")]),
            entry(&[("What", "Mask/Paint"), ("MaskSyncID", "A")]),
            entry(&[("What", "Mask/Paint"), ("MaskSyncID", "B")]),
            entry(&[("What", "Mask/CircularGradient")]),
            entry(&[("What", "Mask/Paint")]),
            entry(&[("What", "Mask/Paint")]),
        ];
        assert_eq!(parts(&items), vec![0..1, 1..3, 3..4, 4..5, 5..7]);
        // An aggregate is one part with its strokes inside.
        let mut agg = std::collections::BTreeMap::new();
        agg.insert("What".to_string(), Value::Text("Mask/Aggregate".into()));
        agg.insert(
            "Masks".to_string(),
            Value::Seq(vec![
                entry(&[("What", "Mask/Paint")]),
                entry(&[("What", "Mask/Paint")]),
            ]),
        );
        let items = vec![
            Value::Struct(agg.clone()),
            entry(&[("What", "Mask/Gradient")]),
        ];
        assert_eq!(parts(&items), vec![0..1, 1..2]);
        assert_eq!(brush_strokes(&[&agg]).len(), 2);
        assert_eq!(MaskKind::of(&agg), MaskKind::Brush);
    }

    #[test]
    fn dabs_and_numbers() {
        assert_eq!(parse_dab("d 0.5 0.25"), Some((0.5, 0.25)));
        assert_eq!(
            parse_dab_token("r 0.003489"),
            Some(DabToken::Radius(0.003489))
        );
        assert_eq!(parse_dab_token("h 1"), Some(DabToken::Hardness(1.0)));
        assert_eq!(parse_dab_token("x 1"), None);
        assert_eq!(parse_dab("m 0.5 0.25"), None);
        assert_eq!(parse_dab(&format_dab(0.1, 0.2)), Some((0.1, 0.2)));
        assert_eq!(number(0.0625), Value::Text("0.0625".into()));
        assert_eq!(number(1.0), Value::Text("1".into()));
        assert_eq!(number(-0.0), Value::Text("0".into()));
        let id = new_sync_id();
        assert_eq!(id.len(), 32);
        assert!(
            id.chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_lowercase())
        );
        assert_ne!(id, new_sync_id());
    }
}
