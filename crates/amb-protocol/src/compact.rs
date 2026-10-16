// SPDX-License-Identifier: GPL-3.0-or-later
//! The compact edit syntax: one short line instead of a JSON array, for
//! scripts and AI clients (`amb do`, the MCP `edit_photo` tool's `do`).
//!
//! ```text
//! exp=+0.5 con+=10 hl=-40 temp=5600 reset:hsl preset:film.xmp@50 undo
//! ```
//!
//! Items are separated by spaces or `;`; double quotes keep spaces in a value.
//!
//! | Item | Command |
//! |---|---|
//! | `Name=v` | `set_param` (a number, `true`/`false`, or text) |
//! | `Name+=d`, `Name-=d` | `adjust_param` |
//! | `reset:Name`, `reset:group` | `reset_param`, `reset_group` |
//! | `preset:PATH[@amount]` | `apply_preset` |
//! | `sky:Name=v,Name=v` | `add_mask` sky with those sliders (`sky:exp=-0.5,sat=10`) |
//! | `subject:Name=v,…` | `add_mask` subject, the same way (`subject:exp=0.3`) |
//! | `snap:NAME`, `load:NAME` | `save_snapshot`, `apply_snapshot` |
//! | `undo`, `redo` | `undo`, `redo` |
//!
//! Names are the `crs:` names, any case, or the short [`ALIASES`].

use crate::catalog::Catalog;
use crate::{Command, Error};

/// Short names for the most used settings.
pub const ALIASES: &[(&str, &str)] = &[
    ("temp", "Temperature"),
    ("tint", "Tint"),
    ("exp", "Exposure2012"),
    ("con", "Contrast2012"),
    ("hl", "Highlights2012"),
    ("sh", "Shadows2012"),
    ("wh", "Whites2012"),
    ("bl", "Blacks2012"),
    ("tex", "Texture"),
    ("cla", "Clarity2012"),
    ("deh", "Dehaze"),
    ("vib", "Vibrance"),
    ("sat", "Saturation"),
    ("sharp", "Sharpness"),
    ("nr", "LuminanceSmoothing"),
    ("cnr", "ColorNoiseReduction"),
    ("vig", "PostCropVignetteAmount"),
    ("grain", "GrainAmount"),
];

/// The setting a compact name stands for: an alias, or a registry name in
/// any case. Unknown names come back as given, so the command reports them
/// with suggestions.
pub fn resolve(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    if let Some((_, full)) = ALIASES.iter().find(|(a, _)| *a == lower) {
        return full.to_string();
    }
    let c = Catalog::builtin();
    if c.lookup(name).is_some() {
        return name.to_string();
    }
    c.tags()
        .iter()
        .find(|t| t.name.eq_ignore_ascii_case(name))
        .map_or_else(|| name.to_string(), |t| t.name.clone())
}

/// Split on spaces and `;` outside double quotes; quotes are removed.
fn items(src: &str) -> Result<Vec<String>, Error> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut started = false;
    for ch in src.chars() {
        match ch {
            '"' => {
                quoted = !quoted;
                started = true;
            }
            c if !quoted && (c.is_whitespace() || c == ';') => {
                if started {
                    out.push(std::mem::take(&mut cur));
                    started = false;
                }
            }
            c => {
                cur.push(c);
                started = true;
            }
        }
    }
    if quoted {
        return Err(Error::Syntax("unclosed quote".into()));
    }
    if started {
        out.push(cur);
    }
    Ok(out)
}

fn number(item: &str, s: &str) -> Result<f64, Error> {
    s.trim_start_matches('+')
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .ok_or_else(|| Error::Syntax(format!("{item:?}: {s:?} is not a number")))
}

fn value(s: &str) -> serde_json::Value {
    match s {
        "true" => true.into(),
        "false" => false.into(),
        _ => match s.trim_start_matches('+').parse::<f64>() {
            Ok(v) if v.is_finite() => v.into(),
            _ => s.into(),
        },
    }
}

fn one(item: &str) -> Result<Command, Error> {
    match item {
        "undo" => return Ok(Command::Undo),
        "redo" => return Ok(Command::Redo),
        _ => {}
    }
    // `key:arg`, when the colon comes before any `=` (a value may hold one)
    // and names a command; otherwise it's part of a setting's name, as in
    // `amb:LUTAmount=50`.
    const KEYS: [&str; 6] = ["reset", "preset", "sky", "subject", "snap", "load"];
    let colon = item
        .find(':')
        .filter(|&c| item.find('=').is_none_or(|e| c < e))
        .filter(|&c| KEYS.contains(&&item[..c]) || !item.contains('='));
    if let Some(c) = colon {
        let (key, arg) = (&item[..c], &item[c + 1..]);
        if arg.is_empty() {
            return Err(Error::Syntax(format!("{item:?}: nothing after the colon")));
        }
        return match key {
            "reset" => {
                let c = Catalog::builtin();
                if c.group(arg).is_some() {
                    Ok(Command::ResetGroup { group: arg.into() })
                } else {
                    Ok(Command::ResetParam { name: resolve(arg) })
                }
            }
            "preset" => {
                let (path, amount) = match arg.rsplit_once('@') {
                    Some((p, a)) => (p, number(item, a)?),
                    None => (arg, 100.0),
                };
                Ok(Command::ApplyPreset {
                    path: path.into(),
                    amount,
                    include_groups: None,
                    exclude_groups: None,
                })
            }
            // A new mask with its own sliders: `sky:exp=-0.5,sat=10`.
            "sky" | "subject" => {
                let mut adjust = serde_json::Map::new();
                for pair in arg.split(',') {
                    let Some((name, v)) = pair.split_once('=') else {
                        return Err(Error::Syntax(format!(
                            "{item:?}: expected {key}:Name=v,Name=v (a new mask's sliders)"
                        )));
                    };
                    let global = resolve(name);
                    let key = if global.starts_with("Local") {
                        global
                    } else {
                        format!("Local{global}")
                    };
                    adjust.insert(key, serde_json::json!(number(item, v)?));
                }
                Ok(Command::AddMask {
                    kind: key.into(),
                    mask: None,
                    mode: None,
                    values: None,
                    adjust: Some(adjust),
                })
            }
            "snap" => Ok(Command::SaveSnapshot { name: arg.into() }),
            "load" => Ok(Command::ApplySnapshot { name: arg.into() }),
            _ => Err(Error::Syntax(format!(
                "{item:?}: unknown {key:?}; use reset:, preset:, sky:, subject:, snap: or load:"
            ))),
        };
    }
    for (op, sign) in [("+=", 1.0), ("-=", -1.0)] {
        if let Some((name, d)) = item.split_once(op) {
            return Ok(Command::AdjustParam {
                name: resolve(name),
                delta: sign * number(item, d)?,
            });
        }
    }
    if let Some((name, v)) = item.split_once('=') {
        if name.is_empty() {
            return Err(Error::Syntax(format!("{item:?}: no setting name")));
        }
        return Ok(Command::SetParam {
            name: resolve(name),
            value: value(v),
        });
    }
    Err(Error::Syntax(format!(
        "{item:?}: expected Name=v, Name+=d, Name-=d, reset:X, preset:PATH[@amount], sky:Name=v,…, snap:NAME, load:NAME, undo or redo"
    )))
}

/// The commands a compact line stands for, in order.
pub fn parse(src: &str) -> Result<Vec<Command>, Error> {
    let items = items(src)?;
    if items.is_empty() {
        return Err(Error::Syntax("no edits given".into()));
    }
    items.iter().map(|i| one(i)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_form() {
        let got = parse(
            r#"exp=+0.5 con+=10; HL-=5 texture=12 reset:hsl reset:sat preset:"my looks/film.xmp"@50 snap:a load:a undo redo ProcessVersion=11.0 Name="a b" CameraProfile="Camera: Vivid""#,
        )
        .unwrap();
        let json = serde_json::to_value(&got).unwrap();
        assert_eq!(
            json,
            serde_json::json!([
                {"cmd": "set_param", "name": "Exposure2012", "value": 0.5},
                {"cmd": "adjust_param", "name": "Contrast2012", "delta": 10.0},
                {"cmd": "adjust_param", "name": "Highlights2012", "delta": -5.0},
                {"cmd": "set_param", "name": "Texture", "value": 12.0},
                {"cmd": "reset_group", "group": "hsl"},
                {"cmd": "reset_param", "name": "Saturation"},
                {"cmd": "apply_preset", "path": "my looks/film.xmp", "amount": 50.0},
                {"cmd": "save_snapshot", "name": "a"},
                {"cmd": "apply_snapshot", "name": "a"},
                {"cmd": "undo"},
                {"cmd": "redo"},
                {"cmd": "set_param", "name": "ProcessVersion", "value": 11.0},
                {"cmd": "set_param", "name": "Name", "value": "a b"},
                {"cmd": "set_param", "name": "CameraProfile", "value": "Camera: Vivid"},
            ])
        );
    }

    #[test]
    fn a_sky_item_makes_a_sky_mask_with_its_sliders() {
        let got = parse("sky:exp=-0.5,sat=10").unwrap();
        assert_eq!(
            serde_json::to_value(&got).unwrap(),
            serde_json::json!([{"cmd": "add_mask", "kind": "sky",
                "adjust": {"LocalExposure2012": -0.5, "LocalSaturation": 10.0}}])
        );
        let mut state = crate::state::DevelopState::new();
        state.execute_all(&got).unwrap();
        let d = crate::masks::describe(state.settings());
        assert_eq!(d["masks"][0]["parts"][0]["kind"], "ai");
        assert_eq!(d["masks"][0]["sliders"]["LocalExposure2012"], -0.5, "{d}");
        assert!(parse("sky:exp").is_err());
    }

    #[test]
    fn names_resolve_by_alias_and_any_case() {
        assert_eq!(resolve("exp"), "Exposure2012");
        assert_eq!(resolve("EXPOSURE2012"), "Exposure2012");
        assert_eq!(resolve("vibrance"), "Vibrance");
        assert_eq!(resolve("NoSuchThing"), "NoSuchThing");
        for (_, full) in ALIASES {
            assert!(Catalog::builtin().lookup(full).is_some(), "{full}");
        }
    }

    #[test]
    fn settings_with_a_colon_in_their_name() {
        assert_eq!(
            parse("amb:LUTAmount=50 amb:LUTAmount+=10").unwrap(),
            vec![
                Command::SetParam {
                    name: "amb:LUTAmount".into(),
                    value: serde_json::json!(50.0),
                },
                Command::AdjustParam {
                    name: "amb:LUTAmount".into(),
                    delta: 10.0,
                },
            ]
        );
        assert!(parse("amb:LUTAmount").is_err());
    }

    #[test]
    fn errors_say_what_was_expected() {
        for bad in ["", "exp", "exp+=x", "foo:bar", "=3", "reset:", "a=\"b"] {
            let e = parse(bad).unwrap_err().to_string();
            assert!(!e.is_empty(), "{bad}");
        }
        assert!(
            parse("exp+=x")
                .unwrap_err()
                .to_string()
                .contains("not a number")
        );
    }
}
