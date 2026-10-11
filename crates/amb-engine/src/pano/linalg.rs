// SPDX-License-Identifier: GPL-3.0-or-later
//! Small dense linear algebra for panorama alignment: 3-vectors, rotations,
//! symmetric eigenproblems (Jacobi), absolute orientation (Horn's
//! quaternion method) and a linear solver for the bundle adjustment.
// Matrix code reads best with explicit indices.
#![allow(clippy::needless_range_loop)]

pub type V3 = [f64; 3];
pub type M3 = [[f64; 3]; 3];

pub const IDENTITY: M3 = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

pub fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

pub fn norm(a: V3) -> f64 {
    dot(a, a).sqrt()
}

pub fn normalize(a: V3) -> V3 {
    let n = norm(a);
    if n > 0.0 { a.map(|v| v / n) } else { a }
}

pub fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub fn mul(a: &M3, b: &M3) -> M3 {
    std::array::from_fn(|i| std::array::from_fn(|j| (0..3).map(|k| a[i][k] * b[k][j]).sum()))
}

pub fn transpose(a: &M3) -> M3 {
    std::array::from_fn(|i| std::array::from_fn(|j| a[j][i]))
}

pub fn apply(m: &M3, v: V3) -> V3 {
    std::array::from_fn(|i| m[i][0] * v[0] + m[i][1] * v[1] + m[i][2] * v[2])
}

/// The rotation by angle |w| about axis w (Rodrigues).
pub fn rodrigues(w: V3) -> M3 {
    let t = norm(w);
    if t < 1e-12 {
        // First order: I + [w]×.
        return [[1.0, -w[2], w[1]], [w[2], 1.0, -w[0]], [-w[1], w[0], 1.0]];
    }
    let k = w.map(|v| v / t);
    let (s, c) = t.sin_cos();
    let v = 1.0 - c;
    [
        [
            c + k[0] * k[0] * v,
            k[0] * k[1] * v - k[2] * s,
            k[0] * k[2] * v + k[1] * s,
        ],
        [
            k[1] * k[0] * v + k[2] * s,
            c + k[1] * k[1] * v,
            k[1] * k[2] * v - k[0] * s,
        ],
        [
            k[2] * k[0] * v - k[1] * s,
            k[2] * k[1] * v + k[0] * s,
            c + k[2] * k[2] * v,
        ],
    ]
}

/// Eigenvalues (ascending) and eigenvectors (as columns, matching order) of
/// a symmetric N×N matrix, by cyclic Jacobi rotations.
pub fn eigen_symmetric<const N: usize>(mut a: [[f64; N]; N]) -> ([f64; N], [[f64; N]; N]) {
    let mut v = [[0.0; N]; N];
    for (i, row) in v.iter_mut().enumerate() {
        row[i] = 1.0;
    }
    for _ in 0..64 {
        let off: f64 = (0..N)
            .flat_map(|i| (0..N).filter(move |&j| j != i).map(move |j| (i, j)))
            .map(|(i, j)| a[i][j] * a[i][j])
            .sum();
        if off < 1e-30 {
            break;
        }
        for p in 0..N {
            for q in p + 1..N {
                if a[p][q].abs() < 1e-300 {
                    continue;
                }
                let theta = (a[q][q] - a[p][p]) / (2.0 * a[p][q]);
                let sign = if theta >= 0.0 { 1.0 } else { -1.0 };
                let t = sign / (theta.abs() + (theta * theta + 1.0).sqrt());
                let c = 1.0 / (t * t + 1.0).sqrt();
                let s = t * c;
                for k in 0..N {
                    let (akp, akq) = (a[k][p], a[k][q]);
                    a[k][p] = c * akp - s * akq;
                    a[k][q] = s * akp + c * akq;
                }
                for k in 0..N {
                    let (apk, aqk) = (a[p][k], a[q][k]);
                    a[p][k] = c * apk - s * aqk;
                    a[q][k] = s * apk + c * aqk;
                }
                for row in v.iter_mut() {
                    let (vkp, vkq) = (row[p], row[q]);
                    row[p] = c * vkp - s * vkq;
                    row[q] = s * vkp + c * vkq;
                }
            }
        }
    }
    let mut order: [usize; N] = std::array::from_fn(|i| i);
    order.sort_by(|&i, &j| a[i][i].total_cmp(&a[j][j]));
    let values = std::array::from_fn(|k| a[order[k]][order[k]]);
    let vectors = std::array::from_fn(|r| std::array::from_fn(|k| v[r][order[k]]));
    (values, vectors)
}

/// The rotation R minimising Σ w·|b − R a|² over weighted pairs (a, b)
/// (Horn, 1987: the largest eigenvector of a 4×4 matrix is the quaternion).
pub fn absolute_orientation(pairs: impl IntoIterator<Item = (V3, V3, f64)>) -> M3 {
    let mut s = [[0.0; 3]; 3];
    for (a, b, w) in pairs {
        for i in 0..3 {
            for j in 0..3 {
                s[i][j] += w * a[i] * b[j];
            }
        }
    }
    let [[sxx, sxy, sxz], [syx, syy, syz], [szx, szy, szz]] = s;
    let n = [
        [sxx + syy + szz, syz - szy, szx - sxz, sxy - syx],
        [syz - szy, sxx - syy - szz, sxy + syx, szx + sxz],
        [szx - sxz, sxy + syx, -sxx + syy - szz, syz + szy],
        [sxy - syx, szx + sxz, syz + szy, -sxx - syy + szz],
    ];
    let (_, vectors) = eigen_symmetric(n);
    let q: [f64; 4] = std::array::from_fn(|r| vectors[r][3]);
    let [w, x, y, z] = q;
    [
        [
            w * w + x * x - y * y - z * z,
            2.0 * (x * y - w * z),
            2.0 * (x * z + w * y),
        ],
        [
            2.0 * (x * y + w * z),
            w * w - x * x + y * y - z * z,
            2.0 * (y * z - w * x),
        ],
        [
            2.0 * (x * z - w * y),
            2.0 * (y * z + w * x),
            w * w - x * x - y * y + z * z,
        ],
    ]
}

/// Solve `a · x = b` (n×n, row-major) by Gaussian elimination with partial
/// pivoting; `None` when singular.
pub fn solve(mut a: Vec<Vec<f64>>, mut b: Vec<f64>) -> Option<Vec<f64>> {
    let n = b.len();
    for col in 0..n {
        let pivot = (col..n).max_by(|&i, &j| a[i][col].abs().total_cmp(&a[j][col].abs()))?;
        if a[pivot][col].abs() < 1e-300 {
            return None;
        }
        a.swap(col, pivot);
        b.swap(col, pivot);
        for row in col + 1..n {
            let f = a[row][col] / a[col][col];
            if f != 0.0 {
                for k in col..n {
                    a[row][k] -= f * a[col][k];
                }
                b[row] -= f * b[col];
            }
        }
    }
    let mut x = vec![0.0; n];
    for row in (0..n).rev() {
        let s: f64 = (row + 1..n).map(|k| a[row][k] * x[k]).sum();
        x[row] = (b[row] - s) / a[row][row];
    }
    x.iter().all(|v| v.is_finite()).then_some(x)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: &M3, b: &M3) -> bool {
        a.iter()
            .flatten()
            .zip(b.iter().flatten())
            .all(|(x, y)| (x - y).abs() < 1e-9)
    }

    #[test]
    fn rotations_and_orientation() {
        let r = rodrigues([0.1, -0.3, 0.2]);
        assert!(close(&mul(&r, &transpose(&r)), &IDENTITY));
        // Recover r from rotated vectors.
        let pts = [
            [1.0, 0.0, 0.2],
            [0.0, 1.0, -0.5],
            [0.3, 0.4, 1.0],
            [-1.0, 0.2, 0.1],
        ];
        let r2 = absolute_orientation(pts.iter().map(|&a| (a, apply(&r, a), 1.0)));
        assert!(close(&r, &r2), "{r:?} vs {r2:?}");
    }

    #[test]
    fn eigen_and_solve() {
        let (vals, vecs) = eigen_symmetric([[2.0, 1.0, 0.0], [1.0, 2.0, 0.0], [0.0, 0.0, 5.0]]);
        assert!(
            (vals[0] - 1.0).abs() < 1e-12
                && (vals[1] - 3.0).abs() < 1e-12
                && (vals[2] - 5.0).abs() < 1e-12
        );
        assert!((vecs[0][0].abs() - 0.5f64.sqrt()).abs() < 1e-9);
        let x = solve(vec![vec![2.0, 1.0], vec![1.0, 3.0]], vec![3.0, 5.0]).unwrap();
        assert!((x[0] - 0.8).abs() < 1e-12 && (x[1] - 1.4).abs() < 1e-12);
        assert!(solve(vec![vec![1.0, 2.0], vec![2.0, 4.0]], vec![1.0, 2.0]).is_none());
    }
}
