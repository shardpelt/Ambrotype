// SPDX-License-Identifier: GPL-3.0-or-later
//! Floating-point linear DNG: a merged (HDR) photo stored as a raw, so it
//! is edited like one here and in Lightroom.
//!
//! One IFD holding a `LinearRaw` image (demosaiced camera RGB, 3 samples per
//! pixel) as 16-bit floats (half precision: 11 significant bits over a range
//! of about 40 stops), in 256-pixel tiles compressed with Deflate after the
//! floating-point predictor (byte planes, then horizontal differences; TIFF
//! Technical Note 3, Predictor 3). `WhiteLevel` is 1, the floating-point
//! default. The camera's colour matrix (`ColorMatrix1`, labelled D65) and
//! as-shot white come from the source photo, and an EXIF IFD carries its
//! exposure and lens, so lens profiles still match.

use std::io::Write;
use std::path::Path;

use amb_engine::raw::CameraImage;
use flate2::Compression;
use flate2::write::ZlibEncoder;
use rayon::prelude::*;

use crate::Error;

/// Tile edge in pixels.
const TILE: usize = 256;

// TIFF field types.
const BYTE: u16 = 1;
const ASCII: u16 = 2;
const SHORT: u16 = 3;
const LONG: u16 = 4;
const RATIONAL: u16 = 5;
const SRATIONAL: u16 = 10;

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
    let bytes = v.iter().flat_map(|x| x.to_le_bytes()).collect();
    entry(tag, SHORT, v.len() as u32, bytes)
}

fn longs(tag: u16, v: &[u32]) -> Entry {
    let bytes = v.iter().flat_map(|x| x.to_le_bytes()).collect();
    entry(tag, LONG, v.len() as u32, bytes)
}

fn ascii(tag: u16, s: &str) -> Entry {
    let mut b: Vec<u8> = s.bytes().filter(|&c| c != 0).collect();
    b.push(0);
    entry(tag, ASCII, b.len() as u32, b)
}

fn rationals(tag: u16, v: &[(u32, u32)]) -> Entry {
    let bytes = v
        .iter()
        .flat_map(|(n, d)| n.to_le_bytes().into_iter().chain(d.to_le_bytes()))
        .collect();
    entry(tag, RATIONAL, v.len() as u32, bytes)
}

fn srationals(tag: u16, v: &[(i32, i32)]) -> Entry {
    let bytes = v
        .iter()
        .flat_map(|(n, d)| n.to_le_bytes().into_iter().chain(d.to_le_bytes()))
        .collect();
    entry(tag, SRATIONAL, v.len() as u32, bytes)
}

/// A real number as a signed rational with a fixed denominator.
fn signed(v: f64) -> (i32, i32) {
    const DEN: i32 = 1_000_000;
    let n = (v * f64::from(DEN)).round();
    (
        n.clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32,
        DEN,
    )
}

fn unsigned(v: f64) -> (u32, u32) {
    const DEN: u32 = 1_000_000;
    let n = (v * f64::from(DEN)).round();
    (n.clamp(0.0, f64::from(u32::MAX)) as u32, DEN)
}

/// Write `img` (camera RGB, white = 1, unbounded above) as a DNG, with an
/// optional XMP packet (develop settings, for Lightroom: it reads a DNG's
/// settings from inside the file).
pub fn write_linear(path: &Path, img: &CameraImage, xmp: Option<&str>) -> Result<(), Error> {
    let io = |source| Error::Io {
        path: path.to_path_buf(),
        source,
    };
    let bytes = encode_linear(img, xmp)?;
    let mut f = std::io::BufWriter::new(std::fs::File::create(path).map_err(io)?);
    f.write_all(&bytes).map_err(io)?;
    f.flush().map_err(io)
}

/// One tile as stored: half floats, byte planes per row, differenced, then
/// Deflate. Tiles past the image edge are padded by repeating the edge.
fn encode_tile(img: &CameraImage, tx: usize, ty: usize) -> Result<Vec<u8>, Error> {
    let (w, h) = (img.width, img.height);
    let line = TILE * 3;
    let mut raw = Vec::with_capacity(line * 2 * TILE);
    let mut row = vec![0u8; line * 2];
    for j in 0..TILE {
        let y = (ty * TILE + j).min(h - 1);
        for i in 0..TILE {
            let x = (tx * TILE + i).min(w - 1);
            for c in 0..3 {
                let v = img.rgb[(y * w + x) * 3 + c];
                let v = if v.is_finite() {
                    v.clamp(0.0, 65504.0)
                } else {
                    0.0
                };
                let [hi, lo] = half::f16::from_f32(v).to_bits().to_be_bytes();
                let k = i * 3 + c;
                row[k] = hi;
                row[line + k] = lo;
            }
        }
        // Horizontal differences of the byte stream, one pixel (3 samples) apart.
        for k in (3..row.len()).rev() {
            row[k] = row[k].wrapping_sub(row[k - 3]);
        }
        raw.extend_from_slice(&row);
    }
    let mut z = ZlibEncoder::new(Vec::new(), Compression::default());
    z.write_all(&raw)
        .and_then(|_| z.finish())
        .map_err(|e| Error::Encode(e.to_string()))
}

/// The DNG file for `img`.
pub fn encode_linear(img: &CameraImage, xmp: Option<&str>) -> Result<Vec<u8>, Error> {
    let (w, h) = (img.width, img.height);
    if w == 0 || h == 0 || img.rgb.len() != w * h * 3 {
        return Err(Error::Unsupported("empty image".into()));
    }
    let (tiles_x, tiles_y) = (w.div_ceil(TILE), h.div_ceil(TILE));
    let tiles: Vec<Vec<u8>> = (0..tiles_x * tiles_y)
        .into_par_iter()
        .map(|t| encode_tile(img, t % tiles_x, t / tiles_x))
        .collect::<Result<_, _>>()?;

    let m = &img.metadata;
    let make = if m.make.is_empty() {
        &img.make
    } else {
        &m.make
    };
    let model = if m.model.is_empty() {
        &img.model
    } else {
        &m.model
    };
    let matrix: Vec<(i32, i32)> = img
        .xyz_to_cam
        .iter()
        .flatten()
        .map(|&v| signed(v))
        .collect();
    let neutral: Vec<(u32, u32)> = img.as_shot_neutral.iter().map(|&v| unsigned(v)).collect();
    let mut entries = vec![
        longs(254, &[0]),                          // NewSubFileType: main image
        longs(256, &[w as u32]),                   // ImageWidth
        longs(257, &[h as u32]),                   // ImageLength
        shorts(258, &[16, 16, 16]),                // BitsPerSample
        shorts(259, &[8]),                         // Compression: Deflate
        shorts(262, &[34892]),                     // PhotometricInterpretation: LinearRaw
        ascii(271, make),                          // Make
        ascii(272, model),                         // Model
        shorts(274, &[m.orientation.clamp(1, 8)]), // Orientation
        shorts(277, &[3]),                         // SamplesPerPixel
        shorts(284, &[1]),                         // PlanarConfiguration: chunky
        ascii(305, "Ambrotype HDR merge"),         // Software
        shorts(317, &[3]),                         // Predictor: floating point
        longs(322, &[TILE as u32]),                // TileWidth
        longs(323, &[TILE as u32]),                // TileLength
        longs(324, &vec![0; tiles.len()]),         // TileOffsets (patched below)
        longs(
            325,
            &tiles.iter().map(|t| t.len() as u32).collect::<Vec<_>>(),
        ), // TileByteCounts
        shorts(339, &[3, 3, 3]),                   // SampleFormat: IEEE floating point
        longs(34665, &[0]),                        // ExifIFD (patched below)
        entry(50706, BYTE, 4, vec![1, 4, 0, 0]),   // DNGVersion 1.4
        entry(50707, BYTE, 4, vec![1, 4, 0, 0]),   // DNGBackwardVersion 1.4 (floats)
        ascii(50708, &format!("{make} {model}")),  // UniqueCameraModel
        longs(50717, &[1, 1, 1]),                  // WhiteLevel: 1.0 for floating point
        srationals(50721, &matrix),                // ColorMatrix1
        rationals(50728, &neutral),                // AsShotNeutral
        srationals(50730, &[(0, 1)]),              // BaselineExposure
        shorts(50778, &[21]),                      // CalibrationIlluminant1: D65
    ];
    if let Some(a) = &m.artist {
        entries.push(ascii(315, a));
    }
    if let Some(c) = &m.copyright {
        entries.push(ascii(33432, c));
    }
    if let Some(xmp) = xmp {
        entries.push(entry(700, BYTE, xmp.len() as u32, xmp.as_bytes().to_vec()));
    }
    entries.sort_by_key(|e| e.tag);

    let mut exif = Vec::new();
    if let Some(t) = m.exposure_time {
        exif.push(rationals(33434, &[t]));
    }
    if let Some(f) = m.fnumber {
        exif.push(rationals(33437, &[f]));
    }
    if let Some(iso) = m.iso {
        exif.push(shorts(34855, &[iso.min(65535) as u16]));
    }
    if let Some(d) = &m.date_time_original {
        exif.push(ascii(36867, d));
    }
    if let Some(b) = m.exposure_bias {
        exif.push(srationals(37380, &[b]));
    }
    if let Some(f) = m.focal_length {
        exif.push(rationals(37386, &[f]));
    }
    if let Some(o) = &m.offset_time_original {
        exif.push(ascii(36881, o));
    }
    if let Some(l) = &m.lens_make {
        exif.push(ascii(42035, l));
    }
    if let Some(l) = &m.lens_model {
        exif.push(ascii(42036, l));
    }
    exif.sort_by_key(|e| e.tag);

    // Layout: header | IFD0 | its values | EXIF IFD | its values | tiles.
    let mut out = Vec::new();
    out.extend_from_slice(b"II\x2a\x00");
    out.extend_from_slice(&8u32.to_le_bytes());
    let ifd0 = write_ifd(&mut out, &entries);
    let exif_at = out.len() as u32;
    write_ifd(&mut out, &exif);
    let mut offsets = Vec::with_capacity(tiles.len());
    for t in &tiles {
        if out.len() % 2 == 1 {
            out.push(0);
        }
        offsets.push(out.len() as u32);
        out.extend_from_slice(t);
    }
    if out.len() > u32::MAX as usize {
        return Err(Error::Unsupported(
            "the merged photo is too large for a DNG (over 4 GB)".into(),
        ));
    }
    patch(&mut out, &ifd0, 34665, &[exif_at]);
    patch(&mut out, &ifd0, 324, &offsets);
    Ok(out)
}

/// Where an IFD's entries put their values: tag → offset of the value bytes.
type Placed = Vec<(u16, usize)>;

/// Append an IFD (and its out-of-line values) at the end of `out`, which is
/// word aligned first. Returns where each entry's value went.
fn write_ifd(out: &mut Vec<u8>, entries: &[Entry]) -> Placed {
    if out.len() % 2 == 1 {
        out.push(0);
    }
    let start = out.len();
    let size = 2 + entries.len() * 12 + 4;
    let mut extra_at = start + size;
    let mut extra = Vec::new();
    let mut placed = Vec::new();
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    for e in entries {
        out.extend_from_slice(&e.tag.to_le_bytes());
        out.extend_from_slice(&e.kind.to_le_bytes());
        out.extend_from_slice(&e.count.to_le_bytes());
        if e.bytes.len() <= 4 {
            placed.push((e.tag, out.len()));
            let mut inline = e.bytes.clone();
            inline.resize(4, 0);
            out.extend_from_slice(&inline);
        } else {
            let at = extra_at + extra.len();
            placed.push((e.tag, at));
            out.extend_from_slice(&(at as u32).to_le_bytes());
            extra.extend_from_slice(&e.bytes);
            if extra.len() % 2 == 1 {
                extra.push(0);
            }
        }
    }
    out.extend_from_slice(&0u32.to_le_bytes()); // no next IFD
    extra_at += extra.len();
    debug_assert_eq!(out.len() + extra.len(), extra_at);
    out.extend_from_slice(&extra);
    placed
}

/// Overwrite the LONG values of `tag`'s entry.
fn patch(out: &mut [u8], placed: &Placed, tag: u16, values: &[u32]) {
    let (_, at) = placed
        .iter()
        .find(|(t, _)| *t == tag)
        .expect("patched tags are written");
    for (i, v) in values.iter().enumerate() {
        out[at + i * 4..at + i * 4 + 4].copy_from_slice(&v.to_le_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use amb_engine::raw::PhotoMetadata;

    fn sample(w: usize, h: usize) -> CameraImage {
        let mut rgb = Vec::with_capacity(w * h * 3);
        for y in 0..h {
            for x in 0..w {
                // Shadows to well above white (an HDR merge's range).
                let v = 0.001 * 2f32.powf((x + y) as f32 / (w + h) as f32 * 14.0);
                rgb.extend_from_slice(&[v * 0.8, v, v * 1.1]);
            }
        }
        CameraImage {
            width: w,
            height: h,
            rgb,
            xyz_to_cam: [[0.7, -0.2, -0.05], [-0.4, 1.2, 0.2], [-0.08, 0.15, 0.6]],
            as_shot_neutral: [0.5, 1.0, 0.7],
            clip: [f32::INFINITY; 3],
            rendered: false,
            make: "Nikon".into(),
            model: "D750".into(),
            shot: Default::default(),
            metadata: PhotoMetadata {
                make: "Nikon".into(),
                model: "D750".into(),
                lens_model: Some("35mm f/1.8".into()),
                exposure_time: Some((1, 60)),
                fnumber: Some((18, 10)),
                focal_length: Some((35, 1)),
                iso: Some(200),
                date_time_original: Some("2026:10:11 12:00:00".into()),
                orientation: 6,
                ..Default::default()
            },
        }
    }

    #[test]
    fn round_trips_through_the_raw_reader() {
        let dir = std::env::temp_dir().join(format!("amb-dng-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("merged.dng");
        // Not a multiple of the tile size, so edge tiles are padded.
        let img = sample(300, 270);
        write_linear(&path, &img, None).unwrap();
        let back = amb_engine::raw::load(&path).unwrap();
        assert_eq!((back.width, back.height), (300, 270));
        // Half floats keep 11 significant bits.
        for (a, b) in back.rgb.iter().zip(&img.rgb) {
            assert!((a - b).abs() <= b.abs() * 1e-3 + 1e-7, "{a} vs {b}");
        }
        assert!(
            img.rgb.iter().any(|v| *v > 10.0),
            "the sample goes above white"
        );
        for (r, s) in back
            .xyz_to_cam
            .iter()
            .flatten()
            .zip(img.xyz_to_cam.iter().flatten())
        {
            assert!((r - s).abs() < 1e-5, "{r} vs {s}");
        }
        for (a, b) in back.as_shot_neutral.iter().zip(&img.as_shot_neutral) {
            assert!((a / back.as_shot_neutral[1] - b).abs() < 1e-4);
        }
        let m = &back.metadata;
        assert_eq!(m.orientation, 6);
        assert_eq!(m.exposure_time, Some((1, 60)));
        assert_eq!(m.iso, Some(200));
        assert_eq!(m.lens_model.as_deref(), Some("35mm f/1.8"));
        assert_eq!(back.shot.focal_mm, Some(35.0));
        std::fs::remove_dir_all(&dir).ok();
    }
}
