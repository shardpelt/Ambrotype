//! Film negative → positive conversion, with film stock looks.
//!
//! Camera Raw has no such feature (photographers use the Negative Lab Pro
//! plug-in), so these settings live in our own `flb:` XMP namespace and work
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
//! Auto mode estimates base, levels and midtone per image ([`analyze`]);
//! stored values (from `flb negative analyze`, or typed in) are used when auto
//! is off, which keeps a whole roll consistent. All constants are in
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
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NegativeType {
    Color,
    BlackAndWhite,
}

/// The negative conversion settings (`flb:Negative*`, `flb:FilmBase*`, `flb:FilmStock*`).
#[derive(Debug, Clone, PartialEq)]
pub struct Negative {
    pub enabled: bool,
    pub kind: NegativeType,
    /// Auto: estimate base, levels and midtone for every image, ignoring stored values.
    pub auto: bool,
    /// Film stock id (see [`STOCKS`]).
    pub stock: String,
    /// How much of the stock look to apply, 0..100.
    pub stock_amount: f64,
    /// Stored analysis (used when `auto` is off; missing parts are estimated).
    pub base: Option<[f64; 3]>,
    pub density_low: Option<[f64; 3]>,
    pub density_high: Option<[f64; 3]>,
    pub midtone: Option<f64>,
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
            exposure: 0.0,
            contrast: 0.0,
            balance: [0.0; 3],
            toe: 0.0,
            shoulder: 0.0,
            saturation: 0.0,
        }
    }
}

/// Per-image analysis: film base transmittance (raw-normalised, white level = 1),
/// per-channel density levels, and the midtone (0..1 in normalised log units).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Analysis {
    pub base: [f64; 3],
    pub density_low: [f64; 3],
    pub density_high: [f64; 3],
    pub midtone: f64,
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

/// Estimate the film base from the brightest unclipped part of the scan
/// (unexposed film: the rebate between frames, or the thinnest shadows).
pub fn estimate_base(rgb: &[f32], w: usize, h: usize, k: &NegativeStrengths) -> [f64; 3] {
    let (blocks, _, _) = downsample(rgb, w, h, 250_000);
    let mut candidates: Vec<&[f64; 3]> = blocks
        .iter()
        .filter(|p| p.iter().all(|v| *v < 0.97) && p.iter().all(|v| *v > 0.0))
        .collect();
    if candidates.is_empty() {
        candidates = blocks.iter().collect();
    }
    candidates.sort_by(|a, b| (b[0] + b[1] + b[2]).total_cmp(&(a[0] + a[1] + a[2])));
    let n =
        ((candidates.len() as f64 * k.base_fraction).ceil() as usize).clamp(1, candidates.len());
    let top = &candidates[..n];
    std::array::from_fn(|c| {
        let mut v: Vec<f64> = top.iter().map(|p| p[c]).collect();
        percentile(&mut v, 0.5).max(1e-4)
    })
}

/// Full automatic analysis of a scanned negative.
pub fn analyze(rgb: &[f32], w: usize, h: usize, k: &NegativeStrengths) -> Analysis {
    let base = estimate_base(rgb, w, h, k);
    levels(rgb, w, h, base, k)
}

/// Levels and midtone for a given base.
fn levels(rgb: &[f32], w: usize, h: usize, base: [f64; 3], k: &NegativeStrengths) -> Analysis {
    let (blocks, bw, bh) = downsample(rgb, w, h, 250_000);
    let (x0, x1) = (
        (bw as f64 * k.border) as usize,
        bw - (bw as f64 * k.border) as usize,
    );
    let (y0, y1) = (
        (bh as f64 * k.border) as usize,
        bh - (bh as f64 * k.border) as usize,
    );
    let inner: Vec<[f64; 3]> = (y0..y1.max(y0 + 1).min(bh))
        .flat_map(|y| (x0..x1.max(x0 + 1).min(bw)).map(move |x| (x, y)))
        .map(|(x, y)| {
            let p = blocks[y * bw + x];
            std::array::from_fn(|c| density(p[c], base[c]))
        })
        .collect();
    let mut density_low = [0.0; 3];
    let mut density_high = [1.0; 3];
    for c in 0..3 {
        let mut d: Vec<f64> = inner.iter().map(|p| p[c]).collect();
        density_low[c] = percentile(&mut d, k.level_low);
        density_high[c] = percentile(&mut d, k.level_high).max(density_low[c] + 0.05);
    }
    let mut mids: Vec<f64> = inner
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
    }
}

/// The analysis to render with: auto, or the stored values with any missing
/// parts estimated from the image.
pub fn resolve(n: &Negative, rgb: &[f32], w: usize, h: usize, k: &NegativeStrengths) -> Analysis {
    if n.auto {
        return analyze(rgb, w, h, k);
    }
    let base = n.base.unwrap_or_else(|| estimate_base(rgb, w, h, k));
    let need_levels = n.density_low.is_none() || n.density_high.is_none() || n.midtone.is_none();
    let estimated = need_levels.then(|| levels(rgb, w, h, base, k));
    let pick = |stored: Option<[f64; 3]>, f: fn(&Analysis) -> [f64; 3]| {
        stored.unwrap_or_else(|| f(estimated.as_ref().expect("estimated when missing")))
    };
    let density_low = pick(n.density_low, |a| a.density_low);
    let mut density_high = pick(n.density_high, |a| a.density_high);
    for c in 0..3 {
        density_high[c] = density_high[c].max(density_low[c] + 0.01);
    }
    Analysis {
        base: base.map(|b| b.max(1e-4)),
        density_low,
        density_high,
        midtone: n
            .midtone
            .or(estimated.map(|a| a.midtone))
            .unwrap_or(0.5)
            .clamp(0.05, 0.95),
    }
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
    balance: [f64; 3],
    saturation: f64,
    range: f64,
    toe: f64,
    shoulder: f64,
    gain: f64,
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
            balance: n.balance.map(|b| b / 100.0 * k.balance_range),
            saturation: blend(1.0, stock.saturation) * (1.0 + n.saturation / 100.0).max(0.0),
            range: k.scene_range * blend(1.0, stock.contrast) * (n.contrast / 100.0).exp2(),
            toe: blend(0.0, stock.toe) + n.toe / 100.0 * k.toe_range,
            shoulder: blend(0.0, stock.shoulder) + n.shoulder / 100.0 * k.shoulder_range,
            gain: k.midtone_grey * n.exposure.exp2(),
        }
    }

    /// Negative camera RGB → positive scene-linear RGB (neutral = equal channels).
    pub fn apply(&self, p: [f32; 3]) -> [f32; 3] {
        let a = &self.analysis;
        let s = &self.stock;
        let m = a.midtone;
        // Normalised log exposure per channel (0 = shadow, 1 = highlight), balanced.
        let neutral: [f64; 3] = std::array::from_fn(|c| {
            let d = density(f64::from(p[c]), a.base[c]);
            (d - a.density_low[c]) / (a.density_high[c] - a.density_low[c]) + self.balance[c]
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
        let base = estimate_base(&rgb, w, h, &K);
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
        let a = resolve(&n, &rgb, w, h, &K);
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
    fn stored_analysis_is_used_when_auto_is_off() {
        let (w, h) = (200, 100);
        let rgb = scan(w, h);
        let auto = analyze(&rgb, w, h, &K);
        let n = Negative {
            enabled: true,
            auto: false,
            base: Some([0.5, 0.5, 0.5]),
            midtone: Some(0.4),
            ..Default::default()
        };
        let a = resolve(&n, &rgb, w, h, &K);
        assert_eq!(a.base, [0.5, 0.5, 0.5]);
        assert_eq!(a.midtone, 0.4);
        // Levels were missing, so they were estimated (relative to the stored base).
        assert!(a.density_low != auto.density_low);
        // Auto ignores stored values.
        let auto_n = Negative { auto: true, ..n };
        assert_eq!(resolve(&auto_n, &rgb, w, h, &K), auto);
    }

    #[test]
    fn controls_move_the_image_the_right_way() {
        let a = Analysis {
            base: BASE,
            density_low: [0.0; 3],
            density_high: GAMMA.map(|g| g * 2.7),
            midtone: 0.5,
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
