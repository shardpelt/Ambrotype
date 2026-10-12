// SPDX-License-Identifier: GPL-3.0-or-later
//! Look matching (`amb match`): find the settings that make a photo look
//! like reference images. Ours: statistics, not a learned model.
//!
//! The references and the photo are measured the same way ([`LookStats`]):
//! the spread of CIE L* (percentiles), the mean colour of the shadows,
//! midtones and highlights, and how colourful they are (chroma
//! percentiles). These describe a look without depending much on what is
//! in the picture, and the spread of a* and b* tells saturation apart from a
//! white balance shift. The photo is then developed small again and again
//! while a least-squares solver (Levenberg–Marquardt) moves its settings
//! together: white balance, tone (Exposure, Contrast, Highlights, Shadows,
//! Whites, Blacks), colour (Vibrance, Saturation) and Color Grading's
//! shadow and highlight tints. Each try costs one small render. A small
//! penalty on every change keeps the answer to the smallest moves that
//! reach the look.

use std::collections::BTreeMap;

use amb_presets::Value;
use amb_presets::registry::Registry;
use serde::Serialize;

use crate::pipeline::{self, DevelopSettings, ProPhotoImage};
use crate::raw::CameraImage;
use crate::stats::lab;

pub type Settings = BTreeMap<String, Value>;

/// The percentiles of L* in [`LookStats::lightness`].
pub const L_PERCENTILES: [f32; 9] = [1.0, 5.0, 10.0, 25.0, 50.0, 75.0, 90.0, 95.0, 99.0];

/// A look as numbers.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LookStats {
    /// CIE L* at [`L_PERCENTILES`].
    pub lightness: [f32; 9],
    /// Mean (a*, b*) of the shadows, midtones and highlights (soft zones of L*).
    pub zones: [[f32; 2]; 3],
    /// Chroma C*ab at the 50th and 90th percentiles.
    pub chroma: [f32; 2],
    /// Spread of (a*, b*) about its mean: the standard deviations of a* and
    /// b*, and the signed root of their covariance. Saturation scales it,
    /// white balance barely moves it, so the two can be told apart.
    pub spread: [f32; 3],
    /// Per HSL band (in [`crate::color::BANDS`] order, weighed as the Color
    /// Mixer weighs colours): its share of the photo, and the mean OkLCh
    /// chroma, lightness and hue offset from the band's centre (radians) of
    /// its colours.
    pub bands: [[f32; 4]; 8],
}

/// Weights of a pixel of lightness `l` in the shadow, midtone and highlight zones.
fn zone_weights(l: f64) -> [f64; 3] {
    let shadow = ((55.0 - l) / 40.0).clamp(0.0, 1.0);
    let highlight = ((l - 45.0) / 40.0).clamp(0.0, 1.0);
    let mid = (1.0 - (l - 50.0).abs() / 35.0).clamp(0.0, 1.0);
    [shadow, mid, highlight]
}

fn percentile(sorted: &[f32], q: f32) -> f32 {
    if sorted.is_empty() {
        return 0.0;
    }
    let pos = (sorted.len() - 1) as f32 * q / 100.0;
    let i = pos.floor() as usize;
    let j = (i + 1).min(sorted.len() - 1);
    sorted[i] + (sorted[j] - sorted[i]) * (pos - i as f32)
}

/// Measure a developed (linear ProPhoto) image.
pub fn look_stats(img: &ProPhotoImage) -> LookStats {
    let n = img.rgb.len() / 3;
    let mut ls = Vec::with_capacity(n);
    let mut cs = Vec::with_capacity(n);
    let mut sums = [[0.0f64; 3]; 3];
    let mut moments = [0.0f64; 5];
    let ok = crate::color::OkLab::new();
    let centres = crate::color::hsl_band_hues();
    let mut band_sums = [[0.0f64; 4]; 8];
    for p in img.rgb.chunks_exact(3) {
        let [ol, oa, ob] = ok.from_working([p[0], p[1], p[2]]);
        if ol.is_finite() && oa.is_finite() && ob.is_finite() {
            let (chroma, hue) = (oa.hypot(ob), ob.atan2(oa));
            for (i, w) in crate::color::hsl_band_membership(hue, chroma) {
                if w > 0.0 {
                    let d = (hue - centres[i] + std::f32::consts::PI)
                        .rem_euclid(std::f32::consts::TAU)
                        - std::f32::consts::PI;
                    let w = w as f64;
                    let sum = &mut band_sums[i];
                    sum[0] += w;
                    sum[1] += w * chroma as f64;
                    sum[2] += w * ol as f64;
                    sum[3] += w * d as f64;
                }
            }
        }
        let [l, a, b] = lab(p);
        for (m, v) in moments.iter_mut().zip([a, b, a * a, b * b, a * b]) {
            *m += v;
        }
        let l = l.clamp(0.0, 100.0);
        ls.push(l as f32);
        cs.push(a.hypot(b) as f32);
        for (sum, w) in sums.iter_mut().zip(zone_weights(l)) {
            sum[0] += w * a;
            sum[1] += w * b;
            sum[2] += w;
        }
    }
    ls.sort_by(f32::total_cmp);
    cs.sort_by(f32::total_cmp);
    let k = n.max(1) as f64;
    let [ma, mb, aa, bb, ab] = moments.map(|m| m / k);
    let cov = ab - ma * mb;
    let spread = [
        (aa - ma * ma).max(0.0).sqrt() as f32,
        (bb - mb * mb).max(0.0).sqrt() as f32,
        (cov.signum() * cov.abs().sqrt()) as f32,
    ];
    LookStats {
        lightness: L_PERCENTILES.map(|q| percentile(&ls, q)),
        zones: sums.map(|[a, b, w]| {
            if w > 1e-9 {
                [(a / w) as f32, (b / w) as f32]
            } else {
                [0.0, 0.0]
            }
        }),
        chroma: [percentile(&cs, 50.0), percentile(&cs, 90.0)],
        spread,
        bands: band_sums.map(|[w, c, l, d]| {
            if w > 1e-9 {
                [
                    (w / k) as f32,
                    (c / w) as f32,
                    (l / w) as f32,
                    (d / w) as f32,
                ]
            } else {
                [0.0; 4]
            }
        }),
    }
}

/// The mean of several looks (several references make one target).
pub fn average(looks: &[LookStats]) -> Option<LookStats> {
    let n = looks.len() as f32;
    let first = looks.first()?;
    let mut out = first.clone();
    for (i, v) in out.lightness.iter_mut().enumerate() {
        *v = looks.iter().map(|s| s.lightness[i]).sum::<f32>() / n;
    }
    for z in 0..3 {
        for c in 0..2 {
            out.zones[z][c] = looks.iter().map(|s| s.zones[z][c]).sum::<f32>() / n;
        }
    }
    for c in 0..2 {
        out.chroma[c] = looks.iter().map(|s| s.chroma[c]).sum::<f32>() / n;
    }
    for c in 0..3 {
        out.spread[c] = looks.iter().map(|s| s.spread[c]).sum::<f32>() / n;
    }
    for b in 0..8 {
        for c in 0..4 {
            out.bands[b][c] = looks.iter().map(|s| s.bands[b][c]).sum::<f32>() / n;
        }
    }
    Some(out)
}

/// Least share of the photo, in both looks, for a band's colours to count.
const MIN_BAND_SHARE: f32 = 0.01;

/// The differences between two looks, weighted so their squares add up to
/// [`distance`]; with `bands`, also each HSL band's colours where both
/// looks have them (in OkLCh, times 100 to be near Lab units, weighted by
/// how much of the photos they cover).
fn residuals(a: &LookStats, b: &LookStats, bands: bool) -> Vec<f64> {
    let mut r = Vec::with_capacity(9 + 6 + 2 + 3);
    let tone = (1.0 / a.lightness.len() as f64).sqrt();
    r.extend(
        a.lightness
            .iter()
            .zip(&b.lightness)
            .map(|(x, y)| tone * (x - y) as f64),
    );
    for (w, (x, y)) in [0.5f64, 1.0, 0.5].iter().zip(a.zones.iter().zip(&b.zones)) {
        r.push(w.sqrt() * (x[0] - y[0]) as f64);
        r.push(w.sqrt() * (x[1] - y[1]) as f64);
    }
    r.extend(
        a.chroma
            .iter()
            .zip(&b.chroma)
            .map(|(x, y)| (x - y) as f64 / 2f64.sqrt()),
    );
    r.extend(a.spread.iter().zip(&b.spread).map(|(x, y)| (x - y) as f64));
    if bands {
        for (x, y) in a.bands.iter().zip(&b.bands) {
            let share = x[0].min(y[0]);
            let w = if share < MIN_BAND_SHARE {
                0.0
            } else {
                (8.0 * share as f64).min(2.0).sqrt() * 100.0
            };
            r.push(w * (x[1] - y[1]) as f64);
            r.push(w * (x[2] - y[2]) as f64);
            r.push(w * (x[3] - y[3]) as f64 * y[1] as f64);
        }
    }
    r
}

/// How far apart two looks are, in squared Lab units: the tone (L*
/// percentiles), the zone casts, chroma and spread.
pub fn distance(a: &LookStats, b: &LookStats) -> f64 {
    distance_with(a, b, false)
}

/// [`distance`], with the HSL bands' colours too when `bands`.
pub fn distance_with(a: &LookStats, b: &LookStats, bands: bool) -> f64 {
    residuals(a, b, bands).iter().map(|r| r * r).sum()
}

/// The groups of settings matching may change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Part {
    WhiteBalance,
    Tone,
    Color,
    Grading,
    /// The Color Mixer: hue, saturation and luminance per colour band.
    Hsl,
}

impl Part {
    pub const ALL: [Part; 5] = [
        Part::WhiteBalance,
        Part::Tone,
        Part::Color,
        Part::Grading,
        Part::Hsl,
    ];

    pub fn parse(s: &str) -> Option<Part> {
        match s {
            "wb" | "white_balance" => Some(Part::WhiteBalance),
            "tone" => Some(Part::Tone),
            "color" | "colour" => Some(Part::Color),
            "grading" | "color_grading" => Some(Part::Grading),
            "hsl" | "mixer" | "color_mixer" => Some(Part::Hsl),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct MatchOptions {
    pub parts: Vec<Part>,
    /// Most renders to try.
    pub max_evaluations: usize,
}

impl Default for MatchOptions {
    fn default() -> Self {
        MatchOptions {
            parts: Part::ALL.to_vec(),
            max_evaluations: 600,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct MatchResult {
    /// Settings to set, with their new values (only those that change).
    pub changes: Vec<(String, Value)>,
    /// The distance to the references before and after.
    pub before: f64,
    pub after: f64,
    pub evaluations: usize,
}

/// One searched parameter: an offset from the photo's own setting.
#[derive(Debug, Clone)]
struct Param {
    key: String,
    lo: f64,
    hi: f64,
    /// Size of a "normal" move, for the penalty.
    scale: f64,
}

fn param(key: &str, range: f64, scale: f64) -> Param {
    Param {
        key: key.to_string(),
        lo: -range,
        hi: range,
        scale,
    }
}

/// The settings a part searches. `mired`, `tint` and the grading `*_u`,
/// `*_v` are stand-ins that [`Start::apply`] turns into real settings.
fn params(part: Part) -> Vec<Param> {
    match part {
        Part::WhiteBalance => vec![param("mired", 200.0, 100.0), param("tint", 50.0, 30.0)],
        Part::Tone => {
            let mut p = vec![param("Exposure2012", 3.0, 1.0)];
            for k in [
                "Contrast2012",
                "Highlights2012",
                "Shadows2012",
                "Whites2012",
                "Blacks2012",
            ] {
                p.push(param(k, 100.0, 50.0));
            }
            p
        }
        Part::Color => vec![
            param("Vibrance", 100.0, 50.0),
            param("Saturation", 100.0, 50.0),
        ],
        Part::Grading => ["shadow_u", "shadow_v", "highlight_u", "highlight_v"]
            .iter()
            .map(|k| param(k, 60.0, 40.0))
            .collect(),
        Part::Hsl => crate::color::BANDS
            .iter()
            .flat_map(|band| {
                ["Hue", "Saturation", "Luminance"]
                    .map(|kind| param(&format!("{kind}Adjustment{band}"), 100.0, 50.0))
            })
            .collect(),
    }
}

fn num(s: &Settings, key: &str) -> Option<f64> {
    s.get(key).and_then(Value::as_f64)
}

fn clamp_to_registry(key: &str, v: f64) -> f64 {
    match Registry::builtin().lookup(key) {
        Some(t) => v.clamp(t.min.unwrap_or(f64::MIN), t.max.unwrap_or(f64::MAX)),
        None => v,
    }
}

/// Where the search starts from: the photo's white balance and its Color
/// Grading tints as (u, v) = sat·(cos hue, sin hue).
struct Start {
    rendered: bool,
    temp: f64,
    tint: f64,
    grading: [[f64; 2]; 2],
}

const STAND_INS: [&str; 6] = [
    "mired",
    "tint",
    "shadow_u",
    "shadow_v",
    "highlight_u",
    "highlight_v",
];

impl Start {
    fn new(img: &CameraImage, base: &Settings) -> Start {
        let custom = base
            .get("WhiteBalance")
            .and_then(Value::as_str)
            .is_some_and(|m| m != "As Shot");
        let (temp, tint) = if img.rendered {
            (
                num(base, "IncrementalTemperature")
                    .filter(|_| custom)
                    .unwrap_or(0.0),
                num(base, "IncrementalTint")
                    .filter(|_| custom)
                    .unwrap_or(0.0),
            )
        } else {
            let shot = pipeline::as_shot_temp_tint(img);
            match num(base, "Temperature").filter(|_| custom) {
                Some(t) => (t, num(base, "Tint").unwrap_or(0.0)),
                None => shot,
            }
        };
        let tints = ["Shadow", "Highlight"].map(|zone| {
            let hue = num(base, &format!("ColorGrade{zone}Hue")).unwrap_or(0.0);
            let sat = num(base, &format!("ColorGrade{zone}Sat")).unwrap_or(0.0);
            let r = hue.to_radians();
            [sat * r.cos(), sat * r.sin()]
        });
        Start {
            rendered: img.rendered,
            temp,
            tint,
            grading: tints,
        }
    }

    /// `base` with the offsets `x` of `params` applied.
    fn apply(&self, base: &Settings, params: &[Param], x: &[f64], round: bool) -> Settings {
        let r = |v: f64, to: f64| if round { (v / to).round() * to } else { v };
        let get = |k: &str| params.iter().position(|p| p.key == k).map_or(0.0, |i| x[i]);
        let mut s = base.clone();
        let set = |s: &mut Settings, k: &str, v: f64| {
            s.insert(k.to_string(), Value::Number(clamp_to_registry(k, v)));
        };
        let (mired, tint) = (get("mired"), get("tint"));
        if mired != 0.0 || tint != 0.0 {
            s.insert("WhiteBalance".into(), Value::Text("Custom".into()));
            if self.rendered {
                set(&mut s, "IncrementalTemperature", r(self.temp + mired, 1.0));
                set(&mut s, "IncrementalTint", r(self.tint + tint, 1.0));
            } else {
                let m = (1e6 / self.temp - mired).max(20.0);
                set(&mut s, "Temperature", r(1e6 / m, 10.0));
                set(&mut s, "Tint", r(self.tint + tint, 1.0));
            }
        }
        for (p, &v) in params.iter().zip(x) {
            if v != 0.0 && !STAND_INS.contains(&p.key.as_str()) {
                let to = if p.key == "Exposure2012" { 0.05 } else { 1.0 };
                set(&mut s, &p.key, r(num(base, &p.key).unwrap_or(0.0) + v, to));
            }
        }
        for (z, zone) in ["Shadow", "Highlight"].iter().enumerate() {
            let lower = zone.to_ascii_lowercase();
            let (du, dv) = (get(&format!("{lower}_u")), get(&format!("{lower}_v")));
            if du == 0.0 && dv == 0.0 {
                continue;
            }
            let [u0, v0] = self.grading[z];
            let (u, v) = (u0 + du, v0 + dv);
            let sat = r(u.hypot(v).min(100.0), 1.0);
            let hue = r(v.atan2(u).to_degrees().rem_euclid(360.0), 1.0) % 360.0;
            set(&mut s, &format!("ColorGrade{zone}Hue"), hue);
            set(&mut s, &format!("ColorGrade{zone}Sat"), sat);
        }
        s
    }
}

/// Solve `(A + damping) x = b` for a small dense system (Gaussian
/// elimination with partial pivoting). `None` if it is singular.
fn solve_linear(mut a: Vec<Vec<f64>>, mut b: Vec<f64>) -> Option<Vec<f64>> {
    let n = b.len();
    for col in 0..n {
        let pivot = (col..n).max_by(|&i, &j| a[i][col].abs().total_cmp(&a[j][col].abs()))?;
        if a[pivot][col].abs() < 1e-12 {
            return None;
        }
        a.swap(col, pivot);
        b.swap(col, pivot);
        let (top, rest) = a.split_at_mut(col + 1);
        let pivot_row = &top[col];
        for (k, row) in rest.iter_mut().enumerate() {
            let f = row[col] / pivot_row[col];
            for (v, p) in row[col..].iter_mut().zip(&pivot_row[col..]) {
                *v -= f * p;
            }
            b[col + 1 + k] -= f * b[col];
        }
    }
    let mut x = vec![0.0; n];
    for row in (0..n).rev() {
        let s: f64 = (row + 1..n).map(|k| a[row][k] * x[k]).sum();
        x[row] = (b[row] - s) / a[row][row];
    }
    Some(x)
}

/// Find the settings that bring `img` (developed from `base`, small) closest
/// to `target`. `develop` turns settings into what the pipeline renders (the
/// caller adds lens and camera profiles).
///
/// White balance, tone, colour and grading are solved together first; the
/// Color Mixer after them, from that result, for the bands whose colours
/// both looks have.
pub fn solve(
    img: &CameraImage,
    base: &Settings,
    target: &LookStats,
    opts: &MatchOptions,
    develop: &dyn Fn(&Settings) -> DevelopSettings,
) -> MatchResult {
    let evaluations = std::cell::Cell::new(0usize);
    let look = |s: &Settings| {
        evaluations.set(evaluations.get() + 1);
        look_stats(&pipeline::render(img, &develop(s)))
    };
    let with_hsl = opts.parts.contains(&Part::Hsl);
    let start_look = look(base);
    let before = distance_with(&start_look, target, with_hsl);
    let global: Vec<Param> = opts
        .parts
        .iter()
        .filter(|p| **p != Part::Hsl)
        .flat_map(|&p| params(p))
        .collect();
    let mut current = base.clone();
    // The global settings first. With the Color Mixer on, they see the
    // bands' colours too, so they don't take on what one band's change did.
    if !global.is_empty() {
        let share = if with_hsl { 0.35 } else { 1.0 };
        let budget = (opts.max_evaluations as f64 * share) as usize;
        let budget = budget.saturating_sub(evaluations.get());
        current = solve_stage(img, &current, target, &global, with_hsl, budget, &look);
    }
    // Then everything together, with the bands whose colours both looks have.
    if with_hsl {
        let now = look(&current);
        let mut all = global.clone();
        all.extend(
            params(Part::Hsl)
                .into_iter()
                .enumerate()
                .filter(|(i, _)| now.bands[i / 3][0].min(target.bands[i / 3][0]) >= MIN_BAND_SHARE)
                .map(|(_, p)| p),
        );
        if all.len() > global.len() {
            let budget = opts.max_evaluations.saturating_sub(evaluations.get());
            current = solve_stage(img, &current, target, &all, true, budget, &look);
        }
    }
    let after = distance_with(&look(&current), target, with_hsl);
    // Changed settings only (one left at its default isn't a change), the
    // white balance first: its mode, then Tint, then Temperature.
    let registry = Registry::builtin();
    let mut changes: Vec<(String, Value)> = current
        .into_iter()
        .filter(|(k, v)| match base.get(k) {
            Some(old) => old != v,
            None => v.as_f64().is_none() || registry.numeric_default(k) != v.as_f64(),
        })
        .collect();
    let rank = |k: &str| match k {
        "WhiteBalance" => 0,
        "Tint" | "IncrementalTint" => 1,
        "Temperature" | "IncrementalTemperature" => 2,
        _ => 3,
    };
    changes.sort_by_key(|(k, _)| rank(k));
    MatchResult {
        changes,
        before,
        after,
        evaluations: evaluations.get(),
    }
}

/// One Levenberg–Marquardt search over `params`, from `base`, within
/// `budget` renders; the result rounded as the sliders show it.
///
/// It minimises the look's residuals plus a penalty residual per setting
/// (its change over a typical move), in units of those typical moves,
/// with the Jacobian from finite differences: one small render per setting
/// per iteration. Settings stay within their ranges.
fn solve_stage(
    img: &CameraImage,
    base: &Settings,
    target: &LookStats,
    params: &[Param],
    bands: bool,
    budget: usize,
    look: &dyn Fn(&Settings) -> LookStats,
) -> Settings {
    let start = Start::new(img, base);
    let n = params.len();
    let used = std::cell::Cell::new(0usize);
    // The Color Mixer's sliders are many: small moves of them mostly fit
    // noise, so they cost more and the smallest are dropped at the end.
    let mixer = |p: &Param| p.key.contains("Adjustment");
    let weights: Vec<f64> = params
        .iter()
        .map(|p| if mixer(p) { 4.0 * PENALTY } else { PENALTY }.sqrt())
        .collect();
    let offsets = |u: &[f64]| -> Vec<f64> {
        params
            .iter()
            .zip(u)
            .map(|(p, v)| (v * p.scale).clamp(p.lo, p.hi))
            .collect()
    };
    let resid = |u: &[f64]| -> Vec<f64> {
        used.set(used.get() + 1);
        let mut r = residuals(
            &look(&start.apply(base, params, &offsets(u), false)),
            target,
            bands,
        );
        r.extend(u.iter().zip(&weights).map(|(v, w)| w * v));
        r
    };
    let sq = |r: &[f64]| r.iter().map(|v| v * v).sum::<f64>();
    let mut u = vec![0.0; n];
    let mut r = resid(&u);
    let mut cost = sq(&r);
    let mut lambda = 1e-2;
    const H: f64 = 0.02;
    while n > 0 && used.get() + n < budget {
        let cols: Vec<Vec<f64>> = (0..n)
            .map(|j| {
                let mut v = u.clone();
                v[j] += H;
                resid(&v).iter().zip(&r).map(|(a, b)| (a - b) / H).collect()
            })
            .collect();
        let jtj: Vec<Vec<f64>> = (0..n)
            .map(|i| {
                (0..n)
                    .map(|j| cols[i].iter().zip(&cols[j]).map(|(a, b)| a * b).sum())
                    .collect()
            })
            .collect();
        let jtr: Vec<f64> = (0..n)
            .map(|i| -cols[i].iter().zip(&r).map(|(a, b)| a * b).sum::<f64>())
            .collect();
        let mut accepted = false;
        for _ in 0..6 {
            if used.get() >= budget {
                break;
            }
            let damped: Vec<Vec<f64>> = (0..n)
                .map(|i| {
                    let mut row = jtj[i].clone();
                    row[i] += lambda * (jtj[i][i] + 1e-6);
                    row
                })
                .collect();
            let Some(step) = solve_linear(damped, jtr.clone()) else {
                lambda *= 10.0;
                continue;
            };
            let trial: Vec<f64> = u
                .iter()
                .zip(&step)
                .zip(params)
                .map(|((v, d), p)| (v + d).clamp(p.lo / p.scale, p.hi / p.scale))
                .collect();
            let rt = resid(&trial);
            let ct = sq(&rt);
            if ct < cost {
                let gain = cost - ct;
                u = trial;
                r = rt;
                cost = ct;
                lambda = (lambda / 3.0).max(1e-7);
                // Converged when the step barely helped.
                accepted = gain > 1e-4 * cost.max(1e-9);
                break;
            }
            lambda *= 4.0;
        }
        if !accepted {
            break;
        }
    }
    if std::env::var_os("AMB_MATCH_DEBUG").is_some() {
        eprintln!("stage: cost {cost:.2} renders {} u {u:?}", used.get());
    }
    let mut x = offsets(&u);
    for (v, p) in x.iter_mut().zip(params) {
        if mixer(p) && v.abs() < MIN_MIXER_MOVE {
            *v = 0.0;
        }
    }
    start.apply(base, params, &x, true)
}

/// Color Mixer moves smaller than this are left out of the result.
const MIN_MIXER_MOVE: f64 = 5.0;

/// Weight of the penalty on each setting's change (per typical move, squared).
const PENALTY: f64 = 0.5;

/// Settings as the pipeline renders them, with no lens or camera profiles.
pub fn develop_plain(s: &Settings) -> DevelopSettings {
    DevelopSettings::from_settings(s).0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn photo(w: usize, h: usize) -> CameraImage {
        let mut rgb = Vec::with_capacity(w * h * 3);
        for y in 0..h {
            for x in 0..w {
                let (u, v) = (x as f32 / w as f32, y as f32 / h as f32);
                let mut p = [
                    0.02 + 0.5 * u * u,
                    0.03 + 0.4 * v,
                    0.04 + 0.3 * (1.0 - u) * v,
                ];
                if (x / 6 + y / 6) % 3 == 0 {
                    p = [p[2] * 1.4, p[0] * 0.6, p[1]];
                }
                rgb.extend_from_slice(&p);
            }
        }
        CameraImage {
            width: w,
            height: h,
            rgb,
            xyz_to_cam: [
                [0.7186, -0.2108, -0.0606],
                [-0.4386, 1.2001, 0.2584],
                [-0.0577, 0.1271, 0.6585],
            ],
            as_shot_neutral: [0.45, 1.0, 0.62],
            clip: [1.0, 1.0, 1.0],
            rendered: false,
            make: "Test".into(),
            model: "Synthetic".into(),
            shot: Default::default(),
            metadata: Default::default(),
        }
    }

    fn settings(tags: &[(&str, f64)]) -> Settings {
        tags.iter()
            .map(|(k, v)| (k.to_string(), Value::Number(*v)))
            .collect()
    }

    #[test]
    fn stats_of_the_same_image_are_the_same_distance_zero() {
        let img = photo(48, 32);
        let s = look_stats(&pipeline::render(&img, &develop_plain(&Settings::new())));
        assert_eq!(distance(&s, &s), 0.0);
        assert_eq!(average(std::slice::from_ref(&s)).unwrap(), s);
        assert!(s.lightness.windows(2).all(|w| w[0] <= w[1]), "{s:?}");
    }

    #[test]
    fn finds_a_known_edit_again() {
        let img = photo(48, 32);
        // The reference: the same photo, warmer, brighter, flatter, less saturated.
        let mut edit = settings(&[
            ("Exposure2012", 0.6),
            ("Contrast2012", -30.0),
            ("Saturation", -25.0),
            ("Tint", 0.0),
        ]);
        let (t0, _) = pipeline::as_shot_temp_tint(&img);
        edit.insert("WhiteBalance".into(), Value::Text("Custom".into()));
        edit.insert("Temperature".into(), Value::Number((t0 + 900.0).round()));
        let target = look_stats(&pipeline::render(&img, &develop_plain(&edit)));
        let opts = MatchOptions {
            parts: vec![Part::WhiteBalance, Part::Tone, Part::Color],
            max_evaluations: 400,
        };
        let r = solve(&img, &Settings::new(), &target, &opts, &develop_plain);
        assert!(r.after < r.before / 20.0, "{r:?}");
        assert!(r.after < 2.0, "{r:?}");
        let got: Settings = r.changes.iter().cloned().collect();
        let n = |k: &str| num(&got, k).unwrap_or(0.0);
        assert!(n("Temperature") > t0 + 300.0, "{r:?}");
        assert!(n("Exposure2012") > 0.2, "{r:?}");
        assert!(n("Saturation") + n("Vibrance") < -5.0, "{r:?}");
        assert!(r.evaluations <= 401, "{r:?}");
    }

    #[test]
    fn the_color_mixer_moves_one_band() {
        let img = photo(48, 32);
        let edit = settings(&[("SaturationAdjustmentRed", -70.0)]);
        let target = look_stats(&pipeline::render(&img, &develop_plain(&edit)));
        let opts = MatchOptions {
            parts: vec![Part::Hsl],
            max_evaluations: 300,
        };
        let r = solve(&img, &Settings::new(), &target, &opts, &develop_plain);
        assert!(r.after < r.before / 4.0, "{r:?}");
        let got: Settings = r.changes.iter().cloned().collect();
        assert!(
            num(&got, "SaturationAdjustmentRed").unwrap_or(0.0) < -25.0,
            "{r:?}"
        );
    }

    #[test]
    fn grading_tints_the_shadows() {
        let img = photo(48, 32);
        let edit = settings(&[
            ("ColorGradeShadowHue", 220.0),
            ("ColorGradeShadowSat", 40.0),
        ]);
        let target = look_stats(&pipeline::render(&img, &develop_plain(&edit)));
        let opts = MatchOptions {
            parts: vec![Part::Grading],
            max_evaluations: 200,
        };
        let r = solve(&img, &Settings::new(), &target, &opts, &develop_plain);
        assert!(r.after < r.before / 5.0, "{r:?}");
        let got: Settings = r.changes.iter().cloned().collect();
        let hue = num(&got, "ColorGradeShadowHue").unwrap_or(0.0);
        assert!((hue - 220.0).abs() < 40.0, "{r:?}");
    }
}
