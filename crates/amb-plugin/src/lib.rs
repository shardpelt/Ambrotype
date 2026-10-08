//! Plugins: small WebAssembly programs that make edits (docs/ai/plugins.md).
//!
//! A plugin is a folder with a manifest (`plugin.json`) and a WebAssembly
//! module (`plugin.wasm`). It runs in a sandbox (the `wasmi` interpreter):
//! no files, no network, no clock, a memory cap and an instruction budget.
//! Its only way to act is the list of `amb-protocol` commands it returns,
//! which the caller applies like any other edit (one undo step), so a plugin
//! can do no more than a person moving sliders.
//!
//! The manifest lists the permissions a plugin wants (camera facts, statistics
//! of the photo); the app asks once and remembers the answer per
//! [`Plugin::fingerprint`], so a changed plugin asks again.
//!
//! The ABI (what `amb_plugin_sdk::export_plugin!` produces):
//! - exports `memory`, `amb_alloc(len: i32) -> i32` and
//!   `amb_run(ptr: i32, len: i32) -> i64` (output at `ret >> 32`, length
//!   `ret & 0xffffffff`); input and output are JSON
//! - may import `amb.log(ptr: i32, len: i32)`, nothing else

mod manifest;
pub mod photo;

pub use amb_plugin_sdk::{API, Stats};
pub use manifest::{FORMAT, Manifest, Param, ParamKind, Permission, Recipe};

use amb_protocol::Command;
use serde_json::{Map, Value};
use std::path::Path;
use wasmi::{Caller, Config, Engine, Linker, Module, Store, StoreLimits, StoreLimitsBuilder};

/// Instructions a run may use (a second or two in the interpreter).
pub const FUEL: u64 = 1_000_000_000;
/// The most memory a plugin may use.
pub const MEMORY: usize = 64 << 20;
const MAX_OUTPUT: usize = 1 << 20;
const MAX_COMMANDS: usize = 1000;
const MAX_LOG_LINES: usize = 200;

/// A loaded plugin: its manifest and module.
#[derive(Debug, Clone)]
pub struct Plugin {
    pub manifest: Manifest,
    pub wasm: Vec<u8>,
}

/// What the app tells a plugin about the photo. Fields the plugin has no
/// permission for are left out when it runs.
#[derive(Debug, Clone, Default)]
pub struct Context {
    /// The develop settings that are set, by name.
    pub settings: Map<String, Value>,
    /// Camera and shot facts (`photo-info`), as `amb_plugin_sdk::Photo` JSON.
    pub photo: Option<Value>,
    /// Statistics of the edited photo (`photo-stats`).
    pub stats: Option<Stats>,
    pub seed: u64,
}

/// The result of a run.
#[derive(Debug, Clone)]
pub struct Outcome {
    pub commands: Vec<Command>,
    pub message: Option<String>,
    /// Lines the plugin wrote with `log`.
    pub log: Vec<String>,
    /// Instructions used.
    pub fuel_used: u64,
}

impl Context {
    /// The settings as plugins see them: numbers and switches as JSON numbers
    /// and booleans (sidecars keep them as text, e.g. "+0.35", "True").
    pub fn settings_json(
        settings: &std::collections::BTreeMap<String, amb_presets::Value>,
    ) -> Map<String, Value> {
        settings
            .iter()
            .map(|(k, v)| {
                let json = match v {
                    amb_presets::Value::Text(t) => match (v.as_f64(), t.as_str()) {
                        (Some(n), _) => Value::from(n),
                        (None, "True" | "true") => Value::Bool(true),
                        (None, "False" | "false") => Value::Bool(false),
                        _ => Value::from(t.clone()),
                    },
                    other => serde_json::to_value(other).unwrap_or(Value::Null),
                };
                (k.clone(), json)
            })
            .collect()
    }
}

struct HostState {
    limits: StoreLimits,
    log: Vec<String>,
}

impl Plugin {
    /// Read `plugin.json` and its module from a folder.
    pub fn load(dir: &Path) -> Result<Plugin, String> {
        let manifest_path = dir.join("plugin.json");
        let text = std::fs::read_to_string(&manifest_path)
            .map_err(|e| format!("{}: {e}", manifest_path.display()))?;
        let manifest =
            Manifest::from_json(&text).map_err(|e| format!("{}: {e}", manifest_path.display()))?;
        let wasm_path = dir.join(&manifest.wasm);
        let wasm = std::fs::read(&wasm_path).map_err(|e| {
            format!(
                "{}: {e} (build the plugin first: amb plugin build)",
                wasm_path.display()
            )
        })?;
        let plugin = Plugin { manifest, wasm };
        plugin.check()?;
        Ok(plugin)
    }

    /// A plugin from its manifest text and module bytes (built-in plugins).
    pub fn from_parts(manifest: &str, wasm: &[u8]) -> Result<Plugin, String> {
        let plugin = Plugin {
            manifest: Manifest::from_json(manifest)?,
            wasm: wasm.to_vec(),
        };
        plugin.check()?;
        Ok(plugin)
    }

    /// Identifies this exact plugin and the permissions it asks for: an
    /// approval is kept per fingerprint.
    pub fn fingerprint(&self) -> String {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        h.update(&self.wasm);
        for p in &self.manifest.permissions {
            h.update(p.id().as_bytes());
            h.update([0]);
        }
        h.finalize()
            .iter()
            .take(16)
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    /// Check the module: valid WebAssembly with the exports a plugin needs
    /// and no imports other than `amb.log`.
    pub fn check(&self) -> Result<(), String> {
        let engine = engine();
        let module = Module::new(&engine, &self.wasm)
            .map_err(|e| format!("not a valid plugin module: {e}"))?;
        for import in module.imports() {
            if (import.module(), import.name()) != ("amb", "log") {
                return Err(format!(
                    "the plugin asks for {}.{}, which Ambrotype doesn't offer (only amb.log)",
                    import.module(),
                    import.name()
                ));
            }
        }
        for name in ["memory", "amb_alloc", "amb_run"] {
            if module.get_export(name).is_none() {
                return Err(format!(
                    "the plugin module doesn't export {name} (use amb_plugin_sdk::export_plugin!)"
                ));
            }
        }
        Ok(())
    }

    /// The input JSON a run with these parameter values gets: only what the
    /// plugin has permission for.
    pub fn input(&self, ctx: &Context, params: &Map<String, Value>) -> Result<Value, String> {
        let mut input = serde_json::json!({
            "api": API,
            "params": self.manifest.resolve_params(params)?,
            "settings": ctx.settings,
            "seed": ctx.seed,
        });
        if self.manifest.permissions.contains(&Permission::PhotoInfo) {
            if let Some(photo) = &ctx.photo {
                input["photo"] = photo.clone();
            }
        }
        if self.manifest.permissions.contains(&Permission::PhotoStats) {
            if let Some(stats) = &ctx.stats {
                input["stats"] = serde_json::to_value(stats).map_err(|e| e.to_string())?;
            }
        }
        Ok(input)
    }

    /// Run the plugin. The caller applies `Outcome::commands`.
    pub fn run(&self, ctx: &Context, params: &Map<String, Value>) -> Result<Outcome, String> {
        let input = serde_json::to_vec(&self.input(ctx, params)?).map_err(|e| e.to_string())?;
        let name = &self.manifest.name;
        let (output, log, fuel_used) =
            execute(&self.wasm, &input).map_err(|e| format!("{name}: {e}"))?;
        let value: Value = serde_json::from_slice(&output)
            .map_err(|e| format!("{name} returned something that isn't JSON: {e}"))?;
        if let Some(err) = value.get("error").and_then(Value::as_str) {
            return Err(format!("{name}: {err}"));
        }
        let commands = parse_commands(value.get("commands")).map_err(|e| format!("{name}: {e}"))?;
        let message = value
            .get("message")
            .and_then(Value::as_str)
            .map(|m| m.chars().take(500).collect());
        Ok(Outcome {
            commands,
            message,
            log,
            fuel_used,
        })
    }
}

fn engine() -> Engine {
    let mut config = Config::default();
    config.consume_fuel(true);
    Engine::new(&config)
}

/// Only edits: no presets (they would read files), no undo or redo.
fn parse_commands(value: Option<&Value>) -> Result<Vec<Command>, String> {
    let Some(list) = value else {
        return Ok(Vec::new());
    };
    let list = list.as_array().ok_or("\"commands\" must be a list")?;
    if list.len() > MAX_COMMANDS {
        return Err(format!(
            "too many commands ({}, at most {MAX_COMMANDS})",
            list.len()
        ));
    }
    list.iter()
        .enumerate()
        .map(|(i, v)| {
            let c: Command = serde_json::from_value(v.clone()).map_err(|e| format!("command {}: {e}", i + 1))?;
            match c {
                Command::SetParam { .. }
                | Command::AdjustParam { .. }
                | Command::ResetParam { .. }
                | Command::ResetGroup { .. } => Ok(c),
                other => Err(format!(
                    "command {}: plugins can't use {} (only set_param, adjust_param, reset_param, reset_group)",
                    i + 1,
                    other.label()
                )),
            }
        })
        .collect()
}

/// Instantiate the module, hand it `input`, and return its output, log and
/// the instructions used.
fn execute(wasm: &[u8], input: &[u8]) -> Result<(Vec<u8>, Vec<String>, u64), String> {
    let engine = engine();
    let module =
        Module::new(&engine, wasm).map_err(|e| format!("not a valid plugin module: {e}"))?;
    let mut store = Store::new(
        &engine,
        HostState {
            limits: StoreLimitsBuilder::new()
                .memory_size(MEMORY)
                .instances(1)
                .build(),
            log: Vec::new(),
        },
    );
    store.limiter(|s| &mut s.limits);
    store.set_fuel(FUEL).map_err(|e| e.to_string())?;
    let mut linker = Linker::new(&engine);
    linker
        .func_wrap(
            "amb",
            "log",
            |mut caller: Caller<'_, HostState>, ptr: i32, len: i32| {
                let Some(memory) = caller.get_export("memory").and_then(|e| e.into_memory()) else {
                    return;
                };
                let mut buf = vec![0u8; (len.max(0) as usize).min(1000)];
                if memory.read(&caller, ptr as u32 as usize, &mut buf).is_ok() {
                    let log = &mut caller.data_mut().log;
                    if log.len() < MAX_LOG_LINES {
                        log.push(String::from_utf8_lossy(&buf).into_owned());
                    }
                }
            },
        )
        .map_err(|e| e.to_string())?;
    let instance = linker
        .instantiate_and_start(&mut store, &module)
        .map_err(|e| format!("couldn't start: {e}"))?;
    let memory = instance
        .get_memory(&store, "memory")
        .ok_or("the plugin module exports no memory")?;
    let alloc = instance
        .get_typed_func::<i32, i32>(&store, "amb_alloc")
        .map_err(|e| format!("amb_alloc: {e}"))?;
    let run = instance
        .get_typed_func::<(i32, i32), i64>(&store, "amb_run")
        .map_err(|e| format!("amb_run: {e}"))?;
    let len = i32::try_from(input.len()).map_err(|_| "input too large")?;
    let ptr = alloc.call(&mut store, len).map_err(|e| trap(&e))?;
    memory
        .write(&mut store, ptr as u32 as usize, input)
        .map_err(|e| format!("couldn't pass the input: {e}"))?;
    let packed = run.call(&mut store, (ptr, len)).map_err(|e| trap(&e))? as u64;
    let (out_ptr, out_len) = ((packed >> 32) as usize, (packed & 0xffff_ffff) as usize);
    if out_len > MAX_OUTPUT {
        return Err(format!("its output is too large ({out_len} bytes)"));
    }
    let mut output = vec![0u8; out_len];
    memory
        .read(&store, out_ptr, &mut output)
        .map_err(|e| format!("couldn't read its output: {e}"))?;
    let fuel_used = FUEL - store.get_fuel().unwrap_or(0);
    Ok((output, std::mem::take(&mut store.data_mut().log), fuel_used))
}

fn trap(e: &wasmi::Error) -> String {
    let text = e.to_string();
    if text.contains("fuel") {
        "it ran too long and was stopped".into()
    } else {
        format!("it stopped with an error: {text}")
    }
}

#[cfg(test)]
mod tests;
