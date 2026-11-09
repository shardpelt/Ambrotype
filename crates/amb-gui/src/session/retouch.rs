// SPDX-License-Identifier: GPL-3.0-or-later
//! Retouching on the open photo: dust, object selection, scratches and heal sources.

use super::*;

impl Session {
    /// Dust and hairs on the whole scan, as heal spots to add
    /// (`amb_library::dust`), leaving out what the edit's spots cover.
    pub fn find_dust(&self) -> amb_library::dust::Found {
        amb_library::dust::find_image(&self.full, self.state.settings())
    }

    /// Select Object: the object `target` points at, with its shadow if
    /// `shadow` (and one was found), kept for its actions.
    pub fn select_object(
        &mut self,
        target: &amb_library::object::Target,
        shadow: bool,
    ) -> Result<&amb_library::object::Selection, String> {
        let mut sel = amb_library::object::select(&self.full, self.state.settings(), target)?;
        sel.set_shadow(shadow);
        Ok(self.selection.insert(sel))
    }

    /// Select Object by words: the thing `like` scores best, kept for its
    /// actions (amb_library::object::select_words).
    pub fn select_words(
        &mut self,
        like: &amb_library::object::Like<'_>,
    ) -> Result<&amb_library::object::Selection, String> {
        let sel = amb_library::object::select_words(&self.full, self.state.settings(), like)?;
        Ok(self.selection.insert(sel))
    }

    /// Remove object: the spot covering the object clicked or boxed
    /// (fractions of the frame).
    pub fn remove_object(
        &self,
        target: amb_library::object::Target,
    ) -> Result<amb_library::object::Object, String> {
        amb_library::object::find(&self.full, self.state.settings(), target)
    }

    /// Heal spots along a straight scratch drawn from `from` to `to`
    /// (fractions): the line moved onto the scratch, in pieces.
    pub fn scratch_spots(&self, from: [f64; 2], to: [f64; 2]) -> Vec<amb_protocol::Command> {
        amb_library::dust::line_image(&self.full, from, to)
    }

    /// A donor for a new spot at `(x, y)` with `radius` (fractions; the radius
    /// of the width): the engine's search on a small render of the photo.
    pub fn spot_source(&self, x: f64, y: f64, radius: f64) -> (f64, f64) {
        let (develop, _) = DevelopSettings::from_settings(self.state.settings());
        let src = downscale(&self.full, 1024);
        let out = pipeline::render(&src, &develop);
        let crop = develop.crop.geometry(src.width, src.height);
        amb_engine::retouch::find_source(
            &out,
            (src.width, src.height),
            crop.as_ref(),
            (x, y),
            radius,
        )
    }

    /// A donor for a brushed spot along `points` (frame fractions) with the
    /// brush's `radius` and `feather` (0-100), searched for the stroke's
    /// own shape: the centre the stroke's centre copies from.
    pub fn stroke_source(
        &self,
        points: &[[f64; 2]],
        radius: f64,
        feather: f64,
    ) -> Result<(f64, f64), String> {
        let (develop, _) = DevelopSettings::from_settings(self.state.settings());
        // The stroke as add_spot would store it, read back as the engine does.
        let mut area = BTreeMap::new();
        let spot = amb_protocol::spots::NewSpot {
            kind: None,
            at: points.first().copied().unwrap_or([0.5, 0.5]),
            radius,
            source: None,
            source_auto: true,
            feather,
            opacity: 100.0,
            points: Some(points),
            separate: false,
        };
        amb_protocol::spots::add(&mut area, &spot, &mut Vec::new()).map_err(|e| e.to_string())?;
        let (spots, _) = amb_engine::retouch::Spots::from_settings(&area);
        let spot = spots.spots.first().ok_or("the stroke is empty")?;
        let src = downscale(&self.full, 1024);
        let out = pipeline::render(&src, &develop);
        let crop = develop.crop.geometry(src.width, src.height);
        amb_engine::retouch::find_spot_source(&out, (src.width, src.height), crop.as_ref(), spot)
            .ok_or_else(|| "the stroke is outside the photo".into())
    }
}
