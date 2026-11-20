// SPDX-License-Identifier: GPL-3.0-or-later
// Before and after side by side (right-click Before / after): each whole in
// its half, each filling its half (a drag moves both), or above and below.
// Zooming and panning show the same part in both (preview.js photoArea).
"use strict";

// `layout`: null (one photo), "side", "fill" or "stacked". `cx, cy`: the
// part the filled halves show, as fractions of the photo. `before`: what the
// before image shows, set as it loads (split.js renderBefore): its photo's
// path and `meta`, a zoomed crop's place in the photo (null: the whole photo).
const sbs = { layout: null, cx: 0.5, cy: 0.5, before: null };

const SBS_LAYOUTS = [["side", tKey("Side by side")], ["fill", tKey("Side by side, filling the view")], ["stacked", tKey("Above and below")]];

// A layout on (or another one), or null: back to one photo. The split and
// the whole-photo Before give way; tools and pickers close first, as one
// tool closes another (and each of them, starting, ends side by side:
// preview.js setPicking, frame.js showFrameSettings).
async function sbsSet(layout) {
  if (layout === sbs.layout || (layout && !ui.info)) return;
  if (layout && !sbs.layout) {
    if (ui.crop.on) await exitCrop(true);
    exitMasks();
    exitSpots();
    exitGuided();
    exitSelect();
    if (ui.picking) setPicking(false);
    if (touchState.on) toggleTouch(false);
    if (!$("frame-panel").hidden) showFrameSettings(false);
  }
  sbs.layout = layout;
  if (layout === "fill") [sbs.cx, sbs.cy] = [0.5, 0.5];
  const v = $("viewer");
  v.classList.toggle("sbs", Boolean(layout));
  v.classList.toggle("sbs-fill", layout === "fill");
  for (const id of ["sbs-cover", "sbs-before-label", "sbs-after-label"]) $(id).hidden = !layout;
  if (layout) {
    ui.before = ui.split = false;
    $("before-badge").hidden = $("split-bar").hidden = true;
  } else {
    sbsLeave();
  }
  $("btn-before").classList.toggle("active", Boolean(layout) || ui.split || ui.before);
  // The frame waits while side by side shows (frame.js frameBlocked).
  drawFrame();
  if (ui.zoom.on) placeRegion();
  else placeSideBySide();
  requestPreview(false);
}

// Back to one photo: Before goes, After fills the viewer again.
function sbsLeave() {
  const b = $("preview-before");
  b.hidden = true;
  b.style.clipPath = "";
  if (b.src) URL.revokeObjectURL(b.src);
  b.removeAttribute("src");
  sbs.before = null;
  if (ui.zoom.on) return;
  const img = $("preview");
  for (const k of ["left", "top", "width", "height"]) img.style[k] = "";
  fitPreview();
}

// The two halves, in pixels from the viewer's corner: Before on the left (or
// above), After on the right (or below), a small gap between.
function sbsPanes() {
  const v = $("viewer");
  const [w, h] = [v.clientWidth, v.clientHeight];
  const gap = parseFloat(getComputedStyle(v).getPropertyValue("--sbs-gap")) || 0;
  if (sbs.layout === "stacked") {
    const half = Math.floor((h - gap) / 2);
    return { before: { x: 0, y: 0, w, h: half }, after: { x: 0, y: h - half, w, h: half } };
  }
  const half = Math.floor((w - gap) / 2);
  return { before: { x: 0, y: 0, w: half, h }, after: { x: w - half, y: 0, w: half, h } };
}

// The filled view's centre (u, v) kept where the photo still covers `pane`
// at w × h, so no empty edge shows.
function sbsCentre(pane, w, h, u, v) {
  const keep = (c, half) => (half >= 0.5 ? 0.5 : Math.min(1 - half, Math.max(half, c)));
  return [keep(u, pane.w / 2 / w), keep(v, pane.h / 2 / h)];
}

// Where the whole photo goes in `pane`, not zoomed: fitted and centred, or
// (fill) covering it, showing the part at sbs.cx, sbs.cy.
function sbsBox(pane) {
  const img = $("preview");
  const b = $("preview-before");
  const [nw, nh] = img.naturalWidth ? [img.naturalWidth, img.naturalHeight] : [b.naturalWidth || 1, b.naturalHeight || 1];
  const fill = sbs.layout === "fill";
  const s = (fill ? Math.max : Math.min)(pane.w / nw, pane.h / nh);
  const [w, h] = [nw * s, nh * s];
  const [u, v] = fill ? sbsCentre(pane, w, h, sbs.cx, sbs.cy) : [0.5, 0.5];
  return { x: pane.x + pane.w / 2 - u * w, y: pane.y + pane.h / 2 - v * h, w, h };
}

// Where the whole photo would be in `pane` at the zoomed scale, as
// preview.js placeRegion puts After in its half: so a crop of it lands on
// the same part. (dx, dy): a pan being dragged.
function sbsZoomBox(pane, dx, dy) {
  const f = ui.zoom.meta ?? sbs.before.meta ?? zoomFull($("preview-before"));
  const k = zoomScale(f) / (window.devicePixelRatio || 1);
  const [u, v] = clampZoomCentre(ui.zoom.cx - dx / k / f.full_width, ui.zoom.cy - dy / k / f.full_height, f);
  const [w, h] = [f.full_width * k, f.full_height * k];
  return { x: pane.x + pane.w / 2 - u * w, y: pane.y + pane.h / 2 - v * h, w, h };
}

// The long edge, in device pixels, at which a half shows the whole photo
// (preview.js previewSize caps it).
function sbsEdge() {
  const { after } = sbsPanes();
  const box = sbs.layout === "fill" && $("preview").naturalWidth ? sbsBox(after) : after;
  return Math.max(box.w, box.h) * (window.devicePixelRatio || 1);
}

// A point on Before, moved to the same place on After (both show the same
// part); any other point as it is.
function sbsToAfter(x, y) {
  if (!sbs.layout) return [x, y];
  const { before, after } = sbsPanes();
  const r = $("viewer").getBoundingClientRect();
  const onBefore = x - r.left < before.x + before.w && y - r.top < before.y + before.h;
  return onBefore ? [x + after.x - before.x, y + after.y - before.y] : [x, y];
}

function sbsPlace(e, r) {
  Object.assign(e.style, { left: `${r.x}px`, top: `${r.y}px`, width: `${r.w}px`, height: `${r.h}px` });
}

// Lay out both halves: the labels, the cover that keeps After to its half,
// After itself when not zoomed (zoomed, preview.js placeRegion places it),
// and Before at the same place in its half. (dx, dy): a pan being dragged.
function placeSideBySide(dx = pan.dx, dy = pan.dy) {
  if (!sbs.layout || !ui.info) return;
  const { before, after } = sbsPanes();
  const stacked = sbs.layout === "stacked";
  sbsPlace($("sbs-cover"), { x: 0, y: 0, w: stacked ? before.w : after.x, h: stacked ? after.y : before.h });
  for (const [id, pane] of [["sbs-before-label", before], ["sbs-after-label", after]]) Object.assign($(id).style, { left: `${pane.x}px`, top: `${pane.y}px` });
  const img = $("preview");
  if (!ui.zoom.on && img.naturalWidth) sbsPlace(img, sbsBox(after));
  const b = $("preview-before");
  b.hidden = !(sbs.before?.path === ui.info.path && b.getAttribute("src") && b.naturalWidth);
  if (b.hidden) return;
  const box = ui.zoom.on ? sbsZoomBox(before, dx, dy) : sbsBox(before);
  const m = sbs.before.meta;
  const r = m
    ? { x: box.x + (m.x / m.full_width) * box.w, y: box.y + (m.y / m.full_height) * box.h, w: (m.width / m.full_width) * box.w, h: (m.height / m.full_height) * box.h }
    : box;
  sbsPlace(b, r);
  const cut = [before.y - r.y, r.x + r.w - before.x - before.w, r.y + r.h - before.y - before.h, before.x - r.x];
  b.style.clipPath = `inset(${cut.map((n) => `${Math.max(0, n)}px`).join(" ")})`;
}

// A drag on either half. Filled and not zoomed, it moves both photos; zoomed,
// a drag on Before pans both (preview.js pans from After). A click on Before
// does what a click on After does: zoom in or out at that point.
function sbsDrag(e) {
  const img = e.currentTarget;
  const before = img.id === "preview-before";
  if (!sbs.layout || e.button !== 0 || (!before && (ui.zoom.on || sbs.layout !== "fill"))) return;
  // On After, preview.js's mouse events still come, to zoom on a click.
  if (before) e.preventDefault();
  try { img.setPointerCapture(e.pointerId); } catch { /* a synthetic pointer */ }
  const d = { x: e.clientX, y: e.clientY, cx: sbs.cx, cy: sbs.cy, moved: false, zoomed: ui.zoom.on };
  const move = (m) => {
    const [dx, dy] = [m.clientX - d.x, m.clientY - d.y];
    if (Math.abs(dx) + Math.abs(dy) > 3) d.moved = true;
    if (!d.moved) return;
    if (d.zoomed) {
      img.classList.add("panning");
      placeRegion(dx, dy);
      return;
    }
    if (sbs.layout !== "fill") return;
    const { after } = sbsPanes();
    const box = sbsBox(after);
    [sbs.cx, sbs.cy] = sbsCentre(after, box.w, box.h, d.cx - dx / box.w, d.cy - dy / box.h);
    placeSideBySide();
  };
  const up = (u) => {
    img.removeEventListener("pointermove", move);
    img.removeEventListener("pointerup", up);
    img.removeEventListener("pointercancel", up);
    img.classList.remove("panning");
    if (u.type === "pointercancel") return d.zoomed ? placeRegion() : undefined;
    if (d.zoomed && d.moved) return panBy(u.clientX - d.x, u.clientY - d.y);
    if (d.moved || !before) return;
    const [x, y] = sbsToAfter(u.clientX, u.clientY);
    const [pu, pv] = photoPoint({ clientX: x, clientY: y });
    setZoom(!ui.zoom.on, pu, pv);
  };
  img.addEventListener("pointermove", move);
  img.addEventListener("pointerup", up);
  img.addEventListener("pointercancel", up);
}

function wireSideBySide() {
  $("btn-before").addEventListener("contextmenu", (e) => {
    if (!ui.info) return;
    contextMenu(e, [
      ...SBS_LAYOUTS.map(([layout, label]) => [t(label), () => sbsSet(layout)]),
      sbs.layout ? [t("Back to one photo"), () => sbsSet(null)] : null,
    ].filter(Boolean));
  });
  for (const id of ["preview", "preview-before"]) $(id).addEventListener("pointerdown", sbsDrag);
}
