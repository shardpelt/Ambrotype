// SPDX-License-Identifier: GPL-3.0-or-later
// The photo on screen: the preview render loop, the histogram, the 1:1
// view and panning, the white-balance picker, soft proofing and clipping.
"use strict";

function previewSize(draft) {
  const v = $("viewer");
  const px = Math.max(v.clientWidth, v.clientHeight) * (window.devicePixelRatio || 1);
  const full = Math.min(2560, Math.max(512, Math.ceil(px)));
  return draft ? Math.max(512, Math.round(full * 0.6)) : full;
}

function requestPreview(draft) {
  if (!ui.info) return;
  ui.preview.dirty = true;
  ui.preview.draft = draft;
  if (!ui.preview.inFlight) renderLoop();
}

async function renderLoop() {
  ui.preview.inFlight = true;
  $("busy").hidden = false;
  while (ui.preview.dirty) {
    ui.preview.dirty = false;
    const draft = ui.preview.draft;
    const uncropped = ui.crop.on || ui.before || gu.on;
    try {
      if (ui.zoom.on) {
        // At 100%, a slider being dragged shows on a reduced copy, enlarged;
        // the full-size crop follows when it's released.
        if (draft) await renderRegionDraft();
        else await renderRegion();
        continue;
      }
      const overrides = ui.before ? {} : { ...ui.overrides };
      const maskOverlay = ui.before ? null : maskOverlayIndex();
      // The crop tool draws its rectangle on the whole photo.
      if (ui.crop.on) overrides.HasCrop = false;
      Object.assign(overrides, guidedOverrides());
      const buf = await invoke("preview", {
        maxEdge: previewSize(draft),
        overrides,
        before: ui.before,
        clipping: ui.clipping,
        maskOverlay,
      });
      const { meta, jpeg } = splitFramed(buf);
      // A draft is smaller than the viewer; keep the image at its settled size
      // so the photo doesn't shrink while dragging.
      const img = $("preview");
      if (draft && !img.hidden && !img.style.width) {
        const r = img.getBoundingClientRect();
        img.style.width = `${r.width}px`;
        img.style.height = `${r.height}px`;
      } else if (!draft) {
        img.style.width = img.style.height = "";
      }
      showJpeg(jpeg);
      ui.preview.uncropped = uncropped;
      ui.preview.overlay = maskOverlay != null;
      drawHistogram(meta.histogram);
      ui.curve?.setHistogram(meta.histogram);
      status(`${meta.width}×${meta.height} preview in ${meta.millis} ms` +
        (meta.gpu ? " on the GPU" : "") +
        (meta.lens_profile ? `  ·  Lens: ${meta.lens_profile}` : ""));
      $("status").title = meta.gpu ? `Developed on ${meta.gpu}` :
        meta.cpu_reason ? `Developed on the CPU: ${meta.cpu_reason}` : "Developed on the CPU (no GPU found)";
    } catch (e) {
      toast(String(e), true);
    }
  }
  $("busy").hidden = true;
  ui.preview.inFlight = false;
}

function drawHistogram(h) {
  ui.histogram = h;
  const c = $("histogram");
  const dpr = window.devicePixelRatio || 1;
  c.width = c.clientWidth * dpr;
  c.height = c.clientHeight * dpr;
  const g = c.getContext("2d");
  g.clearRect(0, 0, c.width, c.height);
  const max = Math.max(1, ...h.flatMap((ch) => ch.slice(2, 254)));
  g.globalCompositeOperation = "lighter";
  const colours = ["rgba(230,70,70,0.75)", "rgba(80,200,90,0.75)", "rgba(80,120,240,0.75)"];
  h.forEach((ch, i) => {
    g.fillStyle = colours[i];
    g.beginPath();
    g.moveTo(0, c.height);
    ch.forEach((n, x) => g.lineTo((x / 255) * c.width, c.height - Math.min(1, n / max) * c.height * 0.95));
    g.lineTo(c.width, c.height);
    g.fill();
  });
  g.globalCompositeOperation = "source-over";
}

// ---------------------------------------------------------------- 100% view, pickers

function splitFramed(buf) {
  const bytes = new Uint8Array(buf);
  const len = new DataView(bytes.buffer, bytes.byteOffset, 4).getUint32(0, true);
  const meta = JSON.parse(new TextDecoder().decode(bytes.subarray(4, 4 + len)));
  return { meta, jpeg: bytes.subarray(4 + len) };
}

function showJpeg(jpeg) {
  const url = URL.createObjectURL(new Blob([jpeg], { type: "image/jpeg" }));
  const img = $("preview");
  img.src = url;
  img.hidden = ui.crop.on;
  if (ui.preview.url) URL.revokeObjectURL(ui.preview.url);
  ui.preview.url = url;
}

// 1:1 crop of the full-size render around (cx, cy); 1.5× the viewport so a
// short pan doesn't show empty space before the next crop arrives.
async function renderRegion() {
  const v = $("viewer");
  const dpr = window.devicePixelRatio || 1;
  status("Rendering 100%…");
  const { meta, jpeg } = splitFramed(await invoke("region", {
    cx: ui.zoom.cx, cy: ui.zoom.cy,
    width: Math.round(v.clientWidth * dpr * 1.5),
    height: Math.round(v.clientHeight * dpr * 1.5),
    before: ui.before,
    clipping: ui.clipping,
  }));
  // 100% was switched off (or another photo opened) while this rendered.
  if (!ui.zoom.on) {
    requestPreview(false);
    return;
  }
  ui.zoom.meta = meta;
  showJpeg(jpeg);
  $("viewer").classList.add("actual");
  placeRegion();
  status(`100%: ${meta.full_width}×${meta.full_height} in ${meta.millis} ms`);
}

// The 100% crop with the slider being dragged, from a reduced copy.
async function renderRegionDraft() {
  if (!ui.zoom.meta) return;
  const v = $("viewer");
  const dpr = window.devicePixelRatio || 1;
  const { meta, jpeg } = splitFramed(await invoke("region_draft", {
    cx: ui.zoom.cx, cy: ui.zoom.cy,
    width: Math.round(v.clientWidth * dpr * 1.5),
    height: Math.round(v.clientHeight * dpr * 1.5),
    overrides: { ...ui.overrides },
  }));
  if (!ui.zoom.on) return;
  ui.zoom.meta = meta;
  showJpeg(jpeg);
  placeRegion();
  status(`100% draft in ${meta.millis} ms (full detail when the slider is released)`);
}

// Position the 1:1 crop so (cx, cy) sits in the viewport centre.
function placeRegion(dx = pan.dx, dy = pan.dy) {
  const m = ui.zoom.meta;
  if (!m) return;
  const v = $("viewer");
  const dpr = window.devicePixelRatio || 1;
  const img = $("preview");
  img.style.width = `${m.width / dpr}px`;
  img.style.height = `${m.height / dpr}px`;
  const centreX = (ui.zoom.cx * m.full_width - m.x) / dpr;
  const centreY = (ui.zoom.cy * m.full_height - m.y) / dpr;
  img.style.left = `${v.clientWidth / 2 - centreX + dx}px`;
  img.style.top = `${v.clientHeight / 2 - centreY + dy}px`;
}

function setZoom(on, u = 0.5, v = 0.5) {
  ui.zoom.on = on;
  ui.zoom.cx = u;
  ui.zoom.cy = v;
  ui.zoom.meta = null;
  const img = $("preview");
  // The "actual" layout is applied once the 1:1 crop arrives (renderRegion).
  if (!on) $("viewer").classList.remove("actual");
  $("btn-zoom").classList.toggle("active", on);
  if (!on) for (const k of ["width", "height", "left", "top"]) img.style[k] = "";
  requestPreview(false);
}

// Move the 1:1 view by a share of the viewer's size (arrow keys), or by
// pixels (scrolling, a trackpad). The picture moves at once; the new area
// is rendered once the moving stops.
const pan = { dx: 0, dy: 0, timer: null };
function panPixels(dx, dy) {
  const m = ui.zoom.meta;
  if (!ui.zoom.on || !m) return;
  pan.dx -= dx;
  pan.dy -= dy;
  placeRegion(pan.dx, pan.dy);
  clearTimeout(pan.timer);
  pan.timer = setTimeout(() => {
    const dpr = window.devicePixelRatio || 1;
    const meta = ui.zoom.meta;
    ui.zoom.cx = Math.min(1, Math.max(0, ui.zoom.cx - (pan.dx * dpr) / meta.full_width));
    ui.zoom.cy = Math.min(1, Math.max(0, ui.zoom.cy - (pan.dy * dpr) / meta.full_height));
    pan.dx = pan.dy = 0;
    requestPreview(false);
  }, 180);
}

function panZoom(fx, fy) {
  const v = $("viewer");
  panPixels(fx * v.clientWidth, fy * v.clientHeight);
}

// Where a click lands, as fractions (u, v) of the upright photo.
function photoPoint(e) {
  const img = $("preview");
  const r = img.getBoundingClientRect();
  const fx = (e.clientX - r.left) / r.width;
  const fy = (e.clientY - r.top) / r.height;
  if (ui.zoom.on && ui.zoom.meta) {
    const m = ui.zoom.meta;
    return [(m.x + fx * m.width) / m.full_width, (m.y + fy * m.height) / m.full_height];
  }
  return [fx, fy];
}

async function pickWhite(u, v) {
  setPicking(false);
  try {
    const r = await invoke("pick_white", { u, v });
    setState(r.state);
    toast(r.notes.join(" "));
    requestPreview(false);
  } catch (e) {
    toast(String(e), true);
  }
}

function setPicking(on) {
  ui.picking = on && Boolean(ui.info);
  $("viewer").classList.toggle("picking", ui.picking);
  $("btn-wb").classList.toggle("active", ui.picking);
  if (ui.picking) status("Click a neutral grey or white spot (Esc to cancel)");
}

// Soft proofing: the photo as it will export to a colour space (colours
// beyond it clipped as the export clips them), with a gamut warning.
async function toggleProof(on = !ui.proofOn) {
  if (!ui.info && on) return;
  ui.proofOn = on;
  $("proof-bar").hidden = !on;
  await applyProof();
}

async function applyProof() {
  try {
    await invoke("set_proof", {
      space: ui.proofOn ? $("proof-space").value : null,
      warn: $("proof-warn").checked,
    });
  } catch (e) {
    toast(String(e), true);
  }
  requestPreview(false);
}

function toggleClipping() {
  if (!ui.info) return;
  ui.clipping = !ui.clipping;
  $("btn-clip").classList.toggle("active", ui.clipping);
  requestPreview(false);
}

function wireViewer() {
  const img = $("preview");
  let drag = null;
  // The webview's own "Save image" can't save (and would save the small
  // preview): right-click exports the photo instead.
  $("viewer").addEventListener("contextmenu", (e) => {
    e.preventDefault();
    // The tools' own canvases use right-click.
    if (ui.info && (e.target === img || e.target === $("viewer"))) exportFlow();
  });
  // Scrolling (a mouse wheel or two fingers on a trackpad) moves the 1:1 view.
  $("viewer").addEventListener("wheel", (e) => {
    if (!ui.zoom.on || e.ctrlKey) return;
    e.preventDefault();
    const k = e.deltaMode === 1 ? 16 : 1;
    panPixels(e.deltaX * k, e.deltaY * k);
  }, { passive: false });
  img.addEventListener("mousedown", (e) => {
    if (e.button !== 0 || !ui.info) return;
    e.preventDefault();
    drag = { x: e.clientX, y: e.clientY, moved: false };
  });
  window.addEventListener("mousemove", (e) => {
    if (!drag) return;
    const dx = e.clientX - drag.x;
    const dy = e.clientY - drag.y;
    if (Math.abs(dx) + Math.abs(dy) > 3) drag.moved = true;
    if (ui.zoom.on && drag.moved) {
      img.classList.add("panning");
      placeRegion(dx, dy);
    }
  });
  window.addEventListener("mouseup", (e) => {
    if (!drag) return;
    const d = drag;
    drag = null;
    img.classList.remove("panning");
    if (ui.zoom.on && d.moved && ui.zoom.meta) {
      // Move the centre by the drag distance (in full-size pixels) and fetch a new crop.
      const dpr = window.devicePixelRatio || 1;
      const m = ui.zoom.meta;
      ui.zoom.cx = Math.min(1, Math.max(0, ui.zoom.cx - ((e.clientX - d.x) * dpr) / m.full_width));
      ui.zoom.cy = Math.min(1, Math.max(0, ui.zoom.cy - ((e.clientY - d.y) * dpr) / m.full_height));
      requestPreview(false);
      return;
    }
    if (d.moved) return;
    if (!ui.zoom.on && !ui.picking && onFrameBorder(e)) return showFrameSettings(true);
    const [u, v] = photoPoint(e);
    if (ui.picking) pickWhite(u, v);
    else setZoom(!ui.zoom.on, u, v);
  });
}
