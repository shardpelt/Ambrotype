// SPDX-License-Identifier: GPL-3.0-or-later
//! The darkroom readouts of a film negative, from the same stage a render
//! uses ([`camera_stage`]), so they read what the print is made from.

use super::*;

/// What the negative readouts say about a scan.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct Readout {
    pub stats: negative::ScanStats,
    /// The density and zone at the point asked for.
    pub probe: Option<negative::Probe>,
}

/// The range and clipping of `img` as `s` converts it, and, with `at` (a
/// point of the cropped photo in its stored orientation, as fractions), the
/// density and zone there. `None` when the conversion is off. Converts the
/// whole of `img`: pass a small copy ([`crate::raw::downscale`]).
pub fn negative_readout(
    img: &CameraImage,
    s: &DevelopSettings,
    at: Option<[f64; 2]>,
) -> Option<Readout> {
    if !s.negative.enabled {
        return None;
    }
    let stage = camera_stage(img, s, &Pass::default());
    let a = stage.negative?;
    let (rgb, width, height) = match &stage.lens_pass {
        Some(p) => (&p.rgb, p.width, p.height),
        None => (&img.rgb, img.width, img.height),
    };
    let scan = negative::Scan { rgb, width, height };
    let drawn = negative::sub_scan(scan, s.negative.meter);
    let metered = match &drawn {
        Some((rgb, width, height)) => negative::Scan {
            rgb,
            width: *width,
            height: *height,
        },
        None => scan,
    };
    let cal = &s.calibration.negative;
    let probe = at.map(|at| {
        let conversion = negative::Conversion::new(&s.negative, a, cal);
        negative::probe(scan, &a, &conversion, at)
    });
    Some(Readout {
        stats: negative::scan_stats(metered, &a),
        probe,
    })
}
