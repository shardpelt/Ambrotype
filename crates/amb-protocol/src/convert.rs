//! Conversions between JSON and [`Value`], validated against the registry.

use std::collections::BTreeMap;

use amb_presets::Value;
use amb_presets::registry::TagInfo;
use amb_presets::xmp_writer::format_number;
use serde::Serialize;

use crate::Error;

/// White balance presets Camera Raw writes (Adobe help: As Shot, Auto, the
/// raw-only light sources, and Custom).
pub const WHITE_BALANCE_PRESETS: &[&str] = &[
    "As Shot",
    "Auto",
    "Daylight",
    "Cloudy",
    "Shade",
    "Tungsten",
    "Fluorescent",
    "Flash",
    "Custom",
];

/// A numeric value that was pulled back into the registry range.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Clamp {
    pub name: String,
    pub requested: f64,
    pub applied: f64,
    pub min: f64,
    pub max: f64,
}

impl std::fmt::Display for Clamp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} = {} is outside [{}, {}]; clamped to {}",
            self.name,
            format_number(self.requested, false),
            format_number(self.min, false),
            format_number(self.max, false),
            format_number(self.applied, false)
        )
    }
}

fn json_kind(json: &serde_json::Value) -> &'static str {
    match json {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "a boolean",
        serde_json::Value::Number(_) => "a number",
        serde_json::Value::String(_) => "a string",
        serde_json::Value::Array(_) => "an array",
        serde_json::Value::Object(_) => "an object",
    }
}

fn wrong_kind(info: &TagInfo, expected: &str, got: &serde_json::Value) -> Error {
    Error::WrongKind {
        name: info.name.clone(),
        expected: expected.to_string(),
        got: json_kind(got).to_string(),
    }
}

/// A finite number from a JSON number or a numeric string such as `"+0.35"`.
pub fn json_number(json: &serde_json::Value) -> Option<f64> {
    let n = match json {
        serde_json::Value::Number(n) => n.as_f64(),
        serde_json::Value::String(s) => s.trim().trim_start_matches('+').parse().ok(),
        _ => None,
    }?;
    n.is_finite().then_some(n)
}

fn clamp(name: &str, v: f64, info: &TagInfo, clamps: &mut Vec<Clamp>) -> f64 {
    debug_assert!(
        v.is_finite(),
        "{name}: callers must reject non-finite values"
    );
    let lo = info.min.unwrap_or(f64::NEG_INFINITY);
    let hi = info.max.unwrap_or(f64::INFINITY);
    let applied = v.clamp(lo, hi);
    if applied != v {
        clamps.push(Clamp {
            name: name.to_string(),
            requested: v,
            applied,
            min: lo,
            max: hi,
        });
    }
    applied
}

/// Clamp a number to the tag's range, recording what happened.
pub fn clamp_number(info: &TagInfo, v: f64, clamps: &mut Vec<Clamp>) -> f64 {
    clamp(&info.name, v, info, clamps)
}

/// Convert a command value for `info`'s tag, checking its kind and clamping
/// numbers to the registry range.
pub fn value_from_command(
    info: &TagInfo,
    json: &serde_json::Value,
    clamps: &mut Vec<Clamp>,
) -> Result<Value, Error> {
    match info.kind.as_str() {
        "number" => {
            let n = json_number(json).ok_or_else(|| wrong_kind(info, "a number", json))?;
            Ok(Value::Number(clamp_number(info, n, clamps)))
        }
        "bool" => match json {
            serde_json::Value::Bool(b) => Ok(Value::Bool(*b)),
            serde_json::Value::String(s) => Value::Text(s.clone())
                .as_bool()
                .map(Value::Bool)
                .ok_or_else(|| wrong_kind(info, "a boolean (true/false)", json)),
            _ => Err(wrong_kind(info, "a boolean (true/false)", json)),
        },
        "enum" if info.min.is_some() || info.max.is_some() => {
            let n = json_number(json).ok_or_else(|| wrong_kind(info, "an integer", json))?;
            let (lo, hi) = (
                info.min.unwrap_or(f64::NEG_INFINITY),
                info.max.unwrap_or(f64::INFINITY),
            );
            if n.fract() != 0.0 || n < lo || n > hi {
                return Err(Error::InvalidValue {
                    name: info.name.clone(),
                    reason: format!(
                        "must be an integer from {} to {}",
                        format_number(lo, false),
                        format_number(hi, false)
                    ),
                });
            }
            Ok(Value::Number(n))
        }
        "enum" if info.name == "WhiteBalance" => {
            let s = json
                .as_str()
                .ok_or_else(|| wrong_kind(info, "a string", json))?;
            WHITE_BALANCE_PRESETS
                .iter()
                .find(|p| p.eq_ignore_ascii_case(s.trim()))
                .map(|p| Value::Text(p.to_string()))
                .ok_or_else(|| Error::InvalidValue {
                    name: info.name.clone(),
                    reason: format!(
                        "unknown white balance {s:?}; use one of: {}",
                        WHITE_BALANCE_PRESETS.join(", ")
                    ),
                })
        }
        "text" | "enum" => match json {
            serde_json::Value::String(s) => Ok(Value::Text(s.clone())),
            serde_json::Value::Number(n) => Ok(Value::Text(n.to_string())),
            serde_json::Value::Array(_) => {
                lang_alt(json).ok_or_else(|| wrong_kind(info, "a string", json))
            }
            _ => Err(wrong_kind(info, "a string", json)),
        },
        "curve" => curve_from_json(info, json, clamps),
        "seq" => match json {
            serde_json::Value::Array(_) => generic(json),
            _ => Err(wrong_kind(info, "an array", json)),
        },
        "struct" => match json {
            serde_json::Value::Object(_) => generic(json),
            _ => Err(wrong_kind(info, "an object", json)),
        },
        _ => generic(json),
    }
}

/// `[["x-default", "Text"], ...]` → [`Value::Alt`].
fn lang_alt(json: &serde_json::Value) -> Option<Value> {
    let items = json.as_array()?;
    let pairs: Option<Vec<(String, String)>> = items
        .iter()
        .map(|item| {
            let pair = item.as_array()?;
            match pair.as_slice() {
                [lang, text] => Some((lang.as_str()?.to_string(), text.as_str()?.to_string())),
                _ => None,
            }
        })
        .collect();
    pairs.filter(|p| !p.is_empty()).map(Value::Alt)
}

fn curve_from_json(
    info: &TagInfo,
    json: &serde_json::Value,
    clamps: &mut Vec<Clamp>,
) -> Result<Value, Error> {
    let expected = "an array of [input, output] points on a 0-255 scale";
    let items = json
        .as_array()
        .ok_or_else(|| wrong_kind(info, expected, json))?;
    let range = TagInfo {
        min: Some(0.0),
        max: Some(255.0),
        ..info.clone()
    };
    let mut points = Vec::with_capacity(items.len());
    for (i, item) in items.iter().enumerate() {
        let (x, y) = match item {
            serde_json::Value::Array(pair) if pair.len() == 2 => {
                (json_number(&pair[0]), json_number(&pair[1]))
            }
            serde_json::Value::Object(map) => (
                map.get("x").and_then(json_number),
                map.get("y").and_then(json_number),
            ),
            serde_json::Value::String(s) => match s.split_once(',') {
                Some((x, y)) => (
                    json_number(&serde_json::Value::String(x.into())),
                    json_number(&serde_json::Value::String(y.into())),
                ),
                None => (None, None),
            },
            _ => (None, None),
        };
        let (Some(x), Some(y)) = (x, y) else {
            return Err(Error::InvalidValue {
                name: info.name.clone(),
                reason: format!("point {i} is not [input, output]; expected {expected}"),
            });
        };
        let x = clamp(&format!("{}[{i}].input", info.name), x, &range, clamps);
        let y = clamp(&format!("{}[{i}].output", info.name), y, &range, clamps);
        points.push((x, y));
    }
    if points.len() < 2 {
        return Err(Error::InvalidValue {
            name: info.name.clone(),
            reason: "a curve needs at least 2 points, e.g. [[0,0],[255,255]]".into(),
        });
    }
    if points.windows(2).any(|w| w[1].0 <= w[0].0) {
        return Err(Error::InvalidValue {
            name: info.name.clone(),
            reason: "point inputs must strictly increase from left to right".into(),
        });
    }
    Ok(Value::Seq(
        points
            .into_iter()
            .map(|(x, y)| {
                Value::Text(format!(
                    "{}, {}",
                    format_number(x, false),
                    format_number(y, false)
                ))
            })
            .collect(),
    ))
}

/// Structural conversion without validation.
pub fn generic(json: &serde_json::Value) -> Result<Value, Error> {
    Ok(match json {
        serde_json::Value::Null => {
            return Err(Error::Json(
                "null is not a value; use reset_param to clear a setting".into(),
            ));
        }
        serde_json::Value::Bool(b) => Value::Bool(*b),
        serde_json::Value::Number(n) => Value::Number(n.as_f64().unwrap_or(0.0)),
        serde_json::Value::String(s) => Value::Text(s.clone()),
        serde_json::Value::Array(items) => {
            Value::Seq(items.iter().map(generic).collect::<Result<_, _>>()?)
        }
        serde_json::Value::Object(map) => Value::Struct(
            map.iter()
                .map(|(k, v)| Ok((k.clone(), generic(v)?)))
                .collect::<Result<BTreeMap<_, _>, Error>>()?,
        ),
    })
}

/// Lossless conversion used when reading saved state: no clamping, but
/// language alternatives and registry kinds are restored.
pub fn value_from_saved(name: &str, json: &serde_json::Value) -> Result<Value, Error> {
    let info = amb_presets::registry::Registry::builtin().lookup(name);
    let is_text = info.as_ref().is_some_and(|i| i.kind == "text");
    if is_text {
        if let Some(alt) = lang_alt(json) {
            return Ok(alt);
        }
    }
    Ok(normalise(name, generic(json)?))
}

/// Store registry-typed scalars in typed form: numeric text becomes
/// [`Value::Number`], `"True"`/`"False"` become [`Value::Bool`]. The XMP writer
/// formats them back the way Camera Raw does. Text that parses only to a
/// non-finite number (`"NaN"`, `"inf"`) stays text, so it can never poison
/// diffs or the JSON form (which has no NaN).
pub fn normalise(name: &str, value: Value) -> Value {
    let Some(info) = amb_presets::registry::Registry::builtin().lookup(name) else {
        return value;
    };
    match (&value, info.kind.as_str()) {
        (Value::Text(_), "number") => finite(&value).map_or(value, Value::Number),
        (Value::Text(_), "enum") if info.min.is_some() => {
            finite(&value).map_or(value, Value::Number)
        }
        (Value::Text(_) | Value::Number(_), "bool") => value.as_bool().map_or(value, Value::Bool),
        _ => value,
    }
}

/// The value as a finite number, if it is one (`"NaN"` and `"inf"` are not).
pub fn finite(value: &Value) -> Option<f64> {
    value.as_f64().filter(|n| n.is_finite())
}

/// Equality that ignores float noise.
pub fn same(a: Option<&Value>, b: Option<&Value>) -> bool {
    match (a, b) {
        (Some(Value::Number(x)), Some(Value::Number(y))) => (x - y).abs() < 1e-9,
        (a, b) => a == b,
    }
}

/// Short human-readable rendering of a value for diffs.
pub fn display(name: &str, value: Option<&Value>) -> String {
    match value {
        None => "(unset)".into(),
        Some(Value::Number(n)) => format_number(*n, amb_presets::is_signed_tag(name)),
        Some(Value::Bool(b)) => b.to_string(),
        Some(Value::Text(s)) => format!("{s:?}"),
        Some(Value::Seq(items)) if items.iter().all(|i| matches!(i, Value::Text(_))) => {
            let parts: Vec<String> = items
                .iter()
                .filter_map(|i| i.as_str().map(|s| format!("({s})")))
                .collect();
            format!("[{}]", parts.join(" "))
        }
        Some(v @ Value::Alt(_)) => format!("{:?}", v.display_text().unwrap_or("")),
        Some(Value::Seq(items)) => format!("[{} items]", items.len()),
        Some(Value::Struct(map)) => format!("{{{} fields}}", map.len()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use amb_presets::registry::Registry;
    use serde_json::json;

    fn info(name: &str) -> TagInfo {
        Registry::builtin().lookup(name).unwrap()
    }

    #[test]
    fn numbers_clamp_and_accept_strings() {
        let mut clamps = Vec::new();
        let v = value_from_command(&info("Exposure2012"), &json!("+0.5"), &mut clamps).unwrap();
        assert_eq!(v, Value::Number(0.5));
        let v = value_from_command(&info("Exposure2012"), &json!(9), &mut clamps).unwrap();
        assert_eq!(v, Value::Number(5.0));
        assert_eq!(clamps.len(), 1);
        assert_eq!((clamps[0].requested, clamps[0].applied), (9.0, 5.0));
        let err = value_from_command(&info("Exposure2012"), &json!(true), &mut clamps);
        assert!(matches!(err, Err(Error::WrongKind { .. })));
    }

    #[test]
    fn enums_bools_and_white_balance() {
        let mut c = Vec::new();
        assert!(value_from_command(&info("PostCropVignetteStyle"), &json!(4), &mut c).is_err());
        assert!(value_from_command(&info("PostCropVignetteStyle"), &json!(1.5), &mut c).is_err());
        assert_eq!(
            value_from_command(&info("PostCropVignetteStyle"), &json!(2), &mut c).unwrap(),
            Value::Number(2.0)
        );
        assert_eq!(
            value_from_command(&info("WhiteBalance"), &json!("custom"), &mut c).unwrap(),
            Value::Text("Custom".into())
        );
        assert!(value_from_command(&info("WhiteBalance"), &json!("Candle"), &mut c).is_err());
        assert_eq!(
            value_from_command(&info("ConvertToGrayscale"), &json!("True"), &mut c).unwrap(),
            Value::Bool(true)
        );
    }

    #[test]
    fn curves() {
        let mut c = Vec::new();
        let tag = info("ToneCurvePV2012");
        let v = value_from_command(
            &tag,
            &json!([[0, 20], "128, 128", {"x": 255, "y": 300}]),
            &mut c,
        )
        .unwrap();
        assert_eq!(
            amb_presets::curve_points(&v).unwrap(),
            vec![(0.0, 20.0), (128.0, 128.0), (255.0, 255.0)]
        );
        assert_eq!(c[0].name, "ToneCurvePV2012[2].output");
        assert!(value_from_command(&tag, &json!([[0, 0]]), &mut c).is_err());
        assert!(value_from_command(&tag, &json!([[10, 0], [5, 9]]), &mut c).is_err());
    }

    #[test]
    fn normalises_xmp_text() {
        assert_eq!(
            normalise("Exposure2012", Value::Text("+0.35".into())),
            Value::Number(0.35)
        );
        assert_eq!(
            normalise("HasCrop", Value::Text("True".into())),
            Value::Bool(true)
        );
        assert_eq!(
            normalise("ProcessVersion", Value::Text("15.4".into())),
            Value::Text("15.4".into())
        );
    }
}
