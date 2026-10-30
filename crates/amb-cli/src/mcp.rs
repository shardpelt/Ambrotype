// SPDX-License-Identifier: GPL-3.0-or-later
//! `amb mcp`: a Model Context Protocol server over stdio, so the user's own
//! AI app (Claude Desktop, Claude Code, or any MCP client) can drive
//! Ambrotype with the same commands as the editor (docs/ai/mcp.md).
//!
//! JSON-RPC 2.0, one message per line on stdin and stdout. Nothing else is
//! written to stdout: each tool call is logged to stderr, where MCP clients
//! show server logs, and renders run as a child `amb render` so its report
//! can't mix into the protocol.
//!
//! Every edit is an `amb-protocol` command on the photo's XMP sidecar, the
//! file Lightroom and the editor read; the AI never paints pixels.

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

const PROTOCOL_VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

const INSTRUCTIONS: &str = "Ambrotype is a raw photo editor that reads and writes Lightroom's own \
settings. Edits are commands on a photo's XMP sidecar, the same ones the editor uses, so they \
are undoable and open in Lightroom. Rules: run edit_photo with dry_run true first and show the \
user the changes; check visual changes with photo_stats (numbers, cheap) or render_preview (a \
picture); never export over an existing file unless the user asks. edit_photo's do takes a \
compact line (\"exp=+0.5 con+=10 hl=-40\") and replies with the net changes only. Read the \
resource ambrotype://core-card first (the compact syntax and common settings); \
ambrotype://ai-guide and the settings_reference tool have the rest, with intent-to-settings \
recipes.";

const AI_GUIDE: &str = include_str!("../../../docs/ai/README.md");

/// Run the server until stdin closes.
pub fn serve() -> Result<(), String> {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout().lock();
    eprintln!("[amb mcp] ready (Ambrotype {})", env!("CARGO_PKG_VERSION"));
    for line in stdin.lock().lines() {
        let line = line.map_err(|e| e.to_string())?;
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Ok(Value::Array(batch)) => {
                let out: Vec<Value> = batch.iter().filter_map(handle).collect();
                (!out.is_empty()).then_some(Value::Array(out))
            }
            Ok(msg) => handle(&msg),
            Err(e) => Some(error(Value::Null, -32700, &format!("parse error: {e}"))),
        };
        if let Some(reply) = reply {
            writeln!(stdout, "{reply}").map_err(|e| e.to_string())?;
            stdout.flush().map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn ok(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

/// Answer one message; notifications (no `id`) get no answer.
pub fn handle(msg: &Value) -> Option<Value> {
    let id = msg.get("id").cloned()?;
    let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
    let params = msg.get("params").cloned().unwrap_or(json!({}));
    Some(match method {
        "initialize" => {
            let asked = params
                .get("protocolVersion")
                .and_then(Value::as_str)
                .unwrap_or("");
            let version = PROTOCOL_VERSIONS
                .iter()
                .find(|v| **v == asked)
                .unwrap_or(&PROTOCOL_VERSIONS[0]);
            ok(
                id,
                json!({
                    "protocolVersion": version,
                    "capabilities": { "tools": {}, "resources": {} },
                    "serverInfo": { "name": "ambrotype", "version": env!("CARGO_PKG_VERSION") },
                    "instructions": INSTRUCTIONS,
                }),
            )
        }
        "ping" => ok(id, json!({})),
        "tools/list" => ok(id, json!({ "tools": tools() })),
        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            eprintln!("[amb mcp] {name} {}", short(&args));
            let result = match call(name, &args) {
                Ok(content) => json!({ "content": content }),
                Err(e) => {
                    eprintln!("[amb mcp]   error: {e}");
                    json!({ "content": [text(&e)], "isError": true })
                }
            };
            ok(id, result)
        }
        "resources/list" => ok(
            id,
            json!({ "resources": [
                { "uri": "ambrotype://core-card", "name": "Core card", "mimeType": "text/markdown",
                  "description": "The compact edit syntax and the most used settings with ranges, in a few hundred tokens." },
                { "uri": "ambrotype://ai-guide", "name": "AI guide", "mimeType": "text/markdown",
                  "description": "How to edit with Ambrotype: the loop, commands, masks, spots, intent-to-settings recipes." },
                { "uri": "ambrotype://settings-reference", "name": "Settings reference", "mimeType": "text/markdown",
                  "description": "Every setting's name, range, default and meaning." },
            ]}),
        ),
        "resources/read" => {
            let uri = params.get("uri").and_then(Value::as_str).unwrap_or("");
            let body = match uri {
                "ambrotype://core-card" => amb_protocol::card::core(),
                "ambrotype://ai-guide" => AI_GUIDE.to_string(),
                "ambrotype://settings-reference" => amb_protocol::reference::markdown(),
                _ => return Some(error(id, -32602, &format!("unknown resource {uri:?}"))),
            };
            ok(
                id,
                json!({ "contents": [{ "uri": uri, "mimeType": "text/markdown", "text": body }] }),
            )
        }
        "resources/templates/list" => ok(id, json!({ "resourceTemplates": [] })),
        "prompts/list" => ok(id, json!({ "prompts": [] })),
        _ => error(id, -32601, &format!("unknown method {method:?}")),
    })
}

/// A short, one-line view of a tool's arguments for the log.
fn short(args: &Value) -> String {
    let s = args.to_string();
    if s.chars().count() > 200 {
        format!("{}…", s.chars().take(200).collect::<String>())
    } else {
        s
    }
}

fn text(s: &str) -> Value {
    json!({ "type": "text", "text": s })
}

fn tools() -> Value {
    let photo = json!({ "type": "string", "description": "Path of the photo (raw, JPEG, ...) or of a virtual copy's .xmp" });
    json!([
        {
            "name": "describe_photo",
            "description": "Camera, lens, exposure and capture time of a photo, its rating, flag, label and keywords, and its current edit: the settings that differ from the defaults, masks, spots, and settings that don't render yet.",
            "inputSchema": { "type": "object", "properties": { "photo": photo }, "required": ["photo"] },
        },
        {
            "name": "edit_photo",
            "description": "Apply edit commands to a photo's XMP sidecar as one undoable step: set_param, adjust_param, reset_param, reset_group, apply_preset, add_mask, set_mask, paint_mask, remove_mask, duplicate_mask, move_mask, add_spot, set_spot, remove_spot, save_snapshot, apply_snapshot, rename_snapshot, delete_snapshot, undo, redo. Values outside a setting's range are clamped and reported. Give either commands (JSON) or do, a compact line such as \"exp=+0.5 con+=10 hl=-40 reset:hsl\" (Name=v, Name+=d, Name-=d, reset:X, preset:PATH[@amount], sky:Name=v,… for a sky mask with those sliders (subject:, background:, people: likewise), snap:N, load:N, undo, redo; short names temp tint exp con hl sh wh bl tex cla deh vib sat sharp nr cnr vig grain). With do (or brief) the reply lists only the net changes. Use dry_run first and show the user the diff.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "photo": photo,
                    "commands": { "type": "array", "items": { "type": "object" }, "description": "Commands, e.g. [{\"cmd\":\"set_param\",\"name\":\"Exposure2012\",\"value\":0.5}]. Schema: `amb protocol schema command`." },
                    "do": { "type": "string", "description": "The edits as one compact line instead of commands, e.g. \"exp=+0.5 con+=10\"." },
                    "dry_run": { "type": "boolean", "description": "Report the changes without writing (default false)." },
                    "brief": { "type": "boolean", "description": "Reply with the net changes only (default: true with do, false with commands)." },
                },
                "required": ["photo"],
            },
        },
        {
            "name": "query_photo",
            "description": "Read-only questions about a photo's edit: get_param, list_params (optionally by group), describe (a setting's documentation and value), list_groups, list_masks, list_spots, list_snapshots.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "photo": photo,
                    "queries": { "type": "array", "items": { "type": "object" }, "description": "Queries, e.g. [{\"query\":\"list_params\",\"group\":\"tone\"}]." },
                },
                "required": ["photo", "queries"],
            },
        },
        {
            "name": "render_preview",
            "description": "Render the photo with its current edit (or with a preset instead) and return a JPEG to look at, with the as-shot white balance and any settings that don't render yet.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "photo": photo,
                    "long_edge": { "type": "integer", "minimum": 256, "maximum": 2048, "description": "Size of the preview's long edge in pixels (default 1024)." },
                    "preset": { "type": "string", "description": "Render with this preset or sidecar file instead of the photo's own edit (nothing is written)." },
                    "amount": { "type": "number", "minimum": 0, "maximum": 200, "description": "The preset's Amount in percent (default 100)." },
                },
                "required": ["photo"],
            },
        },
        {
            "name": "photo_stats",
            "description": "The photo developed small (with its edit, or a preset instead) as a few numbers, a cheap check before or instead of render_preview: CIE L* percentiles (p1..p99) and mean, clipped shadows and highlights (% of pixels, per channel), the midtone colour cast (a*, b*: + is magenta, + is warm) and mean chroma.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "photo": photo,
                    "preset": { "type": "string", "description": "Measure with this preset or sidecar file instead of the photo's own edit (nothing is written)." },
                    "amount": { "type": "number", "minimum": 0, "maximum": 200, "description": "The preset's Amount in percent (default 100)." },
                },
                "required": ["photo"],
            },
        },
        {
            "name": "match_look",
            "description": "Find the settings that make the photo look like reference images (JPEGs, or raws with their edits): white balance, tone, Vibrance/Saturation, Color Grading shadow/highlight tints and the Color Mixer (HSL per colour), solved by measuring small renders (no AI involved). Returns the changes; with apply they are written as one undoable step. Show the user the changes before applying.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "photo": photo,
                    "references": { "type": "array", "items": { "type": "string" }, "minItems": 1, "description": "Paths of the reference images." },
                    "parts": { "type": "array", "items": { "type": "string", "enum": ["wb", "tone", "color", "grading", "hsl"] }, "description": "What may change (default: all)." },
                    "apply": { "type": "boolean", "description": "Write the changes to the photo's edit (default false)." },
                },
                "required": ["photo", "references"],
            },
        },
        {
            "name": "export_photo",
            "description": "Export the photo with its edit to a file (JPEG, PNG, TIFF, WebP, AVIF or EXR, from the extension). Refuses to overwrite an existing file unless overwrite is true.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "photo": photo,
                    "out": { "type": "string", "description": "The file to write, e.g. /photos/out/IMG_1.jpg" },
                    "long_edge": { "type": "integer", "minimum": 16, "maximum": 65535, "description": "Resize so the long edge is at most this many pixels." },
                    "quality": { "type": "integer", "minimum": 1, "maximum": 100, "description": "JPEG/AVIF quality (default 90)." },
                    "color_space": { "type": "string", "enum": ["srgb", "p3", "adobergb", "prophoto", "rec2020"] },
                    "overwrite": { "type": "boolean" },
                },
                "required": ["photo", "out"],
            },
        },
        {
            "name": "list_photos",
            "description": "The photos in a folder (raws and rendered images; a raw's same-name JPEG is left out), with virtual copies, ratings, flags, labels, keywords and whether they are edited.",
            "inputSchema": { "type": "object", "properties": { "folder": { "type": "string" } }, "required": ["folder"] },
        },
        {
            "name": "set_photo_info",
            "description": "Set a photo's rating (0-5, 0 clears), colour label (Red, Yellow, Green, Blue, Purple, or \"\" to clear), flag (pick, reject, or \"\" to clear), keywords, title, caption, creator and copyright (\"\" clears), written to its sidecar where Lightroom reads them and embedded in exports.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "photo": photo,
                    "rating": { "type": "integer", "minimum": 0, "maximum": 5 },
                    "label": { "type": "string" },
                    "flag": { "type": "string", "enum": ["pick", "reject", ""] },
                    "add_keywords": { "type": "array", "items": { "type": "string" } },
                    "remove_keywords": { "type": "array", "items": { "type": "string" } },
                    "title": { "type": "string", "description": "\"\" clears it." },
                    "caption": { "type": "string" },
                    "creator": { "type": "string", "description": "Several separated by \"; \"." },
                    "copyright": { "type": "string" },
                },
                "required": ["photo"],
            },
        },
        {
            "name": "ai_models",
            "description": "The local AI models behind the subject, background and people masks. action list (default): each task's models (only freely licensed ones), their licence, size, speed, and which is installed and in use. install: download one (tell the user its size first). use: make an installed one its task's model. remove: delete one. Bigger models give finer edges but are slower.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["list", "install", "use", "remove"] },
                    "id": { "type": "string", "description": "The model's id, from list" },
                },
            },
        },
        {
            "name": "settings_reference",
            "description": "The settings an edit can use: without a group, the list of groups; with a group (e.g. tone, color_grading, hsl, local), each setting's name, range, default, status and meaning.",
            "inputSchema": { "type": "object", "properties": { "group": { "type": "string" } } },
        },
    ])
}

fn arg<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| format!("{key} is required"))
}

/// The photo's file, which must exist.
fn photo(args: &Value) -> Result<PathBuf, String> {
    let p = PathBuf::from(arg(args, "photo")?);
    if !p.is_file() {
        return Err(format!("{}: no such file", p.display()));
    }
    Ok(p)
}

fn call(name: &str, args: &Value) -> Result<Vec<Value>, String> {
    match name {
        "describe_photo" => describe(&photo(args)?).map(|v| vec![json_text(&v)]),
        "edit_photo" => {
            let p = photo(args)?;
            let line = args.get("do").and_then(Value::as_str);
            let commands = match line {
                Some(line) => line.to_string(),
                None => args
                    .get("commands")
                    .filter(|c| c.is_array() || c.is_object())
                    .ok_or("give commands (an array of command objects) or do (a compact line)")?
                    .to_string(),
            };
            let sidecar = amb_engine::sidecar_path(&p);
            let req = amb_protocol::edit::EditRequest {
                sidecar: &sidecar,
                commands: &commands,
                out: None,
                dry_run: args
                    .get("dry_run")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            };
            let report = match line {
                Some(_) => amb_protocol::edit::run_compact(&req),
                None => amb_protocol::edit::run(&req),
            }
            .map_err(|e| e.to_string())?;
            let brief = args
                .get("brief")
                .and_then(Value::as_bool)
                .unwrap_or(line.is_some());
            Ok(vec![text(&if brief {
                report.to_brief()
            } else {
                report.to_text()
            })])
        }
        "query_photo" => {
            let p = photo(args)?;
            let queries = args
                .get("queries")
                .filter(|c| c.is_array() || c.is_object())
                .ok_or("queries is required: an array of query objects")?;
            let v = amb_protocol::edit::run_queries(
                &amb_engine::sidecar_path(&p),
                &queries.to_string(),
            )
            .map_err(|e| e.to_string())?;
            Ok(vec![json_text(&v)])
        }
        "render_preview" => render_preview(&photo(args)?, args),
        "photo_stats" => photo_stats(&photo(args)?, args),
        "match_look" => match_look(&photo(args)?, args),
        "export_photo" => export(&photo(args)?, args),
        "list_photos" => list_photos(Path::new(arg(args, "folder")?)).map(|v| vec![json_text(&v)]),
        "set_photo_info" => set_info(&photo(args)?, args).map(|v| vec![json_text(&v)]),
        "ai_models" => {
            let dir = amb_ai::default_dir().ok_or("no home folder; set AMB_MODELS")?;
            match args.get("action").and_then(Value::as_str).unwrap_or("list") {
                "list" => Ok(vec![json_text(&json!({
                    "tasks": amb_ai::TASKS.map(|(t, about)| json!({"task": t, "for": about})),
                    "models": amb_ai::list(&dir),
                }))]),
                "install" => amb_ai::install(&dir, arg(args, "id")?, &|_, _| {}).map(|m| {
                    vec![text(&format!(
                        "{} is installed; `use` makes it the {} model.",
                        m.name, m.task
                    ))]
                }),
                "use" => amb_ai::choose(&dir, arg(args, "id")?).map(|m| {
                    vec![text(&if m.task == "search" {
                        format!("Smart search now uses {}.", m.name)
                    } else {
                        format!("{} masks now use {}.", m.task, m.name)
                    })]
                }),
                "remove" => amb_ai::remove(&dir, arg(args, "id")?)
                    .map(|()| vec![text("Removed; its task uses its default model again.")]),
                other => Err(format!("{other:?}: action is list, install, use or remove")),
            }
        }
        "settings_reference" => {
            let all = amb_protocol::reference::json();
            let groups = all["groups"].as_array().cloned().unwrap_or_default();
            let v = match args.get("group").and_then(Value::as_str) {
                Some(g) => groups
                    .into_iter()
                    .find(|x| x["name"] == g)
                    .ok_or_else(|| format!("no group {g:?}; call settings_reference without a group for the list"))?,
                None => Value::Array(
                    groups
                        .iter()
                        .map(|g| {
                            json!({ "name": g["name"], "label": g["label"], "description": g["description"],
                                    "settings": g["tags"].as_array().map_or(0, Vec::len) })
                        })
                        .collect(),
                ),
            };
            Ok(vec![json_text(&v)])
        }
        _ => Err(format!("unknown tool {name:?}")),
    }
}

fn json_text(v: &Value) -> Value {
    text(&serde_json::to_string_pretty(v).unwrap_or_default())
}

fn describe(p: &Path) -> Result<Value, String> {
    let source = amb_engine::source_path(p);
    let (shot, meta) = amb_engine::raw::read_metadata(&source)
        .map_err(|e| format!("{}: {e}", source.display()))?;
    let sidecar = amb_engine::sidecar_path(p);
    let xmp = std::fs::read_to_string(&sidecar).ok();
    let info = xmp
        .as_deref()
        .map(amb_protocol::sidecar::read_info)
        .unwrap_or_default();
    let state = if sidecar.is_file() {
        amb_protocol::DevelopState::load(&sidecar).map_err(|e| e.to_string())?
    } else {
        amb_protocol::DevelopState::new()
    };
    let (_, not_rendered) = amb_engine::pipeline::DevelopSettings::from_settings(state.settings());
    let q = |query: Value| {
        state
            .query(&serde_json::from_value(query).expect("valid query"))
            .unwrap_or(Value::Null)
    };
    Ok(json!({
        "photo": p.display().to_string(),
        "virtual_copy_of": amb_engine::virtual_copy_of(p).map(|(raw, n)| json!({ "photo": raw.display().to_string(), "copy": n })),
        "sidecar": sidecar.display().to_string(),
        "camera": format!("{} {}", meta.make, meta.model).trim(),
        "lens": shot.lens_model,
        "focal_mm": shot.focal_mm,
        "aperture": shot.aperture,
        "iso": meta.iso,
        "captured": meta.date_time_original,
        "orientation": meta.orientation,
        "rating": info.rating,
        "label": info.label,
        "flag": info.flag,
        "keywords": info.keywords,
        "edit": q(json!({ "query": "list_params" })),
        "masks": q(json!({ "query": "list_masks" })),
        "spots": q(json!({ "query": "list_spots" })),
        "snapshots": q(json!({ "query": "list_snapshots" })),
        "not_rendered": not_rendered,
    }))
}

/// Run `amb stats` as a child, so a panic or slow decode stays out of the server.
fn photo_stats(p: &Path, args: &Value) -> Result<Vec<Value>, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut cmd = std::process::Command::new(exe);
    cmd.arg("stats").arg(p);
    if let Some(preset) = args.get("preset").and_then(Value::as_str) {
        cmd.args(["--preset", preset]);
    }
    if let Some(amount) = args.get("amount").and_then(Value::as_f64) {
        cmd.args(["--amount", &amount.to_string()]);
    }
    let out = cmd
        .output()
        .map_err(|e| format!("couldn't run amb stats: {e}"))?;
    if out.status.success() {
        Ok(vec![text(String::from_utf8_lossy(&out.stdout).trim())])
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Run `amb match` as a child: it renders the photo many times.
fn match_look(p: &Path, args: &Value) -> Result<Vec<Value>, String> {
    let refs: Vec<&str> = args
        .get("references")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    if refs.is_empty() {
        return Err("references is required: one or more image paths".into());
    }
    for r in &refs {
        if !Path::new(r).is_file() {
            return Err(format!("{r}: no such file"));
        }
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut cmd = std::process::Command::new(exe);
    cmd.arg("match").arg(p).args(&refs);
    if let Some(parts) = args.get("parts").and_then(Value::as_array) {
        let list: Vec<&str> = parts.iter().filter_map(Value::as_str).collect();
        cmd.args(["--parts", &list.join(",")]);
    }
    if args.get("apply").and_then(Value::as_bool).unwrap_or(false) {
        cmd.arg("--apply");
    }
    let out = cmd
        .output()
        .map_err(|e| format!("couldn't run amb match: {e}"))?;
    if out.status.success() {
        Ok(vec![text(String::from_utf8_lossy(&out.stdout).trim())])
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Run `amb render` as a child, so its report stays off our stdout.
fn amb_render(args: &[String]) -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let out = std::process::Command::new(exe)
        .arg("render")
        .args(args)
        .output()
        .map_err(|e| format!("couldn't run amb render: {e}"))?;
    let report = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if out.status.success() {
        Ok(report)
    } else {
        Err(format!(
            "{}\n{}",
            report,
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

/// `amb render`'s photo and edit (a preset, or the photo's own sidecar);
/// the output file goes in at index 1.
fn render_args(p: &Path, preset: Option<&str>) -> Vec<String> {
    let mut a = vec![p.display().to_string()];
    let sidecar = amb_engine::sidecar_path(p);
    let edit = preset
        .map(PathBuf::from)
        .or_else(|| sidecar.is_file().then_some(sidecar));
    if let Some(e) = edit {
        a.extend(["--preset".into(), e.display().to_string()]);
    }
    a
}

fn render_preview(p: &Path, args: &Value) -> Result<Vec<Value>, String> {
    let edge = args
        .get("long_edge")
        .and_then(Value::as_u64)
        .unwrap_or(1024)
        .clamp(256, 2048);
    let tmp = std::env::temp_dir().join(format!(
        "amb-mcp-preview-{}-{}.jpg",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    ));
    let mut a = render_args(p, args.get("preset").and_then(Value::as_str));
    a.insert(1, tmp.display().to_string());
    a.extend([
        "--long-edge".into(),
        edge.to_string(),
        "--quality".into(),
        "85".into(),
    ]);
    if let Some(amount) = args.get("amount").and_then(Value::as_f64) {
        a.extend(["--amount".into(), amount.to_string()]);
    }
    let report = amb_render(&a);
    let bytes = std::fs::read(&tmp);
    let _ = std::fs::remove_file(&tmp);
    let report = report?;
    let bytes = bytes.map_err(|e| format!("the preview wasn't written: {e}"))?;
    Ok(vec![
        json!({ "type": "image", "data": base64(&bytes), "mimeType": "image/jpeg" }),
        text(&report),
    ])
}

fn export(p: &Path, args: &Value) -> Result<Vec<Value>, String> {
    let out = PathBuf::from(arg(args, "out")?);
    if out.exists()
        && !args
            .get("overwrite")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    {
        return Err(format!(
            "{} already exists; ask the user, then pass overwrite: true",
            out.display()
        ));
    }
    let mut a = render_args(p, None);
    a.insert(1, out.display().to_string());
    if let Some(e) = args.get("long_edge").and_then(Value::as_u64) {
        a.extend(["--long-edge".into(), e.to_string()]);
    }
    if let Some(q) = args.get("quality").and_then(Value::as_u64) {
        a.extend(["--quality".into(), q.to_string()]);
    }
    if let Some(c) = args.get("color_space").and_then(Value::as_str) {
        a.extend(["--color-space".into(), c.to_string()]);
    }
    Ok(vec![text(&amb_render(&a)?)])
}

fn list_photos(dir: &Path) -> Result<Value, String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .filter(|p| {
            !p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with('.'))
        })
        .collect();
    files.sort();
    let is_photo = |p: &Path| amb_engine::raw::is_raw_path(p) || amb_engine::raster::is_raster(p);
    let raw_stems: std::collections::HashSet<String> = files
        .iter()
        .filter(|p| amb_engine::raw::is_raw_path(p))
        .filter_map(|p| p.file_stem().map(|s| s.to_string_lossy().to_lowercase()))
        .collect();
    let photos: Vec<Value> = files
        .iter()
        .filter(|p| {
            let jpeg_of_raw = amb_engine::raster::is_raster(p)
                && p.file_stem()
                    .is_some_and(|s| raw_stems.contains(&s.to_string_lossy().to_lowercase()));
            (is_photo(p) && !jpeg_of_raw) || amb_engine::virtual_copy_of(p).is_some()
        })
        .map(|p| {
            let xmp = std::fs::read_to_string(amb_engine::sidecar_path(p)).ok();
            let info = xmp
                .as_deref()
                .map(amb_protocol::sidecar::read_info)
                .unwrap_or_default();
            let edited = xmp
                .as_deref()
                .and_then(|t| amb_presets::xmp::parse_crs(t).ok())
                .is_some_and(|s| !s.is_empty());
            json!({
                "photo": p.display().to_string(),
                "copy_of": amb_engine::virtual_copy_of(p).map(|(raw, _)| raw.display().to_string()),
                "rating": info.rating,
                "label": info.label,
                "flag": info.flag,
                "keywords": info.keywords,
                "edited": edited,
            })
        })
        .collect();
    Ok(json!({ "folder": dir.display().to_string(), "photos": photos }))
}

fn set_info(p: &Path, args: &Value) -> Result<Value, String> {
    use amb_protocol::sidecar::{self, Flag};
    let path = amb_engine::sidecar_path(p);
    let on_disk = std::fs::read_to_string(&path).ok();
    let mut info = on_disk
        .as_deref()
        .map(sidecar::read_info)
        .unwrap_or_default();
    if let Some(r) = args.get("rating").and_then(Value::as_u64) {
        info.rating = (1..=5).contains(&r).then_some(r as u8);
    }
    if let Some(l) = args.get("label").and_then(Value::as_str) {
        info.label = Some(l.trim().to_string()).filter(|l| !l.is_empty());
    }
    if let Some(f) = args.get("flag").and_then(Value::as_str) {
        info.flag = Flag::parse(f);
        if info.flag.is_none() && !f.trim().is_empty() {
            return Err(format!("flag {f:?}: use pick, reject or \"\""));
        }
    }
    let list = |k: &str| -> Vec<String> {
        args.get(k)
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(|s| s.trim().to_string())
                    .collect()
            })
            .unwrap_or_default()
    };
    let remove: Vec<String> = list("remove_keywords")
        .iter()
        .map(|k| k.to_lowercase())
        .collect();
    info.keywords
        .retain(|k| !remove.contains(&k.to_lowercase()));
    info.keywords.extend(list("add_keywords"));
    for (key, field) in [
        ("title", &mut info.title),
        ("caption", &mut info.caption),
        ("creator", &mut info.creator),
        ("copyright", &mut info.copyright),
    ] {
        if let Some(v) = args.get(key).and_then(Value::as_str) {
            *field = Some(v.trim().to_string());
        }
    }
    let text = sidecar::with_info(on_disk.as_deref(), &info).map_err(|e| e.to_string())?;
    sidecar::write_atomic(&path, &text).map_err(|e| e.to_string())?;
    let back = sidecar::read_info(&text);
    Ok(
        json!({ "sidecar": path.display().to_string(), "rating": back.rating, "label": back.label,
               "flag": back.flag, "keywords": back.keywords }),
    )
}

/// Standard base64 (RFC 4648), for image content.
fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let n = (u32::from(c[0]) << 16)
            | (u32::from(*c.get(1).unwrap_or(&0)) << 8)
            | u32::from(*c.get(2).unwrap_or(&0));
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 {
            T[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if c.len() > 2 {
            T[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(id: u64, method: &str, params: Value) -> Value {
        json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })
    }

    #[test]
    fn handshake_tools_and_errors() {
        let r = handle(&req(
            1,
            "initialize",
            json!({ "protocolVersion": "2025-03-26" }),
        ))
        .unwrap();
        assert_eq!(r["result"]["protocolVersion"], "2025-03-26");
        assert_eq!(r["result"]["serverInfo"]["name"], "ambrotype");
        let r = handle(&req(
            2,
            "initialize",
            json!({ "protocolVersion": "1999-01-01" }),
        ))
        .unwrap();
        assert_eq!(r["result"]["protocolVersion"], PROTOCOL_VERSIONS[0]);
        assert!(
            handle(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })).is_none()
        );
        let tools = handle(&req(3, "tools/list", json!({}))).unwrap();
        let names: Vec<&str> = tools["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"edit_photo") && names.contains(&"render_preview"));
        let bad = handle(&req(4, "nope", json!({}))).unwrap();
        assert_eq!(bad["error"]["code"], -32601);
        let missing = handle(&req(
            5,
            "tools/call",
            json!({ "name": "describe_photo", "arguments": { "photo": "/no/such.nef" } }),
        ))
        .unwrap();
        assert_eq!(missing["result"]["isError"], true);
        let groups = handle(&req(
            6,
            "tools/call",
            json!({ "name": "settings_reference", "arguments": {} }),
        ))
        .unwrap();
        assert!(
            groups["result"]["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("\"tone\"")
        );
        let guide = handle(&req(
            7,
            "resources/read",
            json!({ "uri": "ambrotype://ai-guide" }),
        ))
        .unwrap();
        assert!(
            guide["result"]["contents"][0]["text"]
                .as_str()
                .unwrap()
                .contains("amb")
        );
    }

    #[test]
    fn base64_matches_the_rfc() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }
}
