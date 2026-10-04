//! Writing XMP sidecars without losing what we do not own.
//!
//! A Lightroom sidecar also carries rating, label, keywords, EXIF/TIFF copies
//! and edit history in other namespaces. The `flb-presets` reader keeps only
//! Camera Raw (`crs:`) properties, so re-writing a sidecar from parsed settings
//! alone would silently drop them. [`merge`] writes the new `crs:` settings and
//! carries every other property of the original over verbatim.

use std::collections::BTreeMap;
use std::path::Path;

use flb_presets::Value;
use flb_presets::xmp::{CRS_NS, RDF_NS};
use quick_xml::NsReader;
use quick_xml::events::Event;
use quick_xml::name::ResolveResult;

use crate::Error;

#[derive(Default)]
struct Foreign {
    /// `xmlns:prefix` → URI, from the Description and its ancestors.
    namespaces: BTreeMap<String, String>,
    /// Raw qualified name → unescaped value.
    attributes: Vec<(String, String)>,
    /// Verbatim XML of non-crs property elements.
    elements: Vec<String>,
}

/// Escape an attribute value. Newlines, carriage returns and tabs become
/// character references, because XML attribute-value normalisation turns raw
/// ones into spaces (XML 1.0 section 3.3.3).
fn escape_attr(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\n', "&#xA;")
        .replace('\r', "&#xD;")
        .replace('\t', "&#x9;")
}

fn xml_err(e: impl std::fmt::Display) -> Error {
    Error::Load(format!("XMP: {e}"))
}

/// Collect the non-Camera-Raw content of the top-level `rdf:Description`s.
fn collect_foreign(original: &str) -> Result<Foreign, Error> {
    let text = original.trim_start_matches('\u{feff}');
    let mut reader = NsReader::from_str(text);
    let mut foreign = Foreign::default();
    // Namespace declarations of open elements, innermost last.
    let mut scopes: Vec<Vec<(String, String)>> = Vec::new();
    // Element path of (namespace, local name).
    let mut path: Vec<(Option<String>, String)> = Vec::new();
    // Depth of the top-level Description we are inside, if any.
    let mut in_desc: Option<usize> = None;
    // Start offset and depth of a foreign property being copied.
    let mut copying: Option<(usize, usize)> = None;
    loop {
        let before = usize::try_from(reader.buffer_position()).unwrap_or(usize::MAX);
        let (ns, event) = reader.read_resolved_event().map_err(xml_err)?;
        let ns = match ns {
            ResolveResult::Bound(n) => Some(String::from_utf8_lossy(n.as_ref()).into_owned()),
            _ => None,
        };
        match event {
            Event::Start(ref e) | Event::Empty(ref e) => {
                let is_empty = matches!(event, Event::Empty(_));
                let local = String::from_utf8_lossy(e.local_name().as_ref()).into_owned();
                let mut decls = Vec::new();
                for attr in e.attributes().with_checks(false).flatten() {
                    let key = String::from_utf8_lossy(attr.key.as_ref()).into_owned();
                    if let Some(prefix) = key.strip_prefix("xmlns:") {
                        let uri = attr.unescape_value().map_err(xml_err)?.into_owned();
                        decls.push((prefix.to_string(), uri));
                    }
                }
                let depth = path.len();
                let parent_is_rdf = path
                    .last()
                    .is_some_and(|(n, l)| n.as_deref() == Some(RDF_NS) && l == "RDF");
                let is_top_desc = in_desc.is_none()
                    && parent_is_rdf
                    && ns.as_deref() == Some(RDF_NS)
                    && local == "Description";
                if is_top_desc {
                    for scope in scopes.iter().chain(std::iter::once(&decls)) {
                        for (p, u) in scope {
                            foreign.namespaces.insert(p.clone(), u.clone());
                        }
                    }
                    for attr in e.attributes().with_checks(false).flatten() {
                        let key = String::from_utf8_lossy(attr.key.as_ref()).into_owned();
                        if key == "xmlns" || key.starts_with("xmlns:") {
                            continue;
                        }
                        let (attr_ns, _) = reader.resolve_attribute(attr.key);
                        let attr_ns = match attr_ns {
                            ResolveResult::Bound(n) => {
                                Some(String::from_utf8_lossy(n.as_ref()).into_owned())
                            }
                            _ => None,
                        };
                        if matches!(attr_ns.as_deref(), Some(CRS_NS) | Some(RDF_NS)) {
                            continue;
                        }
                        let value = attr.unescape_value().map_err(xml_err)?.into_owned();
                        if !foreign.attributes.iter().any(|(k, _)| *k == key) {
                            foreign.attributes.push((key, value));
                        }
                    }
                    if !is_empty {
                        in_desc = Some(depth);
                    }
                } else if in_desc == Some(depth.wrapping_sub(1))
                    && copying.is_none()
                    && ns.as_deref() != Some(CRS_NS)
                {
                    // A direct child property of the Description, not ours.
                    if is_empty {
                        let end = usize::try_from(reader.buffer_position()).unwrap_or(0);
                        foreign.elements.push(text[before..end].trim().to_string());
                    } else {
                        copying = Some((before, depth));
                    }
                }
                if !is_empty {
                    path.push((ns, local));
                    scopes.push(decls);
                }
            }
            Event::End(_) => {
                path.pop();
                scopes.pop();
                let depth = path.len();
                if let Some((start, d)) = copying {
                    if d == depth {
                        let end = usize::try_from(reader.buffer_position()).unwrap_or(0);
                        foreign.elements.push(text[start..end].trim().to_string());
                        copying = None;
                    }
                }
                if in_desc == Some(depth) {
                    in_desc = None;
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(foreign)
}

/// Write `settings` as the Camera Raw content of `original` (an existing XMP
/// sidecar), keeping all of its non-Camera-Raw properties.
pub fn merge(original: &str, settings: &BTreeMap<String, Value>) -> Result<String, Error> {
    let foreign = collect_foreign(original)?;
    let mut packet = flb_presets::xmp_writer::write_packet(settings, flb_presets::is_signed_tag);
    if foreign.attributes.is_empty() && foreign.elements.is_empty() {
        return Ok(packet);
    }
    // The packet we just wrote has one top-level Description that starts with
    // `<rdf:Description rdf:about=""`; values are escaped, so the first `>`
    // after it ends the start tag.
    const OPEN: &str = "<rdf:Description rdf:about=\"\"";
    let open_at = packet.find(OPEN).ok_or_else(|| xml_err("writer output"))? + OPEN.len();
    let mut extra = String::new();
    let reserved = ["x", "rdf", "crs", "xml"];
    for (prefix, uri) in &foreign.namespaces {
        if reserved.contains(&prefix.as_str()) || uri == CRS_NS {
            continue;
        }
        let decl = format!("xmlns:{prefix}=");
        if packet.contains(&decl) {
            continue;
        }
        extra.push_str(&format!("\n    {decl}\"{}\"", escape_attr(uri)));
    }
    for (key, value) in &foreign.attributes {
        extra.push_str(&format!("\n    {key}=\"{}\"", escape_attr(value)));
    }
    packet.insert_str(open_at, &extra);

    if !foreign.elements.is_empty() {
        let tag_end = open_at
            + packet[open_at..]
                .find('>')
                .ok_or_else(|| xml_err("writer"))?;
        let children: String = foreign
            .elements
            .iter()
            .map(|e| format!("   {e}\n"))
            .collect();
        if packet[..tag_end].ends_with('/') {
            // `<rdf:Description .../>` → open it and close it after the children.
            packet.replace_range(
                tag_end - 1..tag_end + 1,
                &format!(">\n{children}  </rdf:Description>"),
            );
        } else {
            let close = packet
                .rfind("</rdf:Description>")
                .ok_or_else(|| xml_err("writer output"))?;
            let line_start = packet[..close].rfind('\n').map_or(close, |i| i + 1);
            packet.insert_str(line_start, &children);
        }
    }
    Ok(packet)
}

/// Write `text` to `path` via a temporary file in the same directory, so a
/// crash never leaves a half-written sidecar.
pub fn write_atomic(path: &Path, text: &str) -> Result<(), Error> {
    let io = |e: std::io::Error| Error::Io(format!("{}: {e}", path.display()));
    let file_name = path
        .file_name()
        .ok_or_else(|| Error::Io(format!("{}: not a file path", path.display())))?;
    let tmp = path.with_file_name(format!(
        ".{}.flb-tmp-{}",
        file_name.to_string_lossy(),
        std::process::id()
    ));
    std::fs::write(&tmp, text).map_err(io)?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        io(e)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIDECAR: &str = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/">
 <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
  <rdf:Description rdf:about=""
    xmlns:xmp="http://ns.adobe.com/xap/1.0/"
    xmlns:dc="http://purl.org/dc/elements/1.1/"
    xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/"
    xmp:Rating="4" xmp:Label="Red &amp; Blue"
    crs:Exposure2012="+0.65" crs:Temperature="5400">
   <dc:subject><rdf:Bag><rdf:li>sunset</rdf:li><rdf:li>beach</rdf:li></rdf:Bag></dc:subject>
   <crs:ToneCurvePV2012><rdf:Seq><rdf:li>0, 0</rdf:li><rdf:li>255, 255</rdf:li></rdf:Seq></crs:ToneCurvePV2012>
  </rdf:Description>
 </rdf:RDF>
</x:xmpmeta>"#;

    #[test]
    fn keeps_foreign_metadata() {
        let mut settings = flb_presets::xmp::parse_crs(SIDECAR).unwrap();
        settings.insert("Exposure2012".into(), Value::Number(1.25));
        settings.remove("ToneCurvePV2012");
        let out = merge(SIDECAR, &settings).unwrap();
        assert!(out.contains(r#"xmp:Rating="4""#), "{out}");
        assert!(out.contains(r#"xmp:Label="Red &amp; Blue""#), "{out}");
        assert!(out.contains("<rdf:li>sunset</rdf:li>"), "{out}");
        assert!(out.contains(r#"xmlns:dc="http://purl.org/dc/elements/1.1/""#));
        assert!(!out.contains("ToneCurvePV2012"), "{out}");
        let back = flb_presets::xmp::parse_crs(&out).unwrap();
        assert_eq!(back["Exposure2012"].as_f64(), Some(1.25));
        assert_eq!(back["Temperature"].as_f64(), Some(5400.0));
        // Still well-formed and the foreign values survive a parse.
        let again = collect_foreign(&out).unwrap();
        assert_eq!(again.attributes.len(), 2);
        assert_eq!(again.elements.len(), 1);
    }

    #[test]
    fn self_closing_description_gets_children() {
        let mut settings = BTreeMap::new();
        settings.insert("Vibrance".to_string(), Value::Number(10.0));
        let out = merge(SIDECAR, &settings).unwrap();
        assert!(out.contains("</rdf:Description>"));
        assert!(collect_foreign(&out).unwrap().elements.len() == 1);
        assert_eq!(
            flb_presets::xmp::parse_crs(&out).unwrap()["Vibrance"].as_f64(),
            Some(10.0)
        );
    }

    #[test]
    fn whitespace_in_foreign_attributes_survives() {
        let src = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description rdf:about="" xmlns:photoshop="http://ns.adobe.com/photoshop/1.0/" xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/" photoshop:Instructions="line1&#xA;line2&#xD;&#x9;end" crs:Exposure2012="+0.30"/></rdf:RDF></x:xmpmeta>"#;
        let mut settings = BTreeMap::new();
        settings.insert("Exposure2012".to_string(), Value::Number(0.5));
        let out = merge(src, &settings).unwrap();
        assert!(!out.contains("line1\nline2"), "{out}");
        assert!(out.contains("line1&#xA;line2&#xD;&#x9;end"), "{out}");
        let again = collect_foreign(&out).unwrap();
        assert_eq!(again.attributes[0].1, "line1\nline2\r\tend");
    }

    #[test]
    fn plain_crs_file_is_rewritten_cleanly() {
        let src = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/" crs:Sharpness="+80"/></rdf:RDF></x:xmpmeta>"#;
        let mut settings = BTreeMap::new();
        settings.insert("Sharpness".to_string(), Value::Number(50.0));
        let out = merge(src, &settings).unwrap();
        assert_eq!(
            out,
            flb_presets::xmp_writer::write_packet(&settings, flb_presets::is_signed_tag)
        );
    }
}
