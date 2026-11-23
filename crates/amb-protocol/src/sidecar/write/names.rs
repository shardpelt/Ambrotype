// SPDX-License-Identifier: GPL-3.0-or-later
//! The names of a photo's place in its sidecar, where Lightroom and the IPTC
//! standard keep them: `photoshop:City`, `photoshop:State`,
//! `photoshop:Country` and `Iptc4xmpCore:CountryCode`. Read into and written
//! from [`PhotoInfo`]'s `city`, `state`, `country` and `country_code`.

use super::*;
use quick_xml::name::ResolveResult;

const PHOTOSHOP_NS: &str = "http://ns.adobe.com/photoshop/1.0/";
const IPTC_CORE_NS: &str = "http://iptc.org/std/Iptc4xmp/1.0/xmlns/";

/// Local name, namespace and the prefix we use when the sidecar has none.
const NAMES: [(&str, &str, &str); 4] = [
    ("City", PHOTOSHOP_NS, "photoshop"),
    ("State", PHOTOSHOP_NS, "photoshop"),
    ("Country", PHOTOSHOP_NS, "photoshop"),
    ("CountryCode", IPTC_CORE_NS, "Iptc4xmpCore"),
];

fn field<'a>(info: &'a mut PhotoInfo, local: &str) -> &'a mut Option<String> {
    match local {
        "City" => &mut info.city,
        "State" => &mut info.state,
        "Country" => &mut info.country,
        _ => &mut info.country_code,
    }
}

fn set(info: &mut PhotoInfo, local: &str, v: &str) {
    let v = v.trim();
    if !v.is_empty() {
        *field(info, local) = Some(v.to_string());
    }
}

/// The name in `NAMES` for this namespace and local name.
fn named(ns: &ResolveResult, local: &[u8]) -> Option<&'static str> {
    NAMES
        .iter()
        .find(|(l, uri, _)| {
            local == l.as_bytes()
                && matches!(ns, ResolveResult::Bound(n) if n.as_ref() == uri.as_bytes())
        })
        .map(|(l, _, _)| *l)
}

/// Read the names in an XMP packet into `info`.
pub(in crate::sidecar) fn read(xmp: &str, info: &mut PhotoInfo) {
    let mut reader = NsReader::from_str(xmp.trim_start_matches('\u{feff}'));
    let mut inside: Option<&'static str> = None;
    loop {
        match reader.read_resolved_event() {
            Ok((ns, Event::Start(e))) => {
                inside = named(&ns, e.local_name().as_ref());
                attributes(&reader, &e, info);
            }
            Ok((_, Event::Empty(e))) => attributes(&reader, &e, info),
            Ok((_, Event::Text(t))) => {
                if let (Some(l), Ok(v)) = (inside, t.unescape()) {
                    set(info, l, &v);
                }
            }
            Ok((_, Event::End(_))) => inside = None,
            Ok((_, Event::Eof)) | Err(_) => break,
            _ => {}
        }
    }
}

/// The names given as attributes of an element (`<rdf:Description photoshop:City="…">`).
fn attributes(reader: &NsReader<&[u8]>, e: &quick_xml::events::BytesStart, info: &mut PhotoInfo) {
    for a in e.attributes().flatten() {
        let (ans, alocal) = reader.resolve_attribute(a.key);
        if let (Some(l), Ok(v)) = (named(&ans, alocal.as_ref()), a.unescape_value()) {
            set(info, l, &v);
        }
    }
}

/// Replace the names `info` has (`""` removes one; `None` keeps the file's).
pub(super) fn write(foreign: &mut Foreign, info: &PhotoInfo) {
    let mut info = info.clone();
    for (local, uri, want) in NAMES {
        let Some(text) = field(&mut info, local).clone() else {
            continue;
        };
        let ns = foreign.namespaces.clone();
        foreign
            .attributes
            .retain(|(name, _)| resolve(name, &ns) != (Some(uri), local));
        foreign.elements.retain(|e| {
            let (u, l) = element_ns(e, &ns);
            !(u.as_deref() == Some(uri) && l == local)
        });
        if !text.is_empty() {
            let p = prefix_for(foreign, uri, want);
            foreign.attributes.push((format!("{p}:{local}"), text));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sidecar::read_info;

    #[test]
    fn names_round_trip_and_read_both_forms() {
        let info = PhotoInfo {
            city: Some("Paris".into()),
            country: Some("France".into()),
            country_code: Some("FR".into()),
            ..PhotoInfo::default()
        };
        let xmp = with_info(None, &info).unwrap();
        assert!(xmp.contains("photoshop:City=\"Paris\""), "{xmp}");
        assert!(xmp.contains("Iptc4xmpCore:CountryCode=\"FR\""), "{xmp}");
        let back = read_info(&xmp);
        assert_eq!(back.city.as_deref(), Some("Paris"));
        assert_eq!(back.country.as_deref(), Some("France"));
        assert_eq!(back.state, None);
        // As elements, the way other programs write them; "" removes one.
        let theirs = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description rdf:about="" xmlns:photoshop="http://ns.adobe.com/photoshop/1.0/"><photoshop:City>Amsterdam</photoshop:City><photoshop:State>NH</photoshop:State></rdf:Description></rdf:RDF></x:xmpmeta>"#;
        let read = read_info(theirs);
        assert_eq!(
            (read.city.as_deref(), read.state.as_deref()),
            (Some("Amsterdam"), Some("NH"))
        );
        let cleared = PhotoInfo {
            city: Some(String::new()),
            ..read
        };
        let out = read_info(&with_info(Some(theirs), &cleared).unwrap());
        assert_eq!((out.city, out.state.as_deref()), (None, Some("NH")));
    }
}
