// SPDX-License-Identifier: GPL-3.0-or-later
//! The hub: browse and install shared looks (layouts and themes) and
//! plugins, and submit your own looks (docs/design/customization-and-hub.md
//! §4; the hub repo is `Ambrotypehq/ambrotype-hub`). The app's built-in looks
//! and starter presets come from the hub too: `data/hub/` is a copy of the
//! items it ships, synced by `tools/sync-hub.py`.
//!
//! Like Omarchy's themes there is no server: a hub is a git repo with one
//! JSON file per look and an `index.json` listing them with their sha256.
//! Each file must match its checksum and pass the same validation as any
//! other look before it is saved as one of the user's. Code comes in two
//! kinds: a plugin's WebAssembly module, which runs in the plugin sandbox and
//! asks for its permissions on first use, like a plugin installed from a
//! folder; and a custom interface's files (interface.rs), which the app
//! warns about before installing and before showing.
//! Submitting opens GitHub's "new file" page with the look filled in, which
//! becomes a pull request to the hub repo.

use crate::looks::{self, Kind};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::Read;
use std::path::Path;

pub const DEFAULT_INDEX: &str =
    "https://raw.githubusercontent.com/Ambrotypehq/ambrotype-hub/main/index.json";
pub const INDEX_FORMAT: &str = "ambrotype-hub/1";
const MAX_INDEX_BYTES: u64 = 2 << 20;
const MAX_ITEM_BYTES: u64 = 256 << 10;
/// A plugin's module (the share dialog keeps whole plugins under 5 MB).
const MAX_PLUGIN_FILE_BYTES: u64 = 5 << 20;
/// A custom interface: how many files, how big each and all together.
pub(crate) const MAX_INTERFACE_FILES: usize = 200;
const MAX_INTERFACE_FILE_BYTES: u64 = 5 << 20;
pub(crate) const MAX_INTERFACE_BYTES: u64 = 20 << 20;
/// What a custom interface may be made of: web pages and their parts.
const INTERFACE_EXTENSIONS: &[&str] = &[
    "html", "htm", "js", "mjs", "css", "json", "svg", "png", "jpg", "jpeg", "webp", "gif", "ico",
    "woff2", "woff", "ttf", "wasm", "md", "txt",
];
/// Kept in an installed plugin's or interface's folder: where it came from.
const HUB_SOURCE_FILE: &str = ".hub.json";
const MAX_ITEMS: usize = 500;
/// GitHub's "new file" link carries the file in the address; longer than this
/// and browsers or GitHub may cut it off.
const MAX_SUBMIT_URL: usize = 7500;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HubItem {
    pub id: String,
    pub kind: String,
    pub name: String,
    /// A develop preset's style group ("Film", "Moody", ...).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub group: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub license: String,
    /// The file, relative to the index (a plugin's `plugin.json`).
    pub path: String,
    pub sha256: String,
    /// A plugin's or interface's other files (a module; pages, scripts and
    /// pictures), relative to `path`'s folder, with their sha256.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub files: BTreeMap<String, String>,
}

#[derive(Deserialize)]
struct Index {
    format: String,
    items: Vec<HubItem>,
}

/// One look in the hub, as the Browse dialog shows it.
#[derive(Debug, Serialize)]
pub struct HubEntry {
    #[serde(flatten)]
    pub item: HubItem,
    /// The look itself (for the preview), or why it can't be used.
    pub data: Option<Value>,
    pub error: Option<String>,
    /// The id of the user's installed copy, if any.
    pub installed: Option<String>,
    /// Installed, but the hub has a newer version.
    pub update: bool,
}

/// Kinds this version can install: looks, plugins, develop presets,
/// collage templates, size profiles and custom interfaces.
fn known_kind(name: &str) -> bool {
    kind_of(name).is_some() || ["plugin", "preset", "collage", "sizes", "interface"].contains(&name)
}

/// Where an installed item came from (kept with collage templates and
/// size presets).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HubSource {
    pub hub: String,
    pub id: String,
    pub sha256: String,
}

/// A size profile: a named set of sizes (`sizes/<name>.json`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SizeProfile {
    pub format: String,
    pub name: String,
    pub sizes: Vec<ProfileSize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfileSize {
    pub name: String,
    pub width: u32,
    pub height: u32,
}

pub const SIZES_FORMAT: &str = "ambrotype-sizes/1";

fn parse_json<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, String> {
    serde_json::from_slice(bytes).map_err(|e| format!("not valid: {e}"))
}

/// A size profile, checked: each size as a size preset would be.
fn fetch_sizes(index_url: &str, item: &HubItem) -> Result<SizeProfile, String> {
    let bytes = fetch_checked(index_url, &item.path, &item.sha256, MAX_ITEM_BYTES)?;
    let profile: SizeProfile = parse_json(&bytes)?;
    if profile.format != SIZES_FORMAT {
        return Err(format!("\"format\" must be \"{SIZES_FORMAT}\""));
    }
    if profile.name.trim().is_empty() || profile.sizes.is_empty() || profile.sizes.len() > 100 {
        return Err("a size profile needs a name and 1 to 100 sizes".into());
    }
    for s in &profile.sizes {
        crate::sizes::validate(&crate::sizes::SizePreset {
            id: String::new(),
            name: s.name.clone(),
            group: profile.name.clone(),
            width: s.width,
            height: s.height,
            builtin: false,
            source: None,
        })?;
    }
    Ok(profile)
}

/// A collage template, checked.
fn fetch_collage(index_url: &str, item: &HubItem) -> Result<amb_export::collage::Template, String> {
    let bytes = fetch_checked(index_url, &item.path, &item.sha256, MAX_ITEM_BYTES)?;
    let template: amb_export::collage::Template = parse_json(&bytes)?;
    template.validate()?;
    if template.name.trim().is_empty() {
        return Err("the template needs a name".into());
    }
    Ok(template)
}

fn find_item(index_url: &str, id: &str, kind: &str) -> Result<HubItem, String> {
    read_index(index_url)?
        .into_iter()
        .find(|i| i.id == id && i.kind == kind)
        .ok_or_else(|| format!("{id} isn't in the hub any more"))
}

fn source_of(index_url: &str, item: &HubItem) -> HubSource {
    HubSource {
        hub: index_url.to_string(),
        id: item.id.clone(),
        sha256: item.sha256.clone(),
    }
}

/// Install (or update) a collage template into the user's templates.
/// Returns its id there.
pub fn install_collage(
    custom: &mut Vec<crate::collages::TemplateEntry>,
    index_url: &str,
    id: &str,
) -> Result<String, String> {
    let item = find_item(index_url, id, "collage")?;
    let template = fetch_collage(index_url, &item)?;
    let existing = custom
        .iter()
        .find(|t| {
            t.source
                .as_ref()
                .is_some_and(|s| s.hub == index_url && s.id == item.id)
        })
        .map(|t| t.id.clone());
    let entry = crate::collages::TemplateEntry {
        id: existing.unwrap_or_default(),
        builtin: false,
        template,
        source: Some(source_of(index_url, &item)),
    };
    crate::collages::upsert(custom, &entry)
}

/// Install (or update) a size profile: its sizes become the user's size
/// presets, grouped under its name. Returns how many.
pub fn install_sizes(
    custom: &mut Vec<crate::sizes::SizePreset>,
    index_url: &str,
    id: &str,
) -> Result<usize, String> {
    let item = find_item(index_url, id, "sizes")?;
    let profile = fetch_sizes(index_url, &item)?;
    custom.retain(|p| {
        !p.source
            .as_ref()
            .is_some_and(|s| s.hub == index_url && s.id == item.id)
    });
    for s in &profile.sizes {
        let preset = crate::sizes::SizePreset {
            id: String::new(),
            name: s.name.clone(),
            group: profile.name.clone(),
            width: s.width,
            height: s.height,
            builtin: false,
            source: Some(source_of(index_url, &item)),
        };
        crate::sizes::upsert(custom, &preset)?;
    }
    Ok(profile.sizes.len())
}

/// Where hub presets go in the user's presets folder (one file per hub id).
const PRESET_FOLDER: &str = "Hub";

/// A develop preset (`.xmp`), checked: what the hub card shows about it.
fn fetch_preset(index_url: &str, item: &HubItem) -> Result<(Vec<u8>, Value), String> {
    let bytes = fetch_checked(index_url, &item.path, &item.sha256, MAX_ITEM_BYTES)?;
    let text = std::str::from_utf8(&bytes).map_err(|_| "the preset isn't text")?;
    let preset = amb_presets::Preset::parse_xmp(text).map_err(|e| e.to_string())?;
    if preset.format != amb_presets::SourceFormat::XmpPreset {
        return Err("not a develop preset (a photo's sidecar?)".into());
    }
    let catalog = amb_protocol::catalog::Catalog::builtin();
    let mut groups: Vec<String> = Vec::new();
    let mut count = 0;
    for name in preset.settings.keys() {
        let Some(tag) = catalog
            .lookup(name)
            .filter(|t| t.group != "metadata" && t.group != "process")
        else {
            continue;
        };
        count += 1;
        let label = catalog
            .group(&tag.group)
            .map_or(tag.group.clone(), |g| g.label.clone());
        if !groups.contains(&label) {
            groups.push(label);
        }
    }
    if count == 0 {
        return Err("the preset changes nothing".into());
    }
    let summary = serde_json::json!({
        "name": preset.name().unwrap_or(&item.name),
        "group": preset.group(),
        "settings": count,
        "groups": groups,
    });
    Ok((bytes, summary))
}

/// Install (or update) a develop preset from the hub into `dir` (the app's
/// presets folder), as `Hub/<id>.xmp`. Returns its path.
pub fn install_preset(dir: &Path, index_url: &str, id: &str) -> Result<std::path::PathBuf, String> {
    let item = read_index(index_url)?
        .into_iter()
        .find(|i| i.id == id && i.kind == "preset")
        .ok_or_else(|| format!("{id} isn't in the hub any more"))?;
    let (bytes, _) = fetch_preset(index_url, &item)?;
    let folder = dir.join(PRESET_FOLDER);
    std::fs::create_dir_all(&folder).map_err(|e| format!("{}: {e}", folder.display()))?;
    let path = folder.join(format!("{}.xmp", item.id));
    let text = String::from_utf8(bytes).map_err(|e| e.to_string())?;
    amb_protocol::sidecar::write_atomic(&path, &text).map_err(|e| e.to_string())?;
    Ok(path)
}

/// The starter presets built into the app (the hub's `presets/` folder):
/// hub id and file.
const BUNDLED_PRESETS: &[(&str, &str)] = include!(concat!(env!("OUT_DIR"), "/bundled_presets.rs"));

/// A develop preset in the Preset Library.
#[derive(Debug, Clone, Serialize)]
pub struct LibraryPreset {
    pub id: String,
    pub name: String,
    pub group: String,
    pub description: String,
    pub author: String,
    /// A local copy, for previewing and installing.
    pub path: String,
    pub installed: bool,
}

/// The Preset Library: the hub's develop presets, each downloaded (and
/// checked) once into `cache`; when the hub can't be reached or has none,
/// the starter presets built into the app. Sorted by group, then name.
pub fn preset_library(index_url: &str, cache: &Path, presets: Option<&Path>) -> Vec<LibraryPreset> {
    let _ = std::fs::create_dir_all(cache);
    let entry = |id: &str, file: &Path, text: &str| -> Option<LibraryPreset> {
        let p = amb_presets::Preset::parse_xmp(text).ok()?;
        let get = |k: &str| {
            p.get(k)
                .and_then(amb_presets::Value::display_text)
                .unwrap_or("")
                .to_string()
        };
        Some(LibraryPreset {
            id: id.to_string(),
            name: p.name().unwrap_or(id).to_string(),
            group: p.group().unwrap_or("Other").to_string(),
            description: get("Description"),
            author: get("Author"),
            path: file.display().to_string(),
            installed: presets
                .is_some_and(|d| d.join(PRESET_FOLDER).join(format!("{id}.xmp")).is_file()),
        })
    };
    let hub: Vec<HubItem> = read_index(index_url)
        .map(|items| items.into_iter().filter(|i| i.kind == "preset").collect())
        .unwrap_or_default();
    let mut out: Vec<LibraryPreset> = hub
        .par_iter()
        .filter_map(|item| {
            let file = cache.join(format!("{}-{}.xmp", item.id, item.sha256.get(..12)?));
            let text = match std::fs::read_to_string(&file) {
                Ok(t) => t,
                Err(_) => {
                    let (bytes, _) = fetch_preset(index_url, item).ok()?;
                    let t = String::from_utf8(bytes).ok()?;
                    amb_protocol::sidecar::write_atomic(&file, &t).ok()?;
                    t
                }
            };
            entry(&item.id, &file, &text)
        })
        .collect();
    if out.is_empty() {
        out = BUNDLED_PRESETS
            .iter()
            .filter_map(|(id, text)| {
                let file = cache.join(format!("{id}.xmp"));
                if std::fs::read_to_string(&file).ok().as_deref() != Some(text) {
                    amb_protocol::sidecar::write_atomic(&file, text).ok()?;
                }
                entry(id, &file, text)
            })
            .collect();
    }
    out.sort_by(|a, b| (&a.group, &a.name).cmp(&(&b.group, &b.name)));
    out
}

/// Install a Preset Library preset (its local copy, which must be in
/// `cache`) into `dir` as `Hub/<id>.xmp`, where hub installs go.
pub fn install_library_preset(
    dir: &Path,
    cache: &Path,
    id: &str,
    file: &Path,
) -> Result<std::path::PathBuf, String> {
    let ok_id = !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    let inside = file
        .canonicalize()
        .ok()
        .zip(cache.canonicalize().ok())
        .is_some_and(|(f, c)| f.starts_with(c));
    if !ok_id || !inside {
        return Err("not a Preset Library preset".into());
    }
    let text = std::fs::read_to_string(file).map_err(|e| e.to_string())?;
    amb_presets::Preset::parse_xmp(&text).map_err(|e| e.to_string())?;
    let folder = dir.join(PRESET_FOLDER);
    std::fs::create_dir_all(&folder).map_err(|e| format!("{}: {e}", folder.display()))?;
    let path = folder.join(format!("{id}.xmp"));
    amb_protocol::sidecar::write_atomic(&path, &text).map_err(|e| e.to_string())?;
    Ok(path)
}

/// The hub presets in the user's presets folder: hub id -> (path, sha256).
fn installed_presets(dir: Option<&Path>) -> HashMap<String, (String, String)> {
    let mut out = HashMap::new();
    let Some(dir) = dir else { return out };
    for entry in std::fs::read_dir(dir.join(PRESET_FOLDER))
        .into_iter()
        .flatten()
        .flatten()
    {
        let path = entry.path();
        let (Some(stem), Ok(bytes)) = (
            path.file_stem().and_then(|s| s.to_str()),
            std::fs::read(&path),
        ) else {
            continue;
        };
        if path.extension().is_some_and(|e| e == "xmp") {
            let shown = path.display().to_string();
            out.insert(stem.to_string(), (shown, sha256_hex(&bytes)));
        }
    }
    out
}

fn kind_of(name: &str) -> Option<Kind> {
    match name {
        "layout" => Some(Kind::Layout),
        "theme" => Some(Kind::Theme),
        _ => None,
    }
}

/// Read a hub file: `https://` (or `file://`, for a hub on disk), at most `max` bytes.
pub fn fetch(url: &str, max: u64) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    if let Some(path) = url.strip_prefix("file://") {
        std::fs::File::open(path)
            .map_err(|e| format!("{path}: {e}"))?
            .take(max + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| format!("{path}: {e}"))?;
    } else if url.starts_with("https://") {
        let agent = ureq::AgentBuilder::new()
            .timeout(std::time::Duration::from_secs(20))
            .try_proxy_from_env(true)
            .build();
        let response = agent.get(url).call().map_err(|e| match e {
            ureq::Error::Status(404, _) => format!("{url}: not found (404)"),
            other => format!("{url}: {other}"),
        })?;
        response
            .into_reader()
            .take(max + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| format!("{url}: {e}"))?;
    } else {
        return Err(format!(
            "{url}: a hub address must start with https:// (or file:// for a hub on this computer)"
        ));
    }
    if bytes.len() as u64 > max {
        return Err(format!("{url}: larger than {} KB", max >> 10));
    }
    Ok(bytes)
}

/// The address of a file listed in the index (paths are relative to it).
fn item_url(index_url: &str, path: &str) -> Result<String, String> {
    let ok = !path.is_empty()
        && !path.starts_with('/')
        && !path.contains(['\\', ':', '?', '#'])
        && path
            .split('/')
            .all(|seg| !seg.is_empty() && seg != "." && seg != "..");
    if !ok {
        return Err(format!("bad path in the index: {path:?}"));
    }
    let base = index_url
        .rsplit_once('/')
        .map(|(b, _)| b)
        .ok_or("bad hub address")?;
    Ok(format!("{base}/{path}"))
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The hub's list of looks (only kinds this version understands).
pub fn read_index(url: &str) -> Result<Vec<HubItem>, String> {
    let bytes = fetch(url, MAX_INDEX_BYTES)?;
    let index: Index =
        serde_json::from_slice(&bytes).map_err(|e| format!("{url}: not a hub index: {e}"))?;
    if index.format != INDEX_FORMAT {
        return Err(format!(
            "{url}: unknown hub format {:?} (this version reads {INDEX_FORMAT})",
            index.format
        ));
    }
    if index.items.len() > MAX_ITEMS {
        return Err(format!("{url}: more than {MAX_ITEMS} items"));
    }
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for item in index.items {
        let id_ok = !item.id.is_empty()
            && item.id.len() <= 80
            && item
                .id
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
        let sha_ok = item.sha256.len() == 64 && item.sha256.chars().all(|c| c.is_ascii_hexdigit());
        let files_ok = item.files.len() <= 8
            && item
                .files
                .values()
                .all(|s| s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit()));
        if !id_ok
            || !sha_ok
            || !files_ok
            || !seen.insert(item.id.clone())
            || !known_kind(&item.kind)
        {
            continue;
        }
        out.push(item);
    }
    Ok(out)
}

/// A file listed in the index, checked against its sha256.
fn fetch_checked(index_url: &str, path: &str, sha256: &str, max: u64) -> Result<Vec<u8>, String> {
    let bytes = fetch(&item_url(index_url, path)?, max)?;
    if sha256_hex(&bytes) != sha256.to_ascii_lowercase() {
        return Err(format!(
            "{path} doesn't match the hub's checksum (changed after it was listed)"
        ));
    }
    Ok(bytes)
}

/// A plugin's manifest, checked; its id must be the item's without `plugin-`.
fn fetch_plugin_manifest(index_url: &str, item: &HubItem) -> Result<Value, String> {
    let bytes = fetch_checked(index_url, &item.path, &item.sha256, MAX_ITEM_BYTES)?;
    let text = String::from_utf8(bytes).map_err(|_| "plugin.json isn't text")?;
    let manifest = amb_plugin::Manifest::from_json(&text)?;
    if item.id.strip_prefix("plugin-") != Some(manifest.id.as_str()) {
        return Err(format!(
            "listed as {} but the plugin's id is {:?}",
            item.id, manifest.id
        ));
    }
    if !item.files.contains_key(&manifest.wasm) {
        return Err(format!(
            "the index doesn't list its module, {}",
            manifest.wasm
        ));
    }
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

/// A custom interface's `interface.json`, checked, and its list of files
/// (for the Browse dialog to show before anything is installed).
fn fetch_interface_manifest(index_url: &str, item: &HubItem) -> Result<Value, String> {
    let name = interface_folder_name(item)?;
    if !item.path.ends_with("/interface.json") {
        return Err("the index should point at its interface.json".into());
    }
    if !item.files.contains_key("index.html") {
        return Err("the index doesn't list its index.html".into());
    }
    if item.files.len() > MAX_INTERFACE_FILES {
        return Err(format!("more than {MAX_INTERFACE_FILES} files"));
    }
    for file in item.files.keys() {
        interface_file_ok(file)?;
    }
    let bytes = fetch_checked(index_url, &item.path, &item.sha256, MAX_ITEM_BYTES)?;
    let manifest: crate::interface::Manifest =
        serde_json::from_slice(&bytes).map_err(|e| format!("interface.json: {e}"))?;
    let mut data = serde_json::to_value(manifest).map_err(|e| e.to_string())?;
    data["folder"] = name.into();
    data["files"] = item.files.keys().cloned().collect::<Vec<_>>().into();
    Ok(data)
}

/// An interface's folder name: its id without `interface-`.
fn interface_folder_name(item: &HubItem) -> Result<&str, String> {
    item.id
        .strip_prefix("interface-")
        .filter(|n| {
            !n.is_empty()
                && n.len() <= 63
                && n.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        })
        .ok_or_else(|| format!("{} isn't a valid interface id", item.id))
}

/// A path inside an interface's folder: plain names, no way out of it, and
/// only the kinds of file a web page is made of.
pub(crate) fn interface_file_ok(path: &str) -> Result<(), String> {
    let bad = || Err(format!("bad file name in the index: {path:?}"));
    if path.is_empty() || path.len() > 200 || path.contains('\\') {
        return bad();
    }
    for part in path.split('/') {
        if part.is_empty()
            || part.starts_with('.')
            || !part
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "-_. ".contains(c))
        {
            return bad();
        }
    }
    let ext = path.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase());
    if !ext.is_some_and(|e| INTERFACE_EXTENSIONS.contains(&e.as_str())) {
        return Err(format!(
            "{path}: not a kind of file an interface is made of"
        ));
    }
    Ok(())
}

/// Install (or update) a custom interface from the hub into `dir` (the
/// user's interfaces folder), every file checked against the index. Returns
/// its folder.
pub fn install_interface(
    dir: &Path,
    index_url: &str,
    id: &str,
) -> Result<std::path::PathBuf, String> {
    let item = read_index(index_url)?
        .into_iter()
        .find(|i| i.id == id && i.kind == "interface")
        .ok_or_else(|| format!("{id} isn't in the hub any more"))?;
    fetch_interface_manifest(index_url, &item)?;
    let name = interface_folder_name(&item)?;
    let folder = item.path.rsplit_once('/').map_or("", |(f, _)| f);
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let tmp = dir.join(format!(".{name}.installing"));
    let _ = std::fs::remove_dir_all(&tmp);
    let staged = (|| -> Result<(), String> {
        let mut total = 0u64;
        let manifest = fetch_checked(index_url, &item.path, &item.sha256, MAX_ITEM_BYTES)?;
        std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
        std::fs::write(tmp.join("interface.json"), manifest).map_err(|e| e.to_string())?;
        for (file, sha) in &item.files {
            let bytes = fetch_checked(
                index_url,
                &format!("{folder}/{file}"),
                sha,
                MAX_INTERFACE_FILE_BYTES,
            )?;
            total += bytes.len() as u64;
            if total > MAX_INTERFACE_BYTES {
                return Err(format!("bigger than {} MB", MAX_INTERFACE_BYTES >> 20));
            }
            let path = tmp.join(file);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            std::fs::write(path, bytes).map_err(|e| e.to_string())?;
        }
        let source = serde_json::json!({ "hub": index_url, "id": item.id, "sha256": item.sha256 });
        std::fs::write(tmp.join(HUB_SOURCE_FILE), source.to_string()).map_err(|e| e.to_string())
    })();
    let target = dir.join(name);
    let done = staged.and_then(|()| {
        // An update replaces the old files whole: none of them is kept.
        let _ = std::fs::remove_dir_all(&target);
        std::fs::rename(&tmp, &target).map_err(|e| e.to_string())
    });
    let _ = std::fs::remove_dir_all(&tmp);
    done.map(|()| target)
}

/// One look's file, checked against the index and validated (for a plugin,
/// its manifest).
fn fetch_item(index_url: &str, item: &HubItem) -> Result<Value, String> {
    match item.kind.as_str() {
        "plugin" => return fetch_plugin_manifest(index_url, item),
        "interface" => return fetch_interface_manifest(index_url, item),
        "preset" => return fetch_preset(index_url, item).map(|(_, summary)| summary),
        "collage" => {
            return fetch_collage(index_url, item)
                .and_then(|t| serde_json::to_value(t).map_err(|e| e.to_string()));
        }
        "sizes" => {
            return fetch_sizes(index_url, item)
                .and_then(|p| serde_json::to_value(p).map_err(|e| e.to_string()));
        }
        _ => {}
    }
    let bytes = fetch_checked(index_url, &item.path, &item.sha256, MAX_ITEM_BYTES)?;
    let data: Value = serde_json::from_slice(&bytes).map_err(|e| format!("not valid JSON: {e}"))?;
    let kind = kind_of(&item.kind).ok_or("unknown kind")?;
    if Kind::of(&data) != Some(kind) {
        return Err(format!(
            "listed as a {} but the file is something else",
            item.kind
        ));
    }
    let errors: Vec<String> = looks::check(kind, &data)
        .into_iter()
        .filter(|p| p.error)
        .map(|p| p.message)
        .collect();
    if !errors.is_empty() {
        return Err(errors.join("; "));
    }
    Ok(data)
}

/// The user's plugins (or interfaces) that came from this hub: hub id ->
/// (folder name, sha256).
fn installed_plugins(dir: Option<&Path>, index_url: &str) -> HashMap<String, (String, String)> {
    let mut out = HashMap::new();
    let Some(dir) = dir else { return out };
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let Ok(text) = std::fs::read_to_string(entry.path().join(HUB_SOURCE_FILE)) else {
            continue;
        };
        let Ok(source) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        if source["hub"].as_str() != Some(index_url) {
            continue;
        }
        if let Some(id) = source["id"].as_str() {
            let sha = source["sha256"].as_str().unwrap_or_default().to_string();
            let name = entry.file_name().to_string_lossy().into_owned();
            out.insert(id.to_string(), (name, sha));
        }
    }
    out
}

/// The user's looks that came from this hub: hub id -> (look id, sha256).
fn installed(dir: Option<&Path>, index_url: &str) -> HashMap<String, (String, String)> {
    let mut out = HashMap::new();
    for kind in [Kind::Layout, Kind::Theme] {
        for look in looks::all(dir, kind) {
            let source = &look.data["source"];
            if look.builtin || source["hub"].as_str() != Some(index_url) {
                continue;
            }
            if let Some(id) = source["id"].as_str() {
                let sha = source["sha256"].as_str().unwrap_or_default().to_string();
                out.insert(id.to_string(), (look.id.clone(), sha));
            }
        }
    }
    out
}

/// Where the user's own things are, to tell what's installed.
#[derive(Debug, Clone, Copy, Default)]
pub struct Mine<'a> {
    pub looks: Option<&'a Path>,
    pub plugins: Option<&'a Path>,
    pub interfaces: Option<&'a Path>,
    pub presets: Option<&'a Path>,
    pub collages: &'a [crate::collages::TemplateEntry],
    pub sizes: &'a [crate::sizes::SizePreset],
}

/// Everything in the hub, with previews and what's installed already.
pub fn browse(index_url: &str, mine: Mine<'_>) -> Result<Vec<HubEntry>, String> {
    use rayon::prelude::*;
    let items = read_index(index_url)?;
    let presets = installed_presets(mine.presets);
    let sources = mine
        .collages
        .iter()
        .filter_map(|t| t.source.as_ref().map(|s| (s, t.id.clone())))
        .chain(
            mine.sizes
                .iter()
                .filter_map(|p| p.source.as_ref().map(|s| (s, s.id.clone()))),
        )
        .filter(|(s, _)| s.hub == index_url)
        .map(|(s, id)| (s.id.clone(), (id, s.sha256.clone())))
        .collect::<Vec<_>>();
    let mut mine = {
        // The app's own looks are hub items too; shown as built in.
        let mut m: HashMap<String, (String, String)> = [Kind::Layout, Kind::Theme]
            .into_iter()
            .flat_map(|kind| looks::all(None, kind))
            .filter(|l| l.builtin)
            .filter_map(|l| {
                let stem = l.id.strip_prefix("builtin:")?;
                let kind = match l.kind {
                    Kind::Layout => "layout",
                    Kind::Theme => "theme",
                };
                let id = format!("{kind}-{stem}");
                let item = items.iter().find(|i| i.id == id)?;
                Some((item.id.clone(), (l.id.clone(), item.sha256.clone())))
            })
            .collect();
        m.extend(installed(mine.looks, index_url));
        m.extend(installed_plugins(mine.plugins, index_url));
        m.extend(installed_plugins(mine.interfaces, index_url));
        m.extend(sources);
        m
    };
    // A hub preset is known by its file name; its content tells if it's current.
    for (id, have) in presets {
        if items.iter().any(|i| i.id == id && i.kind == "preset") {
            mine.insert(id, have);
        }
    }
    Ok(items
        .into_par_iter()
        .map(|item| {
            let (data, error) = match fetch_item(index_url, &item) {
                Ok(mut d) => {
                    // A theme may give only a few colours; preview it whole.
                    if item.kind == "theme" {
                        looks::complete_theme(&mut d);
                    }
                    (Some(d), None)
                }
                Err(e) => (None, Some(e)),
            };
            let have = mine.get(&item.id);
            HubEntry {
                installed: have.map(|(id, _)| id.clone()),
                update: have.is_some_and(|(_, sha)| !sha.eq_ignore_ascii_case(&item.sha256)),
                item,
                data,
                error,
            }
        })
        .collect())
}

/// Install (or update) a plugin from the hub into `dir` (the user's plugins
/// folder): its manifest and module, each checked against the index, then
/// the same checks as installing from a folder. Returns the plugin's id.
pub fn install_plugin(dir: &Path, index_url: &str, id: &str) -> Result<String, String> {
    let item = read_index(index_url)?
        .into_iter()
        .find(|i| i.id == id && i.kind == "plugin")
        .ok_or_else(|| format!("{id} isn't in the hub any more"))?;
    let manifest = fetch_plugin_manifest(index_url, &item)?;
    let folder = item.path.rsplit_once('/').map_or("", |(f, _)| f);
    let tmp = std::env::temp_dir().join(format!("amb-hub-{}-{}", std::process::id(), item.id));
    let _ = std::fs::remove_dir_all(&tmp);
    let staged = (|| -> Result<String, String> {
        std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
        let manifest_text = serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?;
        std::fs::write(tmp.join("plugin.json"), manifest_text).map_err(|e| e.to_string())?;
        for (name, sha) in &item.files {
            // One plain file name each, next to the manifest.
            if name.is_empty() || name.contains(['/', '\\']) || name.starts_with('.') {
                return Err(format!("bad file name in the index: {name:?}"));
            }
            let path = if folder.is_empty() {
                name.clone()
            } else {
                format!("{folder}/{name}")
            };
            let bytes = fetch_checked(index_url, &path, sha, MAX_PLUGIN_FILE_BYTES)?;
            std::fs::write(tmp.join(name), bytes).map_err(|e| e.to_string())?;
        }
        let id = crate::plugins::install(dir, &tmp)?;
        let source = serde_json::json!({ "hub": index_url, "id": item.id, "sha256": item.sha256 });
        std::fs::write(dir.join(&id).join(HUB_SOURCE_FILE), source.to_string())
            .map_err(|e| e.to_string())?;
        Ok(id)
    })();
    let _ = std::fs::remove_dir_all(&tmp);
    staged
}

/// Install (or update) one look from the hub as one of the user's.
pub fn install(dir: &Path, index_url: &str, id: &str) -> Result<(Kind, String), String> {
    let item = read_index(index_url)?
        .into_iter()
        .find(|i| i.id == id)
        .ok_or_else(|| format!("{id} isn't in the hub any more"))?;
    let mut data = fetch_item(index_url, &item)?;
    let kind = kind_of(&item.kind).ok_or("unknown kind")?;
    data["source"] = serde_json::json!({ "hub": index_url, "id": item.id, "sha256": item.sha256 });
    let existing = installed(Some(dir), index_url)
        .remove(&item.id)
        .map(|(id, _)| id);
    looks::save(dir, kind, existing.as_deref(), data).map(|id| (kind, id))
}

/// The file addresses to try for a link someone shared: a file's own
/// address (a GitHub page for it becomes its raw address), or a GitHub repo,
/// whose `theme.json` or `layout.json` at the top is tried.
pub fn link_candidates(link: &str) -> Result<Vec<String>, String> {
    let link = link.trim().trim_end_matches('/');
    let link = link.strip_suffix(".git").unwrap_or(link);
    let bad = || {
        format!(
            "{link}: paste an https:// link to a layout or theme file, or to a GitHub repo with one"
        )
    };
    if !link.starts_with("https://") || link.contains(char::is_whitespace) {
        return Err(bad());
    }
    let Some(rest) = link
        .strip_prefix("https://github.com/")
        .or_else(|| link.strip_prefix("https://www.github.com/"))
    else {
        return Ok(vec![link.to_string()]);
    };
    let rest = rest.split(['?', '#']).next().unwrap_or_default();
    let parts: Vec<&str> = rest.split('/').collect();
    match parts.as_slice() {
        [owner, repo] if !owner.is_empty() && !repo.is_empty() => Ok(["theme.json", "layout.json"]
            .iter()
            .map(|f| format!("https://raw.githubusercontent.com/{owner}/{repo}/HEAD/{f}"))
            .collect()),
        [owner, repo, "blob" | "raw", path @ ..] if !path.is_empty() => Ok(vec![format!(
            "https://raw.githubusercontent.com/{owner}/{repo}/{}",
            path.join("/")
        )]),
        _ => Err(bad()),
    }
}

/// Install a layout or theme from a link (see `link_candidates`). It passes
/// the same checks as any other look; the link is kept as its source.
pub fn install_link(dir: &Path, link: &str) -> Result<(Kind, String), String> {
    let mut last = String::new();
    for url in link_candidates(link)? {
        let bytes = match fetch(&url, MAX_ITEM_BYTES) {
            Ok(b) => b,
            Err(e) => {
                last = e;
                continue;
            }
        };
        let mut data: Value = serde_json::from_slice(&bytes)
            .map_err(|_| format!("{url}: not a layout or theme file (not JSON)"))?;
        let kind = Kind::of(&data).ok_or_else(|| {
            format!("{url}: not an Ambrotype layout or theme (\"format\" is missing)")
        })?;
        data["source"] = serde_json::json!({ "link": link.trim() });
        return looks::save(dir, kind, None, data).map(|id| (kind, id));
    }
    Err(if last.contains("404") {
        format!("{}: no theme.json or layout.json found there", link.trim())
    } else {
        last
    })
}

/// Where a look goes in a GitHub-hosted hub: (owner/repo, branch, folder of the index).
pub(crate) fn github_repo(index_url: &str) -> Option<(String, String, String)> {
    let rest = index_url.strip_prefix("https://raw.githubusercontent.com/")?;
    let mut parts = rest.split('/');
    let owner = parts.next()?;
    let repo = parts.next()?;
    let branch = parts.next()?;
    let mut folder: Vec<&str> = parts.collect();
    folder.pop(); // index.json
    let folder = folder.iter().map(|p| format!("{p}/")).collect::<String>();
    Some((format!("{owner}/{repo}"), branch.to_string(), folder))
}

fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// A link that opens GitHub's "new file" page in the hub repo with this look
/// filled in; committing it there proposes a pull request.
pub fn submit_url(index_url: &str, kind: Kind, data: &Value) -> Result<String, String> {
    let (repo, branch, folder) = github_repo(index_url).ok_or(
        "submitting needs a hub on GitHub (an address on raw.githubusercontent.com); see Settings → General",
    )?;
    let mut data = data.clone();
    if let Some(obj) = data.as_object_mut() {
        obj.remove("source");
    }
    let name = data.get("name").and_then(Value::as_str).unwrap_or("look");
    let mut stem: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    while stem.contains("--") {
        stem = stem.replace("--", "-");
    }
    let stem = stem.trim_matches('-');
    let stem = if stem.is_empty() { "look" } else { stem };
    let sub = match kind {
        Kind::Layout => "layouts",
        Kind::Theme => "themes",
    };
    let text = serde_json::to_string_pretty(&data).map_err(|e| e.to_string())? + "\n";
    let url = format!(
        "https://github.com/{repo}/new/{branch}?filename={}&value={}&message={}",
        url_encode(&format!("{folder}looks/{sub}/{stem}.json")),
        url_encode(&text),
        url_encode(&format!("Add {name}")),
    );
    if url.len() > MAX_SUBMIT_URL {
        return Err(
            "this file is too big to send through the browser link; use Share… to save it, then add the file to the hub repo on GitHub".into(),
        );
    }
    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hub_url() -> String {
        format!("file://{}/data/hub/index.json", env!("CARGO_MANIFEST_DIR"))
    }

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("amb-hub-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// The hub items the app ships (data/hub/) are clean and current, and its
    /// own looks show as built in.
    #[test]
    fn the_bundled_hub_items_are_valid() {
        let entries = browse(&hub_url(), Mine::default()).unwrap();
        assert!(entries.len() >= 9);
        for e in &entries {
            assert!(e.error.is_none(), "{}: {:?}", e.item.id, e.error);
            assert!(e.data.is_some());
            if e.item.kind == "preset" {
                assert!(e.installed.is_none());
            } else {
                let stem = e.item.id.split_once('-').unwrap().1;
                assert_eq!(e.installed.as_deref(), Some(&*format!("builtin:{stem}")));
                assert!(!e.update);
            }
        }
    }

    #[test]
    fn install_update_and_checksums() {
        let dir = temp_dir("install");
        let url = hub_url();
        let (kind, id) = install(&dir, &url, "theme-light").unwrap();
        assert_eq!((kind, id.as_str()), (Kind::Theme, "user:light"));
        let entries = browse(
            &url,
            Mine {
                looks: Some(&dir),
                ..Default::default()
            },
        )
        .unwrap();
        let light = entries.iter().find(|e| e.item.id == "theme-light").unwrap();
        assert_eq!(light.installed.as_deref(), Some("user:light"));
        assert!(!light.update);
        // Installing again updates the same look rather than making a copy.
        assert_eq!(install(&dir, &url, "theme-light").unwrap().1, "user:light");
        assert!(install(&dir, &url, "theme-nope").is_err());

        // A file changed after it was listed is refused.
        let copy = temp_dir("tampered");
        std::fs::create_dir_all(copy.join("looks/themes")).unwrap();
        let index = std::fs::read_to_string(format!(
            "{}/data/hub/index.json",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap();
        std::fs::write(copy.join("index.json"), index).unwrap();
        std::fs::write(copy.join("looks/themes/dark.json"), "{}").unwrap();
        let tampered = format!("file://{}/index.json", copy.display());
        let entries = browse(&tampered, Mine::default()).unwrap();
        let dark = entries.iter().find(|e| e.item.id == "theme-dark").unwrap();
        assert!(dark.error.as_deref().unwrap().contains("checksum"));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&copy);
    }

    #[test]
    fn plugins_install_from_the_hub_checked() {
        let hub = temp_dir("plugin-hub");
        let src = format!("{}/../../plugins/random-look", env!("CARGO_MANIFEST_DIR"));
        let wasm = std::fs::read(format!("{src}/plugin.wasm")).unwrap();
        let text = std::fs::read_to_string(format!("{src}/plugin.json")).unwrap();
        let mut manifest: Value = serde_json::from_str(&text).unwrap();
        manifest["id"] = "dice".into();
        let manifest = serde_json::to_vec(&manifest).unwrap();
        std::fs::create_dir_all(hub.join("plugins/dice")).unwrap();
        std::fs::write(hub.join("plugins/dice/plugin.json"), &manifest).unwrap();
        std::fs::write(hub.join("plugins/dice/plugin.wasm"), &wasm).unwrap();
        let index = serde_json::json!({ "format": INDEX_FORMAT, "items": [{
            "id": "plugin-dice", "kind": "plugin", "name": "Dice",
            "path": "plugins/dice/plugin.json", "sha256": sha256_hex(&manifest),
            "files": { "plugin.wasm": sha256_hex(&wasm) },
        }]});
        std::fs::write(hub.join("index.json"), index.to_string()).unwrap();
        let url = format!("file://{}/index.json", hub.display());
        let mine = temp_dir("plugin-mine");
        let entries = browse(
            &url,
            Mine {
                plugins: Some(&mine),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(entries[0].error.is_none(), "{:?}", entries[0].error);
        assert_eq!(entries[0].data.as_ref().unwrap()["id"], "dice");
        assert!(entries[0].installed.is_none());
        assert_eq!(install_plugin(&mine, &url, "plugin-dice").unwrap(), "dice");
        assert!(mine.join("dice/plugin.wasm").is_file());
        let entries = browse(
            &url,
            Mine {
                plugins: Some(&mine),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(entries[0].installed.as_deref(), Some("dice"));
        assert!(!entries[0].update);
        // A module changed after it was listed is refused.
        std::fs::write(hub.join("plugins/dice/plugin.wasm"), b"\0asm").unwrap();
        let err = install_plugin(&mine, &url, "plugin-dice").unwrap_err();
        assert!(err.contains("checksum"), "{err}");
        let _ = std::fs::remove_dir_all(&hub);
        let _ = std::fs::remove_dir_all(&mine);
    }

    #[test]
    fn interfaces_install_from_the_hub_checked() {
        let hub = temp_dir("ui-hub");
        let files: [(&str, &[u8]); 3] = [
            ("index.html", b"<script src=\"js/app.js\"></script>"),
            ("js/app.js", b"// hi"),
            ("look.css", b"body{}"),
        ];
        let manifest = br#"{"name": "Tiny", "author": "Me"}"#;
        let base = hub.join("interfaces/tiny");
        std::fs::create_dir_all(base.join("js")).unwrap();
        std::fs::write(base.join("interface.json"), manifest).unwrap();
        for (name, bytes) in files {
            std::fs::write(base.join(name), bytes).unwrap();
        }
        let item = |files: Value| {
            serde_json::json!({ "format": INDEX_FORMAT, "items": [{
                "id": "interface-tiny", "kind": "interface", "name": "Tiny",
                "path": "interfaces/tiny/interface.json", "sha256": sha256_hex(manifest),
                "files": files,
            }]})
            .to_string()
        };
        let listed: serde_json::Map<String, Value> = files
            .iter()
            .map(|(n, b)| (n.to_string(), sha256_hex(b).into()))
            .collect();
        std::fs::write(hub.join("index.json"), item(listed.clone().into())).unwrap();
        let url = format!("file://{}/index.json", hub.display());
        let mine = temp_dir("ui-mine");
        let browse_mine = || {
            browse(
                &url,
                Mine {
                    interfaces: Some(&mine),
                    ..Default::default()
                },
            )
            .unwrap()
        };
        let entries = browse_mine();
        assert!(entries[0].error.is_none(), "{:?}", entries[0].error);
        assert_eq!(entries[0].data.as_ref().unwrap()["name"], "Tiny");
        assert_eq!(entries[0].data.as_ref().unwrap()["files"][1], "js/app.js");
        let dir = install_interface(&mine, &url, "interface-tiny").unwrap();
        assert!(crate::interface::is_interface(&dir));
        assert!(dir.join("js/app.js").is_file());
        assert_eq!(browse_mine()[0].installed.as_deref(), Some("tiny"));
        // Changed after it was listed: refused, and the installed copy stays.
        std::fs::write(base.join("js/app.js"), b"// changed").unwrap();
        let err = install_interface(&mine, &url, "interface-tiny").unwrap_err();
        assert!(err.contains("checksum"), "{err}");
        assert!(dir.join("js/app.js").is_file());
        // Nothing outside its folder, and only what pages are made of.
        for bad in [
            "../evil.html",
            "js/../../x.js",
            ".hidden.js",
            "run.sh",
            "a\\b.js",
        ] {
            let mut files = listed.clone();
            files.insert(bad.into(), sha256_hex(b"x").into());
            std::fs::write(hub.join("index.json"), item(files.into())).unwrap();
            assert!(browse_mine()[0].error.is_some(), "{bad}");
            assert!(
                install_interface(&mine, &url, "interface-tiny").is_err(),
                "{bad}"
            );
        }
        let _ = std::fs::remove_dir_all(&hub);
        let _ = std::fs::remove_dir_all(&mine);
    }

    #[test]
    fn preset_library_falls_back_to_the_built_in_presets() {
        let cache = temp_dir("plib-cache");
        let mine = temp_dir("plib-mine");
        let lib = preset_library("file:///nowhere/index.json", &cache, Some(&mine));
        assert_eq!(lib.len(), BUNDLED_PRESETS.len());
        assert!(lib.len() >= 20);
        assert!(
            lib.iter()
                .all(|p| !p.group.is_empty() && !p.description.is_empty() && !p.installed)
        );
        let bw = lib.iter().find(|p| p.id == "preset-bw-classic").unwrap();
        install_library_preset(&mine, &cache, &bw.id, Path::new(&bw.path)).unwrap();
        let again = preset_library("file:///nowhere/index.json", &cache, Some(&mine));
        assert!(again.iter().find(|p| p.id == bw.id).unwrap().installed);
        // Only the library's own copies can be installed.
        let elsewhere = mine.join("x.xmp");
        std::fs::copy(&bw.path, &elsewhere).unwrap();
        assert!(install_library_preset(&mine, &cache, "preset-x", &elsewhere).is_err());
        assert!(install_library_preset(&mine, &cache, "../x", Path::new(&bw.path)).is_err());
    }

    #[test]
    fn presets_install_from_the_hub_checked() {
        let hub = temp_dir("preset-hub");
        let fixtures = format!(
            "{}/../amb-presets/tests/fixtures",
            env!("CARGO_MANIFEST_DIR")
        );
        let preset = std::fs::read(format!("{fixtures}/punchy-teal-orange.xmp")).unwrap();
        let sidecar = std::fs::read(format!("{fixtures}/sidecar.xmp")).unwrap();
        std::fs::create_dir_all(hub.join("presets")).unwrap();
        std::fs::write(hub.join("presets/punchy.xmp"), &preset).unwrap();
        std::fs::write(hub.join("presets/sidecar.xmp"), &sidecar).unwrap();
        let item = |id: &str, file: &str, bytes: &[u8]| {
            serde_json::json!({ "id": id, "kind": "preset", "name": id,
                "path": format!("presets/{file}"), "sha256": sha256_hex(bytes) })
        };
        let index = serde_json::json!({ "format": INDEX_FORMAT, "items": [
            item("preset-punchy", "punchy.xmp", &preset),
            item("preset-sidecar", "sidecar.xmp", &sidecar),
        ]});
        std::fs::write(hub.join("index.json"), index.to_string()).unwrap();
        let url = format!("file://{}/index.json", hub.display());
        let mine = temp_dir("preset-mine");
        let presets = Mine {
            presets: Some(&mine),
            ..Default::default()
        };
        let entries = browse(&url, presets).unwrap();
        let punchy = entries
            .iter()
            .find(|e| e.item.id == "preset-punchy")
            .unwrap();
        assert!(punchy.error.is_none(), "{:?}", punchy.error);
        assert!(punchy.data.as_ref().unwrap()["settings"].as_u64().unwrap() > 3);
        let side = entries
            .iter()
            .find(|e| e.item.id == "preset-sidecar")
            .unwrap();
        assert!(
            side.error
                .as_deref()
                .unwrap()
                .contains("not a develop preset")
        );
        let path = install_preset(&mine, &url, "preset-punchy").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), preset);
        let entries = browse(&url, presets).unwrap();
        let punchy = entries
            .iter()
            .find(|e| e.item.id == "preset-punchy")
            .unwrap();
        assert!(punchy.installed.is_some() && !punchy.update);
        assert!(install_preset(&mine, &url, "preset-sidecar").is_err());
        let _ = std::fs::remove_dir_all(&hub);
        let _ = std::fs::remove_dir_all(&mine);
    }

    #[test]
    fn collages_and_size_profiles_install_from_the_hub() {
        let hub = temp_dir("data-hub");
        std::fs::create_dir_all(hub.join("collages")).unwrap();
        std::fs::create_dir_all(hub.join("sizes")).unwrap();
        let collage = serde_json::json!({ "format": amb_export::collage::FORMAT, "name": "Two up",
            "width": 1200, "height": 800, "cells": [[0, 0, 0.5, 1], [0.5, 0, 0.5, 1]] })
        .to_string();
        let sizes = serde_json::json!({ "format": SIZES_FORMAT, "name": "Etsy",
            "sizes": [{ "name": "Listing", "width": 2000, "height": 1600 },
                      { "name": "Banner", "width": 3360, "height": 840 }] })
        .to_string();
        std::fs::write(hub.join("collages/two-up.json"), &collage).unwrap();
        std::fs::write(hub.join("sizes/etsy.json"), &sizes).unwrap();
        let item = |id: &str, kind: &str, path: &str, text: &str| {
            serde_json::json!({ "id": id, "kind": kind, "name": id, "path": path,
                "sha256": sha256_hex(text.as_bytes()) })
        };
        let index = serde_json::json!({ "format": INDEX_FORMAT, "items": [
            item("collage-two-up", "collage", "collages/two-up.json", &collage),
            item("sizes-etsy", "sizes", "sizes/etsy.json", &sizes),
        ]});
        std::fs::write(hub.join("index.json"), index.to_string()).unwrap();
        let url = format!("file://{}/index.json", hub.display());
        let entries = browse(&url, Mine::default()).unwrap();
        assert!(entries.iter().all(|e| e.error.is_none()), "{entries:?}");

        let mut templates = Vec::new();
        let id = install_collage(&mut templates, &url, "collage-two-up").unwrap();
        // Installing again updates the same template.
        assert_eq!(
            install_collage(&mut templates, &url, "collage-two-up").unwrap(),
            id
        );
        assert_eq!(templates.len(), 1);
        let mut presets = Vec::new();
        assert_eq!(install_sizes(&mut presets, &url, "sizes-etsy").unwrap(), 2);
        assert_eq!(install_sizes(&mut presets, &url, "sizes-etsy").unwrap(), 2);
        assert_eq!(presets.len(), 2);
        assert!(presets.iter().all(|p| p.group == "Etsy"));
        let mine = Mine {
            collages: &templates,
            sizes: &presets,
            ..Default::default()
        };
        let entries = browse(&url, mine).unwrap();
        assert!(entries.iter().all(|e| e.installed.is_some() && !e.update));
        let _ = std::fs::remove_dir_all(&hub);
    }

    #[test]
    fn shared_links() {
        let raw = "https://raw.githubusercontent.com";
        assert_eq!(
            link_candidates("https://github.com/someone/warm-theme.git/").unwrap(),
            [
                format!("{raw}/someone/warm-theme/HEAD/theme.json"),
                format!("{raw}/someone/warm-theme/HEAD/layout.json"),
            ]
        );
        assert_eq!(
            link_candidates("https://github.com/a/b/blob/main/looks/x.json").unwrap(),
            [format!("{raw}/a/b/main/looks/x.json")]
        );
        assert_eq!(
            link_candidates(" https://example.com/t.json ").unwrap(),
            ["https://example.com/t.json"]
        );
        assert!(link_candidates("http://example.com/t.json").is_err());
        assert!(link_candidates("https://github.com/someone").is_err());

        let dir = temp_dir("link");
        let file = dir.join("t.json");
        std::fs::write(
            &file,
            r##"{"format":"ambrotype-theme/1","name":"Mint","tokens":{"bg":"#102020","text":"#e0f0f0","accent":"#40d0a0"}}"##,
        )
        .unwrap();
        // file:// stands in for a web address here.
        let link = format!("file://{}", file.display());
        let looks_dir = dir.join("looks");
        assert!(install_link(&looks_dir, &link).is_err(), "only https links");
        let mut data: Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
        data["source"] = serde_json::json!({ "link": link });
        let id = looks::save(&looks_dir, Kind::Theme, None, data).unwrap();
        let theme = looks::all(Some(&looks_dir), Kind::Theme)
            .into_iter()
            .find(|l| l.id == id)
            .unwrap();
        assert!(theme.usable);
        assert_eq!(theme.data["tokens"]["muted"].as_str().unwrap().len(), 7);
    }

    #[test]
    fn addresses_and_paths() {
        assert!(fetch("http://example.com/index.json", 10).is_err());
        assert!(fetch("ftp://example.com/index.json", 10).is_err());
        let base = "https://raw.githubusercontent.com/o/r/main/index.json";
        assert_eq!(
            item_url(base, "looks/themes/a.json").unwrap(),
            "https://raw.githubusercontent.com/o/r/main/looks/themes/a.json"
        );
        for bad in [
            "../x.json",
            "/etc/passwd",
            "a/../../b",
            "https://evil/x",
            "a\\b",
            "",
        ] {
            assert!(item_url(base, bad).is_err(), "{bad}");
        }
        assert_eq!(
            github_repo("https://raw.githubusercontent.com/o/r/main/hub/index.json"),
            Some(("o/r".into(), "main".into(), "hub/".into()))
        );
        assert_eq!(github_repo("https://example.com/index.json"), None);
    }

    #[test]
    fn submit_link() {
        let data = serde_json::json!({
            "format": looks::THEME_FORMAT, "name": "My Warm Look!", "tokens": {},
            "source": { "hub": "x", "id": "y", "sha256": "z" }
        });
        let url = submit_url(DEFAULT_INDEX, Kind::Theme, &data).unwrap();
        assert!(url.starts_with("https://github.com/Ambrotypehq/ambrotype-hub/new/main?filename=looks%2Fthemes%2Fmy-warm-look.json&value="));
        assert!(
            !url.contains("source"),
            "the install record isn't submitted"
        );
        assert!(submit_url("file:///tmp/index.json", Kind::Theme, &data).is_err());
    }
}
