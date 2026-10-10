// SPDX-License-Identifier: GPL-3.0-or-later
//! Correlated colour temperature and tint ↔ CIE xy chromaticity.
//!
//! Lightroom/Camera Raw express white balance as Temperature (K) and Tint, with
//! Tint measured along Robertson's isotemperature lines in CIE 1960 UCS (u, v).
//! This is Robertson's method (A. R. Robertson, "Computation of correlated
//! color temperature and distribution temperature", JOSA 58, 1968) with the
//! isotherm table from Wyszecki & Stiles, *Color Science* (2nd ed.), and the
//! scaling conventions published in the DNG specification ecosystem:
//! a tint of 1 unit is 1/3000 of a (u, v) unit along the isotherm, and positive
//! tint means a greener illuminant (so neutralising it renders more magenta).
//!
//! The table-based method covers 1667 K and up (r = 0..600 mired); Adobe's
//! newer releases also extend below 2000 K, which still needs measuring
//! against Lightroom (docs/research/05 §2).
//!
//! To verify during the Lightroom month: the Temp/Tint readouts Lightroom shows
//! for the synthetic DNGs at known white points (exact interpolation details).

/// Tint units per unit of (u, v) distance along the isotherm (negative: positive tint = green side).
pub const TINT_SCALE: f64 = -3000.0;

/// Robertson isotemperature lines: reciprocal megakelvin (mired) r, the
/// Planckian locus point (u, v) at that temperature, and the isotherm slope t.
const ISOTHERMS: [[f64; 4]; 31] = [
    [0.0, 0.18006, 0.26352, -0.24341],
    [10.0, 0.18066, 0.26589, -0.25479],
    [20.0, 0.18133, 0.26846, -0.26876],
    [30.0, 0.18208, 0.27119, -0.28539],
    [40.0, 0.18293, 0.27407, -0.30470],
    [50.0, 0.18388, 0.27709, -0.32675],
    [60.0, 0.18494, 0.28021, -0.35156],
    [70.0, 0.18611, 0.28342, -0.37915],
    [80.0, 0.18740, 0.28668, -0.40955],
    [90.0, 0.18880, 0.28997, -0.44278],
    [100.0, 0.19032, 0.29326, -0.47888],
    [125.0, 0.19462, 0.30141, -0.58204],
    [150.0, 0.19962, 0.30921, -0.70471],
    [175.0, 0.20525, 0.31647, -0.84901],
    [200.0, 0.21142, 0.32312, -1.0182],
    [225.0, 0.21807, 0.32909, -1.2168],
    [250.0, 0.22511, 0.33439, -1.4512],
    [275.0, 0.23247, 0.33904, -1.7298],
    [300.0, 0.24010, 0.34308, -2.0637],
    [325.0, 0.24702, 0.34655, -2.4681],
    [350.0, 0.25591, 0.34951, -2.9641],
    [375.0, 0.26400, 0.35200, -3.5814],
    [400.0, 0.27218, 0.35407, -4.3633],
    [425.0, 0.28039, 0.35577, -5.3762],
    [450.0, 0.28863, 0.35714, -6.7262],
    [475.0, 0.29685, 0.35823, -8.5955],
    [500.0, 0.30505, 0.35907, -11.324],
    [525.0, 0.31320, 0.35968, -15.628],
    [550.0, 0.32129, 0.36011, -23.325],
    [575.0, 0.32931, 0.36038, -40.770],
    [600.0, 0.33724, 0.36051, -116.45],
];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TempTint {
    pub temperature: f64,
    pub tint: f64,
}

/// Unit vector along an isotherm with slope `t`: (1, t) normalised.
fn isotherm_dir(t: f64) -> (f64, f64) {
    let len = (1.0 + t * t).sqrt();
    (1.0 / len, t / len)
}

fn uv_to_xy(u: f64, v: f64) -> (f64, f64) {
    let d = u - 4.0 * v + 2.0;
    (1.5 * u / d, v / d)
}

fn xy_to_uv(x: f64, y: f64) -> (f64, f64) {
    let d = 1.5 - x + 6.0 * y;
    (2.0 * x / d, 3.0 * y / d)
}

/// White point chromaticity for a temperature (K) and tint.
pub fn temp_tint_to_xy(temperature: f64, tint: f64) -> (f64, f64) {
    let r = 1.0e6 / temperature;
    let offset = tint / TINT_SCALE;
    let i = (0..30).find(|&i| r < ISOTHERMS[i + 1][0]).unwrap_or(29);
    let (lo, hi) = (ISOTHERMS[i], ISOTHERMS[i + 1]);
    // Weight of the lower isotherm (extrapolates past the table ends).
    let f = (hi[0] - r) / (hi[0] - lo[0]);
    let mut u = lo[1] * f + hi[1] * (1.0 - f);
    let mut v = lo[2] * f + hi[2] * (1.0 - f);
    let (du1, dv1) = isotherm_dir(lo[3]);
    let (du2, dv2) = isotherm_dir(hi[3]);
    let (du, dv) = (du1 * f + du2 * (1.0 - f), dv1 * f + dv2 * (1.0 - f));
    let len = du.hypot(dv);
    u += du / len * offset;
    v += dv / len * offset;
    uv_to_xy(u, v)
}

/// Temperature (K) and tint of a white point chromaticity.
pub fn xy_to_temp_tint(x: f64, y: f64) -> TempTint {
    let (u, v) = xy_to_uv(x, y);
    let (mut last_dt, mut last_du, mut last_dv) = (0.0, 0.0, 0.0);
    for i in 1..=30 {
        let [r, iu, iv, t] = ISOTHERMS[i];
        let (du, dv) = isotherm_dir(t);
        let (uu, vv) = (u - iu, v - iv);
        // Signed distance of the point from this isotherm line.
        let mut dt = -uu * dv + vv * du;
        if dt <= 0.0 || i == 30 {
            if dt > 0.0 {
                dt = 0.0;
            }
            let dt = -dt;
            // Weight of isotherm i-1: proportional to the distance from isotherm i.
            let f = if i == 1 || last_dt + dt == 0.0 {
                0.0
            } else {
                dt / (last_dt + dt)
            };
            let [r0, u0, v0, _] = ISOTHERMS[i - 1];
            let temperature = 1.0e6 / (r0 * f + r * (1.0 - f));
            let uu = u - (u0 * f + iu * (1.0 - f));
            let vv = v - (v0 * f + iv * (1.0 - f));
            let (du, dv) = (du * (1.0 - f) + last_du * f, dv * (1.0 - f) + last_dv * f);
            let len = du.hypot(dv);
            let tint = (uu * du / len + vv * dv / len) * TINT_SCALE;
            return TempTint { temperature, tint };
        }
        (last_dt, last_du, last_dv) = (dt, du, dv);
    }
    unreachable!("loop returns at i == 30")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() <= tol
    }

    #[test]
    fn planckian_points_match_reference_chromaticities() {
        // Planckian locus (CIE 1931 2°) reference values.
        for (t, x, y) in [
            (2856.0, 0.4476, 0.4074),
            (5000.0, 0.3451, 0.3516),
            (6500.0, 0.3135, 0.3237),
        ] {
            let (px, py) = temp_tint_to_xy(t, 0.0);
            assert!(
                close(px, x, 0.0015) && close(py, y, 0.0015),
                "{t} K → ({px:.4}, {py:.4}), want ({x}, {y})"
            );
        }
    }

    #[test]
    fn round_trips_across_range() {
        for t in [
            2000.0, 2856.0, 3200.0, 4000.0, 5500.0, 6500.0, 8000.0, 12000.0, 25000.0,
        ] {
            for tint in [-150.0, -40.0, 0.0, 10.0, 150.0] {
                let (x, y) = temp_tint_to_xy(t, tint);
                let back = xy_to_temp_tint(x, y);
                // Not exactly invertible: isotherms are not parallel, so at large tints the
                // forward (temperature-weighted) and inverse (distance-weighted) interpolations
                // disagree slightly. The error stays far below one slider step.
                assert!(
                    close(back.temperature, t, t * 5e-4),
                    "{t}/{tint} → {back:?}"
                );
                assert!(close(back.tint, tint, 0.05), "{t}/{tint} → {back:?}");
            }
        }
    }

    #[test]
    fn standard_illuminants() {
        // D65 sits slightly on the green side of the locus near 6500 K; D50 near 5000 K.
        let d65 = xy_to_temp_tint(0.3127, 0.3290);
        assert!(close(d65.temperature, 6504.0, 15.0), "{d65:?}");
        assert!(d65.tint > 0.0 && d65.tint < 15.0, "{d65:?}");
        let d50 = xy_to_temp_tint(0.3457, 0.3585);
        assert!(close(d50.temperature, 5003.0, 15.0), "{d50:?}");
        assert!(d50.tint > 0.0, "{d50:?}");
    }

    #[test]
    fn positive_tint_is_greener_illuminant() {
        // Green side = above the locus in (u, v): larger v for similar u.
        let (x0, y0) = temp_tint_to_xy(5500.0, 0.0);
        let (x1, y1) = temp_tint_to_xy(5500.0, 50.0);
        let (_, v0) = xy_to_uv(x0, y0);
        let (_, v1) = xy_to_uv(x1, y1);
        assert!(v1 > v0);
    }
}
