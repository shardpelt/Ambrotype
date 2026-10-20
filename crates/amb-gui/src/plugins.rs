// SPDX-License-Identifier: GPL-3.0-or-later
//! Plugins in the editor (docs/design/customization-and-hub.md §5): the
//! built-in ones, the user's (folders in `<settings>/plugins/`), installing,
//! removing, permission approvals, and sharing one as a pull request to the
//! hub repo.
//!
//! Running is in commands/plugins.rs: the plugin returns commands, which go through the
//! session like any other edit (one undo step), or through the protocol into
//! the sidecars of photos that aren't open ([`context_for_file`]).

use amb_plugin::Plugin;
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use amb_plugin::BUILTIN;

/// What a plugin is told about a photo that isn't open: its sidecar's
/// settings, and (only when allowed) its camera facts and statistics, which
/// need the raw decoded.
pub fn context_for_file(
    photo: &Path,
    permissions: &[amb_plugin::Permission],
    seed: u64,
) -> Result<amb_plugin::Context, String> {
    use amb_plugin::Permission;
    let sidecar = crate::session::sidecar_path(photo);
    let settings = if sidecar.is_file() {
        amb_protocol::DevelopState::load(&sidecar)
            .map_err(|e| format!("{}: {e}", sidecar.display()))?
            .settings()
            .clone()
    } else {
        BTreeMap::new()
    };
    let wants = |p: Permission| permissions.contains(&p);
    let img = if wants(Permission::PhotoInfo) || wants(Permission::PhotoStats) {
        let source = amb_engine::source_path(photo);
        Some(amb_engine::raw::load(&source).map_err(|e| format!("{}: {e}", source.display()))?)
    } else {
        None
    };
    Ok(amb_plugin::Context {
        settings: amb_plugin::Context::settings_json(&settings),
        photo: img
            .as_ref()
            .filter(|_| wants(Permission::PhotoInfo))
            .map(amb_plugin::photo::info),
        stats: img
            .as_ref()
            .filter(|_| wants(Permission::PhotoStats))
            .map(|img| amb_plugin::photo::stats(img, &settings)),
        seed,
    })
}

/// Files shared with a plugin: its manifest, module and source, not build output.
const SKIP: [&str; 3] = ["target", ".git", "Cargo.lock"];
const MAX_SHARE_BYTES: u64 = 5 << 20;

/// Permissions approved per plugin id: the fingerprint they were approved for.
pub type Approvals = BTreeMap<String, String>;

/// A plugin as the Plugins dialog lists it.
#[derive(Debug, Clone, Serialize)]
pub struct PluginEntry {
    pub id: String,
    pub builtin: bool,
    pub manifest: Option<amb_plugin::Manifest>,
    pub fingerprint: String,
    /// Its permissions are approved (or it asks for none).
    pub approved: bool,
    /// Explanations of what it asks for.
    pub permissions: Vec<String>,
    pub folder: Option<String>,
    /// Why it couldn't be loaded.
    pub error: Option<String>,
}

pub fn builtin(id: &str) -> Option<Plugin> {
    amb_plugin::builtin(id)
}

fn entry(
    id: String,
    builtin: bool,
    folder: Option<&Path>,
    plugin: Result<Plugin, String>,
    approvals: &Approvals,
) -> PluginEntry {
    match plugin {
        Ok(p) => {
            let fingerprint = p.fingerprint();
            PluginEntry {
                approved: p.manifest.permissions.is_empty()
                    || approvals.get(&id) == Some(&fingerprint),
                permissions: p
                    .manifest
                    .permissions
                    .iter()
                    .map(|x| x.describe().to_string())
                    .collect(),
                id,
                builtin,
                fingerprint,
                manifest: Some(p.manifest),
                folder: folder.map(|f| f.display().to_string()),
                error: None,
            }
        }
        Err(e) => PluginEntry {
            id,
            builtin,
            manifest: None,
            fingerprint: String::new(),
            approved: false,
            permissions: Vec::new(),
            folder: folder.map(|f| f.display().to_string()),
            error: Some(e),
        },
    }
}

/// Built-in plugins, then the user's (each folder in `dir` with a plugin.json).
pub fn list(dir: Option<&Path>, approvals: &Approvals) -> Vec<PluginEntry> {
    let mut out: Vec<PluginEntry> = BUILTIN
        .iter()
        .map(|(m, id, w)| {
            entry(
                id.to_string(),
                true,
                None,
                Plugin::from_parts(m, w),
                approvals,
            )
        })
        .collect();
    let Some(dir) = dir else { return out };
    let mut folders: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|r| {
            r.flatten()
                .map(|e| e.path())
                .filter(|p| p.join("plugin.json").is_file())
                .collect()
        })
        .unwrap_or_default();
    folders.sort();
    for f in folders {
        let id = f
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let plugin = Plugin::load(&f).and_then(|p| {
            if p.manifest.id != id {
                Err(format!(
                    "its id is {:?} but its folder is {id:?}; they must match",
                    p.manifest.id
                ))
            } else if BUILTIN.iter().any(|(_, b, _)| *b == id) {
                Err(format!(
                    "{id:?} is the id of a built-in plugin; give it another id"
                ))
            } else {
                Ok(p)
            }
        });
        out.push(entry(id, false, Some(&f), plugin, approvals));
    }
    out
}

/// A plugin by id: built-in or from the user's folder.
pub fn find(dir: Option<&Path>, id: &str) -> Result<Plugin, String> {
    amb_plugin::find(dir, id)
}

/// Files of a plugin folder to copy or share: relative path → absolute path.
fn files(src: &Path) -> Result<Vec<(String, PathBuf)>, String> {
    fn walk(root: &Path, at: &Path, out: &mut Vec<(String, PathBuf)>) -> Result<(), String> {
        let mut entries: Vec<_> = std::fs::read_dir(at)
            .map_err(|e| format!("{}: {e}", at.display()))?
            .flatten()
            .collect();
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            let name = e.file_name().to_string_lossy().into_owned();
            if SKIP.contains(&name.as_str()) || name.starts_with('.') && name != ".gitignore" {
                continue;
            }
            let path = e.path();
            let kind = e.file_type().map_err(|e| e.to_string())?;
            if kind.is_dir() {
                walk(root, &path, out)?;
            } else if kind.is_file() {
                let rel = path.strip_prefix(root).map_err(|e| e.to_string())?;
                let rel = rel
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("/");
                out.push((rel, path));
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    walk(src, src, &mut out)?;
    let total: u64 = out
        .iter()
        .filter_map(|(_, p)| std::fs::metadata(p).ok())
        .map(|m| m.len())
        .sum();
    if total > MAX_SHARE_BYTES {
        return Err(format!(
            "the plugin folder holds {} MB; a plugin should be well under 5 MB",
            total >> 20
        ));
    }
    Ok(out)
}

/// Install a plugin from a folder (checked first) into `dir/<id>/`.
pub fn install(dir: &Path, src: &Path) -> Result<String, String> {
    let plugin = Plugin::load(src)?;
    let id = plugin.manifest.id.clone();
    if builtin(&id).is_some() {
        return Err(format!(
            "{id:?} is the id of a built-in plugin; give yours another id"
        ));
    }
    let dest = dir.join(&id);
    if std::fs::canonicalize(src).ok() == std::fs::canonicalize(&dest).ok() && dest.exists() {
        return Ok(id); // already in place
    }
    let list = files(src)?;
    if dest.exists() {
        std::fs::remove_dir_all(&dest).map_err(|e| format!("{}: {e}", dest.display()))?;
    }
    for (rel, path) in list {
        let to = dest.join(&rel);
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        std::fs::copy(&path, &to).map_err(|e| format!("{}: {e}", to.display()))?;
    }
    Ok(id)
}

pub fn remove(dir: &Path, id: &str) -> Result<(), String> {
    if builtin(id).is_some() {
        return Err("built-in plugins can't be removed".into());
    }
    find(Some(dir), id).map_err(|_| format!("no plugin {id:?} in your plugins folder"))?;
    std::fs::remove_dir_all(dir.join(id)).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------- sharing

/// What sharing a plugin (or a custom interface, interface.rs) would send,
/// shown to the user before anything happens.
#[derive(Debug, Clone, Serialize)]
pub struct SharePlan {
    pub id: String,
    /// The branch on the user's fork, e.g. `plugin-warm-it-up`.
    pub pr_branch: String,
    /// The hub repo (owner/name) and its branch.
    pub repo: String,
    pub branch: String,
    /// Where the files go in the repo, e.g. `plugins/warm-it-up/`.
    pub target: String,
    pub files: Vec<ShareFile>,
    pub has_source: bool,
    /// The GitHub CLI is installed and logged in, as this user.
    pub gh_user: Option<String>,
    /// The browser fallback: GitHub's upload page for that folder.
    pub upload_url: String,
    pub title: String,
    pub body: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ShareFile {
    pub path: String,
    pub bytes: u64,
    #[serde(skip)]
    pub source: PathBuf,
}

/// The pull request text: what the plugin is and asks for.
fn pr_body(p: &Plugin, has_source: bool) -> String {
    let m = &p.manifest;
    let permissions = if m.permissions.is_empty() {
        "none".to_string()
    } else {
        m.permissions
            .iter()
            .map(|x| format!("`{}` ({})", x.id(), x.describe()))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let params = m
        .params
        .iter()
        .map(|x| format!("`{}` ({})", x.name, x.label))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "## {name} {version}\n\n{desc}\n\n- **Id:** `{id}`\n- **Author:** {author}\n- **Permissions:** {permissions}\n- **Parameters:** {params}\n- **Source included:** {source}\n- **Module fingerprint:** `{fp}`\n\n## Checklist\n\n- [ ] I made this plugin or may share it under the hub's licence\n- [ ] `amb plugin check` passes\n- [ ] Reviewers can rebuild `plugin.wasm` from the source\n\n_Sent from Ambrotype's Plugins dialog._\n",
        name = m.name,
        version = m.version,
        desc = if m.description.is_empty() {
            "(no description)"
        } else {
            &m.description
        },
        id = m.id,
        author = if m.author.is_empty() {
            "(not given)"
        } else {
            &m.author
        },
        params = if params.is_empty() {
            "none".into()
        } else {
            params
        },
        source = if has_source {
            "yes"
        } else {
            "no (please add it: reviewers need to see the code)"
        },
        fp = p.fingerprint(),
    )
}

pub fn share_plan(dir: &Path, id: &str, index_url: &str) -> Result<SharePlan, String> {
    if builtin(id).is_some() {
        return Err("built-in plugins are already part of Ambrotype".into());
    }
    let src = dir.join(id);
    let plugin = Plugin::load(&src)?;
    let (repo, branch, folder) = crate::hub::github_repo(index_url).ok_or(
        "sharing needs a hub on GitHub (an address on raw.githubusercontent.com); see Settings → General",
    )?;
    let target = format!("{folder}plugins/{id}/");
    let files: Vec<ShareFile> = files(&src)?
        .into_iter()
        .map(|(path, source)| ShareFile {
            bytes: std::fs::metadata(&source).map(|m| m.len()).unwrap_or(0),
            path,
            source,
        })
        .collect();
    let has_source = files.iter().any(|f| f.path.ends_with(".rs"));
    Ok(SharePlan {
        id: id.to_string(),
        pr_branch: format!("plugin-{id}"),
        upload_url: format!("https://github.com/{repo}/upload/{branch}/{target}"),
        title: format!(
            "Add plugin: {} {}",
            plugin.manifest.name, plugin.manifest.version
        ),
        body: pr_body(&plugin, has_source),
        gh_user: gh_user(),
        repo,
        branch,
        target: target.clone(),
        files,
        has_source,
    })
}

/// The GitHub CLI user, if `gh` is installed and logged in.
pub(crate) fn gh_user() -> Option<String> {
    let out = std::process::Command::new("gh")
        .args(["api", "user", "--jq", ".login"])
        .output()
        .ok()?;
    let login = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (out.status.success() && !login.is_empty()).then_some(login)
}

/// One GitHub API call through the GitHub CLI (the user's own login).
fn gh_api(method: &str, path: &str, body: Option<&Value>) -> Result<Value, String> {
    use std::io::Write;
    let mut cmd = std::process::Command::new("gh");
    cmd.args([
        "api",
        "-X",
        method,
        path,
        "-H",
        "Accept: application/vnd.github+json",
    ]);
    if body.is_some() {
        cmd.args(["--input", "-"]);
    }
    let mut child = cmd
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("couldn't run gh: {e}"))?;
    if let (Some(body), Some(mut stdin)) = (body, child.stdin.take()) {
        stdin
            .write_all(body.to_string().as_bytes())
            .map_err(|e| e.to_string())?;
    }
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!(
            "GitHub said: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    if text.trim().is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_str(&text).map_err(|e| format!("unexpected answer from GitHub: {e}"))
}

pub(crate) fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, &b)| n | u32::from(b) << (16 - 8 * i));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// One GitHub API call: method, path, JSON body.
pub type GitHubApi<'a> = dyn FnMut(&str, &str, Option<&Value>) -> Result<Value, String> + 'a;

/// Send the plan as a pull request: fork the hub repo, add the files in one
/// commit on a new branch of the fork, and open the pull request. Returns its
/// address. `api` is the GitHub API (`gh_api`; a fake in tests).
pub fn share_with(
    plan: &SharePlan,
    user: &str,
    api: &mut GitHubApi,
    wait: &mut dyn FnMut(),
) -> Result<String, String> {
    let fork = api(
        "POST",
        &format!("repos/{}/forks", plan.repo),
        Some(&json!({})),
    )?;
    let fork = fork["full_name"]
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| format!("{user}/{}", plan.repo.split('/').nth(1).unwrap_or_default()));
    // A new fork takes a moment to appear.
    let mut base = None;
    for _ in 0..10 {
        match api(
            "GET",
            &format!("repos/{fork}/git/ref/heads/{}", plan.branch),
            None,
        ) {
            Ok(r) => {
                base = r["object"]["sha"].as_str().map(str::to_string);
                break;
            }
            Err(_) => wait(),
        }
    }
    let base = base.ok_or("your fork of the hub isn't ready yet; try again in a minute")?;
    let commit = api("GET", &format!("repos/{fork}/git/commits/{base}"), None)?;
    let base_tree = commit["tree"]["sha"]
        .as_str()
        .ok_or("unexpected answer from GitHub (no tree)")?;
    let mut tree = Vec::new();
    for f in &plan.files {
        let bytes = std::fs::read(&f.source).map_err(|e| format!("{}: {e}", f.source.display()))?;
        let blob = api(
            "POST",
            &format!("repos/{fork}/git/blobs"),
            Some(&json!({ "content": base64(&bytes), "encoding": "base64" })),
        )?;
        let sha = blob["sha"]
            .as_str()
            .ok_or("unexpected answer from GitHub (no blob)")?;
        tree.push(json!({ "path": format!("{}{}", plan.target, f.path), "mode": "100644", "type": "blob", "sha": sha }));
    }
    let tree = api(
        "POST",
        &format!("repos/{fork}/git/trees"),
        Some(&json!({ "base_tree": base_tree, "tree": tree })),
    )?;
    let commit = api(
        "POST",
        &format!("repos/{fork}/git/commits"),
        Some(&json!({ "message": plan.title, "tree": tree["sha"], "parents": [base] })),
    )?;
    let sha = commit["sha"]
        .as_str()
        .ok_or("unexpected answer from GitHub (no commit)")?;
    let branch = &plan.pr_branch;
    let created = api(
        "POST",
        &format!("repos/{fork}/git/refs"),
        Some(&json!({ "ref": format!("refs/heads/{branch}"), "sha": sha })),
    );
    if created.is_err() {
        // Shared before: move the branch (its open pull request updates too).
        api(
            "PATCH",
            &format!("repos/{fork}/git/refs/heads/{branch}"),
            Some(&json!({ "sha": sha, "force": true })),
        )?;
    }
    let owner = fork.split('/').next().unwrap_or(user);
    let pr = api(
        "POST",
        &format!("repos/{}/pulls", plan.repo),
        Some(
            &json!({ "title": plan.title, "head": format!("{owner}:{branch}"), "base": plan.branch, "body": plan.body }),
        ),
    );
    match pr {
        Ok(pr) => pr["html_url"]
            .as_str()
            .map(str::to_string)
            .ok_or("unexpected answer from GitHub (no pull request)".into()),
        // Already open from an earlier share: the push above updated it.
        Err(e) if e.contains("already exists") => Ok(format!(
            "https://github.com/{}/pulls?q=head%3A{owner}%3A{branch}",
            plan.repo
        )),
        Err(e) => Err(e),
    }
}

/// Share through the GitHub CLI.
pub fn share(plan: &SharePlan) -> Result<String, String> {
    let user = gh_user().ok_or(
        "the GitHub CLI (gh) isn't installed or logged in: run `gh auth login`, or use the browser",
    )?;
    share_with(plan, &user, &mut gh_api, &mut || {
        std::thread::sleep(std::time::Duration::from_secs(2))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("amb-plugins-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// A user plugin: the random-look example under another id.
    fn make_plugin(dir: &Path, id: &str) -> PathBuf {
        let src = dir.join("src-plugin");
        std::fs::create_dir_all(src.join("src")).unwrap();
        std::fs::create_dir_all(src.join("target/x")).unwrap();
        let manifest = include_str!("../../../plugins/random-look/plugin.json")
            .replace("\"random-look\"", &format!("\"{id}\""));
        std::fs::write(src.join("plugin.json"), manifest).unwrap();
        std::fs::write(
            src.join("plugin.wasm"),
            include_bytes!("../../../plugins/random-look/plugin.wasm"),
        )
        .unwrap();
        std::fs::write(src.join("src/lib.rs"), "// code").unwrap();
        std::fs::write(src.join("target/x/big.bin"), "build output").unwrap();
        src
    }

    #[test]
    fn a_photo_that_isnt_open_gets_its_sidecars_settings() {
        let dir = temp("context");
        let photo = dir.join("a.dng");
        amb_testkit::dng::write(
            &photo,
            &amb_testkit::patterns::generate("ramp").unwrap(),
            &Default::default(),
        )
        .unwrap();
        let none = context_for_file(&photo, &[], 7).unwrap();
        assert!(none.settings.is_empty() && none.photo.is_none() && none.stats.is_none());
        crate::library::paste_to_file(
            &photo,
            &[amb_protocol::Command::SetParam {
                name: "Exposure2012".into(),
                value: json!(0.5),
            }],
        )
        .unwrap();
        let all = [
            amb_plugin::Permission::PhotoInfo,
            amb_plugin::Permission::PhotoStats,
        ];
        let ctx = context_for_file(&photo, &all, 7).unwrap();
        assert_eq!(ctx.settings["Exposure2012"], json!(0.5));
        assert!(ctx.photo.is_some() && ctx.stats.is_some());
        assert_eq!(ctx.seed, 7);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn builtins_load() {
        let list = list(None, &Approvals::new());
        assert_eq!(list.len(), BUILTIN.len());
        for e in &list {
            assert!(e.error.is_none(), "{:?}", e.error);
        }
        // Auto tone asks to see the histogram, so it needs approving; Random look doesn't.
        assert!(!list[0].approved && list[1].approved);
        let mut approvals = Approvals::new();
        approvals.insert("auto-tone".into(), list[0].fingerprint.clone());
        assert!(super::list(None, &approvals)[0].approved);
    }

    #[test]
    fn install_list_share_and_remove() {
        let root = temp("install");
        let src = make_plugin(&root, "my-look");
        let dir = root.join("plugins");
        assert_eq!(install(&dir, &src).unwrap(), "my-look");
        assert!(dir.join("my-look/src/lib.rs").exists());
        assert!(
            !dir.join("my-look/target").exists(),
            "build output isn't copied"
        );
        let all = list(Some(&dir), &Approvals::new());
        assert_eq!(all.len(), BUILTIN.len() + 1);
        assert!(all[BUILTIN.len()].error.is_none());
        assert!(find(Some(&dir), "my-look").is_ok());
        assert!(find(Some(&dir), "../etc").is_err());

        let plan = share_plan(&dir, "my-look", crate::hub::DEFAULT_INDEX).unwrap();
        assert_eq!(plan.repo, "Ambrotypehq/ambrotype-hub");
        assert_eq!(plan.target, "plugins/my-look/");
        let paths: Vec<_> = plan.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, ["plugin.json", "plugin.wasm", "src/lib.rs"]);
        assert!(plan.has_source && plan.body.contains("Random look"));

        // The pull request flow against a fake GitHub.
        let mut calls = Vec::new();
        let mut api = |method: &str, path: &str, body: Option<&Value>| -> Result<Value, String> {
            calls.push(format!("{method} {path}"));
            Ok(match (method, path) {
                ("POST", p) if p.ends_with("/forks") => json!({ "full_name": "me/ambrotype-hub" }),
                ("GET", p) if p.contains("/git/ref/") => json!({ "object": { "sha": "base" } }),
                ("GET", _) => json!({ "tree": { "sha": "tree0" } }),
                ("POST", p) if p.ends_with("/blobs") => {
                    assert_eq!(body.unwrap()["encoding"], "base64");
                    json!({ "sha": "blob" })
                }
                ("POST", p) if p.ends_with("/trees") => {
                    assert_eq!(
                        body.unwrap()["tree"][0]["path"],
                        "plugins/my-look/plugin.json"
                    );
                    json!({ "sha": "tree1" })
                }
                ("POST", p) if p.ends_with("/commits") => json!({ "sha": "c1" }),
                ("POST", p) if p.ends_with("/refs") => json!({}),
                ("POST", p) if p.ends_with("/pulls") => {
                    assert_eq!(body.unwrap()["head"], "me:plugin-my-look");
                    json!({ "html_url": "https://github.com/Ambrotypehq/ambrotype-hub/pull/1" })
                }
                _ => return Err(format!("unexpected {method} {path}")),
            })
        };
        let url = share_with(&plan, "me", &mut api, &mut || {}).unwrap();
        assert_eq!(url, "https://github.com/Ambrotypehq/ambrotype-hub/pull/1");
        assert_eq!(calls.iter().filter(|c| c.ends_with("/blobs")).count(), 3);

        remove(&dir, "my-look").unwrap();
        assert!(!dir.join("my-look").exists());
        assert!(remove(&dir, "auto-tone").is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn base64_matches_the_standard() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }
}
