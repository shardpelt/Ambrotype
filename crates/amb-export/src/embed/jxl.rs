// SPDX-License-Identifier: GPL-3.0-or-later
//! Metadata boxes in JPEG XL files.

use super::bmff::{boxes, make_box};

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
