// SPDX-License-Identifier: GPL-3.0-or-later
//! `amb negative`: film stocks and a negative's conversion.

use crate::*;

pub(crate) fn negative_stocks(json: bool) {
    use amb_engine::negative::STOCKS;
    use amb_presets::stocks::texture;
    if json {
        let list: Vec<serde_json::Value> = STOCKS
            .iter()
            .filter_map(|s| {
                let t = texture(s.id)?;
                Some(serde_json::json!({
                    "id": s.id,
                    "name": s.name,
                    "black_and_white": s.black_and_white,
                    "description": s.description,
                    "grain_amount": t.grain_amount,
                    "grain_size": t.grain_size,
                    "grain_roughness": t.grain_roughness,
                    "halation": t.halation,
                    "bloom": t.bloom,
                }))
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&list).expect("serialises")
        );
        return;
    }
    for s in STOCKS {
        let Some(t) = texture(s.id) else { continue };
        println!(
            "{:<22} {:<22} {}{}\n{:<45}grain {} (size {}, roughness {}), halation {}, bloom {}",
            s.id,
            s.name,
            if s.black_and_white { "[B&W] " } else { "" },
            s.description,
            "",
            t.grain_amount,
            t.grain_size,
            t.grain_roughness,
            t.halation,
            t.bloom
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
    let mut readouts: Vec<serde_json::Value> = Vec::new();
    for raw in raws {
        let img = amb_engine::raw::load(Path::new(raw)).map_err(|e| format!("{raw}: {e}"))?;
        let a = negative::analyze_as(&img.rgb, img.width, img.height, kind, &k);
        let whole = negative::Scan {
            rgb: &img.rgb,
            width: img.width,
            height: img.height,
        };
        let stats = negative::scan_stats(whole, &a);
        readouts.push(
            serde_json::json!({ "photo": raw, "range": stats.range, "clipped": stats.clipped }),
        );
        if !json {
            println!(
                "{raw}: base {:.4} {:.4} {:.4} | shadow density {:.3} {:.3} {:.3} | highlight density {:.3} {:.3} {:.3} | midtone {:.3} | range {:.2} {:.2} {:.2} D | clipped {:.1} %",
                a.base[0],
                a.base[1],
                a.base[2],
                a.density_low[0],
                a.density_low[1],
                a.density_low[2],
                a.density_high[0],
                a.density_high[1],
                a.density_high[2],
                a.midtone,
                stats.range[0],
                stats.range[1],
                stats.range[2],
                stats.clipped * 100.0
            );
        }
        frames.push(a);
    }
    let mut settings = amb_ops::negative::frozen(&frames);
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
        let mut map = map;
        map.insert("frames".into(), readouts.into());
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
    use amb_ops::negative::{RollOptions, analyze_files, commands, roll};
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
            let mut state =
                amb_protocol::DevelopState::load_or_new(&sidecar).map_err(|e| e.to_string())?;
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

/// `amb negative probe`: the range and clipping of a negative as its edit
/// converts it, and the density and zone at a point of the photo as shown
/// (`--at X,Y`, fractions from the top left).
pub(crate) fn negative_probe(
    photo: &Path,
    options: &BTreeMap<&str, &str>,
    json: bool,
) -> Result<String, String> {
    use amb_engine::raw;
    let at = parse_at(options.get("--at").ok_or("probe needs --at X,Y (0 to 1)")?)?;
    let source = amb_engine::source_path(photo);
    let img = raw::load(&source).map_err(|e| format!("{}: {e}", source.display()))?;
    let orientation = img.metadata.orientation;
    let mut img = raw::downscale(&img, PROBE_EDGE);
    let sidecar = amb_engine::sidecar_path(photo);
    let edit = options
        .get("--preset")
        .map(|p| p.to_string())
        .or_else(|| sidecar.is_file().then(|| sidecar.display().to_string()));
    let (develop, ..) = develop_settings(&mut img, edit.as_deref(), None, None, None, options)?;
    let stored = amb_export::geometry::stored_fraction(orientation, at[0], at[1]);
    let readout =
        amb_engine::pipeline::negative_readout(&img, &develop, Some([stored.0, stored.1]))
            .ok_or("The film negative conversion is off for this photo (amb:NegativeConvert).")?;
    if json {
        return serde_json::to_string_pretty(&readout).map_err(|e| e.to_string());
    }
    let (s, p) = (readout.stats, readout.probe.ok_or("no probe")?);
    Ok(format!(
        "range {:.2} {:.2} {:.2} D | clipped at scan {:.1} %\nat {:.3},{:.3}: zone {} | density {:.2} {:.2} {:.2}",
        s.range[0],
        s.range[1],
        s.range[2],
        s.clipped * 100.0,
        at[0],
        at[1],
        amb_engine::negative::ZONE_NAMES[usize::from(p.zone)],
        p.density[0],
        p.density[1],
        p.density[2]
    ))
}

/// The long edge a probe develops the scan at: the editor's preview size.
const PROBE_EDGE: usize = 1024;

/// `X,Y` as two fractions of the photo.
fn parse_at(text: &str) -> Result<[f64; 2], String> {
    let bad = || format!("--at wants X,Y as fractions of the photo (0 to 1), not {text:?}");
    let (x, y) = text.split_once(',').ok_or_else(bad)?;
    let x: f64 = x.trim().parse().map_err(|_| bad())?;
    let y: f64 = y.trim().parse().map_err(|_| bad())?;
    if !(0.0..=1.0).contains(&x) || !(0.0..=1.0).contains(&y) {
        return Err(bad());
    }
    Ok([x, y])
}
