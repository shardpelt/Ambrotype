// SPDX-License-Identifier: GPL-3.0-or-later
//! Dust and hairs on a film scan, found on the whole raw and turned into
//! heal spots (`add_spot`): round ones for specks, brushed ones along hairs,
//! each with the donor the search picked. They are Lightroom's own
//! `RetouchAreas`, so they undo, show and edit like spots made by hand, and
//! Lightroom reads them. Defects under a spot already there are left out,
//! so a second run adds only new ones (docs/design/dust.md).

use std::collections::BTreeMap;
use std::path::Path;

use amb_engine::dust::{self, DEFAULT_DUST, Defect};
use amb_engine::raw::CameraImage;
use amb_engine::retouch::{SpotShape, Spots};
use amb_presets::Value;
use amb_protocol::Command;

/// The spots to add, and how many of each kind.
#[derive(Debug, Default)]
pub struct Found {
    pub commands: Vec<Command>,
    pub specks: usize,
    pub hairs: usize,
    /// Defects a spot already covers.
    pub covered: usize,
}

/// How much softer than the defect the spot is (0-100), and how much
/// larger, in pixels: a speck's soft edge and the scanner's blur.
const FEATHER: f64 = 30.0;
const MARGIN: f64 = 1.5;

/// Find dust on a photo's raw, with its edit's spots.
pub fn find_file(photo: &Path) -> Result<Found, String> {
    let source = amb_engine::source_path(photo);
    let img = amb_engine::raw::load(&source).map_err(|e| format!("{}: {e}", source.display()))?;
    let settings = std::fs::read_to_string(amb_engine::sidecar_path(photo))
        .ok()
        .and_then(|t| amb_presets::xmp::parse_crs(&t).ok())
        .unwrap_or_default();
    Ok(find_image(&img, &settings))
}

/// Find dust on a decoded raw (the whole scan, as stored: dust and hairs are
/// darker there, on a negative and a slide alike).
pub fn find_image(img: &CameraImage, settings: &BTreeMap<String, Value>) -> Found {
    let defects = dust::find(&img.rgb, img.width, img.height, true, &DEFAULT_DUST);
    spots_for(&defects, img.width, img.height, settings)
}

/// `add_spot` commands for `defects` (pixels of a `w`×`h` image), leaving out
/// those an existing spot in `settings` covers.
pub fn spots_for(
    defects: &[Defect],
    w: usize,
    h: usize,
    settings: &BTreeMap<String, Value>,
) -> Found {
    let (wf, hf) = (w as f64, h as f64);
    // Existing spots as circles: centre (pixels) and radius (pixels).
    let mut taken: Vec<((f64, f64), f64)> = Vec::new();
    for spot in Spots::from_settings(settings).0.spots {
        match spot.shape {
            SpotShape::Ellipse { centre, size } => {
                taken.push(((centre.0 * wf, centre.1 * hf), size.0.max(size.1) * wf));
            }
            SpotShape::Brush(strokes) => {
                for d in strokes.iter().flat_map(|s| &s.dabs) {
                    taken.push(((d.x * wf, d.y * hf), d.radius * wf));
                }
            }
        }
    }
    let mut found = Found::default();
    for d in defects {
        let covered = d.path.iter().any(|p| {
            taken
                .iter()
                .any(|(c, r)| (p.0 - c.0).hypot(p.1 - c.1) <= r + d.radius)
        });
        if covered {
            found.covered += 1;
            continue;
        }
        if d.is_hair() {
            found.hairs += 1;
        } else {
            found.specks += 1;
        }
        let command = spot_command(d, w, h);
        found.commands.push(command);
        let r = d.radius + MARGIN;
        taken.extend(d.path.iter().map(|&p| (p, r)));
    }
    found
}

/// The heal spot for one defect (pixels of a `w`×`h` image): round for a
/// speck, brushed along a hair or a line.
pub fn spot_command(d: &Defect, w: usize, h: usize) -> Command {
    let (wf, hf) = (w as f64, h as f64);
    let radius = (d.radius + MARGIN) / wf;
    let frac = |p: (f64, f64)| [(p.0 + 0.5) / wf, (p.1 + 0.5) / hf];
    if d.is_hair() {
        // A brushed spot copies its stroke's box centre from `source`.
        let (x0, x1) = d
            .path
            .iter()
            .fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p.0), b.max(p.0)));
        let (y0, y1) = d
            .path
            .iter()
            .fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p.1), b.max(p.1)));
        let middle = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
        Command::AddSpot {
            at: frac(middle),
            radius,
            kind: Some("heal".into()),
            source: Some(frac((middle.0 + d.donor.0, middle.1 + d.donor.1))),
            source_auto: true,
            feather: FEATHER,
            opacity: 100.0,
            points: Some(d.path.iter().map(|&p| frac(p)).collect()),
        }
    } else {
        Command::AddSpot {
            at: frac(d.centre),
            radius,
            kind: Some("heal".into()),
            source: Some(frac((d.centre.0 + d.donor.0, d.centre.1 + d.donor.1))),
            source_auto: true,
            feather: FEATHER,
            opacity: 100.0,
            points: None,
        }
    }
}

/// A straight scratch along a line from `from` to `to` (fractions of the
/// photo): the line moved onto the scratch, as brushed heal spots, one per
/// piece.
pub fn line_image(img: &CameraImage, from: [f64; 2], to: [f64; 2]) -> Vec<Command> {
    let (wf, hf) = (img.width as f64, img.height as f64);
    let px = |p: [f64; 2]| (p[0] * wf - 0.5, p[1] * hf - 0.5);
    dust::line(&img.rgb, img.width, img.height, px(from), px(to))
        .iter()
        .map(|d| spot_command(d, img.width, img.height))
        .collect()
}

/// [`line_image`] on a photo's raw.
pub fn line_file(photo: &Path, from: [f64; 2], to: [f64; 2]) -> Result<Vec<Command>, String> {
    let source = amb_engine::source_path(photo);
    let img = amb_engine::raw::load(&source).map_err(|e| format!("{}: {e}", source.display()))?;
    Ok(line_image(&img, from, to))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn speck(x: f64, y: f64) -> Defect {
        Defect {
            centre: (x, y),
            radius: 2.0,
            path: vec![(x, y)],
            donor: (8.0, 0.0),
            contrast: 1.0,
            pixels: Vec::new(),
        }
    }

    #[test]
    fn specks_become_round_spots_and_hairs_brushed_ones() {
        let hair = Defect {
            centre: (50.0, 40.0),
            radius: 2.0,
            path: vec![(40.0, 40.0), (60.0, 42.0)],
            donor: (0.0, 10.0),
            contrast: 1.0,
            pixels: Vec::new(),
        };
        let found = spots_for(&[speck(10.0, 20.0), hair], 200, 100, &BTreeMap::new());
        assert_eq!((found.specks, found.hairs, found.covered), (1, 1, 0));
        let Command::AddSpot {
            at,
            radius,
            source,
            points,
            ..
        } = &found.commands[0]
        else {
            panic!("{:?}", found.commands[0]);
        };
        assert!((at[0] - 10.5 / 200.0).abs() < 1e-9 && (at[1] - 20.5 / 100.0).abs() < 1e-9);
        assert!((radius - 3.5 / 200.0).abs() < 1e-9);
        assert!((source.unwrap()[0] - 18.5 / 200.0).abs() < 1e-9);
        assert!(points.is_none());
        let Command::AddSpot { points, source, .. } = &found.commands[1] else {
            panic!();
        };
        assert_eq!(points.as_ref().map(Vec::len), Some(2));
        assert!(
            (source.unwrap()[1] - 51.5 / 100.0).abs() < 1e-9,
            "{source:?}"
        );
    }

    #[test]
    fn a_speck_under_a_spot_is_left_alone() {
        let mut settings = BTreeMap::new();
        let first = spots_for(&[speck(10.0, 20.0)], 200, 100, &settings);
        let mut state = amb_protocol::DevelopState::from_settings(settings.clone());
        state.execute_all(&first.commands).unwrap();
        settings = state.settings().clone();
        let again = spots_for(
            &[speck(10.0, 20.0), speck(150.0, 80.0)],
            200,
            100,
            &settings,
        );
        assert_eq!((again.specks, again.covered), (1, 1));
    }
}
