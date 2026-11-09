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

mod check;
mod kinds;
mod submit;
#[cfg(test)]
mod tests;

pub use check::*;
pub use kinds::*;
pub use submit::*;

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
/// collage templates, size profiles, border presets and custom interfaces.
fn known_kind(name: &str) -> bool {
    kind_of(name).is_some()
        || [
            "plugin",
            "preset",
            "collage",
            "sizes",
            "border",
            "interface",
        ]
        .contains(&name)
}

/// Where an installed item came from (kept with collage templates and
/// size presets).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HubSource {
    pub hub: String,
    pub id: String,
    pub sha256: String,
}

fn parse_json<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, String> {
    serde_json::from_slice(bytes).map_err(|e| format!("not valid: {e}"))
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

fn kind_of(name: &str) -> Option<Kind> {
    match name {
        "layout" => Some(Kind::Layout),
        "theme" => Some(Kind::Theme),
        _ => None,
    }
}

/// Read a hub file: `https://` (or `file://`, for a hub on disk), at most `max` bytes.
pub fn fetch(url: &str, max: u64) -> Result<Vec<u8>, String> {
    fetch_with(url, max, Some(std::time::Duration::from_secs(20)))
}

/// A large download (the sample photo): no limit on the whole, only on
/// connecting and on each read, so a slow connection still gets there.
pub fn fetch_large(url: &str, max: u64) -> Result<Vec<u8>, String> {
    fetch_with(url, max, None)
}

fn fetch_with(
    url: &str,
    max: u64,
    overall: Option<std::time::Duration>,
) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    if let Some(path) = url.strip_prefix("file://") {
        std::fs::File::open(path)
            .map_err(|e| format!("{path}: {e}"))?
            .take(max + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| format!("{path}: {e}"))?;
    } else if url.starts_with("https://") {
        let mut agent = ureq::AgentBuilder::new()
            .timeout_connect(std::time::Duration::from_secs(20))
            .timeout_read(std::time::Duration::from_secs(30))
            .try_proxy_from_env(true);
        if let Some(t) = overall {
            agent = agent.timeout(t);
        }
        let agent = agent.build();
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

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
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
        "border" => {
            return fetch_border(index_url, item)
                .and_then(|b| serde_json::to_value(b).map_err(|e| e.to_string()));
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
    pub borders: &'a [crate::borders::BorderPreset],
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
        .chain(
            mine.borders
                .iter()
                .filter_map(|b| b.source.as_ref().map(|s| (s, b.id.clone()))),
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
