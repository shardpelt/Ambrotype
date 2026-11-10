// SPDX-License-Identifier: GPL-3.0-or-later
//! A photo's orientation (EXIF 1–8): the camera's, or the one its sidecar
//! keeps once it was turned in the library (`tiff:Orientation`, as Lightroom
//! writes it). Pixels, crops and masks stay in the stored (sensor)
//! orientation; only what is shown and exported is turned.

use std::path::Path;

/// Each EXIF orientation as (mirrored, quarter turns clockwise): mirrored
/// left to right first, then turned.
const PARTS: [(bool, u8); 8] = [
    (false, 0),
    (true, 0),
    (false, 2),
    (true, 2),
    (true, 3),
    (false, 1),
    (true, 1),
    (false, 3),
];

/// `code` turned `quarters` clockwise (negative: anticlockwise), then, with
/// `flip`, mirrored left to right, as the photo is seen upright.
pub fn turned(code: u16, quarters: i32, flip: bool) -> u16 {
    let (mut mirrored, turns) = PARTS[usize::from(code.clamp(1, 8)) - 1];
    let mut turns = (i32::from(turns) + quarters).rem_euclid(4);
    if flip {
        // A mirror turns the other way: H·R(r) = R(−r)·H.
        mirrored = !mirrored;
        turns = (4 - turns) % 4;
    }
    let at = PARTS.iter().position(|&p| p == (mirrored, turns as u8));
    at.map_or(1, |i| i as u16 + 1)
}

/// The orientation the photo's sidecar keeps, if it was turned.
pub fn from_sidecar(photo: &Path) -> Option<u16> {
    crate::sidecar_text(photo)
        .as_deref()
        .and_then(amb_presets::xmp::tiff_orientation)
}

/// The camera's orientation, from the file's header (1 when unknown).
pub fn camera(photo: &Path) -> u16 {
    crate::raw::read_metadata(&crate::source_path(photo)).map_or(1, |(_, m)| m.orientation)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turns_and_flips_compose_as_exif_says() {
        assert_eq!(turned(1, 1, false), 6);
        assert_eq!(turned(1, -1, false), 8);
        assert_eq!(turned(6, 1, false), 3);
        assert_eq!(turned(6, -1, false), 1);
        assert_eq!(turned(1, 0, true), 2);
        assert_eq!(turned(6, 0, true), 5);
        for code in 1..=8 {
            assert_eq!(turned(code, 4, false), code);
            assert_eq!(turned(turned(code, 0, true), 0, true), code);
            assert_eq!(turned(turned(code, 1, false), -1, false), code);
        }
    }
}
