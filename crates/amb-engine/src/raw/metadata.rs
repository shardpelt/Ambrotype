// SPDX-License-Identifier: GPL-3.0-or-later
//! What a raw file says about the shot: exposure, lens, place, colour matrices.

use crate::Error;
use amb_color::matrix::Mat3;
use rawler::imgop::xyz::Illuminant;
use std::path::Path;

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
pub(super) fn calibration_matrices(raw: &rawler::RawImage) -> Vec<(f64, Mat3)> {
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

pub(super) fn color_matrix(raw: &rawler::RawImage) -> Result<Mat3, Error> {
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
