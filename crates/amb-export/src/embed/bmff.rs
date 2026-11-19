// SPDX-License-Identifier: GPL-3.0-or-later
//! The box format (ISO BMFF) that AVIF and JPEG XL files share.

/// An ISOBMFF box in `bytes[start..end]`, its content from `body`.
pub(super) struct Bmff {
    pub(super) kind: [u8; 4],
    pub(super) start: usize,
    pub(super) body: usize,
    pub(super) end: usize,
}

/// The boxes in `bytes[from..to]`, or `None` when they don't add up.
pub(super) fn boxes(bytes: &[u8], from: usize, to: usize) -> Option<Vec<Bmff>> {
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

pub(super) fn read_be(bytes: &[u8], at: &mut usize, n: usize) -> Option<u64> {
    let v = bytes
        .get(*at..*at + n)?
        .iter()
        .fold(0u64, |v, &b| v << 8 | u64::from(b));
    *at += n;
    Some(v)
}

pub(super) fn push_be(out: &mut Vec<u8>, v: u64, n: usize) -> Option<()> {
    if n < 8 && v >> (8 * n) != 0 {
        return None;
    }
    out.extend_from_slice(&v.to_be_bytes()[8 - n..]);
    Some(())
}

/// A box of `kind` around `body`; `full` adds the version and flags.
pub(super) fn make_box(kind: &[u8; 4], full: Option<[u8; 4]>, body: &[u8]) -> Option<Vec<u8>> {
    let len = 8 + full.map_or(0, |_| 4) + body.len();
    let mut out = u32::try_from(len).ok()?.to_be_bytes().to_vec();
    out.extend_from_slice(kind);
    if let Some(vf) = full {
        out.extend_from_slice(&vf);
    }
    out.extend_from_slice(body);
    Some(out)
}
