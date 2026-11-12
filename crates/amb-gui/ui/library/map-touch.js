// SPDX-License-Identifier: GPL-3.0-or-later
// What the library's Map view and the place picker's map share: the zoom
// limits, drawing the tiles (or the grid of latitudes and longitudes) at any
// zoom, and the gestures. They are the photo's (core/pinch.js): press and
// drag moves the map, the wheel and a pinch zoom at the pointer, two fingers
// on a trackpad move it, a double-click zooms in. Nothing else scrolls
// while the pointer is over a map.
"use strict";

const MAP_MIN_Z = 1;
// OpenStreetMap's tiles go to 19; 18 is plenty for finding a street.
const MAP_MAX_Z = 18;
// A press that moves less than this is a click (a pin, a place).
const MAP_DRAG_PX = 4;

// The view `st` is { z, x, y }: zoom (any number), and the centre in world
// pixels at that zoom.
// The least zoom, where the world still fills the map's height.
const mapMinZoom = (H) => Math.max(MAP_MIN_Z, Math.log2(H / TILE) || 0);

// Keep the view in the world: sideways it wraps round, up and down it ends.
function mapClamp(st, H) {
  st.z = Math.min(MAP_MAX_Z, Math.max(mapMinZoom(H), st.z));
  st.y = Math.min(TILE * 2 ** st.z - H / 2, Math.max(H / 2, st.y));
}

// Zoom by `factor` keeping the point under client (cx, cy), or the middle.
function mapZoomAt(st, el, factor, cx = null, cy = null) {
  const r = el.getBoundingClientRect();
  const px = cx == null ? 0 : cx - r.left - r.width / 2;
  const py = cy == null ? 0 : cy - r.top - r.height / 2;
  const z = Math.min(MAP_MAX_Z, Math.max(mapMinZoom(r.height), st.z + Math.log2(factor)));
  const k = 2 ** (z - st.z);
  st.x = (st.x + px) * k - px;
  st.y = (st.y + py) * k - py;
  st.z = z;
}

// Fill `box` with the map images (when turned on) or the grid. Images come
// at the whole zoom nearest, scaled to fit the zoom between.
function mapTiles(box, st, W, H) {
  const left = st.x - W / 2, top = st.y - H / 2;
  box.replaceChildren();
  if (!mapTilesWanted()) {
    box.append(graticule(left, top, W, H, st.z));
    return;
  }
  const tz = Math.min(MAP_MAX_Z, Math.max(0, Math.round(st.z)));
  const n = 2 ** tz;
  const size = TILE * 2 ** (st.z - tz);
  for (let ty = Math.floor(top / size); ty * size < top + H; ty++) {
    if (ty < 0 || ty >= n) continue;
    for (let tx = Math.floor(left / size); tx * size < left + W; tx++) {
      const img = el("img", { alt: "", draggable: "false" });
      img.src = `https://tile.openstreetmap.org/${tz}/${((tx % n) + n) % n}/${ty}.png`;
      img.style.left = `${tx * size - left}px`;
      img.style.top = `${ty * size - top}px`;
      img.style.width = img.style.height = `${size}px`;
      box.append(img);
    }
  }
}

// Wire the gestures on `el` for the view `st`. `redraw()` draws it; `skip`
// is what a press on doesn't start a drag; `click(e)` gets a press that
// didn't move.
function mapAttach(el, st, { redraw, skip = ".map-pin, .map-credit", click = null }) {
  let drag = null;
  el.addEventListener("pointerdown", (e) => {
    if (e.button !== 0 || e.target.closest(skip)) return;
    drag = { x: e.clientX, y: e.clientY, ox: st.x, oy: st.y, moved: false };
    try {
      el.setPointerCapture(e.pointerId);
    } catch { /* no such pointer: a script's */ }
  });
  el.addEventListener("pointermove", (e) => {
    if (!drag || (!drag.moved && Math.hypot(e.clientX - drag.x, e.clientY - drag.y) < MAP_DRAG_PX)) return;
    drag.moved = true;
    el.classList.add("panning");
    st.x = drag.ox - (e.clientX - drag.x);
    st.y = drag.oy - (e.clientY - drag.y);
    redraw();
  });
  const end = (e) => {
    const d = drag;
    drag = null;
    el.classList.remove("panning");
    if (d && !d.moved && e.type === "pointerup") click?.(e);
  };
  el.addEventListener("pointerup", end);
  el.addEventListener("pointercancel", end);
  wirePinchZoom(el, {
    zoom: (f, x, y) => {
      mapZoomAt(st, el, f, x, y);
      redraw();
    },
    scroll: (dx, dy) => {
      st.x += dx;
      st.y += dy;
      redraw();
      return true;
    },
  });
  el.addEventListener("dblclick", (e) => {
    if (e.target.closest(skip)) return;
    mapZoomAt(st, el, 2, e.clientX, e.clientY);
    redraw();
  });
}

// Ctrl + and Ctrl − (the zoom actions) in the library: the Map view's.
function mapKeyZoom(dir) {
  if (!(libraryOn() && cull.view === "map")) return false;
  zoomMap(dir);
  return true;
}
