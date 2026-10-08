//! Writing XMP sidecars without losing what we do not own.
//!
//! A Lightroom sidecar also carries rating, label, keywords, EXIF/TIFF copies
//! and edit history in other namespaces. The `amb-presets` reader keeps only
//! Camera Raw (`crs:`) properties, so re-writing a sidecar from parsed settings
//! alone would silently drop them. [`merge`] writes the new `crs:` settings and
//! carries every other property of the original over verbatim.
//!
//! The photo's rating and keywords ([`PhotoInfo`]) are the exception: they
//! live in Lightroom's own places (`xmp:Rating`, `dc:subject`, and
//! `lr:hierarchicalSubject` for keyword paths) and [`merge_with_info`] can
//! replace them, so Lightroom and other apps read what Ambrotype writes.

use std::collections::BTreeMap;
use std::path::Path;

use amb_presets::Value;
use amb_presets::xmp::{AMB_NS, CRS_NS, RDF_NS};
use quick_xml::NsReader;
use quick_xml::events::Event;
use quick_xml::name::ResolveResult;

use crate::Error;

const XMP_NS: &str = "http://ns.adobe.com/xap/1.0/";
const DC_NS: &str = "http://purl.org/dc/elements/1.1/";
const LR_NS: &str = "http://ns.adobe.com/lightroom/1.0/";

/// A photo's rating and keywords, as stored in its sidecar.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PhotoInfo {
    /// 1..=5 stars; `None` (or 0 in Lightroom's terms) is unrated.
    pub rating: Option<u8>,
    /// In the order given; duplicates (ignoring case) are dropped when writing.
    pub keywords: Vec<String>,
}

impl PhotoInfo {
    /// Trimmed, non-empty, unique keywords and a rating in range.
    pub fn cleaned(&self) -> PhotoInfo {
        let mut seen = std::collections::HashSet::new();
        let keywords = self
            .keywords
            .iter()
            .map(|k| k.trim().to_string())
            .filter(|k| !k.is_empty() && seen.insert(k.to_lowercase()))
            .collect();
        PhotoInfo {
            rating: self.rating.filter(|r| (1..=5).contains(r)),
            keywords,
        }
    }
}

/// The rating and keywords in an XMP packet (empty when there are none).
pub fn read_info(xmp: &str) -> PhotoInfo {
    let mut info = PhotoInfo::default();
    let text = xmp.trim_start_matches('\u{feff}');
    let mut reader = NsReader::from_str(text);
    let mut in_subject = false;
    let mut in_rating = false;
    let mut in_li = false;
    loop {
        match reader.read_resolved_event() {
            Ok((ns, Event::Start(e))) | Ok((ns, Event::Empty(e))) => {
                let local = e.local_name();
                let local = local.as_ref();
                let is = |uri: &str| matches!(ns, ResolveResult::Bound(n) if n.as_ref() == uri.as_bytes());
                if is(DC_NS) && local == b"subject" {
                    in_subject = true;
                } else if is(XMP_NS) && local == b"Rating" {
                    in_rating = true;
                } else if in_subject && is(RDF_NS) && local == b"li" {
                    in_li = true;
                }
                for a in e.attributes().flatten() {
                    let (ans, alocal) = reader.resolve_attribute(a.key);
                    let bound =
                        matches!(ans, ResolveResult::Bound(n) if n.as_ref() == XMP_NS.as_bytes());
                    if bound && alocal.as_ref() == b"Rating" {
                        let v = a
                            .unescape_value()
                            .map(|v| v.to_string())
                            .unwrap_or_default();
                        info.rating = v
                            .trim()
                            .parse::<f64>()
                            .ok()
                            .map(|r| r.round() as i64)
                            .and_then(|r| u8::try_from(r).ok());
                    }
                }
            }
            Ok((_, Event::Text(t))) => {
                let v = t.unescape().map(|v| v.to_string()).unwrap_or_default();
                if in_li {
                    info.keywords.push(v);
                } else if in_rating {
                    info.rating = v
                        .trim()
                        .parse::<f64>()
                        .ok()
                        .map(|r| r.round() as i64)
                        .and_then(|r| u8::try_from(r).ok());
                }
            }
            Ok((ns, Event::End(e))) => {
                let local = e.local_name();
                let is = |uri: &str| matches!(ns, ResolveResult::Bound(n) if n.as_ref() == uri.as_bytes());
                if is(DC_NS) && local.as_ref() == b"subject" {
                    in_subject = false;
                } else if is(XMP_NS) && local.as_ref() == b"Rating" {
                    in_rating = false;
                } else if is(RDF_NS) && local.as_ref() == b"li" {
                    in_li = false;
                }
            }
            Ok((_, Event::Eof)) | Err(_) => break,
            _ => {}
        }
    }
    info.rating = info.rating.filter(|r| (1..=5).contains(r));
    info
}

fn escape_text(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// The namespace URI and local name of a raw `prefix:local` name.
fn resolve<'a>(
    name: &'a str,
    namespaces: &'a BTreeMap<String, String>,
) -> (Option<&'a str>, &'a str) {
    match name.split_once(':') {
        Some((prefix, local)) => (namespaces.get(prefix).map(String::as_str), local),
        None => (None, name),
    }
}

/// The qualified name of a verbatim element's start tag.
fn element_name(xml: &str) -> &str {
    let rest = xml.trim_start().trim_start_matches('<');
    let end = rest
        .find(|c: char| c.is_whitespace() || c == '>' || c == '/')
        .unwrap_or(rest.len());
    &rest[..end]
}

/// The namespace URI and local name of a verbatim element: its prefix is
/// looked up in the Description's declarations, then on the element itself.
fn element_ns(xml: &str, namespaces: &BTreeMap<String, String>) -> (Option<String>, String) {
    let name = element_name(xml);
    let (uri, local) = resolve(name, namespaces);
    if uri.is_some() {
        return (uri.map(str::to_string), local.to_string());
    }
    let prefix = name.split_once(':').map_or("", |(p, _)| p);
    let decl = format!("xmlns:{prefix}=\"");
    let inline = xml.find(&decl).and_then(|i| {
        let rest = &xml[i + decl.len()..];
        rest.find('"').map(|j| rest[..j].to_string())
    });
    (inline, local.to_string())
}

/// The `rdf:li` texts of a verbatim element.
fn list_items(xml: &str) -> Vec<String> {
    let mut reader = NsReader::from_str(xml);
    let mut out = Vec::new();
    let mut in_li = false;
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) if e.local_name().as_ref() == b"li" => in_li = true,
            Ok(Event::End(e)) if e.local_name().as_ref() == b"li" => in_li = false,
            Ok(Event::Text(t)) if in_li => {
                out.push(t.unescape().map(|v| v.to_string()).unwrap_or_default())
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    out
}

/// Replace the rating and keywords in the collected metadata with `info`.
fn apply_info(foreign: &mut Foreign, info: &PhotoInfo) {
    let info = info.cleaned();
    let ns = foreign.namespaces.clone();
    foreign
        .attributes
        .retain(|(name, _)| resolve(name, &ns) != (Some(XMP_NS), "Rating"));
    let kept: std::collections::HashSet<String> =
        info.keywords.iter().map(|k| k.to_lowercase()).collect();
    let mut elements = Vec::new();
    for e in std::mem::take(&mut foreign.elements) {
        let (uri, local) = element_ns(&e, &ns);
        match (uri.as_deref(), local.as_str()) {
            (Some(XMP_NS), "Rating") | (Some(DC_NS), "subject") => {}
            (Some(LR_NS), "hierarchicalSubject") => {
                // Keep the keyword paths whose last part is still a keyword.
                let paths: Vec<String> = list_items(&e)
                    .into_iter()
                    .filter(|p| {
                        kept.contains(&p.rsplit('|').next().unwrap_or("").trim().to_lowercase())
                    })
                    .collect();
                if !paths.is_empty() {
                    let name = element_name(&e).to_string();
                    let items: String = paths
                        .iter()
                        .map(|p| format!("<rdf:li>{}</rdf:li>", escape_text(p)))
                        .collect();
                    // Keep the prefix declared on the element, if that's where it was.
                    let prefix = name.split_once(':').map_or("", |(p, _)| p);
                    let decl = if ns.contains_key(prefix) {
                        String::new()
                    } else {
                        format!(" xmlns:{prefix}=\"{LR_NS}\"")
                    };
                    elements.push(format!("<{name}{decl}><rdf:Bag>{items}</rdf:Bag></{name}>"));
                }
            }
            _ => elements.push(e),
        }
    }
    foreign.elements = elements;
    // Our own prefixes for what we write, unless the sidecar already has them.
    let prefix_for = |foreign: &mut Foreign, uri: &str, want: &str| -> String {
        if let Some((p, _)) = foreign.namespaces.iter().find(|(_, u)| u.as_str() == uri) {
            return p.clone();
        }
        let mut p = want.to_string();
        while foreign.namespaces.contains_key(&p) {
            p.push('x');
        }
        foreign.namespaces.insert(p.clone(), uri.to_string());
        p
    };
    if let Some(r) = info.rating {
        let p = prefix_for(foreign, XMP_NS, "xmp");
        foreign
            .attributes
            .push((format!("{p}:Rating"), r.to_string()));
    }
    if !info.keywords.is_empty() {
        let p = prefix_for(foreign, DC_NS, "dc");
        let items: String = info
            .keywords
            .iter()
            .map(|k| format!("<rdf:li>{}</rdf:li>", escape_text(k)))
            .collect();
        foreign.elements.push(format!(
            "<{p}:subject><rdf:Bag>{items}</rdf:Bag></{p}:subject>"
        ));
    }
}

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
                        if matches!(attr_ns.as_deref(), Some(CRS_NS | AMB_NS | RDF_NS)) {
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
                    && !matches!(ns.as_deref(), Some(CRS_NS | AMB_NS))
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
    merge_with_info(original, settings, None)
}

/// [`merge`], also setting the rating and keywords when `info` is given.
pub fn merge_with_info(
    original: &str,
    settings: &BTreeMap<String, Value>,
    info: Option<&PhotoInfo>,
) -> Result<String, Error> {
    let mut foreign = if original.trim().is_empty() {
        Foreign::default()
    } else {
        collect_foreign(original)?
    };
    if let Some(info) = info {
        apply_info(&mut foreign, info);
    }
    let mut packet = amb_presets::xmp_writer::write_packet(settings, amb_presets::is_signed_tag);
    if foreign.attributes.is_empty() && foreign.elements.is_empty() {
        return Ok(packet);
    }
    // The packet we just wrote has one top-level Description that starts with
    // `<rdf:Description rdf:about=""`; values are escaped, so the first `>`
    // after it ends the start tag.
    const OPEN: &str = "<rdf:Description rdf:about=\"\"";
    let open_at = packet.find(OPEN).ok_or_else(|| xml_err("writer output"))? + OPEN.len();
    let mut extra = String::new();
    let reserved = ["x", "rdf", "crs", "amb", "xml"];
    for (prefix, uri) in &foreign.namespaces {
        if reserved.contains(&prefix.as_str()) || uri == CRS_NS || uri == AMB_NS {
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

/// A sidecar with `info` as its rating and keywords and everything else of
/// `original` (the file as it is on disk) unchanged, develop settings included.
pub fn with_info(original: Option<&str>, info: &PhotoInfo) -> Result<String, Error> {
    let settings = match original {
        Some(text) if !text.trim().is_empty() => {
            amb_presets::xmp::parse_crs(text).map_err(|e| Error::Load(format!("XMP: {e}")))?
        }
        _ => BTreeMap::new(),
    };
    merge_with_info(original.unwrap_or(""), &settings, Some(info))
}

/// Write `text` to `path` via a temporary file in the same directory, so a
/// crash never leaves a half-written sidecar.
pub fn write_atomic(path: &Path, text: &str) -> Result<(), Error> {
    let io = |e: std::io::Error| Error::Io(format!("{}: {e}", path.display()));
    let file_name = path
        .file_name()
        .ok_or_else(|| Error::Io(format!("{}: not a file path", path.display())))?;
    let tmp = path.with_file_name(format!(
        ".{}.amb-tmp-{}",
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
    fn rating_and_keywords_round_trip() {
        let info = read_info(SIDECAR);
        assert_eq!(info.rating, Some(4));
        assert_eq!(info.keywords, ["sunset", "beach"]);

        // New rating and keywords; everything else (label, develop settings) stays.
        let new = PhotoInfo {
            rating: Some(2),
            keywords: vec!["sunset".into(), " family ".into(), "Sunset".into()],
        };
        let out = with_info(Some(SIDECAR), &new).unwrap();
        let back = read_info(&out);
        assert_eq!(back.rating, Some(2));
        assert_eq!(back.keywords, ["sunset", "family"]);
        assert!(out.contains(r#"xmp:Label="Red &amp; Blue""#), "{out}");
        assert!(!out.contains("beach"));
        let settings = amb_presets::xmp::parse_crs(&out).unwrap();
        assert!(settings.contains_key("Exposure2012") && settings.contains_key("ToneCurvePV2012"));

        // Clearing removes them; a sidecar that didn't exist gets created.
        let cleared = with_info(Some(&out), &PhotoInfo::default()).unwrap();
        assert_eq!(read_info(&cleared), PhotoInfo::default());
        let both = PhotoInfo {
            rating: Some(5),
            keywords: vec!["a & b".into()],
        };
        let fresh = with_info(None, &both).unwrap();
        assert_eq!(read_info(&fresh), both);
    }

    #[test]
    fn keyword_paths_follow_the_keywords() {
        let lr = SIDECAR.replace(
            "<dc:subject>",
            r#"<lr:hierarchicalSubject xmlns:lr="http://ns.adobe.com/lightroom/1.0/"><rdf:Bag><rdf:li>Places|beach</rdf:li><rdf:li>Time|sunset</rdf:li></rdf:Bag></lr:hierarchicalSubject>
   <dc:subject>"#,
        );
        let info = PhotoInfo {
            rating: Some(4),
            keywords: vec!["sunset".into()],
        };
        let out = with_info(Some(&lr), &info).unwrap();
        assert!(
            out.contains("Time|sunset") && !out.contains("Places|beach"),
            "{out}"
        );
    }

    #[test]
    fn keeps_foreign_metadata() {
        let mut settings = amb_presets::xmp::parse_crs(SIDECAR).unwrap();
        settings.insert("Exposure2012".into(), Value::Number(1.25));
        settings.remove("ToneCurvePV2012");
        let out = merge(SIDECAR, &settings).unwrap();
        assert!(out.contains(r#"xmp:Rating="4""#), "{out}");
        assert!(out.contains(r#"xmp:Label="Red &amp; Blue""#), "{out}");
        assert!(out.contains("<rdf:li>sunset</rdf:li>"), "{out}");
        assert!(out.contains(r#"xmlns:dc="http://purl.org/dc/elements/1.1/""#));
        assert!(!out.contains("ToneCurvePV2012"), "{out}");
        let back = amb_presets::xmp::parse_crs(&out).unwrap();
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
            amb_presets::xmp::parse_crs(&out).unwrap()["Vibrance"].as_f64(),
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
            amb_presets::xmp_writer::write_packet(&settings, amb_presets::is_signed_tag)
        );
    }
}
