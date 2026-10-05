//! Interpolating calibration samples to a shot's focal length, aperture and
//! focus distance. Shared by the lensfun and LCP readers.

/// Focus distance assumed when the shot's is unknown (lensfun's "infinity").
pub(crate) const DEFAULT_DISTANCE_M: f64 = 1000.0;

/// The key to interpolate at: the shot's value, or the only key there is when
/// the shot's is unknown (a prime lens). `None` otherwise.
pub(crate) fn pick_key(keys: impl IntoIterator<Item = f64>, at: Option<f64>) -> Option<f64> {
    if let Some(x) = at.filter(|x| x.is_finite()) {
        return Some(x);
    }
    let mut keys = keys.into_iter();
    let first = keys.next()?;
    keys.all(|k| (k - first).abs() < 1e-6).then_some(first)
}

/// Linear interpolation over samples sorted by key, clamped to the ends.
/// When `lerp` cannot blend two samples (different model kinds) the nearer
/// one is taken.
pub(crate) fn interpolate<T: Clone>(
    samples: &[(f64, T)],
    x: f64,
    lerp: impl Fn(&T, &T, f64) -> Option<T>,
) -> Option<T> {
    let first = samples.first()?;
    let last = samples.last()?;
    if x <= first.0 {
        return Some(first.1.clone());
    }
    if x >= last.0 {
        return Some(last.1.clone());
    }
    let i = samples.iter().position(|(k, _)| *k >= x)?;
    let (k1, v1) = &samples[i];
    if (*k1 - x).abs() < 1e-9 || i == 0 {
        return Some(v1.clone());
    }
    let (k0, v0) = &samples[i - 1];
    let t = (x - k0) / (k1 - k0);
    lerp(v0, v1, t).or_else(|| Some(if t < 0.5 { v0.clone() } else { v1.clone() }))
}

/// Index of the distance nearest to `target` in log space; with no target, the
/// largest distance.
pub(crate) fn nearest_distance(distances: &[f64], target: Option<f64>) -> Option<usize> {
    let ln = |d: f64| d.max(1e-3).ln();
    let key = |d: f64| match target {
        Some(t) => (ln(d) - ln(t)).abs(),
        None => -d,
    };
    (0..distances.len()).min_by(|&a, &b| key(distances[a]).total_cmp(&key(distances[b])))
}

/// One vignetting calibration sample.
#[derive(Debug, Clone)]
pub(crate) struct VigPoint<T> {
    pub focal: f64,
    /// f-number.
    pub aperture: f64,
    /// Focus distance in metres.
    pub distance: f64,
    pub value: T,
}

/// Vignetting at a shot: per calibrated focal length, the samples at the
/// distance nearest the shot's (log space) are interpolated in aperture
/// (stops); the per-focal results are then interpolated in focal length.
pub(crate) fn interpolate_vignetting<T: Clone>(
    points: &[VigPoint<T>],
    focal: Option<f64>,
    aperture: f64,
    distance: f64,
    lerp: impl Fn(&T, &T, f64) -> Option<T>,
) -> Option<T> {
    if !(aperture.is_finite() && aperture > 0.0) {
        return None;
    }
    let mut focals: Vec<f64> = points.iter().map(|p| p.focal).collect();
    focals.sort_by(f64::total_cmp);
    focals.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    let focal = pick_key(focals.iter().copied(), focal)?;
    let stops = aperture.log2();
    let mut per_focal: Vec<(f64, T)> = Vec::new();
    for &f in &focals {
        let at_f: Vec<&VigPoint<T>> = points
            .iter()
            .filter(|p| (p.focal - f).abs() < 1e-6)
            .collect();
        let distances: Vec<f64> = at_f.iter().map(|p| p.distance).collect();
        let Some(di) = nearest_distance(&distances, Some(distance)) else {
            continue;
        };
        let d = distances[di];
        let mut by_aperture: Vec<(f64, T)> = at_f
            .iter()
            .filter(|p| (p.distance.max(1e-3).ln() - d.max(1e-3).ln()).abs() < 1e-6)
            .filter(|p| p.aperture > 0.0)
            .map(|p| (p.aperture.log2(), p.value.clone()))
            .collect();
        by_aperture.sort_by(|a, b| a.0.total_cmp(&b.0));
        if let Some(v) = interpolate(&by_aperture, stops, &lerp) {
            per_focal.push((f, v));
        }
    }
    interpolate(&per_focal, focal, &lerp)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lerp(a: &f64, b: &f64, t: f64) -> Option<f64> {
        Some(a + (b - a) * t)
    }

    #[test]
    fn interpolates_and_clamps() {
        let s = [(10.0, 1.0), (20.0, 3.0)];
        assert_eq!(interpolate(&s, 15.0, lerp), Some(2.0));
        assert_eq!(interpolate(&s, 5.0, lerp), Some(1.0));
        assert_eq!(interpolate(&s, 25.0, lerp), Some(3.0));
        assert_eq!(interpolate(&s, 12.0, |_, _, _| None), Some(1.0));
        assert_eq!(interpolate::<f64>(&[], 1.0, lerp), None);
    }

    #[test]
    fn picks_key_for_primes_only() {
        assert_eq!(pick_key([35.0, 35.0], None), Some(35.0));
        assert_eq!(pick_key([24.0, 70.0], None), None);
        assert_eq!(pick_key([24.0, 70.0], Some(50.0)), Some(50.0));
    }

    #[test]
    fn vignetting_in_stops_and_nearest_distance() {
        let p = |focal, aperture, distance, value| VigPoint {
            focal,
            aperture,
            distance,
            value,
        };
        let points = [
            p(50.0, 2.0, 1000.0, 0.0),
            p(50.0, 8.0, 1000.0, 4.0),
            p(50.0, 2.0, 0.5, 100.0),
        ];
        // f/4 is one stop of two between f/2 and f/8.
        let v = interpolate_vignetting(&points, Some(50.0), 4.0, 1000.0, lerp).unwrap();
        assert!((v - 2.0).abs() < 1e-12);
        let near = interpolate_vignetting(&points, Some(50.0), 4.0, 0.4, lerp).unwrap();
        assert_eq!(near, 100.0);
    }
}
