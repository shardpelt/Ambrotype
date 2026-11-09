// SPDX-License-Identifier: GPL-3.0-or-later
//! Submission links: a look or preset opened as GitHub's "new file" page in the hub repo.

use super::*;

/// GitHub's "new file" link carries the file in the address; longer than this
/// and browsers or GitHub may cut it off.
const MAX_SUBMIT_URL: usize = 7500;

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

/// Where a preset of the user's goes on the hub: GitHub's "new file" page in
/// the hub's `presets/` folder with the .xmp filled in (`Ok(url)`), or, for a
/// file too big for a link, `Err(upload page)` to drop the file on.
pub fn submit_preset_url(
    index_url: &str,
    name: &str,
    xmp: &str,
) -> Result<Result<String, String>, String> {
    let (repo, branch, folder) = github_repo(index_url).ok_or(
        "submitting needs a hub on GitHub (an address on raw.githubusercontent.com); see Settings → General",
    )?;
    let mut stem: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    while stem.contains("--") {
        stem = stem.replace("--", "-");
    }
    let stem = stem.trim_matches('-');
    let stem = if stem.is_empty() { "preset" } else { stem };
    let url = format!(
        "https://github.com/{repo}/new/{branch}?filename={}&value={}&message={}",
        url_encode(&format!("{folder}presets/{stem}.xmp")),
        url_encode(xmp),
        url_encode(&format!("Add preset {name}")),
    );
    Ok(if url.len() <= MAX_SUBMIT_URL {
        Ok(url)
    } else {
        Err(format!(
            "https://github.com/{repo}/upload/{branch}/{folder}presets"
        ))
    })
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
mod preset_submit_tests {
    use super::*;

    #[test]
    fn a_preset_goes_to_the_hubs_presets_folder() {
        let index = "https://raw.githubusercontent.com/Ambrotypehq/ambrotype-hub/main/index.json";
        let url = submit_preset_url(index, "Warm Matte!", "<x:xmpmeta/>")
            .unwrap()
            .unwrap();
        assert!(url.starts_with("https://github.com/Ambrotypehq/ambrotype-hub/new/main?filename=presets%2Fwarm-matte.xmp&value=%3Cx%3Axmpmeta%2F%3E"), "{url}");
        let big = "x".repeat(MAX_SUBMIT_URL);
        assert_eq!(
            submit_preset_url(index, "Big", &big).unwrap(),
            Err("https://github.com/Ambrotypehq/ambrotype-hub/upload/main/presets".into())
        );
        assert!(submit_preset_url("https://example.com/index.json", "A", "").is_err());
    }
}
