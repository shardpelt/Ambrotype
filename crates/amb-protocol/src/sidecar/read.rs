// SPDX-License-Identifier: GPL-3.0-or-later
//! Reading a sidecar: its history, snapshots and the photo's info.

use super::{
    DC_NS, DC_TEXTS, EXIF_NS, Flag, LIBRARY_NS, PhotoInfo, StackRef, XMP_NS, parse_rating,
    parse_xmp_coordinate,
};
use amb_presets::xmp::RDF_NS;
use quick_xml::NsReader;
use quick_xml::events::Event;
use quick_xml::name::ResolveResult;

/// The stored edit history in an XMP packet (`amblib:History`), if any.
pub fn read_history(xmp: &str) -> Option<String> {
    read_library_text(xmp, "History")
}

/// The stored snapshots in an XMP packet (`amblib:Snapshots`), if any.
pub fn read_snapshots(xmp: &str) -> Option<String> {
    read_library_text(xmp, "Snapshots")
}

/// The text of the `amblib:<local>` element.
fn read_library_text(xmp: &str, local: &str) -> Option<String> {
    let mut reader = NsReader::from_str(xmp.trim_start_matches('\u{feff}'));
    let mut inside = false;
    let mut out = String::new();
    loop {
        match reader.read_resolved_event() {
            Ok((ResolveResult::Bound(n), Event::Start(e)))
                if n.as_ref() == LIBRARY_NS.as_bytes()
                    && e.local_name().as_ref() == local.as_bytes() =>
            {
                inside = true;
            }
            Ok((_, Event::Text(t))) if inside => out.push_str(&t.unescape().ok()?),
            Ok((_, Event::End(_))) if inside => return Some(out),
            Ok((_, Event::Eof)) | Err(_) => return None,
            _ => {}
        }
    }
}

/// The rating, label and keywords in an XMP packet (empty when there are none).
pub fn read_info(xmp: &str) -> PhotoInfo {
    let mut info = PhotoInfo::default();
    let text = xmp.trim_start_matches('\u{feff}');
    let mut reader = NsReader::from_str(text);
    let mut in_subject = false;
    let mut in_rating = false;
    let mut in_label = false;
    let mut in_flag = false;
    let mut rated_reject = false;
    let mut in_li = false;
    // Inside dc:title and the like: which, and the items read so far.
    let mut in_text: Option<&'static str> = None;
    let mut items: Vec<String> = Vec::new();
    let mut in_item = false;
    // exif:GPSLatitude and exif:GPSLongitude, as attributes or elements.
    let mut gps: [Option<f64>; 2] = [None, None];
    let mut in_gps: Option<usize> = None;
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
                } else if is(XMP_NS) && local == b"Label" {
                    in_label = true;
                } else if is(LIBRARY_NS) && local == b"Flag" {
                    in_flag = true;
                } else if is(EXIF_NS) && local == b"GPSLatitude" {
                    in_gps = Some(0);
                } else if is(EXIF_NS) && local == b"GPSLongitude" {
                    in_gps = Some(1);
                } else if in_subject && is(RDF_NS) && local == b"li" {
                    in_li = true;
                } else if is(DC_NS) {
                    if let Some((name, _)) = DC_TEXTS.iter().find(|(n, _)| n.as_bytes() == local) {
                        in_text = Some(name);
                        items.clear();
                    }
                } else if in_text.is_some() && is(RDF_NS) && local == b"li" {
                    in_item = true;
                    items.push(String::new());
                }
                for a in e.attributes().flatten() {
                    let (ans, alocal) = reader.resolve_attribute(a.key);
                    let bound =
                        matches!(ans, ResolveResult::Bound(n) if n.as_ref() == XMP_NS.as_bytes());
                    if bound && alocal.as_ref() == b"Label" {
                        info.label = a.unescape_value().ok().map(|v| v.to_string());
                    }
                    if bound && alocal.as_ref() == b"Rating" {
                        let v = a
                            .unescape_value()
                            .map(|v| v.to_string())
                            .unwrap_or_default();
                        (info.rating, rated_reject) = parse_rating(&v);
                    }
                    let exif =
                        matches!(ans, ResolveResult::Bound(n) if n.as_ref() == EXIF_NS.as_bytes());
                    if exif && matches!(alocal.as_ref(), b"GPSLatitude" | b"GPSLongitude") {
                        let i = usize::from(alocal.as_ref() == b"GPSLongitude");
                        gps[i] = a
                            .unescape_value()
                            .ok()
                            .and_then(|v| parse_xmp_coordinate(&v));
                    }
                    let library = matches!(ans, ResolveResult::Bound(n) if n.as_ref() == LIBRARY_NS.as_bytes());
                    if library && alocal.as_ref() == b"Flag" {
                        info.flag = a.unescape_value().ok().and_then(|v| Flag::parse(&v));
                    }
                    if library && alocal.as_ref() == b"Stack" {
                        let id = a
                            .unescape_value()
                            .map(|v| v.trim().to_string())
                            .unwrap_or_default();
                        if !id.is_empty() {
                            info.stack.get_or_insert_with(StackRef::default).id = id;
                        }
                    }
                    if library && alocal.as_ref() == b"StackTop" {
                        let top = a.unescape_value().is_ok_and(|v| v.trim() == "true");
                        info.stack.get_or_insert_with(StackRef::default).top = top;
                    }
                }
            }
            Ok((_, Event::Text(t))) => {
                let v = t.unescape().map(|v| v.to_string()).unwrap_or_default();
                if let Some(i) = in_gps {
                    gps[i] = parse_xmp_coordinate(&v);
                } else if in_item {
                    if let Some(last) = items.last_mut() {
                        last.push_str(&v);
                    }
                } else if in_li {
                    info.keywords.push(v);
                } else if in_label {
                    info.label = Some(v);
                } else if in_flag {
                    info.flag = Flag::parse(&v);
                } else if in_rating {
                    (info.rating, rated_reject) = parse_rating(&v);
                }
            }
            Ok((ns, Event::End(e))) => {
                let local = e.local_name();
                let is = |uri: &str| matches!(ns, ResolveResult::Bound(n) if n.as_ref() == uri.as_bytes());
                if is(DC_NS) && local.as_ref() == b"subject" {
                    in_subject = false;
                } else if is(XMP_NS) && local.as_ref() == b"Rating" {
                    in_rating = false;
                } else if is(XMP_NS) && local.as_ref() == b"Label" {
                    in_label = false;
                } else if is(LIBRARY_NS) && local.as_ref() == b"Flag" {
                    in_flag = false;
                } else if is(EXIF_NS) && matches!(local.as_ref(), b"GPSLatitude" | b"GPSLongitude")
                {
                    in_gps = None;
                } else if is(RDF_NS) && local.as_ref() == b"li" {
                    in_li = false;
                    in_item = false;
                } else if let Some(name) =
                    in_text.filter(|n| is(DC_NS) && local.as_ref() == n.as_bytes())
                {
                    // A text keeps its first (x-default) item; creators are joined.
                    let joined = if name == "creator" {
                        items.join("; ")
                    } else {
                        items.first().cloned().unwrap_or_default()
                    };
                    *info.text_mut(name) = Some(joined);
                    in_text = None;
                }
            }
            Ok((_, Event::Eof)) | Err(_) => break,
            _ => {}
        }
    }
    if let [Some(lat), Some(lon)] = gps {
        info.gps = Some(format!("{lat:.6},{lon:.6}"));
    }
    info.orientation = amb_presets::xmp::tiff_orientation(text);
    if info.flag.is_none() && rated_reject {
        info.flag = Some(Flag::Reject);
    }
    // A top mark without a stack means nothing.
    if info.stack.as_ref().is_some_and(|s| s.id.is_empty()) {
        info.stack = None;
    }
    info.cleaned_keeping_keywords()
}

impl PhotoInfo {
    /// Rating in range and a non-empty label, keywords as read.
    fn cleaned_keeping_keywords(mut self) -> PhotoInfo {
        let keywords = std::mem::take(&mut self.keywords);
        PhotoInfo {
            keywords,
            ..self.cleaned()
        }
    }
}
