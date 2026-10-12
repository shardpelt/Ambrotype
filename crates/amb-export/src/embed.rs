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
//! - JPEG XL: an `xml ` box, the codestream moved into a container first
//!   (its EXIF, an `Exif` box, is added the same way when encoding).
//! - OpenEXR: not yet.

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
    tiff_with_entries(bytes, &[(700, 1, xmp.as_bytes())])
}

/// A little-endian classic TIFF (what our encoder writes) with `extra`
/// entries (tag, type, value) in its first directory, replacing any of the
/// same tag. The directory is copied to the end of the file, with each of
/// its values that sit on an odd offset, and the header points at the copy;
/// the old directory stays where it is, unused.
pub(crate) fn tiff_with_entries(bytes: &[u8], extra: &[(u16, u16, &[u8])]) -> Option<Vec<u8>> {
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
    entries.retain(|e| {
        let tag = u16::from_le_bytes([e[0], e[1]]);
        !extra.iter().any(|(t, _, _)| *t == tag)
    });
    let mut out = bytes.to_vec();
    let append = |out: &mut Vec<u8>, data: &[u8]| -> Option<u32> {
        if out.len() % 2 == 1 {
            out.push(0);
        }
        let at = u32::try_from(out.len()).ok()?;
        out.extend_from_slice(data);
        Some(at)
    };
    // Values on odd offsets move.
    for e in &mut entries {
        let kind = u16::from_le_bytes([e[2], e[3]]);
        let size = match kind {
            1 | 2 | 6 | 7 => 1,
            3 | 8 => 2,
            4 | 9 | 11 | 13 => 4,
            5 | 10 | 12 => 8,
            _ => continue,
        };
        let len = (u32::from_le_bytes(e[4..8].try_into().ok()?) as usize).checked_mul(size)?;
        let at = u32::from_le_bytes(e[8..12].try_into().ok()?) as usize;
        if len > 4 && at % 2 == 1 {
            let value = bytes.get(at..at.checked_add(len)?)?;
            let new = append(&mut out, value)?;
            e[8..12].copy_from_slice(&new.to_le_bytes());
        }
    }
    for (tag, kind, data) in extra {
        let mut entry = [0u8; 12];
        entry[..2].copy_from_slice(&tag.to_le_bytes());
        entry[2..4].copy_from_slice(&kind.to_le_bytes());
        entry[4..8].copy_from_slice(&u32::try_from(data.len()).ok()?.to_le_bytes());
        if data.len() <= 4 {
            entry[8..8 + data.len()].copy_from_slice(data);
        } else {
            entry[8..12].copy_from_slice(&append(&mut out, data)?.to_le_bytes());
        }
        entries.push(entry);
    }
    entries.sort_by_key(|e| u16::from_le_bytes([e[0], e[1]]));
    let new_ifd = append(&mut out, &[])?;
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

/// `iloc` with `new` (an item id and where its one extent is) added, if
/// given, and the file offsets at or past `moved_from` moved by `delta`.
fn grown_iloc(
    b: &[u8],
    moved_from: u64,
    delta: u64,
    new: Option<(u64, u64, u64)>,
) -> Option<Vec<u8>> {
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
    push_be(&mut out, count + u64::from(new.is_some()), id_size)?;
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
    if let Some((id, offset, length)) = new {
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
    }
    Some(out)
}

/// An AVIF's `meta` box and what is in it.
struct Meta<'a> {
    bytes: &'a [u8],
    meta: Bmff,
    children: Vec<Bmff>,
    /// The primary item (`pitm`).
    primary: u64,
}

impl<'a> Meta<'a> {
    fn read(bytes: &'a [u8]) -> Option<Meta<'a>> {
        let top = boxes(bytes, 0, bytes.len())?;
        if top.first()?.kind != *b"ftyp" {
            return None;
        }
        let meta = top.into_iter().find(|b| &b.kind == b"meta")?;
        if meta.body != meta.start + 8 || meta.end < meta.body + 4 {
            return None;
        }
        let children = boxes(bytes, meta.body + 4, meta.end)?;
        let mut m = Meta {
            bytes,
            meta,
            children,
            primary: 0,
        };
        let pitm = m.body(m.child(b"pitm")?);
        let mut at = 4;
        m.primary = read_be(pitm, &mut at, if *pitm.first()? == 0 { 2 } else { 4 })?;
        Some(m)
    }

    fn child(&self, kind: &[u8; 4]) -> Option<&Bmff> {
        self.children.iter().find(|c| &c.kind == kind)
    }

    fn body(&self, b: &Bmff) -> &'a [u8] {
        &self.bytes[b.body..b.end]
    }

    /// The file with `meta` rebuilt: children `replace` gives new bytes for
    /// (whole boxes, or `None` to keep), `after_iinf` added after `iinf`,
    /// and `item` (an id and its data) added in an `mdat` at the end.
    /// Offsets in `iloc` past `meta` move with it.
    fn rebuild(
        &self,
        replace: &dyn Fn(&Bmff) -> Option<Vec<u8>>,
        after_iinf: &[u8],
        item: Option<(u64, &[u8])>,
    ) -> Option<Vec<u8>> {
        let (bytes, meta) = (self.bytes, &self.meta);
        let build = |item_at: u64, delta: u64| -> Option<Vec<u8>> {
            let mut inner = bytes[meta.body..meta.body + 4].to_vec();
            for c in &self.children {
                if &c.kind == b"iloc" {
                    let new = item.map(|(id, data)| (id, item_at, data.len() as u64));
                    let grown = grown_iloc(self.body(c), meta.end as u64, delta, new)?;
                    inner.extend(make_box(b"iloc", None, &grown)?);
                } else {
                    match replace(c) {
                        Some(b) => inner.extend(b),
                        None => inner.extend_from_slice(&bytes[c.start..c.end]),
                    }
                }
                if &c.kind == b"iinf" {
                    inner.extend_from_slice(after_iinf);
                }
            }
            make_box(b"meta", None, &inner)
        };
        // The sizes don't depend on the offsets: once to measure, once for real.
        let delta = (build(0, 0)?.len() - (meta.end - meta.start)) as u64;
        let item_at = (bytes.len() as u64) + delta + 8;
        let new_meta = build(item_at, delta)?;
        let mut out = Vec::with_capacity(bytes.len() + new_meta.len() + 64);
        out.extend_from_slice(&bytes[..meta.start]);
        out.extend_from_slice(&new_meta);
        out.extend_from_slice(&bytes[meta.end..]);
        if let Some((_, data)) = item {
            out.extend(make_box(b"mdat", None, data)?);
        }
        Some(out)
    }
}

fn embed_avif(bytes: &[u8], xmp: &str) -> Option<Vec<u8>> {
    let m = Meta::read(bytes)?;
    let primary = m.primary;

    // The item ids in use, from `iinf`.
    let iinf = m.child(b"iinf")?;
    let iinf_body = m.body(iinf);
    let mut at = 4;
    let count_size = if *iinf_body.first()? == 0 { 2 } else { 4 };
    let count = read_be(iinf_body, &mut at, count_size)?;
    let entries = boxes(bytes, iinf.body + at, iinf.end)?;
    if entries.len() as u64 != count {
        return None;
    }
    let mut last = primary;
    for e in &entries {
        let b = m.body(e);
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
    let iref = m.child(b"iref");
    let wide = iref.is_some_and(|r| m.body(r).first() == Some(&1));
    let id_size = if wide { 4 } else { 2 };
    let mut cdsc = Vec::new();
    push_be(&mut cdsc, id, id_size)?;
    push_be(&mut cdsc, 1, 2)?;
    push_be(&mut cdsc, primary, id_size)?;
    let cdsc = make_box(b"cdsc", None, &cdsc)?;
    let new_iref = match iref {
        Some(r) => {
            let mut b = m.body(r).to_vec();
            b.extend_from_slice(&cdsc);
            make_box(b"iref", None, &b)?
        }
        None => make_box(b"iref", Some([0; 4]), &cdsc)?,
    };
    m.rebuild(
        &|c| match &c.kind {
            b"iinf" => Some(new_iinf.clone()),
            b"iref" => Some(new_iref.clone()),
            _ => None,
        },
        if iref.is_none() { &new_iref } else { &[] },
        Some((id, xmp.as_bytes())),
    )
}

/// Bits, most significant first.
struct Bits<'a> {
    data: &'a [u8],
    at: usize,
}

impl Bits<'_> {
    fn read(&mut self, n: usize) -> Option<u32> {
        let mut v = 0;
        for _ in 0..n {
            let byte = *self.data.get(self.at / 8)?;
            v = v << 1 | u32::from(byte >> (7 - self.at % 8) & 1);
            self.at += 1;
        }
        Some(v)
    }
}

/// Where the colour primaries (then the transfer, 8 bits each) are in an
/// AV1 sequence header OBU's payload, in bits: `None` when it has none.
fn av1_colour_bits(seq: &[u8]) -> Option<usize> {
    let mut b = Bits { data: seq, at: 0 };
    let profile = b.read(3)?;
    let _still = b.read(1)?;
    let reduced = b.read(1)? == 1;
    if reduced {
        b.read(5)?;
    } else {
        if b.read(1)? == 1 {
            return None; // timing info: not from our encoder
        }
        let delay = b.read(1)? == 1;
        for _ in 0..=b.read(5)? {
            b.read(12)?;
            if b.read(5)? > 7 {
                b.read(1)?;
            }
            if delay && b.read(1)? == 1 {
                b.read(4)?;
            }
        }
    }
    let (wb, hb) = (b.read(4)? as usize + 1, b.read(4)? as usize + 1);
    b.read(wb)?;
    b.read(hb)?;
    if !reduced && b.read(1)? == 1 {
        b.read(7)?;
    }
    b.read(3)?;
    if !reduced {
        b.read(4)?;
        let order_hint = b.read(1)? == 1;
        if order_hint {
            b.read(2)?;
        }
        let force_screen = if b.read(1)? == 1 { 2 } else { b.read(1)? };
        if force_screen > 0 && b.read(1)? == 0 {
            b.read(1)?;
        }
        if order_hint {
            b.read(3)?;
        }
    }
    b.read(3)?;
    let high = b.read(1)? == 1;
    if profile == 2 && high {
        b.read(1)?;
    }
    if profile != 1 {
        b.read(1)?; // mono_chrome
    }
    (b.read(1)? == 1).then_some(b.at)
}

/// Write the colour primaries and transfer into every AV1 sequence header
/// among `obus` that has them, in place. Returns how many.
fn av1_set_colour(obus: &mut [u8], primaries: u8, transfer: u8) -> usize {
    let mut done = 0;
    let mut at = 0;
    while let Some(&header) = obus.get(at) {
        let kind = header >> 3 & 15;
        let mut body = at + 1 + usize::from(header >> 2 & 1);
        let size = if header >> 1 & 1 == 1 {
            let mut size = 0usize;
            for i in 0..8 {
                let Some(&byte) = obus.get(body) else {
                    return done;
                };
                body += 1;
                size |= usize::from(byte & 0x7F) << (7 * i);
                if byte & 0x80 == 0 {
                    break;
                }
            }
            size
        } else {
            obus.len().saturating_sub(body)
        };
        let Some(end) = body.checked_add(size).filter(|&e| e <= obus.len()) else {
            return done;
        };
        if kind == 1
            && let Some(bit) = av1_colour_bits(&obus[body..end])
        {
            for (i, v) in [primaries, transfer].into_iter().enumerate() {
                for j in 0..8 {
                    let pos = bit + 8 * i + j;
                    let byte = &mut obus[body + pos / 8];
                    let mask = 0x80 >> (pos % 8);
                    *byte = if v >> (7 - j) & 1 == 1 {
                        *byte | mask
                    } else {
                        *byte & !mask
                    };
                }
            }
            done += 1;
        }
        at = end;
    }
    done
}

/// Where an item's data is (offset, length), for items in this file.
fn item_extents(m: &Meta, id: u64) -> Option<Vec<(usize, usize)>> {
    let b = m.body(m.child(b"iloc")?);
    let version = *b.first()?;
    let mut at = 4;
    let sizes = read_be(b, &mut at, 2)?;
    let (offset_size, length_size) = ((sizes >> 12) as usize, (sizes >> 8 & 15) as usize);
    let (base_size, index_size) = ((sizes >> 4 & 15) as usize, (sizes & 15) as usize);
    let index_size = if version == 0 { 0 } else { index_size };
    if ![offset_size, length_size, base_size, index_size]
        .into_iter()
        .all(|n| matches!(n, 0 | 4 | 8))
    {
        return None;
    }
    let id_size = if version == 2 { 4 } else { 2 };
    for _ in 0..read_be(b, &mut at, id_size)? {
        let this = read_be(b, &mut at, id_size)?;
        let method = if version == 0 {
            0
        } else {
            read_be(b, &mut at, 2)? & 15
        };
        let data_ref = read_be(b, &mut at, 2)?;
        let base = read_be(b, &mut at, base_size)?;
        let mut out = Vec::new();
        for _ in 0..read_be(b, &mut at, 2)? {
            read_be(b, &mut at, index_size)?;
            let offset = usize::try_from(base + read_be(b, &mut at, offset_size)?).ok()?;
            let length = usize::try_from(read_be(b, &mut at, length_size)?).ok()?;
            out.push((offset, length));
        }
        if this == id {
            return (method == 0 && data_ref == 0).then_some(out);
        }
    }
    None
}

/// Name the colour space (CICP primaries and transfer) in an AVIF: in its
/// `nclx` colour box, or a new one for the primary image (BT.601 full
/// range, as the encoder writes the pixels), and in the AV1 data.
pub fn avif_set_colour(bytes: Vec<u8>, cicp: (u16, u16)) -> Option<Vec<u8>> {
    let mut out = avif_colour_box(bytes, cicp)?;
    // The AV1 sequence header says it too (the encoder writes BT.709 and
    // sRGB there), in the image's data and its `av1C` copy: readers that
    // go by it (FFmpeg) see the same.
    let (p, t) = (u8::try_from(cicp.0).ok()?, u8::try_from(cicp.1).ok()?);
    let m = Meta::read(&out)?;
    let mut ranges = item_extents(&m, m.primary)?;
    let iprp = m.child(b"iprp")?;
    let ipco = boxes(&out, iprp.body, iprp.end)?
        .into_iter()
        .find(|b| &b.kind == b"ipco")?;
    for prop in boxes(&out, ipco.body, ipco.end)? {
        if &prop.kind == b"av1C" && prop.end >= prop.body + 4 {
            ranges.push((prop.body + 4, prop.end - prop.body - 4));
        }
    }
    let mut patched = 0;
    for (start, len) in ranges {
        let end = start.checked_add(len).filter(|&e| e <= out.len())?;
        patched += av1_set_colour(&mut out[start..end], p, t);
    }
    (patched > 0).then_some(out)
}

/// [`avif_set_colour`]'s `colr` part.
fn avif_colour_box(bytes: Vec<u8>, (primaries, transfer): (u16, u16)) -> Option<Vec<u8>> {
    let m = Meta::read(&bytes)?;
    let iprp = m.child(b"iprp")?;
    let parts = boxes(&bytes, iprp.body, iprp.end)?;
    let ipco = parts.iter().find(|b| &b.kind == b"ipco")?;
    let props = boxes(&bytes, ipco.body, ipco.end)?;
    let nclx = props.iter().find(|b| {
        &b.kind == b"colr" && bytes.get(b.body..b.body + 4) == Some(b"nclx") && b.end >= b.body + 8
    });
    if let Some(colr) = nclx {
        let at = colr.body + 4;
        let mut out = bytes.clone();
        out[at..at + 2].copy_from_slice(&primaries.to_be_bytes());
        out[at + 2..at + 4].copy_from_slice(&transfer.to_be_bytes());
        return Some(out);
    }
    let mut colr = b"nclx".to_vec();
    for v in [primaries, transfer, 6] {
        colr.extend_from_slice(&v.to_be_bytes());
    }
    colr.push(0x80);
    let mut new_ipco = bytes[ipco.body..ipco.end].to_vec();
    new_ipco.extend(make_box(b"colr", None, &colr)?);
    let index = props.len() as u64 + 1;

    // Associate it with the primary image in `ipma`.
    let ipma = parts.iter().find(|b| &b.kind == b"ipma")?;
    let b = m.body(ipma);
    let (version, wide_index) = (*b.first()?, b.get(3)? & 1 == 1);
    let id_size = if version == 0 { 2 } else { 4 };
    let index_size = if wide_index { 2 } else { 1 };
    if index >= 1 << (8 * index_size - 1) {
        return None;
    }
    let mut at = 4;
    let entries = read_be(b, &mut at, 4)?;
    let mut new_ipma = b[..8].to_vec();
    let mut found = false;
    for _ in 0..entries {
        let id = read_be(b, &mut at, id_size)?;
        let n = read_be(b, &mut at, 1)?;
        let assoc = b.get(at..at + n as usize * index_size)?;
        at += assoc.len();
        push_be(&mut new_ipma, id, id_size)?;
        if id == m.primary && n < 255 {
            found = true;
            push_be(&mut new_ipma, n + 1, 1)?;
            new_ipma.extend_from_slice(assoc);
            push_be(&mut new_ipma, index, index_size)?;
        } else {
            push_be(&mut new_ipma, n, 1)?;
            new_ipma.extend_from_slice(assoc);
        }
    }
    if !found {
        return None;
    }
    let mut new_iprp = Vec::new();
    for part in &parts {
        match &part.kind {
            b"ipco" => new_iprp.extend(make_box(b"ipco", None, &new_ipco)?),
            b"ipma" => new_iprp.extend(make_box(b"ipma", None, &new_ipma)?),
            _ => new_iprp.extend_from_slice(&bytes[part.start..part.end]),
        }
    }
    let new_iprp = make_box(b"iprp", None, &new_iprp)?;
    m.rebuild(
        &|c| (&c.kind == b"iprp").then(|| new_iprp.clone()),
        &[],
        None,
    )
}

const JXL_SIGNATURE: [u8; 12] = [0, 0, 0, 12, b'J', b'X', b'L', b' ', 0x0D, 0x0A, 0x87, 0x0A];

/// A JPEG XL file with `extra` boxes (`Exif`, `xml `) before its image:
/// a bare codestream is put in a container (`jxlc`) first.
pub fn jxl_with_boxes(bytes: &[u8], extra: &[([u8; 4], &[u8])]) -> Option<Vec<u8>> {
    let mut added = Vec::new();
    for (kind, body) in extra {
        added.extend(make_box(kind, None, body)?);
    }
    if bytes.starts_with(&[0xFF, 0x0A]) {
        let mut out = JXL_SIGNATURE.to_vec();
        out.extend(make_box(b"ftyp", None, b"jxl \0\0\0\0jxl ")?);
        out.extend(added);
        out.extend(make_box(b"jxlc", None, bytes)?);
        return Some(out);
    }
    if !bytes.starts_with(&JXL_SIGNATURE) {
        return None;
    }
    let image = boxes(bytes, 0, bytes.len())?
        .into_iter()
        .find(|b| matches!(&b.kind, b"jxlc" | b"jxlp"))?;
    let mut out = bytes[..image.start].to_vec();
    out.extend(added);
    out.extend_from_slice(&bytes[image.start..]);
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
        Format::Jxl => jxl_with_boxes(&bytes, &[(*b"xml ", xmp.as_bytes())]),
        Format::Exr => None,
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
    fn avif_colour_is_named_in_the_box_and_the_av1_data() {
        let named = avif_set_colour(avif(true), (12, 13)).unwrap();
        let m = Meta::read(&named).unwrap();
        // Every sequence header (in the image, and in `av1C`) says P3 / sRGB.
        let mut ranges = item_extents(&m, m.primary).unwrap();
        let p = named.windows(4).position(|w| w == b"av1C").unwrap();
        let size = u32::from_be_bytes(named[p - 4..p].try_into().unwrap()) as usize;
        // `av1C` may carry no copy of the header.
        if size > 12 {
            ranges.push((p + 8, size - 12));
        }
        for (start, len) in ranges {
            let mut obus = named[start..start + len].to_vec();
            // Writing what is there changes nothing.
            assert_eq!(av1_set_colour(&mut obus, 12, 13), 1);
            assert_eq!(obus, &named[start..start + len]);
        }
        let image = avif(true);
        let (old, _) = items(&image);
        let (new, _) = items(&named);
        assert_eq!(old.len(), new.len());
        // Damaged files: refused or named, never a panic.
        let mut seed = 0x9e37_79b9u64;
        for _ in 0..2000 {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let mut bad = image.clone();
            let i = (seed % bad.len() as u64) as usize;
            bad[i] = (seed >> 32) as u8;
            let _ = avif_set_colour(bad, (9, 1));
        }
    }

    #[test]
    fn jxl_boxes_go_before_the_image() {
        let code = [0xFF, 0x0A, 1, 2, 3];
        let one = jxl_with_boxes(&code, &[(*b"Exif", b"\0\0\0\0II*\0")]).unwrap();
        let two = embed(Format::Jxl, one, &tags());
        let kinds: Vec<_> = boxes(&two, 0, two.len())
            .unwrap()
            .iter()
            .map(|b| b.kind)
            .collect();
        assert_eq!(kinds, [*b"JXL ", *b"ftyp", *b"Exif", *b"xml ", *b"jxlc"]);
        assert!(two.ends_with(&code));
        assert!(jxl_with_boxes(b"not jxl", &[]).is_none());
    }

    #[test]
    fn crc_matches_the_png_spec() {
        assert_eq!(crc32(b"IEND"), 0xAE42_6082);
    }
}
