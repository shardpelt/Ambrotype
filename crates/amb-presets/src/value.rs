// SPDX-License-Identifier: GPL-3.0-or-later
//! Format-neutral value tree shared by the XMP and `.lrtemplate` parsers.
//!
//! Values are kept close to their source representation so that nothing is lost:
//! XMP scalars stay as text (`"+0.35"`, `"True"`), `.lrtemplate` scalars keep their
//! Lua type. Typed accessors interpret either form.

use std::collections::BTreeMap;

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum Value {
    /// XMP simple value, or a Lua string.
    Text(String),
    /// Lua number.
    Number(f64),
    /// Lua boolean.
    Bool(bool),
    /// `rdf:Seq` / `rdf:Bag`, or a Lua array table.
    Seq(Vec<Value>),
    /// `rdf:Alt` with `xml:lang` qualifiers, as `(lang, text)` pairs in document order.
    Alt(Vec<(String, String)>),
    /// XMP struct (nested `rdf:Description` / `parseType="Resource"`), or a Lua keyed table.
    Struct(BTreeMap<String, Value>),
}

impl Value {
    /// Numeric interpretation. Accepts XMP text such as `"+0.35"`, `"-12"` or `"1"`.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Number(n) => Some(*n),
            Value::Text(s) => s.trim().trim_start_matches('+').parse().ok(),
            Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
            _ => None,
        }
    }

    /// Boolean interpretation. XMP uses `"True"`/`"False"`; some writers use `"1"`/`"0"`.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            Value::Number(n) => Some(*n != 0.0),
            Value::Text(s) => match s.trim().to_ascii_lowercase().as_str() {
                "true" | "1" => Some(true),
                "false" | "0" => Some(false),
                _ => None,
            },
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Text(s) => Some(s),
            _ => None,
        }
    }

    /// Text for display: the `x-default` (or first) entry of a language alternative,
    /// or a plain string.
    pub fn display_text(&self) -> Option<&str> {
        match self {
            Value::Text(s) => Some(s),
            Value::Alt(items) => items
                .iter()
                .find(|(lang, _)| lang == "x-default")
                .or_else(|| items.first())
                .map(|(_, text)| text.as_str()),
            _ => None,
        }
    }

    pub fn as_struct(&self) -> Option<&BTreeMap<String, Value>> {
        match self {
            Value::Struct(map) => Some(map),
            _ => None,
        }
    }

    pub fn as_seq(&self) -> Option<&[Value]> {
        match self {
            Value::Seq(items) => Some(items),
            _ => None,
        }
    }
}

/// Parse a tone curve from either representation:
/// XMP `rdf:Seq` of `"x, y"` strings, or a flat Lua array `{ x0, y0, x1, y1, ... }`.
pub fn curve_points(value: &Value) -> Option<Vec<(f64, f64)>> {
    let items = value.as_seq()?;
    if items.iter().all(|v| matches!(v, Value::Number(_))) {
        if items.len() % 2 != 0 {
            return None;
        }
        return items
            .chunks(2)
            .map(|pair| Some((pair[0].as_f64()?, pair[1].as_f64()?)))
            .collect();
    }
    items
        .iter()
        .map(|item| {
            let (x, y) = item.as_str()?.split_once(',')?;
            Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numeric_text_with_sign() {
        assert_eq!(Value::Text("+0.35".into()).as_f64(), Some(0.35));
        assert_eq!(Value::Text("-12".into()).as_f64(), Some(-12.0));
        assert_eq!(Value::Text("abc".into()).as_f64(), None);
    }

    #[test]
    fn booleans() {
        assert_eq!(Value::Text("True".into()).as_bool(), Some(true));
        assert_eq!(Value::Text("False".into()).as_bool(), Some(false));
        assert_eq!(Value::Bool(true).as_bool(), Some(true));
    }

    #[test]
    fn curves_both_forms() {
        let xmp = Value::Seq(vec![
            Value::Text("0, 0".into()),
            Value::Text("255, 250".into()),
        ]);
        assert_eq!(curve_points(&xmp), Some(vec![(0.0, 0.0), (255.0, 250.0)]));
        let lua = Value::Seq(
            vec![0.0, 18.0, 255.0, 245.0]
                .into_iter()
                .map(Value::Number)
                .collect(),
        );
        assert_eq!(curve_points(&lua), Some(vec![(0.0, 18.0), (255.0, 245.0)]));
        let odd = Value::Seq(vec![Value::Number(1.0)]);
        assert_eq!(curve_points(&odd), None);
    }
}
