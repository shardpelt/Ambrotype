// SPDX-License-Identifier: GPL-3.0-or-later
//! Aligning panorama frames: a rotation per frame and one focal length,
//! for a camera turned about its lens (Brown & Lowe, 2007).
//!
//! 1. Features in every frame, matched between every pair.
//! 2. Per pair, the rotation that maps one frame's rays onto the other's,
//!    by RANSAC on two matches at a time; a pair counts as overlapping when
//!    enough matches agree (more than 8 + 0.3 × the matches).
//! 3. Starting rotations along the strongest links from the reference frame
//!    (a maximum spanning tree), then all rotations and the focal length
//!    refined together on every agreeing match (bundle adjustment,
//!    Levenberg–Marquardt with a Huber loss).
//! 4. Levelling ("wave correction"): the world is turned so the frames'
//!    horizontal axes lie in the horizontal plane, so a sweep doesn't curve.

use rayon::prelude::*;

use super::features::{self, Feature};
use super::linalg::{self, IDENTITY, M3, V3};
use crate::raw::CameraImage;

/// Long side of the copy features are found on.
const FEATURE_SIDE: usize = 1600;
/// Features per frame.
const FEATURES: usize = 1500;
/// RANSAC iterations per pair (and per focal length tried).
const ITERATIONS: usize = 600;

/// Frames turned into one world.
#[derive(Debug, Clone)]
pub struct Aligned {
    /// Per frame: world → camera rotation (camera axes: x right, y down, z
    /// out of the lens).
    pub rotations: Vec<M3>,
    /// Focal length in full-resolution pixels.
    pub focal: f64,
    /// The frame the others were placed against.
    pub reference: usize,
    /// Overlapping pairs and how many matches agreed.
    pub links: Vec<(usize, usize, usize)>,
    /// Root-mean-square distance between matched points after alignment,
    /// in full-resolution pixels.
    pub rms: f64,
}

/// A frame's principal point (its centre).
fn centre(f: &CameraImage) -> (f64, f64) {
    ((f.width as f64 - 1.0) / 2.0, (f.height as f64 - 1.0) / 2.0)
}

fn ray(frame: &CameraImage, x: f64, y: f64, focal: f64) -> V3 {
    let (cx, cy) = centre(frame);
    linalg::normalize([x - cx, y - cy, focal])
}

/// Deterministic random numbers (xorshift).
struct Rng(u64);

impl Rng {
    fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % n as u64) as usize
    }
}

/// One pair's matches as rays.
struct PairRays {
    a: Vec<V3>,
    b: Vec<V3>,
}

fn pair_rays(
    fa: &CameraImage,
    fb: &CameraImage,
    pa: &[Feature],
    pb: &[Feature],
    matches: &[(usize, usize)],
    focal: f64,
) -> PairRays {
    PairRays {
        a: matches
            .iter()
            .map(|&(i, _)| ray(fa, pa[i].x, pa[i].y, focal))
            .collect(),
        b: matches
            .iter()
            .map(|&(_, j)| ray(fb, pb[j].x, pb[j].y, focal))
            .collect(),
    }
}

/// The rotation taking rays of the first frame to the second's, by RANSAC
/// on pairs of matches, and the indices of the matches that agree (within
/// `tol` radians).
fn rotation_ransac(rays: &PairRays, tol: f64, seed: u64) -> (M3, Vec<usize>) {
    let n = rays.a.len();
    let inliers = |r: &M3| -> Vec<usize> {
        (0..n)
            .filter(|&k| linalg::norm(linalg::sub(linalg::apply(r, rays.a[k]), rays.b[k])) < tol)
            .collect()
    };
    if n < 3 {
        return (IDENTITY, Vec::new());
    }
    let mut rng = Rng(seed | 1);
    let mut best: (M3, Vec<usize>) = (IDENTITY, Vec::new());
    for _ in 0..ITERATIONS {
        let (i, j) = (rng.below(n), rng.below(n));
        let na = linalg::cross(rays.a[i], rays.a[j]);
        let nb = linalg::cross(rays.b[i], rays.b[j]);
        if i == j || linalg::norm(na) < 1e-4 || linalg::norm(nb) < 1e-4 {
            continue;
        }
        let r = linalg::absolute_orientation([
            (rays.a[i], rays.b[i], 1.0),
            (rays.a[j], rays.b[j], 1.0),
            (linalg::normalize(na), linalg::normalize(nb), 1.0),
        ]);
        let found = inliers(&r);
        if found.len() > best.1.len() {
            best = (r, found);
        }
    }
    // Refit on everything that agrees, twice.
    for _ in 0..2 {
        if best.1.len() < 3 {
            break;
        }
        let r = linalg::absolute_orientation(best.1.iter().map(|&k| (rays.a[k], rays.b[k], 1.0)));
        let found = inliers(&r);
        if found.len() >= best.1.len() {
            best = (r, found);
        }
    }
    best
}

/// Whether a pair's agreeing matches show real overlap (Brown & Lowe's test).
fn overlapping(inliers: usize, matches: usize) -> bool {
    inliers >= 10 && inliers as f64 > 8.0 + 0.3 * matches as f64
}

/// Focal length from EXIF: the lens's focal length against the sensor's
/// diagonal (43.27 mm on full frame, smaller by the crop factor).
pub fn exif_focal(frame: &CameraImage) -> Option<f64> {
    let mm = frame.shot.focal_mm.or_else(|| {
        frame
            .metadata
            .focal_length
            .filter(|r| r.1 != 0)
            .map(|r| f64::from(r.0) / f64::from(r.1))
    })?;
    let crop = frame.shot.crop_factor?;
    let diag = (frame.width as f64).hypot(frame.height as f64);
    let f = mm * crop / 43.27 * diag;
    (f.is_finite() && f > 0.0).then_some(f)
}

/// Align `frames` (upright, from one camera and lens). `focal_hint` is a
/// focal length in pixels to start from ([`exif_focal`]).
pub fn align(frames: &[CameraImage], focal_hint: Option<f64>) -> Result<Aligned, crate::Error> {
    let unsupported = |m: String| crate::Error::Unsupported(m);
    let n = frames.len();
    if n < 2 {
        return Err(unsupported("a panorama needs at least two photos".into()));
    }
    let feats: Vec<Vec<Feature>> = frames
        .par_iter()
        .map(|f| features::detect(f, FEATURE_SIDE, FEATURES))
        .collect();
    let pairs: Vec<(usize, usize)> = (0..n)
        .flat_map(|i| (i + 1..n).map(move |j| (i, j)))
        .collect();
    let matches: Vec<Vec<(usize, usize)>> = pairs
        .par_iter()
        .map(|&(i, j)| features::match_features(&feats[i], &feats[j]))
        .collect();
    // Matching accuracy: about two feature pixels, at full resolution.
    let long = frames
        .iter()
        .map(|f| f.width.max(f.height))
        .max()
        .unwrap_or(1);
    let px_tol = 2.0 * long.div_ceil(FEATURE_SIDE) as f64;

    // The focal length: the one at which the best pair agrees most.
    let strongest = (0..pairs.len())
        .max_by_key(|&k| matches[k].len())
        .expect("at least one pair");
    let ref_side = (frames[0].width as f64).hypot(frames[0].height as f64);
    let candidates: Vec<f64> = match focal_hint {
        Some(f) => (-6..=6).map(|k| f * 2f64.powf(k as f64 / 8.0)).collect(),
        None => (-12..=16)
            .map(|k| ref_side * 2f64.powf(k as f64 / 6.0))
            .collect(),
    };
    let (i0, j0) = pairs[strongest];
    let focal = candidates
        .par_iter()
        .map(|&f| {
            let rays = pair_rays(
                &frames[i0],
                &frames[j0],
                &feats[i0],
                &feats[j0],
                &matches[strongest],
                f,
            );
            let (_, inl) = rotation_ransac(&rays, px_tol / f, 7);
            // Ties go to the hint (or, without one, the middle).
            let prior = match focal_hint {
                Some(h) => -(f / h).ln().abs(),
                None => -(f / ref_side).ln().abs(),
            };
            (inl.len(), prior, f)
        })
        .max_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)))
        .map(|(_, _, f)| f)
        .expect("candidates");

    // Rotation per pair at that focal length.
    let pair_results: Vec<(M3, Vec<usize>)> = pairs
        .par_iter()
        .enumerate()
        .map(|(k, &(i, j))| {
            let rays = pair_rays(
                &frames[i],
                &frames[j],
                &feats[i],
                &feats[j],
                &matches[k],
                focal,
            );
            rotation_ransac(&rays, px_tol / focal, 11 + k as u64)
        })
        .collect();
    let links: Vec<(usize, usize, usize)> = pairs
        .iter()
        .enumerate()
        .filter(|(k, _)| overlapping(pair_results[*k].1.len(), matches[*k].len()))
        .map(|(k, &(i, j))| (i, j, pair_results[k].1.len()))
        .collect();

    // Reference: the frame with the most agreeing matches; then a maximum
    // spanning tree from it.
    let mut weight = vec![0usize; n];
    for &(i, j, c) in &links {
        weight[i] += c;
        weight[j] += c;
    }
    let reference = (0..n)
        .max_by_key(|&k| (weight[k], usize::MAX - k))
        .unwrap_or(0);
    let mut rotations: Vec<Option<M3>> = vec![None; n];
    rotations[reference] = Some(IDENTITY);
    loop {
        let next = links
            .iter()
            .filter(|&&(i, j, _)| rotations[i].is_some() != rotations[j].is_some())
            .max_by_key(|&&(_, _, c)| c);
        let Some(&(i, j, _)) = next else { break };
        let k = pairs
            .iter()
            .position(|&p| p == (i, j))
            .expect("a link is a pair");
        let rij = pair_results[k].0;
        if let Some(ri) = rotations[i] {
            rotations[j] = Some(linalg::mul(&rij, &ri));
        } else if let Some(rj) = rotations[j] {
            rotations[i] = Some(linalg::mul(&linalg::transpose(&rij), &rj));
        }
    }
    let unplaced: Vec<usize> = (0..n).filter(|&k| rotations[k].is_none()).collect();
    if !unplaced.is_empty() {
        let list: Vec<String> = unplaced.iter().map(|k| (k + 1).to_string()).collect();
        return Err(unsupported(format!(
            "photo {} doesn't overlap the others enough to be placed (panoramas need about a third of each frame shared with a neighbour)",
            list.join(", ")
        )));
    }
    let start: Vec<M3> = rotations.into_iter().map(|r| r.expect("placed")).collect();

    // Bundle adjustment on every agreeing match.
    let mut obs: Vec<Observation> = Vec::new();
    for (k, &(i, j)) in pairs.iter().enumerate() {
        if !overlapping(pair_results[k].1.len(), matches[k].len()) {
            continue;
        }
        for &m in &pair_results[k].1 {
            let (a, b) = matches[k][m];
            obs.push(Observation {
                i,
                j,
                a: (feats[i][a].x, feats[i][a].y),
                b: (feats[j][b].x, feats[j][b].y),
            });
        }
    }
    let problem = Bundle {
        frames,
        obs: &obs,
        start: &start,
        reference,
        huber: px_tol,
        focal_hint,
    };
    let (rotations, focal) = problem.solve(focal);
    let rms = problem.rms(&rotations, focal);
    let rotations = level(&rotations);
    Ok(Aligned {
        rotations,
        focal,
        reference,
        links,
        rms,
    })
}

/// A match between feature `a` of frame `i` and `b` of frame `j`.
struct Observation {
    i: usize,
    j: usize,
    a: (f64, f64),
    b: (f64, f64),
}

struct Bundle<'a> {
    frames: &'a [CameraImage],
    obs: &'a [Observation],
    start: &'a [M3],
    reference: usize,
    huber: f64,
    focal_hint: Option<f64>,
}

/// How far (in ln f) the EXIF focal length is trusted: lens breathing,
/// rounded crop factors and a lens correction's zoom put it off by up to
/// about this much. Only frames that leave the focal length loose (long
/// lenses, where frames barely bend) feel it.
const FOCAL_PRIOR_SIGMA: f64 = 0.1;

impl Bundle<'_> {
    /// Parameters: a rotation vector per frame but the reference, then ln f.
    fn rotations(&self, p: &[f64]) -> Vec<M3> {
        let mut k = 0;
        (0..self.start.len())
            .map(|i| {
                if i == self.reference {
                    self.start[i]
                } else {
                    let w = [p[k], p[k + 1], p[k + 2]];
                    k += 3;
                    linalg::mul(&linalg::rodrigues(w), &self.start[i])
                }
            })
            .collect()
    }

    /// Where feature `a` of frame `i` lands in frame `j`, minus where `b` is.
    fn error(&self, o: &Observation, r: &[M3], focal: f64) -> (f64, f64) {
        let world = linalg::apply(
            &linalg::transpose(&r[o.i]),
            ray(&self.frames[o.i], o.a.0, o.a.1, focal),
        );
        let c = linalg::apply(&r[o.j], world);
        if c[2] <= 1e-6 {
            return (1e3, 1e3);
        }
        let (cx, cy) = centre(&self.frames[o.j]);
        (
            focal * c[0] / c[2] + cx - o.b.0,
            focal * c[1] / c[2] + cy - o.b.1,
        )
    }

    /// Robust residuals (Huber: quadratic within `huber` pixels, linear past).
    fn residuals(&self, p: &[f64]) -> Vec<f64> {
        let r = self.rotations(p);
        let focal = p[p.len() - 1].exp();
        let mut out = Vec::with_capacity(self.obs.len() * 2);
        for o in self.obs {
            let (ex, ey) = self.error(o, &r, focal);
            let len = ex.hypot(ey);
            let w = if len > self.huber {
                (self.huber / len).sqrt()
            } else {
                1.0
            };
            out.push(ex * w);
            out.push(ey * w);
        }
        if let Some(h) = self.focal_hint {
            out.push((focal / h).ln() / FOCAL_PRIOR_SIGMA);
        }
        out
    }

    fn rms(&self, r: &[M3], focal: f64) -> f64 {
        if self.obs.is_empty() {
            return 0.0;
        }
        let sum: f64 = self
            .obs
            .iter()
            .map(|o| {
                let (ex, ey) = self.error(o, r, focal);
                ex * ex + ey * ey
            })
            .sum();
        (sum / self.obs.len() as f64).sqrt()
    }

    /// Levenberg–Marquardt from the start rotations and `focal`.
    fn solve(&self, focal: f64) -> (Vec<M3>, f64) {
        let m = 3 * (self.start.len() - 1) + 1;
        let mut p = vec![0.0; m];
        p[m - 1] = focal.ln();
        if self.obs.len() < m {
            return (self.rotations(&p), focal);
        }
        let cost = |r: &[f64]| r.iter().map(|v| v * v).sum::<f64>();
        let mut res = self.residuals(&p);
        let mut c = cost(&res);
        let mut lambda = 1e-3;
        for _ in 0..50 {
            // Numerical Jacobian, column by column.
            let h = 1e-6;
            let columns: Vec<Vec<f64>> = (0..m)
                .into_par_iter()
                .map(|k| {
                    let (mut up, mut down) = (p.clone(), p.clone());
                    up[k] += h;
                    down[k] -= h;
                    let (ru, rd) = (self.residuals(&up), self.residuals(&down));
                    ru.iter()
                        .zip(&rd)
                        .map(|(a, b)| (a - b) / (2.0 * h))
                        .collect()
                })
                .collect();
            let jtj: Vec<Vec<f64>> = (0..m)
                .map(|a| {
                    (0..m)
                        .map(|b| columns[a].iter().zip(&columns[b]).map(|(x, y)| x * y).sum())
                        .collect()
                })
                .collect();
            let jtr: Vec<f64> = (0..m)
                .map(|a| columns[a].iter().zip(&res).map(|(x, y)| x * y).sum())
                .collect();
            let mut improved = false;
            for _ in 0..10 {
                let mut a = jtj.clone();
                for (k, row) in a.iter_mut().enumerate() {
                    row[k] += lambda * jtj[k][k].max(1e-12);
                }
                let Some(step) = linalg::solve(a, jtr.iter().map(|v| -v).collect()) else {
                    lambda *= 10.0;
                    continue;
                };
                let trial: Vec<f64> = p.iter().zip(&step).map(|(a, b)| a + b).collect();
                let r2 = self.residuals(&trial);
                let c2 = cost(&r2);
                if c2 < c {
                    let gain = (c - c2) / c.max(1e-30);
                    (p, res, c) = (trial, r2, c2);
                    lambda = (lambda / 3.0).max(1e-12);
                    improved = gain > 1e-10;
                    break;
                }
                lambda *= 4.0;
            }
            if !improved {
                break;
            }
        }
        (self.rotations(&p), p[m - 1].exp())
    }
}

/// Turn the world so the frames' horizontal axes are level and their
/// average view points along +z (Brown & Lowe's wave correction).
fn level(rotations: &[M3]) -> Vec<M3> {
    let axis = |r: &M3, k: usize| -> V3 { [r[k][0], r[k][1], r[k][2]] };
    let mut cov = [[0.0; 3]; 3];
    let (mut up, mut forward) = ([0.0; 3], [0.0; 3]);
    for r in rotations {
        // Rows of a world → camera rotation are the camera axes in the world.
        let x = axis(r, 0);
        for i in 0..3 {
            for j in 0..3 {
                cov[i][j] += x[i] * x[j];
            }
        }
        let (y, z) = (axis(r, 1), axis(r, 2));
        for i in 0..3 {
            up[i] -= y[i];
            forward[i] += z[i];
        }
    }
    let (values, vectors) = linalg::eigen_symmetric(cov);
    let mut u: V3 = std::array::from_fn(|i| vectors[i][0]);
    if values[1] < 0.02 * values[2] {
        // All frames share one horizontal axis (a vertical sweep): up is
        // the average up, square to it.
        let x = linalg::normalize(std::array::from_fn(|i| vectors[i][2]));
        u = linalg::normalize(linalg::sub(up, x.map(|v| v * linalg::dot(up, x))));
    }
    if linalg::dot(u, up) < 0.0 {
        u = u.map(|v| -v);
    }
    // New world axes: y down (−u), z the average view square to it, x = y × z.
    let down = u.map(|v| -v);
    let mut z = linalg::sub(forward, down.map(|v| v * linalg::dot(forward, down)));
    if linalg::norm(z) < 1e-9 {
        z = [0.0, 0.0, 1.0];
    }
    let z = linalg::normalize(z);
    let x = linalg::normalize(linalg::cross(down, z));
    let g: M3 = [x, down, z];
    let gt = linalg::transpose(&g);
    rotations.iter().map(|r| linalg::mul(r, &gt)).collect()
}
