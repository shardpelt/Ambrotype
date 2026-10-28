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
