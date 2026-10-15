// SPDX-License-Identifier: GPL-3.0-or-later
//! The fonts for text on exports (watermarks, border captions), bundled
//! with their licences in `data/fonts/` (credited in NOTICE).

/// id, name shown, the font file.
pub const FONTS: &[(&str, &str, &[u8])] = &[
    (
        "sans",
        "Sans",
        include_bytes!("../data/fonts/FreeSansBold.ttf"),
    ),
    (
        "handwritten",
        "Handwritten",
        include_bytes!("../data/fonts/Caveat.ttf"),
    ),
    (
        "pen",
        "Pen",
        include_bytes!("../data/fonts/ReenieBeanie.ttf"),
    ),
    (
        "marker",
        "Marker",
        include_bytes!("../data/fonts/PermanentMarker.ttf"),
    ),
    (
        "typewriter",
        "Typewriter",
        include_bytes!("../data/fonts/SpecialElite.ttf"),
    ),
    // Digits and a few letters only, like a film camera's date imprint.
    (
        "datestamp",
        "Date stamp",
        include_bytes!("../data/fonts/DSEG7Classic-Bold.ttf"),
    ),
];

pub const DEFAULT: &str = "sans";

/// The font file for `id` (the default for an unknown or empty one).
pub fn bytes(id: &str) -> &'static [u8] {
    FONTS
        .iter()
        .find(|(i, _, _)| *i == id)
        .unwrap_or(&FONTS[0])
        .2
}

pub fn is_known(id: &str) -> bool {
    id.is_empty() || FONTS.iter().any(|(i, _, _)| *i == id)
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_font_sets_text() {
        for (id, _, _) in super::FONTS {
            let text = if *id == "datestamp" {
                "'26 10 15"
            } else {
                "Summer '26"
            };
            let (cov, w, h) = crate::watermark::set_text(id, text, 48.0);
            assert!(w > 60 && h > 15, "{id}: {w}x{h}");
            assert!(
                cov.iter().filter(|c| **c > 0.5).count() > 200,
                "{id}: too little ink"
            );
        }
    }
}
