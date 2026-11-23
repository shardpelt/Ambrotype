// SPDX-License-Identifier: GPL-3.0-or-later
//! The grain and glow each film stock brings when picked (`set_film_stock`).
//!
//! The stocks themselves (their colour and tone) are in `amb-engine`'s
//! `negative::STOCKS`; a test there checks both lists have the same ids.
//! Kept here so the protocol can read it without the engine.

/// In the units of the settings `GrainAmount`, `GrainSize`, `GrainFrequency`,
/// `amb:Halation` and `amb:Bloom` (0-100). Our own values, set by each
/// stock's description, not measured from any film.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Texture {
    pub grain_amount: f64,
    pub grain_size: f64,
    pub grain_roughness: f64,
    pub halation: f64,
    pub bloom: f64,
}

/// One row per stock id. Black-and-white stocks have no halation: ours is
/// the red-layer glow of colour film.
pub const TEXTURES: &[(&str, Texture)] = &[
    (
        "neutral",
        Texture {
            grain_amount: 0.0,
            grain_size: 25.0,
            grain_roughness: 50.0,
            halation: 0.0,
            bloom: 0.0,
        },
    ),
    (
        "warm-portrait-400",
        Texture {
            grain_amount: 22.0,
            grain_size: 30.0,
            grain_roughness: 50.0,
            halation: 8.0,
            bloom: 10.0,
        },
    ),
    (
        "pastel-portrait-160",
        Texture {
            grain_amount: 14.0,
            grain_size: 22.0,
            grain_roughness: 45.0,
            halation: 5.0,
            bloom: 12.0,
        },
    ),
    (
        "vivid-100",
        Texture {
            grain_amount: 8.0,
            grain_size: 18.0,
            grain_roughness: 40.0,
            halation: 4.0,
            bloom: 4.0,
        },
    ),
    (
        "golden-200",
        Texture {
            grain_amount: 18.0,
            grain_size: 26.0,
            grain_roughness: 50.0,
            halation: 8.0,
            bloom: 8.0,
        },
    ),
    (
        "everyday-400",
        Texture {
            grain_amount: 25.0,
            grain_size: 32.0,
            grain_roughness: 55.0,
            halation: 8.0,
            bloom: 6.0,
        },
    ),
    (
        "cinema-500t",
        Texture {
            grain_amount: 28.0,
            grain_size: 34.0,
            grain_roughness: 55.0,
            halation: 30.0,
            bloom: 10.0,
        },
    ),
    (
        "cinema-50d",
        Texture {
            grain_amount: 6.0,
            grain_size: 16.0,
            grain_roughness: 40.0,
            halation: 12.0,
            bloom: 4.0,
        },
    ),
    (
        "bw-classic-400",
        Texture {
            grain_amount: 30.0,
            grain_size: 32.0,
            grain_roughness: 60.0,
            halation: 0.0,
            bloom: 0.0,
        },
    ),
    (
        "bw-fine-100",
        Texture {
            grain_amount: 12.0,
            grain_size: 18.0,
            grain_roughness: 45.0,
            halation: 0.0,
            bloom: 0.0,
        },
    ),
    (
        "bw-high-contrast",
        Texture {
            grain_amount: 35.0,
            grain_size: 30.0,
            grain_roughness: 65.0,
            halation: 0.0,
            bloom: 4.0,
        },
    ),
    (
        "bw-red-filter",
        Texture {
            grain_amount: 28.0,
            grain_size: 30.0,
            grain_roughness: 60.0,
            halation: 0.0,
            bloom: 0.0,
        },
    ),
    (
        "bw-orthochromatic",
        Texture {
            grain_amount: 20.0,
            grain_size: 24.0,
            grain_roughness: 55.0,
            halation: 0.0,
            bloom: 6.0,
        },
    ),
];

/// The texture of a stock id (case-insensitive), or `None` for an unknown id.
pub fn texture(id: &str) -> Option<Texture> {
    TEXTURES
        .iter()
        .find(|(i, _)| i.eq_ignore_ascii_case(id.trim()))
        .map(|(_, t)| *t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn textures_are_in_range_and_neutral_has_none() {
        for (id, t) in TEXTURES {
            for v in [
                t.grain_amount,
                t.grain_size,
                t.grain_roughness,
                t.halation,
                t.bloom,
            ] {
                assert!((0.0..=100.0).contains(&v), "{id}: {v}");
            }
            if id.starts_with("bw-") {
                assert_eq!(t.halation, 0.0, "{id}: B&W has no halation");
            }
        }
        let n = texture("neutral").unwrap();
        assert_eq!((n.grain_amount, n.halation, n.bloom), (0.0, 0.0, 0.0));
        assert!(texture("nope").is_none());
    }
}
