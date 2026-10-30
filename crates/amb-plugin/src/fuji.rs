// SPDX-License-Identifier: GPL-3.0-or-later
//! Fujifilm-style camera recipes, pasted as text and recreated as develop
//! settings through the built-in Film recipe plugin (docs/ai/film-recipes.md).
//!
//! A recipe is the list of picture settings people share for Fujifilm
//! cameras ("Film Simulation: …", "Dynamic Range: DR400", "White Balance:
//! Auto, +2 Red & -4 Blue", …). [`parse`] reads such text line by line into
//! the plugin's parameters, keeping to what the sensor generation's cameras
//! offer (ranges, half steps, which settings exist); [`recreate`] runs the
//! plugin on an empty edit, so the result is a preset.
//!
//! Nothing here is Fujifilm's: the looks and the numbers are ours, matched
//! by name only (docs/design/film-look-measurement.md).

use std::collections::BTreeMap;

use serde_json::{Map, Value};

/// A Fujifilm sensor generation: which picture settings its cameras have.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
pub enum Sensor {
    /// Bayer and EXR cameras (X100, X10, X-A, XF10 …): treated as X-Trans II.
    Bayer,
    XTrans1,
    XTrans2,
    XTrans3,
    XTrans4,
    XTrans5,
    /// GFX: has every setting of X-Trans V.
    Gfx,
}

impl Sensor {
    pub const ALL: [Sensor; 7] = [
        Sensor::XTrans5,
        Sensor::XTrans4,
        Sensor::XTrans3,
        Sensor::XTrans2,
        Sensor::XTrans1,
        Sensor::Bayer,
        Sensor::Gfx,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Sensor::Bayer => "Bayer",
            Sensor::XTrans1 => "X-Trans I",
            Sensor::XTrans2 => "X-Trans II",
            Sensor::XTrans3 => "X-Trans III",
            Sensor::XTrans4 => "X-Trans IV",
            Sensor::XTrans5 => "X-Trans V",
            Sensor::Gfx => "GFX",
        }
    }

    /// From a name ("X-Trans IV", "xtrans4", "IV", "5", "gfx") or a camera
    /// model ("X-T3", "FUJIFILM X100V").
    pub fn from_name(text: &str) -> Option<Sensor> {
        let t = text
            .trim()
            .to_ascii_lowercase()
            .replace(['-', ' ', '_'], "");
        let t = t.strip_prefix("fujifilm").unwrap_or(&t);
        let generation = t.strip_prefix("xtrans").unwrap_or(t);
        let by_gen = match generation {
            "i" | "1" => Some(Sensor::XTrans1),
            "ii" | "2" => Some(Sensor::XTrans2),
            "iii" | "3" => Some(Sensor::XTrans3),
            "iv" | "4" => Some(Sensor::XTrans4),
            "v" | "5" => Some(Sensor::XTrans5),
            "gfx" => Some(Sensor::Gfx),
            "bayer" | "exr" => Some(Sensor::Bayer),
            _ => None,
        };
        by_gen.or_else(|| camera(t))
    }

    fn at_least(self, s: Sensor) -> bool {
        self == Sensor::Gfx || (self != Sensor::Bayer && self >= s)
    }
}

/// The sensor of a camera model (letters and digits only, lowercase).
fn camera(model: &str) -> Option<Sensor> {
    use Sensor::*;
    if model.starts_with("gfx") {
        return Some(Gfx);
    }
    const MODELS: &[(&str, Sensor)] = &[
        ("xpro1", XTrans1),
        ("xe1", XTrans1),
        ("xm1", XTrans1),
        ("xt1", XTrans2),
        ("xt10", XTrans2),
        ("xe2", XTrans2),
        ("xe2s", XTrans2),
        ("x100s", XTrans2),
        ("x100t", XTrans2),
        ("x70", XTrans2),
        ("x30", XTrans2),
        ("xq1", XTrans2),
        ("xq2", XTrans2),
        ("x20", XTrans2),
        ("xpro2", XTrans3),
        ("xt2", XTrans3),
        ("xt20", XTrans3),
        ("xe3", XTrans3),
        ("xh1", XTrans3),
        ("x100f", XTrans3),
        ("xt100", Bayer),
        ("xt200", Bayer),
        ("xa1", Bayer),
        ("xa2", Bayer),
        ("xa3", Bayer),
        ("xa5", Bayer),
        ("xa7", Bayer),
        ("xf10", Bayer),
        ("x100", Bayer),
        ("x10", Bayer),
        ("xt3", XTrans4),
        ("xt30", XTrans4),
        ("xt30ii", XTrans4),
        ("xpro3", XTrans4),
        ("x100v", XTrans4),
        ("xt4", XTrans4),
        ("xs10", XTrans4),
        ("xe4", XTrans4),
        ("xt5", XTrans5),
        ("xh2", XTrans5),
        ("xh2s", XTrans5),
        ("xs20", XTrans5),
        ("x100vi", XTrans5),
        ("xt50", XTrans5),
        ("xm5", XTrans5),
        ("xe5", XTrans5),
    ];
    MODELS.iter().find(|(m, _)| *m == model).map(|&(_, s)| s)
}

/// A recipe read from text.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Recipe {
    /// Its title: the first line that isn't a setting.
    pub name: Option<String>,
    pub sensor: Sensor,
    /// The sensor was named in the text, not chosen by the caller.
    pub sensor_from_text: bool,
    /// The Film recipe plugin's parameters.
    pub params: Map<String, Value>,
    /// What was left out, changed or approximated, one line each.
    pub notes: Vec<String>,
}

/// Film simulation names, in the order they're tried (longest first where
/// one contains another), and our look for each.
const LOOKS: &[(&[&str], &str, Sensor)] = &[
    (
        &["eterna bleach bypass", "bleach bypass"],
        "Bleach Bypass",
        Sensor::XTrans4,
    ),
    (&["eterna", "cinema"], "Cine Flat", Sensor::XTrans3),
    (&["classic chrome"], "Muted Chrome", Sensor::XTrans2),
    (
        &["classic neg", "classic negative"],
        "Street Negative",
        Sensor::XTrans4,
    ),
    (
        &["nostalgic neg", "nostalgic negative"],
        "Amber Negative",
        Sensor::XTrans5,
    ),
    (
        &["pro neg hi", "pro neg. hi", "pro neg high", "proneg hi"],
        "Studio Negative",
        Sensor::XTrans1,
    ),
    (
        &[
            "pro neg std",
            "pro neg. std",
            "pro neg standard",
            "proneg std",
        ],
        "Studio Negative Soft",
        Sensor::XTrans1,
    ),
    (&["reala ace", "reala"], "Real Colour", Sensor::XTrans5),
    (&["velvia", "vivid"], "Vivid Slide", Sensor::XTrans1),
    (&["astia", "soft"], "Soft Slide", Sensor::XTrans1),
    (&["provia", "standard"], "Standard", Sensor::XTrans1),
    (&["acros"], "Fine Mono", Sensor::XTrans3),
    (
        &["monochrome", "mono", "b&w", "black and white"],
        "Mono",
        Sensor::XTrans1,
    ),
    (&["sepia"], "Sepia", Sensor::XTrans1),
];

/// Lowercase, plain ASCII signs and dashes, no bullets.
fn clean(line: &str) -> String {
    line.trim()
        .trim_start_matches(['-', '*', '•', '·', '–', '—'])
        .trim()
        .replace(['−', '–', '—'], "-")
        .replace('＋', "+")
        .to_lowercase()
}

/// The numbers in a text, with fractions such as "+1/3" and half steps.
fn numbers(text: &str) -> Vec<f64> {
    let mut out = Vec::new();
    let b = text.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let start = i;
        if (b[i] == b'+' || b[i] == b'-') && i + 1 < b.len() && b[i + 1].is_ascii_digit() {
            i += 1;
        }
        if i < b.len() && b[i].is_ascii_digit() {
            while i < b.len() && (b[i].is_ascii_digit() || b[i] == b'.' || b[i] == b'/') {
                i += 1;
            }
            let s = &text[start..i];
            let v = match s.split_once('/') {
                Some((n, d)) => n
                    .parse::<f64>()
                    .ok()
                    .zip(d.parse::<f64>().ok())
                    .map(|(n, d)| n / d),
                None => s.trim_end_matches('.').parse().ok(),
            };
            out.extend(v);
        } else {
            i = start + 1;
        }
    }
    out
}

/// "Weak" / "Strong" / "Off", and "Auto" when `auto` is allowed.
fn strength(v: &str, auto: bool) -> Option<&'static str> {
    if v.contains("strong") || v.contains("high") {
        Some("Strong")
    } else if v.contains("weak") || v.contains("low") {
        Some("Weak")
    } else if auto && v.contains("auto") {
        Some("Auto")
    } else if v.contains("off") || v.contains("none") || v.trim() == "0" {
        Some("Off")
    } else {
        None
    }
}

/// Read a pasted recipe. `sensor` is used when the text doesn't name one;
/// with neither, X-Trans V.
pub fn parse(text: &str, sensor: Option<Sensor>) -> Recipe {
    let mut r = Recipe {
        name: None,
        sensor: sensor.unwrap_or(Sensor::XTrans5),
        sensor_from_text: false,
        params: Map::new(),
        notes: Vec::new(),
    };
    // The sensor first: it decides the ranges.
    for line in text.lines().map(clean) {
        if let Some(found) = sensor_in(&line) {
            r.sensor = found;
            r.sensor_from_text = true;
            break;
        }
    }
    let mut look: Option<(&str, Sensor, String)> = None;
    let mut filter: Option<&str> = None;
    for raw in text.lines() {
        let line = clean(raw);
        if line.is_empty() {
            continue;
        }
        let (key, value) = match line.split_once(':') {
            Some((k, v)) => (k.trim().to_string(), v.trim().to_string()),
            None => (String::new(), line.clone()),
        };
        let key = key.as_str();
        let v = value.as_str();
        let set = |r: &mut Recipe, k: &str, x: Value| {
            r.params.insert(k.into(), x);
        };
        if key.is_empty() {
            // A bare film simulation name, the title, or the sensor.
            if let Some(l) = look_for(v).filter(|_| bare_look(v)) {
                look = Some((l.0, l.1, raw.trim().to_string()));
                filter = filter.or(mono_filter(v));
            } else if sensor_in(v).is_none() && r.name.is_none() && look.is_none() {
                r.name = Some(raw.trim().to_string());
            }
            continue;
        }
        let has = |w: &str| key.contains(w);
        if has("film sim") || key == "simulation" || key == "film" {
            match look_for(v) {
                Some(l) => {
                    look = Some((l.0, l.1, raw.trim().to_string()));
                    filter = mono_filter(v);
                }
                None => r.notes.push(format!(
                    "Unknown film simulation “{}”: used Standard",
                    value
                )),
            }
        } else if has("priority") {
            match strength(v, true) {
                Some(s) => set(&mut r, "dr_priority", s.into()),
                None => r.notes.push(format!("Couldn't read “{}”", raw.trim())),
            }
        } else if has("dynamic range") || key == "dr" {
            let dr = if v.contains("400") {
                "DR400"
            } else if v.contains("200") {
                "DR200"
            } else if v.contains("auto") {
                r.notes.push("DR-Auto: used DR200".into());
                "DR200"
            } else {
                "DR100"
            };
            set(&mut r, "dynamic_range", dr.into());
        } else if has("tone curve") {
            // "H -1 S +1", "+1H & -2S", "Highlight -1, Shadow +1".
            for (k, n) in tone_curve(v) {
                set(&mut r, k, n.into());
            }
        } else if has("highlight") {
            if let Some(&n) = numbers(v).first() {
                set(&mut r, "highlight", n.into());
            }
        } else if has("shadow") {
            if let Some(&n) = numbers(v).first() {
                set(&mut r, "shadow", n.into());
            }
        } else if has("chrome") && (has("blue") || has("fx")) {
            match strength(v, false) {
                Some(s) => set(&mut r, "blue_depth", s.into()),
                None => r.notes.push(format!("Couldn't read “{}”", raw.trim())),
            }
        } else if has("chrome") || key == "cce" {
            match strength(v, false) {
                Some(s) => set(&mut r, "colour_depth", s.into()),
                None => r.notes.push(format!("Couldn't read “{}”", raw.trim())),
            }
        } else if has("monochromatic")
            || has("toning")
            || key == "mono color"
            || key == "monochrome color"
        {
            // "WC +2 & MG -1", "Warm 2, Magenta -1".
            for part in v.split([',', '&', ';']) {
                let Some(&n) = numbers(part).first() else {
                    continue;
                };
                if part.contains("wc") || part.contains("warm") || part.contains("cool") {
                    set(&mut r, "tone_warm", n.into());
                } else if part.contains("mg") || part.contains("magenta") || part.contains("green")
                {
                    set(&mut r, "tone_magenta", n.into());
                }
            }
        } else if has("color") || has("colour") || has("saturation") {
            if let Some(&n) = numbers(v).first() {
                set(&mut r, "colour", n.into());
            }
        } else if has("sharp") {
            if let Some(&n) = numbers(v).first() {
                set(&mut r, "sharpness", n.into());
            }
        } else if has("noise") || key == "nr" || key == "high iso nr" {
            if let Some(&n) = numbers(v).first() {
                set(&mut r, "noise_reduction", n.into());
            }
        } else if has("clarity") {
            if let Some(&n) = numbers(v).first() {
                set(&mut r, "clarity", n.into());
            }
        } else if has("grain") {
            let s = strength(v, false).unwrap_or("Off");
            set(&mut r, "grain", s.into());
            if v.contains("large") {
                set(&mut r, "grain_size", "Large".into());
            } else if v.contains("small") {
                set(&mut r, "grain_size", "Small".into());
            }
        } else if has("white balance") || key == "wb" || has("wb shift") || key == "shift" {
            white_balance(&mut r, key, v, raw);
        } else if has("iso") || has("exposure") || has("ev") {
            r.notes.push(format!(
                "“{}” is how the photo is taken: set it on the camera (or adjust Exposure)",
                raw.trim()
            ));
        } else if has("sensor") || has("camera") || has("x-trans") || has("xtrans") {
            // Read above.
        } else {
            r.notes.push(format!("Ignored “{}”", raw.trim()));
        }
    }

    // The look, with a monochrome filter.
    let mut name = "Standard";
    if let Some((ours, needs, written)) = look {
        name = ours;
        if !r.sensor.at_least(needs) {
            r.notes.push(format!(
                "{} cameras don't have “{written}”; recreated it anyway",
                r.sensor.name()
            ));
        }
        if let Some(f) = filter {
            name = match (ours, f) {
                ("Mono", "Yellow") => "Mono Yellow",
                ("Mono", "Red") => "Mono Red",
                ("Mono", "Green") => "Mono Green",
                ("Fine Mono", "Yellow") => "Fine Mono Yellow",
                ("Fine Mono", "Red") => "Fine Mono Red",
                ("Fine Mono", "Green") => "Fine Mono Green",
                _ => ours,
            };
        }
    }
    r.params.insert("look".into(), name.into());
    if let Some(n) = r.name.as_mut() {
        *n = n
            .trim_matches(|c: char| c == '#' || c == '*' || c.is_whitespace())
            .to_string();
    }
    limits(&mut r);
    r
}

/// Highlight and shadow from a tone curve line: a label (`h`, `highlight`,
/// `s`, `shadow`) next to a number, before or after it.
fn tone_curve(v: &str) -> Vec<(&'static str, f64)> {
    let mut out = Vec::new();
    let (mut label, mut number): (Option<&'static str>, Option<f64>) = (None, None);
    for token in v.split(|c: char| c.is_whitespace() || ",&;:=".contains(c)) {
        let letters: String = token.chars().filter(|c| c.is_ascii_alphabetic()).collect();
        let l = match letters.as_str() {
            "h" | "hl" | "highlight" | "highlights" => Some("highlight"),
            "s" | "sh" | "shadow" | "shadows" => Some("shadow"),
            _ => None,
        };
        let n = numbers(token).first().copied();
        match (l, n) {
            (Some(l), Some(n)) => out.push((l, n)),
            (Some(l), None) => match number.take() {
                Some(n) => out.push((l, n)),
                None => label = Some(l),
            },
            (None, Some(n)) => match label.take() {
                Some(l) => out.push((l, n)),
                None => number = Some(n),
            },
            (None, None) => {}
        }
    }
    out
}

/// The sensor a line names ("X-Trans IV", "Sensor: X-Trans 4", "Camera: X-T3").
fn sensor_in(line: &str) -> Option<Sensor> {
    let line = line.to_lowercase();
    if let Some(i) = line
        .find("x-trans")
        .or_else(|| line.find("xtrans"))
        .or_else(|| line.find("x trans"))
    {
        let rest: String = line[i..].chars().take(12).collect();
        let generation: String = rest
            .trim_start_matches("x-trans")
            .trim_start_matches("xtrans")
            .trim_start_matches("x trans")
            .trim()
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect();
        return Sensor::from_name(&generation);
    }
    if line.contains("gfx") {
        return Some(Sensor::Gfx);
    }
    if line.contains("bayer") {
        return Some(Sensor::Bayer);
    }
    let (key, value) = line.split_once(':')?;
    if key.contains("camera") || key.contains("sensor") {
        return value.split([',', '/', '&']).find_map(Sensor::from_name);
    }
    None
}

/// A line that is only a film simulation's name (and a filter), not a title
/// that happens to hold one ("Soft Portrait").
fn bare_look(v: &str) -> bool {
    let words: String = v
        .chars()
        .filter(|c| c.is_ascii_alphabetic() || *c == ' ')
        .collect();
    LOOKS
        .iter()
        .flat_map(|(names, _, _)| names.iter())
        .any(|n| {
            let rest = words.trim().strip_prefix(n).map(str::trim);
            matches!(
                rest,
                Some("" | "ye" | "r" | "g" | "filter" | "ye filter" | "r filter" | "g filter")
            )
        })
}

fn look_for(v: &str) -> Option<(&'static str, Sensor)> {
    let v = v.replace(['.', '(', ')'], " ");
    let v = v.split_whitespace().collect::<Vec<_>>().join(" ");
    LOOKS.iter().find_map(|(names, ours, needs)| {
        names
            .iter()
            .any(|n| v.starts_with(n) || v.contains(&format!(" {n}")))
            .then_some((*ours, *needs))
    })
}

/// A monochrome filter: "+Ye", "+R", "+G", "Yellow filter" …
fn mono_filter(v: &str) -> Option<&'static str> {
    let v = v.replace(' ', "");
    if v.contains("+ye") || v.contains("yellow") {
        Some("Yellow")
    } else if v.contains("+r") || v.contains("red") {
        Some("Red")
    } else if v.contains("+g") || v.contains("green") {
        Some("Green")
    } else {
        None
    }
}

/// "Auto, +2 Red & -4 Blue", "Daylight, R2 B-4", "5200K, +1 Red & -6 Blue",
/// "WB Shift: R+2 B-4".
fn white_balance(r: &mut Recipe, key: &str, v: &str, raw: &str) {
    let (base, shift) = match v.split_once(',') {
        Some((b, s)) if !key.contains("shift") => (b.trim(), s),
        _ if key.contains("shift") => ("", v),
        _ => (v, ""),
    };
    let base_name = if base.is_empty() {
        None
    } else if base.contains("auto") || base.contains("awb") || base.contains("ambience") {
        Some("As shot")
    } else if base.contains("daylight") || base.contains("sunny") || base.contains("fine") {
        Some("Daylight")
    } else if base.contains("cloud") {
        Some("Cloudy")
    } else if base.contains("shade") {
        Some("Shade")
    } else if base.contains("incandescent") || base.contains("tungsten") {
        Some("Tungsten")
    } else if base.contains("fluorescent") || base.contains("fl") {
        Some(match numbers(base).first().map(|n| *n as i32) {
            Some(1) => "Fluorescent 1",
            Some(2) => "Fluorescent 2",
            Some(3) => "Fluorescent 3",
            _ => "Fluorescent",
        })
    } else if base.contains('k') || base.contains("kelvin") {
        match numbers(base).first() {
            Some(&k) => {
                r.params
                    .insert("kelvin".into(), k.clamp(2500.0, 10000.0).into());
                Some("Kelvin")
            }
            None => None,
        }
    } else if base.contains("underwater") {
        r.notes
            .push("Underwater white balance: used As shot".into());
        Some("As shot")
    } else {
        r.notes.push(format!(
            "Couldn't read the white balance in “{}”: used As shot",
            raw.trim()
        ));
        Some("As shot")
    };
    if let Some(b) = base_name {
        r.params.insert("white_balance".into(), b.into());
    }
    // The shift: "+2 Red & -4 Blue", "R:2 B:-4", "R+2 B-4".
    for part in shift.split(['&', ',', ';']).flat_map(|p| p.split(" and ")) {
        let p = part.trim();
        let Some(&n) = numbers(p).first() else {
            continue;
        };
        if p.contains("red") || p.starts_with('r') || p.ends_with('r') {
            r.params.insert("wb_red".into(), n.into());
        } else if p.contains("blue") || p.starts_with('b') || p.ends_with('b') {
            r.params.insert("wb_blue".into(), n.into());
        }
    }
    // "R+2 B-4" with no separator.
    if !shift.contains(['&', ',', ';']) && shift.contains('r') && shift.contains('b') {
        if let Some(i) = shift.find('b') {
            let (red, blue) = shift.split_at(i);
            if let Some(&n) = numbers(red).first() {
                r.params.insert("wb_red".into(), n.into());
            }
            if let Some(&n) = numbers(blue).first() {
                r.params.insert("wb_blue".into(), n.into());
            }
        }
    }
}

/// Keep each setting to what the sensor generation's cameras offer.
fn limits(r: &mut Recipe) {
    use Sensor::*;
    let s = r.sensor;
    let old = !s.at_least(XTrans3);
    let tone = if old { (-2.0, 2.0) } else { (-2.0, 4.0) };
    let half = s.at_least(XTrans4);
    let wide = if old { (-2.0, 2.0) } else { (-4.0, 4.0) };
    let ranges: [(&str, &str, (f64, f64), bool); 7] = [
        ("highlight", "Highlight", tone, half),
        ("shadow", "Shadow", tone, half),
        ("colour", "Color", wide, false),
        ("sharpness", "Sharpness", wide, false),
        ("noise_reduction", "Noise reduction", wide, false),
        ("clarity", "Clarity", (-5.0, 5.0), false),
        ("wb_red", "WB shift", (-9.0, 9.0), false),
    ];
    let mut ranges = ranges.to_vec();
    ranges.push(("wb_blue", "WB shift", (-9.0, 9.0), false));
    ranges.push(("tone_warm", "Monochromatic color", (-9.0, 9.0), false));
    ranges.push(("tone_magenta", "Monochromatic color", (-9.0, 9.0), false));
    for (key, label, (lo, hi), half) in ranges {
        let Some(v) = r.params.get(key).and_then(Value::as_f64) else {
            continue;
        };
        let step = if half { 0.5 } else { 1.0 };
        let fixed = ((v / step).round() * step).clamp(lo, hi);
        if (fixed - v).abs() > 1e-9 {
            r.notes.push(format!(
                "{label} {v:+} is outside what {} cameras offer: used {fixed:+}",
                s.name()
            ));
        }
        r.params.insert(key.into(), fixed.into());
    }
    let missing: &[(&str, &str, Sensor)] = &[
        ("grain", "Grain effect", XTrans3),
        ("grain_size", "Grain size", XTrans4),
        ("colour_depth", "Color chrome effect", XTrans4),
        ("blue_depth", "Color chrome FX blue", XTrans4),
        ("clarity", "Clarity", XTrans4),
        ("dr_priority", "D range priority", XTrans4),
        ("tone_warm", "Monochromatic color", XTrans4),
        ("tone_magenta", "Monochromatic color", XTrans4),
    ];
    for &(key, label, needs) in missing {
        let set = match r.params.get(key) {
            Some(Value::String(t)) => t != "Off" && t != "Small",
            Some(v) => v.as_f64().is_some_and(|n| n != 0.0),
            None => false,
        };
        if set && !s.at_least(needs) {
            r.notes.push(format!(
                "{} cameras have no {label}; recreated it anyway",
                s.name()
            ));
        }
    }
}

/// What a recipe becomes: the develop settings of a preset.
#[derive(Debug, Clone)]
pub struct Recreated {
    pub recipe: Recipe,
    pub settings: BTreeMap<String, amb_presets::Value>,
    /// The plugin's commands, to put the recipe on a photo.
    pub commands: Vec<amb_protocol::Command>,
    /// The plugin's summary ("Muted Chrome look, …").
    pub message: Option<String>,
}

/// Read a recipe and run it through the built-in Film recipe plugin on an
/// empty edit.
pub fn recreate(text: &str, sensor: Option<Sensor>) -> Result<Recreated, String> {
    let recipe = parse(text, sensor);
    let plugin = crate::builtin("film-recipe").ok_or("the Film recipe plugin is missing")?;
    let ctx = crate::Context {
        settings: Map::new(),
        photo: None,
        stats: None,
        seed: 0,
    };
    let out = plugin.run(&ctx, &recipe.params)?;
    let mut state = amb_protocol::DevelopState::new();
    state
        .execute_all(&out.commands)
        .map_err(|e| e.to_string())?;
    Ok(Recreated {
        settings: state.settings().clone(),
        message: out.message,
        commands: out.commands,
        recipe,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(r: &Recipe, k: &str) -> Value {
        r.params.get(k).cloned().unwrap_or(Value::Null)
    }

    // A recipe written for these tests, in the usual shared layout.
    const HARBOUR: &str = "Harbour Evening
X-Trans IV
Film Simulation: Classic Chrome
Dynamic Range: DR400
Highlight: −1.5
Shadow: +1
Color: +2
Noise Reduction: -4
Sharpening: -2
Clarity: -3
Grain Effect: Strong, Small
Color Chrome Effect: Strong
Color Chrome FX Blue: Weak
White Balance: Auto, +2 Red & -4 Blue
ISO: Auto, up to ISO 6400
Exposure Compensation: +1/3 to +2/3 (typically)";

    #[test]
    fn a_pasted_recipe_becomes_the_plugins_parameters() {
        let r = parse(HARBOUR, None);
        assert_eq!(r.name.as_deref(), Some("Harbour Evening"));
        assert_eq!(r.sensor, Sensor::XTrans4);
        assert_eq!(p(&r, "look"), "Muted Chrome");
        assert_eq!(p(&r, "dynamic_range"), "DR400");
        assert_eq!(p(&r, "highlight"), -1.5);
        assert_eq!(p(&r, "shadow"), 1.0);
        assert_eq!(p(&r, "colour"), 2.0);
        assert_eq!(p(&r, "noise_reduction"), -4.0);
        assert_eq!(p(&r, "sharpness"), -2.0);
        assert_eq!(p(&r, "clarity"), -3.0);
        assert_eq!(p(&r, "grain"), "Strong");
        assert_eq!(p(&r, "grain_size"), "Small");
        assert_eq!(p(&r, "colour_depth"), "Strong");
        assert_eq!(p(&r, "blue_depth"), "Weak");
        assert_eq!(p(&r, "white_balance"), "As shot");
        assert_eq!(
            (p(&r, "wb_red"), p(&r, "wb_blue")),
            (2.0.into(), (-4.0).into())
        );
        assert_eq!(r.notes.len(), 2, "{:?}", r.notes);
    }

    #[test]
    fn the_sensor_keeps_settings_to_its_cameras() {
        let r = parse(HARBOUR, Some(Sensor::XTrans5));
        assert_eq!(r.sensor, Sensor::XTrans4, "the text names one");
        let text = HARBOUR.replace("X-Trans IV\n", "");
        let r = parse(&text, Some(Sensor::XTrans3));
        assert_eq!(p(&r, "highlight"), -2.0, "whole steps on X-Trans III");
        assert!(
            r.notes.iter().any(|n| n.contains("no Clarity")),
            "{:?}",
            r.notes
        );
        let r = parse(&text, Some(Sensor::XTrans2));
        assert_eq!(p(&r, "colour"), 2.0);
        assert_eq!(p(&r, "noise_reduction"), -2.0);
        assert_eq!(Sensor::from_name("FUJIFILM X-T3"), Some(Sensor::XTrans4));
        assert_eq!(Sensor::from_name("x100vi"), Some(Sensor::XTrans5));
        assert_eq!(Sensor::from_name("X-Trans III"), Some(Sensor::XTrans3));
    }

    #[test]
    fn other_layouts_read_too() {
        let r = parse(
            "Acros +R\nTone Curve: H -1 S +2\nWhite Balance: 5200K, R:1 B:-6\nMonochromatic Color: WC +2 & MG -1\nGrain: Weak Large\nD Range Priority: Auto",
            Some(Sensor::XTrans5),
        );
        assert_eq!(p(&r, "look"), "Fine Mono Red");
        assert_eq!(
            (p(&r, "highlight"), p(&r, "shadow")),
            ((-1.0).into(), 2.0.into())
        );
        assert_eq!(p(&r, "white_balance"), "Kelvin");
        assert_eq!(p(&r, "kelvin"), 5200.0);
        assert_eq!(
            (p(&r, "wb_red"), p(&r, "wb_blue")),
            (1.0.into(), (-6.0).into())
        );
        assert_eq!(
            (p(&r, "tone_warm"), p(&r, "tone_magenta")),
            (2.0.into(), (-1.0).into())
        );
        assert_eq!(p(&r, "grain_size"), "Large");
        assert_eq!(p(&r, "dr_priority"), "Auto");
        let r = parse("Film Simulation: Monochrome+Ye\nWB Shift: R+3 B-5", None);
        assert_eq!(p(&r, "look"), "Mono Yellow");
        assert_eq!(
            (p(&r, "wb_red"), p(&r, "wb_blue")),
            (3.0.into(), (-5.0).into())
        );
        let r = parse(
            "Film Simulation: Classic Negative\nWhite Balance: Fluorescent 3, -1 Red & +2 Blue",
            None,
        );
        assert_eq!(p(&r, "look"), "Street Negative");
        assert_eq!(p(&r, "white_balance"), "Fluorescent 3");
        assert_eq!(p(&r, "wb_red"), -1.0);
    }

    #[test]
    fn a_recipe_is_recreated_as_settings() {
        let made = recreate(HARBOUR, None).unwrap();
        let num = |k: &str| made.settings.get(k).and_then(amb_presets::Value::as_f64);
        assert_eq!(num("amb:WBShiftRed"), Some(2.0));
        assert_eq!(num("GrainAmount"), Some(45.0));
        assert_eq!(num("Highlights2012"), Some(-40.0));
        assert_eq!(
            made.settings
                .get("WhiteBalance")
                .and_then(amb_presets::Value::as_str),
            Some("As Shot")
        );
    }
}
