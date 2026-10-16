// SPDX-License-Identifier: GPL-3.0-or-later
//! Rendering the preview and the 1:1 view.

use crate::*;

#[derive(Serialize)]
pub(crate) struct PreviewMeta {
    pub(crate) width: usize,
    pub(crate) height: usize,
    pub(crate) histogram: [Vec<u32>; 3],
    pub(crate) millis: u128,
    pub(crate) lens_profile: Option<String>,
    /// The GPU's name, or why this preview was rendered on the CPU.
    pub(crate) gpu: Option<String>,
    pub(crate) cpu_reason: Option<String>,
}

/// Preview JPEG with a JSON header: `[u32 LE header length][header JSON][JPEG]`.
#[tauri::command]
pub(crate) async fn preview(
    max_edge: usize,
    overrides: BTreeMap<String, serde_json::Value>,
    before: bool,
    clipping: bool,
    mask_overlay: Option<usize>,
    state: tauri::State<'_, AppState>,
) -> Res<Response> {
    let slot = state.session.clone();
    let lens = state.lens.clone();
    let gpu = state.gpu.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let overrides: BTreeMap<String, Value> = overrides
            .iter()
            .filter_map(|(k, v)| Some((k.clone(), to_value(v)?)))
            .collect();
        let mut guard = slot.lock().map_err(|e| e.to_string())?;
        let s = guard.as_mut().ok_or("no photo open")?;
        let lens = lens.get().unwrap_or_else(|| empty_lens());
        let gpu = gpu.get().and_then(Option::as_ref);
        let p = s.preview_masked(
            max_edge,
            &overrides,
            before,
            clipping,
            lens,
            mask_overlay,
            gpu,
        );
        let meta = serde_json::to_vec(&PreviewMeta {
            width: p.width,
            height: p.height,
            histogram: p.histogram,
            millis: p.millis,
            lens_profile: p.lens_profile,
            gpu: p.gpu.as_ref().and_then(|g| g.as_ref().ok().cloned()),
            cpu_reason: p.gpu.and_then(Result::err),
        })
        .map_err(|e| e.to_string())?;
        let mut out = Vec::with_capacity(4 + meta.len() + p.jpeg.len());
        out.extend_from_slice(&(meta.len() as u32).to_le_bytes());
        out.extend_from_slice(&meta);
        out.extend_from_slice(&p.jpeg);
        Ok(Response::new(out))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Serialize)]
pub(crate) struct RegionMeta {
    pub(crate) x: usize,
    pub(crate) y: usize,
    pub(crate) width: usize,
    pub(crate) height: usize,
    pub(crate) full_width: usize,
    pub(crate) full_height: usize,
    pub(crate) millis: u128,
}

/// A reduced 1:1 crop with uncommitted slider changes, for the 100% view
/// while a slider moves (framed like `region`).
#[tauri::command]
pub(crate) async fn region_draft(
    cx: f64,
    cy: f64,
    width: usize,
    height: usize,
    overrides: BTreeMap<String, serde_json::Value>,
    state: tauri::State<'_, AppState>,
) -> Res<Response> {
    let slot = state.session.clone();
    let lens = state.lens.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let overrides: BTreeMap<String, Value> = overrides
            .iter()
            .filter_map(|(k, v)| Some((k.clone(), to_value(v)?)))
            .collect();
        let mut guard = slot.lock().map_err(|e| e.to_string())?;
        let s = guard.as_mut().ok_or("no photo open")?;
        let lens = lens.get().unwrap_or_else(|| empty_lens());
        let r = s.region_draft(cx, cy, width, height, &overrides, lens);
        framed_region(&r)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// A region as the page reads it: the length of its JSON facts, the facts,
/// then the JPEG.
pub(crate) fn framed_region(r: &session::Region) -> Res<Response> {
    let meta = serde_json::to_vec(&RegionMeta {
        x: r.x,
        y: r.y,
        width: r.width,
        height: r.height,
        full_width: r.full_width,
        full_height: r.full_height,
        millis: r.millis,
    })
    .map_err(|e| e.to_string())?;
    let mut out = Vec::with_capacity(4 + meta.len() + r.jpeg.len());
    out.extend_from_slice(&(meta.len() as u32).to_le_bytes());
    out.extend_from_slice(&meta);
    out.extend_from_slice(&r.jpeg);
    Ok(Response::new(out))
}

/// 1:1 crop for the 100% view, framed like `preview`: `[u32 LE len][JSON][JPEG]`.
#[tauri::command]
pub(crate) async fn region(
    cx: f64,
    cy: f64,
    width: usize,
    height: usize,
    before: bool,
    clipping: bool,
    state: tauri::State<'_, AppState>,
) -> Res<Response> {
    let slot = state.session.clone();
    let lens = state.lens.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut guard = slot.lock().map_err(|e| e.to_string())?;
        let s = guard.as_mut().ok_or("no photo open")?;
        let lens = lens.get().unwrap_or_else(|| empty_lens());
        let r = s.region(cx, cy, width, height, before, clipping, lens);
        framed_region(&r)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Soft proofing: show the photo as it exports to `space` (an output space
/// id), with the colours beyond it marked when `warn`; `None` turns it off.
#[tauri::command]
pub(crate) fn set_proof(
    space: Option<String>,
    warn: bool,
    state: tauri::State<'_, AppState>,
) -> Res<()> {
    let proof = match space {
        Some(id) => Some((
            amb_color::space::OutputSpace::from_id(&id)
                .ok_or(format!("unknown colour space {id}"))?,
            warn,
        )),
        None => None,
    };
    *state.proof.lock().map_err(|e| e.to_string())? = proof;
    if let Some(s) = state.session.lock().map_err(|e| e.to_string())?.as_mut() {
        s.set_proof(proof);
    }
    Ok(())
}
