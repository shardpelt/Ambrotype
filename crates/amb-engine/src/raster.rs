// SPDX-License-Identifier: GPL-3.0-or-later
//! Rendered images (JPEG, PNG, TIFF, WebP) as a [`CameraImage`].
//!
//! These are already white-balanced and in a display colour space, so the
//! "camera" space is the image's own RGB, made linear with the tone curves
//! of its embedded ICC profile (sRGB when there is none), and white is equal
//! RGB. Lightroom edits such files with relative white balance
//! (`IncrementalTemperature`, `IncrementalTint`) instead of Kelvin.

use std::path::Path;

use amb_color::icc::{self, Curve, RgbProfile};
use amb_color::matrix::invert3;

use crate::Error;
use crate::raw::{CameraImage, Gps, PhotoMetadata, Ratio};

/// File extensions opened as rendered images (lower case).
pub const EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "tif", "tiff", "webp"];

/// Whether `path` is a rendered image rather than a raw file, by extension.
pub fn is_raster(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

pub fn load(path: &Path) -> Result<CameraImage, Error> {
    use image::ImageDecoder;
    let decode = |e: image::ImageError| Error::Decode(e.to_string());
    let reader = image::ImageReader::open(path)
        .map_err(|e| Error::Decode(e.to_string()))?
        .with_guessed_format()
        .map_err(|e| Error::Decode(e.to_string()))?;
    let mut decoder = reader.into_decoder().map_err(decode)?;
    let profile = decoder
        .icc_profile()
        .ok()
        .flatten()
        .and_then(|b| icc::parse(&b))
        .unwrap_or_else(icc::srgb);
    let image = image::DynamicImage::from_decoder(decoder).map_err(decode)?;
    let (width, height) = (image.width() as usize, image.height() as usize);
    let encoded = image.into_rgb32f();
    let luts = profile.curves.clone().map(|c| Lut::new(&c));
    let rgb: Vec<f32> = encoded
        .as_raw()
        .chunks_exact(3)
        .flat_map(|p| [luts[0].get(p[0]), luts[1].get(p[1]), luts[2].get(p[2])])
        .collect();

    let (shot, metadata) = read_metadata(path).unwrap_or_else(|_| {
        let none = PhotoMetadata {
            orientation: 1,
            ..Default::default()
        };
        (Default::default(), none)
    });
    Ok(CameraImage {
        width,
        height,
        rgb,
        xyz_to_cam: invert3(&profile.to_xyz_d50),
        matrices: Vec::new(),
        highlights_rebuilt: false,
        as_shot_neutral: [1.0; 3],
        clip: [f32::INFINITY; 3],
        rendered: true,
        make: metadata.make.clone(),
        model: metadata.model.clone(),
        shot,
        metadata,
    })
}

/// A tone curve sampled finely enough for 16-bit input.
struct Lut(Vec<f32>);

impl Lut {
    const N: usize = 4096;

    fn new(c: &Curve) -> Self {
        Self(
            (0..=Self::N)
                .map(|i| c.to_linear(i as f64 / Self::N as f64) as f32)
                .collect(),
        )
    }

    fn get(&self, v: f32) -> f32 {
        let x = v.clamp(0.0, 1.0) * Self::N as f32;
        let i = (x as usize).min(Self::N - 1);
        let f = x - i as f32;
        self.0[i] * (1.0 - f) + self.0[i + 1] * f
    }
}

/// A quick picture of a rendered image at most `max_edge` pixels on its
/// long side, for thumbnails: a JPEG is decoded straight at 1/2, 1/4 or 1/8
/// size (its own DCT scaling), other formats in full and scaled down. Its
/// colours are shown as stored (no profile conversion) and it isn't turned
/// upright.
pub fn quick_image(path: &Path, max_edge: u32) -> Result<image::RgbImage, Error> {
    let decode = |e: String| Error::Decode(format!("{}: {e}", path.display()));
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    let img = if ext == "jpg" || ext == "jpeg" {
        let file = std::fs::File::open(path).map_err(|e| decode(e.to_string()))?;
        let mut d = jpeg_decoder::Decoder::new(std::io::BufReader::new(file));
        d.read_info().map_err(|e| decode(e.to_string()))?;
        let edge = max_edge.clamp(16, u32::from(u16::MAX)) as u16;
        let (w, h) = d.scale(edge, edge).map_err(|e| decode(e.to_string()))?;
        let pixels = d.decode().map_err(|e| decode(e.to_string()))?;
        let info = d.info().ok_or_else(|| decode("no image".into()))?;
        let (w, h) = (u32::from(w), u32::from(h));
        let rgb: Vec<u8> = match info.pixel_format {
            jpeg_decoder::PixelFormat::RGB24 => pixels,
            jpeg_decoder::PixelFormat::L8 => pixels.iter().flat_map(|&v| [v, v, v]).collect(),
            jpeg_decoder::PixelFormat::L16 => pixels
                .chunks_exact(2)
                .flat_map(|v| [v[0], v[0], v[0]])
                .collect(),
            jpeg_decoder::PixelFormat::CMYK32 => pixels
                .chunks_exact(4)
                .flat_map(|p| {
                    let k = 255 - u16::from(p[3]);
                    [0, 1, 2].map(|c| ((255 - u16::from(p[c])) * k / 255) as u8)
                })
                .collect(),
        };
        image::RgbImage::from_raw(w, h, rgb).ok_or_else(|| decode("bad size".into()))?
    } else {
        image::ImageReader::open(path)
            .map_err(|e| decode(e.to_string()))?
            .with_guessed_format()
            .map_err(|e| decode(e.to_string()))?
            .decode()
            .map_err(|e| decode(e.to_string()))?
            .to_rgb8()
    };
    let (w, h) = (img.width(), img.height());
    Ok(if w.max(h) > max_edge {
        let k = f64::from(max_edge) / f64::from(w.max(h));
        let size = |v: u32| ((f64::from(v) * k).round() as u32).max(1);
        image::imageops::thumbnail(&img, size(w), size(h))
    } else {
        img
    })
}

/// The colour space a rendered image declares (sRGB when it has no
/// readable profile).
pub fn profile(path: &Path) -> RgbProfile {
    use image::ImageDecoder;
    image::ImageReader::open(path)
        .ok()
        .and_then(|r| r.with_guessed_format().ok())
        .and_then(|r| r.into_decoder().ok())
        .and_then(|mut d| d.icc_profile().ok().flatten())
        .and_then(|b| icc::parse(&b))
        .unwrap_or_else(icc::srgb)
}

/// Camera, lens and exposure facts from the image's EXIF. Missing fields
/// stay empty; a file without EXIF gives defaults.
pub fn read_metadata(path: &Path) -> Result<(amb_lens::ShotInfo, PhotoMetadata), Error> {
    use exif::{In, Tag, Value};
    let file = std::fs::File::open(path).map_err(|e| Error::Decode(e.to_string()))?;
    let exif = exif::Reader::new()
        .read_from_container(&mut std::io::BufReader::new(file))
        .map_err(|e| Error::Decode(e.to_string()))?;
    let field = |t: Tag| exif.get_field(t, In::PRIMARY).map(|f| &f.value);
    let text = |t: Tag| match field(t) {
        Some(Value::Ascii(v)) => v
            .first()
            .map(|s| {
                String::from_utf8_lossy(s)
                    .trim_matches(char::from(0))
                    .trim()
                    .to_string()
            })
            .filter(|s| !s.is_empty()),
        _ => None,
    };
    let ratio = |t: Tag| -> Option<Ratio> {
        match field(t) {
            Some(Value::Rational(v)) => {
                v.first().filter(|r| r.denom != 0).map(|r| (r.num, r.denom))
            }
            _ => None,
        }
    };
    let number = |t: Tag| field(t).and_then(|v| v.get_uint(0));
    let dms = |t: Tag| -> Option<[Ratio; 3]> {
        match field(t) {
            Some(Value::Rational(v)) if v.len() == 3 => {
                Some([0, 1, 2].map(|i| (v[i].num, v[i].denom)))
            }
            _ => None,
        }
    };
    let gps = (|| {
        Some(Gps {
            latitude: (text(Tag::GPSLatitudeRef)?, dms(Tag::GPSLatitude)?),
            longitude: (text(Tag::GPSLongitudeRef)?, dms(Tag::GPSLongitude)?),
            altitude: ratio(Tag::GPSAltitude)
                .map(|a| (number(Tag::GPSAltitudeRef).unwrap_or(0) as u8, a)),
        })
    })();
    let make = text(Tag::Make).unwrap_or_default();
    let model = text(Tag::Model).unwrap_or_default();
    let as_f64 = |r: Option<Ratio>| {
        r.map(|(n, d)| f64::from(n) / f64::from(d))
            .filter(|v| *v > 0.0)
    };
    let metadata = PhotoMetadata {
        make: make.clone(),
        model: model.clone(),
        lens_make: text(Tag::LensMake),
        lens_model: text(Tag::LensModel),
        exposure_time: ratio(Tag::ExposureTime),
        fnumber: ratio(Tag::FNumber),
        focal_length: ratio(Tag::FocalLength),
        iso: number(Tag::PhotographicSensitivity),
        exposure_bias: match field(Tag::ExposureBiasValue) {
            Some(Value::SRational(v)) => {
                v.first().filter(|r| r.denom != 0).map(|r| (r.num, r.denom))
            }
            _ => None,
        },
        date_time_original: text(Tag::DateTimeOriginal),
        offset_time_original: text(Tag::OffsetTimeOriginal),
        artist: text(Tag::Artist),
        copyright: text(Tag::Copyright),
        orientation: number(Tag::Orientation)
            .map(|o| o as u16)
            .filter(|o| (1..=8).contains(o))
            .unwrap_or(1),
        gps,
    };
    let shot = amb_lens::ShotInfo {
        camera_make: make,
        camera_model: model,
        lens_make: metadata.lens_make.clone(),
        lens_model: metadata.lens_model.clone(),
        focal_mm: as_f64(metadata.focal_length),
        aperture: as_f64(metadata.fnumber),
        distance_m: as_f64(ratio(Tag::SubjectDistance)),
        crop_factor: None,
        embedded: None,
    };
    Ok((shot, metadata))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("amb-raster-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn png_without_profile_is_srgb() {
        let path = temp_dir().join("grey.png");
        let img = image::RgbImage::from_pixel(4, 3, image::Rgb([188, 188, 188]));
        img.save(&path).unwrap();
        let c = load(&path).unwrap();
        assert_eq!((c.width, c.height), (4, 3));
        assert!(c.rendered);
        // sRGB 188 is linear 0.5.
        assert!((c.rgb[0] - 0.5029).abs() < 2e-3, "{}", c.rgb[0]);
        assert_eq!(c.as_shot_neutral, [1.0; 3]);
        assert_eq!(c.metadata.orientation, 1);
    }

    #[test]
    fn knows_rendered_extensions() {
        assert!(is_raster(Path::new("a/b.JPG")));
        assert!(is_raster(Path::new("scan.tiff")));
        assert!(!is_raster(Path::new("DSCF0001.RAF")));
        assert!(!is_raster(Path::new("photo.dng")));
    }
}
