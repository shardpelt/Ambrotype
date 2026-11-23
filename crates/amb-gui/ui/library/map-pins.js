// SPDX-License-Identifier: GPL-3.0-or-later
// Dragging a one-photo pin on the Map view to move that photo's place. A
// press that moves less than MAP_DRAG_PX stays a click (select, peek); a
// longer one carries the pin, and where it is let go becomes the place,
// saved the way a typed place is (placePhotos).
"use strict";

const pinDrag = { active: null, moved: false };

// Make `pin` (one photo `p`) draggable.
function pinDragAttach(pin, p) {
  pin.addEventListener("pointerdown", (e) => {
    if (e.button !== 0) return;
    pinDrag.moved = false;
    pinDrag.active = { pin, p, x: e.clientX, y: e.clientY, left: parseFloat(pin.style.left), top: parseFloat(pin.style.top), moved: false };
    try {
      pin.setPointerCapture(e.pointerId);
    } catch { /* no such pointer: a script's */ }
  });
  pin.addEventListener("pointermove", (e) => {
    const d = pinDrag.active;
    if (!d || d.pin !== pin || (!d.moved && Math.hypot(e.clientX - d.x, e.clientY - d.y) < MAP_DRAG_PX)) return;
    d.moved = true;
    hidePinPeek();
    pin.classList.add("dragging");
    pin.style.left = `${d.left + e.clientX - d.x}px`;
    pin.style.top = `${d.top + e.clientY - d.y}px`;
  });
  const end = (e) => {
    const d = pinDrag.active;
    if (!d || d.pin !== pin) return;
    pinDrag.active = null;
    pin.classList.remove("dragging");
    if (!d.moved) return;
    pinDrag.moved = true;
    if (e.type === "pointerup") pinDragDrop(d, e);
    else drawMap();
  };
  pin.addEventListener("pointerup", end);
  pin.addEventListener("pointercancel", end);
}

// Save the place under where the pin was let go.
async function pinDragDrop(d, e) {
  const map = d.pin.closest(".map");
  if (!map) return;
  const r = map.getBoundingClientRect();
  const [lat, lon] = mapToLatLon(browse.map, e.clientX - r.left, e.clientY - r.top, map.clientWidth, map.clientHeight);
  await placePhotos([d.p.path], `${lat.toFixed(6)},${lon.toFixed(6)}`);
  toast(t("Moved the place of {name}", { name: d.p.name }));
}

function wireMapPins() {
  // The click that ends a drag is not a selection.
  document.addEventListener("click", (e) => {
    if (!pinDrag.moved) return;
    pinDrag.moved = false;
    if (e.target.closest?.(".map-pin")) {
      e.stopPropagation();
      e.preventDefault();
    }
  }, true);
}
