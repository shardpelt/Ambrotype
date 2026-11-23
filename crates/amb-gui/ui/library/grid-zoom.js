// SPDX-License-Identifier: GPL-3.0-or-later
// The library bar's thumbnail size and sort: one `libSetSize` for the slider,
// Ctrl + wheel, a pinch and Ctrl + / − / 0, keeping a chosen photo where it
// is on screen; and the sort and size remembered on this computer.
"use strict";

const LIB_SIZE_DEFAULT = 160;     // px, the slider's start
const LIB_ZOOM_STEP = 1.25;       // Ctrl + / −: one step

// Is the grid (not the viewer, cull views or the map) what's showing?
const libGridShowing = () => lib.on && cull.view === "grid";

// A photo's place in the grid's cells (folder tiles come first), or -1.
function libGridIndex(path) {
  const i = lib.shown.findIndex((p) => p.path === path);
  return i < 0 ? -1 : i + (lib.dirs ?? []).length;
}

// How far below the grid's top edge a photo's cell starts, or null.
function libCellTop(path) {
  const i = libGridIndex(path);
  if (i < 0) return null;
  const { cols, rowH } = gridMetrics();
  return GRID_PAD + Math.floor(i / cols) * rowH - $("library-grid").scrollTop;
}

// The first photo near the top of the view.
function libFirstInView() {
  const { cols, rowH } = gridMetrics();
  const row = Math.max(0, Math.round(($("library-grid").scrollTop - GRID_PAD) / rowH));
  const i = Math.min(lib.shown.length - 1, Math.max(0, row * cols - (lib.dirs ?? []).length));
  return lib.shown[i]?.path ?? null;
}

// The thumbnail size, as exact as the wheel made it (the slider rounds).
function libSizeNow() {
  return lib.sizeExact ?? Number($("lib-size").value);
}

// Size the thumbnails to `px` (within the slider's own limits), keeping the
// photo `anchor` where it is on screen; remembered for next time.
function libSetSize(px, anchor = null, force = false) {
  const range = $("lib-size");
  const wanted = Number.isFinite(px) ? px : LIB_SIZE_DEFAULT;
  lib.sizeExact = Math.min(Number(range.max), Math.max(Number(range.min), wanted));
  const size = Math.round(lib.sizeExact);
  store.setText("amb.libSize", String(size));
  const grid = $("library-grid");
  // (The slider already shows its new value when it calls: ask the grid.)
  if (grid.style.getPropertyValue("--thumb") === `${size}px` && !force) return;
  const at = anchor ? libCellTop(anchor) : null;
  range.value = size;
  grid.style.setProperty("--thumb", `${size}px`);
  renderRange(true);
  if (at !== null) {
    const { cols, rowH } = gridMetrics();
    grid.scrollTop = GRID_PAD + Math.floor(libGridIndex(anchor) / cols) * rowH - at;
    renderRange();
  }
  for (const cell of lib.cells.values()) {
    if (cell.dataset.dir) continue;
    const url = cachedThumb(libPhoto(cell.dataset.path));
    if (url) cell.querySelector("img").src = url;
    else lib.queue.add(cell.dataset.path);
  }
}

// Ctrl + = − 0 in the grid: a step bigger or smaller, or back to the start
// (true when the grid was showing, so it used the key).
function libZoomKey(dir) {
  if (!libGridShowing()) return false;
  const { rowH } = gridMetrics();
  const height = $("library-grid").clientHeight;
  const chosen = selectedPhotos().find((p) => {
    const top = libCellTop(p.path);
    return top !== null && top > -rowH && top < height;
  });
  const size = dir > 0 ? libSizeNow() * LIB_ZOOM_STEP : dir < 0 ? libSizeNow() / LIB_ZOOM_STEP : LIB_SIZE_DEFAULT;
  libSetSize(size, chosen?.path ?? libFirstInView());
  return true;
}

// The key action: flip the sort's direction.
function libReverseSort() {
  const box = $("lib-reverse");
  box.checked = !box.checked;
  box.dispatchEvent(new Event("change", { bubbles: true }));
}

function wireLibraryZoom() {
  const grid = $("library-grid");
  // The remembered sort and size, before anything is drawn.
  const sort = store.text("amb.libSort");
  if (sort && [...$("lib-sort").options].some((o) => o.value === sort)) $("lib-sort").value = sort;
  $("lib-reverse").checked = store.get("amb.libReverse", false) === true;
  libSetSize(Number(store.text("amb.libSize") ?? NaN), null, true);
  // Only the person's own choice is remembered, not what a view sets.
  $("lib-sort").addEventListener("change", () => store.setText("amb.libSort", $("lib-sort").value));
  $("lib-reverse").addEventListener("change", () => store.set("amb.libReverse", $("lib-reverse").checked));
  $("lib-size").addEventListener("input", () => libSetSize(Number($("lib-size").value), libFirstInView()));
  // The photo under the pointer, else the first in view.
  const photoAt = (e) => {
    const path = e.target?.closest?.(".lib-cell")?.dataset.path;
    return path && libGridIndex(path) >= 0 ? path : libFirstInView();
  };
  // Ctrl + wheel and a pinch size the thumbnails; a plain wheel scrolls.
  wirePinchZoom(grid, {
    wheelScrolls: true,
    active: () => libGridShowing(),
    zoom: (factor, x, y) => libSetSize(libSizeNow() * factor, photoAt({ target: document.elementFromPoint(x, y) })),
  });
}
