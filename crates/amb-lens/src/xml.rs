// SPDX-License-Identifier: GPL-3.0-or-later
//! A minimal owned XML tree, enough for the lensfun database and LCP files.
//!
//! Namespace prefixes are stripped from element and attribute names (both
//! formats use unambiguous local names), `xmlns` declarations are dropped, and
//! comments, the doctype and processing instructions are skipped.

use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Element {
    /// Local name, without namespace prefix.
    pub name: String,
    /// Attributes by local name, in document order.
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Element>,
    /// Concatenated text content (not trimmed).
    pub text: String,
}

impl Element {
    /// Parse a document and return its root element.
    pub fn parse(input: &str) -> Result<Element, String> {
        let input = input.trim_start_matches('\u{feff}');
        let mut reader = Reader::from_str(input);
        // Keep whitespace inside text; callers trim.
        reader.config_mut().trim_text(false);
        let mut stack: Vec<Element> = vec![Element::default()];
        loop {
            let event = reader
                .read_event()
                .map_err(|e| format!("at byte {}: {e}", reader.buffer_position()))?;
            match event {
                Event::Start(e) => stack.push(start(&e)?),
                Event::Empty(e) => {
                    let el = start(&e)?;
                    top(&mut stack).children.push(el);
                }
                Event::End(_) => {
                    if stack.len() < 2 {
                        return Err("unbalanced end tag".into());
                    }
                    let el = stack.pop().unwrap_or_default();
                    top(&mut stack).children.push(el);
                }
                Event::Text(t) => {
                    // Unknown entities (e.g. HTML ones) are kept verbatim rather than failing.
                    let text = match t.unescape() {
                        Ok(s) => s.into_owned(),
                        Err(_) => String::from_utf8_lossy(&t).into_owned(),
                    };
                    top(&mut stack).text.push_str(&text);
                }
                Event::CData(t) => top(&mut stack).text.push_str(&String::from_utf8_lossy(&t)),
                Event::Eof => break,
                _ => {}
            }
        }
        if stack.len() != 1 {
            return Err("unexpected end of document".into());
        }
        let mut doc = stack.pop().unwrap_or_default();
        match doc.children.len() {
            0 => Err("no root element".into()),
            _ => Ok(doc.children.swap_remove(0)),
        }
    }

    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    pub fn child(&self, name: &str) -> Option<&Element> {
        self.children.iter().find(|c| c.name == name)
    }

    pub fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Element> + 'a {
        self.children.iter().filter(move |c| c.name == name)
    }

    /// Trimmed text content.
    pub fn text(&self) -> &str {
        self.text.trim()
    }

    /// Depth-first search for the first descendant (or self) with this name.
    pub fn find(&self, name: &str) -> Option<&Element> {
        if self.name == name {
            return Some(self);
        }
        self.children.iter().find_map(|c| c.find(name))
    }
}

fn top(stack: &mut [Element]) -> &mut Element {
    // The synthetic document root is never popped.
    let n = stack.len();
    &mut stack[n - 1]
}

fn start(e: &BytesStart) -> Result<Element, String> {
    let mut el = Element {
        name: String::from_utf8_lossy(e.local_name().as_ref()).into_owned(),
        ..Element::default()
    };
    for attr in e.attributes() {
        let attr = attr.map_err(|err| err.to_string())?;
        let key = attr.key.as_ref();
        if key == b"xmlns" || key.starts_with(b"xmlns:") {
            continue;
        }
        let value = match attr.unescape_value() {
            Ok(v) => v.into_owned(),
            Err(_) => String::from_utf8_lossy(&attr.value).into_owned(),
        };
        el.attrs.push((
            String::from_utf8_lossy(attr.key.local_name().as_ref()).into_owned(),
            value,
        ));
    }
    Ok(el)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_names_attrs_text_and_entities() {
        let doc = r#"<?xml version="1.0"?>
<!DOCTYPE lensdatabase SYSTEM "lensfun-database.dtd">
<!-- comment -->
<a:root xmlns:a="urn:x" a:k="1 &amp; 2">
  <model>Sigma 150-600mm | C</model>
  <model xml:lang="en">A &lt;b&gt;</model>
  <empty x="y"/>
</a:root>"#;
        let root = Element::parse(doc).unwrap();
        assert_eq!(root.name, "root");
        assert_eq!(root.attr("k"), Some("1 & 2"));
        let models: Vec<_> = root.children_named("model").collect();
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].text(), "Sigma 150-600mm | C");
        assert_eq!(models[1].attr("lang"), Some("en"));
        assert_eq!(models[1].text(), "A <b>");
        assert_eq!(root.find("empty").and_then(|e| e.attr("x")), Some("y"));
    }

    #[test]
    fn rejects_broken_documents() {
        assert!(Element::parse("<a><b></a>").is_err());
        assert!(Element::parse("").is_err());
    }
}
