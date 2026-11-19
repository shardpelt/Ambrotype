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

mod avif;
mod bmff;
mod jxl;
#[cfg(test)]
mod tests;

pub use avif::avif_set_colour;
use avif::embed_avif;
pub use jxl::jxl_with_boxes;

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
pub(super) fn iptc_app13(keywords: &[String]) -> Option<Vec<u8>> {
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

pub(super) fn crc32(data: &[u8]) -> u32 {
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
