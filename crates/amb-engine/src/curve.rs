//! Natural cubic spline through tone-curve control points.
//!
//! Camera Raw's point curves are natural cubic splines: C2-continuous with zero
//! second derivative at the end points, as described for the DNG reference
//! implementation (docs/research/05 §8). Control points use Lightroom's 0..255
//! axes; evaluation works on 0..1 and clamps the output to 0..1.

#[derive(Debug, Clone)]
pub struct Spline {
    xs: Vec<f64>,
    ys: Vec<f64>,
    /// Second derivatives at each knot.
    m: Vec<f64>,
}

impl Spline {
    /// Build from (x, y) points on 0..255 axes. Points are sorted by x; duplicate x values
    /// keep the last point. Returns `None` for fewer than two distinct points.
    pub fn from_points_255(points: &[(f64, f64)]) -> Option<Spline> {
        let mut pts: Vec<(f64, f64)> = points.iter().map(|(x, y)| (x / 255.0, y / 255.0)).collect();
        pts.sort_by(|a, b| a.0.total_cmp(&b.0));
        pts.dedup_by(|later, earlier| {
            let same = (later.0 - earlier.0).abs() < 1e-9;
            if same {
                *earlier = *later;
            }
            same
        });
        if pts.len() < 2 {
            return None;
        }
        let xs: Vec<f64> = pts.iter().map(|p| p.0).collect();
        let ys: Vec<f64> = pts.iter().map(|p| p.1).collect();
        let n = xs.len();
        let mut m = vec![0.0; n];
        if n > 2 {
            // Tridiagonal system for the interior second derivatives (natural ends: m0 = mn = 0).
            let mut c = vec![0.0; n];
            let mut d = vec![0.0; n];
            for i in 1..n - 1 {
                let h0 = xs[i] - xs[i - 1];
                let h1 = xs[i + 1] - xs[i];
                let a = h0;
                let b = 2.0 * (h0 + h1);
                let cc = h1;
                let rhs = 6.0 * ((ys[i + 1] - ys[i]) / h1 - (ys[i] - ys[i - 1]) / h0);
                let denom = b - a * c[i - 1];
                c[i] = cc / denom;
                d[i] = (rhs - a * d[i - 1]) / denom;
            }
            for i in (1..n - 1).rev() {
                m[i] = d[i] - c[i] * m[i + 1];
            }
        }
        Some(Spline { xs, ys, m })
    }

    /// Whether the curve is the identity (within rounding), so it can be skipped.
    pub fn is_identity(&self) -> bool {
        (0..=16).all(|i| {
            let x = f64::from(i) / 16.0;
            (self.eval(x) - x).abs() < 1e-6
        })
    }

    /// Evaluate at `x` in 0..1. Outside the first/last knot the curve is flat at the end values.
    pub fn eval(&self, x: f64) -> f64 {
        let n = self.xs.len();
        if x <= self.xs[0] {
            return self.ys[0].clamp(0.0, 1.0);
        }
        if x >= self.xs[n - 1] {
            return self.ys[n - 1].clamp(0.0, 1.0);
        }
        let i = self
            .xs
            .partition_point(|&k| k <= x)
            .saturating_sub(1)
            .min(n - 2);
        let h = self.xs[i + 1] - self.xs[i];
        let a = (self.xs[i + 1] - x) / h;
        let b = (x - self.xs[i]) / h;
        let y = a * self.ys[i]
            + b * self.ys[i + 1]
            + ((a * a * a - a) * self.m[i] + (b * b * b - b) * self.m[i + 1]) * h * h / 6.0;
        y.clamp(0.0, 1.0)
    }

    /// Sample into a lookup table of `size` entries over 0..1.
    pub fn lut(&self, size: usize) -> Vec<f32> {
        (0..size)
            .map(|i| self.eval(i as f64 / (size - 1) as f64) as f32)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passes_through_knots_and_identity() {
        let s =
            Spline::from_points_255(&[(0.0, 0.0), (64.0, 80.0), (192.0, 200.0), (255.0, 255.0)])
                .unwrap();
        assert!((s.eval(64.0 / 255.0) - 80.0 / 255.0).abs() < 1e-12);
        assert!((s.eval(192.0 / 255.0) - 200.0 / 255.0).abs() < 1e-12);
        let id = Spline::from_points_255(&[(0.0, 0.0), (255.0, 255.0)]).unwrap();
        assert!(id.is_identity());
        assert!(!s.is_identity());
    }

    #[test]
    fn natural_spline_matches_reference_value() {
        // Natural cubic spline through (0,0), (0.5,0.75), (1,1). With h = 0.5 the interior
        // equation is 2(h+h)·m1 = 6·((1-0.75)/h - 0.75/h), so m1 = -6 / 2 = -3.
        let s = Spline::from_points_255(&[(0.0, 0.0), (127.5, 191.25), (255.0, 255.0)]).unwrap();
        // At x = 0.25: a = b = 0.5, y = 0.375 + (0.125 - 0.5) * (-3) * 0.25 / 6 = 0.375 + 0.046875.
        assert!((s.eval(0.25) - 0.421_875).abs() < 1e-12, "{}", s.eval(0.25));
    }

    #[test]
    fn flat_outside_end_points_and_rejects_degenerate() {
        let s = Spline::from_points_255(&[(20.0, 10.0), (235.0, 250.0)]).unwrap();
        assert!((s.eval(0.0) - 10.0 / 255.0).abs() < 1e-12);
        assert!((s.eval(1.0) - 250.0 / 255.0).abs() < 1e-12);
        assert!(Spline::from_points_255(&[(10.0, 10.0)]).is_none());
        assert!(Spline::from_points_255(&[(10.0, 10.0), (10.0, 20.0)]).is_none());
    }
}
