// SPDX-License-Identifier: GPL-3.0-or-later
//! TIFF files: LZW-compressed (with the horizontal predictor for whole
//! numbers; LZW is in baseline TIFF 6, so every reader has it), with the ICC
//! profile, and the EXIF in an Exif directory (and a GPS one when the
//! location goes too), as Lightroom's TIFFs carry it.

use std::io::{Cursor, Seek, Write};

use tiff::encoder::colortype::{ColorType, RGB8, RGB16, RGB32Float};
use tiff::encoder::{Compression, DirectoryEncoder, Rational, TiffEncoder, TiffKind, TiffValue};
use tiff::tags::{Predictor, ResolutionUnit, Tag, Type};

use crate::exif::{Dirs, Entry};

/// The samples, interleaved RGB.
pub(crate) enum Samples {
    U8(Vec<u8>),
    U16(Vec<u16>),
    F32(Vec<f32>),
}

/// Fields the image directory writes itself, and the JPEG-only pixel size.
const SKIP: [u16; 5] = [0x011A, 0x011B, 0x0128, 0xA002, 0xA003];

const ASCII: u16 = 2;

pub(crate) fn encode(
    samples: &Samples,
    width: u32,
    height: u32,
    icc: &[u8],
    exif: Option<&Dirs>,
) -> Result<Vec<u8>, String> {
    let mut buf = Cursor::new(Vec::new());
    let e = |e: tiff::TiffError| e.to_string();
    let compression = Compression::Lzw;
    match samples {
        Samples::U8(v) => write::<RGB8, _>(
            TiffEncoder::new(&mut buf)
                .map_err(e)?
                .with_compression(compression)
                .with_predictor(Predictor::Horizontal),
            width,
            height,
            v,
            icc,
            exif,
        ),
        Samples::U16(v) => write::<RGB16, _>(
            TiffEncoder::new(&mut buf)
                .map_err(e)?
                .with_compression(compression)
                .with_predictor(Predictor::Horizontal),
            width,
            height,
            v,
            icc,
            exif,
        ),
        Samples::F32(v) => write::<RGB32Float, _>(
            TiffEncoder::new(&mut buf)
                .map_err(e)?
                .with_compression(compression),
            width,
            height,
            v,
            icc,
            exif,
        ),
    }
    .map_err(e)?;
    // The directory's values follow the compressed strips, so may start on
    // an odd offset, which TIFF asks not to.
    let bytes = buf.into_inner();
    Ok(crate::embed::tiff_with_entries(&bytes, &[]).unwrap_or(bytes))
}

/// Add `entries` (little-endian values, as our EXIF writes them) to a
/// directory being written.
fn put<W: Write + Seek, K: TiffKind>(
    d: &mut DirectoryEncoder<'_, W, K>,
    entries: &[Entry],
) -> tiff::TiffResult<()> {
    let mut dir = tiff::Directory::empty();
    for e in entries.iter().filter(|e| !SKIP.contains(&e.tag)) {
        let Some(ty) = Type::from_u16(e.kind) else {
            continue;
        };
        // Keep the values that follow on even offsets: a second NUL ends
        // an odd-length text as well.
        let mut data = e.data.clone();
        if e.kind == ASCII && data.len() > 4 && data.len() % 2 == 1 {
            data.push(0);
        }
        let entry = d.write_entry_bytes(ty, &data)?;
        dir.extend([(Tag::from_u16_exhaustive(e.tag), entry)]);
    }
    d.extend_from(&dir);
    Ok(())
}

fn write<C: ColorType, W: Write + Seek>(
    mut enc: TiffEncoder<W>,
    width: u32,
    height: u32,
    data: &[C::Inner],
    icc: &[u8],
    exif: Option<&Dirs>,
) -> tiff::TiffResult<()>
where
    [C::Inner]: TiffValue,
{
    // Our EXIF values are little-endian: only for a file that is too.
    let exif = exif.filter(|_| cfg!(target_endian = "little"));
    // The Exif and GPS directories first, not in the chain of images.
    let mut pointers = Vec::new();
    if let Some(dirs) = exif {
        for (tag, entries) in [
            (Tag::ExifDirectory, &dirs.exif),
            (Tag::GpsDirectory, &dirs.gps),
        ] {
            if entries.is_empty() {
                continue;
            }
            let mut d = enc.extra_directory()?;
            put(&mut d, entries)?;
            pointers.push((tag, d.finish_with_offsets()?.offset));
        }
    }
    let mut image = enc.new_image::<C>(width, height)?;
    image.resolution(
        ResolutionUnit::Inch,
        Rational {
            n: crate::exif::RESOLUTION_PPI,
            d: 1,
        },
    );
    let d = image.encoder();
    if !icc.is_empty() {
        // UNDEFINED, as the ICC specification asks (not BYTE).
        let entry = d.write_entry_bytes(Type::UNDEFINED, icc)?;
        let mut dir = tiff::Directory::empty();
        dir.extend([(Tag::IccProfile, entry)]);
        d.extend_from(&dir);
    }
    if let Some(dirs) = exif {
        put(d, &dirs.ifd0)?;
    }
    for (tag, offset) in pointers {
        d.write_tag(tag, offset)?;
    }
    image.write_data(data)
}
