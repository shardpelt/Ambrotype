// SPDX-License-Identifier: GPL-3.0-or-later
// Before / after split (Y): the photo before its edits on the left of a bar,
// after on the right; drag the bar to move it. "Before" keeps the crop and
// geometry (src/session.rs `effective_settings`), so both halves line up.
"use strict";

// Where the bar is, 0..1 across the photo; remembered on this computer.
let splitAt = (() => {
  try { return Number(localStorage.getItem("amb.splitAt")) || 0.5; } catch { return 0.5; }
})();

function toggleSplit() {
  if (!ui.info) return;
  ui.split = !ui.split;
  if (ui.split && ui.before) toggleBefore();
  if (!ui.split) {
    const img = $("preview-before");
    if (img.src) URL.revokeObjectURL(img.src);
    img.removeAttribute("src");
  }
  placeSplit();
  if (ui.split) requestPreview(false);
}

// The "before" half, rendered at the same size as the photo on screen.
async function renderSplitBefore(maxEdge) {
  const { jpeg } = splitFramed(await invoke("preview", { maxEdge, overrides: {}, before: true, clipping: false, maskOverlay: null }));
  if (!ui.split) return;
  const img = $("preview-before");
  const old = img.src;
  img.onload = placeSplit;
  img.src = URL.createObjectURL(new Blob([jpeg], { type: "image/jpeg" }));
  if (old) URL.revokeObjectURL(old);
}

// Lay the before image, the bar and the labels over the shown photo.
function placeSplit() {
  const before = $("preview-before");
  const bar = $("split-bar");
  const photo = $("preview");
  const show = ui.split && ui.info && !photo.hidden && !ui.zoom.on && !ui.crop.on && !gu.on && before.getAttribute("src");
  before.hidden = bar.hidden = !show;
  if (!show) return;
  const v = $("viewer").getBoundingClientRect();
  const r = photo.getBoundingClientRect();
  Object.assign(before.style, { left: `${r.left - v.left}px`, top: `${r.top - v.top}px`, width: `${r.width}px`, height: `${r.height}px` });
  before.style.clipPath = `inset(0 ${(1 - splitAt) * 100}% 0 0)`;
  Object.assign(bar.style, { left: `${r.left - v.left + splitAt * r.width}px`, top: `${r.top - v.top}px`, height: `${r.height}px` });
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
      splitAt = Math.min(1, Math.max(0, (m.clientX - r.left) / r.width));
      placeSplit();
    };
    const up = () => {
      bar.classList.remove("dragging");
      bar.removeEventListener("pointermove", move);
      try { localStorage.setItem("amb.splitAt", String(splitAt)); } catch { /* not kept */ }
    };
    bar.addEventListener("pointermove", move);
    bar.addEventListener("pointerup", up, { once: true });
    bar.addEventListener("pointercancel", up, { once: true });
  });
  // A click on the bar isn't a click on the photo (which zooms).
  bar.addEventListener("click", (e) => e.stopPropagation());
  new ResizeObserver(() => requestAnimationFrame(placeSplit)).observe($("viewer"));
}
