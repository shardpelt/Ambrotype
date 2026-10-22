// SPDX-License-Identifier: GPL-3.0-or-later
//! The sample photo: a Nikon D750 raw from raw.pixls.us (CC0; NOTICE),
//! downloaded when asked for, since raw files aren't kept in the repo. It
//! comes with an edit already made, one History step per slider, so stepping
//! through History shows what each one does (`ui/teach.js`).

use crate::*;

const URL: &str = "https://raw.pixls.us/download/data/Nikon/D750/compressed_12_bit.NEF";
const SHA256: &str = "e4a3f85e014275fc0a47600a373af95ec850872a5f6cf87649bcaa42b77ef2e4";
const SIZE: u64 = 18_031_834;
/// The edit, made with `amb do` one setting at a time.
const SIDECAR: &str = include_str!("../../data/sample/d750.xmp");

/// Where the sample is kept: `sample/` in the settings folder.
fn sample_dir(state: &AppState) -> Res<PathBuf> {
    let path = state.config_path.lock().map_err(|e| e.to_string())?;
    path.as_ref()
        .and_then(|p| p.parent())
        .map(|d| d.join("sample"))
        .ok_or_else(|| "the settings folder isn't available".into())
}

/// The raw, downloaded and checked the first time; `fresh` puts the edit
/// back as it came.
fn sample_at(dir: &Path, fresh: bool, fetch: impl Fn() -> Res<Vec<u8>>) -> Res<PathBuf> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let raw = dir.join("d750.nef");
    let have = std::fs::read(&raw).is_ok_and(|b| hub::sha256_hex(&b) == SHA256);
    if !have {
        let bytes = fetch()?;
        if hub::sha256_hex(&bytes) != SHA256 {
            return Err(
                "the sample photo didn't download correctly (its checksum is wrong)".into(),
            );
        }
        std::fs::write(&raw, bytes).map_err(|e| e.to_string())?;
    }
    let sidecar = dir.join("d750.xmp");
    if fresh || !sidecar.exists() {
        std::fs::write(&sidecar, SIDECAR).map_err(|e| e.to_string())?;
    }
    Ok(raw)
}

/// The sample photo's path (18 MB to download the first time).
#[tauri::command]
pub(crate) async fn sample_photo(state: tauri::State<'_, AppState>, fresh: bool) -> Res<PathBuf> {
    let dir = sample_dir(state.inner())?;
    tauri::async_runtime::spawn_blocking(move || sample_at(&dir, fresh, || hub::fetch(URL, SIZE)))
        .await
        .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sample_is_checked_and_its_edit_kept() {
        let dir = std::env::temp_dir().join(format!("amb-sample-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let err = sample_at(&dir, false, || Ok(b"not the raw".to_vec())).unwrap_err();
        assert!(err.contains("checksum"), "{err}");
        assert!(!dir.join("d750.nef").exists());
        // The edit parses, and has one History step per setting.
        let s = amb_presets::xmp::parse_crs(SIDECAR).unwrap();
        assert!(s.contains_key("Exposure2012"), "{s:?}");
        assert_eq!(SIDECAR.matches("\"label\":").count(), 9);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
