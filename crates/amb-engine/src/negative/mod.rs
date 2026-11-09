// SPDX-License-Identifier: GPL-3.0-or-later
//! Film negative → positive conversion, with film stock looks.
//!
//! Camera Raw has no such feature (photographers use the Negative Lab Pro
//! plug-in), so these settings live in our own `amb:` XMP namespace and work
//! from presets like any other setting.
//!
//! **Model (ours).** On camera RGB of a scanned negative (camera + light
//! source), before white balance:
//!
//! 1. Divide by the **film base** (the orange mask of unexposed film) so the
//!    base becomes 1, and take densities D = −log10(transmittance).
//! 2. Per-channel **levels** in density: D_low → 0 (deepest shadow),
//!    D_high → 1 (brightest highlight). This removes the mask's colour and
//!    balances the three dye layers.
//! 3. The **film stock look** works on these normalised log values: channel
//!    contrasts, dye crosstalk, saturation, shadow/highlight tints, or a
//!    black-and-white channel mix.
//! 4. Back to scene-linear light around a midtone (which lands on 18% grey),
//!    with contrast, toe and shoulder, so the rest of the pipeline (Exposure,
//!    tone, colour, curves, ...) works on an ordinary positive.
//!
//! The film base is a stored value (picked on the photo, or from Freeze or
//! `amb negative analyze`) or estimated from the whole scan, where the rebate
//! between frames is. Auto measures the levels and midtone per image, inside
//! the crop ([`resolve`]); with Auto off they come from stored values or a
//! fixed density range, so frames of a roll print alike. All constants are in
//! [`NegativeStrengths`].

mod analysis;
mod cast;
mod frame;

pub use analysis::{analyze, analyze_as, estimate_base, stock};
use analysis::{auto_density_grade, levels, log_exposure, samples_grid};
use cast::{crossover, remove_cast, uncross};
pub use frame::detect_frame;

use rayon::prelude::*;

use crate::math::smoothstep_f64 as smoothstep;

/// Model constants.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct NegativeStrengths {
    /// Scene range, in decades of exposure, spanned by the levels (contrast 1).
    pub scene_range: f64,
    /// Scene-linear value of the midtone.
    pub midtone_grey: f64,
    /// Film base: fraction of brightest (unclipped) samples whose median is the base.
    pub base_fraction: f64,
    /// Levels: density percentiles taken as deepest shadow and brightest highlight.
    pub level_low: f64,
    pub level_high: f64,
    /// Fraction of each image side ignored by the levels analysis (film holder, rebate).
    pub border: f64,
    /// Colour balance ±100 → offset in normalised log units.
    pub balance_range: f64,
    /// Toe/Shoulder ±100 → change of the compression coefficient.
    pub toe_range: f64,
    pub shoulder_range: f64,
    /// Bare light (perforations, gaps beside the film) is at least this much
    /// density brighter than the film base, in every channel.
    pub light_gap: f64,
    /// On colour negative film, the orange base is at least this much denser
    /// in blue than in red, seen against bare light.
    pub mask_tilt: f64,
    /// Share of bare light's edge that must border film base (holes in the
    /// rebate; a gap along the film).
    pub light_surround: f64,
    /// Auto off without stored levels: shadow density above the base, and
    /// where the midtone sits in the range (the range is `amb:NegativeRange`).
    pub fixed_floor: f64,
    pub fixed_midtone: f64,
    /// Expired film: the levels (of green, 0..1) where red and blue are
    /// matched to green, how far around each the grey axis is read, and the
    /// most a layer may move there.
    pub crossover_at: [f64; 3],
    pub crossover_window: f64,
    pub crossover_limit: f64,
    /// Cast removal: the levels (of green, 0..1) where the greys are read,
    /// how far around each, the least share of a level's pixels a colour
    /// needs to be a candidate grey, how near the line of greys a candidate
    /// must sit, the least share of the frame's pixels the greys must
    /// cover, and the most a layer's lift or slope against green may change.
    pub cast_at: [f64; 5],
    pub cast_window: f64,
    pub cast_share: f64,
    pub cast_tolerance: f64,
    pub cast_coverage: f64,
    pub cast_limit: f64,
    /// Auto Density and Auto Grade: a block has detail when its density
    /// changes at least this much to its neighbours (D), and when it is in
    /// the busier `detail_share` of the frame's blocks.
    pub detail_floor: f64,
    pub detail_share: f64,
    /// Auto Density: how far the midtone moves from `fixed_midtone` to the
    /// detail's median (1 all the way).
    pub density_pull: f64,
    /// Auto Grade: how far the range moves from the film's usual one
    /// (`amb:NegativeRange`) to the detail's, and the levels' percentiles on
    /// the detailed blocks.
    pub grade_pull: f64,
    pub grade_low: f64,
    pub grade_high: f64,
}

impl Default for NegativeStrengths {
    fn default() -> Self {
        DEFAULT_STRENGTHS
    }
}

pub const DEFAULT_STRENGTHS: NegativeStrengths = NegativeStrengths {
    scene_range: 2.2,
    midtone_grey: 0.18,
    base_fraction: 0.003,
    level_low: 0.002,
    level_high: 0.998,
    border: 0.05,
    balance_range: 0.08,
    toe_range: 0.5,
    shoulder_range: 0.5,
    light_gap: 0.1,
    mask_tilt: 0.2,
    light_surround: 0.6,
    fixed_floor: 0.05,
    fixed_midtone: 0.5,
    crossover_at: [0.25, 0.5, 0.75],
    crossover_window: 0.15,
    crossover_limit: 0.18,
    cast_at: [0.15, 0.3, 0.5, 0.7, 0.85],
    cast_window: 0.08,
    cast_share: 0.03,
    cast_tolerance: 0.04,
    cast_coverage: 0.1,
    cast_limit: 0.3,
    detail_floor: 0.02,
    detail_share: 0.5,
    density_pull: 0.7,
    grade_pull: 0.6,
    grade_low: 0.005,
    grade_high: 0.995,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NegativeType {
    Color,
    BlackAndWhite,
    /// Slide (reversal) film: already a positive, so no inversion and no
    /// film base. The same model runs on log transmittance instead of
    /// density, and the levels take out the light source's colour.
    Positive,
}

/// The negative conversion settings (`amb:Negative*`, `amb:FilmBase*`, `amb:FilmStock*`).
#[derive(Debug, Clone, PartialEq)]
pub struct Negative {
    pub enabled: bool,
    pub kind: NegativeType,
    /// Auto: measure levels and midtone for every image, ignoring stored ones.
    pub auto: bool,
    /// Film stock id (see [`STOCKS`]).
    pub stock: String,
    /// How much of the stock look to apply, 0..100.
    pub stock_amount: f64,
    /// The film base; estimated from the scan when missing.
    pub base: Option<[f64; 3]>,
    /// Stored analysis, used when `auto` is off.
    pub density_low: Option<[f64; 3]>,
    pub density_high: Option<[f64; 3]>,
    pub midtone: Option<f64>,
    /// Auto off, levels not stored: the density range above the base (negatives).
    pub range: f64,
    /// Expired film: even out the colour layers' midtones (crossover).
    pub expired: bool,
    /// Stored crossover (used when `auto` is off and `expired` is on).
    pub crossover: Option<Crossover>,
    /// Cast removal along the greys, 0..100: how far measured levels move
    /// so the frame's greys come out grey (stored levels already have it).
    pub cast_removal: f64,
    /// Auto Density: the midtone from the parts with detail, pulled part-way
    /// to a typical print (Auto only).
    pub auto_density: bool,
    /// Auto Grade: the contrast from the parts with detail, pulled part-way
    /// to the film's usual range, with holds for highlights and shadows
    /// (Auto only).
    pub auto_grade: bool,
    /// Print exposure in EV.
    pub exposure: f64,
    /// −100..100 each.
    pub contrast: f64,
    /// Colour balance: Cyan↔Red, Magenta↔Green, Yellow↔Blue (positive = more red/green/blue).
    pub balance: [f64; 3],
    pub toe: f64,
    pub shoulder: f64,
    pub saturation: f64,
}

impl Default for Negative {
    fn default() -> Self {
        Negative {
            enabled: false,
            kind: NegativeType::Color,
            auto: true,
            stock: "neutral".into(),
            stock_amount: 100.0,
            base: None,
            density_low: None,
            density_high: None,
            midtone: None,
            range: DEFAULT_RANGE,
            expired: false,
            crossover: None,
            cast_removal: 0.0,
            auto_density: false,
            auto_grade: false,
            exposure: 0.0,
            contrast: 0.0,
            balance: [0.0; 3],
            toe: 0.0,
            shoulder: 0.0,
            saturation: 0.0,
        }
    }
}

/// `amb:NegativeRange`'s default: the green layer's density range in a
/// normally exposed negative of an ordinary scene (ours; about the median of
/// the BlueNeg frames we tried, docs/design/film-negatives.md).
pub const DEFAULT_RANGE: f64 = 1.0;

/// Expired film's crossover: per layer (red, green, blue), how far its
/// neutral sits from green's at each of [`NegativeStrengths::crossover_at`],
/// in normalised units. Green's row is always zero.
pub type Crossover = [[f64; 3]; 3];

/// No crossover.
pub const NO_CROSSOVER: Crossover = [[0.0; 3]; 3];

/// Per-image analysis: film base transmittance (raw-normalised, white level = 1),
/// per-channel density levels, the midtone (0..1 in normalised log units), and
/// the crossover (none unless the film is expired).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Analysis {
    pub base: [f64; 3],
    pub density_low: [f64; 3],
    pub density_high: [f64; 3],
    pub midtone: f64,
    pub crossover: Crossover,
}

/// An interleaved camera RGB image (a whole scan, or the part being developed).
#[derive(Debug, Clone, Copy)]
pub struct Scan<'a> {
    pub rgb: &'a [f32],
    pub width: usize,
    pub height: usize,
}

/// A film stock look, applied to normalised log values (0 = shadow, 1 = highlight).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stock {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub black_and_white: bool,
    /// Overall contrast (multiplies the scene range).
    pub contrast: f64,
    /// Per-channel contrast around the midtone (colour crossover).
    pub channel_contrast: [f64; 3],
    /// Shadow and highlight compression (0 = straight line).
    pub toe: f64,
    pub shoulder: f64,
    pub saturation: f64,
    /// Dye crosstalk: rows sum to 1, identity = none.
    pub crosstalk: [[f64; 3]; 3],
    /// Colour offsets added in the shadows / highlights (normalised log units).
    pub shadow_tint: [f64; 3],
    pub highlight_tint: [f64; 3],
    /// Black-and-white channel mix (sums to 1).
    pub mix: [f64; 3],
}

const IDENTITY: [[f64; 3]; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
const NEUTRAL_MIX: [f64; 3] = [0.3, 0.55, 0.15];

const NEUTRAL: Stock = Stock {
    id: "neutral",
    name: "Neutral",
    description: "A straight conversion with no look: a starting point for your own edits.",
    black_and_white: false,
    contrast: 1.0,
    channel_contrast: [1.0; 3],
    toe: 0.0,
    shoulder: 0.0,
    saturation: 1.0,
    crosstalk: IDENTITY,
    shadow_tint: [0.0; 3],
    highlight_tint: [0.0; 3],
    mix: NEUTRAL_MIX,
};

/// Built-in film stock looks. Our own designs, described by character, not
/// measurements of any manufacturer's film, and named generically on purpose.
pub const STOCKS: &[Stock] = &[
    NEUTRAL,
    Stock {
        id: "warm-portrait-400",
        name: "Warm Portrait 400",
        description: "Soft contrast, gentle highlights, warm skin and slightly cool shadows.",
        contrast: 0.88,
        channel_contrast: [1.0, 1.0, 0.97],
        toe: 0.1,
        shoulder: 0.25,
        saturation: 0.88,
        crosstalk: [[0.95, 0.04, 0.01], [0.03, 0.94, 0.03], [0.01, 0.05, 0.94]],
        shadow_tint: [-0.01, 0.0, 0.008],
        highlight_tint: [0.015, 0.005, -0.015],
        ..NEUTRAL
    },
    Stock {
        id: "pastel-portrait-160",
        name: "Pastel Portrait 160",
        description: "Low contrast and muted, pastel colour; very forgiving highlights.",
        contrast: 0.82,
        toe: 0.12,
        shoulder: 0.32,
        saturation: 0.8,
        crosstalk: [[0.94, 0.05, 0.01], [0.03, 0.94, 0.03], [0.01, 0.05, 0.94]],
        shadow_tint: [-0.006, 0.004, 0.004],
        highlight_tint: [0.01, 0.006, -0.008],
        ..NEUTRAL
    },
    Stock {
        id: "vivid-100",
        name: "Vivid 100",
        description: "Punchy contrast and saturated colour for landscapes and travel.",
        contrast: 1.12,
        channel_contrast: [1.03, 1.0, 1.02],
        shoulder: 0.15,
        saturation: 1.28,
        highlight_tint: [0.006, 0.0, -0.004],
        ..NEUTRAL
    },
    Stock {
        id: "golden-200",
        name: "Golden 200",
        description: "Classic consumer colour film: warm, golden highlights and lively saturation.",
        contrast: 1.04,
        shoulder: 0.2,
        saturation: 1.12,
        shadow_tint: [0.005, 0.0, -0.01],
        highlight_tint: [0.02, 0.01, -0.02],
        ..NEUTRAL
    },
    Stock {
        id: "everyday-400",
        name: "Everyday 400",
        description: "Cool consumer colour film: green-leaning shadows and crisp, slightly cool tones.",
        contrast: 1.06,
        channel_contrast: [0.98, 1.02, 1.0],
        shoulder: 0.15,
        saturation: 1.06,
        shadow_tint: [-0.01, 0.012, 0.004],
        highlight_tint: [0.0, 0.004, 0.004],
        ..NEUTRAL
    },
    Stock {
        id: "cinema-500t",
        name: "Cinema Tungsten 500T",
        description: "Motion-picture look: low contrast, teal shadows, warm highlights.",
        contrast: 0.9,
        toe: 0.15,
        shoulder: 0.3,
        saturation: 0.95,
        crosstalk: [[0.96, 0.03, 0.01], [0.02, 0.96, 0.02], [0.01, 0.03, 0.96]],
        shadow_tint: [-0.015, 0.0, 0.015],
        highlight_tint: [0.01, 0.0, -0.004],
        ..NEUTRAL
    },
    Stock {
        id: "cinema-50d",
        name: "Cinema Daylight 50D",
        description: "Fine-grained motion-picture look: natural colour, smooth highlight roll-off.",
        contrast: 0.95,
        shoulder: 0.25,
        saturation: 0.98,
        highlight_tint: [0.004, 0.002, -0.004],
        ..NEUTRAL
    },
    Stock {
        id: "bw-classic-400",
        name: "B&W Classic 400",
        description: "Black and white with punchy midtones and a crisp toe.",
        black_and_white: true,
        contrast: 1.12,
        toe: 0.05,
        shoulder: 0.15,
        mix: [0.3, 0.52, 0.18],
        ..NEUTRAL
    },
    Stock {
        id: "bw-fine-100",
        name: "B&W Fine 100",
        description: "Black and white with smooth, long tonality.",
        black_and_white: true,
        contrast: 1.0,
        shoulder: 0.25,
        mix: [0.27, 0.55, 0.18],
        ..NEUTRAL
    },
    Stock {
        id: "bw-high-contrast",
        name: "B&W High Contrast",
        description: "Hard black and white: deep blacks and bright whites.",
        black_and_white: true,
        contrast: 1.35,
        toe: -0.05,
        shoulder: 0.1,
        mix: [0.3, 0.5, 0.2],
        ..NEUTRAL
    },
    Stock {
        id: "bw-red-filter",
        name: "B&W Red Filter",
        description: "Black and white as through a red filter: dark skies, light skin and foliage.",
        black_and_white: true,
        contrast: 1.15,
        shoulder: 0.15,
        mix: [0.65, 0.3, 0.05],
        ..NEUTRAL
    },
    Stock {
        id: "bw-orthochromatic",
        name: "B&W Orthochromatic",
        description: "Black and white blind to red: dark reds and lips, bright skies, vintage feel.",
        black_and_white: true,
        contrast: 1.1,
        shoulder: 0.15,
        mix: [0.0, 0.62, 0.38],
        ..NEUTRAL
    },
];

/// The analysis to render with. `whole` is the whole scan, where the film
/// base is found (the rebate is usually outside the crop); `metered` is the
/// part being developed, where Auto measures the levels and midtone.
///
/// - The film base: the stored one, or estimated from `whole`.
/// - Auto: levels and midtone measured on `metered`.
/// - Auto off: the stored levels and midtone; missing ones come from the
///   fixed density range `n.range` above the base for green, shared out over
///   the layers as measured (each layer's range belongs to the film and the
///   scanner, not the exposure). A slide, which has no base, measures them.
/// - Expired film: the crossover, stored (Auto off) or measured; none otherwise.
pub fn resolve(n: &Negative, whole: Scan, metered: Scan, k: &NegativeStrengths) -> Analysis {
    let positive = n.kind == NegativeType::Positive;
    let base = match n.base {
        Some(b) => b,
        None if positive => [1.0; 3],
        None => estimate_base(whole.rgb, whole.width, whole.height, n.kind, k),
    }
    .map(|b| b.max(1e-4));
    let measured = std::cell::OnceCell::new();
    let grid = || measured.get_or_init(|| samples_grid(metered, base, positive, k));
    let samples = || &grid().0;
    let mut a = if n.auto {
        let a = levels(samples(), base, k);
        if n.auto_density || n.auto_grade {
            auto_density_grade(samples(), grid().1, &a, n, k)
        } else {
            a
        }
    } else {
        let missing = n.density_low.is_none() || n.density_high.is_none() || n.midtone.is_none();
        let fallback = (positive && missing).then(|| levels(samples(), base, k));
        let density_low = n
            .density_low
            .or(fallback.map(|a| a.density_low))
            .unwrap_or([k.fixed_floor; 3]);
        let mut density_high = n
            .density_high
            .or(fallback.map(|a| a.density_high))
            .unwrap_or_else(|| {
                let m = levels(samples(), base, k);
                let green = (m.density_high[1] - m.density_low[1]).max(0.05);
                std::array::from_fn(|c| {
                    let share = (m.density_high[c] - m.density_low[c]) / green;
                    density_low[c] + n.range.max(0.1) * share
                })
            });
        for c in 0..3 {
            density_high[c] = density_high[c].max(density_low[c] + 0.01);
        }
        Analysis {
            base,
            density_low,
            density_high,
            midtone: n
                .midtone
                .or(fallback.map(|a| a.midtone))
                .unwrap_or(k.fixed_midtone)
                .clamp(0.05, 0.95),
            crossover: NO_CROSSOVER,
        }
    };
    let stored = !n.auto && n.density_low.is_some() && n.density_high.is_some();
    if n.cast_removal > 0.0 && n.kind != NegativeType::BlackAndWhite && !stored {
        a = remove_cast(samples(), &a, (n.cast_removal / 100.0).clamp(0.0, 1.0), k);
    }
    if n.expired && n.kind != NegativeType::BlackAndWhite {
        a.crossover = match (n.auto, n.crossover) {
            (false, Some(stored)) => stored,
            _ => crossover(samples(), &a, k),
        };
    }
    a
}

/// Prepared per-image conversion.
pub struct Conversion {
    analysis: Analysis,
    stock: Stock,
    amount: f64,
    black_and_white: bool,
    positive: bool,
    balance: [f64; 3],
    saturation: f64,
    range: f64,
    toe: f64,
    shoulder: f64,
    gain: f64,
    crossover_at: [f64; 3],
}

impl Conversion {
    pub fn new(n: &Negative, a: Analysis, k: &NegativeStrengths) -> Conversion {
        let stock = *stock(&n.stock);
        let amount = (n.stock_amount / 100.0).clamp(0.0, 1.0);
        let blend = |neutral: f64, look: f64| neutral + (look - neutral) * amount;
        Conversion {
            analysis: a,
            stock,
            amount,
            black_and_white: n.kind == NegativeType::BlackAndWhite || stock.black_and_white,
            positive: n.kind == NegativeType::Positive,
            balance: n.balance.map(|b| b / 100.0 * k.balance_range),
            saturation: blend(1.0, stock.saturation) * (1.0 + n.saturation / 100.0).max(0.0),
            range: k.scene_range * blend(1.0, stock.contrast) * (n.contrast / 100.0).exp2(),
            toe: blend(0.0, stock.toe) + n.toe / 100.0 * k.toe_range,
            shoulder: blend(0.0, stock.shoulder) + n.shoulder / 100.0 * k.shoulder_range,
            gain: k.midtone_grey * n.exposure.exp2(),
            crossover_at: k.crossover_at,
        }
    }

    /// Negative camera RGB → positive scene-linear RGB (neutral = equal channels).
    pub fn apply(&self, p: [f32; 3]) -> [f32; 3] {
        let a = &self.analysis;
        let s = &self.stock;
        let m = a.midtone;
        // Normalised log exposure per channel (0 = shadow, 1 = highlight), balanced.
        let neutral: [f64; 3] = std::array::from_fn(|c| {
            let d = log_exposure(f64::from(p[c]), a.base[c], self.positive);
            let v = (d - a.density_low[c]) / (a.density_high[c] - a.density_low[c]);
            uncross(v, a.crossover[c], self.crossover_at) + self.balance[c]
        });
        // Stock look (channel contrast, dye crosstalk, tints), blended by the stock amount.
        let contrasted: [f64; 3] =
            std::array::from_fn(|c| m + (neutral[c] - m) * s.channel_contrast[c]);
        let crossed: [f64; 3] =
            std::array::from_fn(|r| (0..3).map(|c| s.crosstalk[r][c] * contrasted[c]).sum());
        let shade = smoothstep(0.0, 1.0, crossed.iter().sum::<f64>() / 3.0);
        let mut l: [f64; 3] = std::array::from_fn(|c| {
            let look = crossed[c] + s.shadow_tint[c] * (1.0 - shade) + s.highlight_tint[c] * shade;
            neutral[c] + (look - neutral[c]) * self.amount
        });
        if self.black_and_white {
            let mix = if s.black_and_white {
                s.mix
            } else {
                NEUTRAL_MIX
            };
            let y = mix[0] * l[0] + mix[1] * l[1] + mix[2] * l[2];
            l = [y; 3];
        } else {
            let y = (l[0] + l[1] + l[2]) / 3.0;
            for v in &mut l {
                *v = y + (*v - y) * self.saturation;
            }
        }
        l.map(|v| {
            // Log exposure relative to the midtone, with toe/shoulder compression.
            let mut e = (v - m) * self.range;
            if e > 0.0 {
                e /= (1.0 + self.shoulder * e).max(0.2);
            } else {
                e /= (1.0 - self.toe * e).max(0.2);
            }
            (self.gain * 10f64.powf(e)) as f32
        })
    }
}

/// Convert interleaved camera RGB in place.
pub fn apply(rgb: &mut [f32], conversion: &Conversion) {
    rgb.par_chunks_mut(3).for_each(|p| {
        let out = conversion.apply([p[0], p[1], p[2]]);
        p.copy_from_slice(&out);
    });
}

#[cfg(test)]
mod tests;
