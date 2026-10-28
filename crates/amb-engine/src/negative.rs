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

use rayon::prelude::*;

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

/// Look up a stock by id (case-insensitive); unknown ids fall back to Neutral.
pub fn stock(id: &str) -> &'static Stock {
    STOCKS
        .iter()
        .find(|s| s.id.eq_ignore_ascii_case(id.trim()))
        .unwrap_or(&STOCKS[0])
}

/// Block-averaged copy of an interleaved RGB image (about `target` samples).
fn downsample(rgb: &[f32], w: usize, h: usize, target: usize) -> (Vec<[f64; 3]>, usize, usize) {
    let block = (((w * h) as f64 / target as f64).sqrt().round() as usize).max(1);
    let (bw, bh) = ((w / block).max(1), (h / block).max(1));
    let blocks: Vec<[f64; 3]> = (0..bw * bh)
        .into_par_iter()
        .map(|i| {
            let (bx, by) = (i % bw, i / bw);
            let mut acc = [0.0f64; 3];
            let mut n = 0.0f64;
            for y in by * block..((by + 1) * block).min(h) {
                for x in bx * block..((bx + 1) * block).min(w) {
                    let p = &rgb[(y * w + x) * 3..(y * w + x) * 3 + 3];
                    for c in 0..3 {
                        acc[c] += f64::from(p[c]);
                    }
                    n += 1.0;
                }
            }
            acc.map(|v| v / n.max(1.0))
        })
        .collect();
    (blocks, bw, bh)
}

fn percentile(values: &mut [f64], q: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let i = ((values.len() - 1) as f64 * q.clamp(0.0, 1.0)).round() as usize;
    *values.select_nth_unstable_by(i, f64::total_cmp).1
}

fn density(v: f64, base: f64) -> f64 {
    -(v.max(1e-6) / base.max(1e-6)).log10()
}

/// What the levels work on: density for a negative (more = brighter
/// scene), log transmittance for a slide (the same direction).
fn log_exposure(v: f64, base: f64, positive: bool) -> f64 {
    if positive {
        -density(v, 1.0)
    } else {
        density(v, base)
    }
}

/// The per-channel median of the first `n` of `order` (brightest first).
fn top_median(blocks: &[[f64; 3]], order: &[usize], n: usize) -> [f64; 3] {
    let top = &order[..n.clamp(1, order.len())];
    std::array::from_fn(|c| {
        let mut v: Vec<f64> = top.iter().map(|&i| blocks[i][c]).collect();
        percentile(&mut v, 0.5).max(1e-4)
    })
}

/// Estimate the film base from the scan: the brightest unclipped film (the
/// rebate between frames, or the thinnest shadows).
///
/// **Ours.** Film can't be brighter than its base, so a brighter area is bare
/// light: sprocket holes, or a gap between the film and the holder, when the
/// scan didn't clip them. The brightest area counts as bare light when a flat
/// area at least `light_gap` denser in every channel borders most of its edge
/// (holes sit inside the rebate), and on colour film that area is orange
/// against it (denser in blue than in red by `mask_tilt`). The base is then
/// that area. A dark picture edge next to the rebate is neither all around it
/// nor orange against it.
pub fn estimate_base(
    rgb: &[f32],
    w: usize,
    h: usize,
    kind: NegativeType,
    k: &NegativeStrengths,
) -> [f64; 3] {
    let (blocks, bw, bh) = downsample(rgb, w, h, 250_000);
    let usable = |p: &[f64; 3]| p.iter().all(|v| *v < 0.97 && *v > 0.0);
    let mut order: Vec<usize> = (0..blocks.len()).filter(|&i| usable(&blocks[i])).collect();
    if order.is_empty() {
        order = (0..blocks.len()).collect();
    }
    let brightness = |i: usize| -> f64 { blocks[i].iter().map(|v| v.max(1e-6).ln()).sum() };
    order.sort_by(|&a, &b| brightness(b).total_cmp(&brightness(a)));
    let n = (order.len() as f64 * k.base_fraction).ceil() as usize;
    let brightest = top_median(&blocks, &order, n);

    let film: Vec<usize> = order
        .iter()
        .copied()
        .filter(|&i| (0..3).all(|c| density(blocks[i][c], brightest[c]) >= k.light_gap))
        .collect();
    if film.len() < n {
        return brightest;
    }
    let below = top_median(&blocks, &film, n);
    let gap: [f64; 3] = std::array::from_fn(|c| density(below[c], brightest[c]));
    if kind == NegativeType::Color && gap[2] - gap[0] < k.mask_tilt {
        return brightest;
    }
    // 1 = like the brightest area, 2 = like the area below it.
    let near = |p: &[f64; 3], to: &[f64; 3]| (0..3).all(|c| density(p[c], to[c]).abs() < 0.05);
    let class: Vec<u8> = blocks
        .iter()
        .map(|p| {
            if !usable(p) {
                0
            } else if near(p, &brightest) {
                1
            } else if near(p, &below) {
                2
            } else {
                0
            }
        })
        .collect();
    if class.iter().filter(|c| **c == 2).count() < n {
        return brightest;
    }
    // The brightest area's edge: how much of it has the area below within
    // a few blocks (past the soft blocks that mix the two).
    const REACH: usize = 3;
    let (mut edge, mut bordered) = (0usize, 0usize);
    for y in 0..bh {
        for x in 0..bw {
            if class[y * bw + x] != 1 {
                continue;
            }
            let outside = [(0, -1), (0, 1), (-1, 0), (1, 0)].iter().any(|(dx, dy)| {
                let (nx, ny) = (x as isize + dx, y as isize + dy);
                nx >= 0
                    && ny >= 0
                    && (nx as usize) < bw
                    && (ny as usize) < bh
                    && class[ny as usize * bw + nx as usize] != 1
            });
            if !outside {
                continue;
            }
            edge += 1;
            let found = (y.saturating_sub(REACH)..(y + REACH + 1).min(bh)).any(|ny| {
                (x.saturating_sub(REACH)..(x + REACH + 1).min(bw))
                    .any(|nx| class[ny * bw + nx] == 2)
            });
            bordered += usize::from(found);
        }
    }
    if edge > 0 && bordered as f64 >= k.light_surround * edge as f64 {
        below
    } else {
        brightest
    }
}

/// Full automatic analysis of a scanned colour negative.
pub fn analyze(rgb: &[f32], w: usize, h: usize, k: &NegativeStrengths) -> Analysis {
    analyze_as(rgb, w, h, NegativeType::Color, k)
}

/// [`analyze`] for a kind of film. A slide has no film base (it is 1).
pub fn analyze_as(
    rgb: &[f32],
    w: usize,
    h: usize,
    kind: NegativeType,
    k: &NegativeStrengths,
) -> Analysis {
    let scan = Scan {
        rgb,
        width: w,
        height: h,
    };
    let n = Negative {
        enabled: true,
        kind,
        ..Default::default()
    };
    resolve(&n, scan, scan, k)
}

/// The part being developed as log exposure per block (density for a
/// negative), without a border for the film holder.
fn samples(scan: Scan, base: [f64; 3], positive: bool, k: &NegativeStrengths) -> Vec<[f64; 3]> {
    let (blocks, bw, bh) = downsample(scan.rgb, scan.width, scan.height, 250_000);
    let (x0, x1) = (
        (bw as f64 * k.border) as usize,
        bw - (bw as f64 * k.border) as usize,
    );
    let (y0, y1) = (
        (bh as f64 * k.border) as usize,
        bh - (bh as f64 * k.border) as usize,
    );
    (y0..y1.max(y0 + 1).min(bh))
        .flat_map(|y| (x0..x1.max(x0 + 1).min(bw)).map(move |x| (x, y)))
        .map(|(x, y)| {
            let p = blocks[y * bw + x];
            std::array::from_fn(|c| log_exposure(p[c], base[c], positive))
        })
        .collect()
}

/// Levels and midtone measured on `samples`, for a given base.
fn levels(samples: &[[f64; 3]], base: [f64; 3], k: &NegativeStrengths) -> Analysis {
    let mut density_low = [0.0; 3];
    let mut density_high = [1.0; 3];
    for c in 0..3 {
        let mut d: Vec<f64> = samples.iter().map(|p| p[c]).collect();
        density_low[c] = percentile(&mut d, k.level_low);
        density_high[c] = percentile(&mut d, k.level_high).max(density_low[c] + 0.05);
    }
    let mut mids: Vec<f64> = samples
        .iter()
        .map(|p| {
            (0..3)
                .map(|c| (p[c] - density_low[c]) / (density_high[c] - density_low[c]))
                .sum::<f64>()
                / 3.0
        })
        .collect();
    let midtone = percentile(&mut mids, 0.5).clamp(0.25, 0.75);
    Analysis {
        base,
        density_low,
        density_high,
        midtone,
        crossover: NO_CROSSOVER,
    }
}

/// Expired film's crossover. **Ours.** Old film loses speed and contrast
/// unevenly per layer (most often the blue-sensitive one, on top), so a grey
/// shows a cast that changes from shadows to highlights. The levels match
/// the ends of each layer; this matches red and blue to green along the
/// grey axis, at a shadow, a midtone and a highlight level of green. At each,
/// the grey axis is the most common colour among the pixels of that level
/// (greys of one cast cluster, coloured things scatter); a level without
/// enough of them is left alone.
fn crossover(samples: &[[f64; 3]], a: &Analysis, k: &NegativeStrengths) -> Crossover {
    let normal: Vec<[f64; 3]> = samples
        .iter()
        .map(|p| {
            std::array::from_fn(|c| {
                (p[c] - a.density_low[c]) / (a.density_high[c] - a.density_low[c])
            })
        })
        .collect();
    const BIN: f64 = 0.02;
    let key = |v: &[f64; 3]| {
        (
            ((v[0] - v[1]) / BIN).round() as i32,
            ((v[2] - v[1]) / BIN).round() as i32,
        )
    };
    let near = |a: (i32, i32), b: (i32, i32)| (a.0 - b.0).abs() <= 1 && (a.1 - b.1).abs() <= 1;
    let mut out = NO_CROSSOVER;
    for (j, &at) in k.crossover_at.iter().enumerate() {
        let level: Vec<&[f64; 3]> = normal
            .iter()
            .filter(|v| (v[1] - at).abs() <= k.crossover_window)
            .collect();
        if level.len() < 64 {
            continue;
        }
        let mut counts = std::collections::BTreeMap::new();
        for v in &level {
            *counts.entry(key(v)).or_insert(0usize) += 1;
        }
        let around = |at: (i32, i32)| -> usize {
            counts
                .iter()
                .filter(|(k, _)| near(**k, at))
                .map(|(_, n)| n)
                .sum()
        };
        let Some(axis) = counts.keys().copied().max_by_key(|k| around(*k)) else {
            continue;
        };
        let grey: Vec<&&[f64; 3]> = level.iter().filter(|v| near(key(v), axis)).collect();
        if grey.len() < level.len() / 20 {
            continue;
        }
        for c in [0, 2] {
            let mut d: Vec<f64> = grey.iter().map(|v| v[c] - v[1]).collect();
            out[c][j] = percentile(&mut d, 0.5).clamp(-k.crossover_limit, k.crossover_limit);
        }
    }
    // Each layer's curve must keep rising: its matched levels stay in order.
    for c in [0, 2] {
        for j in 1..3 {
            let floor = k.crossover_at[j - 1] + out[c][j - 1] + 0.1;
            out[c][j] = out[c][j].max(floor - k.crossover_at[j]);
        }
    }
    out
}

/// A layer's crossover curve: the level `v` where its neutral sits goes to
/// green's, through the matched levels and the ends (0 and 1 stay put),
/// straight lines between them and beyond.
fn uncross(v: f64, offsets: [f64; 3], at: [f64; 3]) -> f64 {
    if offsets == [0.0; 3] {
        return v;
    }
    let from = [
        0.0,
        at[0] + offsets[0],
        at[1] + offsets[1],
        at[2] + offsets[2],
        1.0,
    ];
    let to = [0.0, at[0], at[1], at[2], 1.0];
    if v <= 0.0 || v >= 1.0 {
        return v;
    }
    let i = (1..5).find(|&i| v <= from[i]).unwrap_or(4);
    let t = (v - from[i - 1]) / (from[i] - from[i - 1]).max(1e-9);
    to[i - 1] + t * (to[i] - to[i - 1])
}

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
    let samples = || measured.get_or_init(|| samples(metered, base, positive, k));
    let mut a = if n.auto {
        levels(samples(), base, k)
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
    if n.expired && n.kind != NegativeType::BlackAndWhite {
        a.crossover = match (n.auto, n.crossover) {
            (false, Some(stored)) => stored,
            _ => crossover(samples(), &a, k),
        };
    }
    a
}

/// The frame of a scan: the photo without the film rebate around it (bare
/// film base on a negative, black unexposed film on a slide) or the
/// holder's dark or bright edges, as (left, top, right, bottom) fractions
/// of the scan. **Ours:** a pixel is border when it is near the film base,
/// nearly black or clipped; each side moves in while most of the row or
/// column there is border, then a little more for the soft edge. `None`
/// when there's no clear border.
pub fn detect_frame(
    rgb: &[f32],
    w: usize,
    h: usize,
    kind: NegativeType,
    k: &NegativeStrengths,
) -> Option<(f64, f64, f64, f64)> {
    let (blocks, bw, bh) = downsample(rgb, w, h, 250_000);
    if bw < 8 || bh < 8 {
        return None;
    }
    let base = (kind != NegativeType::Positive).then(|| estimate_base(rgb, w, h, kind, k));
    let bright = blocks
        .iter()
        .map(|p| p[0].max(p[1]).max(p[2]))
        .fold(0.0f64, f64::max)
        .max(1e-6);
    let border: Vec<bool> = blocks
        .iter()
        .map(|p| {
            let top = p[0].max(p[1]).max(p[2]);
            let dark = top < 0.02 * bright;
            let clipped = p.iter().all(|v| *v > 0.97);
            let bare = base.is_some_and(|b| (0..3).all(|c| (p[c] / b[c]).log10().abs() < 0.04));
            dark || clipped || bare
        })
        .collect();
    let row = |y: usize, x0: usize, x1: usize| {
        (x0..x1).filter(|&x| border[y * bw + x]).count() as f64 / (x1 - x0) as f64
    };
    let col = |x: usize, y0: usize, y1: usize| {
        (y0..y1).filter(|&y| border[y * bw + x]).count() as f64 / (y1 - y0) as f64
    };
    const MOSTLY: f64 = 0.6;
    let (max_x, max_y) = (bw / 4, bh / 4);
    // Two rounds, so a side's border doesn't count against the others.
    let (mut l, mut t, mut r, mut b) = (0usize, 0usize, bw, bh);
    for _ in 0..2 {
        let mut nt = 0;
        while nt < max_y && row(nt, l, r) > MOSTLY {
            nt += 1;
        }
        let mut nb = bh;
        while bh - nb < max_y && row(nb - 1, l, r) > MOSTLY {
            nb -= 1;
        }
        let mut nl = 0;
        while nl < max_x && col(nl, nt, nb) > MOSTLY {
            nl += 1;
        }
        let mut nr = bw;
        while bw - nr < max_x && col(nr - 1, nt, nb) > MOSTLY {
            nr -= 1;
        }
        (l, t, r, b) = (nl, nt, nr, nb);
    }
    if l == 0 && t == 0 && r == bw && b == bh {
        return None;
    }
    // Into the photo a little past the edge, which is soft on a scan.
    let pad = 0.005;
    let fx = |x: usize| x as f64 / bw as f64;
    let fy = |y: usize| y as f64 / bh as f64;
    let inset = |v: f64, edge: bool, dir: f64| if edge { v + dir * pad } else { v };
    Some((
        inset(fx(l), l > 0, 1.0),
        inset(fy(t), t > 0, 1.0),
        inset(fx(r), r < bw, -1.0),
        inset(fy(b), b < bh, -1.0),
    ))
}

fn smoothstep(e0: f64, e1: f64, x: f64) -> f64 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
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
mod tests {
    use super::*;

    const K: NegativeStrengths = DEFAULT_STRENGTHS;
    /// An orange film base as a camera might see it (raw-normalised).
    const BASE: [f64; 3] = [0.8, 0.45, 0.25];
    /// Dye densities per unit of log scene exposure (colour negative gamma ~0.6,
    /// different per layer).
    const GAMMA: [f64; 3] = [0.55, 0.6, 0.65];

    fn scan_of(rgb: &[f32], width: usize, height: usize) -> Scan<'_> {
        Scan { rgb, width, height }
    }

    /// Simulate scanning a negative of a scene with linear values `scene` (RGB).
    fn negative_of(scene: [f64; 3]) -> [f32; 3] {
        std::array::from_fn(|c| {
            // Density above base grows with log exposure (toe at 0.002 exposure).
            let d = GAMMA[c] * (scene[c].max(0.002) / 0.002).log10();
            (BASE[c] * 10f64.powf(-d)) as f32
        })
    }

    /// A test scan: a rebate strip of bare base on the left, then a scene of
    /// grey ramps and colour patches.
    fn scan(w: usize, h: usize) -> Vec<f32> {
        let mut rgb = vec![0.0f32; w * h * 3];
        for y in 0..h {
            for x in 0..w {
                let p = if x < w / 20 {
                    BASE.map(|v| v as f32)
                } else {
                    let t = x as f64 / w as f64;
                    let v = 0.005 * (1.0f64 / 0.005).powf(t); // 0.005..1 log ramp
                    // Colour patches stay inside the grey range, as in real scenes
                    // (auto levels assume the deepest shadow and brightest highlight
                    // are close to neutral).
                    let m = 0.03 + 0.4 * t;
                    let scene = match (y * 4) / h {
                        0 | 1 => [v, v, v],
                        2 => [m, m * 0.5, m * 0.3],
                        _ => [m * 0.3, m * 0.6, m],
                    };
                    negative_of(scene)
                };
                rgb[(y * w + x) * 3..(y * w + x) * 3 + 3].copy_from_slice(&p);
            }
        }
        rgb
    }

    #[test]
    fn base_estimate_finds_the_rebate() {
        let (w, h) = (400, 200);
        let rgb = scan(w, h);
        let base = estimate_base(&rgb, w, h, NegativeType::Color, &K);
        for c in 0..3 {
            assert!((base[c] - BASE[c]).abs() < 0.01, "{base:?}");
        }
    }

    #[test]
    fn auto_conversion_gives_a_neutral_monotone_positive() {
        let (w, h) = (400, 200);
        let mut rgb = scan(w, h);
        let n = Negative {
            enabled: true,
            ..Default::default()
        };
        let a = resolve(&n, scan_of(&rgb, w, h), scan_of(&rgb, w, h), &K);
        let conv = Conversion::new(&n, a, &K);
        apply(&mut rgb, &conv);
        // Grey rows stay neutral and get brighter left → right.
        let y = h / 8;
        let mut last = 0.0;
        for x in (w / 10..w - w / 20).step_by(10) {
            let p = &rgb[(y * w + x) * 3..(y * w + x) * 3 + 3];
            let mean = (p[0] + p[1] + p[2]) / 3.0;
            assert!(mean > last, "monotone at x={x}");
            last = mean;
            let spread = p.iter().cloned().fold(f32::MIN, f32::max)
                - p.iter().cloned().fold(f32::MAX, f32::min);
            assert!(spread < 0.12 * mean + 1e-3, "neutral at x={x}: {p:?}");
        }
        // The warm patch row is warm, the cool one cool.
        let at = |y: usize| {
            let x = w * 3 / 4;
            let i = (y * w + x) * 3;
            [rgb[i], rgb[i + 1], rgb[i + 2]]
        };
        let warm = at(h * 5 / 8);
        let cool = at(h * 7 / 8);
        assert!(warm[0] > warm[2], "{warm:?}");
        assert!(cool[2] > cool[0], "{cool:?}");
    }

    #[test]
    fn a_slide_is_balanced_but_not_inverted() {
        // A slide of grey ramps and a warm row, lit by a bluish light, with
        // slide film's steep contrast (transmittance ∝ scene^1.5).
        let (w, h) = (300, 120);
        let light = [0.55, 0.75, 0.95];
        let mut rgb = vec![0.0f32; w * h * 3];
        for y in 0..h {
            for x in 0..w {
                let t = x as f64 / w as f64;
                let v = 0.01 * (1.0f64 / 0.01).powf(t);
                // The warm row stays inside the grey range, as auto levels assume.
                let m = 0.05 + 0.5 * t;
                let scene = if y < h * 2 / 3 {
                    [v; 3]
                } else {
                    [m, m * 0.6, m * 0.4]
                };
                for c in 0..3 {
                    rgb[(y * w + x) * 3 + c] = (light[c] * scene[c].powf(1.5)) as f32;
                }
            }
        }
        let n = Negative {
            enabled: true,
            kind: NegativeType::Positive,
            ..Default::default()
        };
        let a = resolve(&n, scan_of(&rgb, w, h), scan_of(&rgb, w, h), &K);
        assert_eq!(a.base, [1.0; 3]);
        apply(&mut rgb, &Conversion::new(&n, a, &K));
        // Brighter on the slide stays brighter, and the grey rows are grey.
        let y = h / 4;
        let mut last = 0.0;
        for x in (w / 10..w - w / 10).step_by(10) {
            let p = &rgb[(y * w + x) * 3..(y * w + x) * 3 + 3];
            let mean = (p[0] + p[1] + p[2]) / 3.0;
            assert!(mean > last, "monotone at x={x}");
            last = mean;
            let spread = p.iter().cloned().fold(f32::MIN, f32::max)
                - p.iter().cloned().fold(f32::MAX, f32::min);
            assert!(spread < 0.12 * mean + 1e-3, "neutral at x={x}: {p:?}");
        }
        let i = ((h * 5 / 6) * w + w / 2) * 3;
        assert!(rgb[i] > rgb[i + 2], "warm stays warm: {:?}", &rgb[i..i + 3]);
    }

    #[test]
    fn the_frame_is_found_inside_rebate_and_holder() {
        // The test scan (rebate strip on the left) inside a black holder edge.
        let (w, h) = (400, 200);
        let inner = scan(w, h);
        let (pw, ph) = (w + 40, h + 30);
        let mut rgb = vec![0.001f32; pw * ph * 3];
        for y in 0..h {
            let (o, i) = (((y + 10) * pw + 30) * 3, y * w * 3);
            rgb[o..o + w * 3].copy_from_slice(&inner[i..i + w * 3]);
        }
        let (l, t, r, b) = detect_frame(&rgb, pw, ph, NegativeType::Color, &K).unwrap();
        // Left: past the holder (30 px) and the rebate (w/20 = 20 px).
        let px = |f: f64, n: usize| f * n as f64;
        assert!((px(l, pw) - 50.0).abs() < 6.0, "left {}", px(l, pw));
        assert!((px(t, ph) - 10.0).abs() < 4.0, "top {}", px(t, ph));
        assert!((px(r, pw) - 430.0).abs() < 4.0, "right {}", px(r, pw));
        assert!((px(b, ph) - 210.0).abs() < 4.0, "bottom {}", px(b, ph));
        // A scan that is all photo has no frame to find.
        let photo: Vec<f32> = inner
            .chunks(3)
            .enumerate()
            .flat_map(|(i, p)| {
                if i % w < w / 20 {
                    negative_of([0.1; 3])
                } else {
                    [p[0], p[1], p[2]]
                }
            })
            .collect();
        assert_eq!(detect_frame(&photo, w, h, NegativeType::Color, &K), None);
    }

    #[test]
    fn stored_analysis_is_used_when_auto_is_off() {
        let (w, h) = (200, 100);
        let rgb = scan(w, h);
        let n = Negative {
            enabled: true,
            auto: false,
            base: Some([0.5, 0.5, 0.5]),
            midtone: Some(0.4),
            range: 1.5,
            ..Default::default()
        };
        let a = resolve(&n, scan_of(&rgb, w, h), scan_of(&rgb, w, h), &K);
        assert_eq!(a.base, [0.5, 0.5, 0.5]);
        assert_eq!(a.midtone, 0.4);
        // Levels were missing, so they come from the fixed range: green
        // spans it, the other layers keep their measured share.
        assert_eq!(a.density_low, [K.fixed_floor; 3]);
        assert!((a.density_high[1] - (K.fixed_floor + 1.5)).abs() < 1e-9);
        let m = resolve(
            &Negative {
                auto: true,
                ..n.clone()
            },
            scan_of(&rgb, w, h),
            scan_of(&rgb, w, h),
            &K,
        );
        let share = |a: &Analysis, c: usize| {
            (a.density_high[c] - a.density_low[c]) / (a.density_high[1] - a.density_low[1])
        };
        for c in [0, 2] {
            assert!((share(&a, c) - share(&m, c)).abs() < 1e-9);
        }
        // Auto measures the levels, keeping the stored base.
        let auto = resolve(
            &Negative {
                auto: true,
                ..n.clone()
            },
            scan_of(&rgb, w, h),
            scan_of(&rgb, w, h),
            &K,
        );
        assert_eq!(auto.base, [0.5, 0.5, 0.5]);
        assert_ne!(auto.density_high, a.density_high);
        // Without a stored base, it is measured from the scan.
        let measured = resolve(
            &Negative { base: None, ..n },
            scan_of(&rgb, w, h),
            scan_of(&rgb, w, h),
            &K,
        );
        assert!(
            (measured.base[0] - BASE[0]).abs() < 0.01,
            "{:?}",
            measured.base
        );
    }

    #[test]
    fn auto_off_prints_frames_alike() {
        // The same scene on two frames, the second exposed a stop more.
        let (w, h) = (300, 120);
        let frame = |gain: f64| -> Vec<f32> {
            let mut rgb = scan(w, h);
            for y in 0..h {
                for x in w / 20..w {
                    let t = x as f64 / w as f64;
                    let v = 0.005 * (1.0f64 / 0.005).powf(t) * gain;
                    let p = negative_of([v; 3]);
                    rgb[(y * w + x) * 3..(y * w + x) * 3 + 3].copy_from_slice(&p);
                }
            }
            rgb
        };
        let (one, two) = (frame(1.0), frame(2.0));
        let grey = negative_of([0.05; 3]);
        let render = |n: &Negative, rgb: &[f32]| {
            let a = resolve(n, scan_of(rgb, w, h), scan_of(rgb, w, h), &K);
            Conversion::new(n, a, &K).apply(grey)[1]
        };
        let off = Negative {
            enabled: true,
            auto: false,
            ..Default::default()
        };
        // Auto off: the same film prints the same on both frames.
        assert!((render(&off, &one) / render(&off, &two) - 1.0).abs() < 0.01);
        // ... so the frame exposed a stop more prints about a stop brighter
        // where the scene is the same (a grey patch shot at 0.05 and at 0.1).
        let brighter = negative_of([0.1; 3]);
        let a = resolve(&off, scan_of(&two, w, h), scan_of(&two, w, h), &K);
        let ratio = Conversion::new(&off, a, &K).apply(brighter)[1] / render(&off, &two);
        assert!((1.6..2.6).contains(&ratio), "{ratio}");
        // Auto evens out each frame's exposure instead.
        let auto = Negative {
            enabled: true,
            ..Default::default()
        };
        let a1 = resolve(&auto, scan_of(&one, w, h), scan_of(&one, w, h), &K);
        let a2 = resolve(&auto, scan_of(&two, w, h), scan_of(&two, w, h), &K);
        let p1 = Conversion::new(&auto, a1, &K).apply(grey)[1];
        let p2 = Conversion::new(&auto, a2, &K).apply(brighter)[1];
        assert!((p1 / p2 - 1.0).abs() < 0.2, "{p1} {p2}");
    }

    #[test]
    fn the_base_comes_from_the_whole_scan_and_the_levels_from_the_crop() {
        // The test scan in a black holder: the holder is denser than any
        // film, so levels measured over it put the highlights too far.
        let (w, h) = (400, 200);
        let inner = scan(w, h);
        let (pw, ph) = (w + 80, h + 80);
        let mut rgb = vec![0.0005f32; pw * ph * 3];
        for y in 0..h {
            let (o, i) = (((y + 40) * pw + 40) * 3, y * w * 3);
            rgb[o..o + w * 3].copy_from_slice(&inner[i..i + w * 3]);
        }
        // The crop: the photo, without the rebate strip.
        let (cw, ch) = (w - w / 20, h);
        let mut crop = Vec::with_capacity(cw * ch * 3);
        for y in 0..h {
            let i = (y * w + w / 20) * 3;
            crop.extend_from_slice(&inner[i..i + cw * 3]);
        }
        let n = Negative {
            enabled: true,
            ..Default::default()
        };
        let whole = scan_of(&rgb, pw, ph);
        let cropped = resolve(&n, whole, scan_of(&crop, cw, ch), &K);
        let uncropped = resolve(&n, whole, whole, &K);
        for (c, base) in BASE.iter().enumerate() {
            assert!((cropped.base[c] - base).abs() < 0.01, "{:?}", cropped.base);
            assert!(cropped.density_high[c] < uncropped.density_high[c] - 0.3);
        }
        // Measured on the crop alone, the base would be the thinnest shadow.
        let alone = resolve(&n, scan_of(&crop, cw, ch), scan_of(&crop, cw, ch), &K);
        assert!(alone.base[2] < BASE[2] - 0.02, "{:?}", alone.base);
    }

    /// The test scan, exposed a stop darker, with sprocket holes in its
    /// rebate strip that let the light through unclipped: the light is
    /// `gap` density brighter than the film base.
    fn holes(w: usize, h: usize, gap: [f64; 3]) -> (Vec<f32>, [f64; 3]) {
        let mut rgb: Vec<f32> = scan(w, h).iter().map(|v| v * 0.5).collect();
        let base = BASE.map(|b| b * 0.5);
        let rebate = w / 20;
        for y in 0..h {
            for x in rebate / 4..rebate * 3 / 4 {
                if y % 16 < 10 {
                    for c in 0..3 {
                        rgb[(y * w + x) * 3 + c] = (base[c] * 10f64.powf(gap[c])) as f32;
                    }
                }
            }
        }
        (rgb, base)
    }

    #[test]
    fn the_base_is_not_the_light_through_the_perforations() {
        let (w, h) = (400, 200);
        let close = |found: [f64; 3], base: [f64; 3]| {
            (0..3).all(|c| (found[c] - base[c]).abs() < 0.005 * base[c].max(0.1) / 0.1)
        };
        // Colour film: the orange base is far denser in blue than in red.
        let (rgb, base) = holes(w, h, [0.25, 0.6, 0.85]);
        let found = estimate_base(&rgb, w, h, NegativeType::Color, &K);
        assert!(close(found, base), "{found:?} {base:?}");
        // Black-and-white film: a grey base, found by the holes it surrounds.
        let (rgb, base) = holes(w, h, [0.25; 3]);
        let found = estimate_base(&rgb, w, h, NegativeType::BlackAndWhite, &K);
        assert!(close(found, base), "{found:?} {base:?}");
        // Without holes the rebate is still the base, though the picture
        // beside it is dark: it is neither around it nor orange against it.
        let rgb = scan(w, h);
        let found = estimate_base(&rgb, w, h, NegativeType::Color, &K);
        assert!(close(found, BASE), "{found:?}");
        let found = estimate_base(&rgb, w, h, NegativeType::BlackAndWhite, &K);
        assert!(close(found, BASE), "{found:?}");
    }

    /// A grey ramp on film whose blue layer is off by `shift(l)` density at
    /// log exposure `l` (0..2.6), its ends matching the others.
    fn crossed(w: usize, h: usize, shift: impl Fn(f64) -> f64) -> Vec<f32> {
        let mut rgb = vec![0.0f32; w * h * 3];
        for y in 0..h {
            for x in 0..w {
                let l = 2.6 * x as f64 / w as f64;
                for c in 0..3 {
                    let d = GAMMA[1] * l + if c == 2 { shift(l) } else { 0.0 };
                    rgb[(y * w + x) * 3 + c] = (BASE[c] * 10f64.powf(-d)) as f32;
                }
            }
        }
        rgb
    }

    /// The blue cast (|ln b/g|) at a fraction `at` along the converted ramp.
    fn cast(rgb: &[f32], w: usize, h: usize, expired: bool, at: f64) -> f64 {
        let n = Negative {
            enabled: true,
            base: Some(BASE),
            expired,
            ..Default::default()
        };
        let a = resolve(&n, scan_of(rgb, w, h), scan_of(rgb, w, h), &K);
        let c = Conversion::new(&n, a, &K);
        let mut out = rgb.to_vec();
        apply(&mut out, &c);
        let i = (h / 2 * w + (w as f64 * at) as usize) * 3;
        (out[i + 2] / out[i + 1]).ln().abs() as f64
    }

    #[test]
    fn expired_film_matches_the_layers_from_shadows_to_highlights() {
        let (w, h) = (300, 120);
        // The blue layer bows: its midtones are too dense (a crossover).
        let bowed = crossed(w, h, |l| 0.12 * l * (2.6 - l));
        let (plain, mended) = (
            cast(&bowed, w, h, false, 0.5),
            cast(&bowed, w, h, true, 0.5),
        );
        assert!(mended < plain * 0.5, "plain {plain}, expired {mended}");
        // An S: blue too dense in the shadows, too thin in the highlights.
        let s = crossed(w, h, |l| 0.06 * (std::f64::consts::TAU * l / 2.6).sin());
        for at in [0.25, 0.75] {
            let (plain, mended) = (cast(&s, w, h, false, at), cast(&s, w, h, true, at));
            assert!(
                mended < plain * 0.5,
                "at {at}: plain {plain}, expired {mended}"
            );
        }
        // Black-and-white film has no layers to even out.
        let n = Negative {
            enabled: true,
            kind: NegativeType::BlackAndWhite,
            expired: true,
            ..Default::default()
        };
        let a = resolve(&n, scan_of(&bowed, w, h), scan_of(&bowed, w, h), &K);
        assert_eq!(a.crossover, NO_CROSSOVER);
    }

    #[test]
    fn controls_move_the_image_the_right_way() {
        let a = Analysis {
            base: BASE,
            density_low: [0.0; 3],
            density_high: GAMMA.map(|g| g * 2.7),
            midtone: 0.5,
            crossover: NO_CROSSOVER,
        };
        let grey = negative_of([0.05; 3]);
        let render = |n: Negative| Conversion::new(&n, a, &K).apply(grey);
        let base = render(Negative::default());
        let brighter = render(Negative {
            exposure: 1.0,
            ..Default::default()
        });
        assert!((brighter[1] / base[1] - 2.0).abs() < 1e-3, "exposure is EV");
        let redder = render(Negative {
            balance: [50.0, 0.0, 0.0],
            ..Default::default()
        });
        assert!(redder[0] > base[0] && (redder[1] - base[1]).abs() < 1e-6);
        // Contrast: a shadow gets darker, a highlight brighter.
        let shadow = negative_of([0.01; 3]);
        let high = negative_of([0.5; 3]);
        let plain = Conversion::new(&Negative::default(), a, &K);
        let punchy = Conversion::new(
            &Negative {
                contrast: 50.0,
                ..Default::default()
            },
            a,
            &K,
        );
        assert!(punchy.apply(shadow)[1] < plain.apply(shadow)[1]);
        assert!(punchy.apply(high)[1] > plain.apply(high)[1]);
        // Shoulder compresses highlights.
        let soft = Conversion::new(
            &Negative {
                shoulder: 100.0,
                ..Default::default()
            },
            a,
            &K,
        );
        assert!(soft.apply(high)[1] < plain.apply(high)[1]);
    }

    #[test]
    fn stocks_are_valid_and_black_and_white_is_grey() {
        let mut ids = std::collections::BTreeSet::new();
        for s in STOCKS {
            assert!(ids.insert(s.id), "duplicate id {}", s.id);
            for row in s.crosstalk {
                assert!((row.iter().sum::<f64>() - 1.0).abs() < 1e-9, "{}", s.id);
            }
            assert!((s.mix.iter().sum::<f64>() - 1.0).abs() < 1e-9, "{}", s.id);
            assert!(s.contrast > 0.0 && s.saturation > 0.0);
        }
        assert_eq!(stock("VIVID-100").id, "vivid-100");
        assert_eq!(stock("no-such-film").id, "neutral");

        let a = Analysis {
            base: BASE,
            density_low: [0.0; 3],
            density_high: GAMMA.map(|g| g * 2.7),
            midtone: 0.5,
            crossover: NO_CROSSOVER,
        };
        let n = Negative {
            stock: "bw-classic-400".into(),
            ..Default::default()
        };
        let p = Conversion::new(&n, a, &K).apply(negative_of([0.1, 0.05, 0.02]));
        assert!(p[0] == p[1] && p[1] == p[2], "{p:?}");
        // Stock amount 0 = neutral conversion.
        let vivid = |amount: f64| {
            Conversion::new(
                &Negative {
                    stock: "vivid-100".into(),
                    stock_amount: amount,
                    ..Default::default()
                },
                a,
                &K,
            )
            .apply(negative_of([0.1, 0.05, 0.02]))
        };
        let neutral =
            Conversion::new(&Negative::default(), a, &K).apply(negative_of([0.1, 0.05, 0.02]));
        assert_eq!(vivid(0.0), neutral);
        assert_ne!(vivid(100.0), neutral);
    }
}
