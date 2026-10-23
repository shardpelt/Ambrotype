// SPDX-License-Identifier: GPL-3.0-or-later
//! Places: where photos were taken, for the library's map. Set by hand
//! (dropped on the map) or from a GPX track; kept in the sidecars as
//! Lightroom keeps them (`exif:GPSLatitude`, `exif:GPSLongitude`).

use crate::*;
use commands::library::{InfoUpdate, change_one};

#[tauri::command]
pub(crate) async fn pick_gpx(app: tauri::AppHandle) -> Option<String> {
    app.dialog()
        .file()
        .set_title("Choose a GPS track")
        .add_filter("GPS tracks", &["gpx", "GPX"])
        .blocking_pick_file()
        .and_then(|p| p.into_path().ok())
        .map(|p| p.display().to_string())
}

/// OpenStreetMap's copyright page, which its map images ask to link to.
#[tauri::command]
pub(crate) fn open_osm_copyright(app: tauri::AppHandle) -> Res<()> {
    app.opener()
        .open_url("https://www.openstreetmap.org/copyright", None::<&str>)
        .map_err(|e| e.to_string())
}

/// What matching a track did.
#[derive(Serialize)]
pub(crate) struct GpxMatch {
    pub(crate) updates: Vec<InfoUpdate>,
    /// Taken while the track wasn't recording, or with no capture time.
    pub(crate) unplaced: usize,
    /// Already had a place, and `replace` wasn't asked for.
    pub(crate) kept: usize,
    /// The track's first and last times (UTC, "YYYY-MM-DD HH:MM").
    pub(crate) track: [String; 2],
}

/// Place `paths` from the GPX file `gpx`, the camera clock being
/// `offset_minutes` ahead of UTC. Photos with a place keep it unless
/// `replace`.
#[tauri::command]
pub(crate) async fn library_match_gpx(
    gpx: String,
    paths: Vec<String>,
    offset_minutes: i64,
    replace: bool,
    state: tauri::State<'_, AppState>,
) -> Res<GpxMatch> {
    let slot = state.session.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let text = std::fs::read_to_string(&gpx).map_err(|e| format!("{gpx}: {e}"))?;
        let track = amb_library::gpx::parse(&text)?;
        let mut out = GpxMatch {
            updates: Vec::new(),
            unplaced: 0,
            kept: 0,
            track: [track.points[0].0, track.points[track.points.len() - 1].0].map(utc_text),
        };
        let records =
            folder::catalog().records(&paths.iter().map(PathBuf::from).collect::<Vec<_>>());
        for (path, record) in paths.into_iter().zip(records) {
            if !replace && record.place().is_some() {
                out.kept += 1;
                continue;
            }
            let at = record.shot.captured.as_deref();
            let Some((lat, lon)) = at.and_then(|c| track.place(c, offset_minutes)) else {
                out.unplaced += 1;
                continue;
            };
            let change = library::InfoChange {
                gps: Some(format!("{lat:.6},{lon:.6}")),
                ..Default::default()
            };
            let info = change_one(&slot, &path, &change)?;
            out.updates.push(InfoUpdate { path, info });
        }
        Ok(out)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// UTC seconds as "YYYY-MM-DD HH:MM".
fn utc_text(secs: i64) -> String {
    let days = secs.div_euclid(86400);
    let rem = secs.rem_euclid(86400);
    // Civil date from days since 1970 (the inverse of days_from_civil).
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}",
        rem / 3600,
        rem % 3600 / 60
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn utc_text_reads_back() {
        let s = amb_library::gpx::local_seconds("2026-02-28 23:59:00").unwrap();
        assert_eq!(super::utc_text(s), "2026-02-28 23:59");
        assert_eq!(super::utc_text(s + 120), "2026-03-01 00:01");
    }
}
