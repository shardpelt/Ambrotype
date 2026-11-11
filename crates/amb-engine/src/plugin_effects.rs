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
//! `amount` goes to 200 (past 100 the change is pushed further than the
//! effect makes it). `where` keeps an effect to part of the photo, judged
//! from the photo before it: `highlights`, `midtones`, `shadows`, or a
//! colour (`reds`, `oranges`, `yellows`, `greens`, `aquas`, `blues`,
//! `purples`, `magentas`), fading in softly.
//!
//! The engine doesn't run plugins itself (they live in `amb-plugin`, which
//! builds on the engine): the app registers a [`Resolver`] that turns an
//! entry into a colour table or a pixel function. Where none is registered,
//! or a plugin isn't installed, the effect is listed as not rendered. A pixel
//! effect that reads the whole photo ([`Resolved::Whole`]) makes a render of
//! part of the photo render all of it.

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
    /// Only on part of the photo (see the module's notes); everywhere when
    /// left out.
    #[serde(default, rename = "where", skip_serializing_if = "Option::is_none")]
    pub place: Option<String>,
}

/// Where an effect goes, judged from the photo before it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Place {
    Highlights,
    Midtones,
    Shadows,
    /// A hue, in degrees.
    Hue(f32),
}

impl Place {
    pub fn parse(name: &str) -> Option<Place> {
        Some(match name.trim().to_lowercase().as_str() {
            "highlights" => Place::Highlights,
            "midtones" => Place::Midtones,
            "shadows" => Place::Shadows,
            "reds" => Place::Hue(0.0),
            "oranges" => Place::Hue(30.0),
            "yellows" => Place::Hue(60.0),
            "greens" => Place::Hue(120.0),
            "aquas" => Place::Hue(180.0),
            "blues" => Place::Hue(225.0),
            "purples" => Place::Hue(270.0),
            "magentas" => Place::Hue(315.0),
            _ => return None,
        })
    }

    /// How much of the effect a pixel gets (0–1), from its sRGB-encoded
    /// colour (0–1) before the effect.
    pub fn weight(self, c: [f32; 3]) -> f32 {
        let smooth = |a: f32, b: f32, x: f32| {
            let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
            t * t * (3.0 - 2.0 * t)
        };
        let l = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
        match self {
            Place::Highlights => smooth(0.45, 0.8, l),
            Place::Shadows => 1.0 - smooth(0.2, 0.55, l),
            Place::Midtones => (1.0 - ((l - 0.5) / 0.32).powi(2)).max(0.0),
            Place::Hue(centre) => {
                let (hi, lo) = (c[0].max(c[1]).max(c[2]), c[0].min(c[1]).min(c[2]));
                let chroma = hi - lo;
                if chroma <= 1e-4 {
                    return 0.0;
                }
                let h = if hi == c[0] {
                    60.0 * ((c[1] - c[2]) / chroma).rem_euclid(6.0)
                } else if hi == c[1] {
                    60.0 * ((c[2] - c[0]) / chroma + 2.0)
                } else {
                    60.0 * ((c[0] - c[1]) / chroma + 4.0)
                };
                let d = (h - centre).rem_euclid(360.0);
                let d = d.min(360.0 - d);
                (1.0 - d / 40.0).max(0.0) * smooth(0.04, 0.18, chroma)
            }
        }
    }
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
    /// A pixel effect that needs the whole photo at once (a kaleidoscope
    /// reads pixels from anywhere): a render of part of the photo renders
    /// all of it while the edit has one.
    Whole(Arc<PixelFn>),
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
    /// 0..2 (past 1, pushed further).
    pub amount: f32,
    pub place: Option<Place>,
}

impl ActiveEffect {
    /// It needs the whole photo at once.
    pub fn whole(&self) -> bool {
        matches!(self.what, Resolved::Whole(_))
    }
}

impl std::fmt::Debug for ActiveEffect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let kind = match self.what {
            Resolved::Color(_) => "color",
            Resolved::Pixels(_) => "pixels",
            Resolved::Whole(_) => "whole",
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
        let amount = (e.amount.clamp(0.0, 200.0) / 100.0) as f32;
        let place = e.place.as_deref().and_then(Place::parse);
        if amount <= 0.0 {
            continue;
        }
        match resolve(&e) {
            Ok(what) => out.push(ActiveEffect {
                plugin: e.plugin,
                what,
                amount,
                place,
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
    let (to_srgb, to_pro) = crate::pipeline::srgb_matrices();
    let mat = |m: &[[f32; 3]; 3], c: &[f32]| {
        [0, 1, 2].map(|i| m[i][0] * c[0] + m[i][1] * c[1] + m[i][2] * c[2])
    };
    // The photo before it, kept where only part of it changes or the
    // change is pushed past the effect's own.
    let before = (e.place.is_some() || e.amount > 1.0).then(|| out.rgb.clone());
    match &e.what {
        Resolved::Color(table) => crate::pipeline::apply_lut(
            out,
            &crate::pipeline::LookLut {
                table: table.clone(),
                amount: f64::from(e.amount.min(1.0)),
            },
        ),
        Resolved::Pixels(run) | Resolved::Whole(run) => {
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
            let amount = e.amount.min(1.0);
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
    let Some(before) = before else {
        return;
    };
    // What changed, pushed (amount past 1) and kept to its place.
    let push = e.amount.max(1.0);
    for (o, b) in out.rgb.chunks_exact_mut(3).zip(before.chunks_exact(3)) {
        let w = e.place.map_or(1.0, |p| {
            let s = mat(&to_srgb, b).map(|v| crate::pipeline::srgb_encode(v.clamp(0.0, 1.0)));
            p.weight(s)
        });
        for i in 0..3 {
            o[i] = (b[i] + (o[i] - b[i]) * push * w).max(0.0);
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

    #[test]
    fn places_weigh_tones_and_colours() {
        let (hi, sh, mid) = (Place::Highlights, Place::Shadows, Place::Midtones);
        assert!(hi.weight([0.95; 3]) > 0.99 && hi.weight([0.1; 3]) < 0.01);
        assert!(sh.weight([0.05; 3]) > 0.99 && sh.weight([0.9; 3]) < 0.01);
        assert!(mid.weight([0.5; 3]) > 0.99 && mid.weight([0.02; 3]) < 0.01);
        let blues = Place::parse("Blues").unwrap();
        assert!(
            blues.weight([0.1, 0.3, 0.9]) > 0.6,
            "{}",
            blues.weight([0.1, 0.3, 0.9])
        );
        assert!(blues.weight([0.9, 0.2, 0.1]) < 0.01 && blues.weight([0.5; 3]) == 0.0);
        assert!(Place::parse("everywhere").is_none());
        let e = parse(r#"[{"plugin":"x","where":"shadows","amount":150}]"#).unwrap();
        assert_eq!(e[0].place.as_deref(), Some("shadows"));
        let back = serde_json::to_string(&parse(r#"[{"plugin":"x"}]"#).unwrap()).unwrap();
        assert!(!back.contains("where"), "{back}");
    }
}
