//! 3×3 colour matrices and standard colour-space conversions.

pub type Mat3 = [[f64; 3]; 3];

/// Linear ProPhoto (ROMM) RGB → XYZ (D50).
pub const PROPHOTO_TO_XYZ_D50: [[f64; 3]; 3] = [
    [0.797_674_9, 0.135_191_7, 0.031_353_4],
    [0.288_040_2, 0.711_874_1, 0.000_085_7],
    [0.0, 0.0, 0.825_210_0],
];

/// Linear sRGB → XYZ, Bradford-adapted from D65 to D50.
pub const SRGB_TO_XYZ_D50: [[f64; 3]; 3] = [
    [0.436_074_7, 0.385_064_9, 0.143_080_4],
    [0.222_504_5, 0.716_878_6, 0.060_616_9],
    [0.013_932_2, 0.097_104_5, 0.714_173_3],
];

pub fn invert3(m: &[[f64; 3]; 3]) -> [[f64; 3]; 3] {
    let det = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
        - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
    assert!(det.abs() > 1e-12, "singular matrix");
    let inv =
        |r1: usize, c1: usize, r2: usize, c2: usize| m[r1][c1] * m[r2][c2] - m[r1][c2] * m[r2][c1];
    [
        [
            inv(1, 1, 2, 2) / det,
            -inv(0, 1, 2, 2) / det,
            inv(0, 1, 1, 2) / det,
        ],
        [
            -inv(1, 0, 2, 2) / det,
            inv(0, 0, 2, 2) / det,
            -inv(0, 0, 1, 2) / det,
        ],
        [
            inv(1, 0, 2, 1) / det,
            -inv(0, 0, 2, 1) / det,
            inv(0, 0, 1, 1) / det,
        ],
    ]
}

pub fn mul3(a: &[[f64; 3]; 3], b: &[[f64; 3]; 3]) -> [[f64; 3]; 3] {
    let mut out = [[0.0; 3]; 3];
    for (i, row) in out.iter_mut().enumerate() {
        for (j, cell) in row.iter_mut().enumerate() {
            *cell = (0..3).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    out
}

pub fn apply3(m: &[[f64; 3]; 3], v: [f64; 3]) -> [f64; 3] {
    [0, 1, 2].map(|i| m[i][0] * v[0] + m[i][1] * v[1] + m[i][2] * v[2])
}

/// XYZ (D50) → linear sRGB (Bradford-adapted to D65).
pub fn xyz_d50_to_srgb() -> Mat3 {
    invert3(&SRGB_TO_XYZ_D50)
}

/// Bradford cone-response matrix.
const BRADFORD: Mat3 = [
    [0.8951, 0.2664, -0.1614],
    [-0.7502, 1.7135, 0.0367],
    [0.0389, -0.0685, 1.0296],
];

/// D50 white point in XYZ (Y = 1), as used by ICC/ProPhoto.
pub const D50_XYZ: [f64; 3] = [0.964_22, 1.0, 0.825_21];

/// xy chromaticity → XYZ with Y = 1.
pub fn xy_to_xyz(x: f64, y: f64) -> [f64; 3] {
    [x / y, 1.0, (1.0 - x - y) / y]
}

/// Bradford chromatic adaptation from white `src` to white `dst` (both XYZ).
pub fn bradford(src: [f64; 3], dst: [f64; 3]) -> Mat3 {
    let s = apply3(&BRADFORD, src);
    let d = apply3(&BRADFORD, dst);
    let scale = [
        [d[0] / s[0], 0.0, 0.0],
        [0.0, d[1] / s[1], 0.0],
        [0.0, 0.0, d[2] / s[2]],
    ];
    mul3(&invert3(&BRADFORD), &mul3(&scale, &BRADFORD))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bradford_maps_white_to_white() {
        let d65 = xy_to_xyz(0.3127, 0.3290);
        let m = bradford(d65, D50_XYZ);
        let w = apply3(&m, d65);
        for c in 0..3 {
            assert!((w[c] - D50_XYZ[c]).abs() < 1e-9);
        }
    }

    #[test]
    fn srgb_white_round_trip() {
        let white = apply3(&xyz_d50_to_srgb(), D50_XYZ);
        for c in white {
            assert!((c - 1.0).abs() < 2e-3, "{white:?}");
        }
    }
}
