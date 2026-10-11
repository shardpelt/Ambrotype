// SPDX-License-Identifier: GPL-3.0-or-later
//! Photo Merge: bracketed raws merged into one floating-point DNG next to
//! them, as Lightroom's Photo Merge > HDR does (docs/design/hdr-merge.md),
//! and overlapping raws stitched into a panorama DNG (Photo Merge >
//! Panorama, docs/design/panorama.md). The merges themselves are
//! `amb_engine::merge` and `amb_engine::pano`; this loads the photos, names
//! the result and writes it.

use std::path::{Path, PathBuf};

use rayon::prelude::*;
use serde::Serialize;

use amb_engine::raw::CameraImage;
use amb_lens::{LcpLibrary, LensfunDb};

pub use amb_engine::merge::{Deghost, HdrOptions};
pub use amb_engine::pano::{PanoOptions, Projection};

/// What a merge made.
#[derive(Debug, Clone, Serialize)]
pub struct HdrReport {
    /// The DNG written.
    pub output: PathBuf,
    /// The photo whose brightness and metadata the merge keeps.
    pub reference: PathBuf,
    /// Each photo's exposure against the reference, in stops, in input order.
    pub stops: Vec<f64>,
    /// Each photo's shift in pixels against the reference.
    pub shifts: Vec<(i32, i32)>,
    /// Share of the photo deghosting took from a single exposure (0..1).
    pub ghost_share: f64,
    pub width: usize,
    pub height: usize,
}

/// Where the merge of `photos` goes by default: `<first>-HDR.dng` next to
/// the first photo (by name), or `-HDR-2`, `-HDR-3` ... when that, or its
/// sidecar, is taken.
pub fn default_output(photos: &[PathBuf]) -> Option<PathBuf> {
    output_named(photos, "HDR")
}

/// [`default_output`] for a panorama: `<first>-Pano.dng`.
pub fn pano_output(photos: &[PathBuf]) -> Option<PathBuf> {
    output_named(photos, "Pano")
}

fn output_named(photos: &[PathBuf], tag: &str) -> Option<PathBuf> {
    let first = photos.iter().min()?;
    let dir = first.parent().unwrap_or(Path::new("."));
    let stem = first.file_stem()?.to_string_lossy();
    (1..)
        .map(|n| {
            let suffix = if n == 1 {
                format!("-{tag}")
            } else {
                format!("-{tag}-{n}")
            };
            dir.join(format!("{stem}{suffix}.dng"))
        })
        .find(|p| !p.exists() && !amb_engine::sidecar_path(p).exists())
}

/// Merge `photos` (raws of one scene at different exposures) into a DNG at
/// `output`, or at [`default_output`].
pub fn hdr(
    photos: &[PathBuf],
    opts: &HdrOptions,
    output: Option<&Path>,
) -> Result<HdrReport, String> {
    if photos.len() < 2 {
        return Err("select at least two photos of the same scene at different exposures".into());
    }
    let output = match output {
        Some(p) => p.to_path_buf(),
        None => default_output(photos).ok_or("no name for the merged photo")?,
    };
    if photos.contains(&output) {
        return Err("the merged photo would replace one of its sources".into());
    }
    let frames = photos
        .par_iter()
        .map(|p| {
            let source = amb_engine::source_path(p);
            amb_engine::raw::load(&source).map_err(|e| format!("{}: {e}", source.display()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let merged = amb_engine::merge::hdr(&frames, opts).map_err(|e| e.to_string())?;
    drop(frames);
    amb_export::dng::write_linear(&output, &merged.image, None).map_err(|e| e.to_string())?;
    Ok(HdrReport {
        output,
        reference: photos[merged.reference].clone(),
        stops: merged.exposures.iter().map(|e| e.log2()).collect(),
        shifts: merged.shifts,
        ghost_share: merged.ghost_share,
        width: merged.image.width,
        height: merged.image.height,
    })
}

/// What a panorama merge made.
#[derive(Debug, Clone, Serialize)]
pub struct PanoReport {
    /// The DNG written.
    pub output: PathBuf,
    /// The photo whose colour and metadata the panorama keeps (the one the
    /// others are placed around).
    pub reference: PathBuf,
    pub projection: Projection,
    /// Field of view covered, degrees across and up.
    pub field: (f64, f64),
    /// Per photo, in input order: the lens profile corrected first (none
    /// when no profile was found), and the brightness gain applied.
    pub lens_profiles: Vec<Option<String>>,
    pub gains: Vec<f64>,
    /// How far matched points land from each other after alignment (pixels).
    pub rms: f64,
    /// Overlapping photos (input indices) and their matched points.
    pub overlaps: Vec<(usize, usize, usize)>,
    pub width: usize,
    pub height: usize,
}

/// Where to look for lens profiles; with `correct`, frames with one are
/// corrected for distortion, vignetting and colour fringes before
/// stitching. The lensfun database also gives cameras' crop factors, which
/// turn EXIF focal lengths into a first guess for alignment.
#[derive(Clone, Copy, Default)]
pub struct Lenses<'a> {
    pub lcp: Option<&'a LcpLibrary>,
    pub db: Option<&'a LensfunDb>,
    pub correct: bool,
}

/// Stitch `photos` (overlapping raws from one camera and lens, turned about
/// the lens) into a DNG at `output`, or at [`pano_output`].
pub fn pano(
    photos: &[PathBuf],
    opts: &PanoOptions,
    output: Option<&Path>,
    lenses: Lenses,
) -> Result<PanoReport, String> {
    if photos.len() < 2 {
        return Err("select at least two overlapping photos".into());
    }
    let output = match output {
        Some(p) => p.to_path_buf(),
        None => pano_output(photos).ok_or("no name for the panorama")?,
    };
    if photos.contains(&output) {
        return Err("the panorama would replace one of its sources".into());
    }
    let (frames, lens_profiles): (Vec<_>, Vec<_>) = photos
        .par_iter()
        .map(|p| {
            let source = amb_engine::source_path(p);
            let mut frame =
                amb_engine::raw::load(&source).map_err(|e| format!("{}: {e}", source.display()))?;
            let profile = correct_lens(&mut frame, lenses);
            upright(&mut frame);
            Ok((frame, profile))
        })
        .collect::<Result<Vec<_>, String>>()?
        .into_iter()
        .unzip();
    let made = amb_engine::pano::stitch(&frames, opts).map_err(|e| e.to_string())?;
    drop(frames);
    let mut image = made.image;
    // The frames are corrected already, and no lens drew the panorama's
    // geometry: a lens profile must not find it again.
    for m in [
        &mut image.metadata.lens_make,
        &mut image.metadata.lens_model,
    ] {
        *m = None;
    }
    image.metadata.focal_length = None;
    image.shot.lens_make = None;
    image.shot.lens_model = None;
    image.shot.focal_mm = None;
    amb_export::dng::write_linear(&output, &image, None).map_err(|e| e.to_string())?;
    Ok(PanoReport {
        output,
        reference: photos[made.reference].clone(),
        projection: made.projection,
        field: made.field,
        lens_profiles,
        gains: made.gains,
        rms: made.rms,
        overlaps: made.links,
        width: image.width,
        height: image.height,
    })
}

/// Fill in the frame's crop factor, and apply its lens profile if one is
/// found (zoomed so no blank edges remain). Returns the profile's name.
fn correct_lens(frame: &mut CameraImage, lenses: Lenses) -> Option<String> {
    use amb_engine::lens::{self, LensCorrections, LensStrengths};
    if frame.shot.crop_factor.is_none() {
        frame.shot.crop_factor = lenses
            .db
            .and_then(|db| db.camera_crop(&frame.shot.camera_make, &frame.shot.camera_model));
    }
    if frame.rendered || !lenses.correct {
        return None;
    }
    let hint = amb_lens::ProfileHint {
        name: None,
        filename: None,
    };
    let profile = amb_lens::resolve(&frame.shot, &hint, lenses.lcp, lenses.db)?;
    let name = profile.description.clone();
    let lc = LensCorrections {
        profile_enable: true,
        profile: Some(profile),
        constrain_crop: true,
        ..LensCorrections::default()
    };
    let strengths = LensStrengths::default();
    let pass = lens::apply_camera(
        &frame.rgb,
        frame.width,
        frame.height,
        frame.shot.crop_factor,
        &lc,
        &strengths,
    );
    frame.rgb = pass.rgb;
    Some(name)
}

/// Turn a frame as its EXIF orientation says, so stitching sees it upright.
fn upright(frame: &mut CameraImage) {
    use amb_export::geometry::{oriented_size, stored_position};
    let o = frame.metadata.orientation;
    if !(2..=8).contains(&o) {
        return;
    }
    let (w, h) = (frame.width, frame.height);
    let (uw, uh) = oriented_size(w, h, o);
    let mut rgb = vec![0.0f32; uw * uh * 3];
    rgb.par_chunks_mut(uw * 3).enumerate().for_each(|(y, row)| {
        for x in 0..uw {
            let (sx, sy) = stored_position(o, w, h, x, y);
            let s = (sy * w + sx) * 3;
            row[x * 3..x * 3 + 3].copy_from_slice(&frame.rgb[s..s + 3]);
        }
    });
    frame.rgb = rgb;
    frame.width = uw;
    frame.height = uh;
    frame.metadata.orientation = 1;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_the_merge_after_the_first_photo_without_overwriting() {
        let dir = std::env::temp_dir().join(format!("amb-merge-name-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let photos = vec![dir.join("DSC_0012.NEF"), dir.join("DSC_0011.NEF")];
        assert_eq!(
            default_output(&photos).unwrap(),
            dir.join("DSC_0011-HDR.dng")
        );
        std::fs::write(dir.join("DSC_0011-HDR.dng"), b"").unwrap();
        std::fs::write(dir.join("DSC_0011-HDR-2.xmp"), b"").unwrap();
        assert_eq!(
            default_output(&photos).unwrap(),
            dir.join("DSC_0011-HDR-3.dng")
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn merges_brackets_into_a_dng_that_opens_as_a_raw() {
        let dir = std::env::temp_dir().join(format!("amb-merge-hdr-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // The same synthetic target at three exposures (the DNG writer
        // stores values to 1, so brighter frames clip).
        let base = amb_testkit::patterns::generate("colorchecker").unwrap();
        let mut photos = Vec::new();
        for (i, gain) in [1.0f32, 4.0, 0.25].iter().enumerate() {
            let mut img = amb_testkit::dng::LinearImage::new(base.width, base.height);
            for (o, v) in img.data.iter_mut().zip(&base.data) {
                *o = (v * gain).min(1.0);
            }
            let path = dir.join(format!("bracket-{i}.dng"));
            amb_testkit::dng::write(&path, &img, &Default::default()).unwrap();
            photos.push(path);
        }
        let report = hdr(&photos, &HdrOptions::default(), None).unwrap();
        assert_eq!(report.output, dir.join("bracket-0-HDR.dng"));
        assert_eq!(report.reference, photos[0], "the middle exposure");
        assert!((report.stops[1] - 2.0).abs() < 0.05, "{:?}", report.stops);
        assert!((report.stops[2] + 2.0).abs() < 0.05, "{:?}", report.stops);
        let merged = amb_engine::raw::load(&report.output).unwrap();
        assert_eq!(
            (merged.width, merged.height),
            (base.width as usize, base.height as usize)
        );
        // Equal to the reference where it is well exposed.
        let k = (100 * merged.width + 100) * 3 + 1;
        assert!(
            (merged.rgb[k] - base.data[k]).abs() < 0.01,
            "{} vs {}",
            merged.rgb[k],
            base.data[k]
        );
        assert!(hdr(&photos[..1], &HdrOptions::default(), None).is_err());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn stitches_a_sweep_into_a_dng_with_no_lens() {
        use amb_engine::pano::testing::{photograph, turn};
        use amb_export::geometry::{oriented_size, stored_position};
        let dir = std::env::temp_dir().join(format!("amb-merge-pano-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut photos = Vec::new();
        for (i, yaw) in [-25.0, 0.0, 25.0].into_iter().enumerate() {
            let mut frame = photograph(&turn(yaw, 0.0, 0.0), 560.0, 640, 427);
            frame.metadata.lens_model = Some("35mm f/1.8".into());
            frame.metadata.focal_length = Some((35, 1));
            if i == 1 {
                // Stored turned, as a camera held on its side writes it.
                let (w, h) = (frame.width, frame.height);
                let (sw, sh) = oriented_size(w, h, 6);
                let mut stored = vec![0.0; w * h * 3];
                for y in 0..h {
                    for x in 0..w {
                        let (sx, sy) = stored_position(6, sw, sh, x, y);
                        let (s, u) = ((sy * sw + sx) * 3, (y * w + x) * 3);
                        stored[s..s + 3].copy_from_slice(&frame.rgb[u..u + 3]);
                    }
                }
                frame = frame.with_pixels(stored, sw, sh);
                frame.metadata.orientation = 6;
            }
            let path = dir.join(format!("pano-{i}.dng"));
            amb_export::dng::write_linear(&path, &frame, None).unwrap();
            photos.push(path);
        }
        let report = pano(&photos, &PanoOptions::default(), None, Lenses::default()).unwrap();
        assert_eq!(report.output, dir.join("pano-0-Pano.dng"));
        assert_eq!(report.reference, photos[1], "the middle photo");
        assert_eq!(report.overlaps.len(), 2, "{:?}", report.overlaps);
        assert!(report.rms < 1.5, "rms {}", report.rms);
        assert!((report.field.0 - 109.0).abs() < 5.0, "{:?}", report.field);
        assert_eq!(report.projection, Projection::Cylindrical);
        assert!(
            report.width > 2 * report.height,
            "{} x {}",
            report.width,
            report.height
        );
        let merged = amb_engine::raw::load(&report.output).unwrap();
        assert_eq!((merged.width, merged.height), (report.width, report.height));
        assert_eq!(merged.metadata.orientation, 1);
        assert_eq!(
            merged.metadata.lens_model, None,
            "no lens profile may match"
        );
        assert!(
            pano(
                &photos[..1],
                &PanoOptions::default(),
                None,
                Lenses::default()
            )
            .is_err()
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}
