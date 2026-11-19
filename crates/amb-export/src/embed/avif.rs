// SPDX-License-Identifier: GPL-3.0-or-later
//! Metadata and colour boxes in AVIF files.

use super::bmff::{Bmff, boxes, make_box, push_be, read_be};

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
pub(super) struct Meta<'a> {
    bytes: &'a [u8],
    meta: Bmff,
    children: Vec<Bmff>,
    /// The primary item (`pitm`).
    pub(super) primary: u64,
}

impl<'a> Meta<'a> {
    pub(super) fn read(bytes: &'a [u8]) -> Option<Meta<'a>> {
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

pub(super) fn embed_avif(bytes: &[u8], xmp: &str) -> Option<Vec<u8>> {
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
pub(super) fn av1_set_colour(obus: &mut [u8], primaries: u8, transfer: u8) -> usize {
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
pub(super) fn item_extents(m: &Meta, id: u64) -> Option<Vec<(usize, usize)>> {
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
