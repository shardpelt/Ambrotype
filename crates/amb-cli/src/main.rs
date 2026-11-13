// SPDX-License-Identifier: GPL-3.0-or-later
//! `amb`: Ambrotype command-line tool.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

mod cleanup;
mod contact;
mod convert;
mod docs;
mod dust;
mod edit;
mod export_batch;
mod fuji;
mod lens;
mod library;
mod look;
mod man;
mod mcp;
mod models;
mod negative;
mod organise;
mod places;
mod plugin;
mod presets;
mod progress;
mod render;
mod search;
mod share;
mod sweep;
mod usage;

use amb_presets::Preset;
use amb_presets::registry::Registry;
use amb_presets::report::{self, Report};

use dust::*;
use edit::*;
use lens::*;
use library::*;
use look::*;
use models::*;
use negative::*;
use presets::*;
use render::*;
use sweep::*;

use usage::USAGE;

/// `amb denoise`: each raw through AI Denoise into a new DNG.
fn denoise(raws: &[&str], options: &BTreeMap<&str, &str>, json: bool) -> Result<(), String> {
    let out = options.get("--out").map(Path::new);
    let amount = match options.get("--amount") {
        Some(a) => a
            .parse::<f32>()
            .ok()
            .filter(|a| (0.0..=100.0).contains(a))
            .ok_or_else(|| format!("--amount {a}: 0 to 100"))?,
        None => amb_ops::denoise::AMOUNT,
    };
    if out.is_some() && raws.len() > 1 {
        return Err("--out is for one raw; with more, each goes beside its raw".into());
    }
    let mut reports = Vec::new();
    for raw in raws {
        let progress = crate::progress::line(json, |done, total| {
            format!("{raw}: {done} of {total} tiles")
        });
        let r = amb_ops::denoise::denoise(Path::new(raw), out, amount, &progress)?;
        if !json {
            println!(
                "{} ({}×{}, {:.0} s)",
                r.output.display(),
                r.width,
                r.height,
                r.seconds
            );
        }
        reports.push(r);
    }
    if json {
        return print_json(&serde_json::to_value(&reports).map_err(|e| e.to_string())?);
    }
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // Effect plugins in edits render as in the editor: built in, or from its
    // plugins folder.
    let plugins =
        amb_library::collections::editor_dir().and_then(|d| Some(d.parent()?.join("plugins")));
    if let Some(models) = amb_ai::default_dir() {
        amb_ai::hooks::install(&models);
    }
    // Lightroom catalogues opened in place: their photos' edits are read
    // from them, as in the editor.
    if let Some(file) = amb_library::lightroom::default_path().filter(|f| f.is_file()) {
        amb_library::lightroom::Catalogues::new(file).install();
    }
    amb_engine::plugin_effects::set_resolver(amb_plugin::effect::resolver(move |id| {
        amb_plugin::find(plugins.as_deref(), id)
    }));
    let mut json = false;
    let mut dry_run = false;
    let mut brief = false;
    let mut options: BTreeMap<&str, &str> = BTreeMap::new();
    let mut positional: Vec<&str> = Vec::new();
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                println!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            "--json" => json = true,
            "--dry-run" => dry_run = true,
            "--brief" => brief = true,
            "--progressive" | "--enlarge" | "--slides" | "--force" | "--no-subfolders"
            | "--no-align" | "--no-crop" | "--no-lens" | "--apply" | "--tone" | "--wb"
            | "--watermark-tile" | "--sigmoid" | "--softmax" | "--nhwc" | "--invert"
            | "--replace" | "--guess" | "--stack" | "--reject" | "--shuffle" | "--window"
            | "--subfolders" | "--no-scenes" | "--no-odd" | "--own-midtone" | "--landscape"
            | "--no-edits" | "--best-of-bursts" | "--no-captions" | "--trash" | "--recursive"
            | "--flat" | "--skip-existing" => {
                options.insert(arg.as_str(), "true");
            }
            "--out"
            | "--line"
            | "--baseline"
            | "--sensor"
            | "--groups"
            | "--synthetic"
            | "--preset"
            | "--amount"
            | "--look"
            | "--calibration"
            | "--commands"
            | "--queries"
            | "--camera-make"
            | "--camera-model"
            | "--lens-make"
            | "--lens-model"
            | "--focal"
            | "--aperture"
            | "--distance"
            | "--crop"
            | "--lens-db"
            | "--lcp-dir"
            | "--format"
            | "--quality"
            | "--bits"
            | "--color-space"
            | "--subsampling"
            | "--long-edge"
            | "--short-edge"
            | "--width"
            | "--height"
            | "--fit"
            | "--exact"
            | "--megapixels"
            | "--percent"
            | "--sharpen"
            | "--metadata"
            | "--max-size"
            | "--small-copy"
            | "--avif-speed"
            | "--border"
            | "--border-caption"
            | "--out-dir"
            | "--watermark"
            | "--watermark-at"
            | "--watermark-size"
            | "--watermark-opacity"
            | "--watermark-rotate"
            | "--rating"
            | "--keywords"
            | "--name"
            | "--params"
            | "--into"
            | "--rename"
            | "--start"
            | "--to"
            | "--from"
            | "--dcp-dir"
            | "--deghost"
            | "--projection"
            | "--parts"
            | "--remap"
            | "--backup"
            | "--size"
            | "--background"
            | "--gap"
            | "--margin"
            | "--collections"
            | "--camera"
            | "--lens"
            | "--keyword"
            | "--text"
            | "--until"
            | "--catalog"
            | "--like"
            | "--folders"
            | "--task"
            | "--channel"
            | "--mean"
            | "--std"
            | "--licence"
            | "--source"
            | "--label"
            | "--flag"
            | "--add"
            | "--remove"
            | "--title"
            | "--style"
            | "--columns"
            | "--paper"
            | "--caption"
            | "--creator"
            | "--copyright"
            | "--place"
            | "--rotate"
            | "--flip"
            | "--gpx"
            | "--offset"
            | "--alike"
            | "--burst"
            | "--by"
            | "--within"
            | "--skip"
            | "--clock"
            | "--seconds"
            | "--effect" => match iter.next() {
                Some(value) => {
                    options.insert(arg.as_str(), value.as_str());
                }
                None => {
                    eprintln!("error: {arg} needs a value\n\n{USAGE}");
                    return ExitCode::from(2);
                }
            },
            other if other.starts_with("--") => {
                eprintln!("error: unknown option {other}\n\n{USAGE}");
                return ExitCode::from(2);
            }
            other => positional.push(other),
        }
    }
    let groups = options
        .get("--groups")
        .map(|g| g.split(',').map(str::trim).collect::<Vec<_>>());
    let result = match positional.as_slice() {
        ["preset", "inspect", files @ ..] if !files.is_empty() => inspect(files, json),
        ["preset", "dump", file] => dump(file),
        ["preset", "census", paths @ ..] if !paths.is_empty() => census(paths, json),
        ["sweep", "plan"] => sweep_plan(groups.as_deref()),
        ["sweep", "generate", raws @ ..] => match options.get("--out") {
            Some(out) => sweep_generate(
                raws,
                options.get("--synthetic").copied(),
                Path::new(out),
                groups.as_deref(),
            ),
            None => Err("sweep generate needs --out <DIR>".into()),
        },
        ["testimage", pattern, out] => testimage(pattern, Path::new(out)),
        ["profile", "list", raw] => profile_list(Path::new(raw), &options),
        ["mcp"] => mcp::serve(),
        ["collage", "list"] => {
            for (i, t) in amb_export::collage::builtin_templates().iter().enumerate() {
                println!("{:>2}  {} ({} photos)", i + 1, t.name, t.cells.len());
            }
            Ok(())
        }
        ["collage", layout, photos @ ..] if !photos.is_empty() => match options.get("--out") {
            Some(out) => collage(layout, photos, Path::new(out), &options),
            None => Err("collage needs --out <FILE>".into()),
        },
        ["lrcat", "import", catalog] => lrcat_import(Path::new(catalog), &options, json),
        ["lrcat", "open", catalog] => lrcat_in_place("open", Some(Path::new(catalog)), json),
        ["lrcat", "close", catalog] => lrcat_in_place("close", Some(Path::new(catalog)), json),
        ["lrcat", "list"] | ["lrcat"] => lrcat_in_place("list", None, json),
        ["lrcat", "sample", out, photos @ ..] if !photos.is_empty() => {
            let photos: Vec<PathBuf> = photos.iter().map(PathBuf::from).collect();
            amb_library::lrcat::write_sample(Path::new(out), &photos)
                .map(|()| println!("wrote {out}: {} photo(s), rated 4, Red, picked, Exposure +1, in a collection \"Selects\"", photos.len()))
        }
        ["catalog", "missing"] => organise::missing(None, &options, json),
        ["catalog", "relink", dir] => organise::missing(Some(dir), &options, json),
        ["catalog", rest @ ..] => catalog(rest, &options, json),
        ["info", photos @ ..] if !photos.is_empty() => organise::info(photos, json),
        ["tag", photos @ ..] if !photos.is_empty() => {
            organise::tag(photos, &options, dry_run, json)
        }
        ["place", photos @ ..] if !photos.is_empty() => {
            places::place(photos, &options, dry_run, json)
        }
        ["insights", places @ ..] if !places.is_empty() => {
            library::insights(places, &options, json)
        }
        ["cleanup", photos @ ..] if !photos.is_empty() => {
            cleanup::cleanup(photos, &options, dry_run, json)
        }
        ["duplicates", photos @ ..] if !photos.is_empty() => {
            organise::duplicates(photos, &options, json)
        }
        [kind @ ("move" | "rename" | "sort"), photos @ ..] if !photos.is_empty() => {
            organise::relocate(kind, photos, &options, dry_run, json)
        }
        ["trash", photos @ ..] if !photos.is_empty() => organise::trash(photos, &options, dry_run),
        ["slideshow", photos @ ..] if !photos.is_empty() => {
            organise::slideshow(photos, &options, dry_run)
        }
        ["keywords", rest @ ..] => organise::keywords(rest, &options, dry_run, json),
        ["collection", rest @ ..] => organise::collection(rest, &options, json),
        ["denoise", raws @ ..] if !raws.is_empty() => denoise(raws, &options, json),
        ["merge", "hdr", raws @ ..] if raws.len() >= 2 => merge_hdr(raws, &options, json),
        ["merge", "pano", raws @ ..] if raws.len() >= 2 => merge_pano(raws, &options, json),
        ["import", source, dest] => {
            import(Path::new(source), Path::new(dest), &options, dry_run, json)
        }
        ["calibration", "defaults"] => {
            println!(
                "{}",
                amb_engine::calibration::Calibration::default().to_json()
            );
            Ok(())
        }
        ["render", inputs @ ..] if !inputs.is_empty() && options.contains_key("--out-dir") => {
            export_batch::render_dir(
                inputs,
                Path::new(options["--out-dir"]),
                &options,
                dry_run,
                json,
            )
        }
        ["render", input, out] => render(
            Path::new(input),
            Path::new(out),
            options.get("--preset").copied(),
            options.get("--amount").copied(),
            options.get("--look").copied(),
            options.get("--calibration").copied(),
            &options,
        ),
        ["edit", sidecar] => match options.get("--commands") {
            Some(commands) => edit(
                Path::new(sidecar),
                commands,
                options.get("--out").map(Path::new),
                dry_run,
                if json {
                    Output::Json
                } else if brief {
                    Output::Brief
                } else {
                    Output::Text
                },
            ),
            None => Err("edit needs --commands <file.json> (or - for stdin)".into()),
        },
        ["do", target, edits @ ..] if !edits.is_empty() => {
            do_edits(Path::new(target), &edits.join(" "), dry_run, json)
                .map(|text| println!("{text}"))
        }
        ["match", photo, refs @ ..] if !refs.is_empty() => match_look(
            Path::new(photo),
            refs,
            &options,
            options.contains_key("--apply"),
            json,
        )
        .map(|t| println!("{t}")),
        ["auto", photo] => auto(
            Path::new(photo),
            &options,
            options.contains_key("--apply"),
            json,
        )
        .map(|t| println!("{t}")),
        ["fuji", recipe] => fuji::fuji(recipe, None, &options, json).map(|t| println!("{t}")),
        ["fuji", recipe, photo] => {
            fuji::fuji(recipe, Some(Path::new(photo)), &options, json).map(|t| println!("{t}"))
        }
        ["replay", photo] => match options.get("--out") {
            Some(out) => (|| -> Result<(), String> {
                let mut o = amb_ops::replay::ReplayOptions::default();
                if let Some(v) = options.get("--long-edge") {
                    o.long_edge = v.parse().map_err(|_| format!("--long-edge {v}: pixels"))?;
                }
                o.captions = !options.contains_key("--no-captions");
                let r = amb_ops::replay::replay(
                    Path::new(photo),
                    Path::new(out),
                    &o,
                    amb_gpu::full::shared(),
                    &crate::progress::line(false, |n, total| format!("{n}/{total} frames")),
                )?;
                if json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&r).map_err(|e| e.to_string())?
                    );
                } else {
                    println!(
                        "wrote {out} ({} steps, {:.1} s, {}x{}, {:.1} MB)",
                        r.steps,
                        r.seconds,
                        r.width,
                        r.height,
                        r.bytes as f64 / 1e6
                    );
                }
                Ok(())
            })(),
            None => Err("replay needs --out FILE.mp4".into()),
        },
        ["convert", photos @ ..] if !photos.is_empty() => {
            convert::convert(photos, &options, json).map(|t| println!("{t}"))
        }
        ["contact", photos @ ..] if !photos.is_empty() => match options.get("--out") {
            Some(out) => {
                contact::contact(photos, Path::new(out), &options).map(|t| println!("{t}"))
            }
            None => Err("contact needs --out SHEET.jpg (or .png, .tif)".into()),
        },
        ["search", rest @ ..] => search::search(rest, &options, json).map(|t| println!("{t}")),
        ["share", photo] => match options.get("--out") {
            Some(out) => {
                share::share(Path::new(photo), Path::new(out), &options).map(|t| println!("{t}"))
            }
            None => Err("share needs --out PAGE.html".into()),
        },
        ["dust", photo] => dust(Path::new(photo), &options, json).map(|t| println!("{t}")),
        ["stats", photo] => stats(Path::new(photo), &options, json).map(|t| println!("{t}")),
        ["query", state] => match options.get("--queries") {
            Some(queries) => query(Path::new(state), queries),
            None => Err("query needs --queries <file.json> (or - for stdin)".into()),
        },
        ["negative", "analyze", raws @ ..] if !raws.is_empty() => negative_analyze(
            raws,
            options.get("--out").map(Path::new),
            options.contains_key("--slides"),
            json,
        ),
        ["negative", "sync", photos @ ..] if photos.len() >= 2 => {
            negative_sync(photos, &options, json)
        }
        ["negative", "stocks"] => {
            negative_stocks(json);
            Ok(())
        }
        ["lens", "info", raw] => lens_info(Path::new(raw), &options).map(|text| println!("{text}")),
        ["lens", "match"] => lens_match_output(&options).map(|text| println!("{text}")),
        ["lens", "fetch"] => lens_fetch(&options),
        ["models"] | ["models", "list"] => models_list(json),
        ["models", "install", id] => models_install(id),
        ["models", "use", id] => models_use(id),
        ["models", "remove", id] => models_remove(id),
        ["models", "add", file] => models_add(Path::new(file), &options),
        ["protocol", "schema"] => print_json(&amb_protocol::schema::protocol_schema()),
        ["protocol", "schema", "command"] => print_json(&amb_protocol::schema::command_schema()),
        ["protocol", "schema", "query"] => print_json(&amb_protocol::schema::query_schema()),
        ["plugin", "new", dir] => plugin::new(Path::new(dir), options.get("--name").copied()),
        ["plugin", "build", dir] => plugin::build(Path::new(dir)),
        ["plugin", "check", dir] => plugin::check(Path::new(dir)),
        ["plugin", "run", dir, target] => plugin::run(
            Path::new(dir),
            Path::new(target),
            options.get("--params").copied(),
            dry_run,
            json,
        )
        .map(|text| println!("{text}")),
        ["ref"] => {
            print!("{}", amb_protocol::card::core());
            Ok(())
        }
        ["ref", group] => match amb_protocol::card::group(group) {
            Some(page) => {
                print!("{page}");
                Ok(())
            }
            None => Err(format!(
                "no group {group:?}; groups: {}",
                amb_protocol::catalog::Catalog::builtin()
                    .groups()
                    .iter()
                    .map(|g| g.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
        },
        ["docs", names @ ..] => docs::docs(names, &options),
        ["man"] => {
            print!("{}", man::page(USAGE));
            Ok(())
        }
        ["settings", "reference"] => {
            if json {
                print_json(&amb_protocol::reference::json())
            } else {
                print!("{}", amb_protocol::reference::markdown());
                Ok(())
            }
        }
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("error: {msg}");
            ExitCode::FAILURE
        }
    }
}

fn print_json(value: &serde_json::Value) -> Result<(), String> {
    println!(
        "{}",
        serde_json::to_string_pretty(value).map_err(|e| e.to_string())?
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edit_command_end_to_end() {
        let dir = std::env::temp_dir().join(format!("amb-cli-edit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let sidecar = dir.join("photo.xmp");
        std::fs::write(
            &sidecar,
            r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description xmlns:xmp="http://ns.adobe.com/xap/1.0/" xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/" xmp:Rating="5" crs:Exposure2012="+0.30"/></rdf:RDF></x:xmpmeta>"#,
        )
        .unwrap();
        let cmds = r#"[{"cmd":"adjust_param","name":"Exposure2012","delta":0.2},
                       {"cmd":"set_param","name":"Vibrance","value":150}]"#;
        let text = edit_output(&sidecar, cmds, None, false, Output::Text).unwrap();
        assert!(text.contains("Exposure2012: +0.3 -> +0.5"), "{text}");
        assert!(text.contains("clamped: Vibrance = 150"), "{text}");
        let written = std::fs::read_to_string(&sidecar).unwrap();
        assert!(written.contains(r#"crs:Exposure2012="+0.5""#), "{written}");
        assert!(written.contains(r#"xmp:Rating="5""#), "{written}");

        let json = edit_output(
            &sidecar,
            r#"{"cmd":"reset_param","name":"Vibrance"}"#,
            None,
            true,
            Output::Json,
        )
        .unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["dry_run"], serde_json::json!(true));
        assert_eq!(
            v["results"][0]["diff"][0]["before"],
            serde_json::json!(100.0)
        );

        let err = edit_output(
            &sidecar,
            r#"{"cmd":"set_param","name":"Exposur","value":1}"#,
            None,
            false,
            Output::Text,
        )
        .unwrap_err();
        assert!(err.contains("did you mean Exposure2012"), "{err}");

        // `amb do` on the photo finds its sidecar, and reports briefly.
        let photo = dir.join("photo.nef");
        let text = do_edits(&photo, "exp+=0.25 vib=10", false, false).unwrap();
        assert_eq!(
            text,
            "Exposure2012 +0.5→+0.75\nVibrance +100→+10\nsaved photo.xmp"
        );
        let text = do_edits(&sidecar, "undo", true, false).unwrap();
        assert!(text.ends_with("dry run, not saved"), "{text}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn lens_match_command() {
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../amb-lens/tests/fixtures");
        let lens_db = fixtures.join("lensfun").display().to_string();
        let lcp_dir = fixtures.join("lcp").display().to_string();
        let empty_dir = fixtures.join("lensfun").display().to_string();
        let mut options: BTreeMap<&str, &str> = BTreeMap::from([
            ("--camera-make", "Acme"),
            ("--camera-model", "Mini 2"),
            ("--lens-make", "Acme"),
            ("--lens-model", "Zoomar 24-70mm F2.8 Pro | A"),
            ("--focal", "47"),
            ("--aperture", "4"),
            ("--lens-db", lens_db.as_str()),
            ("--lcp-dir", lcp_dir.as_str()),
        ]);
        let text = lens_match_output(&options).unwrap();
        assert!(
            text.contains("crop 1.5 (lensfun camera database)"),
            "{text}"
        );
        assert!(
            text.contains("profile: Lcp | LCP: Adobe (Acme Zoomar"),
            "{text}"
        );
        assert!(
            text.contains(
                "lensfun  Acme Zoomar 24-70mm f/2.8 Pro | A (score 1.000, calibration crop 1.5)"
            ),
            "{text}"
        );
        assert!(text.contains("distortion  acm k=["), "{text}");

        // Without LCP profiles the lensfun calibration for the APS-C body is used.
        options.insert("--lcp-dir", empty_dir.as_str());
        let text = lens_match_output(&options).unwrap();
        assert!(text.contains("profile: Lensfun | lensfun: Acme Zoomar 24-70mm f/2.8 Pro | A (calibrated at crop 1.5)"), "{text}");
        // 47 mm is halfway between a poly3 (24 mm) and a ptlens (70 mm) calibration.
        assert!(
            text.contains("distortion  ptlens a=0.001 b=0.002 c=0.003"),
            "{text}"
        );
        assert!(
            text.contains("tca         linear kr=1.002 kb=0.9985"),
            "{text}"
        );

        options.insert("--focal", "wide");
        assert!(
            lens_match_output(&options)
                .unwrap_err()
                .contains("bad --focal")
        );
        options.remove("--lens-model");
        assert!(
            lens_match_output(&options)
                .unwrap_err()
                .contains("--lens-model")
        );
    }
}
