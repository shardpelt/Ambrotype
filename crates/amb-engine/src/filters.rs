//! Shared single-channel image filters: box blur, Gaussian blur and the guided
//! filter. All take a row-major plane of `w * h` values and clamp at the edges.
//! Cost is independent of the radius (running sums), so large radii (Clarity,
//! Dehaze) stay cheap on 50+ MP images.

use rayon::prelude::*;

/// Transpose a `w × h` plane into `h × w`.
pub(crate) fn transpose(src: &[f32], w: usize, h: usize) -> Vec<f32> {
    let mut out = vec![0.0f32; src.len()];
    const B: usize = 32;
    out.par_chunks_mut(h * B)
        .enumerate()
        .for_each(|(bx, chunk)| {
            // chunk holds output rows x in [bx*B, bx*B+B), each of length h.
            let x0 = bx * B;
            let rows = chunk.len() / h;
            for y in 0..h {
                for dx in 0..rows {
                    chunk[dx * h + y] = src[y * w + x0 + dx];
                }
            }
        });
    out
}

/// Box blur of one row with radius `r` (window 2r+1), clamped edges, running sum.
fn box_row(src: &[f32], dst: &mut [f32], r: usize) {
    let n = src.len();
    if n == 0 {
        return;
    }
    if r == 0 {
        dst.copy_from_slice(src);
        return;
    }
    let at = |i: isize| src[i.clamp(0, n as isize - 1) as usize];
    let win = (2 * r + 1) as f32;
    let mut sum: f32 = (-(r as isize)..=r as isize).map(at).sum();
    for (i, d) in dst.iter_mut().enumerate() {
        *d = sum / win;
        let i = i as isize;
        sum += at(i + r as isize + 1) - at(i - r as isize);
    }
}

fn box_rows(src: &[f32], w: usize, r: usize) -> Vec<f32> {
    let mut out = vec![0.0f32; src.len()];
    out.par_chunks_mut(w)
        .zip(src.par_chunks(w))
        .for_each(|(d, s)| box_row(s, d, r));
    out
}

/// Separable box blur with radius `r` (window (2r+1)²).
pub fn box_blur(plane: &[f32], w: usize, h: usize, r: usize) -> Vec<f32> {
    assert_eq!(plane.len(), w * h);
    if r == 0 || plane.is_empty() {
        return plane.to_vec();
    }
    let horiz = box_rows(plane, w, r);
    let t = transpose(&horiz, w, h);
    let vert = box_rows(&t, h, r);
    transpose(&vert, h, w)
}

/// Gaussian blur with standard deviation `sigma` (pixels). Small sigmas use an
/// exact separable kernel; larger ones three box passes (error < 3%).
pub fn gaussian_blur(plane: &[f32], w: usize, h: usize, sigma: f32) -> Vec<f32> {
    assert_eq!(plane.len(), w * h);
    if sigma < 0.3 || plane.is_empty() {
        return plane.to_vec();
    }
    if sigma <= 3.0 {
        let r = (3.0 * sigma).ceil() as isize;
        let mut k: Vec<f32> = (-r..=r)
            .map(|i| (-(i * i) as f32 / (2.0 * sigma * sigma)).exp())
            .collect();
        let s: f32 = k.iter().sum();
        k.iter_mut().for_each(|v| *v /= s);
        let conv_rows = |src: &[f32], width: usize| -> Vec<f32> {
            let mut out = vec![0.0f32; src.len()];
            out.par_chunks_mut(width)
                .zip(src.par_chunks(width))
                .for_each(|(d, s)| {
                    let n = s.len() as isize;
                    for (i, dv) in d.iter_mut().enumerate() {
                        let mut acc = 0.0;
                        for (j, kv) in k.iter().enumerate() {
                            let idx = (i as isize + j as isize - r).clamp(0, n - 1) as usize;
                            acc += kv * s[idx];
                        }
                        *dv = acc;
                    }
                });
            out
        };
        let horiz = conv_rows(plane, w);
        let t = transpose(&horiz, w, h);
        let vert = conv_rows(&t, h);
        return transpose(&vert, h, w);
    }
    // Three box passes approximating a Gaussian (Kovesi's box sizes).
    let n = 3.0f32;
    let w_ideal = (12.0 * sigma * sigma / n + 1.0).sqrt();
    let mut wl = w_ideal.floor() as i32;
    if wl % 2 == 0 {
        wl -= 1;
    }
    let wu = wl + 2;
    let m_ideal = (12.0 * sigma * sigma - n * (wl * wl) as f32 - 4.0 * n * wl as f32 - 3.0 * n)
        / (-4.0 * wl as f32 - 4.0);
    let m = m_ideal.round() as i32;
    let mut out = plane.to_vec();
    for i in 0..3 {
        let size = if i < m { wl } else { wu };
        out = box_blur(&out, w, h, ((size - 1) / 2).max(0) as usize);
    }
    out
}

/// Guided filter (He, Sun & Tang, 2010): edge-preserving smoothing of `src`
/// steered by `guide`. `radius` is the box radius in pixels; `eps` the
/// regularisation (larger = smoother, in units of the signal squared).
pub fn guided_filter(
    guide: &[f32],
    src: &[f32],
    w: usize,
    h: usize,
    radius: usize,
    eps: f32,
) -> Vec<f32> {
    assert_eq!(guide.len(), w * h);
    assert_eq!(src.len(), w * h);
    let mean = |p: &[f32]| box_blur(p, w, h, radius);
    let mean_i = mean(guide);
    let mean_p = mean(src);
    let ii: Vec<f32> = guide.par_iter().map(|v| v * v).collect();
    let ip: Vec<f32> = guide
        .par_iter()
        .zip(src.par_iter())
        .map(|(a, b)| a * b)
        .collect();
    let corr_i = mean(&ii);
    let corr_ip = mean(&ip);
    let (a, b): (Vec<f32>, Vec<f32>) = (0..w * h)
        .into_par_iter()
        .map(|k| {
            let var_i = corr_i[k] - mean_i[k] * mean_i[k];
            let cov_ip = corr_ip[k] - mean_i[k] * mean_p[k];
            let a = cov_ip / (var_i + eps);
            (a, mean_p[k] - a * mean_i[k])
        })
        .unzip();
    let mean_a = mean(&a);
    let mean_b = mean(&b);
    (0..w * h)
        .into_par_iter()
        .map(|k| mean_a[k] * guide[k] + mean_b[k])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ramp(w: usize, h: usize) -> Vec<f32> {
        (0..w * h)
            .map(|i| ((i % w) as f32 * 0.37 + (i / w) as f32 * 0.11).sin())
            .collect()
    }

    #[test]
    fn transpose_round_trips() {
        let (w, h) = (37, 53);
        let p = ramp(w, h);
        assert_eq!(transpose(&transpose(&p, w, h), h, w), p);
    }

    #[test]
    fn blurs_preserve_constants() {
        let (w, h) = (40, 30);
        let c = vec![0.42f32; w * h];
        for out in [
            box_blur(&c, w, h, 5),
            gaussian_blur(&c, w, h, 1.5),
            gaussian_blur(&c, w, h, 12.0),
        ] {
            assert!(out.iter().all(|v| (v - 0.42).abs() < 1e-5));
        }
    }

    #[test]
    fn box_blur_matches_brute_force() {
        let (w, h, r) = (23, 17, 3usize);
        let p = ramp(w, h);
        let fast = box_blur(&p, w, h, r);
        for y in 0..h {
            for x in 0..w {
                let mut acc = 0.0;
                for dy in -(r as isize)..=r as isize {
                    for dx in -(r as isize)..=r as isize {
                        let xx = (x as isize + dx).clamp(0, w as isize - 1) as usize;
                        let yy = (y as isize + dy).clamp(0, h as isize - 1) as usize;
                        acc += p[yy * w + xx];
                    }
                }
                let brute = acc / ((2 * r + 1) * (2 * r + 1)) as f32;
                assert!((fast[y * w + x] - brute).abs() < 1e-4, "({x},{y})");
            }
        }
    }

    #[test]
    fn gaussian_spreads_an_impulse_with_the_right_width() {
        let (w, h) = (201, 201);
        for sigma in [2.0f32, 8.0] {
            let mut p = vec![0.0f32; w * h];
            p[100 * w + 100] = 1.0;
            let out = gaussian_blur(&p, w, h, sigma);
            let total: f32 = out.iter().sum();
            assert!((total - 1.0).abs() < 1e-3, "mass {total}");
            // Variance along x of the centre row's marginal.
            let mut var = 0.0;
            for y in 0..h {
                for x in 0..w {
                    var += out[y * w + x] * ((x as f32 - 100.0).powi(2));
                }
            }
            let measured = var.sqrt();
            assert!(
                (measured - sigma).abs() / sigma < 0.05,
                "sigma {sigma} measured {measured}"
            );
        }
    }

    #[test]
    fn guided_filter_preserves_edges_and_smooths_flats() {
        let (w, h) = (64, 32);
        // Step edge with mild noise on each side.
        let src: Vec<f32> = (0..w * h)
            .map(|i| {
                let x = i % w;
                let base = if x < w / 2 { 0.2 } else { 0.8 };
                base + 0.02 * ((i * 7919) % 13) as f32 / 13.0
            })
            .collect();
        let out = guided_filter(&src, &src, w, h, 4, 1e-3);
        // Edge stays sharp: values two pixels either side of the edge remain far apart.
        let row = 16 * w;
        assert!(out[row + w / 2 + 2] - out[row + w / 2 - 3] > 0.5);
        // Flat regions are smoothed (noise reduced).
        let flat: Vec<f32> = (8..24).map(|x| out[row + x]).collect();
        let spread = flat.iter().cloned().fold(f32::MIN, f32::max)
            - flat.iter().cloned().fold(f32::MAX, f32::min);
        assert!(spread < 0.012, "spread {spread}");
    }
}
