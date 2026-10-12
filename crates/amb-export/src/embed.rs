// SPDX-License-Identifier: GPL-3.0-or-later
//! The photo's rating and keywords inside exported files, as XMP: the
//! standard place that Lightroom, Bridge, Capture One, Apple Photos, digiKam,
//! darktable and most websites read them from (EXIF Rating/XPKeywords, in
//! [`crate::exif`], is mainly for Windows). JPEGs also get IPTC keywords,
//! for older software that reads only those.
//!
//! The metadata is added to the finished file, so it works the same for every
//! encoder:
//! - JPEG: an APP1 "XMP" segment and an APP13 "Photoshop 3.0" IPTC block,
//!   after the file's other APP segments.
//! - PNG: an `iTXt` chunk with the keyword `XML:com.adobe.xmp`, before the image data.
//! - TIFF: tag 700 (XMLPacket) in the first IFD.
//! - WebP: an `XMP ` chunk, flagged in the `VP8X` header.
//! - AVIF: an XMP item (`mime`, `application/rdf+xml`) that describes the
//!   image (`cdsc`), as HEIF files carry it, with its data at the end.
//! - OpenEXR and JPEG XL: not yet.

use crate::Format;
use crate::exif::Tags;

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// An XMP packet with the rating, label and keywords, or `None` when there are none.
pub fn xmp_packet(tags: &Tags) -> Option<String> {
    let rating = tags.rating.filter(|r| (1..=5).contains(r));
    let label = tags
        .label
        .as_deref()
        .map(str::trim)
        .filter(|l| !l.is_empty());
    let keywords: Vec<&String> = tags
        .keywords
        .iter()
        .filter(|k| !k.trim().is_empty())
        .collect();
    let texts: Vec<(&str, bool, &str)> = [
        ("title", false, &tags.title),
        ("description", false, &tags.caption),
        ("creator", true, &tags.creator),
        ("rights", false, &tags.copyright),
    ]
    .into_iter()
    .filter_map(|(name, list, v)| {
        let v = v.as_deref().map(str::trim).filter(|t| !t.is_empty())?;
        Some((name, list, v))
    })
    .collect();
    if rating.is_none() && label.is_none() && keywords.is_empty() && texts.is_empty() {
        return None;
    }
    let mut out = String::from(
        "<?xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\n\
         <x:xmpmeta xmlns:x=\"adobe:ns:meta/\" x:xmptk=\"Ambrotype\">\n\
         \x20<rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\n\
         \x20 <rdf:Description rdf:about=\"\"\n\
         \x20   xmlns:xmp=\"http://ns.adobe.com/xap/1.0/\"\n\
         \x20   xmlns:dc=\"http://purl.org/dc/elements/1.1/\"\n\
         \x20   xmp:CreatorTool=\"Ambrotype\"",
    );
    if let Some(r) = rating {
        out.push_str(&format!("\n    xmp:Rating=\"{r}\""));
    }
    if let Some(l) = label {
        out.push_str(&format!("\n    xmp:Label=\"{}\"", escape(l)));
    }
    if keywords.is_empty() && texts.is_empty() {
        out.push_str("/>\n");
    } else {
        out.push_str(">\n");
        if !keywords.is_empty() {
            out.push_str("   <dc:subject><rdf:Bag>");
            for k in &keywords {
                out.push_str(&format!("<rdf:li>{}</rdf:li>", escape(k.trim())));
            }
            out.push_str("</rdf:Bag></dc:subject>\n");
        }
        for (name, list, v) in texts {
            let items = if list {
                let li: String = v
                    .split(';')
                    .map(str::trim)
                    .filter(|t| !t.is_empty())
                    .map(|t| format!("<rdf:li>{}</rdf:li>", escape(t)))
                    .collect();
                format!("<rdf:Seq>{li}</rdf:Seq>")
            } else {
                format!(
                    "<rdf:Alt><rdf:li xml:lang=\"x-default\">{}</rdf:li></rdf:Alt>",
                    escape(v)
                )
            };
            out.push_str(&format!("   <dc:{name}>{items}</dc:{name}>\n"));
        }
        out.push_str("  </rdf:Description>\n");
    }
    out.push_str(" </rdf:RDF>\n</x:xmpmeta>\n<?xpacket end=\"r\"?>");
    Some(out)
}

/// IPTC-IIM keywords (2:25, UTF-8) in a Photoshop "8BIM" resource, for a JPEG APP13.
fn iptc_app13(keywords: &[String]) -> Option<Vec<u8>> {
    let keywords: Vec<&str> = keywords
        .iter()
        .map(|k| k.trim())
        .filter(|k| !k.is_empty())
        .collect();
    if keywords.is_empty() {
        return None;
    }
    let mut iim = Vec::new();
    let mut dataset = |record: u8, id: u8, data: &[u8]| {
        iim.extend_from_slice(&[0x1C, record, id]);
        iim.extend_from_slice(&(data.len() as u16).to_be_bytes());
        iim.extend_from_slice(data);
    };
    dataset(1, 90, b"\x1b%G"); // coded character set: UTF-8
    dataset(2, 0, &[0, 4]); // record version 4
    for k in keywords {
        // IPTC keywords hold at most 64 bytes; cut on a character boundary.
        let mut end = k.len().min(64);
        while !k.is_char_boundary(end) {
            end -= 1;
        }
        dataset(2, 25, &k.as_bytes()[..end]);
    }
    let mut out = b"Photoshop 3.0\0".to_vec();
    out.extend_from_slice(b"8BIM");
    out.extend_from_slice(&0x0404u16.to_be_bytes());
    out.extend_from_slice(&[0, 0]); // empty name, padded to even
    out.extend_from_slice(&(iim.len() as u32).to_be_bytes());
    out.extend_from_slice(&iim);
    if iim.len() % 2 == 1 {
        out.push(0);
    }
    Some(out)
}

fn jpeg_segment(marker: u8, payload: &[u8]) -> Option<Vec<u8>> {
    let len = u16::try_from(payload.len() + 2).ok()?;
    let mut seg = vec![0xFF, marker];
    seg.extend_from_slice(&len.to_be_bytes());
    seg.extend_from_slice(payload);
    Some(seg)
}

fn embed_jpeg(bytes: &[u8], xmp: Option<&str>, tags: &Tags) -> Option<Vec<u8>> {
    if bytes.get(..2)? != [0xFF, 0xD8] {
        return None;
    }
    // After SOI and the APPn segments already there (JFIF, EXIF, ICC).
    let mut at = 2;
    while bytes.get(at) == Some(&0xFF) && matches!(bytes.get(at + 1), Some(0xE0..=0xEF)) {
        let len = u16::from_be_bytes([*bytes.get(at + 2)?, *bytes.get(at + 3)?]) as usize;
        at += 2 + len;
    }
    let mut insert = Vec::new();
    if let Some(x) = xmp {
        let mut payload = b"http://ns.adobe.com/xap/1.0/\0".to_vec();
        payload.extend_from_slice(x.as_bytes());
        insert.extend(jpeg_segment(0xE1, &payload)?);
    }
    if let Some(iptc) = iptc_app13(&tags.keywords) {
        insert.extend(jpeg_segment(0xED, &iptc)?);
    }
    let mut out = Vec::with_capacity(bytes.len() + insert.len());
    out.extend_from_slice(&bytes[..at]);
    out.extend_from_slice(&insert);
    out.extend_from_slice(&bytes[at..]);
    Some(out)
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

fn embed_png(bytes: &[u8], xmp: &str) -> Option<Vec<u8>> {
    if bytes.get(..8)? != b"\x89PNG\r\n\x1a\n" {
        return None;
    }
    // Before the first IDAT.
    let mut at = 8;
    loop {
        let len = u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?) as usize;
        if bytes.get(at + 4..at + 8)? == b"IDAT" {
            break;
        }
        at += 12 + len;
    }
    // iTXt: keyword, NUL, compression flag 0, method 0, language "", NUL, translated "", NUL, text.
    let mut body = b"iTXtXML:com.adobe.xmp\0\0\0\0\0".to_vec();
    body.extend_from_slice(xmp.as_bytes());
    let mut chunk = ((body.len() - 4) as u32).to_be_bytes().to_vec();
    chunk.extend_from_slice(&body);
    chunk.extend_from_slice(&crc32(&body).to_be_bytes());
    let mut out = bytes[..at].to_vec();
    out.extend_from_slice(&chunk);
    out.extend_from_slice(&bytes[at..]);
    Some(out)
}

fn embed_tiff(bytes: &[u8], xmp: &str) -> Option<Vec<u8>> {
    // Little-endian classic TIFF (what our encoder writes). The first IFD is
    // copied to the end of the file with tag 700 added, and the header points
    // at the copy; the old IFD's values stay where they are.
    if bytes.get(..4)? != b"II*\0" {
        return None;
    }
    let u16_at = |i: usize| -> Option<u16> {
        Some(u16::from_le_bytes(bytes.get(i..i + 2)?.try_into().ok()?))
    };
    let u32_at = |i: usize| -> Option<u32> {
        Some(u32::from_le_bytes(bytes.get(i..i + 4)?.try_into().ok()?))
    };
    let ifd = u32_at(4)? as usize;
    let n = u16_at(ifd)? as usize;
    let mut entries: Vec<[u8; 12]> = (0..n)
        .map(|i| {
            bytes
                .get(ifd + 2 + 12 * i..ifd + 14 + 12 * i)?
                .try_into()
                .ok()
        })
        .collect::<Option<_>>()?;
    let next = u32_at(ifd + 2 + 12 * n)?;
    entries.retain(|e| u16::from_le_bytes([e[0], e[1]]) != 700);
    let mut out = bytes.to_vec();
    if out.len() % 2 == 1 {
        out.push(0);
    }
    let data_at = u32::try_from(out.len()).ok()?;
    out.extend_from_slice(xmp.as_bytes());
    if out.len() % 2 == 1 {
        out.push(0);
    }
    let mut entry = [0u8; 12];
    entry[..2].copy_from_slice(&700u16.to_le_bytes());
    entry[2..4].copy_from_slice(&1u16.to_le_bytes()); // BYTE
    entry[4..8].copy_from_slice(&(xmp.len() as u32).to_le_bytes());
    entry[8..12].copy_from_slice(&data_at.to_le_bytes());
    entries.push(entry);
    entries.sort_by_key(|e| u16::from_le_bytes([e[0], e[1]]));
    let new_ifd = u32::try_from(out.len()).ok()?;
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    for e in &entries {
        out.extend_from_slice(e);
    }
    out.extend_from_slice(&next.to_le_bytes());
    out[4..8].copy_from_slice(&new_ifd.to_le_bytes());
    Some(out)
}

fn embed_webp(bytes: &[u8], xmp: &str) -> Option<Vec<u8>> {
    if bytes.get(..4)? != b"RIFF" || bytes.get(8..12)? != b"WEBP" || bytes.get(12..16)? != b"VP8X" {
        return None;
    }
    let mut out = bytes.to_vec();
    out[20] |= 0x04; // VP8X: XMP present
    out.extend_from_slice(b"XMP ");
    out.extend_from_slice(&(xmp.len() as u32).to_le_bytes());
    out.extend_from_slice(xmp.as_bytes());
    if xmp.len() % 2 == 1 {
        out.push(0);
    }
    let riff = u32::try_from(out.len() - 8).ok()?;
    out[4..8].copy_from_slice(&riff.to_le_bytes());
    Some(out)
}

/// An ISOBMFF box in `bytes[start..end]`, its content from `body`.
struct Bmff {
    kind: [u8; 4],
    start: usize,
    body: usize,
    end: usize,
}

/// The boxes in `bytes[from..to]`, or `None` when they don't add up.
fn boxes(bytes: &[u8], from: usize, to: usize) -> Option<Vec<Bmff>> {
    let mut out = Vec::new();
    let mut at = from;
    while at < to {
        let size = u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?) as usize;
        let kind: [u8; 4] = bytes.get(at + 4..at + 8)?.try_into().ok()?;
        let (body, end) = match size {
            0 => (at + 8, to),
            1 => {
                let large = u64::from_be_bytes(bytes.get(at + 8..at + 16)?.try_into().ok()?);
                (at + 16, at.checked_add(usize::try_from(large).ok()?)?)
            }
            n => (at + 8, at.checked_add(n)?),
        };
        if end > to || end < body {
            return None;
        }
        out.push(Bmff {
            kind,
            start: at,
            body,
            end,
        });
        at = end;
    }
    Some(out)
}

fn read_be(bytes: &[u8], at: &mut usize, n: usize) -> Option<u64> {
    let v = bytes
        .get(*at..*at + n)?
        .iter()
        .fold(0u64, |v, &b| v << 8 | u64::from(b));
    *at += n;
    Some(v)
}

fn push_be(out: &mut Vec<u8>, v: u64, n: usize) -> Option<()> {
    if n < 8 && v >> (8 * n) != 0 {
        return None;
    }
    out.extend_from_slice(&v.to_be_bytes()[8 - n..]);
    Some(())
}

/// A box of `kind` around `body`; `full` adds the version and flags.
fn make_box(kind: &[u8; 4], full: Option<[u8; 4]>, body: &[u8]) -> Option<Vec<u8>> {
    let len = 8 + full.map_or(0, |_| 4) + body.len();
    let mut out = u32::try_from(len).ok()?.to_be_bytes().to_vec();
    out.extend_from_slice(kind);
    if let Some(vf) = full {
        out.extend_from_slice(&vf);
    }
    out.extend_from_slice(body);
    Some(out)
}

/// `iloc` with `new` (an item id and where its one extent is) added, and
/// the file offsets at or past `moved_from` moved by `delta`.
fn grown_iloc(b: &[u8], moved_from: u64, delta: u64, new: (u64, u64, u64)) -> Option<Vec<u8>> {
    let version = *b.first()?;
    if version > 2 {
        return None;
    }
    let mut at = 4;
    let sizes = read_be(b, &mut at, 2)?;
    let (offset_size, length_size) = ((sizes >> 12) as usize, (sizes >> 8 & 15) as usize);
    let (base_size, index_size) = ((sizes >> 4 & 15) as usize, (sizes & 15) as usize);
    let index_size = if version == 0 { 0 } else { index_size };
    let valid = |n: usize| matches!(n, 0 | 4 | 8);
    if offset_size == 0
        || length_size == 0
        || ![offset_size, length_size, base_size, index_size]
            .into_iter()
            .all(valid)
    {
        return None;
    }
    let id_size = if version == 2 { 4 } else { 2 };
    let count = read_be(b, &mut at, id_size)?;
    let mut out = b[..4].to_vec();
    push_be(&mut out, sizes, 2)?;
    push_be(&mut out, count + 1, id_size)?;
    let shift = |v: u64| if v >= moved_from { v + delta } else { v };
    for _ in 0..count {
        let id = read_be(b, &mut at, id_size)?;
        push_be(&mut out, id, id_size)?;
        let method = if version == 0 {
            0
        } else {
            let m = read_be(b, &mut at, 2)?;
            push_be(&mut out, m, 2)?;
            m & 15
        };
        let data_ref = read_be(b, &mut at, 2)?;
        push_be(&mut out, data_ref, 2)?;
        // Offsets into this file: construction method 0, no other file.
        let in_file = method == 0 && data_ref == 0;
        let base = read_be(b, &mut at, base_size)?;
        let shift_base = in_file && base_size > 0 && base != 0;
        push_be(
            &mut out,
            if shift_base { shift(base) } else { base },
            base_size,
        )?;
        let extents = read_be(b, &mut at, 2)?;
        push_be(&mut out, extents, 2)?;
        for _ in 0..extents {
            let index = read_be(b, &mut at, index_size)?;
            push_be(&mut out, index, index_size)?;
            let offset = read_be(b, &mut at, offset_size)?;
            let offset = if in_file && !shift_base {
                shift(offset)
            } else {
                offset
            };
            push_be(&mut out, offset, offset_size)?;
            let length = read_be(b, &mut at, length_size)?;
            push_be(&mut out, length, length_size)?;
        }
    }
    let (id, offset, length) = new;
    push_be(&mut out, id, id_size)?;
    if version > 0 {
        push_be(&mut out, 0, 2)?;
    }
    push_be(&mut out, 0, 2)?;
    push_be(&mut out, 0, base_size)?;
    push_be(&mut out, 1, 2)?;
    push_be(&mut out, 0, index_size)?;
    push_be(&mut out, offset, offset_size)?;
    push_be(&mut out, length, length_size)?;
    Some(out)
}

fn embed_avif(bytes: &[u8], xmp: &str) -> Option<Vec<u8>> {
    let top = boxes(bytes, 0, bytes.len())?;
    if top.first()?.kind != *b"ftyp" {
        return None;
    }
    let meta = top.iter().find(|b| &b.kind == b"meta")?;
    if meta.body != meta.start + 8 {
        return None;
    }
    let meta_vf: [u8; 4] = bytes.get(meta.body..meta.body + 4)?.try_into().ok()?;
    let children = boxes(bytes, meta.body + 4, meta.end)?;
    let child = |kind: &[u8; 4]| children.iter().find(|c| &c.kind == kind);
    let body = |c: &Bmff| &bytes[c.body..c.end];

    let pitm = body(child(b"pitm")?);
    let mut at = 4;
    let primary = read_be(pitm, &mut at, if *pitm.first()? == 0 { 2 } else { 4 })?;

    // The item ids in use, from `iinf`.
    let iinf = child(b"iinf")?;
    let iinf_body = body(iinf);
    let mut at = 4;
    let count_size = if *iinf_body.first()? == 0 { 2 } else { 4 };
    let count = read_be(iinf_body, &mut at, count_size)?;
    let entries = boxes(bytes, iinf.body + at, iinf.end)?;
    if entries.len() as u64 != count {
        return None;
    }
    let mut last = primary;
    for e in &entries {
        let b = body(e);
        let mut at = 4;
        let id = match b.first()? {
            2 => read_be(b, &mut at, 2)?,
            3 => read_be(b, &mut at, 4)?,
            _ => continue,
        };
        last = last.max(id);
    }
    let id = last + 1;
    if id > u64::from(u16::MAX) {
        return None;
    }
    let mut infe = Vec::new();
    push_be(&mut infe, id, 2)?;
    infe.extend_from_slice(&[0, 0]);
    infe.extend_from_slice(b"mime\0application/rdf+xml\0");
    let mut new_iinf = iinf_body[..4].to_vec();
    push_be(&mut new_iinf, count + 1, count_size)?;
    new_iinf.extend_from_slice(bytes.get(iinf.body + 4 + count_size..iinf.end)?);
    new_iinf.extend(make_box(b"infe", Some([2, 0, 0, 0]), &infe)?);
    let new_iinf = make_box(b"iinf", None, &new_iinf)?;

    // `cdsc`: the XMP describes the primary image.
    let iref = child(b"iref");
    let wide = iref.is_some_and(|r| body(r).first() == Some(&1));
    let id_size = if wide { 4 } else { 2 };
    let mut cdsc = Vec::new();
    push_be(&mut cdsc, id, id_size)?;
    push_be(&mut cdsc, 1, 2)?;
    push_be(&mut cdsc, primary, id_size)?;
    let cdsc = make_box(b"cdsc", None, &cdsc)?;
    let new_iref = match iref {
        Some(r) => {
            let mut b = body(r).to_vec();
            b.extend_from_slice(&cdsc);
            make_box(b"iref", None, &b)?
        }
        None => make_box(b"iref", Some([0; 4]), &cdsc)?,
    };

    let iloc = child(b"iloc")?;
    let build = |xmp_at: u64, delta: u64| -> Option<Vec<u8>> {
        let mut inner = meta_vf.to_vec();
        for c in &children {
            match &c.kind {
                b"iinf" => {
                    inner.extend_from_slice(&new_iinf);
                    if iref.is_none() {
                        inner.extend_from_slice(&new_iref);
                    }
                }
                b"iref" => inner.extend_from_slice(&new_iref),
                b"iloc" => {
                    let grown = grown_iloc(
                        body(iloc),
                        meta.end as u64,
                        delta,
                        (id, xmp_at, xmp.len() as u64),
                    )?;
                    inner.extend(make_box(b"iloc", None, &grown)?);
                }
                _ => inner.extend_from_slice(&bytes[c.start..c.end]),
            }
        }
        make_box(b"meta", None, &inner)
    };
    // The sizes don't depend on the offsets: once to measure, once for real.
    let delta = (build(0, 0)?.len() - (meta.end - meta.start)) as u64;
    let xmp_at = (bytes.len() as u64) + delta + 8;
    let new_meta = build(xmp_at, delta)?;
    let mut out = Vec::with_capacity(bytes.len() + new_meta.len() + xmp.len() + 8);
    out.extend_from_slice(&bytes[..meta.start]);
    out.extend_from_slice(&new_meta);
    out.extend_from_slice(&bytes[meta.end..]);
    out.extend(make_box(b"mdat", None, xmp.as_bytes())?);
    Some(out)
}

/// `bytes` (a finished file) with the rating and keywords added, or unchanged
/// when there are none or the format can't carry them yet.
pub fn embed(format: Format, bytes: Vec<u8>, tags: &Tags) -> Vec<u8> {
    let Some(xmp) = xmp_packet(tags) else {
        return bytes;
    };
    let done = match format {
        Format::Jpeg => embed_jpeg(&bytes, Some(&xmp), tags),
        Format::Png => embed_png(&bytes, &xmp),
        Format::Tiff => embed_tiff(&bytes, &xmp),
        Format::Webp => embed_webp(&bytes, &xmp),
        Format::Avif => embed_avif(&bytes, &xmp),
        Format::Exr | Format::Jxl => None,
    };
    done.unwrap_or(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn tags() -> Tags {
        Tags {
            rating: Some(4),
            keywords: vec!["zee".into(), "Strand & duin".into()],
            label: Some("Green".into()),
            title: Some("Duinen".into()),
            creator: Some("Ann; Bo".into()),
            ..Default::default()
        }
    }

    #[test]
    fn packet_and_iptc() {
        let p = xmp_packet(&tags()).unwrap();
        assert!(
            p.contains(r#"xmp:Rating="4""#) && p.contains("<rdf:li>Strand &amp; duin</rdf:li>")
        );
        assert!(p.contains(r#"xmp:Label="Green""#), "{p}");
        assert!(
            p.contains(r#"<dc:title><rdf:Alt><rdf:li xml:lang="x-default">Duinen</rdf:li>"#),
            "{p}"
        );
        assert!(
            p.contains("<dc:creator><rdf:Seq><rdf:li>Ann</rdf:li><rdf:li>Bo</rdf:li></rdf:Seq>"),
            "{p}"
        );
        assert!(xmp_packet(&Tags::default()).is_none());
        let iptc = iptc_app13(&["x".repeat(100)]).unwrap();
        assert!(iptc.starts_with(b"Photoshop 3.0\0"));
        assert!(
            iptc.windows(2).any(|w| w == [0x00, 64]),
            "keywords are cut to 64 bytes"
        );
    }

    fn avif(exif: bool) -> Vec<u8> {
        use image::ImageEncoder;
        let mut buf = Vec::new();
        let mut enc = image::codecs::avif::AvifEncoder::new_with_speed_quality(&mut buf, 10, 80);
        if exif {
            let block = crate::exif::build_with_tags(
                &Default::default(),
                crate::exif::MetadataPolicy::All,
                &tags(),
            )
            .unwrap();
            enc.set_exif_metadata(block).unwrap();
        }
        let rgb: Vec<u8> = (0..4 * 4 * 3).map(|i| (i * 13) as u8).collect();
        enc.write_image(&rgb, 4, 4, image::ExtendedColorType::Rgb8)
            .unwrap();
        buf
    }

    /// Each item's data, read back through `iloc`, and the `meta` children.
    fn items(bytes: &[u8]) -> (BTreeMap<u64, Vec<u8>>, Vec<[u8; 4]>) {
        let top = boxes(bytes, 0, bytes.len()).unwrap();
        let meta = top.iter().find(|b| &b.kind == b"meta").unwrap();
        let children = boxes(bytes, meta.body + 4, meta.end).unwrap();
        let iloc = children.iter().find(|c| &c.kind == b"iloc").unwrap();
        let b = &bytes[iloc.body..iloc.end];
        assert_eq!((b[0], b[4], b[5]), (0, 0x44, 0), "iloc v0, 4-byte offsets");
        let mut at = 6;
        let mut out = BTreeMap::new();
        for _ in 0..read_be(b, &mut at, 2).unwrap() {
            let id = read_be(b, &mut at, 2).unwrap();
            at += 2;
            let mut data = Vec::new();
            for _ in 0..read_be(b, &mut at, 2).unwrap() {
                let off = read_be(b, &mut at, 4).unwrap() as usize;
                let len = read_be(b, &mut at, 4).unwrap() as usize;
                data.extend_from_slice(&bytes[off..off + len]);
            }
            out.insert(id, data);
        }
        (out, children.iter().map(|c| c.kind).collect())
    }

    #[test]
    fn avif_gets_an_xmp_item() {
        let xmp = xmp_packet(&tags()).unwrap();
        for exif in [false, true] {
            let before = avif(exif);
            let after = embed(Format::Avif, before.clone(), &tags());
            let (old, _) = items(&before);
            let (new, kinds) = items(&after);
            // The image (and the EXIF) still read the same, and the XMP is there.
            for (id, data) in &old {
                assert_eq!(&new[id], data, "item {id}");
            }
            let added = new.keys().max().unwrap();
            assert_eq!(new[added], xmp.as_bytes());
            assert!(kinds.contains(b"iref"), "{kinds:?}");
            let text = String::from_utf8_lossy(&after);
            assert!(text.contains("mime\0application/rdf+xml\0"));
            assert!(text.contains("cdsc"));
        }
        // Damaged files are left as they are, never a panic.
        let good = avif(true);
        let mut seed = 0x2545_f491u64;
        for _ in 0..2000 {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let mut bad = good.clone();
            let i = (seed % bad.len() as u64) as usize;
            bad[i] = (seed >> 32) as u8;
            if seed % 5 == 0 {
                bad.truncate(i);
            }
            let _ = embed(Format::Avif, bad, &tags());
        }
    }

    #[test]
    fn crc_matches_the_png_spec() {
        assert_eq!(crc32(b"IEND"), 0xAE42_6082);
    }
}
