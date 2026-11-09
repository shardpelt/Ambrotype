// SPDX-License-Identifier: GPL-3.0-or-later
//! The crop, mask, white balance, upright and negative tools.

use crate::*;
use commands::photo::Executed;

/// The committed crop, upright on the uncropped photo (the crop tool's start).
#[tauri::command]
pub(crate) fn crop_state(state: tauri::State<'_, AppState>) -> Res<CropState> {
    let guard = state.session.lock().map_err(|e| e.to_string())?;
    Ok(guard.as_ref().ok_or("no photo open")?.crop_state())
}

/// The mask tool's map from the shown photo to mask coordinates.
#[tauri::command]
pub(crate) fn mask_frame(state: tauri::State<'_, AppState>) -> Res<session::MaskFrame> {
    let guard = state.session.lock().map_err(|e| e.to_string())?;
    Ok(guard.as_ref().ok_or("no photo open")?.mask_frame())
}

#[derive(Serialize)]
pub(crate) struct MaskSlider {
    pub(crate) tag: &'static str,
    pub(crate) label: &'static str,
    pub(crate) min: f64,
    pub(crate) max: f64,
    pub(crate) step: f64,
    pub(crate) section: &'static str,
    pub(crate) rendered: bool,
}

/// The masks' local sliders, in panel order (UI units).
#[tauri::command]
pub(crate) fn mask_sliders() -> Vec<MaskSlider> {
    amb_presets::masks::LOCAL_SLIDERS
        .iter()
        .map(|s| MaskSlider {
            tag: s.tag,
            label: s.label,
            min: s.min,
            max: s.max,
            step: s.step,
            section: s.section,
            rendered: s.rendered,
        })
        .collect()
}

/// The protocol commands that set the crop to `rect`; the UI runs them as one
/// undo step.
#[tauri::command]
pub(crate) fn crop_commands(
    rect: CropRect,
    state: tauri::State<'_, AppState>,
) -> Res<Vec<Command>> {
    let guard = state.session.lock().map_err(|e| e.to_string())?;
    Ok(guard.as_ref().ok_or("no photo open")?.crop_commands(&rect))
}

/// White balance from a clicked neutral spot: sets WB to Custom with the
/// picked Temperature/Tint (one undoable step).
#[tauri::command]
pub(crate) fn pick_white(u: f64, v: f64, state: tauri::State<'_, AppState>) -> Res<Executed> {
    let mut guard = state.session.lock().map_err(|e| e.to_string())?;
    let s = guard.as_mut().ok_or("no photo open")?;
    let [(temp_name, t), (tint_name, tint)] = s
        .pick_white(u, v)
        .ok_or("pick a neutral grey or white area that is not clipped")?;
    s.execute(
        &[
            Command::SetParam {
                name: "WhiteBalance".into(),
                value: serde_json::json!("Custom"),
            },
            Command::SetParam {
                name: temp_name.into(),
                value: serde_json::json!(t),
            },
            Command::SetParam {
                name: tint_name.into(),
                value: serde_json::json!(tint),
            },
        ],
        Some("white balance picker"),
    )?;
    let note = if temp_name == "Temperature" {
        format!("White balance set to {t:.0} K, tint {tint:+.0}")
    } else {
        format!("White balance set to temp {t:+.0}, tint {tint:+.0}")
    };
    Ok(Executed {
        state: s.state_info(),
        notes: vec![note],
    })
}

/// Film base from a click on the clear film between frames: stores it (and
/// turns the conversion on) as one undoable step.
#[tauri::command]
pub(crate) fn pick_film_base(u: f64, v: f64, state: tauri::State<'_, AppState>) -> Res<Executed> {
    let mut guard = state.session.lock().map_err(|e| e.to_string())?;
    let s = guard.as_mut().ok_or("no photo open")?;
    let (base, warning) = s
        .pick_film_base(u, v)
        .ok_or("pick the clear film between frames: that spot is clipped or black")?;
    let round = |v: f64| (v * 10_000.0).round() / 10_000.0;
    let mut cmds = vec![Command::SetParam {
        name: "amb:NegativeConvert".into(),
        value: serde_json::json!(true),
    }];
    for (c, name) in ["Red", "Green", "Blue"].iter().enumerate() {
        cmds.push(Command::SetParam {
            name: format!("amb:FilmBase{name}"),
            value: serde_json::json!(round(base[c])),
        });
    }
    s.execute(&cmds, Some("film base picker"))?;
    Ok(Executed {
        state: s.state_info(),
        notes: vec![warning.unwrap_or_else(|| {
            "Film base set. Alt-click Pick base to measure it from the scan again.".into()
        })],
    })
}

#[derive(Serialize)]
pub(crate) struct Guided {
    pub(crate) commands: Vec<Command>,
    /// How far the worst guide is still off, degrees.
    pub(crate) worst_degrees: f64,
}

/// Guided Upright for the open photo: the Transform sliders that straighten
/// `guides` (fractions of the upright, uncropped photo).
#[tauri::command]
pub(crate) fn guided_upright(
    guides: Vec<[f64; 4]>,
    state: tauri::State<'_, AppState>,
) -> Res<Guided> {
    let guard = state.session.lock().map_err(|e| e.to_string())?;
    let s = guard.as_ref().ok_or("no photo open")?;
    let (commands, worst_degrees) = s.guided_upright(&guides)?;
    Ok(Guided {
        commands,
        worst_degrees,
    })
}

/// The guides Guided Upright kept for the open photo.
#[tauri::command]
pub(crate) fn upright_guides(state: tauri::State<'_, AppState>) -> Res<Vec<[f64; 4]>> {
    let guard = state.session.lock().map_err(|e| e.to_string())?;
    Ok(guard
        .as_ref()
        .map(Session::upright_guides)
        .unwrap_or_default())
}

/// The open photo's film negative analysis as commands that store it with
/// Auto off ("Freeze").
#[tauri::command]
pub(crate) async fn negative_freeze(state: tauri::State<'_, AppState>) -> Res<Vec<Command>> {
    let slot = state.session.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let guard = slot.lock().map_err(|e| e.to_string())?;
        let s = guard.as_ref().ok_or("no photo open")?;
        let frozen = amb_ops::negative::frozen(&[s.negative_analysis()]);
        Ok(amb_ops::negative::commands(&frozen))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The crop tool's Level: the straighten angle that levels the open photo;
/// `None` when it has too few long lines to tell.
#[tauri::command]
pub(crate) async fn level_angle(state: tauri::State<'_, AppState>) -> Res<Option<f64>> {
    let slot = state.session.clone();
    let lens = state.lens.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut guard = slot.lock().map_err(|e| e.to_string())?;
        let s = guard.as_mut().ok_or("no photo open")?;
        let lens = lens.get().unwrap_or_else(|| empty_lens());
        Ok(s.level_angle(lens))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The open photo's long straight lines, for the crop tool's guides.
#[tauri::command]
pub(crate) async fn guide_lines(state: tauri::State<'_, AppState>) -> Res<Vec<[f64; 4]>> {
    let slot = state.session.clone();
    let lens = state.lens.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut guard = slot.lock().map_err(|e| e.to_string())?;
        let s = guard.as_mut().ok_or("no photo open")?;
        let lens = lens.get().unwrap_or_else(|| empty_lens());
        Ok(s.guide_lines(lens))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Crop the open scan to its frame; `None` when no border was found.
#[tauri::command]
pub(crate) async fn negative_frame(state: tauri::State<'_, AppState>) -> Res<Option<Vec<Command>>> {
    let slot = state.session.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let guard = slot.lock().map_err(|e| e.to_string())?;
        Ok(guard.as_ref().ok_or("no photo open")?.negative_frame())
    })
    .await
    .map_err(|e| e.to_string())?
}
