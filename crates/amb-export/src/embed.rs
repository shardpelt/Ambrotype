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
//! - AVIF and OpenEXR: not yet (EXIF only for AVIF).

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
    if rating.is_none() && label.is_none() && keywords.is_empty() {
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
    if keywords.is_empty() {
        out.push_str("/>\n");
    } else {
        out.push_str(">\n   <dc:subject><rdf:Bag>");
        for k in &keywords {
            out.push_str(&format!("<rdf:li>{}</rdf:li>", escape(k.trim())));
        }
        out.push_str("</rdf:Bag></dc:subject>\n  </rdf:Description>\n");
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
        Format::Avif | Format::Exr => None,
    };
    done.unwrap_or(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags() -> Tags {
        Tags {
            rating: Some(4),
            keywords: vec!["zee".into(), "Strand & duin".into()],
            label: Some("Green".into()),
        }
    }

    #[test]
    fn packet_and_iptc() {
        let p = xmp_packet(&tags()).unwrap();
        assert!(
            p.contains(r#"xmp:Rating="4""#) && p.contains("<rdf:li>Strand &amp; duin</rdf:li>")
        );
        assert!(p.contains(r#"xmp:Label="Green""#), "{p}");
        assert!(xmp_packet(&Tags::default()).is_none());
        let iptc = iptc_app13(&["x".repeat(100)]).unwrap();
        assert!(iptc.starts_with(b"Photoshop 3.0\0"));
        assert!(
            iptc.windows(2).any(|w| w == [0x00, 64]),
            "keywords are cut to 64 bytes"
        );
    }

    #[test]
    fn crc_matches_the_png_spec() {
        assert_eq!(crc32(b"IEND"), 0xAE42_6082);
    }
}
