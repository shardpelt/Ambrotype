//! Raw decoding into linear camera RGB, via `rawler` (pure Rust).
//!
//! rawler handles container parsing, decompression, black/white scaling and
//! demosaicing. Colour (white balance, camera matrix) is done by our pipeline.

use std::path::Path;

use flb_color::matrix::{Mat3, apply3, invert3, xy_to_xyz};
use rawler::imgop::develop::{Intermediate, ProcessingStep, RawDevelop};
use rawler::imgop::xyz::Illuminant;

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
    pub make: String,
    pub model: String,
    /// Lens and exposure metadata for lens-profile matching.
    pub shot: flb_lens::ShotInfo,
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

pub fn load(path: &Path) -> Result<CameraImage, Error> {
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
    let intermediate = develop
        .develop_intermediate(&raw)
        .map_err(|e| Error::Decode(e.to_string()))?;
    let (width, height, rgb) = match intermediate {
        Intermediate::ThreeColor(pixels) => (pixels.dim().w, pixels.dim().h, pixels.flatten()),
        Intermediate::Monochrome(pixels) => {
            let rgb = pixels.data.iter().flat_map(|v| [*v, *v, *v]).collect();
            (pixels.dim().w, pixels.dim().h, rgb)
        }
        Intermediate::FourColor(_) => {
            return Err(Error::Unsupported(
                "four-colour images are not supported yet".into(),
            ));
        }
    };

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

    let shot = read_shot(path).unwrap_or_else(|_| flb_lens::ShotInfo {
        camera_make: raw.clean_make.clone(),
        camera_model: raw.clean_model.clone(),
        ..Default::default()
    });

    Ok(CameraImage {
        width,
        height,
        rgb,
        xyz_to_cam,
        as_shot_neutral,
        make: raw.clean_make.clone(),
        model: raw.clean_model.clone(),
        shot,
    })
}

/// Camera and lens metadata of a raw file, without decoding the image.
/// Missing fields stay `None`.
pub fn read_shot(path: &Path) -> Result<flb_lens::ShotInfo, Error> {
    let src = rawler::rawsource::RawSource::new(path).map_err(|e| Error::Decode(e.to_string()))?;
    let decoder = rawler::get_decoder(&src).map_err(|e| Error::Decode(e.to_string()))?;
    let md = decoder
        .raw_metadata(&src, &rawler::decoders::RawDecodeParams::default())
        .map_err(|e| Error::Decode(e.to_string()))?;
    let ratio = |r: Option<rawler::formats::tiff::Rational>| {
        r.filter(|r| r.d != 0)
            .map(|r| f64::from(r.n) / f64::from(r.d))
            .filter(|v| v.is_finite() && *v > 0.0)
    };
    let non_empty = |s: &Option<String>| s.clone().filter(|s| !s.trim().is_empty());
    let (lens_make, lens_model) = match &md.lens {
        Some(lens) => (Some(lens.lens_make.clone()), Some(lens.lens_model.clone())),
        None => (
            non_empty(&md.exif.lens_make),
            non_empty(&md.exif.lens_model),
        ),
    };
    Ok(flb_lens::ShotInfo {
        camera_make: md.make.clone(),
        camera_model: md.model.clone(),
        lens_make,
        lens_model,
        focal_mm: ratio(md.exif.focal_length),
        aperture: ratio(md.exif.fnumber),
        distance_m: ratio(md.exif.subject_distance),
        crop_factor: None,
    })
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
