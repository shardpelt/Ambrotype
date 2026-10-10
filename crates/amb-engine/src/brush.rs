// SPDX-License-Identifier: GPL-3.0-or-later
//! Lightroom's brush, for brush masks and brushed spots.
//!
//! A stroke's `Dabs` are a little state machine (`r`, `f`, `h` tokens set the
//! radius, flow and hardness; `d x y` stamps a dab), starting from the
//! stroke's `Radius`, `Flow` and `CenterWeight`. The dab model is the one
//! measured against Lightroom exports by the autoshade project (MIT licence,
//! © 2026 skymanbp; see NOTICE), on 29 controlled exports:
//!
//! ```text
//! ρ      = distance / (Radius · frame width)     a dab is a circle in pixels
//! k(ρ;h) = (1 − ρ^m(h))^n(h)                     0 ≤ ρ < 1, else 0
//! D(f)   = κf / (1 − f + κf)                     κ = 0.1284
//! α      = 1 − Π (1 − density · D(f) · k)        screen over every dab
//! ```
//!
//! with ln m and ln n cubic in the hardness h. Lightroom spaces dabs 0.2 × the
//! radius apart, so dabs are stamped as given. Erasing is ours (Lightroom's
//! storage of it is not known): a stroke with `MaskValue` 0 inside a brush
//! that paints multiplies the coverage by Π (1 − D(f) · k).

use std::collections::BTreeMap;

use amb_presets::Value;
use amb_presets::masks::{self, DabToken, num};
use rayon::prelude::*;

/// One resolved dab.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Dab {
    /// Centre, image fractions (x of the width, y of the height).
    pub x: f64,
    pub y: f64,
    /// Radius as a fraction of the frame width.
    pub radius: f64,
    /// Alpha at the centre: density × D(flow).
    pub deposit: f32,
    /// Kernel exponents (m / 2 for the squared-distance form, n).
    pub half_m: f32,
    pub n: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stroke {
    pub dabs: Vec<Dab>,
    pub erase: bool,
}

/// Kernel exponents (m, n) at hardness `h` (`CenterWeight`), clamped to 0..1
/// where the fit holds.
pub fn kernel_exponents(h: f64) -> (f64, f64) {
    let h = if h.is_nan() { 0.0 } else { h.clamp(0.0, 1.0) };
    let ln_m = ((-5.272996 * h + 9.420690) * h - 1.864655) * h + 0.605303;
    let ln_n = ((-5.976385 * h + 12.872100) * h - 7.987943) * h + 1.861162;
    (ln_m.exp(), ln_n.exp())
}

/// Alpha one dab lays down at flow `f`: an odds law, with D(1) = 1.
pub fn flow_deposit(f: f64) -> f64 {
    const KAPPA: f64 = 0.1284;
    let f = f.clamp(0.0, 1.0);
    let kf = KAPPA * f;
    let denom = 1.0 - f + kf;
    if denom <= 0.0 {
        1.0
    } else {
        (kf / denom).clamp(0.0, 1.0)
    }
}

/// The dab falloff at squared normalised distance `rho2`.
#[inline]
fn kernel(rho2: f32, half_m: f32, n: f32) -> f32 {
    if rho2.is_nan() || rho2 >= 1.0 {
        return 0.0;
    }
    if rho2 <= 0.0 {
        return 1.0;
    }
    (1.0 - rho2.powf(half_m)).max(0.0).powf(n)
}

/// Read one `Mask/Paint` entry. `density` overrides its `MaskValue` (erasing
/// strokes stamp at full strength).
pub fn stroke(entry: &BTreeMap<String, Value>, density: f64, erase: bool) -> Stroke {
    let mut radius = num(entry, "Radius").unwrap_or(0.05);
    let mut flow = num(entry, "Flow").unwrap_or(1.0);
    let mut hard = num(entry, "CenterWeight").unwrap_or(0.0);
    let mut exps = kernel_exponents(hard);
    let density = density.clamp(0.0, 1.0);
    let mut dabs = Vec::new();
    if let Some(Value::Seq(items)) = entry.get("Dabs") {
        for t in items.iter().filter_map(Value::as_str) {
            match masks::parse_dab_token(t) {
                Some(DabToken::Radius(r)) => radius = r,
                Some(DabToken::Flow(f)) => flow = f,
                Some(DabToken::Hardness(h)) => {
                    hard = h;
                    exps = kernel_exponents(hard);
                }
                Some(DabToken::Dab(x, y)) => {
                    let deposit = (density * flow_deposit(flow)) as f32;
                    if radius > 0.0 && radius.is_finite() && deposit > 0.0 {
                        dabs.push(Dab {
                            x,
                            y,
                            radius,
                            deposit,
                            half_m: (exps.0 * 0.5) as f32,
                            n: exps.1 as f32,
                        });
                    }
                }
                None => {}
            }
        }
    }
    Stroke { dabs, erase }
}

/// Stamp `dabs` into `remain` (the product Π (1 − a·k), starting at 1) for an
/// output of `w × h` pixels. `centre` maps a dab's image fractions to output
/// pixel coordinates; `px_per_width` is output pixels per frame width.
/// Returns the touched output rectangle.
pub fn stamp(
    remain: &mut [f32],
    w: usize,
    h: usize,
    dabs: &[Dab],
    centre: impl Fn(f64, f64) -> (f64, f64),
    px_per_width: f64,
) -> Option<(usize, usize, usize, usize)> {
    let mut bounds: Option<(usize, usize, usize, usize)> = None;
    for d in dabs {
        let r = d.radius * px_per_width;
        if r.is_nan() || r <= 0.0 {
            continue;
        }
        let (cx, cy) = centre(d.x, d.y);
        let x0 = (cx - r).floor().max(0.0) as usize;
        let y0 = (cy - r).floor().max(0.0) as usize;
        let x1 = ((cx + r).ceil() + 1.0).clamp(0.0, w as f64) as usize;
        let y1 = ((cy + r).ceil() + 1.0).clamp(0.0, h as f64) as usize;
        if x0 >= x1 || y0 >= y1 {
            continue;
        }
        bounds = Some(match bounds {
            Some((a, b, c, e)) => (a.min(x0), b.min(y0), c.max(x1), e.max(y1)),
            None => (x0, y0, x1, y1),
        });
        let inv_r2 = (1.0 / (r * r)) as f32;
        remain[y0 * w..y1 * w]
            .par_chunks_mut(w)
            .enumerate()
            .for_each(|(j, row)| {
                let dy = ((y0 + j) as f64 - cy) as f32;
                for (x, p) in row.iter_mut().enumerate().take(x1).skip(x0) {
                    let dx = (x as f64 - cx) as f32;
                    let k = kernel((dx * dx + dy * dy) * inv_r2, d.half_m, d.n);
                    if k > 0.0 {
                        *p *= 1.0 - d.deposit * k;
                    }
                }
            });
    }
    bounds
}

/// Draw `strokes` into a coverage plane (0..1), in order: painting strokes
/// screen together, erasing ones take coverage away.
pub fn draw(
    w: usize,
    h: usize,
    strokes: &[Stroke],
    centre: impl Fn(f64, f64) -> (f64, f64) + Copy,
    px_per_width: f64,
) -> Vec<f32> {
    let mut alpha = vec![0.0f32; w * h];
    let mut remain = vec![1.0f32; w * h];
    for s in strokes {
        let Some((x0, y0, x1, y1)) = stamp(&mut remain, w, h, &s.dabs, centre, px_per_width) else {
            continue;
        };
        for y in y0..y1 {
            let row = y * w;
            for i in row + x0..row + x1 {
                let r = remain[i];
                if s.erase {
                    alpha[i] *= r;
                } else {
                    alpha[i] = 1.0 - (1.0 - alpha[i]) * r;
                }
                remain[i] = 1.0;
            }
        }
    }
    alpha
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kernel_shape_matches_the_measurement() {
        // Soft brushes are close to a gaussian, hard ones to a top hat.
        let (m0, n0) = kernel_exponents(0.0);
        let (m1, n1) = kernel_exponents(1.0);
        assert!(
            (m0 - 1.832).abs() < 0.01 && (n0 - 6.43).abs() < 0.01,
            "{m0} {n0}"
        );
        assert!(m1 > 15.0 && n1 < 2.5, "{m1} {n1}");
        assert_eq!(kernel(0.0, (m0 * 0.5) as f32, n0 as f32), 1.0);
        assert_eq!(kernel(1.0, (m0 * 0.5) as f32, n0 as f32), 0.0);
        // A hard brush stays near full until close to its edge.
        let hard = kernel(0.64, (m1 * 0.5) as f32, n1 as f32);
        let soft = kernel(0.64, (m0 * 0.5) as f32, n0 as f32);
        assert!(hard > 0.9 && soft < 0.2, "{hard} {soft}");
    }

    #[test]
    fn flow_law() {
        assert_eq!(flow_deposit(1.0), 1.0);
        assert_eq!(flow_deposit(0.0), 0.0);
        assert!((flow_deposit(0.5) - 0.1138).abs() < 1e-3);
    }

    #[test]
    fn tokens_change_the_state_and_strokes_screen() {
        let entry: BTreeMap<String, Value> = [
            ("Radius".to_string(), Value::Text("0.1".into())),
            ("Flow".to_string(), Value::Text("1".into())),
            ("CenterWeight".to_string(), Value::Text("1".into())),
            (
                "Dabs".to_string(),
                Value::Seq(vec![
                    Value::Text("d 0.25 0.5".into()),
                    Value::Text("r 0.05".into()),
                    Value::Text("d 0.75 0.5".into()),
                ]),
            ),
        ]
        .into_iter()
        .collect();
        let s = stroke(&entry, 1.0, false);
        assert_eq!(s.dabs.len(), 2);
        assert_eq!(s.dabs[1].radius, 0.05);
        let (w, h) = (100, 50);
        let px = |x: f64, y: f64| (x * w as f64 - 0.5, y * h as f64 - 0.5);
        let a = draw(w, h, std::slice::from_ref(&s), px, w as f64);
        assert!(a[25 * w + 25] > 0.99);
        // The second dab is half the size: 8 px out is outside it.
        assert!(a[25 * w + 75 + 8] < 0.01 && a[25 * w + 25 + 6] > 0.9);
        let erase = Stroke {
            dabs: s.dabs[..1].to_vec(),
            erase: true,
        };
        let a = draw(w, h, &[s, erase], px, w as f64);
        assert!(a[25 * w + 25] < 0.01 && a[25 * w + 75] > 0.99);
    }
}
