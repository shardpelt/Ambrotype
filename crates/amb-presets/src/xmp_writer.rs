//! Serialise Camera Raw settings back to an XMP packet (sidecar or preset).
//!
//! Simple values are written as attributes on `rdf:Description`, as Lightroom
//! does; sequences, language alternatives and structs are written as elements.
//! Keys of the form `{uri}local` (non-Camera-Raw properties preserved by the
//! reader) get generated namespace prefixes.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::value::Value;
use crate::xmp::{AMB_NS, AMB_PREFIX, CRS_NS, RDF_NS};

fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c => out.push(c),
        }
    }
    out
}

/// Format a number the way Camera Raw writes it: no trailing zeros, and an
/// explicit `+` on positive values of signed settings.
pub fn format_number(n: f64, signed: bool) -> String {
    let mut s = format!("{n:.4}");
    while s.contains('.') && (s.ends_with('0') || s.ends_with('.')) {
        s.pop();
    }
    if s == "-0" {
        s = "0".into();
    }
    if signed && n > 0.0 {
        s.insert(0, '+');
    }
    s
}

struct Writer {
    prefixes: BTreeMap<String, String>,
    signed: fn(&str) -> bool,
}

impl Writer {
    fn qname(&self, key: &str) -> String {
        match key.strip_prefix('{').and_then(|rest| rest.split_once('}')) {
            Some((uri, local)) => format!("{}:{local}", self.prefixes[uri]),
            None if key.starts_with(AMB_PREFIX) => key.to_string(),
            None => format!("crs:{key}"),
        }
    }

    fn scalar(&self, key: &str, value: &Value) -> Option<String> {
        match value {
            Value::Text(s) => Some(s.clone()),
            Value::Number(n) => Some(format_number(*n, (self.signed)(key))),
            Value::Bool(b) => Some(if *b { "True" } else { "False" }.to_string()),
            _ => None,
        }
    }

    fn element(&self, out: &mut String, key: &str, value: &Value, indent: usize) {
        let pad = " ".repeat(indent);
        let name = self.qname(key);
        match value {
            Value::Seq(items) => {
                let _ = writeln!(out, "{pad}<{name}>\n{pad} <rdf:Seq>");
                for item in items {
                    match item {
                        Value::Struct(map) => {
                            let _ = writeln!(out, "{pad}  <rdf:li>");
                            self.description(out, map, indent + 3);
                            let _ = writeln!(out, "{pad}  </rdf:li>");
                        }
                        other => {
                            let text = self.scalar(key, other).unwrap_or_default();
                            let _ = writeln!(out, "{pad}  <rdf:li>{}</rdf:li>", escape(&text));
                        }
                    }
                }
                let _ = writeln!(out, "{pad} </rdf:Seq>\n{pad}</{name}>");
            }
            Value::Alt(items) => {
                let _ = writeln!(out, "{pad}<{name}>\n{pad} <rdf:Alt>");
                for (lang, text) in items {
                    let _ = writeln!(
                        out,
                        "{pad}  <rdf:li xml:lang=\"{}\">{}</rdf:li>",
                        escape(lang),
                        escape(text)
                    );
                }
                let _ = writeln!(out, "{pad} </rdf:Alt>\n{pad}</{name}>");
            }
            Value::Struct(map) => {
                let _ = writeln!(out, "{pad}<{name}>");
                self.description(out, map, indent + 1);
                let _ = writeln!(out, "{pad}</{name}>");
            }
            scalar => {
                let text = self.scalar(key, scalar).unwrap_or_default();
                let _ = writeln!(out, "{pad}<{name}>{}</{name}>", escape(&text));
            }
        }
    }

    fn description(&self, out: &mut String, map: &BTreeMap<String, Value>, indent: usize) {
        let pad = " ".repeat(indent);
        let (simple, complex): (Vec<_>, Vec<_>) =
            map.iter().partition(|(k, v)| self.scalar(k, v).is_some());
        let _ = write!(out, "{pad}<rdf:Description");
        for (k, v) in &simple {
            let text = self.scalar(k, v).unwrap_or_default();
            let _ = write!(out, "\n{pad}   {}=\"{}\"", self.qname(k), escape(&text));
        }
        if complex.is_empty() {
            let _ = writeln!(out, "/>");
            return;
        }
        let _ = writeln!(out, ">");
        for (k, v) in complex {
            self.element(out, k, v, indent + 1);
        }
        let _ = writeln!(out, "{pad}</rdf:Description>");
    }
}

fn collect_namespaces(map: &BTreeMap<String, Value>, out: &mut BTreeMap<String, String>) {
    for (key, value) in map {
        if let Some((uri, _)) = key.strip_prefix('{').and_then(|r| r.split_once('}')) {
            let next = format!("ns{}", out.len() + 1);
            out.entry(uri.to_string()).or_insert(next);
        }
        match value {
            Value::Struct(m) => collect_namespaces(m, out),
            Value::Seq(items) => {
                for item in items {
                    if let Value::Struct(m) = item {
                        collect_namespaces(m, out);
                    }
                }
            }
            _ => {}
        }
    }
}

/// Write `settings` as a complete XMP packet. `signed` reports whether a tag's
/// positive values should carry an explicit `+` (cosmetic; parsers accept both).
pub fn write_packet(settings: &BTreeMap<String, Value>, signed: fn(&str) -> bool) -> String {
    let mut prefixes = BTreeMap::new();
    collect_namespaces(settings, &mut prefixes);
    let writer = Writer { prefixes, signed };

    let mut body = String::new();
    writer.description(&mut body, settings, 2);
    // Put the namespace declarations on the top-level description.
    let mut decls = format!(" rdf:about=\"\"\n    xmlns:crs=\"{CRS_NS}\"");
    if settings.keys().any(|k| k.starts_with(AMB_PREFIX)) {
        let _ = write!(decls, "\n    xmlns:amb=\"{AMB_NS}\"");
    }
    for (uri, prefix) in &writer.prefixes {
        let _ = write!(decls, "\n    xmlns:{prefix}=\"{}\"", escape(uri));
    }
    let body = body.replacen("<rdf:Description", &format!("<rdf:Description{decls}"), 1);

    format!(
        "<?xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\n\
         <x:xmpmeta xmlns:x=\"adobe:ns:meta/\" x:xmptk=\"ambrotype\">\n \
         <rdf:RDF xmlns:rdf=\"{RDF_NS}\">\n{body} </rdf:RDF>\n</x:xmpmeta>\n\
         <?xpacket end=\"w\"?>\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xmp::parse_crs;

    fn never_signed(_: &str) -> bool {
        false
    }

    #[test]
    fn number_formatting() {
        assert_eq!(format_number(0.35, true), "+0.35");
        assert_eq!(format_number(-12.0, true), "-12");
        assert_eq!(format_number(40.0, false), "40");
        assert_eq!(format_number(-0.0, true), "0");
        assert_eq!(format_number(1.0 / 3.0, false), "0.3333");
    }

    #[test]
    fn round_trips_through_reader() {
        let mut look = BTreeMap::new();
        look.insert("Name".to_string(), Value::Text("Adobe Color".into()));
        look.insert(
            "{http://example.com/ns/}Extra".to_string(),
            Value::Text("x".into()),
        );
        let mut mask = BTreeMap::new();
        mask.insert("What".to_string(), Value::Text("Mask/Gradient".into()));
        mask.insert("ZeroX".to_string(), Value::Number(0.5));
        let mut settings = BTreeMap::new();
        settings.insert("Exposure2012".to_string(), Value::Number(0.35));
        settings.insert(
            "Name".to_string(),
            Value::Alt(vec![("x-default".into(), "A & B".into())]),
        );
        settings.insert(
            "ToneCurvePV2012".to_string(),
            Value::Seq(vec![
                Value::Text("0, 0".into()),
                Value::Text("255, 255".into()),
            ]),
        );
        settings.insert("Look".to_string(), Value::Struct(look));
        settings.insert(
            "GradientBasedCorrections".to_string(),
            Value::Seq(vec![Value::Struct(mask)]),
        );

        let packet = write_packet(&settings, never_signed);
        let back = parse_crs(&packet).unwrap();
        assert_eq!(back["Exposure2012"].as_f64(), Some(0.35));
        assert_eq!(back["Name"].display_text(), Some("A & B"));
        assert_eq!(back["ToneCurvePV2012"], settings["ToneCurvePV2012"]);
        let look = back["Look"].as_struct().unwrap();
        assert_eq!(look["Name"].as_str(), Some("Adobe Color"));
        assert_eq!(look["{http://example.com/ns/}Extra"].as_str(), Some("x"));
        let mask = back["GradientBasedCorrections"].as_seq().unwrap()[0]
            .as_struct()
            .unwrap();
        assert_eq!(mask["ZeroX"].as_f64(), Some(0.5));
    }

    #[test]
    fn ambrotype_properties_round_trip_in_their_own_namespace() {
        let mut settings = BTreeMap::new();
        settings.insert("Exposure2012".to_string(), Value::Number(0.5));
        settings.insert("amb:NegativeConvert".to_string(), Value::Bool(true));
        settings.insert(
            "amb:FilmStock".to_string(),
            Value::Text("warm-portrait-400".into()),
        );
        let packet = write_packet(&settings, never_signed);
        assert!(packet.contains("xmlns:amb=\"http://ns.ambrotype.org/develop/1.0/\""));
        assert!(packet.contains("amb:NegativeConvert=\"True\""));
        assert!(!packet.contains("crs:amb"));
        let back = parse_crs(&packet).unwrap();
        assert_eq!(back["amb:NegativeConvert"].as_bool(), Some(true));
        assert_eq!(back["amb:FilmStock"].as_str(), Some("warm-portrait-400"));
        assert_eq!(back["Exposure2012"].as_f64(), Some(0.5));
        // Without amb: settings, no amb namespace is declared.
        let plain = write_packet(
            &BTreeMap::from([("Exposure2012".to_string(), Value::Number(0.5))]),
            never_signed,
        );
        assert!(!plain.contains("xmlns:amb"));
    }
}
