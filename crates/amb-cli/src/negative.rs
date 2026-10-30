// SPDX-License-Identifier: GPL-3.0-or-later
//! `amb negative`: film stocks and a negative's conversion.

use crate::*;

pub(crate) fn negative_stocks(json: bool) {
    use amb_engine::negative::STOCKS;
    if json {
        let list: Vec<serde_json::Value> = STOCKS
            .iter()
            .map(|s| {
                serde_json::json!({
                    "id": s.id,
                    "name": s.name,
                    "black_and_white": s.black_and_white,
                    "description": s.description,
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&list).expect("serialises")
        );
        return;
    }
    for s in STOCKS {
        println!(
            "{:<22} {:<22} {}{}",
            s.id,
            s.name,
            if s.black_and_white { "[B&W] " } else { "" },
            s.description
        );
    }
    println!(
        "\nUse with: amb:NegativeConvert=True, amb:FilmStock=<id> (e.g. in a preset or `amb edit`)."
    );
}

/// `amb negative analyze`: per-frame analysis and the roll median, as `amb:` settings.
pub(crate) fn negative_analyze(
    raws: &[&str],
    out: Option<&Path>,
    slides: bool,
    json: bool,
) -> Result<(), String> {
    use amb_engine::negative::{self, Analysis, NegativeType};
    let kind = if slides {
        NegativeType::Positive
    } else {
        NegativeType::Color
    };
    let k = amb_engine::negative::DEFAULT_STRENGTHS;
    let mut frames: Vec<Analysis> = Vec::new();
    for raw in raws {
        let img = amb_engine::raw::load(Path::new(raw)).map_err(|e| format!("{raw}: {e}"))?;
        let a = negative::analyze_as(&img.rgb, img.width, img.height, kind, &k);
        if !json {
            println!(
                "{raw}: base {:.4} {:.4} {:.4} | shadow density {:.3} {:.3} {:.3} | highlight density {:.3} {:.3} {:.3} | midtone {:.3}",
                a.base[0],
                a.base[1],
                a.base[2],
                a.density_low[0],
                a.density_low[1],
                a.density_low[2],
                a.density_high[0],
                a.density_high[1],
                a.density_high[2],
                a.midtone
            );
        }
        frames.push(a);
    }
    let mut settings = amb_library::negative::frozen(&frames);
    if slides {
        settings.insert(
            "amb:NegativeType".into(),
            amb_presets::Value::Text("positive".into()),
        );
    }
    if json {
        let map: serde_json::Map<String, serde_json::Value> = settings
            .iter()
            .map(|(k, v)| {
                let value = match v {
                    amb_presets::Value::Bool(b) => serde_json::Value::Bool(*b),
                    amb_presets::Value::Text(t) => serde_json::json!(t),
                    other => serde_json::json!(other.as_f64()),
                };
                (k.clone(), value)
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&map).expect("serialises")
        );
    } else if frames.len() > 1 {
        println!("roll median over {} frames", frames.len());
    }
    if let Some(out) = out {
        settings.insert(
            "Name".into(),
            amb_presets::Value::Alt(vec![("x-default".into(), "Negative roll".into())]),
        );
        let packet = amb_presets::xmp_writer::write_packet(&settings, amb_presets::is_signed_tag);
        std::fs::write(out, packet).map_err(|e| format!("{}: {e}", out.display()))?;
        if !json {
            println!(
                "wrote {} (apply with `amb render <RAW> <OUT> --preset {}`; add amb:FilmStock to pick a look)",
                out.display(),
                out.display()
            );
        }
    }
    Ok(())
}

/// `amb negative sync`: one conversion for a roll (roll baseline): each
/// scene's median, odd frames their own; `--apply` writes every sidecar.
pub(crate) fn negative_sync(
    photos: &[&str],
    options: &BTreeMap<&str, &str>,
    json: bool,
) -> Result<(), String> {
    use amb_library::negative::{RollOptions, analyze_files, commands, roll};
    let paths: Vec<&Path> = photos.iter().map(Path::new).collect();
    let baseline = match options.get("--baseline") {
        Some(b) => Some(
            paths
                .iter()
                .position(|p| *p == Path::new(b))
                .ok_or_else(|| format!("--baseline {b} is not one of the photos"))?,
        ),
        None => None,
    };
    let o = RollOptions {
        scenes: !options.contains_key("--no-scenes"),
        odd: !options.contains_key("--no-odd"),
        own_midtone: options.contains_key("--own-midtone"),
        baseline,
    };
    let frames = analyze_files(&paths)?;
    let plan = roll(&frames, &o);
    let apply = options.contains_key("--apply");
    let mut written = Vec::new();
    if apply {
        for (photo, frame) in paths.iter().zip(&plan) {
            let sidecar = crate::edit::edit_target(photo);
            let mut state = if sidecar.exists() {
                amb_protocol::DevelopState::load(&sidecar).map_err(|e| e.to_string())?
            } else {
                amb_protocol::DevelopState::new()
            };
            state
                .execute_grouped("Sync negative roll", &commands(&frame.settings))
                .map_err(|e| e.to_string())?;
            state.save(&sidecar).map_err(|e| e.to_string())?;
            written.push(sidecar.display().to_string());
        }
    }
    if json {
        let frames: Vec<serde_json::Value> = paths
            .iter()
            .zip(&plan)
            .map(|(p, f)| {
                serde_json::json!({
                    "photo": p.display().to_string(),
                    "scene": f.scene,
                    "odd": f.odd,
                    "settings": f.settings,
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(
                &serde_json::json!({ "frames": frames, "written": written })
            )
            .map_err(|e| e.to_string())?
        );
        return Ok(());
    }
    for (p, f) in paths.iter().zip(&plan) {
        println!(
            "{}: scene {}{}",
            p.display(),
            f.scene + 1,
            if f.odd { ", odd: keeps its own" } else { "" }
        );
    }
    println!(
        "{}",
        if apply {
            format!("wrote {} sidecar(s)", written.len())
        } else {
            "not applied (add --apply)".into()
        }
    );
    Ok(())
}
