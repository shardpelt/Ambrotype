// SPDX-License-Identifier: GPL-3.0-or-later
//! `amb plugin`: make, build, check and run plugins (docs/ai/plugins.md).

use std::path::{Path, PathBuf};

use amb_plugin::{Context, ParamKind, Plugin};
use serde_json::{Map, Value};

const SDK_SOURCE: &str = include_str!("../../amb-plugin-sdk/src/lib.rs");

/// `amb plugin new <DIR> [--name "My plugin"]`: a plugin project to start from.
pub fn new(dir: &Path, name: Option<&str>) -> Result<(), String> {
    if dir.exists()
        && std::fs::read_dir(dir)
            .map_err(|e| e.to_string())?
            .next()
            .is_some()
    {
        return Err(format!("{} already exists and isn't empty", dir.display()));
    }
    let folder = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let id: String = folder
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    if id.is_empty() {
        return Err("give the plugin folder a name, e.g. amb plugin new warm-it-up".into());
    }
    let name = name.map(str::to_string).unwrap_or_else(|| {
        let mut n = id.replace('-', " ");
        if let Some(first) = n.get(..1) {
            n.replace_range(..1, &first.to_uppercase());
        }
        n
    });
    let manifest = serde_json::json!({
        "format": amb_plugin::FORMAT,
        "id": id,
        "name": name,
        "version": "0.1.0",
        "author": "",
        "description": "What it does, in one or two sentences.",
        "permissions": [],
        "params": [
            { "name": "amount", "label": "Amount", "type": "number", "min": 0, "max": 100, "default": 50, "step": 1 }
        ]
    });
    let files: [(&str, String); 6] = [
        (
            "plugin.json",
            serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())? + "\n",
        ),
        (
            "Cargo.toml",
            format!(
                r#"[package]
name = "{id}"
version = "0.1.0"
edition = "2024"
# Your plugin, your licence: the SDK and this template are MIT OR Apache-2.0.
# license = "MIT OR Apache-2.0"

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
amb-plugin-sdk = {{ path = "sdk" }}
serde_json = "1"

# `amb plugin build` builds with this profile: a small module.
[profile.plugin]
inherits = "release"
opt-level = "s"
lto = true
codegen-units = 1
strip = true
panic = "abort"

# A project of its own, even inside another Cargo workspace.
[workspace]
"#
            ),
        ),
        (
            "src/lib.rs",
            r#"//! An Ambrotype plugin. Read docs/ai/plugins.md in the Ambrotype repo.
//! Build: `amb plugin build .`  Try it: `amb plugin check .`  Test: `cargo test`.

use amb_plugin_sdk::{Input, Output};

pub fn run(input: Input) -> Result<Output, String> {
    let amount = input.number("amount");
    Ok(Output::new()
        .adjust("Vibrance", amount / 2.0)
        .adjust("Clarity2012", amount / 5.0)
        .message(format!("Vibrance +{}, Clarity +{}", amount / 2.0, amount / 5.0)))
}

amb_plugin_sdk::export_plugin!(run);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds_vibrance() {
        let out = run(Input::for_test(serde_json::json!({ "amount": 50 }))).unwrap();
        assert_eq!(out.commands[0]["name"], "Vibrance");
    }
}
"#
            .to_string(),
        ),
        (
            "sdk/Cargo.toml",
            r#"[package]
name = "amb-plugin-sdk"
version = "0.1.0"
edition = "2024"
description = "A copy of Ambrotype's plugin SDK (crates/amb-plugin-sdk)"
license = "MIT OR Apache-2.0"

[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
"#
            .to_string(),
        ),
        ("sdk/src/lib.rs", SDK_SOURCE.to_string()),
        (".gitignore", "target/\n".to_string()),
    ];
    for (rel, text) in files {
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    println!(
        "Made {}: edit src/lib.rs and plugin.json, then `amb plugin build {}`.",
        dir.display(),
        dir.display()
    );
    Ok(())
}

/// `amb plugin build <DIR>`: compile to WebAssembly and put the module next
/// to plugin.json, then check it.
pub fn build(dir: &Path) -> Result<(), String> {
    let manifest_text = std::fs::read_to_string(dir.join("plugin.json"))
        .map_err(|e| format!("{}: {e}", dir.join("plugin.json").display()))?;
    let manifest = amb_plugin::Manifest::from_json(&manifest_text)?;
    let cargo_toml = dir.join("Cargo.toml");
    let wanted =
        std::fs::canonicalize(&cargo_toml).map_err(|e| format!("{}: {e}", cargo_toml.display()))?;
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let out = std::process::Command::new(cargo)
        .args([
            "build",
            "--profile",
            "plugin",
            "--target",
            "wasm32-unknown-unknown",
        ])
        .args([
            "--message-format",
            "json-render-diagnostics",
            "--manifest-path",
        ])
        .arg(&cargo_toml)
        .stderr(std::process::Stdio::inherit())
        .output()
        .map_err(|e| format!("couldn't run cargo: {e} (install Rust from https://rustup.rs)"))?;
    if !out.status.success() {
        return Err(
            "the build failed (see above). If it says the target isn't installed: \
                    rustup target add wasm32-unknown-unknown"
                .into(),
        );
    }
    let built = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter(|m| m["reason"] == "compiler-artifact")
        .filter(|m| {
            m["manifest_path"]
                .as_str()
                .and_then(|p| std::fs::canonicalize(p).ok())
                .is_some_and(|p| p == wanted)
        })
        .flat_map(|m| m["filenames"].as_array().cloned().unwrap_or_default())
        .filter_map(|f| f.as_str().map(PathBuf::from))
        .find(|f| f.extension().is_some_and(|e| e == "wasm"))
        .ok_or("cargo built no .wasm (is crate-type = [\"cdylib\", ...] in Cargo.toml?)")?;
    let dest = dir.join(&manifest.wasm);
    std::fs::copy(&built, &dest).map_err(|e| format!("{}: {e}", dest.display()))?;
    let size = std::fs::metadata(&dest).map(|m| m.len()).unwrap_or(0);
    println!("Built {} ({} KB)", dest.display(), size / 1000);
    check(dir)
}

/// A photo for `check`: a mid-grey ramp, as a plugin with every permission sees it.
fn sample_context() -> Context {
    let rgb: Vec<u8> = (0..64u32 * 48)
        .flat_map(|i| {
            let v = (40 + (i % 64) * 2) as u8;
            [v, v, v]
        })
        .collect();
    Context {
        settings: Map::new(),
        photo: Some(
            serde_json::json!({ "camera": "Sample Camera", "lens": "50 mm", "width": 6000,
            "height": 4000, "iso": 400, "shutter": 0.008, "aperture": 2.8, "focal_length": 50.0 }),
        ),
        stats: Some(amb_plugin::Stats::from_rgb8(&rgb, 64, 48)),
        seed: 1,
    }
}

/// `amb plugin check <DIR>`: validate the manifest and module, and run it
/// once on a sample photo.
pub fn check(dir: &Path) -> Result<(), String> {
    let plugin = Plugin::load(dir)?;
    let m = &plugin.manifest;
    println!("{} {} ({}): ok", m.name, m.version, m.id);
    if !m.description.is_empty() {
        println!("  {}", m.description);
    }
    if m.permissions.is_empty() {
        println!("  permissions: none");
    }
    for p in &m.permissions {
        println!("  permission {}: {}", p.id(), p.describe());
    }
    for p in &m.params {
        let kind = match &p.kind {
            ParamKind::Number {
                min, max, default, ..
            } => format!("number {min} to {max}, default {default}"),
            ParamKind::Choice { options, default } => {
                format!("one of {}, default {default}", options.join(" / "))
            }
            ParamKind::Bool { default } => format!("on/off, default {default}"),
        };
        println!("  param {} ({}): {kind}", p.name, p.label);
    }
    if m.is_effect() {
        let start = std::time::Instant::now();
        if m.effect == "color" {
            plugin.color_table(&Map::new(), 1)?;
            println!(
                "  sample run: a {}³ colour table in {} ms",
                amb_plugin::effect::GRID,
                start.elapsed().as_millis()
            );
        } else {
            let (w, h) = (640, 480);
            let mut rgb: Vec<f32> = (0..w * h)
                .flat_map(|i| {
                    let v = (i % w) as f32 / w as f32;
                    [v, v, v]
                })
                .collect();
            let region = amb_plugin::effect::Region {
                x: 0,
                y: 0,
                full_width: w,
                full_height: h,
            };
            plugin.apply_pixels(&Map::new(), 1, &mut rgb, w, h, region)?;
            let ms = start.elapsed().as_millis();
            println!(
                "  sample run: {w}×{h} pixels in {ms} ms (a 24 MP export: about {} s)",
                (ms as f64 * 24e6 / (w * h) as f64 / 1000.0).ceil()
            );
        }
        return Ok(());
    }
    let out = plugin.run(&sample_context(), &Map::new())?;
    println!(
        "  sample run: {} edit(s) in {:.1} M steps{}",
        out.commands.len(),
        out.fuel_used as f64 / 1e6,
        out.message.map(|m| format!(": {m}")).unwrap_or_default()
    );
    for c in &out.commands {
        println!("    {}", c.label());
    }
    for line in &out.log {
        println!("  log: {line}");
    }
    Ok(())
}

/// `--params "a=1,b=Film"` as JSON values (strings; the manifest types them).
fn parse_params(text: Option<&str>) -> Result<Map<String, Value>, String> {
    let mut out = Map::new();
    for pair in text
        .unwrap_or("")
        .split(',')
        .map(str::trim)
        .filter(|p| !p.is_empty())
    {
        let (k, v) = pair
            .split_once('=')
            .ok_or(format!("bad --params entry {pair:?} (use name=value)"))?;
        out.insert(k.trim().to_string(), Value::from(v.trim()));
    }
    Ok(out)
}

/// `amb plugin run <DIR> <PHOTO|SIDECAR.xmp> [--params a=1,b=x] [--dry-run] [--json]`:
/// run a plugin on a photo's edit and write the result to its sidecar.
pub fn run(
    dir: &Path,
    target: &Path,
    params: Option<&str>,
    dry_run: bool,
    json: bool,
) -> Result<String, String> {
    let plugin = Plugin::load(dir)?;
    let is_sidecar = target
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("xmp"));
    let sidecar = if is_sidecar {
        target.to_path_buf()
    } else {
        amb_engine::sidecar_path(target)
    };
    let state = if sidecar.exists() {
        amb_protocol::DevelopState::load(&sidecar)
            .map_err(|e| format!("{}: {e}", sidecar.display()))?
    } else {
        amb_protocol::DevelopState::new()
    };
    if plugin.manifest.is_effect() {
        // An effect isn't run once: it is added to the edit and rendered with it.
        let params = plugin.manifest.resolve_params(&parse_params(params)?)?;
        let mut effects = state
            .settings()
            .get("amb:Effects")
            .and_then(|v| v.as_str().map(str::to_string))
            .map(|t| amb_engine::plugin_effects::parse(&t))
            .transpose()?
            .unwrap_or_default();
        effects.push(amb_engine::plugin_effects::EffectEntry {
            plugin: plugin.manifest.id.clone(),
            params,
            amount: 100.0,
            seed: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64 & 0xffff_ffff)
                .unwrap_or(1),
        });
        let value = serde_json::to_string(&effects).map_err(|e| e.to_string())?;
        let commands = serde_json::to_string(&serde_json::json!([
            { "cmd": "set_param", "name": "amb:Effects", "value": value }
        ]))
        .map_err(|e| e.to_string())?;
        let report = amb_protocol::edit::run(&amb_protocol::edit::EditRequest {
            sidecar: &sidecar,
            commands: &commands,
            out: None,
            dry_run,
        })
        .map_err(|e| e.to_string())?;
        return if json {
            serde_json::to_string_pretty(&report.to_json()).map_err(|e| e.to_string())
        } else {
            Ok(format!(
                "Added the effect {} to {}{}",
                plugin.manifest.name,
                sidecar.display(),
                if dry_run {
                    " (dry run: nothing written)"
                } else {
                    ""
                }
            ))
        };
    }
    let mut ctx = Context {
        settings: Context::settings_json(state.settings()),
        photo: None,
        stats: None,
        seed: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(1),
    };
    if !plugin.manifest.permissions.is_empty() && !is_sidecar {
        let img =
            amb_engine::raw::load(target).map_err(|e| format!("{}: {e}", target.display()))?;
        ctx.photo = Some(amb_plugin::photo::info(&img));
        ctx.stats = Some(amb_plugin::photo::stats(&img, state.settings()));
    }
    let out = plugin.run(&ctx, &parse_params(params)?)?;
    for line in &out.log {
        eprintln!("log: {line}");
    }
    let commands = serde_json::to_string(&out.commands).map_err(|e| e.to_string())?;
    let report = amb_protocol::edit::run(&amb_protocol::edit::EditRequest {
        sidecar: &sidecar,
        commands: &commands,
        out: None,
        dry_run,
    })
    .map_err(|e| e.to_string())?;
    if json {
        let mut v = report.to_json();
        v["message"] = Value::from(out.message);
        serde_json::to_string_pretty(&v).map_err(|e| e.to_string())
    } else {
        let mut text = out
            .message
            .map(|m| format!("{}: {m}\n", plugin.manifest.name))
            .unwrap_or_default();
        text.push_str(report.to_text().trim_end());
        Ok(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runs_a_plugin_on_a_sidecar() {
        let dir = std::env::temp_dir().join(format!("amb-cli-plugin-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let sidecar = dir.join("photo.xmp");
        let plugin = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins/random-look");
        let text = run(
            &plugin,
            &sidecar,
            Some("style=Moody,strength=80"),
            false,
            false,
        )
        .unwrap();
        assert!(text.starts_with("Random look: Moody look"), "{text}");
        let state = amb_protocol::DevelopState::load(&sidecar).unwrap();
        assert!(state.settings().contains_key("Clarity2012"));
        // Unknown parameters are refused.
        assert!(run(&plugin, &sidecar, Some("nope=1"), true, false).is_err());
        check(&plugin).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn new_makes_a_project() {
        let dir = std::env::temp_dir()
            .join(format!("amb-cli-new-{}", std::process::id()))
            .join("warm-it-up");
        let _ = std::fs::remove_dir_all(&dir);
        new(&dir, None).unwrap();
        let m = amb_plugin::Manifest::from_json(
            &std::fs::read_to_string(dir.join("plugin.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            (m.id.as_str(), m.name.as_str()),
            ("warm-it-up", "Warm it up")
        );
        assert!(dir.join("sdk/src/lib.rs").exists());
        assert!(
            new(&dir, None).is_err(),
            "refuses a folder that isn't empty"
        );
        let _ = std::fs::remove_dir_all(dir.parent().unwrap());
    }
}
