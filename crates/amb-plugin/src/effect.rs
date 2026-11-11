// SPDX-License-Identifier: GPL-3.0-or-later
//! Effect plugins: looks rendered with the photo (docs/ai/plugins.md).
//!
//! An effect exports `amb_effect(info_ptr, info_len, px_ptr, n) -> i64`: it
//! changes `n` floats (RGB, sRGB-encoded 0–1) at `px_ptr` in place, given
//! the [`amb_plugin_sdk::Effect`] JSON at `info_ptr` (with `width` and
//! `height`), and returns 0 or an error message packed as `(ptr << 32) | len`.
//!
//! - A **colour** effect is asked once for a 33³ grid of colours; the result
//!   becomes a colour table the engine applies like a `.cube` LUT, so it
//!   costs nothing per render.
//! - A **pixel** effect gets the image, in strips of rows that overlap by
//!   the manifest's `margin`, each in a fresh instance (so the memory cap
//!   holds for any photo size).

use crate::{FUEL, Plugin, instantiate, pass, trap};
use serde_json::{Map, Value};

/// Points per side of a colour effect's table.
pub const GRID: usize = 33;
/// Instructions per colour of the table, and per pixel.
const FUEL_PER_POINT: u64 = 20_000;
const FUEL_PER_PIXEL: u64 = 2_000;
/// Floats per strip (16 MB, well under the memory cap).
const STRIP_FLOATS: usize = 4 << 20;

/// Where the pixels sit in the whole photo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Region {
    pub x: usize,
    pub y: usize,
    pub full_width: usize,
    pub full_height: usize,
}

impl Plugin {
    fn effect_info(&self, params: &Map<String, Value>, seed: u64) -> Result<Value, String> {
        if !self.manifest.is_effect() {
            return Err(format!("{} isn't an effect", self.manifest.name));
        }
        Ok(serde_json::json!({
            "api": crate::API,
            "params": self.manifest.resolve_params(params)?,
            "seed": seed,
        }))
    }

    /// A colour effect as a table: `GRID`³ sRGB colours, red changing
    /// fastest, then green, then blue.
    pub fn color_table(
        &self,
        params: &Map<String, Value>,
        seed: u64,
    ) -> Result<Vec<[f32; 3]>, String> {
        if self.manifest.effect != "color" {
            return Err(format!("{} isn't a colour effect", self.manifest.name));
        }
        let n = GRID * GRID * GRID;
        let step = 1.0 / (GRID - 1) as f32;
        let mut data: Vec<f32> = (0..n)
            .flat_map(|i| {
                let (r, g, b) = (i % GRID, (i / GRID) % GRID, i / (GRID * GRID));
                [r as f32 * step, g as f32 * step, b as f32 * step]
            })
            .collect();
        let mut info = self.effect_info(params, seed)?;
        info["width"] = n.into();
        info["height"] = 1.into();
        info["full_width"] = n.into();
        info["full_height"] = 1.into();
        if let Some(crate::native::Native::Color(f)) = crate::native::of(self) {
            let e: amb_plugin_sdk::Effect =
                serde_json::from_value(info).map_err(|e| e.to_string())?;
            for c in data.chunks_exact_mut(3) {
                let out = f(&e, [c[0], c[1], c[2]]);
                c.copy_from_slice(&out);
            }
        } else {
            call(self, &info, &mut data, FUEL + FUEL_PER_POINT * n as u64)?;
        }
        Ok(data
            .chunks_exact(3)
            .map(|c| {
                [c[0], c[1], c[2]].map(|v| {
                    if v.is_finite() {
                        v.clamp(0.0, 1.0)
                    } else {
                        0.0
                    }
                })
            })
            .collect())
    }

    /// Run a pixel effect on `rgb` (`width` × `height`, interleaved sRGB
    /// 0–1), changed in place.
    pub fn apply_pixels(
        &self,
        params: &Map<String, Value>,
        seed: u64,
        rgb: &mut [f32],
        width: usize,
        height: usize,
        region: Region,
    ) -> Result<(), String> {
        if self.manifest.effect != "pixels" {
            return Err(format!("{} isn't a pixel effect", self.manifest.name));
        }
        if width == 0 || height == 0 || rgb.len() != width * height * 3 {
            return Err("the image doesn't match its size".into());
        }
        let margin = self.manifest.margin as usize;
        let base = self.effect_info(params, seed)?;
        // The whole photo at once: one strip as tall as the photo.
        let margin = if self.manifest.whole { height } else { margin };
        if let Some(crate::native::Native::Pixels(f)) = crate::native::of(self) {
            return native_pixels(f, &base, rgb, width, height, region, margin);
        }
        if self.manifest.whole {
            let mut info = base.clone();
            info["x"] = region.x.into();
            info["y"] = region.y.into();
            info["width"] = width.into();
            info["height"] = height.into();
            info["full_width"] = region.full_width.into();
            info["full_height"] = region.full_height.into();
            call(
                self,
                &info,
                rgb,
                FUEL + FUEL_PER_PIXEL * (width * height) as u64,
            )?;
            for v in rgb.iter_mut() {
                *v = if v.is_finite() {
                    v.clamp(0.0, 1.0)
                } else {
                    0.0
                };
            }
            return Ok(());
        }
        let rows = (STRIP_FLOATS / (width * 3))
            .saturating_sub(2 * margin)
            .max(1);
        let mut y0 = 0;
        while y0 < height {
            let y1 = (y0 + rows).min(height);
            // The strip with its overlap above and below.
            let (a, b) = (y0.saturating_sub(margin), (y1 + margin).min(height));
            let mut strip = rgb[a * width * 3..b * width * 3].to_vec();
            let mut info = base.clone();
            info["x"] = region.x.into();
            info["y"] = (region.y + a).into();
            info["width"] = width.into();
            info["height"] = (b - a).into();
            info["full_width"] = region.full_width.into();
            info["full_height"] = region.full_height.into();
            let fuel = FUEL + FUEL_PER_PIXEL * (width * (b - a)) as u64;
            call(self, &info, &mut strip, fuel)?;
            let keep = &strip[(y0 - a) * width * 3..(y1 - a) * width * 3];
            for (o, v) in rgb[y0 * width * 3..y1 * width * 3].iter_mut().zip(keep) {
                *o = if v.is_finite() {
                    v.clamp(0.0, 1.0)
                } else {
                    0.0
                };
            }
            y0 = y1;
        }
        Ok(())
    }
}

/// A built-in pixel effect run natively, in strips on all cores (as in
/// the sandbox, each strip with `margin` rows of overlap, so the result is
/// the same).
fn native_pixels(
    f: fn(&amb_plugin_sdk::Effect, &mut amb_plugin_sdk::Pixels) -> Result<(), String>,
    base: &Value,
    rgb: &mut [f32],
    width: usize,
    height: usize,
    region: Region,
    margin: usize,
) -> Result<(), String> {
    use rayon::prelude::*;
    let mut e: amb_plugin_sdk::Effect =
        serde_json::from_value(base.clone()).map_err(|e| e.to_string())?;
    e.x = region.x as u32;
    e.full_width = region.full_width as u32;
    e.full_height = region.full_height as u32;
    // Strips tall enough that the overlap stays a small part of the work
    // (one strip for an effect that needs the whole photo).
    let rows = if margin >= height {
        height
    } else {
        (4 * margin).max(128)
    };
    let starts: Vec<usize> = (0..height).step_by(rows).collect();
    let src: &[f32] = rgb;
    let done: Vec<Result<Vec<f32>, String>> = starts
        .par_iter()
        .map(|&y0| {
            let y1 = (y0 + rows).min(height);
            let (a, b) = (y0.saturating_sub(margin), (y1 + margin).min(height));
            let mut strip = src[a * width * 3..b * width * 3].to_vec();
            let mut e = e.clone();
            e.y = (region.y + a) as u32;
            f(
                &e,
                &mut amb_plugin_sdk::Pixels {
                    data: &mut strip,
                    width,
                    height: b - a,
                },
            )?;
            Ok(strip[(y0 - a) * width * 3..(y1 - a) * width * 3].to_vec())
        })
        .collect();
    for (y0, keep) in starts.into_iter().zip(done) {
        let keep = keep?;
        for (o, v) in rgb[y0 * width * 3..].iter_mut().zip(&keep) {
            *o = if v.is_finite() {
                v.clamp(0.0, 1.0)
            } else {
                0.0
            };
        }
    }
    Ok(())
}

/// One call to `amb_effect` in a fresh instance.
fn call(plugin: &Plugin, info: &Value, data: &mut [f32], fuel: u64) -> Result<(), String> {
    let name = &plugin.manifest.name;
    let fail = |e: String| format!("{name}: {e}");
    let (mut store, instance) = instantiate(&plugin.wasm, fuel).map_err(fail)?;
    let run = instance
        .get_typed_func::<(i32, i32, i32, i32), i64>(&store, "amb_effect")
        .map_err(|e| fail(format!("amb_effect: {e}")))?;
    let info_bytes = serde_json::to_vec(info).map_err(|e| e.to_string())?;
    let info_ptr = pass(&mut store, &instance, &info_bytes).map_err(fail)?;
    let bytes: Vec<u8> = data.iter().flat_map(|v| v.to_le_bytes()).collect();
    let px = pass(&mut store, &instance, &bytes).map_err(fail)?;
    let n = i32::try_from(data.len()).map_err(|_| fail("too many pixels".into()))?;
    let packed = run
        .call(&mut store, (info_ptr, info_bytes.len() as i32, px, n))
        .map_err(|e| fail(trap(&e)))? as u64;
    let memory = instance
        .get_memory(&store, "memory")
        .ok_or_else(|| fail("no memory".into()))?;
    if packed != 0 {
        let (p, len) = (
            (packed >> 32) as usize,
            ((packed & 0xffff_ffff) as usize).min(1000),
        );
        let mut msg = vec![0u8; len];
        memory
            .read(&store, p, &mut msg)
            .map_err(|e| fail(e.to_string()))?;
        return Err(fail(String::from_utf8_lossy(&msg).into_owned()));
    }
    let mut out = vec![0u8; bytes.len()];
    memory
        .read(&store, px as u32 as usize, &mut out)
        .map_err(|e| fail(format!("couldn't read its pixels: {e}")))?;
    for (v, b) in data.iter_mut().zip(out.chunks_exact(4)) {
        *v = f32::from_le_bytes([b[0], b[1], b[2], b[3]]);
    }
    Ok(())
}

/// The engine's resolver for effect plugins: finds each effect with `find`
/// (by plugin id), makes colour effects into tables (kept for the last few
/// parameter sets, so moving another slider doesn't run the plugin again)
/// and wraps pixel effects.
pub fn resolver(
    find: impl Fn(&str) -> Result<Plugin, String> + Send + Sync + 'static,
) -> std::sync::Arc<amb_engine::plugin_effects::Resolver> {
    use amb_engine::lut::CubeLut;
    use amb_engine::plugin_effects::{EffectEntry, Resolved};
    use std::sync::{Arc, Mutex};
    type Cache = Vec<(String, Arc<CubeLut>)>;
    let tables: Mutex<Cache> = Mutex::new(Vec::new());
    let plugins: Mutex<Vec<(String, Arc<Plugin>)>> = Mutex::new(Vec::new());
    Arc::new(move |e: &EffectEntry| -> Result<Resolved, String> {
        // Plugins are loaded once per id (a reinstall shows on restart).
        let plugin = {
            let mut known = plugins.lock().unwrap_or_else(|p| p.into_inner());
            match known.iter().find(|(id, _)| *id == e.plugin) {
                Some((_, p)) => p.clone(),
                None => {
                    let p = Arc::new(find(&e.plugin)?);
                    known.push((e.plugin.clone(), p.clone()));
                    p
                }
            }
        };
        if !plugin.manifest.is_effect() {
            return Err(format!("{} isn't an effect", plugin.manifest.name));
        }
        if plugin.manifest.effect == "color" {
            let key = format!(
                "{}|{}|{}",
                plugin.fingerprint(),
                serde_json::Value::Object(e.params.clone()),
                e.seed
            );
            let mut cache = tables.lock().unwrap_or_else(|p| p.into_inner());
            if let Some((_, t)) = cache.iter().find(|(k, _)| *k == key) {
                return Ok(Resolved::Color(t.clone()));
            }
            let table = plugin.color_table(&e.params, e.seed)?;
            let lut = Arc::new(CubeLut::from_grid(&plugin.manifest.name, GRID, table)?);
            cache.push((key, lut.clone()));
            if cache.len() > 16 {
                cache.remove(0);
            }
            return Ok(Resolved::Color(lut));
        }
        // Checked now, so a bad parameter shows as "not rendered".
        plugin.manifest.resolve_params(&e.params)?;
        let (params, seed) = (e.params.clone(), e.seed);
        let whole = plugin.manifest.whole;
        let run: Arc<amb_engine::plugin_effects::PixelFn> = Arc::new(move |rgb, w, h, tile| {
            let region = Region {
                x: tile.x,
                y: tile.y,
                full_width: tile.full_width,
                full_height: tile.full_height,
            };
            plugin.apply_pixels(&params, seed, rgb, w, h, region)
        });
        Ok(if whole {
            Resolved::Whole(run)
        } else {
            Resolved::Pixels(run)
        })
    })
}
