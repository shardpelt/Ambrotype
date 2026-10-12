// SPDX-License-Identifier: GPL-3.0-or-later
//! The lensfun lens database, fetched from lensfun's own update server the
//! way `lensfun-update-data` does it, so lens corrections work without a
//! system lensfun install.
//!
//! The files go to the user's lensfun updates folder
//! (`~/.local/share/lensfun/updates/version_2`), which
//! [`amb_lens::lensfun::LensfunDb::default_dirs`] already searches first
//! after `AMB_LENSFUN_DB`, and which lensfun itself and darktable read too.
//! The database is CC BY-SA 3.0 (lensfun's data licence).

use std::io::Read;
use std::path::{Path, PathBuf};

/// lensfun's update server: `versions.json` and `version_<n>.tar.bz2`.
pub const SERVER: &str = "https://lensfun.github.io/db/";
/// The database format we read.
pub const VERSION: u32 = 2;
/// Largest archive accepted (it is about 0.5 MB).
const MAX_BYTES: u64 = 64 << 20;

/// What a fetch or install did.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Installed {
    pub dir: PathBuf,
    /// XML files written.
    pub files: usize,
    /// The database's timestamp (seconds since 1970).
    pub timestamp: u64,
}

/// Where the database goes: the user's lensfun updates folder.
pub fn user_dir() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .filter(|h| !h.is_empty())?;
    Some(
        PathBuf::from(home)
            .join(".local/share/lensfun/updates")
            .join(format!("version_{VERSION}")),
    )
}

/// The timestamp of the database installed in `dir`, if any.
pub fn installed_timestamp(dir: &Path) -> Option<u64> {
    std::fs::read_to_string(dir.join("timestamp.txt"))
        .ok()?
        .trim()
        .parse()
        .ok()
}

fn get(url: &str) -> Result<Vec<u8>, String> {
    let agent = ureq::AgentBuilder::new()
        .try_proxy_from_env(true)
        .timeout(std::time::Duration::from_secs(60))
        .build();
    let response = agent.get(url).call().map_err(|e| format!("{url}: {e}"))?;
    let mut out = Vec::new();
    response
        .into_reader()
        .take(MAX_BYTES)
        .read_to_end(&mut out)
        .map_err(|e| format!("{url}: {e}"))?;
    Ok(out)
}

/// The newest database's timestamp on the server, if it offers our format.
pub fn latest() -> Result<u64, String> {
    let text = get(&format!("{SERVER}versions.json"))?;
    // [timestamp, [versions…], [...]]
    let v: serde_json::Value =
        serde_json::from_slice(&text).map_err(|e| format!("versions.json: {e}"))?;
    let timestamp = v[0].as_u64().ok_or("versions.json: no timestamp")?;
    let offered = v[1]
        .as_array()
        .is_some_and(|a| a.iter().any(|n| n.as_u64() == Some(u64::from(VERSION))));
    if !offered {
        return Err(format!(
            "the server no longer offers database version {VERSION}"
        ));
    }
    Ok(timestamp)
}

/// Download the newest database into `dir` (default [`user_dir`]). Does
/// nothing when it is already there, unless `force`.
pub fn fetch(dir: Option<&Path>, force: bool) -> Result<Installed, String> {
    let dir = match dir {
        Some(d) => d.to_path_buf(),
        None => user_dir().ok_or("no home folder to put the lens database in")?,
    };
    let timestamp = latest()?;
    if !force && installed_timestamp(&dir) == Some(timestamp) {
        let files = xml_count(&dir);
        return Ok(Installed {
            dir,
            files,
            timestamp,
        });
    }
    let archive = get(&format!("{SERVER}version_{VERSION}.tar.bz2"))?;
    install(&archive, timestamp, &dir)
}

fn xml_count(dir: &Path) -> usize {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "xml"))
        .count()
}

/// Unpack a `version_2.tar.bz2` into `dir`: its XML files replace the old
/// ones (written beside, then moved in), and `timestamp.txt` records it.
pub fn install(archive: &[u8], timestamp: u64, dir: &Path) -> Result<Installed, String> {
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    let mut tar = tar::Archive::new(bzip2::read::BzDecoder::new(archive));
    for entry in tar.entries().map_err(|e| format!("lens database: {e}"))? {
        let mut entry = entry.map_err(|e| format!("lens database: {e}"))?;
        let path = entry.path().map_err(|e| e.to_string())?.into_owned();
        // Only plain XML files, by their bare name (nothing outside `dir`).
        let Some(name) = path
            .file_name()
            .and_then(|n| n.to_str())
            .map(str::to_string)
        else {
            continue;
        };
        if !entry.header().entry_type().is_file()
            || !name.ends_with(".xml")
            || name.starts_with('.')
        {
            continue;
        }
        let mut data = Vec::new();
        entry
            .read_to_end(&mut data)
            .map_err(|e| format!("lens database {name}: {e}"))?;
        files.push((name, data));
    }
    if files.is_empty() {
        return Err("the lens database download had no lens files".into());
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for (name, data) in &files {
        let tmp = dir.join(format!(".{name}.part"));
        std::fs::write(&tmp, data).map_err(|e| format!("{}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, dir.join(name)).map_err(|e| format!("{name}: {e}"))?;
    }
    std::fs::write(dir.join("timestamp.txt"), format!("{timestamp}\n"))
        .map_err(|e| format!("{}: {e}", dir.display()))?;
    Ok(Installed {
        dir: dir.to_path_buf(),
        files: files.len(),
        timestamp,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn archive(files: &[(&str, &str)]) -> Vec<u8> {
        let mut tar = tar::Builder::new(Vec::new());
        for (name, text) in files {
            let mut h = tar::Header::new_gnu();
            h.set_size(text.len() as u64);
            h.set_mode(0o644);
            h.set_cksum();
            tar.append_data(&mut h, name, text.as_bytes()).unwrap();
        }
        let raw = tar.into_inner().unwrap();
        let mut out = Vec::new();
        let mut enc = bzip2::read::BzEncoder::new(&raw[..], bzip2::Compression::fast());
        enc.read_to_end(&mut out).unwrap();
        out
    }

    #[test]
    fn installs_the_xml_files_and_the_timestamp() {
        let dir = std::env::temp_dir().join(format!("amb-lensdb-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let a = archive(&[
            ("version_2/mil-sony.xml", "<lensdatabase/>"),
            ("version_2/README", "not a lens file"),
            ("deep/nested/other.xml", "<lensdatabase/>"),
        ]);
        let got = install(&a, 1_791_482_221, &dir).unwrap();
        assert_eq!(got.files, 2);
        assert!(dir.join("mil-sony.xml").is_file());
        // Every file lands in `dir` under its bare name.
        assert!(dir.join("other.xml").is_file());
        assert!(!dir.join("README").exists());
        assert_eq!(installed_timestamp(&dir), Some(1_791_482_221));
        assert!(install(&archive(&[("a.txt", "x")]), 1, &dir).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
