// SPDX-License-Identifier: GPL-3.0-or-later
//! Saved watermarks and the Watermark maker's live preview
//! (`amb_library::watermarks`, `amb_export`).

use crate::*;
use amb_export::watermark::Watermark;
use amb_library::watermarks::{self, SavedWatermark};

/// The preview photo's long edge (px): big enough to judge a mark on, small
/// enough to come back quickly.
const PREVIEW_EDGE: u32 = 1200;
/// The preview JPEG's quality.
const PREVIEW_QUALITY: u8 = 85;

fn dir() -> Res<PathBuf> {
    Ok(watermarks::editor_dir().ok_or("no config folder for the watermarks")?)
}

/// Every saved watermark, logos as full paths.
fn all() -> Res<Vec<SavedWatermark>> {
    Ok(watermarks::list(&dir()?)
        .into_iter()
        .map(|(_, s)| s)
        .collect())
}

#[tauri::command]
pub(crate) fn watermarks() -> Res<Vec<SavedWatermark>> {
    all()
}

#[tauri::command]
pub(crate) fn save_watermark(name: String, mark: Watermark) -> Res<Vec<SavedWatermark>> {
    watermarks::save(&dir()?, &name, &mark)?;
    all()
}

#[tauri::command]
pub(crate) fn delete_watermark(name: String) -> Res<Vec<SavedWatermark>> {
    watermarks::delete(&dir()?, &name)?;
    all()
}

/// The open photo with `mark` on it, as the export would make it, as JPEG.
#[tauri::command]
pub(crate) async fn watermark_preview(
    mark: Watermark,
    state: tauri::State<'_, AppState>,
) -> Res<Response> {
    use amb_export::{ExportSettings, Format, MetadataPolicy, Resize};
    mark.validate()?;
    let slot = state.session.clone();
    let lens = state.lens.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut s = ExportSettings::new(Format::Jpeg);
        s.quality = PREVIEW_QUALITY;
        s.resize = Resize::LongEdge(PREVIEW_EDGE);
        s.metadata = MetadataPolicy::None;
        s.watermark = Some(mark);
        let out =
            std::env::temp_dir().join(format!("amb-watermark-preview-{}.jpg", std::process::id()));
        let result = {
            let mut guard = slot.lock()?;
            let session = guard.as_mut().ok_or("no photo open")?;
            let lens = lens.get().unwrap_or_else(|| empty_lens());
            // Read and remove the file under the lock, so two previews in
            // flight can't take each other's file away.
            let done = session.export(&out, &s, lens);
            let bytes = std::fs::read(&out);
            let _ = std::fs::remove_file(&out);
            done.map(|_| bytes)
        };
        Ok(Response::new(result?.map_err(|e| e.to_string())?))
    })
    .await?
}
