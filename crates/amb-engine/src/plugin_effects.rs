// SPDX-License-Identifier: GPL-3.0-or-later
//! Looks from effect plugins, kept in the edit as `amb:Effects`
//! (docs/ai/plugins.md). Lightroom ignores the setting.
//!
//! The setting is a JSON list, applied in order after the LUT:
//!
//! ```json
//! [ { "plugin": "duotone", "params": { "shadows": 220 }, "amount": 80, "seed": 12345 } ]
//! ```
//!
//! The engine doesn't run plugins itself (they live in `amb-plugin`, which
//! builds on the engine): the app registers a [`Resolver`] that turns an
//! entry into a colour table or a pixel function. Where none is registered,
//! or a plugin isn't installed, the effect is listed as not rendered.

use crate::buffer::{Image, Tile};
use crate::lut::CubeLut;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};

/// One effect in the edit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EffectEntry {
    /// The plugin's id.
    pub plugin: String,
    /// Its parameters (the plugin's defaults fill the rest).
    #[serde(default)]
    pub params: serde_json::Map<String, serde_json::Value>,
    /// 0–100: how much of it, mixed with the photo before it.
    #[serde(default = "hundred")]
    pub amount: f64,
    /// The same for every render of this photo, so the look holds still.
    #[serde(default)]
    pub seed: u64,
}

fn hundred() -> f64 {
    100.0
}

/// The effects in an `amb:Effects` value.
pub fn parse(text: &str) -> Result<Vec<EffectEntry>, String> {
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }
    serde_json::from_str(text).map_err(|e| format!("amb:Effects isn't a list of effects: {e}"))
}

/// Runs a pixel effect on interleaved sRGB-encoded RGB (0–1) of the given
/// size, at its place in the photo.
pub type PixelFn = dyn Fn(&mut [f32], usize, usize, Tile) -> Result<(), String> + Send + Sync;

/// What an effect is, ready to render.
#[derive(Clone)]
pub enum Resolved {
    /// A colour table over sRGB-encoded values.
    Color(Arc<CubeLut>),
    Pixels(Arc<PixelFn>),
}

/// Turns an entry into something to render, or says why it can't.
pub type Resolver = dyn Fn(&EffectEntry) -> Result<Resolved, String> + Send + Sync;

static RESOLVER: RwLock<Option<Arc<Resolver>>> = RwLock::new(None);

/// Register how plugins are found (the editor and `amb` do this at start).
pub fn set_resolver(r: Arc<Resolver>) {
    *RESOLVER.write().unwrap_or_else(|e| e.into_inner()) = Some(r);
}

pub fn resolve(e: &EffectEntry) -> Result<Resolved, String> {
    let r = RESOLVER.read().unwrap_or_else(|e| e.into_inner()).clone();
    match r {
        Some(r) => r(e),
        None => Err("plugins aren't available here".into()),
    }
}

/// An effect of the edit, resolved.
#[derive(Clone)]
pub struct ActiveEffect {
    pub plugin: String,
    pub what: Resolved,
    /// 0..1.
    pub amount: f32,
}

impl std::fmt::Debug for ActiveEffect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let kind = match self.what {
            Resolved::Color(_) => "color",
            Resolved::Pixels(_) => "pixels",
        };
        f.debug_struct("ActiveEffect")
            .field("plugin", &self.plugin)
            .field("kind", &kind)
            .field("amount", &self.amount)
            .finish()
    }
}

/// Resolve the effects of an `amb:Effects` value: those that can be
/// rendered, and notes on those that can't.
pub fn active(text: &str) -> (Vec<ActiveEffect>, Vec<String>) {
    let entries = match parse(text) {
        Ok(e) => e,
        Err(e) => return (Vec::new(), vec![e]),
    };
    let mut out = Vec::new();
    let mut notes = Vec::new();
    for e in entries {
        let amount = (e.amount.clamp(0.0, 100.0) / 100.0) as f32;
        if amount <= 0.0 {
            continue;
        }
        match resolve(&e) {
            Ok(what) => out.push(ActiveEffect {
                plugin: e.plugin,
                what,
                amount,
            }),
            Err(why) => notes.push(format!("amb:Effects {} ({why})", e.plugin)),
        }
    }
    (out, notes)
}

/// Apply one effect to display-linear ProPhoto: through sRGB-encoded values
/// (colours outside sRGB are clipped for it, as for a LUT), mixed with the
/// photo by its amount. A pixel effect that fails leaves the photo as it was.
pub(crate) fn apply(out: &mut Image, e: &ActiveEffect, tile: &Tile) {
    match &e.what {
        Resolved::Color(table) => crate::pipeline::apply_lut(
            out,
            &crate::pipeline::LookLut {
                table: table.clone(),
                amount: f64::from(e.amount),
            },
        ),
        Resolved::Pixels(run) => {
            let (to_srgb, to_pro) = crate::pipeline::srgb_matrices();
            let mat = |m: &[[f32; 3]; 3], c: &[f32]| {
                [0, 1, 2].map(|i| m[i][0] * c[0] + m[i][1] * c[1] + m[i][2] * c[2])
            };
            let linear: Vec<f32> = out
                .rgb
                .chunks_exact(3)
                .flat_map(|c| mat(&to_srgb, c))
                .collect();
            let mut enc: Vec<f32> = linear
                .iter()
                .map(|v| crate::pipeline::srgb_encode(v.clamp(0.0, 1.0)))
                .collect();
            if let Err(why) = run(&mut enc, out.width, out.height, *tile) {
                eprintln!("effect {}: {why}", e.plugin);
                return;
            }
            let amount = e.amount;
            for ((o, lin), new) in out
                .rgb
                .chunks_exact_mut(3)
                .zip(linear.chunks_exact(3))
                .zip(enc.chunks_exact(3))
            {
                let mixed: Vec<f32> = (0..3)
                    .map(|i| {
                        let n = crate::pipeline::srgb_decode(new[i].clamp(0.0, 1.0));
                        lin[i] + (n - lin[i]) * amount
                    })
                    .collect();
                o.copy_from_slice(&mat(&to_pro, &mixed));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_parse_with_defaults() {
        let e = parse(r#"[{"plugin":"duotone","params":{"shadows":200}}]"#).unwrap();
        assert_eq!(e[0].plugin, "duotone");
        assert_eq!((e[0].amount, e[0].seed), (100.0, 0));
        assert!(parse("").unwrap().is_empty());
        assert!(parse("{").is_err());
    }
}
