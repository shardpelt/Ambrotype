// SPDX-License-Identifier: GPL-3.0-or-later
//! Select Object, Remove object, dust and spot finding, healing lines.

use crate::*;
use commands::photo::Executed;

/// A donor for a new spot (the spot tool shows it, and the user can move it).
#[tauri::command]
pub(crate) async fn spot_source(
    x: f64,
    y: f64,
    radius: f64,
    points: Option<Vec<[f64; 2]>>,
    feather: Option<f64>,
    state: tauri::State<'_, AppState>,
) -> Res<[f64; 2]> {
    let slot = state.session.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let guard = slot.lock().map_err(|e| e.to_string())?;
        let session = guard.as_ref().ok_or("no photo open")?;
        // A brushed spot: searched for the stroke's own shape.
        let (sx, sy) = match &points {
            Some(p) => session.stroke_source(p, radius, feather.unwrap_or(50.0))?,
            None => session.spot_source(x, y, radius),
        };
        Ok([sx, sy])
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Find dust and hairs on the scan and heal them, as one undoable step.
#[tauri::command]
pub(crate) async fn find_dust(state: tauri::State<'_, AppState>) -> Res<Executed> {
    let slot = state.session.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut guard = slot.lock().map_err(|e| e.to_string())?;
        let s = guard.as_mut().ok_or("no photo open")?;
        let found = s.find_dust();
        if !found.commands.is_empty() {
            s.execute(&found.commands, Some("dust and hairs"))?;
        }
        let note = match (found.specks, found.hairs) {
            (0, 0) if found.covered > 0 => "No new dust: spots already cover what was found.".to_string(),
            (0, 0) => "No dust found. Faint specks, busy areas and long straight scratches are left to you.".to_string(),
            (specks, hairs) => format!(
                "Healed {specks} speck{} and {hairs} hair{}. Click a spot to change or delete it.",
                if specks == 1 { "" } else { "s" },
                if hairs == 1 { "" } else { "s" }
            ),
        };
        Ok(Executed {
            state: s.state_info(),
            notes: vec![note],
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Remove object: the object at `point` ([x, y]) or in `rect` ([x0, y0,
/// x1, y1]; frame fractions, as spots are) is outlined, and one Remove
/// spot covers all of it, as one step.
#[tauri::command]
pub(crate) async fn remove_object(
    rect: Option<[f64; 4]>,
    point: Option<[f64; 2]>,
    part: Option<bool>,
    state: tauri::State<'_, AppState>,
) -> Res<Executed> {
    use amb_ops::object::Target;
    let target = match (point, rect) {
        (Some(p), _) if part == Some(true) => Target::Part(p),
        (Some(p), _) => Target::Click(p),
        (None, Some(r)) => Target::Box(r),
        (None, None) => return Err("click the object, or drag a box round it".into()),
    };
    let slot = state.session.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut guard = slot.lock().map_err(|e| e.to_string())?;
        let s = guard.as_mut().ok_or("no photo open")?;
        let found = s.remove_object(target.clone())?;
        s.execute(&[found.command], Some("remove object"))?;
        let what = match target {
            Target::Click(_) | Target::Part(_) | Target::Clicks(_) => {
                format!("{:.0} % of the photo", found.share * 100.0)
            }
            Target::Box(_) => format!("{:.0} % of the box", found.share * 100.0),
        };
        let tip = if found.selected {
            "Not what you meant? Undo, then Alt-click for just a part, or drag a box round it."
        } else {
            "Not all of it? Undo and draw the box closer, or brush over the rest. Settings → AI models → SAM 2.1 tiny selects by a click, more exactly."
        };
        Ok(Executed {
            state: s.state_info(),
            notes: vec![format!("Found the object ({what}) and removed it in one spot. {tip}")],
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// What Select Object found: dabs to draw its outline with (`[x, y,
/// radius]`, frame fractions, the radius of the width) and how much of the
/// photo it is.
#[derive(serde::Serialize)]
pub(crate) struct Selected {
    dabs: Vec<[f64; 3]>,
    share: f64,
    centre: [f64; 2],
    /// Its shadow, when one was found: its dabs (shown, to offer it), its
    /// size as a share of the object's, and whether it is taken too.
    shadow_dabs: Vec<[f64; 3]>,
    shadow: f64,
    with_shadow: bool,
}

impl Selected {
    fn of(sel: &amb_ops::object::Selection) -> Self {
        Selected {
            dabs: sel.mask_dabs(),
            share: sel.share,
            centre: sel.centre(),
            shadow_dabs: sel.shadow_dabs(),
            shadow: sel.shadow_share(),
            with_shadow: sel.with_shadow(),
        }
    }
}

/// Select Object: the object under `clicks` (`[x, y, 1]` on it, `[x, y, 0]`
/// on what isn't; frame fractions), or `part` for just the part under one
/// click, or in `rect`; with its `shadow` too. Kept for `selection_apply`.
#[tauri::command]
pub(crate) async fn select_object(
    clicks: Vec<[f64; 3]>,
    part: Option<bool>,
    rect: Option<[f64; 4]>,
    shadow: Option<bool>,
    state: tauri::State<'_, AppState>,
) -> Res<Selected> {
    use amb_ops::object::Target;
    let target = match (rect, clicks.as_slice()) {
        (Some(r), _) => Target::Box(r),
        (None, [one]) if part == Some(true) => Target::Part([one[0], one[1]]),
        (None, []) => return Err("click on the object".into()),
        (None, list) => Target::Clicks(list.iter().map(|c| ([c[0], c[1]], c[2] > 0.5)).collect()),
    };
    let slot = state.session.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut guard = slot.lock().map_err(|e| e.to_string())?;
        let s = guard.as_mut().ok_or("no photo open")?;
        Ok(Selected::of(
            s.select_object(&target, shadow == Some(true))?,
        ))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Select Object by words ("car", "the boat"): the thing in the photo the
/// search model finds most like them, outlined by the object selector
/// (amb_ops::object::select_words). Kept for `selection_apply`.
#[tauri::command]
pub(crate) async fn select_words(
    words: String,
    state: tauri::State<'_, AppState>,
) -> Res<Selected> {
    let dir = crate::commands::models::models_dir(state.inner())?;
    let slot = state.session.clone();
    tauri::async_runtime::spawn_blocking(move || {
        if words.trim().is_empty() {
            return Err("say what to select: a car, a dog, the boat".into());
        }
        let model = crate::commands::models::search_model(&dir, "Selecting by words")?;
        let found = model.words(&words)?;
        let like =
            |rgb: &[f32], w: usize, h: usize| model.image(rgb, w, h).map(|v| found.score(&v));
        let mut guard = slot.lock().map_err(|e| e.to_string())?;
        let s = guard.as_mut().ok_or("no photo open")?;
        Ok(Selected::of(s.select_words(&like)?))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Take the selected object's shadow with it, or not.
#[tauri::command]
pub(crate) async fn selection_shadow(on: bool, state: tauri::State<'_, AppState>) -> Res<Selected> {
    let slot = state.session.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut guard = slot.lock().map_err(|e| e.to_string())?;
        let s = guard.as_mut().ok_or("no photo open")?;
        let sel = s.selection.as_mut().ok_or("select an object first")?;
        sel.set_shadow(on);
        Ok(Selected::of(sel))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Do something with the selected object, as one step: `remove` it (one
/// AI Remove spot), `move` it `by` [dx, dy], or make a mask of it
/// (`adjust`, `brighten`, `darken`; `background` for everything else,
/// softened). Returns the new state; a mask is the last one.
#[tauri::command]
pub(crate) async fn selection_apply(
    action: String,
    by: Option<[f64; 2]>,
    state: tauri::State<'_, AppState>,
) -> Res<Executed> {
    let slot = state.session.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut guard = slot.lock().map_err(|e| e.to_string())?;
        let s = guard.as_mut().ok_or("no photo open")?;
        let sel = s.selection.clone().ok_or("select an object first")?;
        let masks_now = amb_protocol::masks::describe(s.state.settings())["masks"]
            .as_array()
            .map_or(0, Vec::len);
        let mask_of = |name: &str, adjust: serde_json::Value, invert: bool| -> Vec<Command> {
            let mut cmds = vec![Command::PaintMask {
                mask: None,
                part: None,
                mode: None,
                points: Vec::new(),
                size: 0.05,
                feather: 20.0,
                flow: 100.0,
                density: 100.0,
                erase: false,
                dabs: Some(sel.mask_dabs()),
            }];
            let mut values = adjust.as_object().cloned().unwrap_or_default();
            values.insert("name".into(), name.into());
            cmds.push(Command::SetMask {
                mask: masks_now,
                part: None,
                values,
            });
            if invert {
                cmds.push(Command::SetMask {
                    mask: masks_now,
                    part: Some(0),
                    values: serde_json::json!({ "invert": true })
                        .as_object()
                        .cloned()
                        .unwrap_or_default(),
                });
            }
            cmds
        };
        let (cmds, label, note): (Vec<Command>, &str, String) = match action.as_str() {
            "remove" => (
                vec![sel.remove().command],
                "remove object",
                "Removed it in one spot.".into(),
            ),
            "move" => {
                let by = by.ok_or("where to: drag the outline")?;
                (
                    sel.move_by(by),
                    "move object",
                    "Moved it: copied to the new place, removed where it was.".into(),
                )
            }
            "copy" => {
                let by = by.ok_or("where to: drag the outline")?;
                (
                    sel.copy_by(by),
                    "copy object",
                    "Copied it to the new place; it stays where it was too.".into(),
                )
            }
            "adjust" => (
                mask_of("Object", serde_json::json!({}), false),
                "object mask",
                "A mask of it: its sliders are in Masks.".into(),
            ),
            "brighten" => (
                mask_of(
                    "Object, brighter",
                    serde_json::json!({ "Exposure": 0.5 }),
                    false,
                ),
                "brighten object",
                "Brighter (+0.5 EV), as a mask you can change in Masks.".into(),
            ),
            "darken" => (
                mask_of(
                    "Object, darker",
                    serde_json::json!({ "Exposure": -0.5 }),
                    false,
                ),
                "darken object",
                "Darker (−0.5 EV), as a mask you can change in Masks.".into(),
            ),
            "background" => (
                mask_of(
                    "Background, softened",
                    serde_json::json!({ "Sharpness": -80, "Clarity": -40 }),
                    true,
                ),
                "soften background",
                "Everything but it, softened, as a mask you can change in Masks.".into(),
            ),
            other => return Err(format!("unknown action {other:?}")),
        };
        s.execute(&cmds, Some(label))?;
        Ok(Executed {
            state: s.state_info(),
            notes: vec![note],
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Heal a straight scratch along a line drawn on the photo, as one step.
#[tauri::command]
pub(crate) async fn heal_line(
    from: [f64; 2],
    to: [f64; 2],
    state: tauri::State<'_, AppState>,
) -> Res<Executed> {
    let slot = state.session.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut guard = slot.lock().map_err(|e| e.to_string())?;
        let s = guard.as_mut().ok_or("no photo open")?;
        let cmds = s.scratch_spots(from, to);
        s.execute(&cmds, Some("scratch"))?;
        Ok(Executed {
            state: s.state_info(),
            notes: Vec::new(),
        })
    })
    .await
    .map_err(|e| e.to_string())?
}
