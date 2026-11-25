// SPDX-License-Identifier: GPL-3.0-or-later
// Before / after split (Y, or Before / After with the bar starting near the
// right edge): the photo before its edits on the left of a bar, after on
// the right; drag the bar to move it. "Before" keeps the crop and
// geometry (src/session/render.rs `effective_settings`), so both halves line up.
"use strict";

// Where the bar is, 0..1 across the photo; remembered on this computer.
const splitView = { at: Number(store.text("amb.splitAt")) || 0.5 };

function toggleSplit() {
  if (!ui.info) return;
  if (sbs.layout) sbsSet(null);
  ui.split = !ui.split;
  if (ui.split && ui.before) {
    ui.before = false;
    $("before-badge").hidden = true;
  }
  $("btn-before").classList.toggle("active", ui.split);
  if (!ui.split) {
    const img = $("preview-before");
    if (img.src) URL.revokeObjectURL(img.src);
    img.removeAttribute("src");
  }
  placeSplit();
  if (ui.split) requestPreview(false);
}

// The before photo, rendered at the same size as the photo on screen: the
// whole photo (the split; side by side), or with `region` the same crop as
// the zoomed view's (side by side, preview.js renderRegion). As it shows,
// side by side learns what it shows (`sbs.before`).
async function renderBefore(maxEdge, region = null) {
  const path = ui.info.path;
  const { meta, jpeg } = splitFramed(await (region
    ? invoke("region", { ...region, before: true, clipping: false })
    : invoke("preview", { maxEdge, overrides: {}, before: true, clipping: false, maskOverlay: null })));
  if (!ui.split && !sbs.layout) return;
  const img = $("preview-before");
  const old = img.src;
  img.onload = () => {
    sbs.before = { path, meta: region ? meta : null };
    placeSplit();
  };
  img.src = URL.createObjectURL(new Blob([jpeg], { type: "image/jpeg" }));
  if (old) URL.revokeObjectURL(old);
}

// Lay the before image, the bar and the labels over the shown photo (side
// by side lays out its own: side-by-side.js).
function placeSplit() {
  if (sbs.layout) return placeSideBySide();
  const before = $("preview-before");
  const bar = $("split-bar");
  const photo = $("preview");
  const show = ui.split && ui.info && !photo.hidden && !ui.zoom.on && !ui.crop.on && !gu.on && before.getAttribute("src");
  before.hidden = bar.hidden = !show;
  if (!show) return;
  const v = $("viewer").getBoundingClientRect();
  const r = photo.getBoundingClientRect();
  Object.assign(before.style, { left: `${r.left - v.left}px`, top: `${r.top - v.top}px`, width: `${r.width}px`, height: `${r.height}px` });
  before.style.clipPath = `inset(0 ${(1 - splitView.at) * 100}% 0 0)`;
  Object.assign(bar.style, { left: `${r.left - v.left + splitView.at * r.width}px`, top: `${r.top - v.top}px`, height: `${r.height}px` });
}

function wireSplit() {
  const bar = $("split-bar");
  bar.addEventListener("pointerdown", (e) => {
    e.preventDefault();
    e.stopPropagation();
    try { bar.setPointerCapture(e.pointerId); } catch { /* a synthetic pointer */ }
    bar.classList.add("dragging");
    const move = (m) => {
      const r = $("preview").getBoundingClientRect();
      splitView.at = Math.min(1, Math.max(0, (m.clientX - r.left) / r.width));
      placeSplit();
    };
    const up = () => {
      bar.classList.remove("dragging");
      bar.removeEventListener("pointermove", move);
      store.setText("amb.splitAt", String(splitView.at));
    };
    bar.addEventListener("pointermove", move);
    bar.addEventListener("pointerup", up, { once: true });
    bar.addEventListener("pointercancel", up, { once: true });
  });
  // A click on the bar isn't a click on the photo (which zooms).
  bar.addEventListener("click", (e) => e.stopPropagation());
  new ResizeObserver(() => requestAnimationFrame(placeSplit)).observe($("viewer"));
}
