// SPDX-License-Identifier: GPL-3.0-or-later
//! Raw decoding into linear camera RGB, via `rawler` (pure Rust).
//!
//! rawler handles container parsing, decompression, black/white scaling and
//! demosaicing, except for X-Trans sensors, which use our own demosaic
//! ([`crate::xtrans`]). Colour (white balance, camera matrix) is done by our
//! pipeline.

use std::path::Path;

use amb_color::matrix::{Mat3, apply3, invert3, xy_to_xyz};
use rawler::imgop::develop::{Intermediate, ProcessingStep, RawDevelop};
use rawler::imgop::xyz::Illuminant;
use rayon::prelude::*;

use crate::Error;

/// Linear camera-space image, scaled so the sensor white level is 1.0.
pub struct CameraImage {
    pub width: usize,
    pub height: usize,
    /// Interleaved RGB, row-major.
    pub rgb: Vec<f32>,
    /// XYZ → camera matrix (D65 calibration preferred).
    pub xyz_to_cam: Mat3,
    /// Camera RGB of the as-shot white (green normalised to 1).
    pub as_shot_neutral: [f64; 3],
    /// Level at which each channel clips ([`clip_levels`]); infinity when it doesn't.
    pub clip: [f32; 3],
    /// A rendered image (JPEG, PNG, TIFF, WebP) rather than a raw: already
    /// white-balanced and toned ([`crate::raster`]).
    pub rendered: bool,
    pub make: String,
    pub model: String,
    /// Lens and exposure metadata for lens-profile matching.
    pub shot: amb_lens::ShotInfo,
    /// Metadata carried into exports (EXIF), including the orientation.
    pub metadata: PhotoMetadata,
}

/// A rational number as stored in EXIF.
pub type Ratio = (u32, u32);

/// GPS position from the raw's EXIF.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Gps {
    /// "N"/"S" and degrees, minutes, seconds.
    pub latitude: (String, [Ratio; 3]),
    /// "E"/"W" and degrees, minutes, seconds.
    pub longitude: (String, [Ratio; 3]),
    /// 0 = above sea level, 1 = below; metres.
    pub altitude: Option<(u8, Ratio)>,
}

/// Photo metadata for exports. Values keep their EXIF form.
/// Raw file extensions we open (lower case).
pub const RAW_EXTENSIONS: [&str; 21] = [
    "dng", "nef", "nrw", "cr2", "cr3", "crw", "arw", "srf", "sr2", "raf", "orf", "rw2", "pef",
    "srw", "3fr", "iiq", "erf", "mef", "mos", "kdc", "dcr",
];

/// Whether `path` has a raw file extension.
pub fn is_raw_path(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| RAW_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct PhotoMetadata {
    pub make: String,
    pub model: String,
    pub lens_make: Option<String>,
    pub lens_model: Option<String>,
    pub exposure_time: Option<Ratio>,
    pub fnumber: Option<Ratio>,
    pub focal_length: Option<Ratio>,
    pub iso: Option<u32>,
    pub exposure_bias: Option<(i32, i32)>,
    /// "YYYY:MM:DD HH:MM:SS"
    pub date_time_original: Option<String>,
    pub offset_time_original: Option<String>,
    pub artist: Option<String>,
    pub copyright: Option<String>,
    /// EXIF orientation 1..8 (1 = as stored). Pixels are kept in sensor order;
    /// exports rotate them.
    pub orientation: u16,
    pub gps: Option<Gps>,
}

fn color_matrix(raw: &rawler::RawImage) -> Result<Mat3, Error> {
    let (_, flat) = raw
        .color_matrix_find_first([
            Illuminant::D65,
            Illuminant::D50,
            Illuminant::D55,
            Illuminant::A,
        ])
        .or_else(|| raw.color_matrix.iter().next().map(|(k, v)| (*k, v.clone())))
        .ok_or_else(|| Error::Unsupported("no colour matrix for this camera".into()))?;
    if flat.len() < 9 {
        return Err(Error::Unsupported(
            "colour matrix has fewer than 9 entries".into(),
        ));
    }
    let mut m = [[0.0; 3]; 3];
    for (i, row) in m.iter_mut().enumerate() {
        for (j, cell) in row.iter_mut().enumerate() {
            *cell = f64::from(flat[i * 3 + j]);
        }
    }
    Ok(m)
}

/// Open a photo: a raw file, or a rendered image by its extension
/// ([`crate::raster`]).
pub fn load(path: &Path) -> Result<CameraImage, Error> {
    if crate::raster::is_raster(path) {
        return crate::raster::load(path);
    }
    let raw = rawler::decode_file(path).map_err(|e| Error::Decode(e.to_string()))?;
    if raw.cpp == 1 && raw.cfa_is_four_color() {
        return Err(Error::Unsupported(
            "four-colour CFA sensors are not supported yet".into(),
        ));
    }
    let xyz_to_cam = color_matrix(&raw)?;

    let develop = RawDevelop::new_with(&[
        ProcessingStep::Rescale,
        ProcessingStep::Demosaic,
        ProcessingStep::FujiRotate,
        ProcessingStep::CropActiveArea,
        ProcessingStep::CropDefault,
    ]);
    let developed = match develop_xtrans(&raw)? {
        Some(xtrans) => xtrans,
        None => developed(
            develop
                .develop_intermediate(&raw)
                .map_err(|e| Error::Decode(e.to_string()))?,
        )?,
    };
    let (width, height, rgb) = developed;

    // As-shot white: rawler stores multipliers (1 / neutral). Fall back to D65.
    let wb = raw.wb_coeffs;
    let as_shot_neutral = if wb[..3].iter().all(|c| c.is_finite() && *c > 0.0) {
        let n = [
            1.0 / f64::from(wb[0]),
            1.0 / f64::from(wb[1]),
            1.0 / f64::from(wb[2]),
        ];
        [n[0] / n[1], 1.0, n[2] / n[1]]
    } else {
        neutral_for_xy(&xyz_to_cam, 0.3127, 0.3290)
    };

    let (shot, mut metadata) = read_metadata(path).unwrap_or_else(|_| {
        (
            amb_lens::ShotInfo {
                camera_make: raw.clean_make.clone(),
                camera_model: raw.clean_model.clone(),
                ..Default::default()
            },
            PhotoMetadata {
                make: raw.clean_make.clone(),
                model: raw.clean_model.clone(),
                orientation: orientation_code(raw.orientation),
                ..Default::default()
            },
        )
    });

    // Some formats only carry the orientation in the decoder's own field.
    if metadata.orientation == 1 {
        metadata.orientation = orientation_code(raw.orientation);
    }

    let clip = clip_levels(&rgb);
    Ok(CameraImage {
        width,
        height,
        rgb,
        clip,
        rendered: false,
        xyz_to_cam,
        as_shot_neutral,
        make: raw.clean_make.clone(),
        model: raw.clean_model.clone(),
        shot,
        metadata,
    })
}

/// Width, height and interleaved RGB of a developed image.
type Developed = (usize, usize, Vec<f32>);

fn developed(intermediate: Intermediate) -> Result<Developed, Error> {
    match intermediate {
        Intermediate::ThreeColor(pixels) => Ok((pixels.dim().w, pixels.dim().h, pixels.flatten())),
        Intermediate::Monochrome(pixels) => {
            let rgb = pixels.data.iter().flat_map(|v| [*v, *v, *v]).collect();
            Ok((pixels.dim().w, pixels.dim().h, rgb))
        }
        Intermediate::FourColor(_) => Err(Error::Unsupported(
            "four-colour images are not supported yet".into(),
        )),
    }
}

/// X-Trans sensors: our Markesteijn demosaic instead of rawler's bilinear
/// one, then the same active-area and default crops rawler would apply.
fn develop_xtrans(raw: &rawler::RawImage) -> Result<Option<Developed>, Error> {
    use rawler::imgop::sensor::SensorType;
    use rawler::rawimage::RawPhotometricInterpretation;
    let RawPhotometricInterpretation::Cfa(config) = &raw.photometric else {
        return Ok(None);
    };
    if config.sensor != SensorType::Xtrans
        || !config.cfa.is_rgb()
        || raw.cpp != 1
        || raw.fuji_rotation_width.is_some()
    {
        return Ok(None);
    }
    let Intermediate::Monochrome(mosaic) = RawDevelop::new_with(&[ProcessingStep::Rescale])
        .develop_intermediate(raw)
        .map_err(|e| Error::Decode(e.to_string()))?
    else {
        return Ok(None);
    };
    let roi = raw.active_area.unwrap_or(mosaic.rect());
    let cfa = config.cfa.shift(roi.p.x, roi.p.y);
    let pattern: crate::xtrans::Pattern =
        std::array::from_fn(|r| std::array::from_fn(|c| cfa.color_at(r, c) as u8));
    let (w, h) = (roi.d.w, roi.d.h);
    let mut samples = Vec::with_capacity(w * h);
    for y in 0..h {
        let start = (roi.p.y + y) * mosaic.width + roi.p.x;
        samples.extend_from_slice(&mosaic.data[start..start + w]);
    }
    drop(mosaic);
    let mut rgb = crate::xtrans::demosaic(&samples, w, h, &pattern);
    drop(samples);
    if let Some(mut crop) = raw.crop_area.or(raw.active_area) {
        if let Some(active) = &raw.active_area {
            crop = crop.adapt(active);
        }
        if (crop.d.w, crop.d.h) != (w, h) {
            // In place: each cropped row moves to or before where it was.
            let row = crop.d.w * 3;
            for (i, y) in (crop.p.y..crop.p.y + crop.d.h).enumerate() {
                let start = (y * w + crop.p.x) * 3;
                rgb.copy_within(start..start + row, i * row);
            }
            rgb.truncate(crop.d.h * row);
            rgb.shrink_to_fit();
            return Ok(Some((crop.d.w, crop.d.h, rgb)));
        }
    }
    Ok(Some((w, h, rgb)))
}

/// Area-average downscale of camera RGB so the long edge is at most `max_edge`.
pub fn downscale(img: &CameraImage, max_edge: usize) -> CameraImage {
    let long = img.width.max(img.height);
    let factor = long.div_ceil(max_edge.max(1)).max(1);
    if factor == 1 {
        return img.with_pixels(img.rgb.clone(), img.width, img.height);
    }
    let (w, h) = (img.width / factor, img.height / factor);
    let mut rgb = vec![0.0f32; w * h * 3];
    let n = (factor * factor) as f32;
    rgb.par_chunks_mut(w * 3).enumerate().for_each(|(y, row)| {
        for x in 0..w {
            let mut acc = [0.0f32; 3];
            for dy in 0..factor {
                let sy = y * factor + dy;
                for dx in 0..factor {
                    let i = (sy * img.width + x * factor + dx) * 3;
                    acc[0] += img.rgb[i];
                    acc[1] += img.rgb[i + 1];
                    acc[2] += img.rgb[i + 2];
                }
            }
            row[x * 3..x * 3 + 3].copy_from_slice(&acc.map(|v| v / n));
        }
    });
    img.with_pixels(rgb, w, h)
}

impl CameraImage {
    /// The same photo (colour data and metadata) with other pixels.
    pub fn with_pixels(&self, rgb: Vec<f32>, width: usize, height: usize) -> CameraImage {
        CameraImage {
            width,
            height,
            rgb,
            xyz_to_cam: self.xyz_to_cam,
            as_shot_neutral: self.as_shot_neutral,
            clip: self.clip,
            rendered: self.rendered,
            make: self.make.clone(),
            model: self.model.clone(),
            shot: self.shot.clone(),
            metadata: self.metadata.clone(),
        }
    }
}

/// Where each channel clips: the level its brightest values pile up at, or
/// infinity when nothing piles up. Sensors saturate at slightly different
/// levels per channel and camera, and demosaicing blurs the edge, so this is
/// read from the image rather than taken from the white level.
pub fn clip_levels(rgb: &[f32]) -> [f32; 3] {
    // Histogram of the top 30% of each channel's range; a clip is a spike in
    // the top 10% well above the counts lower down.
    const BINS: usize = 120;
    let step = (rgb.len() / 3 / 2_000_000).max(1);
    let mut out = [f32::INFINITY; 3];
    for (ch, slot) in out.iter_mut().enumerate() {
        let values = || rgb.chunks_exact(3).step_by(step).map(|p| p[ch]);
        let max = values().fold(0.0f32, f32::max);
        if max <= 0.25 {
            continue;
        }
        let lo = 0.7 * max;
        let width = (max - lo) / BINS as f32;
        let mut hist = [0usize; BINS];
        let mut n = 0usize;
        for v in values() {
            n += 1;
            if v >= lo {
                hist[(((v - lo) / width) as usize).min(BINS - 1)] += 1;
            }
        }
        let baseline = hist[..BINS / 3].iter().sum::<usize>() as f64 / (BINS / 3) as f64;
        let (peak, count) = hist
            .iter()
            .enumerate()
            .skip(BINS * 2 / 3)
            .max_by_key(|(_, c)| **c)
            .unwrap();
        if *count as f64 >= n as f64 * 5e-5 && *count as f64 > 5.0 * baseline.max(1.0) {
            *slot = lo + peak as f32 * width;
        }
    }
    out
}

fn orientation_code(o: rawler::decoders::Orientation) -> u16 {
    use rawler::decoders::Orientation as O;
    match o {
        O::Normal | O::Unknown => 1,
        O::HorizontalFlip => 2,
        O::Rotate180 => 3,
        O::VerticalFlip => 4,
        O::Transpose => 5,
        O::Rotate90 => 6,
        O::Transverse => 7,
        O::Rotate270 => 8,
    }
}

/// Camera and lens metadata of a raw file, without decoding the image.
/// Missing fields stay `None`.
pub fn read_shot(path: &Path) -> Result<amb_lens::ShotInfo, Error> {
    read_metadata(path).map(|(shot, _)| shot)
}

/// Lens-matching info and export metadata of a raw file, without decoding the image.
pub fn read_metadata(path: &Path) -> Result<(amb_lens::ShotInfo, PhotoMetadata), Error> {
    use rawler::formats::tiff::Rational;
    if crate::raster::is_raster(path) {
        return crate::raster::read_metadata(path);
    }
    let src = rawler::rawsource::RawSource::new(path).map_err(|e| Error::Decode(e.to_string()))?;
    let decoder = rawler::get_decoder(&src).map_err(|e| Error::Decode(e.to_string()))?;
    let md = decoder
        .raw_metadata(&src, &rawler::decoders::RawDecodeParams::default())
        .map_err(|e| Error::Decode(e.to_string()))?;
    let ratio = |r: Option<Rational>| {
        r.filter(|r| r.d != 0)
            .map(|r| f64::from(r.n) / f64::from(r.d))
            .filter(|v| v.is_finite() && *v > 0.0)
    };
    let raw_ratio = |r: Option<Rational>| r.filter(|r| r.d != 0).map(|r| (r.n, r.d));
    let non_empty = |s: &Option<String>| {
        s.as_ref()
            .map(|s| s.trim_matches(char::from(0)).trim().to_string())
            .filter(|s| !s.is_empty())
    };
    let (lens_make, lens_model) = match &md.lens {
        Some(lens) => (Some(lens.lens_make.clone()), Some(lens.lens_model.clone())),
        None => (
            non_empty(&md.exif.lens_make),
            non_empty(&md.exif.lens_model),
        ),
    };
    let e = &md.exif;
    let dms = |v: &Option<[Rational; 3]>| v.map(|a| a.map(|r| (r.n, r.d)));
    let gps = e.gps.as_ref().and_then(|g| {
        Some(Gps {
            latitude: (g.gps_latitude_ref.clone()?, dms(&g.gps_latitude)?),
            longitude: (g.gps_longitude_ref.clone()?, dms(&g.gps_longitude)?),
            altitude: g
                .gps_altitude
                .map(|a| (g.gps_altitude_ref.unwrap_or(0), (a.n, a.d))),
        })
    });
    let metadata = PhotoMetadata {
        make: md.make.clone(),
        model: md.model.clone(),
        lens_make: lens_make.clone(),
        lens_model: lens_model.clone(),
        exposure_time: raw_ratio(e.exposure_time),
        fnumber: raw_ratio(e.fnumber),
        focal_length: raw_ratio(e.focal_length),
        iso: e
            .iso_speed_ratings
            .map(u32::from)
            .or(e.iso_speed)
            .or(e.recommended_exposure_index),
        exposure_bias: e.exposure_bias.filter(|r| r.d != 0).map(|r| (r.n, r.d)),
        date_time_original: non_empty(&e.date_time_original),
        offset_time_original: non_empty(&e.offset_time_original),
        artist: non_empty(&e.artist),
        copyright: non_empty(&e.copyright),
        orientation: e.orientation.filter(|o| (1..=8).contains(o)).unwrap_or(1),
        gps,
    };
    let shot = amb_lens::ShotInfo {
        camera_make: md.make.clone(),
        camera_model: md.model.clone(),
        lens_make,
        lens_model,
        focal_mm: ratio(e.focal_length),
        aperture: ratio(e.fnumber),
        distance_m: ratio(e.subject_distance),
        crop_factor: None,
    };
    Ok((shot, metadata))
}

/// Camera RGB of a white with chromaticity (x, y), green normalised to 1.
pub fn neutral_for_xy(xyz_to_cam: &Mat3, x: f64, y: f64) -> [f64; 3] {
    let n = apply3(xyz_to_cam, xy_to_xyz(x, y));
    [n[0] / n[1], 1.0, n[2] / n[1]]
}

/// Chromaticity of a camera-space white (the inverse of [`neutral_for_xy`]).
pub fn xy_for_neutral(xyz_to_cam: &Mat3, neutral: [f64; 3]) -> (f64, f64) {
    let xyz = apply3(&invert3(xyz_to_cam), neutral);
    let sum = xyz[0] + xyz[1] + xyz[2];
    (xyz[0] / sum, xyz[1] / sum)
}

trait CfaExt {
    fn cfa_is_four_color(&self) -> bool;
}

impl CfaExt for rawler::RawImage {
    fn cfa_is_four_color(&self) -> bool {
        match &self.photometric {
            rawler::rawimage::RawPhotometricInterpretation::Cfa(config) => {
                config.cfa.unique_colors() == 4
            }
            _ => false,
        }
    }
}

/// A small thumbnail from the preview the camera embedded in the raw (no raw
/// decoding, so it's quick), upright and at most `max_edge` pixels on its long
/// side. `None` when the file has no usable preview. It shows the camera's
/// own JPEG rendering, not the edit.
pub fn embedded_thumbnail(path: &Path, max_edge: u32) -> Result<Option<image::RgbImage>, Error> {
    // A rendered image has no separate preview: callers render it instead.
    if crate::raster::is_raster(path) {
        return Ok(None);
    }
    let src = rawler::rawsource::RawSource::new(path).map_err(|e| Error::Decode(e.to_string()))?;
    let decoder = rawler::get_decoder(&src).map_err(|e| Error::Decode(e.to_string()))?;
    let params = rawler::decoders::RawDecodeParams::default();
    // The larger preview first (sharper when scaled down), then the small thumbnail.
    let found = match decoder.preview_image(&src, &params) {
        Ok(Some(img)) => Some(img),
        _ => decoder.thumbnail_image(&src, &params).ok().flatten(),
    };
    let Some(img) = found.filter(|i| i.width() >= 32 && i.height() >= 32) else {
        return Ok(None);
    };
    let orientation = read_metadata(path).map(|(_, m)| m.orientation).unwrap_or(1);
    let small = img.thumbnail(max_edge.max(16), max_edge.max(16)).to_rgb8();
    use image::imageops::{flip_horizontal, flip_vertical, rotate90, rotate180, rotate270};
    Ok(Some(match orientation {
        2 => flip_horizontal(&small),
        3 => rotate180(&small),
        4 => flip_vertical(&small),
        5 => rotate90(&flip_horizontal(&small)),
        6 => rotate90(&small),
        7 => rotate270(&flip_horizontal(&small)),
        8 => rotate270(&small),
        _ => small,
    }))
}

#[cfg(test)]
mod thumbnail_tests {
    /// Runs on real files when given: AMB_TEST_RAWS=a.nef:b.cr3 (none are in the repo).
    #[test]
    fn embedded_thumbnails_of_real_raws() {
        let Ok(list) = std::env::var("AMB_TEST_RAWS") else {
            return;
        };
        for path in list.split(':') {
            let start = std::time::Instant::now();
            let t = super::embedded_thumbnail(std::path::Path::new(path), 256).unwrap();
            let t = t.unwrap_or_else(|| panic!("{path}: no embedded preview"));
            assert!(
                t.width().max(t.height()) <= 256 && t.width().min(t.height()) > 64,
                "{path}"
            );
            if let Ok(dir) = std::env::var("AMB_TEST_OUT") {
                let name = std::path::Path::new(path)
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .to_string();
                t.save(format!("{dir}/{name}.thumb.png")).unwrap();
            }
            eprintln!(
                "{path}: {}x{} in {:?}",
                t.width(),
                t.height(),
                start.elapsed()
            );
        }
    }
}
