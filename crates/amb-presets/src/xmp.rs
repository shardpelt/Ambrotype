// SPDX-License-Identifier: GPL-3.0-or-later
//! XMP (RDF/XML) reader for Camera Raw settings.
//!
//! Handles both serialisations Adobe writes: simple values as attributes on
//! `rdf:Description`, and arrays / language alternatives / structs as child
//! elements. Namespace prefixes are resolved properly, so files that bind the
//! Camera Raw namespace to a prefix other than `crs` still parse.

use std::collections::BTreeMap;

use quick_xml::NsReader;
use quick_xml::events::{BytesStart, Event};
use quick_xml::name::ResolveResult;

use crate::Error;
use crate::value::Value;

pub const CRS_NS: &str = "http://ns.adobe.com/camera-raw-settings/1.0/";
/// Our own develop settings (features Camera Raw has no tag for, e.g. film
/// negative conversion), written with the `amb:` prefix and keyed `amb:Name`.
/// The URI changes with the product name before the first public release.
pub const AMB_NS: &str = "http://ns.ambrotype.org/develop/1.0/";
/// Key prefix for [`AMB_NS`] properties in a settings map.
pub const AMB_PREFIX: &str = "amb:";
pub const RDF_NS: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
const XML_NS: &str = "http://www.w3.org/XML/1998/namespace";

#[derive(Debug, Default)]
struct Node {
    ns: Option<String>,
    local: String,
    attrs: Vec<Attr>,
    children: Vec<Node>,
    text: String,
}

#[derive(Debug)]
struct Attr {
    ns: Option<String>,
    local: String,
    value: String,
}

impl Node {
    fn is(&self, ns: &str, local: &str) -> bool {
        self.ns.as_deref() == Some(ns) && self.local == local
    }

    fn attr(&self, ns: &str, local: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|a| a.ns.as_deref() == Some(ns) && a.local == local)
            .map(|a| a.value.as_str())
    }
}

/// Key used for a property: Camera Raw properties use their bare local name
/// (`Exposure2012`); anything else keeps its namespace as `{uri}local` so that
/// nothing collides and nothing is lost.
fn key_for(ns: Option<&str>, local: &str) -> String {
    match ns {
        Some(CRS_NS) => local.to_string(),
        Some(AMB_NS) => format!("{AMB_PREFIX}{local}"),
        Some(uri) => format!("{{{uri}}}{local}"),
        None => local.to_string(),
    }
}

fn resolved_ns(result: ResolveResult) -> Option<String> {
    match result {
        ResolveResult::Bound(ns) => Some(String::from_utf8_lossy(ns.as_ref()).into_owned()),
        // The `xml:` prefix is predeclared by the XML spec.
        ResolveResult::Unknown(prefix) if prefix == b"xml" => Some(XML_NS.to_string()),
        _ => None,
    }
}

fn start_node(reader: &NsReader<&[u8]>, ns: Option<String>, e: &BytesStart) -> Result<Node, Error> {
    let mut node = Node {
        ns,
        local: String::from_utf8_lossy(e.local_name().as_ref()).into_owned(),
        ..Node::default()
    };
    for attr in e.attributes() {
        let attr = attr.map_err(|err| Error::Xml(err.to_string()))?;
        let raw_key = attr.key.as_ref();
        if raw_key == b"xmlns" || raw_key.starts_with(b"xmlns:") {
            continue;
        }
        let (ns, local) = reader.resolve_attribute(attr.key);
        let value = attr
            .unescape_value()
            .map_err(|err| Error::Xml(err.to_string()))?
            .into_owned();
        node.attrs.push(Attr {
            ns: resolved_ns(ns),
            local: String::from_utf8_lossy(local.as_ref()).into_owned(),
            value,
        });
    }
    Ok(node)
}

fn parse_tree(input: &str) -> Result<Node, Error> {
    // Tolerate a UTF-8 BOM and leading whitespace before the xpacket header.
    let input = input.trim_start_matches('\u{feff}');
    let mut reader = NsReader::from_str(input);
    reader.config_mut().trim_text(true);

    let mut stack: Vec<Node> = vec![Node::default()]; // synthetic document root
    loop {
        let (ns, event) = reader
            .read_resolved_event()
            .map_err(|err| Error::Xml(err.to_string()))?;
        // Own the namespace so the reader is no longer borrowed mutably.
        let ns = resolved_ns(ns);
        match event {
            Event::Start(e) => {
                let node = start_node(&reader, ns, &e)?;
                stack.push(node);
            }
            Event::Empty(e) => {
                let node = start_node(&reader, ns, &e)?;
                stack.last_mut().expect("root").children.push(node);
            }
            Event::End(_) => {
                let node = stack.pop().expect("balanced");
                let parent = stack
                    .last_mut()
                    .ok_or_else(|| Error::Xml("unbalanced end tag".into()))?;
                parent.children.push(node);
            }
            Event::Text(t) => {
                let text = t.unescape().map_err(|err| Error::Xml(err.to_string()))?;
                stack.last_mut().expect("root").text.push_str(&text);
            }
            Event::CData(t) => {
                stack
                    .last_mut()
                    .expect("root")
                    .text
                    .push_str(&String::from_utf8_lossy(&t));
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if stack.len() != 1 {
        return Err(Error::Xml("unexpected end of document".into()));
    }
    Ok(stack.pop().expect("root"))
}

fn is_skipped_attr(attr: &Attr) -> bool {
    matches!(attr.ns.as_deref(), Some(RDF_NS) | Some(XML_NS))
}

/// Properties of an `rdf:Description` (or a `parseType="Resource"` element):
/// its non-RDF attributes plus its child property elements.
fn description_struct(node: &Node) -> BTreeMap<String, Value> {
    let mut map = BTreeMap::new();
    for attr in node.attrs.iter().filter(|a| !is_skipped_attr(a)) {
        map.insert(
            key_for(attr.ns.as_deref(), &attr.local),
            Value::Text(attr.value.clone()),
        );
    }
    for child in &node.children {
        map.insert(
            key_for(child.ns.as_deref(), &child.local),
            property_value(child),
        );
    }
    map
}

/// Interpret one RDF property element.
fn property_value(node: &Node) -> Value {
    if node.attr(RDF_NS, "parseType") == Some("Resource") {
        return Value::Struct(description_struct(node));
    }
    if let Some(resource) = node.attr(RDF_NS, "resource") {
        return Value::Text(resource.to_string());
    }
    if let Some(container) = node
        .children
        .iter()
        .find(|c| c.is(RDF_NS, "Seq") || c.is(RDF_NS, "Bag") || c.is(RDF_NS, "Alt"))
    {
        let items: Vec<&Node> = container
            .children
            .iter()
            .filter(|c| c.is(RDF_NS, "li"))
            .collect();
        let is_lang_alt = container.local == "Alt"
            && !items.is_empty()
            && items.iter().all(|li| li.attr(XML_NS, "lang").is_some());
        if is_lang_alt {
            return Value::Alt(
                items
                    .iter()
                    .map(|li| {
                        (
                            li.attr(XML_NS, "lang").unwrap_or_default().to_string(),
                            li.text.clone(),
                        )
                    })
                    .collect(),
            );
        }
        return Value::Seq(items.into_iter().map(property_value).collect());
    }
    if let Some(desc) = node.children.iter().find(|c| c.is(RDF_NS, "Description")) {
        return Value::Struct(description_struct(desc));
    }
    // Shorthand struct: property attributes directly on the element.
    let shorthand: Vec<&Attr> = node.attrs.iter().filter(|a| !is_skipped_attr(a)).collect();
    if node.children.is_empty() && node.text.is_empty() && !shorthand.is_empty() {
        return Value::Struct(description_struct(node));
    }
    if !node.children.is_empty() {
        return Value::Struct(description_struct(node));
    }
    Value::Text(node.text.clone())
}

fn collect_descriptions<'a>(node: &'a Node, out: &mut Vec<&'a Node>) {
    for child in &node.children {
        if child.is(RDF_NS, "Description") {
            out.push(child);
        } else {
            collect_descriptions(child, out);
        }
    }
}

/// All top-level Camera Raw (`crs:`) properties in an XMP packet, keyed by local
/// name, plus our own (`amb:`) properties keyed `amb:Name`. Other namespaces
/// (Dublin Core, EXIF, ...) are ignored here.
pub fn parse_crs(input: &str) -> Result<BTreeMap<String, Value>, Error> {
    let root = parse_tree(input)?;
    let mut descriptions = Vec::new();
    collect_descriptions(&root, &mut descriptions);
    if descriptions.is_empty() {
        return Err(Error::NotXmp);
    }
    let mut settings = BTreeMap::new();
    for desc in descriptions {
        let ours = |ns: Option<&str>| matches!(ns, Some(CRS_NS) | Some(AMB_NS));
        for attr in desc.attrs.iter().filter(|a| ours(a.ns.as_deref())) {
            settings.insert(
                key_for(attr.ns.as_deref(), &attr.local),
                Value::Text(attr.value.clone()),
            );
        }
        for child in desc.children.iter().filter(|c| ours(c.ns.as_deref())) {
            settings.insert(
                key_for(child.ns.as_deref(), &child.local),
                property_value(child),
            );
        }
    }
    Ok(settings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attributes_elements_and_alt() {
        let xmp = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/">
 <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
  <rdf:Description xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/"
    crs:Exposure2012="+0.35" crs:Contrast2012="-12">
   <crs:Name><rdf:Alt><rdf:li xml:lang="x-default">Warm &amp; Matte</rdf:li></rdf:Alt></crs:Name>
   <crs:ToneCurvePV2012><rdf:Seq><rdf:li>0, 0</rdf:li><rdf:li>255, 255</rdf:li></rdf:Seq></crs:ToneCurvePV2012>
  </rdf:Description>
 </rdf:RDF>
</x:xmpmeta>"#;
        let s = parse_crs(xmp).unwrap();
        assert_eq!(s["Exposure2012"].as_f64(), Some(0.35));
        assert_eq!(s["Name"].display_text(), Some("Warm & Matte"));
        assert_eq!(s["ToneCurvePV2012"].as_seq().unwrap().len(), 2);
    }

    #[test]
    fn non_crs_prefix_is_resolved() {
        let xmp = r#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
  <rdf:Description xmlns:cr="http://ns.adobe.com/camera-raw-settings/1.0/" cr:Vibrance="10"/>
 </rdf:RDF>"#;
        assert_eq!(parse_crs(xmp).unwrap()["Vibrance"].as_f64(), Some(10.0));
    }

    #[test]
    fn nested_struct_list() {
        let xmp = r#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
   xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/">
  <rdf:Description>
   <crs:Look><rdf:Description crs:Name="Adobe Color" crs:Amount="1"/></crs:Look>
   <crs:MaskGroupBasedCorrections><rdf:Seq>
    <rdf:li rdf:parseType="Resource">
     <crs:LocalExposure2012>0.5</crs:LocalExposure2012>
     <crs:CorrectionMasks><rdf:Seq>
      <rdf:li crs:What="Mask/Gradient" crs:ZeroX="0.5"/>
     </rdf:Seq></crs:CorrectionMasks>
    </rdf:li>
   </rdf:Seq></crs:MaskGroupBasedCorrections>
  </rdf:Description>
 </rdf:RDF>"#;
        let s = parse_crs(xmp).unwrap();
        assert_eq!(
            s["Look"].as_struct().unwrap()["Name"].as_str(),
            Some("Adobe Color")
        );
        let groups = s["MaskGroupBasedCorrections"].as_seq().unwrap();
        let group = groups[0].as_struct().unwrap();
        assert_eq!(group["LocalExposure2012"].as_f64(), Some(0.5));
        let mask = group["CorrectionMasks"].as_seq().unwrap()[0]
            .as_struct()
            .unwrap();
        assert_eq!(mask["What"].as_str(), Some("Mask/Gradient"));
    }

    #[test]
    fn rejects_non_xmp() {
        assert!(matches!(
            parse_crs("<html><body/></html>"),
            Err(Error::NotXmp)
        ));
    }
}
