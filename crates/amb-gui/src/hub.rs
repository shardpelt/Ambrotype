//! The hub: browse and install shared looks (layouts and themes), and
//! submit your own (docs/design/customization-and-hub.md §4; the hub repo
//! template is `hub/` in this repo).
//!
//! Like Omarchy's themes there is no server: a hub is a git repo with one
//! JSON file per look and an `index.json` listing them with their sha256.
//! Only data is installed, never code. Each file must match its checksum and
//! pass the same validation as any other look before it is saved as one of
//! the user's. Submitting opens GitHub's "new file" page with the look filled
//! in, which becomes a pull request to the hub repo.

use crate::looks::{self, Kind};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::Path;

pub const DEFAULT_INDEX: &str =
    "https://raw.githubusercontent.com/Ambrotypehq/ambrotype-hub/main/index.json";
pub const INDEX_FORMAT: &str = "ambrotype-hub/1";
const MAX_INDEX_BYTES: u64 = 2 << 20;
const MAX_ITEM_BYTES: u64 = 256 << 10;
const MAX_ITEMS: usize = 500;
/// GitHub's "new file" link carries the file in the address; longer than this
/// and browsers or GitHub may cut it off.
const MAX_SUBMIT_URL: usize = 7500;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HubItem {
    pub id: String,
    pub kind: String,
    pub name: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub license: String,
    /// The file, relative to the index.
    pub path: String,
    pub sha256: String,
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
        return Err(format!("{url}: more than {MAX_ITEMS} looks"));
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
        if !id_ok || !sha_ok || !seen.insert(item.id.clone()) || kind_of(&item.kind).is_none() {
            continue;
        }
        out.push(item);
    }
    Ok(out)
}

/// One look's file, checked against the index and validated.
fn fetch_item(index_url: &str, item: &HubItem) -> Result<Value, String> {
    let bytes = fetch(&item_url(index_url, &item.path)?, MAX_ITEM_BYTES)?;
    if sha256_hex(&bytes) != item.sha256.to_ascii_lowercase() {
        return Err(
            "the file doesn't match the hub's checksum (changed after it was listed)".into(),
        );
    }
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

/// Everything in the hub, with previews and what's installed already.
pub fn browse(index_url: &str, dir: Option<&Path>) -> Result<Vec<HubEntry>, String> {
    use rayon::prelude::*;
    let items = read_index(index_url)?;
    let mine = installed(dir, index_url);
    Ok(items
        .into_par_iter()
        .map(|item| {
            let (data, error) = match fetch_item(index_url, &item) {
                Ok(d) => (Some(d), None),
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
            "this look is too big to send through the browser link; use Share… to save it, then add the file to the hub repo on GitHub".into(),
        );
    }
    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hub_url() -> String {
        format!("file://{}/../../hub/index.json", env!("CARGO_MANIFEST_DIR"))
    }

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("amb-hub-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// The hub template in this repo is what people copy: it must be clean and current.
    #[test]
    fn the_hub_template_is_valid() {
        let entries = browse(&hub_url(), None).unwrap();
        assert!(entries.len() >= 3);
        for e in &entries {
            assert!(e.error.is_none(), "{}: {:?}", e.item.id, e.error);
            assert!(e.data.is_some() && e.installed.is_none());
        }
    }

    #[test]
    fn install_update_and_checksums() {
        let dir = temp_dir("install");
        let url = hub_url();
        let (kind, id) = install(&dir, &url, "theme-warm-dark").unwrap();
        assert_eq!((kind, id.as_str()), (Kind::Theme, "user:warm-dark"));
        let entries = browse(&url, Some(&dir)).unwrap();
        let warm = entries
            .iter()
            .find(|e| e.item.id == "theme-warm-dark")
            .unwrap();
        assert_eq!(warm.installed.as_deref(), Some("user:warm-dark"));
        assert!(!warm.update);
        // Installing again updates the same look rather than making a copy.
        assert_eq!(
            install(&dir, &url, "theme-warm-dark").unwrap().1,
            "user:warm-dark"
        );
        assert!(install(&dir, &url, "theme-nope").is_err());

        // A file changed after it was listed is refused.
        let copy = temp_dir("tampered");
        std::fs::create_dir_all(copy.join("looks/themes")).unwrap();
        let index = std::fs::read_to_string(format!(
            "{}/../../hub/index.json",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap();
        std::fs::write(copy.join("index.json"), index).unwrap();
        std::fs::write(copy.join("looks/themes/paper.json"), "{}").unwrap();
        let tampered = format!("file://{}/index.json", copy.display());
        let entries = browse(&tampered, None).unwrap();
        let paper = entries.iter().find(|e| e.item.id == "theme-paper").unwrap();
        assert!(paper.error.as_deref().unwrap().contains("checksum"));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&copy);
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
