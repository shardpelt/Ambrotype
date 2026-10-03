//! Minimal DNG writer for synthetic calibration targets.
//!
//! Writes a single-IFD, uncompressed, 16-bit `LinearRaw` DNG (3 samples per
//! pixel, already demosaiced). The "camera" colour space is defined to be
//! linear ProPhoto (ROMM) RGB with a D50 white, so the colour transform Camera
//! Raw applies is known exactly:
//! - `ColorMatrix1` = XYZ(D50) → ProPhoto
//! - `ForwardMatrix1` = ProPhoto → XYZ(D50)
//! - `CalibrationIlluminant1` = D50
//! - `AsShotNeutral` = (1, 1, 1)
//!
//! A linear `ProfileToneCurve` is embedded by default, which removes Adobe's
//! default base tone curve as a confounder when fitting individual sliders.
//! Develop settings can be embedded as an XMP packet (tag 700). Lightroom reads
//! settings from inside DNGs, not from sidecars.

use std::io::Write;

/// Linear ProPhoto (ROMM) RGB → XYZ (D50).
pub const PROPHOTO_TO_XYZ_D50: [[f64; 3]; 3] = [
    [0.797_674_9, 0.135_191_7, 0.031_353_4],
    [0.288_040_2, 0.711_874_1, 0.000_085_7],
    [0.0, 0.0, 0.825_210_0],
];

/// Linear sRGB → XYZ, Bradford-adapted from D65 to D50.
pub const SRGB_TO_XYZ_D50: [[f64; 3]; 3] = [
    [0.436_074_7, 0.385_064_9, 0.143_080_4],
    [0.222_504_5, 0.716_878_6, 0.060_616_9],
    [0.013_932_2, 0.097_104_5, 0.714_173_3],
];

pub fn invert3(m: &[[f64; 3]; 3]) -> [[f64; 3]; 3] {
    let det = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
        - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
    assert!(det.abs() > 1e-12, "singular matrix");
    let inv =
        |r1: usize, c1: usize, r2: usize, c2: usize| m[r1][c1] * m[r2][c2] - m[r1][c2] * m[r2][c1];
    [
        [
            inv(1, 1, 2, 2) / det,
            -inv(0, 1, 2, 2) / det,
            inv(0, 1, 1, 2) / det,
        ],
        [
            -inv(1, 0, 2, 2) / det,
            inv(0, 0, 2, 2) / det,
            -inv(0, 0, 1, 2) / det,
        ],
        [
            inv(1, 0, 2, 1) / det,
            -inv(0, 0, 2, 1) / det,
            inv(0, 0, 1, 1) / det,
        ],
    ]
}

pub fn mul3(a: &[[f64; 3]; 3], b: &[[f64; 3]; 3]) -> [[f64; 3]; 3] {
    let mut out = [[0.0; 3]; 3];
    for (i, row) in out.iter_mut().enumerate() {
        for (j, cell) in row.iter_mut().enumerate() {
            *cell = (0..3).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    out
}

pub fn apply3(m: &[[f64; 3]; 3], v: [f64; 3]) -> [f64; 3] {
    [0, 1, 2].map(|i| m[i][0] * v[0] + m[i][1] * v[1] + m[i][2] * v[2])
}

/// A linear-light image in camera space (= linear ProPhoto), values nominally 0..1.
pub struct LinearImage {
    pub width: u32,
    pub height: u32,
    /// Interleaved RGB, row-major, `width * height * 3` values.
    pub data: Vec<f32>,
}

impl LinearImage {
    pub fn new(width: u32, height: u32) -> Self {
        LinearImage {
            width,
            height,
            data: vec![0.0; (width * height * 3) as usize],
        }
    }

    pub fn set(&mut self, x: u32, y: u32, rgb: [f64; 3]) {
        let i = ((y * self.width + x) * 3) as usize;
        for (slot, v) in self.data[i..i + 3].iter_mut().zip(rgb) {
            *slot = v as f32;
        }
    }

    pub fn get(&self, x: u32, y: u32) -> [f32; 3] {
        let i = ((y * self.width + x) * 3) as usize;
        [self.data[i], self.data[i + 1], self.data[i + 2]]
    }
}

pub struct DngOptions<'a> {
    /// XMP packet to embed (develop settings), if any.
    pub xmp: Option<&'a str>,
    /// Embed a linear `ProfileToneCurve` (removes Adobe's default base curve).
    pub linear_tone_curve: bool,
    pub description: &'a str,
}

impl Default for DngOptions<'_> {
    fn default() -> Self {
        DngOptions {
            xmp: None,
            linear_tone_curve: true,
            description: "Flightbroom synthetic target",
        }
    }
}

// TIFF field types.
const BYTE: u16 = 1;
const ASCII: u16 = 2;
const SHORT: u16 = 3;
const LONG: u16 = 4;
const RATIONAL: u16 = 5;
const SRATIONAL: u16 = 10;
const FLOAT: u16 = 11;

struct Entry {
    tag: u16,
    kind: u16,
    count: u32,
    bytes: Vec<u8>,
}

fn entry(tag: u16, kind: u16, count: u32, bytes: Vec<u8>) -> Entry {
    Entry {
        tag,
        kind,
        count,
        bytes,
    }
}

fn shorts(tag: u16, v: &[u16]) -> Entry {
    entry(
        tag,
        SHORT,
        v.len() as u32,
        v.iter().flat_map(|x| x.to_le_bytes()).collect(),
    )
}

fn longs(tag: u16, v: &[u32]) -> Entry {
    entry(
        tag,
        LONG,
        v.len() as u32,
        v.iter().flat_map(|x| x.to_le_bytes()).collect(),
    )
}

fn ascii(tag: u16, s: &str) -> Entry {
    let mut b = s.as_bytes().to_vec();
    b.push(0);
    entry(tag, ASCII, b.len() as u32, b)
}

fn srationals(tag: u16, v: &[f64]) -> Entry {
    const DEN: i32 = 1_000_000;
    let bytes = v
        .iter()
        .flat_map(|x| {
            let num = (x * f64::from(DEN)).round() as i32;
            num.to_le_bytes().into_iter().chain(DEN.to_le_bytes())
        })
        .collect();
    entry(tag, SRATIONAL, v.len() as u32, bytes)
}

fn rationals(tag: u16, v: &[f64]) -> Entry {
    const DEN: u32 = 1_000_000;
    let bytes = v
        .iter()
        .flat_map(|x| {
            let num = (x * f64::from(DEN)).round() as u32;
            num.to_le_bytes().into_iter().chain(DEN.to_le_bytes())
        })
        .collect();
    entry(tag, RATIONAL, v.len() as u32, bytes)
}

fn floats(tag: u16, v: &[f32]) -> Entry {
    entry(
        tag,
        FLOAT,
        v.len() as u32,
        v.iter().flat_map(|x| x.to_le_bytes()).collect(),
    )
}

fn flatten(m: &[[f64; 3]; 3]) -> Vec<f64> {
    m.iter().flatten().copied().collect()
}

/// Encode `image` as a DNG file.
pub fn encode(image: &LinearImage, opts: &DngOptions) -> Vec<u8> {
    let (w, h) = (image.width, image.height);
    let pixels: Vec<u8> = image
        .data
        .iter()
        .flat_map(|v| ((v.clamp(0.0, 1.0) * 65535.0).round() as u16).to_le_bytes())
        .collect();

    let xyz_to_camera = invert3(&PROPHOTO_TO_XYZ_D50);
    let mut entries = vec![
        longs(254, &[0]),                                  // NewSubFileType: main image
        longs(256, &[w]),                                  // ImageWidth
        longs(257, &[h]),                                  // ImageLength
        shorts(258, &[16, 16, 16]),                        // BitsPerSample
        shorts(259, &[1]),                                 // Compression: none
        shorts(262, &[34892]),                             // PhotometricInterpretation: LinearRaw
        ascii(270, opts.description),                      // ImageDescription
        ascii(271, "Flightbroom"),                         // Make
        ascii(272, "Synthetic"),                           // Model
        longs(273, &[0]),                                  // StripOffsets (patched below)
        shorts(274, &[1]),                                 // Orientation
        shorts(277, &[3]),                                 // SamplesPerPixel
        longs(278, &[h]),                                  // RowsPerStrip
        longs(279, &[pixels.len() as u32]),                // StripByteCounts
        shorts(284, &[1]),                                 // PlanarConfiguration: chunky
        ascii(305, "flightbroom flb-testkit"),             // Software
        entry(50706, BYTE, 4, vec![1, 4, 0, 0]),           // DNGVersion 1.4
        entry(50707, BYTE, 4, vec![1, 1, 0, 0]),           // DNGBackwardVersion 1.1
        ascii(50708, "Flightbroom Synthetic"),             // UniqueCameraModel
        longs(50717, &[65535]),                            // WhiteLevel
        srationals(50721, &flatten(&xyz_to_camera)),       // ColorMatrix1
        rationals(50728, &[1.0, 1.0, 1.0]),                // AsShotNeutral
        srationals(50730, &[0.0]),                         // BaselineExposure
        shorts(50778, &[23]),                              // CalibrationIlluminant1: D50
        ascii(50936, "Flightbroom Linear ProPhoto"),       // ProfileName
        longs(50941, &[3]),                                // ProfileEmbedPolicy: no restrictions
        srationals(50964, &flatten(&PROPHOTO_TO_XYZ_D50)), // ForwardMatrix1
    ];
    if opts.linear_tone_curve {
        entries.push(floats(50940, &[0.0, 0.0, 1.0, 1.0])); // ProfileToneCurve
    }
    if let Some(xmp) = opts.xmp {
        entries.push(entry(700, BYTE, xmp.len() as u32, xmp.as_bytes().to_vec()));
    }
    entries.sort_by_key(|e| e.tag);

    // Layout: header (8) | IFD | out-of-line values | pixel data.
    let ifd_size = 2 + entries.len() * 12 + 4;
    let mut extra_offset = 8 + ifd_size as u32;
    let mut ifd = Vec::with_capacity(ifd_size);
    let mut extra = Vec::new();
    ifd.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    let strip_entry_pos = entries
        .iter()
        .position(|e| e.tag == 273)
        .expect("strip offsets");
    for e in &entries {
        ifd.extend_from_slice(&e.tag.to_le_bytes());
        ifd.extend_from_slice(&e.kind.to_le_bytes());
        ifd.extend_from_slice(&e.count.to_le_bytes());
        if e.bytes.len() <= 4 {
            let mut inline = e.bytes.clone();
            inline.resize(4, 0);
            ifd.extend_from_slice(&inline);
        } else {
            ifd.extend_from_slice(&(extra_offset + extra.len() as u32).to_le_bytes());
            extra.extend_from_slice(&e.bytes);
            if extra.len() % 2 == 1 {
                extra.push(0); // keep word alignment
            }
        }
    }
    ifd.extend_from_slice(&0u32.to_le_bytes()); // no next IFD
    extra_offset += extra.len() as u32;
    let pixel_offset = extra_offset;
    // Patch StripOffsets value (inline LONG).
    let pos = 2 + strip_entry_pos * 12 + 8;
    ifd[pos..pos + 4].copy_from_slice(&pixel_offset.to_le_bytes());

    let mut out = Vec::with_capacity(pixel_offset as usize + pixels.len());
    out.extend_from_slice(b"II\x2a\x00");
    out.extend_from_slice(&8u32.to_le_bytes());
    out.extend_from_slice(&ifd);
    out.extend_from_slice(&extra);
    out.extend_from_slice(&pixels);
    out
}

pub fn write(
    path: &std::path::Path,
    image: &LinearImage,
    opts: &DngOptions,
) -> std::io::Result<()> {
    let mut f = std::io::BufWriter::new(std::fs::File::create(path)?);
    f.write_all(&encode(image, opts))?;
    f.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_u16(b: &[u8], at: usize) -> u16 {
        u16::from_le_bytes([b[at], b[at + 1]])
    }
    fn read_u32(b: &[u8], at: usize) -> u32 {
        u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
    }

    /// Parse the IFD back: (tag, type, count, value-or-offset).
    fn ifd(b: &[u8]) -> Vec<(u16, u16, u32, u32)> {
        let start = read_u32(b, 4) as usize;
        let n = read_u16(b, start) as usize;
        (0..n)
            .map(|i| {
                let p = start + 2 + i * 12;
                (
                    read_u16(b, p),
                    read_u16(b, p + 2),
                    read_u32(b, p + 4),
                    read_u32(b, p + 8),
                )
            })
            .collect()
    }

    #[test]
    fn matrix_inverse_round_trips() {
        let inv = invert3(&PROPHOTO_TO_XYZ_D50);
        let id = mul3(&PROPHOTO_TO_XYZ_D50, &inv);
        for (i, row) in id.iter().enumerate() {
            for (j, v) in row.iter().enumerate() {
                assert!((v - if i == j { 1.0 } else { 0.0 }).abs() < 1e-9);
            }
        }
        // D50 white maps to (1,1,1) in ProPhoto.
        let white = apply3(&inv, [0.964_22, 1.0, 0.825_21]);
        for c in white {
            assert!((c - 1.0).abs() < 1e-3, "{white:?}");
        }
    }

    #[test]
    fn writes_valid_structure_with_pixels_and_xmp() {
        let mut img = LinearImage::new(4, 2);
        img.set(1, 0, [1.0, 0.5, 0.0]);
        let xmp = "<x:xmpmeta xmlns:x=\"adobe:ns:meta/\"/>";
        let bytes = encode(
            &img,
            &DngOptions {
                xmp: Some(xmp),
                ..Default::default()
            },
        );
        assert_eq!(&bytes[..4], b"II\x2a\x00");
        let entries = ifd(&bytes);
        let tags: Vec<u16> = entries.iter().map(|e| e.0).collect();
        let mut sorted = tags.clone();
        sorted.sort();
        assert_eq!(tags, sorted, "IFD entries must be sorted");
        let get = |t: u16| entries.iter().find(|e| e.0 == t).copied().unwrap();
        assert_eq!(get(262).3 & 0xffff, 34892);
        let strip = get(273).3 as usize;
        assert_eq!(get(279).3 as usize, 4 * 2 * 3 * 2);
        // Pixel (1,0) = (1.0, 0.5, 0.0) → (65535, 32768, 0)
        let px = strip + 3 * 2;
        assert_eq!(read_u16(&bytes, px), 65535);
        assert_eq!(read_u16(&bytes, px + 2), 32768);
        assert_eq!(read_u16(&bytes, px + 4), 0);
        let (_, _, count, offset) = get(700);
        let packet = &bytes[offset as usize..offset as usize + count as usize];
        assert_eq!(packet, xmp.as_bytes());
        assert_eq!(bytes.len(), strip + 4 * 2 * 3 * 2);
    }
}
