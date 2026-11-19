// SPDX-License-Identifier: GPL-3.0-or-later
//! The previews cameras embed in raw files, and the raw's size.

use super::metadata::read_metadata;
use crate::Error;
use std::path::Path;

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
        let Some(img) = embedded_jpeg(path, false)? else {
            return Ok(None);
        };
        img.thumbnail(max_edge.max(16), max_edge.max(16)).to_rgb8()
    };
    Ok(Some(upright(path, small)))
}

/// The camera's own JPEG inside a raw, as large as it stores one, upright:
/// what the camera would have saved had it been set to JPEG (often full
/// size; on some cameras smaller). `None` when it keeps none.
/// The raw's sensor size in pixels, read without decoding it (None for a
/// raster file or what can't be read).
pub fn raw_size(path: &Path) -> Option<(usize, usize)> {
    if crate::raster::is_raster(path) {
        return None;
    }
    let src = rawler::rawsource::RawSource::new(path).ok()?;
    let decoder = rawler::get_decoder(&src).ok()?;
    let img = decoder
        .raw_image(&src, &rawler::decoders::RawDecodeParams::default(), true)
        .ok()?;
    Some((img.width, img.height))
}

pub fn embedded_preview(path: &Path) -> Result<Option<image::RgbImage>, Error> {
    Ok(embedded_jpeg(path, true)?.map(|img| upright(path, img.to_rgb8())))
}

/// The larger preview a raw keeps (sharper when scaled down), else its small thumbnail.
/// `largest` also looks for a full-size JPEG rawler doesn't offer (slow to
/// decode: not for thumbnails).
fn embedded_jpeg(path: &Path, largest: bool) -> Result<Option<image::DynamicImage>, Error> {
    let src = rawler::rawsource::RawSource::new(path).map_err(|e| Error::Decode(e.to_string()))?;
    let decoder = rawler::get_decoder(&src).map_err(|e| Error::Decode(e.to_string()))?;
    let params = rawler::decoders::RawDecodeParams::default();
    let found = match decoder.preview_image(&src, &params) {
        Ok(Some(img)) => Some(img),
        _ => decoder.thumbnail_image(&src, &params).ok().flatten(),
    };
    // A larger JPEG in the file's IFDs (Sony's full-size one).
    let larger = largest
        .then(|| crate::sony::largest_jpeg(path))
        .flatten()
        .and_then(|b| image::load_from_memory_with_format(&b, image::ImageFormat::Jpeg).ok())
        .filter(|j| {
            found.as_ref().is_none_or(|f| {
                u64::from(j.width()) * u64::from(j.height())
                    > u64::from(f.width()) * u64::from(f.height())
            })
        });
    Ok(larger
        .or(found)
        .filter(|i| i.width() >= 32 && i.height() >= 32))
}

/// An embedded preview turned as the raw says it was held, or as its
/// sidecar says once it was turned in the library.
fn upright(path: &Path, small: image::RgbImage) -> image::RgbImage {
    let orientation = crate::orientation::from_sidecar(path)
        .unwrap_or_else(|| read_metadata(path).map(|(_, m)| m.orientation).unwrap_or(1));
    use image::imageops::{flip_horizontal, flip_vertical, rotate90, rotate180, rotate270};
    match orientation {
        2 => flip_horizontal(&small),
        3 => rotate180(&small),
        4 => flip_vertical(&small),
        // As amb_export::geometry::orient: 5 mirrors then turns 270°, 7 90°.
        5 => rotate270(&flip_horizontal(&small)),
        6 => rotate90(&small),
        7 => rotate90(&flip_horizontal(&small)),
        8 => rotate270(&small),
        _ => small,
    }
}
