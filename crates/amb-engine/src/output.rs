// SPDX-License-Identifier: GPL-3.0-or-later
//! Output: linear ProPhoto → display-referred files.

use std::path::Path;

use amb_color::matrix::{PROPHOTO_TO_XYZ_D50, mul3, xyz_d50_to_srgb};
use image::{ImageBuffer, Rgb};
use rayon::prelude::*;

use crate::Error;
use crate::pipeline::ProPhotoImage;

fn srgb_oetf(v: f32) -> f32 {
    let v = v.clamp(0.0, 1.0);
    if v <= 0.003_130_8 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    }
}

/// Convert to sRGB-encoded values in 0..1 (hard clip; gamut mapping comes later).
pub fn to_srgb(img: &ProPhotoImage) -> Vec<f32> {
    let m = mul3(&xyz_d50_to_srgb(), &PROPHOTO_TO_XYZ_D50).map(|row| row.map(|v| v as f32));
    let mut out = vec![0.0f32; img.rgb.len()];
    out.par_chunks_mut(3)
        .zip(img.rgb.par_chunks(3))
        .for_each(|(o, p)| {
            for (row, slot) in m.iter().zip(o.iter_mut()) {
                *slot = srgb_oetf(row[0] * p[0] + row[1] * p[1] + row[2] * p[2]);
            }
        });
    out
}

/// Write as sRGB: 8-bit for .png/.jpg, 16-bit for .tif/.tiff.
pub fn write_srgb(img: &ProPhotoImage, path: &Path) -> Result<(), Error> {
    let encoded = to_srgb(img);
    let (w, h) = (img.width as u32, img.height as u32);
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let result = match ext.as_str() {
        "tif" | "tiff" => {
            let data: Vec<u16> = encoded
                .iter()
                .map(|v| (v * 65535.0).round() as u16)
                .collect();
            ImageBuffer::<Rgb<u16>, _>::from_raw(w, h, data)
                .ok_or_else(|| Error::Output("buffer size mismatch".into()))?
                .save(path)
        }
        "png" | "jpg" | "jpeg" => {
            let data: Vec<u8> = encoded.iter().map(|v| (v * 255.0).round() as u8).collect();
            ImageBuffer::<Rgb<u8>, _>::from_raw(w, h, data)
                .ok_or_else(|| Error::Output("buffer size mismatch".into()))?
                .save(path)
        }
        other => {
            return Err(Error::Output(format!(
                "unsupported output format {other:?} (use .png, .jpg or .tif)"
            )));
        }
    };
    result.map_err(|e| Error::Output(e.to_string()))
}
