// SPDX-License-Identifier: GPL-3.0-or-later
//! Raw decoding into linear camera RGB, via `rawler` (pure Rust).
//!
//! rawler handles container parsing, decompression and black/white scaling;
//! demosaicing is ours: RCD for Bayer sensors ([`crate::rcd`]) and
//! Markesteijn for X-Trans ([`crate::xtrans`]), with rawler's for anything
//! else. Colour (white balance, camera matrix) is done by our pipeline.

use std::path::Path;

use amb_color::matrix::{Mat3, apply3, invert3, xy_to_xyz};
use rawler::imgop::develop::{Intermediate, ProcessingStep, RawDevelop};
use rawler::imgop::xyz::Illuminant;
use rayon::prelude::*;

use crate::{Error, dng_opcodes};

/// Linear camera-space image, scaled so the sensor white level is 1.0.
pub struct CameraImage {
    pub width: usize,
    pub height: usize,
    /// Interleaved RGB, row-major.
    pub rgb: Vec<f32>,
    /// XYZ → camera matrix (D65 calibration preferred).
    pub xyz_to_cam: Mat3,
    /// The camera's matrices for each of its calibration illuminants
    /// (temperature in kelvin, XYZ → camera), coolest first. With two, the
    /// white balance picks the mix, as the DNG specification says
    /// ([`CameraImage::xyz_to_cam_at`]); otherwise `xyz_to_cam` is used.
    pub matrices: Vec<(f64, Mat3)>,
    /// Camera RGB of the as-shot white (green normalised to 1).
    pub as_shot_neutral: [f64; 3],
    /// Level at which each channel clips ([`clip_levels`]); infinity when it doesn't.
    pub clip: [f32; 3],
    /// Partly clipped pixels were rebuilt ([`crate::highlights`]): only
    /// pixels with every channel clipped are made neutral.
    pub highlights_rebuilt: bool,
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

/// Every colour matrix rawler has for the camera with its illuminant's
/// temperature, coolest first.
fn calibration_matrices(raw: &rawler::RawImage) -> Vec<(f64, Mat3)> {
    let mut out: Vec<(f64, Mat3)> = raw
        .color_matrix
        .iter()
        .filter_map(|(illuminant, flat)| {
            let temp = crate::profile::illuminant_temperature(*illuminant as u16)?;
            if flat.len() < 9 || flat.iter().any(|v| !v.is_finite()) {
                return None;
            }
            let m: Mat3 =
                std::array::from_fn(|r| std::array::from_fn(|c| f64::from(flat[r * 3 + c])));
            let det = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
                - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
                + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
            (det.abs() > 1e-9).then_some((temp, m))
        })
        .collect();
    out.sort_by(|a, b| a.0.total_cmp(&b.0));
    out.dedup_by(|a, b| a.0 == b.0);
    out
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
    let matrices = calibration_matrices(&raw);

    let develop = RawDevelop::new_with(&[
        ProcessingStep::Rescale,
        ProcessingStep::Demosaic,
        ProcessingStep::FujiRotate,
        ProcessingStep::CropActiveArea,
        ProcessingStep::CropDefault,
    ]);
    let developed = match develop_xtrans(&raw)? {
        Some(xtrans) => xtrans,
        None => match develop_bayer(&raw)? {
            Some(bayer) => bayer,
            None => developed(
                develop
                    .develop_intermediate(&raw)
                    .map_err(|e| Error::Decode(e.to_string()))?,
            )?,
        },
    };
    let (width, height, mut rgb) = developed;
    // A DNG's own corrections: lens shading now, its lens warp and vignette
    // as the photo's built-in lens correction (applied in the lens pass).
    let opcodes = read_opcodes(path);
    let placement = placement(&raw, width, height);
    let gain_maps: Vec<&dng_opcodes::GainMap> = opcodes
        .iter()
        .filter_map(|o| match o {
            dng_opcodes::Opcode::GainMap(m) => Some(m),
            _ => None,
        })
        .collect();
    if !gain_maps.is_empty() {
        use rawler::imgop::sensor::SensorType;
        use rawler::rawimage::RawPhotometricInterpretation as P;
        let (ax, ay) = raw.active_area.map_or((0, 0), |a| (a.p.x, a.p.y));
        match &raw.photometric {
            P::Cfa(c) if c.sensor == SensorType::Bayer => {
                let colour = |r: usize, col: usize| c.cfa.color_at(r + ay, col + ax);
                dng_opcodes::apply_gain_maps(
                    &mut rgb,
                    width,
                    height,
                    &gain_maps,
                    placement,
                    Some(&colour),
                );
            }
            P::LinearRaw => {
                dng_opcodes::apply_gain_maps(&mut rgb, width, height, &gain_maps, placement, None)
            }
            _ => {}
        }
    }
    let embedded = dng_opcodes::correction(&opcodes, placement, width, height);

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

    let (mut shot, mut metadata) = read_metadata(path).unwrap_or_else(|_| {
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

    shot.embedded = embedded;
    // Some formats only carry the orientation in the decoder's own field.
    if metadata.orientation == 1 {
        metadata.orientation = orientation_code(raw.orientation);
    }

    let clip = clip_levels(&rgb);
    // Channels that clipped where others didn't, rebuilt from those others.
    let rebuilt = crate::highlights::reconstruct(&mut rgb, width, height, clip, as_shot_neutral);
    Ok(CameraImage {
        width,
        height,
        rgb,
        clip,
        rendered: false,
        xyz_to_cam,
        matrices,
        highlights_rebuilt: rebuilt > 0,
        as_shot_neutral,
        make: raw.clean_make.clone(),
        model: raw.clean_model.clone(),
        shot,
        metadata,
    })
}

/// The opcodes in a DNG's `OpcodeList2` and `OpcodeList3`, in that order
/// (none for other raws, or when they can't be read).
fn read_opcodes(path: &Path) -> Vec<dng_opcodes::Opcode> {
    use rawler::decoders::WellKnownIFD;
    use rawler::formats::tiff::Value;
    use rawler::tags::DngTag;
    let read = || -> Option<Vec<dng_opcodes::Opcode>> {
        let src = rawler::rawsource::RawSource::new(path).ok()?;
        let decoder = rawler::get_decoder(&src).ok()?;
        let ifd = decoder.ifd(WellKnownIFD::VirtualDngRawTags).ok()??;
        let mut out = Vec::new();
        for tag in [DngTag::OpcodeList2, DngTag::OpcodeList3] {
            if let Some(Value::Undefined(b) | Value::Byte(b)) = ifd.get_entry(tag).map(|e| &e.value)
            {
                out.extend(dng_opcodes::parse(b));
            }
        }
        Some(out)
    };
    std::panic::catch_unwind(read)
        .ok()
        .flatten()
        .unwrap_or_default()
}

/// Where the developed photo sits in the image DNG opcodes describe (the
/// active area), from the default crop rawler applied.
fn placement(raw: &rawler::RawImage, width: usize, height: usize) -> dng_opcodes::Placement {
    let stage = raw
        .active_area
        .map_or((raw.width, raw.height), |a| (a.d.w, a.d.h));
    let origin = match (raw.crop_area, raw.active_area) {
        (Some(c), Some(a)) => (c.p.x.saturating_sub(a.p.x), c.p.y.saturating_sub(a.p.y)),
        (Some(c), None) => (c.p.x, c.p.y),
        _ => (0, 0),
    };
    // Something else cropped it (an X-Trans rotation): assume centred.
    if origin.0 + width > stage.0 || origin.1 + height > stage.1 {
        return dng_opcodes::Placement {
            stage,
            origin: (
                stage.0.saturating_sub(width) / 2,
                stage.1.saturating_sub(height) / 2,
            ),
        };
    }
    dng_opcodes::Placement { stage, origin }
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

/// A sensor's samples, its width and height, and its colour pattern.
type Mosaic = (Vec<f32>, usize, usize, rawler::CFA);

/// The sensor's mosaic, black/white scaled, cut to its active area, with
/// the colour pattern from the area's top-left; `None` for a sensor of
/// another kind.
fn mosaic(
    raw: &rawler::RawImage,
    sensor: rawler::imgop::sensor::SensorType,
) -> Result<Option<Mosaic>, Error> {
    use rawler::rawimage::RawPhotometricInterpretation;
    let RawPhotometricInterpretation::Cfa(config) = &raw.photometric else {
        return Ok(None);
    };
    if config.sensor != sensor
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
    let (w, h) = (roi.d.w, roi.d.h);
    let mut samples = Vec::with_capacity(w * h);
    for y in 0..h {
        let start = (roi.p.y + y) * mosaic.width + roi.p.x;
        samples.extend_from_slice(&mosaic.data[start..start + w]);
    }
    Ok(Some((samples, w, h, cfa)))
}

/// The default crop rawler would apply after the active area, on a
/// developed `w × h` image.
fn crop_default(raw: &rawler::RawImage, w: usize, h: usize, mut rgb: Vec<f32>) -> Developed {
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
            return (crop.d.w, crop.d.h, rgb);
        }
    }
    (w, h, rgb)
}

/// X-Trans sensors: our Markesteijn demosaic instead of rawler's bilinear
/// one, then the same default crop rawler would apply.
fn develop_xtrans(raw: &rawler::RawImage) -> Result<Option<Developed>, Error> {
    let Some((samples, w, h, cfa)) = mosaic(raw, rawler::imgop::sensor::SensorType::Xtrans)? else {
        return Ok(None);
    };
    let pattern: crate::xtrans::Pattern =
        std::array::from_fn(|r| std::array::from_fn(|c| cfa.color_at(r, c) as u8));
    let rgb = crate::xtrans::demosaic(&samples, w, h, &pattern);
    drop(samples);
    Ok(Some(crop_default(raw, w, h, rgb)))
}

/// Bayer sensors: RCD instead of rawler's PPG, worked at the as-shot white
/// balance, then the same default crop rawler would apply.
fn develop_bayer(raw: &rawler::RawImage) -> Result<Option<Developed>, Error> {
    let Some((samples, w, h, cfa)) = mosaic(raw, rawler::imgop::sensor::SensorType::Bayer)? else {
        return Ok(None);
    };
    let pattern: crate::rcd::Pattern =
        std::array::from_fn(|r| std::array::from_fn(|c| cfa.color_at(r, c) as u8));
    // rawler's multipliers, green 1; none known: as they are.
    let c = raw.wb_coeffs;
    let wb = if c[..3].iter().all(|v| v.is_finite() && *v > 0.0) {
        [c[0] / c[1], 1.0, c[2] / c[1]]
    } else {
        [1.0; 3]
    };
    let rgb = crate::rcd::demosaic(&samples, w, h, &pattern, wb);
    drop(samples);
    Ok(Some(crop_default(raw, w, h, rgb)))
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
            matrices: self.matrices.clone(),
            highlights_rebuilt: self.highlights_rebuilt,
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
        embedded: None,
    };
    Ok((shot, metadata))
}

/// Camera RGB of a white with chromaticity (x, y), green normalised to 1.
impl CameraImage {
    /// XYZ → camera for a white of `temp` kelvin: the two calibration
    /// illuminants' matrices mixed by inverse temperature (the cooler one's
    /// alone at or below its temperature), or the one matrix there is.
    pub fn xyz_to_cam_at(&self, temp: f64) -> Mat3 {
        let (Some(lo), Some(hi)) = (self.matrices.first(), self.matrices.last()) else {
            return self.xyz_to_cam;
        };
        if self.matrices.len() < 2 || lo.0 >= hi.0 {
            return self.xyz_to_cam;
        }
        let inv = |t: f64| 1.0 / t.max(1.0);
        let w = ((inv(temp) - inv(hi.0)) / (inv(lo.0) - inv(hi.0))).clamp(0.0, 1.0);
        std::array::from_fn(|r| std::array::from_fn(|c| w * lo.1[r][c] + (1.0 - w) * hi.1[r][c]))
    }

    /// Camera RGB of a white with chromaticity (x, y), green = 1.
    pub fn neutral_for(&self, x: f64, y: f64) -> [f64; 3] {
        let temp = amb_color::temperature::xy_to_temp_tint(x, y).temperature;
        neutral_for_xy(&self.xyz_to_cam_at(temp), x, y)
    }

    /// Chromaticity of a camera-space white. The matrix depends on it, so
    /// it is found by iteration, as the DNG specification describes.
    pub fn white_xy(&self, neutral: [f64; 3]) -> (f64, f64) {
        if self.matrices.len() < 2 {
            return xy_for_neutral(&self.xyz_to_cam, neutral);
        }
        let (mut x, mut y) = (0.3457, 0.3585);
        for _ in 0..30 {
            let temp = amb_color::temperature::xy_to_temp_tint(x, y).temperature;
            let (nx, ny) = xy_for_neutral(&self.xyz_to_cam_at(temp), neutral);
            if !(nx.is_finite() && ny.is_finite()) {
                break;
            }
            let done = (nx - x).abs() + (ny - y).abs() < 1e-7;
            (x, y) = (nx, ny);
            if done {
                break;
            }
        }
        (x, y)
    }

    /// Camera RGB → linear ProPhoto for white balance `neutral`, with the
    /// matrix for that white.
    pub fn to_prophoto(&self, neutral: [f64; 3]) -> Mat3 {
        let (x, y) = self.white_xy(neutral);
        let temp = amb_color::temperature::xy_to_temp_tint(x, y).temperature;
        crate::pipeline::camera_to_prophoto(&self.xyz_to_cam_at(temp), neutral)
    }
}

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
/// own JPEG rendering, not the edit. A JPEG, PNG, TIFF or WebP is decoded
/// small instead ([`crate::raster::quick_image`]).
pub fn embedded_thumbnail(path: &Path, max_edge: u32) -> Result<Option<image::RgbImage>, Error> {
    // A rendered image is its own preview, decoded small.
    let small = if crate::raster::is_raster(path) {
        crate::raster::quick_image(path, max_edge.max(16))?
    } else {
        let src =
            rawler::rawsource::RawSource::new(path).map_err(|e| Error::Decode(e.to_string()))?;
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
        img.thumbnail(max_edge.max(16), max_edge.max(16)).to_rgb8()
    };
    let orientation = read_metadata(path).map(|(_, m)| m.orientation).unwrap_or(1);
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

#[cfg(test)]
mod matrix_tests {
    use super::*;

    /// A camera with an illuminant-A and a D65 matrix (made up, but shaped
    /// like a real one's).
    fn camera() -> CameraImage {
        let d65: Mat3 = [
            [0.70, -0.20, -0.05],
            [-0.40, 1.20, 0.20],
            [-0.08, 0.15, 0.60],
        ];
        let a: Mat3 = [
            [0.85, -0.30, -0.02],
            [-0.45, 1.25, 0.22],
            [-0.05, 0.25, 0.75],
        ];
        CameraImage {
            width: 1,
            height: 1,
            rgb: vec![0.5; 3],
            xyz_to_cam: d65,
            matrices: vec![(2856.0, a), (6504.0, d65)],
            highlights_rebuilt: false,
            as_shot_neutral: [1.0; 3],
            clip: [f32::INFINITY; 3],
            rendered: false,
            make: "Test".into(),
            model: "Two matrices".into(),
            shot: Default::default(),
            metadata: Default::default(),
        }
    }

    #[test]
    fn the_matrix_follows_the_white() {
        let img = camera();
        assert_eq!(img.xyz_to_cam_at(2000.0), img.matrices[0].1);
        assert_eq!(img.xyz_to_cam_at(2856.0), img.matrices[0].1);
        assert_eq!(img.xyz_to_cam_at(9000.0), img.matrices[1].1);
        // Halfway in inverse temperature, halfway between the matrices.
        let mid = 2.0 / (1.0 / 2856.0 + 1.0 / 6504.0);
        let m = img.xyz_to_cam_at(mid);
        assert!((m[0][0] - 0.775).abs() < 1e-9, "{}", m[0][0]);
    }

    #[test]
    fn whites_round_trip_through_the_mixed_matrix() {
        let img = camera();
        for (t, tint) in [(2900.0, 0.0), (4300.0, 10.0), (6500.0, -5.0)] {
            let (x, y) = amb_color::temperature::temp_tint_to_xy(t, tint);
            let n = img.neutral_for(x, y);
            let (bx, by) = img.white_xy(n);
            assert!(
                (bx - x).abs() < 1e-5 && (by - y).abs() < 1e-5,
                "{t}: {bx},{by} vs {x},{y}"
            );
            // That white comes out neutral.
            let m = img.to_prophoto(n);
            let out = apply3(&m, n);
            assert!(
                (out[0] - out[1]).abs() < 1e-6 && (out[2] - out[1]).abs() < 1e-6,
                "{out:?}"
            );
        }
        // One matrix: as before.
        let mut one = camera();
        one.matrices.clear();
        let n = [0.5, 1.0, 0.7];
        assert_eq!(one.white_xy(n), xy_for_neutral(&one.xyz_to_cam, n));
    }
}
