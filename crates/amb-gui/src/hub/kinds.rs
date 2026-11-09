// SPDX-License-Identifier: GPL-3.0-or-later
//! Fetching and installing each kind of hub item: collages, borders, sizes, presets, plugins, interfaces and shared links.

use super::*;

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

/// A size profile, checked: each size as a size preset would be.
pub(super) fn fetch_sizes(index_url: &str, item: &HubItem) -> Result<SizeProfile, String> {
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
pub(super) fn fetch_collage(
    index_url: &str,
    item: &HubItem,
) -> Result<amb_export::collage::Template, String> {
    let bytes = fetch_checked(index_url, &item.path, &item.sha256, MAX_ITEM_BYTES)?;
    let template: amb_export::collage::Template = parse_json(&bytes)?;
    template.validate()?;
    if template.name.trim().is_empty() {
        return Err("the template needs a name".into());
    }
    Ok(template)
}

/// A border preset, checked.
pub(super) fn fetch_border(
    index_url: &str,
    item: &HubItem,
) -> Result<amb_export::border::Border, String> {
    let bytes = fetch_checked(index_url, &item.path, &item.sha256, MAX_ITEM_BYTES)?;
    let text = std::str::from_utf8(&bytes).map_err(|e| e.to_string())?;
    let border = amb_export::border::Border::from_json(text)?;
    if border.name.trim().is_empty() {
        return Err("the border needs a name".into());
    }
    Ok(border)
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

/// Install (or update) a border preset into the user's. Returns its id there.
pub fn install_border(
    custom: &mut Vec<crate::borders::BorderPreset>,
    index_url: &str,
    id: &str,
) -> Result<String, String> {
    let item = find_item(index_url, id, "border")?;
    let border = fetch_border(index_url, &item)?;
    let existing = custom
        .iter()
        .find(|b| {
            b.source
                .as_ref()
                .is_some_and(|s| s.hub == index_url && s.id == item.id)
        })
        .map(|b| b.id.clone());
    let preset = crate::borders::BorderPreset {
        id: existing.unwrap_or_default(),
        builtin: false,
        border,
        source: Some(source_of(index_url, &item)),
    };
    crate::borders::upsert(custom, &preset)
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
pub(super) fn fetch_preset(index_url: &str, item: &HubItem) -> Result<(Vec<u8>, Value), String> {
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
pub(super) const BUNDLED_PRESETS: &[(&str, &str)] =
    include!(concat!(env!("OUT_DIR"), "/bundled_presets.rs"));

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
pub(super) fn installed_presets(dir: Option<&Path>) -> HashMap<String, (String, String)> {
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

/// A plugin's manifest, checked; its id must be the item's without `plugin-`.
pub(super) fn fetch_plugin_manifest(index_url: &str, item: &HubItem) -> Result<Value, String> {
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
pub(super) fn fetch_interface_manifest(index_url: &str, item: &HubItem) -> Result<Value, String> {
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

/// The user's plugins (or interfaces) that came from this hub: hub id ->
/// (folder name, sha256).
pub(super) fn installed_plugins(
    dir: Option<&Path>,
    index_url: &str,
) -> HashMap<String, (String, String)> {
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
