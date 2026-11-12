// SPDX-License-Identifier: GPL-3.0-or-later
//! AI Denoise on the raw: RawNIND's UtNet2 networks (Benoit Brummer,
//! UCLouvain; weights GPL-3.0 or CC BY 4.0, trained on the RawNIND
//! dataset), as darktable-ai converts them to ONNX.
//!
//! - **Bayer** (`model_bayer`): the sensor's samples, black and white level
//!   scaled to 0–1 and not white balanced, packed as R, G, G, B planes at
//!   half size; out comes camera RGB, denoised and demosaiced, at full size.
//!   It replaces our demosaic ([`amb_engine::raw_denoise::set_raw_denoiser`]).
//! - **Linear** (`model_linear`): already demosaiced linear Rec.2020, white
//!   balanced; out comes the same, denoised. For X-Trans and other sensors.
//!
//! Both take 512 × 512 tiles. Their output's scale is arbitrary (they were
//! trained to match brightness, not keep it), so each tile is scaled back to
//! its input's mean, colour by colour. Tiles overlap by 64 and are blended
//! with a feather, so no seams show.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use tract_onnx::prelude::*;

/// The tile side the networks take.
pub const TILE: usize = 512;
/// How much neighbouring tiles share, blended.
const OVERLAP: usize = 64;

/// A loaded network.
#[derive(Clone)]
pub struct Model {
    net: Arc<crate::net::Net>,
    channels: usize,
}

/// Load a RawNIND network taking `channels` (4: Bayer, 3: linear) planes.
pub fn load(path: &Path, channels: usize) -> Result<Model, String> {
    tract_linalg::multithread::set_default_executor(
        tract_linalg::multithread::Executor::RayonGlobal,
    );
    tract_onnx::onnx()
        .model_for_path(path)
        .and_then(|m| m.with_input_fact(0, f32::fact([1, channels, TILE, TILE]).into()))
        .and_then(crate::net::build)
        .map(|net| Model { net, channels })
        .map_err(|e| format!("{} didn't load: {e}", path.display()))
}

impl Model {
    /// One tile: `planes` (`channels` × 512 × 512) in, the network's planes
    /// out (3 × side × side) and that side.
    fn tile(&self, planes: Vec<f32>) -> Result<(Vec<f32>, usize), String> {
        let input = tract_ndarray::Array4::from_shape_vec((1, self.channels, TILE, TILE), planes)
            .map_err(|e| e.to_string())?;
        let out = self
            .net
            .run(tvec!(Tensor::from(input).into()))
            .map_err(|e| format!("AI Denoise: {e}"))?;
        let side = out[0].shape().last().copied().unwrap_or(0);
        let view = out[0]
            .to_plain_array_view::<f32>()
            .map_err(|e| e.to_string())?;
        Ok((view.iter().copied().collect(), side))
    }
}

/// Where tiles of `TILE` start along `len`, overlapping by `OVERLAP`; the
/// last ends at `len` (or starts at 0 when `len` is shorter than a tile).
fn starts(len: usize) -> Vec<usize> {
    if len <= TILE {
        return vec![0];
    }
    let step = TILE - OVERLAP;
    let mut out: Vec<usize> = (0..)
        .map(|i| i * step)
        .take_while(|s| s + TILE < len)
        .collect();
    out.push(len - TILE);
    out
}

/// A weight that rises over the overlap at a tile's inner edges, so tiles
/// blend; 1 at the photo's own edges. `at` is where the tile starts along
/// `len`, `i` the position in it, `scale` output pixels per input pixel.
fn feather(i: usize, at: usize, len: usize, scale: usize) -> f32 {
    let ramp = (OVERLAP * scale) as f32;
    let side = TILE * scale;
    let mut w = 1.0f32;
    if at > 0 {
        w = w.min((i as f32 + 0.5) / ramp);
    }
    if at + TILE < len {
        w = w.min((side - i) as f32 / ramp);
    }
    w.clamp(1e-3, 1.0)
}

/// Read `planes` (`c` planes of `w` × `h`) into a tile at (x0, y0),
/// mirrored past the edges.
fn cut(planes: &[f32], c: usize, w: usize, h: usize, x0: usize, y0: usize) -> Vec<f32> {
    let mirror = |v: usize, n: usize| -> usize {
        if n == 1 {
            return 0;
        }
        let p = 2 * (n - 1);
        let m = v % p;
        if m < n { m } else { p - m }
    };
    let mut out = vec![0.0f32; c * TILE * TILE];
    for k in 0..c {
        for y in 0..TILE {
            let sy = mirror(y0 + y, h);
            for x in 0..TILE {
                out[(k * TILE + y) * TILE + x] = planes[(k * h + sy) * w + mirror(x0 + x, w)];
            }
        }
    }
    out
}

/// Run `model` over `planes` (`c` × `w` × `h`) tile by tile. `means(tile)`
/// gives each output colour's mean in the input tile, which the output is
/// scaled back to. Returns 3 planes at `w·s` × `h·s` (s: the network's
/// scale), and `progress(done, total)` as tiles finish.
fn run_tiles(
    model: &Model,
    planes: &[f32],
    (c, w, h): (usize, usize, usize),
    means: &dyn Fn(&[f32]) -> [f32; 3],
    progress: &dyn Fn(usize, usize),
) -> Result<(Vec<f32>, usize), String> {
    let (xs, ys) = (starts(w), starts(h));
    let total = xs.len() * ys.len();
    let mut out: Vec<f32> = Vec::new();
    let mut weight: Vec<f32> = Vec::new();
    let mut scale = 0;
    let mut done = 0;
    for &y0 in &ys {
        for &x0 in &xs {
            let tile = cut(planes, c, w, h, x0, y0);
            let want = means(&tile);
            let (mut got, side) = model.tile(tile)?;
            let s = side / TILE;
            if s == 0 || got.len() != 3 * side * side {
                return Err("AI Denoise: the network gave an unexpected size".into());
            }
            if scale == 0 {
                scale = s;
                out = vec![0.0; 3 * w * s * h * s];
                weight = vec![0.0; w * s * h * s];
            }
            // Back to the input's brightness, colour by colour.
            for k in 0..3 {
                let plane = &mut got[k * side * side..(k + 1) * side * side];
                let mean = plane.iter().map(|v| f64::from(*v)).sum::<f64>() / plane.len() as f64;
                let g = if mean.abs() > 1e-12 {
                    (f64::from(want[k]) / mean) as f32
                } else {
                    0.0
                };
                plane.iter_mut().for_each(|v| *v *= g);
            }
            let (ow, oh) = (w * s, h * s);
            for ty in 0..side {
                let oy = y0 * s + ty;
                if oy >= oh {
                    break;
                }
                let wy = feather(ty, y0, h, s);
                for tx in 0..side {
                    let ox = x0 * s + tx;
                    if ox >= ow {
                        break;
                    }
                    let wt = wy * feather(tx, x0, w, s);
                    let o = oy * ow + ox;
                    for k in 0..3 {
                        out[k * ow * oh + o] += got[(k * side + ty) * side + tx] * wt;
                    }
                    weight[o] += wt;
                }
            }
            done += 1;
            progress(done, total);
        }
    }
    let n = weight.len();
    for (o, wt) in weight.iter().enumerate() {
        for k in 0..3 {
            out[k * n + o] /= wt.max(1e-6);
        }
    }
    Ok((out, scale))
}

/// Denoise and demosaic a Bayer mosaic: `samples` (`w` × `h`, black and
/// white level scaled to 0–1), `pattern[r][c]` the colour (0 R, 1 G, 2 B)
/// at row `r % 2`, column `c % 2`. Returns interleaved camera RGB, `w` × `h`.
pub fn bayer(
    model: &Model,
    samples: &[f32],
    (w, h): (usize, usize),
    pattern: [[u8; 2]; 2],
    progress: &dyn Fn(usize, usize),
) -> Result<Vec<f32>, String> {
    // Start where the pattern reads R, G / G, B.
    let (oy, ox) = (0..2)
        .flat_map(|y| (0..2).map(move |x| (y, x)))
        .find(|&(y, x)| pattern[y][x] == 0 && pattern[y ^ 1][x ^ 1] == 2)
        .ok_or("AI Denoise works on Bayer sensors (an R, G, G, B pattern)")?;
    let (pw, ph) = ((w - ox) / 2, (h - oy) / 2);
    if pw < 8 || ph < 8 {
        return Err("the photo is too small".into());
    }
    let mut planes = vec![0.0f32; 4 * pw * ph];
    for y in 0..ph {
        for x in 0..pw {
            let at = |dy: usize, dx: usize| {
                samples[(oy + 2 * y + dy) * w + ox + 2 * x + dx].clamp(0.0, 1.0)
            };
            let i = y * pw + x;
            planes[i] = at(0, 0);
            planes[pw * ph + i] = at(0, 1);
            planes[2 * pw * ph + i] = at(1, 0);
            planes[3 * pw * ph + i] = at(1, 1);
        }
    }
    let means = |tile: &[f32]| {
        let n = (TILE * TILE) as f32;
        let m: Vec<f32> = (0..4)
            .map(|k| {
                tile[k * TILE * TILE..(k + 1) * TILE * TILE]
                    .iter()
                    .sum::<f32>()
                    / n
            })
            .collect();
        [m[0], (m[1] + m[2]) / 2.0, m[3]]
    };
    let (out, s) = run_tiles(model, &planes, (4, pw, ph), &means, progress)?;
    if s != 2 {
        return Err("AI Denoise: this isn't the Bayer network".into());
    }
    // Into the photo's own frame; a row or column the pattern's offset or an
    // odd size left out copies its neighbour.
    let (ow, oh) = (2 * pw, 2 * ph);
    let mut rgb = vec![0.0f32; w * h * 3];
    for y in 0..h {
        let sy = y.saturating_sub(oy).min(oh - 1);
        for x in 0..w {
            let sx = x.saturating_sub(ox).min(ow - 1);
            for k in 0..3 {
                rgb[(y * w + x) * 3 + k] = out[k * ow * oh + sy * ow + sx].max(0.0);
            }
        }
    }
    Ok(rgb)
}

/// Denoise interleaved linear Rec.2020 (white balanced), `w` × `h`, in place.
pub fn linear(
    model: &Model,
    rgb: &mut [f32],
    (w, h): (usize, usize),
    progress: &dyn Fn(usize, usize),
) -> Result<(), String> {
    let mut planes = vec![0.0f32; 3 * w * h];
    for (i, c) in rgb.chunks_exact(3).enumerate() {
        for k in 0..3 {
            planes[k * w * h + i] = c[k].max(0.0);
        }
    }
    let means = |tile: &[f32]| {
        let n = (TILE * TILE) as f32;
        [0, 1, 2].map(|k| {
            tile[k * TILE * TILE..(k + 1) * TILE * TILE]
                .iter()
                .sum::<f32>()
                / n
        })
    };
    let (out, s) = run_tiles(model, &planes, (3, w, h), &means, progress)?;
    if s != 1 {
        return Err("AI Denoise: this isn't the linear network".into());
    }
    for (i, c) in rgb.chunks_exact_mut(3).enumerate() {
        for k in 0..3 {
            c[k] = out[k * w * h + i].max(0.0);
        }
    }
    Ok(())
}

/// A model's id, and its Bayer and linear networks.
type Loaded = (String, Option<Model>, Option<Model>);

/// The installed denoise model's two networks, loaded once.
pub struct Denoiser {
    dir: PathBuf,
    loaded: Mutex<Option<Loaded>>,
}

impl Denoiser {
    pub fn new(dir: PathBuf) -> Denoiser {
        Denoiser {
            dir,
            loaded: Mutex::new(None),
        }
    }

    /// The chosen denoise model's (bayer, linear) networks, installing
    /// nothing: `Err` says to install it.
    fn models(&self) -> Result<(Option<Model>, Option<Model>), String> {
        let spec = crate::chosen(&self.dir, "denoise")?;
        if !crate::is_installed(&self.dir, &spec) {
            return Err(format!(
                "AI Denoise needs its model: install {} in Settings → AI models (or amb models install {})",
                spec.name, spec.id
            ));
        }
        let mut l = self.loaded.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((id, b, lin)) = l.as_ref()
            && *id == spec.id
        {
            return Ok((b.clone(), lin.clone()));
        }
        let bayer = load(&crate::file_of(&self.dir, &spec.id), 4)?;
        let linear = spec
            .files
            .iter()
            .find(|f| f.name == "linear.onnx")
            .map(|f| load(&self.dir.join(format!("{}.{}", spec.id, f.name)), 3))
            .transpose()?;
        *l = Some((spec.id.clone(), Some(bayer.clone()), linear.clone()));
        Ok((Some(bayer), linear))
    }

    /// The engine's raw denoiser ([`amb_engine::raw_denoise::RawDenoiser`]).
    pub fn engine(self: Arc<Self>) -> Arc<amb_engine::raw_denoise::RawDenoiser> {
        Arc::new(
            move |input: amb_engine::raw_denoise::DenoiseInput<'_>,
                  progress: &dyn Fn(usize, usize)| {
                let (b, lin) = self.models()?;
                match input {
                    amb_engine::raw_denoise::DenoiseInput::Bayer {
                        samples,
                        size,
                        pattern,
                    } => {
                        let m = b.ok_or("the denoise model has no Bayer network")?;
                        bayer(&m, samples, size, pattern, progress)
                    }
                    amb_engine::raw_denoise::DenoiseInput::Linear { rgb, size } => {
                        let m = lin.ok_or(
                            "the denoise model has no network for this sensor (only Bayer)",
                        )?;
                        let mut rgb = rgb.to_vec();
                        linear(&m, &mut rgb, size, progress)?;
                        Ok(rgb)
                    }
                }
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiles_cover_the_length_and_overlap() {
        assert_eq!(starts(300), vec![0]);
        assert_eq!(starts(512), vec![0]);
        let s = starts(1300);
        assert_eq!((s[0], *s.last().unwrap()), (0, 1300 - TILE));
        for p in s.windows(2) {
            assert!(p[1] - p[0] <= TILE - OVERLAP && p[1] > p[0], "{s:?}");
        }
    }

    #[test]
    fn the_feather_is_full_inside_and_rises_at_inner_edges() {
        assert_eq!(feather(0, 0, 2000, 1), 1.0);
        assert!(feather(0, 448, 2000, 1) < 0.02);
        assert_eq!(feather(256, 448, 2000, 1), 1.0);
        assert!(feather(TILE * 2 - 1, 0, 2000, 2) < 0.02);
    }

    #[test]
    fn a_tile_past_the_edge_is_mirrored() {
        let (w, h) = (5, 3);
        let planes: Vec<f32> = (0..w * h).map(|i| i as f32).collect();
        let t = cut(&planes, 1, w, h, 0, 0);
        assert_eq!(&t[..7], &[0.0, 1.0, 2.0, 3.0, 4.0, 3.0, 2.0]);
        assert_eq!(t[3 * TILE], 5.0, "row 3 mirrors row 1");
    }

    /// Run with AMB_DENOISE_MODEL=model_bayer.onnx: a flat grey mosaic
    /// with noise comes out smoother and as bright.
    #[test]
    fn the_bayer_network_smooths_and_keeps_brightness() {
        let Ok(path) = std::env::var("AMB_DENOISE_MODEL") else {
            return;
        };
        let model = load(Path::new(&path), 4).unwrap();
        let (w, h) = (600, 520);
        let mut seed = 1u64;
        let mut noise = || {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((seed >> 33) as f32 / (1u64 << 31) as f32 - 0.5) * 0.1
        };
        let samples: Vec<f32> = (0..w * h).map(|_| 0.3 + noise()).collect();
        let rgb = bayer(&model, &samples, (w, h), [[0, 1], [1, 2]], &|_, _| {}).unwrap();
        let g: Vec<f32> = rgb.chunks_exact(3).map(|c| c[1]).collect();
        let mean = g.iter().sum::<f32>() / g.len() as f32;
        let var = g.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / g.len() as f32;
        eprintln!("mean {mean}, sd {}", var.sqrt());
        assert!((mean - 0.3).abs() < 0.02, "{mean}");
        assert!(
            var.sqrt() < 0.1 / 12f32.sqrt() / 2.0,
            "smoother: sd {}",
            var.sqrt()
        );
    }
}
