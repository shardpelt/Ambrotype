// SPDX-License-Identifier: GPL-3.0-or-later
//! Files from a zip archive, for models published only inside one (the
//! RawNIND networks, in darktable-ai's `.dtmodel`). Just enough of the zip
//! format: the central directory, stored or deflated members, no zip64.

use std::io::Read;
use std::path::{Path, PathBuf};

/// The zip a model is published in, and the model's own file's name in it.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct Archive {
    pub bytes: u64,
    pub sha256: String,
    pub urls: Vec<String>,
    pub member: String,
}

/// Take `parts` (where to, name inside, bytes, sha256) out of `zip`, each
/// checked; written beside their place first, so a failure leaves none.
pub(crate) fn unpack(zip: &Path, parts: &[(PathBuf, String, u64, String)]) -> Result<(), String> {
    use sha2::{Digest, Sha256};
    let data = std::fs::read(zip).map_err(|e| format!("{}: {e}", zip.display()))?;
    for (to, name, bytes, sha256) in parts {
        let body = member_of(&data, name)?;
        let sum: String = Sha256::digest(&body)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        if body.len() as u64 != *bytes || sum != *sha256 {
            return Err(format!("{name} isn't the expected file"));
        }
        let part = to.with_extension("part");
        std::fs::write(&part, &body).map_err(|e| e.to_string())?;
        std::fs::rename(&part, to).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn u16_at(d: &[u8], i: usize) -> Option<usize> {
    Some(u16::from_le_bytes(d.get(i..i + 2)?.try_into().ok()?) as usize)
}

fn u32_at(d: &[u8], i: usize) -> Option<usize> {
    Some(u32::from_le_bytes(d.get(i..i + 4)?.try_into().ok()?) as usize)
}

fn member_of(d: &[u8], name: &str) -> Result<Vec<u8>, String> {
    let bad = || "not a zip archive this can read".to_string();
    // The end of central directory record, searched from the end (a comment
    // may follow it).
    let end = (0..d.len().saturating_sub(21))
        .rev()
        .take(65_557)
        .find(|&i| d[i..].starts_with(b"PK\x05\x06"))
        .ok_or_else(bad)?;
    let entries = u16_at(d, end + 10).ok_or_else(bad)?;
    let mut at = u32_at(d, end + 16).ok_or_else(bad)?;
    for _ in 0..entries {
        if !d.get(at..).is_some_and(|r| r.starts_with(b"PK\x01\x02")) {
            return Err(bad());
        }
        let method = u16_at(d, at + 10).ok_or_else(bad)?;
        let packed = u32_at(d, at + 20).ok_or_else(bad)?;
        let size = u32_at(d, at + 24).ok_or_else(bad)?;
        let (n, m, k) = (
            u16_at(d, at + 28).ok_or_else(bad)?,
            u16_at(d, at + 30).ok_or_else(bad)?,
            u16_at(d, at + 32).ok_or_else(bad)?,
        );
        let local = u32_at(d, at + 42).ok_or_else(bad)?;
        let this = d.get(at + 46..at + 46 + n).ok_or_else(bad)?;
        at += 46 + n + m + k;
        if this != name.as_bytes() {
            continue;
        }
        if !d.get(local..).is_some_and(|r| r.starts_with(b"PK\x03\x04")) {
            return Err(bad());
        }
        let start = local
            + 30
            + u16_at(d, local + 26).ok_or_else(bad)?
            + u16_at(d, local + 28).ok_or_else(bad)?;
        let body = d.get(start..start + packed).ok_or_else(bad)?;
        let out = match method {
            0 => body.to_vec(),
            8 => {
                let mut out = Vec::with_capacity(size);
                flate2::read::DeflateDecoder::new(body)
                    .read_to_end(&mut out)
                    .map_err(|e| format!("{name}: {e}"))?;
                out
            }
            m => return Err(format!("{name}: packed in a way this can't read ({m})")),
        };
        if out.len() != size {
            return Err(format!("{name}: not the size the archive says"));
        }
        return Ok(out);
    }
    Err(format!("{name} isn't in the archive"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_stored_and_deflated_members() {
        // Made with Python's zipfile: a.txt stored ("hello"), b.txt
        // deflated ("hello hello hello hello").
        let zip: &[u8] = include_bytes!("../tests/two.zip");
        assert_eq!(member_of(zip, "a.txt").unwrap(), b"hello");
        assert_eq!(member_of(zip, "b.txt").unwrap(), b"hello hello hello hello");
        assert!(member_of(zip, "c.txt").unwrap_err().contains("isn't in"));
        assert!(member_of(b"not a zip at all, sorry", "a.txt").is_err());
    }
}
