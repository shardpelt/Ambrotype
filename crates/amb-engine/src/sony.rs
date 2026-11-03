// SPDX-License-Identifier: GPL-3.0-or-later
//! What a Sony ARW carries beyond what rawler reads: the camera's own
//! lens correction, and its full-size JPEG ([`largest_jpeg`]).
//!
//! The distortion the camera corrects in its JPEGs: many E-mount zooms rely
//! on it (the camera's JPEG then shows a frame about 10 % tighter than the
//! raw).
//!
//! The raw image's IFD carries `DistortionCorrection` (0x7036, 0 when the
//! camera's setting was off) and `DistortionCorrParams` (0x7037: a knot
//! count, then that many signed values). Measured against the cameras'
//! JPEGs (an A6400 with the E PZ 18-105mm): the knots sit evenly from the
//! centre to the corner of the camera's corrected (and so cropped) frame,
//! each scaling the radius an output pixel comes from by 1 + v / 2^14. The vignetting and
//! CA tags (0x7032, 0x7035) are not used yet: our samples didn't show what
//! the camera does with them.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use amb_lens::model::MAX_KNOTS;
use amb_lens::{Correction, Distortion, Frame, Source};

const SUB_IFDS: u16 = 0x014a;
const DISTORTION_ON: u16 = 0x7036;
const DISTORTION_PARAMS: u16 = 0x7037;

/// The distortion correction an ARW's camera applied, for the developed
/// photo `width` × `height` placed `at` in the sensor image; `None` for
/// other files, or when the camera's correction was off.
pub fn correction(
    path: &Path,
    at: crate::dng_opcodes::Placement,
    width: usize,
    height: usize,
) -> Option<Correction> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    if !matches!(ext.as_str(), "arw" | "sr2" | "srf") {
        return None;
    }
    let params = distortion_params(path)?;
    let n = (params.len()).min(MAX_KNOTS);
    if n < 2 {
        return None;
    }
    let mut scale = [1.0; MAX_KNOTS];
    for (s, v) in scale.iter_mut().zip(&params) {
        *s = 1.0 + f64::from(*v) / 16384.0;
    }
    // The knots span the camera's corrected frame, which is zoomed in by
    // the corner's scale so it stays filled: r = 1 at the sensor image's
    // corner divided by that scale (our fill zoom then makes the same crop).
    let corner = scale[n - 1].max(0.5);
    let (sw, sh) = (
        at.stage.0.max(1) as f64 - 1.0,
        at.stage.1.max(1) as f64 - 1.0,
    );
    let (w, h) = ((width.max(2) - 1) as f64, (height.max(2) - 1) as f64);
    let frame = Frame::Dng {
        cx: (sw / 2.0 - at.origin.0 as f64) / w,
        cy: (sh / 2.0 - at.origin.1 as f64) / h,
        unit: (sw.hypot(sh) / 2.0 / corner).max(1.0) / w.max(h),
    };
    Some(Correction {
        source: Source::Embedded,
        description: "Built-in (Sony)".into(),
        file: None,
        distortion: Some((Distortion::Knots { n, scale }, frame)),
        tca: None,
        vignetting: None,
    })
}

/// The knots of `DistortionCorrParams` when `DistortionCorrection` is on.
fn distortion_params(path: &Path) -> Option<Vec<i16>> {
    let mut t = Tiff::open(path)?;
    let first = t.first;
    // IFD0's chain and its SubIFDs (where the raw image's IFD is).
    let mut queue = vec![first];
    let mut seen = Vec::new();
    while let Some(off) = queue.pop() {
        if off == 0 || seen.contains(&off) || seen.len() > 32 {
            continue;
        }
        seen.push(off);
        let entries = t.entries(off)?;
        let mut on = None;
        let mut params = None;
        for e in &entries {
            match e.tag {
                SUB_IFDS => queue.extend(t.values_u32(e)),
                DISTORTION_ON => on = t.values_u32(e).first().copied(),
                DISTORTION_PARAMS => params = t.values_i16(e),
                _ => {}
            }
        }
        if let (Some(on), Some(p)) = (on, params) {
            let n = usize::try_from(*p.first()?).ok()?;
            return (on != 0 && n >= 2 && p.len() > n).then(|| p[1..=n].to_vec());
        }
        if let Some(next) = t.next_ifd(off, entries.len()) {
            queue.push(next);
        }
    }
    None
}

const JPEG_OFFSET: u16 = 0x0201;
const JPEG_LENGTH: u16 = 0x0202;

/// The largest JPEG a TIFF-based raw keeps in its IFDs (or their SubIFDs),
/// as bytes. Sony's full-size camera JPEG lives there, where rawler's
/// preview (the 1616 px one) doesn't look.
pub fn largest_jpeg(path: &Path) -> Option<Vec<u8>> {
    let mut t = Tiff::open(path)?;
    let mut queue = vec![t.first];
    let mut seen = Vec::new();
    let mut best: Option<(u32, u32)> = None;
    while let Some(off) = queue.pop() {
        if off == 0 || seen.contains(&off) || seen.len() > 32 {
            continue;
        }
        seen.push(off);
        let Some(entries) = t.entries(off) else {
            continue;
        };
        let (mut at, mut len) = (None, None);
        for e in &entries {
            match e.tag {
                SUB_IFDS => queue.extend(t.values_u32(e)),
                JPEG_OFFSET => at = t.values_u32(e).first().copied(),
                JPEG_LENGTH => len = t.values_u32(e).first().copied(),
                _ => {}
            }
        }
        if let (Some(at), Some(len)) = (at, len) {
            if best.is_none_or(|(_, l)| len > l) {
                best = Some((at, len));
            }
        }
        if let Some(next) = t.next_ifd(off, entries.len()) {
            queue.push(next);
        }
    }
    let (at, len) = best?;
    // At most 64 MB, and a JPEG.
    if len > 64 << 20 {
        return None;
    }
    let b = t.read(u64::from(at), len as usize)?;
    b.starts_with(&[0xff, 0xd8]).then_some(b)
}

struct Entry {
    tag: u16,
    kind: u16,
    count: u32,
    /// The value field: the data itself when it fits in 4 bytes.
    raw: [u8; 4],
}

struct Tiff {
    f: File,
    little: bool,
    /// IFD0's offset.
    first: u32,
}

impl Tiff {
    fn open(path: &Path) -> Option<Tiff> {
        let mut f = File::open(path).ok()?;
        let mut head = [0u8; 8];
        f.read_exact(&mut head).ok()?;
        let little = match &head[..4] {
            b"II*\0" => true,
            b"MM\0*" => false,
            _ => return None,
        };
        let mut t = Tiff {
            f,
            little,
            first: 0,
        };
        t.first = t.u32_at(&head[4..8]);
        Some(t)
    }

    fn u16_at(&self, b: &[u8]) -> u16 {
        let b = [b[0], b[1]];
        if self.little {
            u16::from_le_bytes(b)
        } else {
            u16::from_be_bytes(b)
        }
    }

    fn u32_at(&self, b: &[u8]) -> u32 {
        let b = [b[0], b[1], b[2], b[3]];
        if self.little {
            u32::from_le_bytes(b)
        } else {
            u32::from_be_bytes(b)
        }
    }

    fn read(&mut self, off: u64, len: usize) -> Option<Vec<u8>> {
        self.f.seek(SeekFrom::Start(off)).ok()?;
        let mut buf = vec![0; len];
        self.f.read_exact(&mut buf).ok()?;
        Some(buf)
    }

    fn entries(&mut self, off: u32) -> Option<Vec<Entry>> {
        let head = self.read(u64::from(off), 2)?;
        let n = self.u16_at(&head);
        if n == 0 || n > 1000 {
            return None;
        }
        let b = self.read(u64::from(off) + 2, usize::from(n) * 12)?;
        Some(
            b.chunks_exact(12)
                .map(|e| Entry {
                    tag: self.u16_at(&e[0..2]),
                    kind: self.u16_at(&e[2..4]),
                    count: self.u32_at(&e[4..8]),
                    raw: [e[8], e[9], e[10], e[11]],
                })
                .collect(),
        )
    }

    fn next_ifd(&mut self, off: u32, n: usize) -> Option<u32> {
        let b = self.read(u64::from(off) + 2 + n as u64 * 12, 4)?;
        Some(self.u32_at(&b))
    }

    /// An entry's data (at most 4096 bytes).
    fn data(&mut self, e: &Entry, size: usize) -> Option<Vec<u8>> {
        let len = size.checked_mul(e.count as usize)?;
        if len > 4096 {
            return None;
        }
        if len <= 4 {
            Some(e.raw[..len].to_vec())
        } else {
            self.read(u64::from(self.u32_at(&e.raw)), len)
        }
    }

    /// SHORT, LONG or IFD values.
    fn values_u32(&mut self, e: &Entry) -> Vec<u32> {
        match e.kind {
            3 => self
                .data(e, 2)
                .map(|b| {
                    b.chunks_exact(2)
                        .map(|c| u32::from(self.u16_at(c)))
                        .collect()
                })
                .unwrap_or_default(),
            4 | 13 => self
                .data(e, 4)
                .map(|b| b.chunks_exact(4).map(|c| self.u32_at(c)).collect())
                .unwrap_or_default(),
            _ => Vec::new(),
        }
    }

    /// SSHORT values.
    fn values_i16(&mut self, e: &Entry) -> Option<Vec<i16>> {
        if e.kind != 8 {
            return None;
        }
        let b = self.data(e, 2)?;
        Some(b.chunks_exact(2).map(|c| self.u16_at(c) as i16).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A little-endian TIFF with the two tags in a SubIFD, as an ARW has.
    fn arw(on: u16, params: &[i16]) -> Vec<u8> {
        let mut b = b"II*\0".to_vec();
        b.extend(8u32.to_le_bytes());
        // IFD0 at 8: one entry, SubIFDs -> 26.
        b.extend(1u16.to_le_bytes());
        b.extend(SUB_IFDS.to_le_bytes());
        b.extend(4u16.to_le_bytes());
        b.extend(1u32.to_le_bytes());
        b.extend(26u32.to_le_bytes());
        b.extend(0u32.to_le_bytes());
        // SubIFD at 26: two entries, params at 26 + 2 + 24 + 4 = 56.
        b.extend(2u16.to_le_bytes());
        b.extend(DISTORTION_ON.to_le_bytes());
        b.extend(3u16.to_le_bytes());
        b.extend(1u32.to_le_bytes());
        b.extend(u32::from(on).to_le_bytes());
        b.extend(DISTORTION_PARAMS.to_le_bytes());
        b.extend(8u16.to_le_bytes());
        b.extend((params.len() as u32).to_le_bytes());
        b.extend(56u32.to_le_bytes());
        b.extend(0u32.to_le_bytes());
        for v in params {
            b.extend(v.to_le_bytes());
        }
        b
    }

    #[test]
    fn the_distortion_knots_are_read_and_scale_the_corner() {
        let dir = std::env::temp_dir().join(format!("amb-sony-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a.ARW");
        // A6400 with the E PZ 18-105mm at 105 mm (padded to 16 as the file is).
        let mut params = vec![11, 29, 29, 58, 144, 259, 417, 633, 891, 1222, 1639, 2156];
        params.resize(17, 0);
        std::fs::write(&path, arw(17, &params)).unwrap();
        let at = crate::dng_opcodes::Placement {
            stage: (6001, 4001),
            origin: (0, 0),
        };
        let c = correction(&path, at, 6001, 4001).unwrap();
        let (d, frame) = c.distortion.unwrap();
        let g = amb_lens::Geometry::new(
            &c,
            &amb_lens::ImageFrame {
                width: 6001,
                height: 4001,
                crop_factor: None,
            },
        );
        // The camera's corrected frame is the sensor's zoomed in by the
        // corner's scale: its corner comes from the sensor's corner.
        let z = 1.0 + 2156.0 / 16384.0;
        let (x, y) = g.distort(3000.0 + 3000.0 / z, 2000.0 + 2000.0 / z);
        assert!(
            (x - 6000.0).abs() < 1e-6 && (y - 4000.0).abs() < 1e-6,
            "{x} {y} {d:?} {frame:?}"
        );
        // The centre is scaled by its first knot.
        let (x, _) = g.distort(3001.0, 2000.0);
        assert!((x - 3000.0 - (1.0 + 29.0 / 16384.0)).abs() < 1e-6);
        // Off in the camera: nothing.
        std::fs::write(&path, arw(0, &params)).unwrap();
        assert!(correction(&path, at, 6001, 4001).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_largest_jpeg_in_the_ifds_is_found() {
        // IFD0 with a 4-byte "JPEG", then IFD1 with a 6-byte one.
        let mut b = b"II*\0".to_vec();
        b.extend(8u32.to_le_bytes());
        let ifd = |b: &mut Vec<u8>, at: u32, len: u32, next: u32| {
            b.extend(2u16.to_le_bytes());
            for (tag, v) in [(JPEG_OFFSET, at), (JPEG_LENGTH, len)] {
                b.extend(tag.to_le_bytes());
                b.extend(4u16.to_le_bytes());
                b.extend(1u32.to_le_bytes());
                b.extend(v.to_le_bytes());
            }
            b.extend(next.to_le_bytes());
        };
        // Each IFD is 2 + 24 + 4 = 30 bytes: IFD0 at 8, IFD1 at 38, data at 68.
        ifd(&mut b, 68, 4, 38);
        ifd(&mut b, 72, 6, 0);
        b.extend([0xff, 0xd8, 0xff, 0xd9]);
        b.extend([0xff, 0xd8, 1, 2, 0xff, 0xd9]);
        let dir = std::env::temp_dir().join(format!("amb-sony-jpeg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("b.ARW");
        std::fs::write(&path, &b).unwrap();
        assert_eq!(
            largest_jpeg(&path).unwrap(),
            vec![0xff, 0xd8, 1, 2, 0xff, 0xd9]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
