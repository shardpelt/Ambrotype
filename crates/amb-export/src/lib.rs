//! Export: developed image (linear ProPhoto) → a file people can use.
//!
//! Formats: JPEG, PNG, TIFF, WebP (lossless), AVIF and OpenEXR, all with pure
//! Rust encoders. JPEG, PNG, TIFF and WebP get an embedded ICC profile for
//! their colour space (sRGB, Display P3, Adobe RGB-compatible, ProPhoto,
//! Rec. 2020); AVIF is sRGB only for now; EXR is linear. EXIF goes into JPEG,
//! PNG, WebP and AVIF (not TIFF yet).
//! Like Lightroom's Export dialog, an export can be resized ("Resize to Fit"),
//! output-sharpened for screen or paper, carry EXIF metadata (with privacy
//! choices), and, for JPEG, be limited to a file size.
//!
//! Order: orient upright → (exact sizes of another aspect: centre-crop) →
//! resize in linear light → convert to the output space
//! → encode the transfer curve → output sharpening → quantise (8-bit with
//! dither) → encode with ICC + EXIF.

pub mod exif;
pub mod geometry;

use std::io::Cursor;
use std::path::{Path, PathBuf};

use amb_color::space::OutputSpace;
use amb_engine::buffer::Image;
use amb_engine::raw::PhotoMetadata;
use image::{ExtendedColorType, ImageEncoder};
use rayon::prelude::*;

pub use exif::MetadataPolicy;
pub use geometry::{Resize, SharpenAmount, SharpenFor, parse_sharpen};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Unsupported(String),
    #[error("encoding failed: {0}")]
    Encode(String),
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Jpeg,
    Png,
    Tiff,
    /// Lossless WebP (lossy WebP needs libwebp; not yet).
    Webp,
    Avif,
    /// OpenEXR, 32-bit float, linear (no transfer curve, no clipping).
    Exr,
}

impl Format {
    pub const ALL: [Format; 6] = [
        Format::Jpeg,
        Format::Png,
        Format::Tiff,
        Format::Webp,
        Format::Avif,
        Format::Exr,
    ];

    pub fn from_name(name: &str) -> Option<Format> {
        Some(match name.to_ascii_lowercase().trim_start_matches('.') {
            "jpg" | "jpeg" => Format::Jpeg,
            "png" => Format::Png,
            "tif" | "tiff" => Format::Tiff,
            "webp" => Format::Webp,
            "avif" => Format::Avif,
            "exr" => Format::Exr,
            _ => return None,
        })
    }

    pub fn from_path(path: &Path) -> Option<Format> {
        path.extension()
            .and_then(|e| e.to_str())
            .and_then(Format::from_name)
    }

    pub fn extension(self) -> &'static str {
        match self {
            Format::Jpeg => "jpg",
            Format::Png => "png",
            Format::Tiff => "tif",
            Format::Webp => "webp",
            Format::Avif => "avif",
            Format::Exr => "exr",
        }
    }

    pub fn bit_depths(self) -> &'static [BitDepth] {
        match self {
            Format::Jpeg | Format::Webp | Format::Avif => &[BitDepth::Eight],
            Format::Png => &[BitDepth::Eight, BitDepth::Sixteen],
            Format::Tiff => &[BitDepth::Eight, BitDepth::Sixteen, BitDepth::Float32],
            Format::Exr => &[BitDepth::Float32],
        }
    }

    pub fn default_bit_depth(self) -> BitDepth {
        match self {
            Format::Tiff => BitDepth::Sixteen,
            Format::Exr => BitDepth::Float32,
            _ => BitDepth::Eight,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BitDepth {
    Eight,
    Sixteen,
    Float32,
}

impl BitDepth {
    pub fn from_bits(bits: u32) -> Option<BitDepth> {
        Some(match bits {
            8 => BitDepth::Eight,
            16 => BitDepth::Sixteen,
            32 => BitDepth::Float32,
            _ => return None,
        })
    }
}

/// JPEG chroma subsampling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Subsampling {
    S444,
    S422,
    S420,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExportSettings {
    pub format: Format,
    /// 1..100 (JPEG, AVIF).
    pub quality: u8,
    /// `None` = the format's default.
    pub bit_depth: Option<BitDepth>,
    pub color_space: OutputSpace,
    /// `None` = 4:4:4 at quality ≥ 90, else 4:2:0.
    pub subsampling: Option<Subsampling>,
    pub progressive: bool,
    pub resize: Resize,
    pub enlarge: bool,
    pub sharpen: Option<(SharpenFor, SharpenAmount)>,
    pub metadata: MetadataPolicy,
    /// JPEG only: lower the quality until the file fits.
    pub max_bytes: Option<u64>,
    /// AVIF encoder speed 1 (slow, small) ..= 10 (fast).
    pub avif_speed: u8,
}

impl ExportSettings {
    pub fn new(format: Format) -> ExportSettings {
        ExportSettings {
            format,
            quality: 90,
            bit_depth: None,
            color_space: OutputSpace::Srgb,
            subsampling: None,
            progressive: false,
            resize: Resize::None,
            enlarge: false,
            sharpen: None,
            metadata: MetadataPolicy::AllExceptLocation,
            max_bytes: None,
            avif_speed: 6,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExportReport {
    pub width: usize,
    pub height: usize,
    pub bytes: u64,
    pub format: Format,
    pub bit_depth: BitDepth,
    pub color_space: OutputSpace,
    /// The JPEG quality actually used (after a file-size limit).
    pub quality: u8,
}

/// Small deterministic dither in [-0.5, 0.5) code values.
fn dither(i: usize) -> f32 {
    let mut x = (i as u32).wrapping_mul(0x9E37_79B9) ^ 0x85EB_CA6B;
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    let a = (x & 0xFFFF) as f32 / 65536.0;
    let b = (x >> 16) as f32 / 65536.0;
    // Triangular distribution, ±1 LSB peak-to-peak.
    (a + b) * 0.5 - 0.5
}

/// Develop-to-file: orient, resize, convert, sharpen and encode.
pub fn export(
    img: &Image,
    meta: &PhotoMetadata,
    s: &ExportSettings,
    path: &Path,
) -> Result<ExportReport, Error> {
    let depth = s.bit_depth.unwrap_or(s.format.default_bit_depth());
    if !s.format.bit_depths().contains(&depth) {
        return Err(Error::Unsupported(format!(
            "{} does not support {} output",
            s.format.extension(),
            match depth {
                BitDepth::Eight => "8-bit",
                BitDepth::Sixteen => "16-bit",
                BitDepth::Float32 => "32-bit float",
            }
        )));
    }
    if s.format == Format::Avif && s.color_space != OutputSpace::Srgb {
        // The AVIF encoder cannot embed an ICC profile yet, and AVIF readers
        // assume sRGB without one.
        return Err(Error::Unsupported(
            "AVIF export is sRGB only for now (use JPEG, PNG or TIFF for wide gamut)".into(),
        ));
    }
    let mut upright = geometry::orient(img, meta.orientation);
    // An exact size with another aspect: keep the centre, then scale.
    if let Some((x, y, w, h)) = s.resize.cover_window(upright.width, upright.height) {
        upright = geometry::cut(&upright, x, y, w, h);
    }
    let (ow, oh) = s.resize.target(upright.width, upright.height, s.enlarge);
    let mut out = geometry::resize(&upright, ow, oh);

    // Working ProPhoto → output space; encoded unless EXR (linear).
    let m = s.color_space.from_prophoto().map(|r| r.map(|v| v as f32));
    let transfer = s.color_space.transfer();
    let float = depth == BitDepth::Float32;
    let linear_out = s.format == Format::Exr;
    out.rgb.par_chunks_mut(3).for_each(|p| {
        let v = [0, 1, 2].map(|i| m[i][0] * p[0] + m[i][1] * p[1] + m[i][2] * p[2]);
        for (slot, x) in p.iter_mut().zip(v) {
            // Integer formats clip to the gamut (hard clip; gamut mapping comes later).
            let x = if float { x } else { x.clamp(0.0, 1.0) };
            *slot = if linear_out {
                x
            } else {
                transfer.encode(f64::from(x)) as f32
            };
        }
    });
    if let Some((media, amount)) = s.sharpen {
        geometry::output_sharpen(&mut out, media, amount);
    }

    let icc = s.color_space.icc_profile();
    let exif = exif::build(meta, s.metadata);
    let (w, h) = (out.width as u32, out.height as u32);
    let mut quality = s.quality.clamp(1, 100);
    let bytes = match (s.format, depth) {
        (Format::Jpeg, _) => {
            let data = to_u8(&out.rgb);
            let mut encoded = encode_jpeg(&data, w, h, quality, s, &icc, exif.as_deref())?;
            if let Some(limit) = s.max_bytes {
                // Binary search for the highest quality that fits.
                if encoded.len() as u64 > limit {
                    let (mut lo, mut hi) = (1u8, quality - 1);
                    let mut best = None;
                    while lo <= hi {
                        let q = lo + (hi - lo) / 2;
                        let e = encode_jpeg(&data, w, h, q, s, &icc, exif.as_deref())?;
                        if e.len() as u64 <= limit {
                            best = Some((q, e));
                            lo = q + 1;
                        } else if q == 1 {
                            break;
                        } else {
                            hi = q - 1;
                        }
                    }
                    match best {
                        Some((q, e)) => {
                            quality = q;
                            encoded = e;
                        }
                        None => {
                            return Err(Error::Encode(format!(
                                "cannot fit {w}x{h} into {limit} bytes even at quality 1; resize it"
                            )));
                        }
                    }
                }
            }
            encoded
        }
        (Format::Png, BitDepth::Sixteen) | (Format::Tiff, BitDepth::Sixteen) => {
            let data = to_u16_bytes(&out.rgb);
            encode_image(s.format, &data, w, h, ExtendedColorType::Rgb16, &icc, exif)?
        }
        (Format::Tiff, BitDepth::Float32) => {
            let data: Vec<u8> = out.rgb.iter().flat_map(|v| v.to_ne_bytes()).collect();
            encode_image(s.format, &data, w, h, ExtendedColorType::Rgb32F, &icc, None)?
        }
        (Format::Exr, _) => {
            let data: Vec<u8> = out.rgb.iter().flat_map(|v| v.to_ne_bytes()).collect();
            let mut buf = Cursor::new(Vec::new());
            image::codecs::openexr::OpenExrEncoder::new(&mut buf)
                .write_image(&data, w, h, ExtendedColorType::Rgb32F)
                .map_err(|e| Error::Encode(e.to_string()))?;
            buf.into_inner()
        }
        (Format::Avif, _) => {
            let data = to_u8(&out.rgb);
            let mut buf = Vec::new();
            let mut enc = image::codecs::avif::AvifEncoder::new_with_speed_quality(
                &mut buf,
                s.avif_speed.clamp(1, 10),
                quality,
            );
            if let Some(e) = exif {
                let _ = enc.set_exif_metadata(e);
            }
            enc.write_image(&data, w, h, ExtendedColorType::Rgb8)
                .map_err(|e| Error::Encode(e.to_string()))?;
            buf
        }
        (format, _) => {
            let data = to_u8(&out.rgb);
            encode_image(format, &data, w, h, ExtendedColorType::Rgb8, &icc, exif)?
        }
    };
    std::fs::write(path, &bytes).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(ExportReport {
        width: out.width,
        height: out.height,
        bytes: bytes.len() as u64,
        format: s.format,
        bit_depth: depth,
        color_space: s.color_space,
        quality,
    })
}

fn to_u8(rgb: &[f32]) -> Vec<u8> {
    rgb.par_iter()
        .enumerate()
        .map(|(i, v)| {
            (v.clamp(0.0, 1.0) * 255.0 + dither(i))
                .round()
                .clamp(0.0, 255.0) as u8
        })
        .collect()
}

fn to_u16_bytes(rgb: &[f32]) -> Vec<u8> {
    rgb.par_iter()
        .flat_map_iter(|v| ((v.clamp(0.0, 1.0) * 65535.0).round() as u16).to_ne_bytes())
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn encode_jpeg(
    data: &[u8],
    w: u32,
    h: u32,
    quality: u8,
    s: &ExportSettings,
    icc: &[u8],
    exif: Option<&[u8]>,
) -> Result<Vec<u8>, Error> {
    use jpeg_encoder::{ColorType, Encoder, SamplingFactor};
    let (w16, h16) = (
        u16::try_from(w).map_err(|_| Error::Unsupported("JPEG is limited to 65535 px".into()))?,
        u16::try_from(h).map_err(|_| Error::Unsupported("JPEG is limited to 65535 px".into()))?,
    );
    let mut buf = Vec::new();
    let mut enc = Encoder::new(&mut buf, quality);
    let sampling = s.subsampling.unwrap_or(if quality >= 90 {
        Subsampling::S444
    } else {
        Subsampling::S420
    });
    enc.set_sampling_factor(match sampling {
        Subsampling::S444 => SamplingFactor::R_4_4_4,
        Subsampling::S422 => SamplingFactor::R_4_2_2,
        Subsampling::S420 => SamplingFactor::R_4_2_0,
    });
    enc.set_progressive(s.progressive);
    enc.set_optimized_huffman_tables(true);
    let err = |e: jpeg_encoder::EncodingError| Error::Encode(e.to_string());
    if let Some(exif) = exif {
        let mut app1 = b"Exif\0\0".to_vec();
        app1.extend_from_slice(exif);
        enc.add_app_segment(1, &app1).map_err(err)?;
    }
    enc.add_icc_profile(icc).map_err(err)?;
    enc.encode(data, w16, h16, ColorType::Rgb).map_err(err)?;
    Ok(buf)
}

fn encode_image(
    format: Format,
    data: &[u8],
    w: u32,
    h: u32,
    color: ExtendedColorType,
    icc: &[u8],
    exif: Option<Vec<u8>>,
) -> Result<Vec<u8>, Error> {
    let mut buf = Cursor::new(Vec::new());
    let err = |e: image::ImageError| Error::Encode(e.to_string());
    macro_rules! write_with {
        ($enc:expr) => {{
            let mut enc = $enc;
            let _ = enc.set_icc_profile(icc.to_vec());
            if let Some(e) = exif {
                let _ = enc.set_exif_metadata(e);
            }
            enc.write_image(data, w, h, color).map_err(err)?;
        }};
    }
    match format {
        Format::Png => write_with!(image::codecs::png::PngEncoder::new_with_quality(
            &mut buf,
            image::codecs::png::CompressionType::Best,
            image::codecs::png::FilterType::Adaptive,
        )),
        Format::Tiff => write_with!(image::codecs::tiff::TiffEncoder::new(&mut buf)),
        Format::Webp => write_with!(image::codecs::webp::WebPEncoder::new_lossless(&mut buf)),
        other => {
            return Err(Error::Unsupported(format!(
                "{} is encoded elsewhere",
                other.extension()
            )));
        }
    }
    Ok(buf.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_image() -> Image {
        Image::from_fn(240, 160, |x, y| {
            let t = x as f32 / 239.0;
            match y / 40 {
                0 => [t, t, t],
                1 => [t, 0.1, 0.05],
                2 => [0.05, t, 0.1],
                _ => [0.1, 0.05, t],
            }
        })
    }

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("amb-export-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    #[test]
    fn every_format_writes_a_decodable_file() {
        let img = test_image();
        let meta = PhotoMetadata {
            make: "Test".into(),
            orientation: 6,
            ..Default::default()
        };
        for format in Format::ALL {
            for &depth in format.bit_depths() {
                let mut s = ExportSettings::new(format);
                s.bit_depth = Some(depth);
                s.avif_speed = 10;
                let path = tmp(&format!("t-{depth:?}.{}", format.extension()));
                let report = export(&img, &meta, &s, &path).unwrap();
                // Rotated upright by orientation 6.
                assert_eq!((report.width, report.height), (160, 240), "{format:?}");
                if format == Format::Avif {
                    // No pure-Rust AVIF decoder here: check the container signature.
                    let bytes = std::fs::read(&path).unwrap();
                    assert_eq!(&bytes[4..12], b"ftypavif");
                    continue;
                }
                let decoded = image::open(&path).unwrap_or_else(|e| panic!("{format:?}: {e}"));
                assert_eq!(
                    (decoded.width(), decoded.height()),
                    (160, 240),
                    "{format:?}"
                );
            }
        }
    }

    #[test]
    fn exact_sizes_cover_and_centre_crop() {
        // 240×160 grey ramp in the top rows: black on the left, white on the right.
        let img = test_image();
        let meta = PhotoMetadata::default();
        let mut s = ExportSettings::new(Format::Png);
        let path = tmp("exact.png");
        // Square from 3:2: the middle 160×160 is kept, so the left edge is
        // no longer black.
        s.resize = Resize::Exact(100, 100);
        let r = export(&img, &meta, &s, &path).unwrap();
        assert_eq!((r.width, r.height), (100, 100));
        let decoded = image::open(&path).unwrap().to_rgb8();
        let left = decoded.get_pixel(0, 5)[0];
        let right = decoded.get_pixel(99, 5)[0];
        assert!(left > 90 && right < 250, "{left} {right}");
        // Same aspect: a plain resize (the black edge stays), and exact sizes
        // enlarge.
        s.resize = Resize::Exact(480, 320);
        let r = export(&img, &meta, &s, &path).unwrap();
        assert_eq!((r.width, r.height), (480, 320));
        let decoded = image::open(&path).unwrap().to_rgb8();
        assert!(decoded.get_pixel(0, 5)[0] < 20);
    }

    #[test]
    fn unsupported_depths_are_refused() {
        let mut s = ExportSettings::new(Format::Jpeg);
        s.bit_depth = Some(BitDepth::Sixteen);
        let r = export(&test_image(), &PhotoMetadata::default(), &s, &tmp("x.jpg"));
        assert!(matches!(r, Err(Error::Unsupported(_))));
    }

    #[test]
    fn jpeg_carries_icc_and_exif_and_respects_size_limit() {
        let img = Image::from_fn(600, 400, |x, y| {
            let n = ((x * 7919 + y * 104_729) % 1000) as f32 / 1000.0;
            [n, 1.0 - n, (n * 3.0) % 1.0]
        });
        let meta = PhotoMetadata {
            make: "Nikon".into(),
            model: "D750".into(),
            ..Default::default()
        };
        let mut s = ExportSettings::new(Format::Jpeg);
        s.quality = 95;
        s.color_space = OutputSpace::DisplayP3;
        let path = tmp("meta.jpg");
        let full = export(&img, &meta, &s, &path).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let has = |needle: &[u8]| bytes.windows(needle.len()).any(|w| w == needle);
        assert!(has(b"ICC_PROFILE") && has(b"Display P3 (Ambrotype)"));
        assert!(has(b"Exif\0\0II*\0") && has(b"D750"));
        s.max_bytes = Some(full.bytes / 3);
        let limited = export(&img, &meta, &s, &path).unwrap();
        assert!(
            limited.bytes <= full.bytes / 3,
            "{} > {}",
            limited.bytes,
            full.bytes / 3
        );
        assert!(limited.quality < 95);
    }

    #[test]
    fn colour_spaces_encode_white_and_grey_consistently() {
        // A neutral grey stays neutral in every space; white stays white.
        let img = Image::from_fn(8, 8, |x, _| if x < 4 { [0.18; 3] } else { [1.0; 3] });
        for space in OutputSpace::ALL {
            let mut s = ExportSettings::new(Format::Png);
            s.bit_depth = Some(BitDepth::Sixteen);
            s.color_space = space;
            let path = tmp(&format!("grey-{}.png", space.id()));
            export(&img, &PhotoMetadata::default(), &s, &path).unwrap();
            let png = image::open(&path).unwrap().to_rgb16();
            let g = png.get_pixel(1, 1).0;
            let w = png.get_pixel(6, 1).0;
            assert!(
                g.iter()
                    .all(|c| (i32::from(*c) - i32::from(g[0])).abs() <= 80),
                "{space:?} {g:?}"
            );
            assert!(w.iter().all(|c| *c >= 65400), "{space:?} {w:?}");
        }
    }
}
