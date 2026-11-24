// SPDX-License-Identifier: GPL-3.0-or-later
//! The darkroom aids that print the open photo several ways on one sheet:
//! the test strip and the ring-around, and placing a spot in a zone
//! (`amb_ops::darkroom`).

use crate::*;

#[derive(Serialize)]
pub(crate) struct AidSheet {
    /// The sheet as a `data:` URL.
    pub(crate) image: String,
    pub(crate) aid: amb_ops::darkroom::Aid,
}

/// A test strip (`kind` "strip") or a ring-around ("ring") of the open
/// photo with its edit as it is now. Progress goes out as
/// "darkroom-progress" [done, total].
#[tauri::command]
pub(crate) async fn darkroom_aid(
    kind: String,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Res<AidSheet> {
    let (photo, settings) = {
        let guard = state.session.lock()?;
        let s = guard.as_ref().ok_or("open a photo first")?;
        (s.path.clone(), s.state.settings().clone())
    };
    tauri::async_runtime::spawn_blocking(move || {
        use amb_ops::darkroom::{RingOptions, StripOptions, ring_around, test_strip};
        let progress = crate::progress::throttled(&app, "darkroom-progress");
        let aid = match kind.as_str() {
            "strip" => test_strip(&photo, &settings, &StripOptions::default(), &progress)?,
            "ring" => ring_around(&photo, &settings, &RingOptions::default(), &progress)?,
            other => return Err(format!("unknown darkroom aid {other:?} (strip, ring)")),
        };
        let image = format!("data:image/png;base64,{}", plugins::base64(&aid.png()?));
        Ok(AidSheet { image, aid })
    })
    .await?
    .map_err(Into::into)
}

/// The exposure that prints the spot `at` (fractions of the photo as shown)
/// in `zone` (0 to 10), from the open photo's edit as it is now. Nothing is
/// changed: the page applies `changes` as one undo step. Progress goes out
/// as "darkroom-progress" [done, total].
#[tauri::command]
pub(crate) async fn darkroom_place(
    at: [f64; 2],
    zone: u8,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Res<amb_ops::darkroom::Placed> {
    let (photo, settings) = {
        let guard = state.session.lock()?;
        let s = guard.as_ref().ok_or("open a photo first")?;
        (s.path.clone(), s.state.settings().clone())
    };
    tauri::async_runtime::spawn_blocking(move || {
        let progress = crate::progress::throttled(&app, "darkroom-progress");
        let o = amb_ops::darkroom::PlaceOptions {
            at,
            zone,
            ..Default::default()
        };
        amb_ops::darkroom::place_in_zone(&photo, &settings, &o, &progress)
    })
    .await?
    .map_err(Into::into)
}
