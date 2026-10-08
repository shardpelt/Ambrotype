//! A small EXIF writer: IFD0, the Exif IFD and the GPS IFD as a little-endian
//! TIFF structure (what JPEG APP1, PNG eXIf, WebP and AVIF carry).

use amb_engine::raw::{PhotoMetadata, Ratio};

/// Which metadata goes into an export (Lightroom's Export → Metadata choices).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetadataPolicy {
    /// Camera, lens, exposure, date, creator, copyright and location.
    All,
    /// Everything except GPS location.
    AllExceptLocation,
    /// Copyright and creator only.
    CopyrightOnly,
    /// No EXIF block at all.
    None,
}

impl MetadataPolicy {
    pub fn from_id(id: &str) -> Option<MetadataPolicy> {
        Some(match id.to_ascii_lowercase().as_str() {
            "all" => MetadataPolicy::All,
            "no-location" | "all-except-location" => MetadataPolicy::AllExceptLocation,
            "copyright" | "copyright-only" => MetadataPolicy::CopyrightOnly,
            "none" => MetadataPolicy::None,
            _ => return None,
        })
    }
}

const ASCII: u16 = 2;
const SHORT: u16 = 3;
const LONG: u16 = 4;
const RATIONAL: u16 = 5;
const UNDEFINED: u16 = 7;
const SRATIONAL: u16 = 10;
const BYTE: u16 = 1;

struct Entry {
    tag: u16,
    kind: u16,
    count: u32,
    data: Vec<u8>,
}

fn ascii(tag: u16, s: &str) -> Entry {
    let s = s.replace('©', "(c)");
    let mut data: Vec<u8> = s.bytes().filter(|b| b.is_ascii() && *b != 0).collect();
    data.push(0);
    Entry {
        tag,
        kind: ASCII,
        count: data.len() as u32,
        data,
    }
}

fn short(tag: u16, v: u16) -> Entry {
    Entry {
        tag,
        kind: SHORT,
        count: 1,
        data: v.to_le_bytes().to_vec(),
    }
}

fn long(tag: u16, v: u32) -> Entry {
    Entry {
        tag,
        kind: LONG,
        count: 1,
        data: v.to_le_bytes().to_vec(),
    }
}

fn rationals(tag: u16, values: &[Ratio]) -> Entry {
    let data = values
        .iter()
        .flat_map(|(n, d)| n.to_le_bytes().into_iter().chain(d.to_le_bytes()))
        .collect();
    Entry {
        tag,
        kind: RATIONAL,
        count: values.len() as u32,
        data,
    }
}

/// Size of an IFD with its out-of-line values.
fn ifd_len(entries: &[Entry]) -> usize {
    let values: usize = entries
        .iter()
        .filter(|e| e.data.len() > 4)
        .map(|e| e.data.len().next_multiple_of(2))
        .sum();
    2 + 12 * entries.len() + 4 + values
}

/// Serialise one IFD that starts at `offset` in the TIFF stream.
fn write_ifd(out: &mut Vec<u8>, entries: &mut [Entry], offset: usize) {
    entries.sort_by_key(|e| e.tag);
    debug_assert_eq!(out.len(), offset);
    let mut value_at = offset + 2 + 12 * entries.len() + 4;
    let mut values = Vec::new();
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    for e in entries.iter() {
        out.extend_from_slice(&e.tag.to_le_bytes());
        out.extend_from_slice(&e.kind.to_le_bytes());
        out.extend_from_slice(&e.count.to_le_bytes());
        if e.data.len() <= 4 {
            let mut inline = e.data.clone();
            inline.resize(4, 0);
            out.extend_from_slice(&inline);
        } else {
            out.extend_from_slice(&(value_at as u32).to_le_bytes());
            values.extend_from_slice(&e.data);
            if e.data.len() % 2 == 1 {
                values.push(0);
            }
            value_at += e.data.len().next_multiple_of(2);
        }
    }
    out.extend_from_slice(&0u32.to_le_bytes()); // no next IFD
    out.extend_from_slice(&values);
}

/// The photo's rating and keywords, written with the full metadata policies:
/// EXIF Rating / RatingPercent and XPKeywords (read by Windows and many
/// photo apps; Lightroom reads them from the XMP sidecar instead).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Tags {
    pub rating: Option<u8>,
    pub keywords: Vec<String>,
    /// Colour label ("Red", ...): XMP only (`xmp:Label`), as Lightroom exports it.
    pub label: Option<String>,
}

/// The EXIF TIFF block for an export (orientation is always 1: exports are
/// rotated upright). `None` when the policy writes no metadata.
pub fn build(meta: &PhotoMetadata, policy: MetadataPolicy) -> Option<Vec<u8>> {
    build_with_tags(meta, policy, &Tags::default())
}

/// [`build`], with the photo's rating and keywords.
pub fn build_with_tags(
    meta: &PhotoMetadata,
    policy: MetadataPolicy,
    tags: &Tags,
) -> Option<Vec<u8>> {
    if policy == MetadataPolicy::None {
        return None;
    }
    let full = matches!(
        policy,
        MetadataPolicy::All | MetadataPolicy::AllExceptLocation
    );
    let mut ifd0 = vec![short(0x0112, 1), ascii(0x0131, "Ambrotype")];
    if let Some(a) = &meta.artist {
        ifd0.push(ascii(0x013B, a));
    }
    if let Some(c) = &meta.copyright {
        ifd0.push(ascii(0x8298, c));
    }
    let mut exif = Vec::new();
    let mut gps = Vec::new();
    if full {
        if let Some(r) = tags.rating.filter(|r| (1..=5).contains(r)) {
            ifd0.push(short(0x4746, u16::from(r)));
            ifd0.push(short(0x4749, [1, 25, 50, 75, 99][usize::from(r) - 1]));
        }
        if !tags.keywords.is_empty() {
            // XPKeywords: UTF-16LE, ";"-separated, NUL-terminated, as BYTEs.
            let text = tags.keywords.join(";");
            let data: Vec<u8> = text
                .encode_utf16()
                .chain([0])
                .flat_map(u16::to_le_bytes)
                .collect();
            ifd0.push(Entry {
                tag: 0x9C9E,
                kind: BYTE,
                count: data.len() as u32,
                data,
            });
        }
        if !meta.make.is_empty() {
            ifd0.push(ascii(0x010F, &meta.make));
        }
        if !meta.model.is_empty() {
            ifd0.push(ascii(0x0110, &meta.model));
        }
        exif.push(Entry {
            tag: 0x9000,
            kind: UNDEFINED,
            count: 4,
            data: b"0232".to_vec(),
        });
        if let Some(t) = meta.exposure_time {
            exif.push(rationals(0x829A, &[t]));
        }
        if let Some(f) = meta.fnumber {
            exif.push(rationals(0x829D, &[f]));
        }
        if let Some(iso) = meta.iso {
            exif.push(short(0x8827, iso.min(65535) as u16));
        }
        if let Some(d) = &meta.date_time_original {
            exif.push(ascii(0x9003, d));
        }
        if let Some(o) = &meta.offset_time_original {
            exif.push(ascii(0x9011, o));
        }
        if let Some((n, d)) = meta.exposure_bias {
            exif.push(Entry {
                tag: 0x9204,
                kind: SRATIONAL,
                count: 1,
                data: n.to_le_bytes().into_iter().chain(d.to_le_bytes()).collect(),
            });
        }
        if let Some(f) = meta.focal_length {
            exif.push(rationals(0x920A, &[f]));
        }
        if let Some(m) = &meta.lens_make {
            exif.push(ascii(0xA433, m));
        }
        if let Some(m) = &meta.lens_model {
            exif.push(ascii(0xA434, m));
        }
        if policy == MetadataPolicy::All {
            if let Some(g) = &meta.gps {
                gps.push(Entry {
                    tag: 0x0000,
                    kind: BYTE,
                    count: 4,
                    data: vec![2, 3, 0, 0],
                });
                gps.push(ascii(0x0001, &g.latitude.0));
                gps.push(rationals(0x0002, &g.latitude.1));
                gps.push(ascii(0x0003, &g.longitude.0));
                gps.push(rationals(0x0004, &g.longitude.1));
                if let Some((r, alt)) = g.altitude {
                    gps.push(Entry {
                        tag: 0x0005,
                        kind: BYTE,
                        count: 1,
                        data: vec![r],
                    });
                    gps.push(rationals(0x0006, &[alt]));
                }
            }
        }
    }
    // Layout: header, IFD0, Exif IFD, GPS IFD. Pointer entries are 4 bytes inline.
    let pointers = usize::from(!exif.is_empty()) + usize::from(!gps.is_empty());
    let ifd0_len = ifd_len(&ifd0) + 12 * pointers;
    let exif_at = 8 + ifd0_len;
    let gps_at = exif_at + if exif.is_empty() { 0 } else { ifd_len(&exif) };
    if !exif.is_empty() {
        ifd0.push(long(0x8769, exif_at as u32));
    }
    if !gps.is_empty() {
        ifd0.push(long(0x8825, gps_at as u32));
    }
    let mut out = b"II*\0".to_vec();
    out.extend_from_slice(&8u32.to_le_bytes());
    write_ifd(&mut out, &mut ifd0, 8);
    if !exif.is_empty() {
        write_ifd(&mut out, &mut exif, exif_at);
    }
    if !gps.is_empty() {
        write_ifd(&mut out, &mut gps, gps_at);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use amb_engine::raw::Gps;

    fn meta() -> PhotoMetadata {
        PhotoMetadata {
            make: "Nikon".into(),
            model: "D750".into(),
            lens_model: Some("SP 35mm f/1.8".into()),
            exposure_time: Some((1, 250)),
            fnumber: Some((18, 10)),
            focal_length: Some((350, 10)),
            iso: Some(400),
            date_time_original: Some("2026:10:05 12:00:00".into()),
            copyright: Some("© Someone".into()),
            orientation: 6,
            gps: Some(Gps {
                latitude: ("N".into(), [(52, 1), (22, 1), (0, 1)]),
                longitude: ("E".into(), [(4, 1), (54, 1), (0, 1)]),
                altitude: Some((0, (10, 1))),
            }),
            ..Default::default()
        }
    }

    /// Minimal reader: tag → (type, count, raw value bytes) for the IFD at `at`.
    fn read_ifd(t: &[u8], at: usize) -> Vec<(u16, u16, u32, Vec<u8>)> {
        let n = u16::from_le_bytes([t[at], t[at + 1]]) as usize;
        (0..n)
            .map(|i| {
                let e = at + 2 + 12 * i;
                let tag = u16::from_le_bytes([t[e], t[e + 1]]);
                let kind = u16::from_le_bytes([t[e + 2], t[e + 3]]);
                let count = u32::from_le_bytes(t[e + 4..e + 8].try_into().unwrap());
                let size = count as usize
                    * match kind {
                        1 | 2 | 7 => 1,
                        3 => 2,
                        4 => 4,
                        _ => 8,
                    };
                let data = if size <= 4 {
                    t[e + 8..e + 8 + size].to_vec()
                } else {
                    let off = u32::from_le_bytes(t[e + 8..e + 12].try_into().unwrap()) as usize;
                    t[off..off + size].to_vec()
                };
                (tag, kind, count, data)
            })
            .collect()
    }

    fn find(ifd: &[(u16, u16, u32, Vec<u8>)], tag: u16) -> Option<Vec<u8>> {
        ifd.iter().find(|e| e.0 == tag).map(|e| e.3.clone())
    }

    #[test]
    fn rating_and_keywords() {
        let tags = Tags {
            rating: Some(4),
            keywords: vec!["sea".into(), "zon".into()],
            label: None,
        };
        let t = build_with_tags(&meta(), MetadataPolicy::AllExceptLocation, &tags).unwrap();
        let ifd0 = read_ifd(&t, 8);
        assert!(ifd0.windows(2).all(|w| w[0].0 < w[1].0), "tags sorted");
        assert_eq!(find(&ifd0, 0x4746).unwrap(), 4u16.to_le_bytes());
        assert_eq!(find(&ifd0, 0x4749).unwrap(), 75u16.to_le_bytes());
        let kw = find(&ifd0, 0x9C9E).unwrap();
        let units: Vec<u16> = kw
            .chunks(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        assert_eq!(
            String::from_utf16(&units[..units.len() - 1]).unwrap(),
            "sea;zon"
        );
        // "Copyright only" leaves them out.
        let t = build_with_tags(&meta(), MetadataPolicy::CopyrightOnly, &tags).unwrap();
        assert!(find(&read_ifd(&t, 8), 0x4746).is_none());
    }

    #[test]
    fn writes_a_readable_exif_block() {
        let t = build(&meta(), MetadataPolicy::All).unwrap();
        assert_eq!(&t[..4], b"II*\0");
        let ifd0 = read_ifd(&t, 8);
        assert!(ifd0.windows(2).all(|w| w[0].0 < w[1].0), "tags sorted");
        assert_eq!(find(&ifd0, 0x010F).unwrap(), b"Nikon\0");
        assert_eq!(find(&ifd0, 0x0112).unwrap(), 1u16.to_le_bytes());
        // EXIF text is ASCII: © becomes (c).
        assert_eq!(find(&ifd0, 0x8298).unwrap(), b"(c) Someone\0");
        let exif_at = u32::from_le_bytes(find(&ifd0, 0x8769).unwrap().try_into().unwrap());
        let exif = read_ifd(&t, exif_at as usize);
        let et = find(&exif, 0x829A).unwrap();
        assert_eq!(et, [1u32.to_le_bytes(), 250u32.to_le_bytes()].concat());
        assert_eq!(find(&exif, 0x8827).unwrap(), 400u16.to_le_bytes());
        assert_eq!(find(&exif, 0xA434).unwrap(), b"SP 35mm f/1.8\0");
        let gps_at = u32::from_le_bytes(find(&ifd0, 0x8825).unwrap().try_into().unwrap());
        let gps = read_ifd(&t, gps_at as usize);
        assert_eq!(find(&gps, 0x0001).unwrap(), b"N\0");
        assert_eq!(find(&gps, 0x0002).unwrap().len(), 24);
    }

    #[test]
    fn policies_limit_what_is_written() {
        let no_loc = build(&meta(), MetadataPolicy::AllExceptLocation).unwrap();
        assert!(find(&read_ifd(&no_loc, 8), 0x8825).is_none());
        let copyright = build(&meta(), MetadataPolicy::CopyrightOnly).unwrap();
        let ifd0 = read_ifd(&copyright, 8);
        assert!(find(&ifd0, 0x8298).is_some());
        assert!(find(&ifd0, 0x010F).is_none() && find(&ifd0, 0x8769).is_none());
        assert!(build(&meta(), MetadataPolicy::None).is_none());
    }
}
