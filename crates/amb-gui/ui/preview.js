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
      // The whole photo as it looks, kept to show under the 100% view at once.
      if (!draft && !uncropped && !ui.before && !ui.clipping && maskOverlay == null) {
        if (ui.preview.fitUrl) URL.revokeObjectURL(ui.preview.fitUrl);
        ui.preview.fitUrl = URL.createObjectURL(new Blob([jpeg], { type: "image/jpeg" }));
        ui.preview.fitPath = ui.info.path;
      }
      drawHistogram(meta.histogram);
      ui.curve?.setHistogram(meta.histogram);
      status(`${meta.width}×${meta.height} preview in ${meta.millis} ms` +
        (meta.gpu ? " on the GPU" : "") +
        (meta.lens_profile ? `  ·  Lens: ${meta.lens_profile}` : ""), true);
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

// The whole photo fills the viewer, scaled up too (a small photo, or a
// preview smaller than the screen), keeping its shape.
function fitPreview() {
  const img = $("preview");
  if (ui.zoom.on || !img.naturalWidth) return;
  const v = $("viewer");
  const s = Math.min(v.clientWidth / img.naturalWidth, v.clientHeight / img.naturalHeight);
  const [w, h] = [`${Math.floor(img.naturalWidth * s)}px`, `${Math.floor(img.naturalHeight * s)}px`];
  if (img.style.width !== w) img.style.width = w;
  if (img.style.height !== h) img.style.height = h;
}

function showJpeg(jpeg) {
  const url = URL.createObjectURL(new Blob([jpeg], { type: "image/jpeg" }));
  const img = $("preview");
  img.onload = fitPreview;
  img.src = url;
  img.hidden = ui.crop.on;
  if (ui.preview.url) URL.revokeObjectURL(ui.preview.url);
  ui.preview.url = url;
}

// A crop of the full-size render around (cx, cy), at 100% or a zoom box's
// scale (regionSize).
async function renderRegion() {
  status(`Rendering ${zoomLabel()}…`, true);
  const [width, height] = regionSize();
  const { meta, jpeg } = splitFramed(await invoke("region", {
    cx: ui.zoom.cx, cy: ui.zoom.cy, width, height,
    before: ui.before,
    clipping: ui.clipping,
  }));
  // 100% was switched off (or another photo opened) while this rendered.
  if (!ui.zoom.on) {
    requestPreview(false);
    return;
  }
  ui.zoom.meta = meta;
  ui.zoom.full = { path: ui.info.path, w: meta.full_width, h: meta.full_height };
  $("preview").style.visibility = "";
  // Now the real full size is known (a crop changes it): if that moves the
  // centre much, render again around the new one.
  const [u, v] = clampZoomCentre(ui.zoom.cx, ui.zoom.cy, meta);
  const moved = Math.abs(u - ui.zoom.cx) * meta.full_width > meta.width / 6 || Math.abs(v - ui.zoom.cy) * meta.full_height > meta.height / 6;
  [ui.zoom.cx, ui.zoom.cy] = [u, v];
  showJpeg(jpeg);
  $("viewer").classList.add("actual");
  placeRegion();
  if (moved) requestPreview(false);
  status(`${zoomLabel()}: ${meta.full_width}×${meta.full_height} in ${meta.millis} ms`, true);
}

// The zoomed crop with the slider being dragged, from a reduced copy.
async function renderRegionDraft() {
  if (!ui.zoom.meta) return;
  const [width, height] = regionSize();
  const { meta, jpeg } = splitFramed(await invoke("region_draft", {
    cx: ui.zoom.cx, cy: ui.zoom.cy, width, height,
    overrides: { ...ui.overrides },
  }));
  if (!ui.zoom.on) return;
  ui.zoom.meta = meta;
  showJpeg(jpeg);
  placeRegion();
  status(`${zoomLabel()} draft in ${meta.millis} ms (full detail when the slider is released)`, true);
}

// Screen pixels per full-size pixel: 1 at 100%; a zoom box (right-drag)
// fills the viewer with the box instead. `m` gives the full size, else the
// photo's own size stands in until the first crop arrives.
function zoomScale(m = ui.zoom.meta) {
  const box = ui.zoom.box;
  if (!box) return 1;
  const v = $("viewer");
  const dpr = window.devicePixelRatio || 1;
  const fw = m?.full_width ?? ui.info?.width ?? 1;
  const fh = m?.full_height ?? ui.info?.height ?? 1;
  const s = Math.min((v.clientWidth * dpr) / (box.w * fw), (v.clientHeight * dpr) / (box.h * fh)) * 0.96;
  return Math.min(8, Math.max(0.05, s));
}

const zoomLabel = () => `${Math.round(zoomScale() * 100)}%`;

// The full-size area to render: 1.5× what the viewer shows, so a short pan
// doesn't show empty space; at most 4096 pixels a side.
function regionSize() {
  const v = $("viewer");
  const dpr = window.devicePixelRatio || 1;
  const s = zoomScale();
  return [v.clientWidth, v.clientHeight].map((n) => Math.min(4096, Math.round((n * dpr * 1.5) / s)));
}

// The view's centre (fractions of the photo) kept where the view stays on
// the photo: near an edge it stops at the edge, and a side smaller than the
// viewer stays centred. `m` gives the full size (else the photo's own).
function clampZoomCentre(u, v, m = ui.zoom.meta) {
  const view = $("viewer");
  const k = zoomScale(m) / (window.devicePixelRatio || 1);
  const fw = m?.full_width ?? ui.info?.width ?? 1;
  const fh = m?.full_height ?? ui.info?.height ?? 1;
  const clamp = (c, half) => (half >= 0.5 ? 0.5 : Math.min(1 - half, Math.max(half, c)));
  return [clamp(u, view.clientWidth / 2 / k / fw), clamp(v, view.clientHeight / 2 / k / fh)];
}

// The whole photo enlarged under the 100% view: shown the moment you zoom in,
// and wherever a drag goes past the sharp area until it's rendered.
function placeUnder(dx = 0, dy = 0) {
  const under = $("zoom-under");
  if (under.hidden || !under.naturalWidth) return;
  const m = ui.zoom.meta;
  const known = ui.zoom.full?.path === ui.info?.path ? ui.zoom.full : null;
  const fw = m?.full_width ?? known?.w ?? ui.info.width;
  const fh = m?.full_height ?? known?.h ?? fw * (under.naturalHeight / under.naturalWidth);
  const fake = { full_width: fw, full_height: fh };
  const k = zoomScale(m ?? fake) / (window.devicePixelRatio || 1);
  const v = $("viewer");
  const [u, w] = clampZoomCentre(ui.zoom.cx - dx / k / fw, ui.zoom.cy - dy / k / fh, m ?? fake);
  under.style.width = `${fw * k}px`;
  under.style.height = `${fh * k}px`;
  under.style.left = `${v.clientWidth / 2 - u * fw * k}px`;
  under.style.top = `${v.clientHeight / 2 - w * fh * k}px`;
}

// Position the crop so (cx, cy) sits in the viewport centre, moved by
// (dx, dy) screen pixels while dragging, never past the photo's edges.
function placeRegion(dx = pan.dx, dy = pan.dy) {
  placeUnder(dx, dy);
  const m = ui.zoom.meta;
  if (!m) return;
  const v = $("viewer");
  const dpr = window.devicePixelRatio || 1;
  const k = zoomScale(m) / dpr;
  const img = $("preview");
  img.style.width = `${m.width * k}px`;
  img.style.height = `${m.height * k}px`;
  const [u, w] = clampZoomCentre(ui.zoom.cx - dx / k / m.full_width, ui.zoom.cy - dy / k / m.full_height, m);
  img.style.left = `${v.clientWidth / 2 - (u * m.full_width - m.x) * k}px`;
  img.style.top = `${v.clientHeight / 2 - (w * m.full_height - m.y) * k}px`;
}

// `box` (fractions of the photo's width and height) zooms to fit that area;
// without it, 100%.
function setZoom(on, u = 0.5, v = 0.5, box = null) {
  ui.zoom.on = on;
  ui.zoom.box = on ? box : null;
  ui.zoom.meta = null;
  [ui.zoom.cx, ui.zoom.cy] = on ? clampZoomCentre(u, v, null) : [u, v];
  const img = $("preview");
  const under = $("zoom-under");
  // Zooming in shows the whole photo enlarged at once, the sharp crop on top
  // when it arrives (renderRegion).
  const quick = on && ui.preview.fitUrl && ui.preview.fitPath === ui.info?.path;
  under.hidden = !quick;
  if (quick) {
    under.onload = () => placeUnder();
    under.src = ui.preview.fitUrl;
    $("viewer").classList.add("actual");
    img.style.visibility = "hidden";
    placeUnder();
  }
  if (!on) {
    $("viewer").classList.remove("actual");
    img.style.visibility = "";
  }
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
    const k = zoomScale(meta);
    [ui.zoom.cx, ui.zoom.cy] = clampZoomCentre(ui.zoom.cx - (pan.dx * dpr) / k / meta.full_width, ui.zoom.cy - (pan.dy * dpr) / k / meta.full_height, meta);
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

// Settings → General: scrolling on the whole photo steps through the folder.
function wheelSteps() {
  try { return localStorage.getItem("amb.wheelPhotos") === "1"; } catch { return false; }
}

function wireViewer() {
  const img = $("preview");
  let drag = null;
  // After layout, so resizing the photo doesn't loop the observer.
  new ResizeObserver(() => requestAnimationFrame(fitPreview)).observe($("viewer"));
  // The webview's own menu can't save the photo (and would copy the small
  // preview): right-click shows the photo's menu instead (Copy image, Export…).
  $("viewer").addEventListener("contextmenu", (e) => {
    e.preventDefault();
    // The tools' own canvases use right-click. With box zoom on, a right
    // click that doesn't drag shows the menu (quickzoom.js).
    if (ui.info && (e.target === img || e.target === $("viewer")) && !boxZoomWanted()) photoMenu(e);
  });
  // Scrolling (a mouse wheel or two fingers on a trackpad) moves the 1:1 view;
  // on the whole photo, if Settings asks, it goes to the next or previous one.
  const wheel = { sum: 0, at: 0 };
  $("viewer").addEventListener("wheel", (e) => {
    if (!ui.zoom.on && !e.ctrlKey && wheelSteps() && keyContext() === "main" && ui.info) {
      e.preventDefault();
      // One photo per notch, or per good swipe on a trackpad.
      wheel.sum += e.deltaMode === 1 ? e.deltaY * 16 : e.deltaY;
      if (Math.abs(wheel.sum) >= 50 && Date.now() - wheel.at > 250) {
        wheel.at = Date.now();
        stepPhoto(Math.sign(wheel.sum));
        wheel.sum = 0;
      }
      return;
    }
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
      const k = zoomScale(m);
      [ui.zoom.cx, ui.zoom.cy] = clampZoomCentre(ui.zoom.cx - ((e.clientX - d.x) * dpr) / k / m.full_width, ui.zoom.cy - ((e.clientY - d.y) * dpr) / k / m.full_height, m);
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
