// SPDX-License-Identifier: MIT OR Apache-2.0
//! Write Ambrotype plugins (docs/ai/plugins.md).
//!
//! A plugin is a function from [`Input`] (the photo's current settings, the
//! plugin's own parameters and, with permission, facts about the photo) to
//! [`Output`] (edits, as `amb-protocol` commands, and a message). Ambrotype
//! runs it as WebAssembly in a sandbox with no file or network access, then
//! applies the edits as one undo step.
//!
//! ```ignore
//! use amb_plugin_sdk::{Input, Output};
//!
//! fn run(input: Input) -> Result<Output, String> {
//!     let warmth = input.number("warmth");
//!     Ok(Output::new()
//!         .adjust("Temperature", warmth * 10.0)
//!         .message(format!("Warmer by {} K", warmth * 10.0)))
//! }
//!
//! amb_plugin_sdk::export_plugin!(run);
//! ```
//!
//! An *effect* plugin changes the look itself rather than moving sliders:
//! a colour effect maps each colour to another ([`export_effect!`] with
//! `color:`), a pixel effect gets the image ([`export_effect!`] with
//! `pixels:`). Its parameters are kept in the photo's edit, so it is
//! applied again whenever the photo is rendered.
//!
//! Build with `amb plugin build <folder>` (it runs `cargo build --target
//! wasm32-unknown-unknown`). The same `run` can be tested natively with
//! `cargo test`, using [`Input::for_test`].

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// The plugin interface version this SDK speaks.
pub const API: u32 = 1;

/// What a plugin gets when it runs.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Input {
    pub api: u32,
    /// The plugin's parameters (from its manifest), with defaults filled in.
    #[serde(default)]
    pub params: Map<String, Value>,
    /// The photo's develop settings that are set, by Lightroom name
    /// ("Exposure2012", "Temperature", ...). Missing ones are at their default.
    #[serde(default)]
    pub settings: Map<String, Value>,
    /// Camera and shot facts; only with the `photo-info` permission.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub photo: Option<Photo>,
    /// Statistics of the photo as currently edited; only with `photo-stats`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stats: Option<Stats>,
    /// A new random number for each run (plugins have no clock or randomness).
    #[serde(default)]
    pub seed: u64,
}

/// Camera and shot facts (the `photo-info` permission).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Photo {
    #[serde(default)]
    pub camera: String,
    #[serde(default)]
    pub lens: Option<String>,
    /// Upright pixel size of the raw.
    #[serde(default)]
    pub width: u32,
    #[serde(default)]
    pub height: u32,
    #[serde(default)]
    pub iso: Option<f64>,
    /// Seconds.
    #[serde(default)]
    pub shutter: Option<f64>,
    #[serde(default)]
    pub aperture: Option<f64>,
    /// Millimetres.
    #[serde(default)]
    pub focal_length: Option<f64>,
    /// The camera's white balance as Temperature (K) and Tint, the values
    /// "As Shot" stands for.
    #[serde(default)]
    pub as_shot_temperature: Option<f64>,
    #[serde(default)]
    pub as_shot_tint: Option<f64>,
}

/// Histograms (256 bins, sRGB 8-bit) and the mean colour of a small render
/// of the photo with its current edits (the `photo-stats` permission).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Stats {
    pub width: u32,
    pub height: u32,
    pub red: Vec<u32>,
    pub green: Vec<u32>,
    pub blue: Vec<u32>,
    /// Rec. 709 luma of the sRGB values.
    pub luma: Vec<u32>,
    /// Mean sRGB colour, 0–1.
    pub mean: [f64; 3],
}

impl Stats {
    /// Statistics of 8-bit sRGB pixels (`rgb` is width × height × 3 bytes).
    pub fn from_rgb8(rgb: &[u8], width: u32, height: u32) -> Stats {
        let mut s = Stats {
            width,
            height,
            red: vec![0; 256],
            green: vec![0; 256],
            blue: vec![0; 256],
            luma: vec![0; 256],
            mean: [0.0; 3],
        };
        let mut sum = [0u64; 3];
        let mut n = 0u64;
        for p in rgb.chunks_exact(3) {
            s.red[p[0] as usize] += 1;
            s.green[p[1] as usize] += 1;
            s.blue[p[2] as usize] += 1;
            let y = 0.2126 * f64::from(p[0]) + 0.7152 * f64::from(p[1]) + 0.0722 * f64::from(p[2]);
            s.luma[(y.round() as usize).min(255)] += 1;
            for c in 0..3 {
                sum[c] += u64::from(p[c]);
            }
            n += 1;
        }
        if n > 0 {
            s.mean = sum.map(|v| v as f64 / n as f64 / 255.0);
        }
        s
    }

    /// The luma level (0–1) below which `fraction` (0–1) of the pixels lie.
    pub fn percentile(&self, fraction: f64) -> f64 {
        let total: u64 = self.luma.iter().map(|&v| u64::from(v)).sum();
        if total == 0 {
            return 0.5;
        }
        let target = (fraction.clamp(0.0, 1.0) * total as f64).ceil() as u64;
        let mut acc = 0u64;
        for (i, &v) in self.luma.iter().enumerate() {
            acc += u64::from(v);
            if acc >= target.max(1) {
                return i as f64 / 255.0;
            }
        }
        1.0
    }

    /// The share (0–1) of pixels whose luma is at or above `level` (0–1).
    pub fn share_above(&self, level: f64) -> f64 {
        let total: u64 = self.luma.iter().map(|&v| u64::from(v)).sum();
        let from = (level.clamp(0.0, 1.0) * 255.0).round() as usize;
        let above: u64 = self.luma[from.min(255)..]
            .iter()
            .map(|&v| u64::from(v))
            .sum();
        if total == 0 {
            0.0
        } else {
            above as f64 / total as f64
        }
    }
}

impl Input {
    /// A number parameter (or 0 if missing).
    pub fn number(&self, name: &str) -> f64 {
        self.params.get(name).and_then(Value::as_f64).unwrap_or(0.0)
    }

    /// An on/off parameter (or false if missing).
    pub fn flag(&self, name: &str) -> bool {
        self.params
            .get(name)
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }

    /// A choice parameter (or "" if missing).
    pub fn choice(&self, name: &str) -> &str {
        self.params.get(name).and_then(Value::as_str).unwrap_or("")
    }

    /// A numeric develop setting, if it is set.
    pub fn setting(&self, name: &str) -> Option<f64> {
        self.settings.get(name).and_then(Value::as_f64)
    }

    /// A pseudo-random generator seeded from [`Input::seed`].
    pub fn rng(&self) -> Rng {
        Rng(self.seed ^ 0x9E37_79B9_7F4A_7C15)
    }

    /// An input for native tests: these parameters, no settings.
    pub fn for_test(params: Value) -> Input {
        Input {
            api: API,
            params: params.as_object().cloned().unwrap_or_default(),
            ..Input::default()
        }
    }
}

/// A small deterministic random number generator (splitmix64).
pub struct Rng(u64);

impl Rng {
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in 0..1.
    pub fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Uniform in `lo..hi`.
    pub fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.unit()
    }
}

/// What an effect gets each time it runs: its parameters, a seed that stays
/// the same for the photo (so the look doesn't change between renders),
/// and where its pixels sit in the photo.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Effect {
    pub api: u32,
    /// The effect's parameters (from its manifest), with defaults filled in.
    #[serde(default)]
    pub params: Map<String, Value>,
    #[serde(default)]
    pub seed: u64,
    /// The pixels' top-left corner in the whole photo, and the whole photo's
    /// size (a pixel effect may get the photo in strips or tiles).
    #[serde(default)]
    pub x: u32,
    #[serde(default)]
    pub y: u32,
    #[serde(default)]
    pub full_width: u32,
    #[serde(default)]
    pub full_height: u32,
}

impl Effect {
    /// A number parameter (or 0 if missing).
    pub fn number(&self, name: &str) -> f64 {
        self.params.get(name).and_then(Value::as_f64).unwrap_or(0.0)
    }

    /// An on/off parameter (or false if missing).
    pub fn flag(&self, name: &str) -> bool {
        self.params
            .get(name)
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }

    /// A choice parameter (or "" if missing).
    pub fn choice(&self, name: &str) -> &str {
        self.params.get(name).and_then(Value::as_str).unwrap_or("")
    }

    /// A random number in 0..1 for the photo's pixel (x, y) (whole-photo
    /// coordinates): the same every time, whatever part is rendered.
    pub fn noise(&self, x: u32, y: u32) -> f32 {
        let mut r =
            Rng(self.seed
                ^ ((u64::from(y) << 32) | u64::from(x)).wrapping_mul(0xD6E8_FEB8_6659_FD93));
        (r.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }

    /// A pseudo-random generator seeded from [`Effect::seed`].
    pub fn rng(&self) -> Rng {
        Rng(self.seed ^ 0x9E37_79B9_7F4A_7C15)
    }

    /// An effect for native tests: these parameters, a `w` × `h` photo.
    pub fn for_test(params: Value, w: u32, h: u32) -> Effect {
        Effect {
            api: API,
            params: params.as_object().cloned().unwrap_or_default(),
            full_width: w,
            full_height: h,
            ..Effect::default()
        }
    }
}

/// The pixels a pixel effect changes in place: RGB, interleaved, as sRGB
/// values from 0 to 1 (as in a JPEG, before it is rounded to 8 bits).
pub struct Pixels<'a> {
    pub data: &'a mut [f32],
    pub width: usize,
    pub height: usize,
}

impl Pixels<'_> {
    pub fn get(&self, x: usize, y: usize) -> [f32; 3] {
        let i = (y * self.width + x) * 3;
        [self.data[i], self.data[i + 1], self.data[i + 2]]
    }

    pub fn set(&mut self, x: usize, y: usize, rgb: [f32; 3]) {
        let i = (y * self.width + x) * 3;
        self.data[i..i + 3].copy_from_slice(&rgb);
    }
}

/// What a plugin returns: edits and an optional message for the user.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Output {
    /// `amb-protocol` commands: `set_param`, `adjust_param`, `reset_param`
    /// and `reset_group` (docs/ai/settings-reference.md has the names).
    #[serde(default)]
    pub commands: Vec<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl Output {
    pub fn new() -> Output {
        Output::default()
    }

    /// Set a setting (numbers are clamped to its range).
    pub fn set(mut self, name: &str, value: impl Into<Value>) -> Output {
        self.commands
            .push(serde_json::json!({ "cmd": "set_param", "name": name, "value": value.into() }));
        self
    }

    /// Add to a numeric setting.
    pub fn adjust(mut self, name: &str, delta: f64) -> Output {
        self.commands
            .push(serde_json::json!({ "cmd": "adjust_param", "name": name, "delta": delta }));
        self
    }

    /// Put a setting back to its default.
    pub fn reset(mut self, name: &str) -> Output {
        self.commands
            .push(serde_json::json!({ "cmd": "reset_param", "name": name }));
        self
    }

    /// Put a group of settings back to their defaults (e.g. "hsl").
    pub fn reset_group(mut self, group: &str) -> Output {
        self.commands
            .push(serde_json::json!({ "cmd": "reset_group", "group": group }));
        self
    }

    /// A short note shown to the user after the run.
    pub fn message(mut self, text: impl Into<String>) -> Output {
        self.message = Some(text.into());
        self
    }
}

/// Lines from [`log`], handed back with the output.
static LOG: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// Write a line to the plugin's log (shown in the Plugins dialog). The lines
/// go back with the output, so a plugin needs nothing from the host.
pub fn log(text: &str) {
    #[cfg(target_arch = "wasm32")]
    if let Ok(mut log) = LOG.lock() {
        if log.len() < 200 {
            log.push(text.chars().take(1000).collect());
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    eprintln!("{text}");
}

/// The lines logged since the last call.
fn take_log() -> Vec<String> {
    LOG.lock()
        .map(|mut l| std::mem::take(&mut *l))
        .unwrap_or_default()
}

/// The WebAssembly glue `export_plugin!` expands to. Not for direct use.
#[doc(hidden)]
pub mod abi {
    use super::{Input, Output, take_log};

    /// Memory for the host to write the input into.
    pub fn alloc(len: i32) -> i32 {
        let mut buf = Vec::<u8>::with_capacity(len.max(0) as usize);
        let ptr = buf.as_mut_ptr();
        std::mem::forget(buf);
        ptr as usize as i32
    }

    /// Read the input, run, and hand back the output as `(ptr << 32) | len`.
    ///
    /// # Safety
    /// `ptr` and `len` must describe the bytes the host wrote after `alloc`.
    pub unsafe fn run(ptr: i32, len: i32, f: fn(Input) -> Result<Output, String>) -> i64 {
        // SAFETY: the host wrote `len` bytes into the buffer `alloc` returned.
        let bytes =
            unsafe { Vec::from_raw_parts(ptr as usize as *mut u8, len as usize, len as usize) };
        let mut result = match serde_json::from_slice::<Input>(&bytes) {
            Ok(input) => f(input)
                .map(|o| serde_json::to_value(o).unwrap_or_default())
                .unwrap_or_else(|e| serde_json::json!({ "error": e })),
            Err(e) => serde_json::json!({ "error": format!("bad input: {e}") }),
        };
        let log = take_log();
        if let (false, Some(map)) = (log.is_empty(), result.as_object_mut()) {
            map.insert("log".into(), log.into());
        }
        let mut out = serde_json::to_vec(&result)
            .unwrap_or_default()
            .into_boxed_slice();
        let (p, n) = (out.as_mut_ptr() as usize as i64, out.len() as i64);
        std::mem::forget(out);
        (p << 32) | n
    }
}

/// What an effect does: map one colour, or change the pixels.
#[doc(hidden)]
pub enum EffectFn {
    Color(fn(&Effect, [f32; 3]) -> [f32; 3]),
    Pixels(fn(&Effect, &mut Pixels) -> Result<(), String>),
}

#[doc(hidden)]
pub mod effect_abi {
    use super::{Effect, EffectFn, Pixels};

    /// Run the effect on `n` floats at `px` (changed in place); `info` is the
    /// [`Effect`] JSON with "width" and "height". Returns 0, or an error
    /// message as `(ptr << 32) | len`.
    ///
    /// # Safety
    /// The buffers must be the host's, from `amb_alloc`.
    pub unsafe fn run(info: i32, info_len: i32, px: i32, n: i32, f: EffectFn) -> i64 {
        // SAFETY: the host wrote `info_len` bytes at `info` and `n` floats at `px`.
        let bytes = unsafe {
            Vec::from_raw_parts(
                info as usize as *mut u8,
                info_len as usize,
                info_len as usize,
            )
        };
        let data = unsafe { std::slice::from_raw_parts_mut(px as usize as *mut f32, n as usize) };
        let result = (|| {
            let v: serde_json::Value =
                serde_json::from_slice(&bytes).map_err(|e| format!("bad input: {e}"))?;
            let effect: Effect =
                serde_json::from_value(v.clone()).map_err(|e| format!("bad input: {e}"))?;
            match f {
                EffectFn::Color(f) => {
                    for c in data.chunks_exact_mut(3) {
                        let out = f(&effect, [c[0], c[1], c[2]]);
                        c.copy_from_slice(&out);
                    }
                    Ok(())
                }
                EffectFn::Pixels(f) => {
                    let width = v["width"].as_u64().unwrap_or(0) as usize;
                    let height = v["height"].as_u64().unwrap_or(0) as usize;
                    if width * height * 3 != data.len() {
                        return Err("the pixels don't match their size".to_string());
                    }
                    f(
                        &effect,
                        &mut Pixels {
                            data,
                            width,
                            height,
                        },
                    )
                }
            }
        })();
        match result {
            Ok(()) => 0,
            Err(e) => {
                let mut out = e.into_bytes().into_boxed_slice();
                let (p, n) = (out.as_mut_ptr() as usize as i64, out.len() as i64);
                std::mem::forget(out);
                (p << 32) | n
            }
        }
    }
}

/// Export an effect: `export_effect!(color: f)` with
/// `f: fn(&Effect, [f32; 3]) -> [f32; 3]` (sRGB 0–1 in and out; the app
/// turns it into a colour table, so it runs fast everywhere), or
/// `export_effect!(pixels: f)` with
/// `f: fn(&Effect, &mut Pixels) -> Result<(), String>`.
#[macro_export]
macro_rules! export_effect {
    (color: $f:path) => {
        $crate::export_effect!(@glue $crate::EffectFn::Color($f));
    };
    (pixels: $f:path) => {
        $crate::export_effect!(@glue $crate::EffectFn::Pixels($f));
    };
    (@glue $e:expr) => {
        #[cfg(target_arch = "wasm32")]
        #[unsafe(no_mangle)]
        pub extern "C" fn amb_alloc(len: i32) -> i32 {
            $crate::abi::alloc(len)
        }

        #[cfg(target_arch = "wasm32")]
        #[unsafe(no_mangle)]
        pub extern "C" fn amb_effect(info: i32, info_len: i32, px: i32, n: i32) -> i64 {
            // SAFETY: called by the host with buffers from `amb_alloc`.
            unsafe { $crate::effect_abi::run(info, info_len, px, n, $e) }
        }
    };
}

/// Export `run` (a `fn(Input) -> Result<Output, String>`) as the plugin.
#[macro_export]
macro_rules! export_plugin {
    ($run:path) => {
        #[cfg(target_arch = "wasm32")]
        #[unsafe(no_mangle)]
        pub extern "C" fn amb_alloc(len: i32) -> i32 {
            $crate::abi::alloc(len)
        }

        #[cfg(target_arch = "wasm32")]
        #[unsafe(no_mangle)]
        pub extern "C" fn amb_run(ptr: i32, len: i32) -> i64 {
            // SAFETY: called by the host with the buffer from `amb_alloc`.
            unsafe { $crate::abi::run(ptr, len, $run) }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stats_and_percentiles() {
        // Half black, half white.
        let mut rgb = vec![0u8; 50 * 3];
        rgb.extend(vec![255u8; 50 * 3]);
        let s = Stats::from_rgb8(&rgb, 10, 10);
        assert_eq!((s.luma[0], s.luma[255]), (50, 50));
        assert_eq!(s.percentile(0.25), 0.0);
        assert_eq!(s.percentile(0.75), 1.0);
        assert!((s.mean[1] - 0.5).abs() < 1e-9);
        assert!((s.share_above(0.9) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn output_builds_commands() {
        let o = Output::new()
            .set("Exposure2012", 0.5)
            .adjust("Contrast2012", -10.0)
            .reset_group("hsl")
            .message("ok");
        let v = serde_json::to_value(&o).unwrap();
        assert_eq!(v["commands"][0]["cmd"], "set_param");
        assert_eq!(v["commands"][1]["delta"], -10.0);
        assert_eq!(v["message"], "ok");
        let input = Input::for_test(serde_json::json!({ "warmth": 3, "keep": true }));
        assert_eq!((input.number("warmth"), input.flag("keep")), (3.0, true));
        let mut r = input.rng();
        assert!((0.0..1.0).contains(&r.unit()));
    }
}
