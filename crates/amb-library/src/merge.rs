// SPDX-License-Identifier: GPL-3.0-or-later
//! Photo Merge: bracketed raws merged into one floating-point DNG next to
//! them, as Lightroom's Photo Merge > HDR does (docs/design/hdr-merge.md).
//! The merge itself is `amb_engine::merge`; this loads the photos, names the
//! result and writes it.

use std::path::{Path, PathBuf};

use rayon::prelude::*;
use serde::Serialize;

pub use amb_engine::merge::{Deghost, HdrOptions};

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
    let first = photos.iter().min()?;
    let dir = first.parent().unwrap_or(Path::new("."));
    let stem = first.file_stem()?.to_string_lossy();
    (1..)
        .map(|n| {
            let suffix = if n == 1 {
                "-HDR".to_string()
            } else {
                format!("-HDR-{n}")
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
}
