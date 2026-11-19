// SPDX-License-Identifier: GPL-3.0-or-later
//! Finding the settings that bring one look to another.

use super::stats::{LookStats, MIN_BAND_SHARE, look_stats, residuals, weighted_distance};
use super::{MatchOptions, MatchResult, Part, Settings, Weights};
use crate::pipeline::{self, DevelopSettings};
use crate::raw::CameraImage;
use amb_presets::Value;
use amb_presets::registry::Registry;

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

pub(super) fn num(s: &Settings, key: &str) -> Option<f64> {
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
    let final_weights = Weights {
        bands: with_hsl,
        ..opts.weights
    };
    let before = weighted_distance(&start_look, target, &final_weights);
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
        current = solve_stage(
            img,
            &current,
            target,
            &global,
            &final_weights,
            budget,
            &look,
        );
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
            current = solve_stage(img, &current, target, &all, &final_weights, budget, &look);
        }
    }
    let after = weighted_distance(&look(&current), target, &final_weights);
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
    weights: &Weights,
    budget: usize,
    look: &dyn Fn(&Settings) -> LookStats,
) -> Settings {
    let start = Start::new(img, base);
    let n = params.len();
    let used = std::cell::Cell::new(0usize);
    // The Color Mixer's sliders are many: small moves of them mostly fit
    // noise, so they cost more and the smallest are dropped at the end.
    let mixer = |p: &Param| p.key.contains("Adjustment");
    let penalty: Vec<f64> = params
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
            weights,
        );
        r.extend(u.iter().zip(&penalty).map(|(v, w)| w * v));
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
