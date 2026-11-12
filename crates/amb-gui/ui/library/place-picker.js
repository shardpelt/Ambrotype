// SPDX-License-Identifier: GPL-3.0-or-later
// Choose a place on a map or by name: from the library panel's Place row,
// the viewer's info panel and each guess in Guess places. Drag to move the
// map, the wheel, a pinch or + − to zoom (library/map-touch.js), a click puts the pin; the search box finds
// an address or a place by name (OpenStreetMap's Nominatim, asked only when
// you search, since it tells openstreetmap.org what was searched). Map
// images follow Settings → General, as the Map view does.
"use strict";

const picker = { z: 3, x: 0, y: 0, pin: null };

// Ask for a place, starting at `start` ([lat, lon]) if given; resolves to
// "lat,lon", or null when cancelled.
async function pickPlace(start = null, title = "Choose the place") {
  const dlg = $("pick-dialog");
  $("pick-title").textContent = title;
  $("pick-results").replaceChildren();
  $("pick-search").value = "";
  picker.pin = start ? [...start] : null;
  // Start at the pin, or where the shown photos are, or the whole world.
  const at = start ?? (typeof lib !== "undefined" ? lib.shown?.map((p) => p.gps ?? p.camera_gps).find(Boolean) : null);
  picker.z = at ? (start ? 14 : 10) : 2;
  [picker.x, picker.y] = at ? project(at[0], at[1], picker.z) : [TILE * 2, TILE * 2 * 0.62];
  dlg.showModal();
  requestAnimationFrame(drawPicker);
  showPickedPlace();
  const answer = await new Promise((resolve) => dlg.addEventListener("close", () => resolve(dlg.returnValue), { once: true }));
  if (dlg.contains(document.activeElement)) document.activeElement.blur();
  return answer === "ok" && picker.pin ? `${picker.pin[0].toFixed(6)},${picker.pin[1].toFixed(6)}` : null;
}

function showPickedPlace() {
  $("pick-at").textContent = picker.pin ? `${picker.pin[0].toFixed(5)}, ${picker.pin[1].toFixed(5)}` : "Click the map, or search, to put the pin.";
  $("pick-ok").disabled = !picker.pin;
}

function drawPicker() {
  const el = $("pick-map");
  const W = el.clientWidth, H = el.clientHeight;
  if (!W) return;
  mapClamp(picker, H);
  const world = TILE * 2 ** picker.z;
  const left = picker.x - W / 2, top = picker.y - H / 2;
  mapTiles(el.querySelector(".map-tiles"), picker, W, H);
  const pin = el.querySelector(".pick-pin");
  pin.hidden = !picker.pin;
  if (picker.pin) {
    let [x, y] = project(picker.pin[0], picker.pin[1], picker.z);
    x -= left;
    x -= Math.round((x - W / 2) / world) * world;
    pin.style.left = `${x}px`;
    pin.style.top = `${y - top}px`;
  }
}

function zoomPicker(d, cx = null, cy = null) {
  mapZoomAt(picker, $("pick-map"), 2 ** d, cx, cy);
  drawPicker();
}

async function searchPlace() {
  const q = $("pick-search").value.trim();
  if (!q) return;
  // Coordinates or a map link need no search.
  const typed = parsePlace(q);
  if (typed) return goPicked(typed.split(",").map(Number), q);
  const list = $("pick-results");
  list.replaceChildren(Object.assign(document.createElement("p"), { className: "hint", textContent: "Searching OpenStreetMap…" }));
  try {
    const found = await invoke("geocode", { query: q });
    list.replaceChildren(...found.map((f) => {
      const b = Object.assign(document.createElement("button"), { type: "button", className: "pick-result", textContent: f.name });
      b.addEventListener("click", () => goPicked([f.lat, f.lon], f.name));
      return b;
    }));
    if (!found.length) list.append(Object.assign(document.createElement("p"), { className: "hint", textContent: "Nothing found. Try a town or a street with its town." }));
    else if (found.length === 1) goPicked([found[0].lat, found[0].lon], found[0].name);
  } catch (e) {
    list.replaceChildren(Object.assign(document.createElement("p"), { className: "hint", textContent: `${e} (offline?)` }));
  }
}

function goPicked(at, name) {
  picker.pin = at;
  picker.z = Math.max(picker.z, 15);
  [picker.x, picker.y] = project(at[0], at[1], picker.z);
  for (const b of $("pick-results").querySelectorAll(".pick-result")) b.classList.toggle("on", b.textContent === name);
  drawPicker();
  showPickedPlace();
}

function wirePlacePicker() {
  const el = $("pick-map");
  // A click puts the pin.
  mapAttach(el, picker, {
    redraw: drawPicker,
    skip: ".pick-pin",
    click: (e) => {
      const r = el.getBoundingClientRect();
      const [lat, lon] = unproject(picker.x - r.width / 2 + (e.clientX - r.left), picker.y - r.height / 2 + (e.clientY - r.top), picker.z);
      picker.pin = [lat, lon];
      drawPicker();
      showPickedPlace();
    },
  });
  $("pick-zoom-in").addEventListener("click", () => zoomPicker(1));
  $("pick-zoom-out").addEventListener("click", () => zoomPicker(-1));
  $("pick-search").addEventListener("keydown", (e) => {
    e.stopPropagation();
    if (e.key === "Enter") {
      e.preventDefault();
      searchPlace();
    }
  });
  $("pick-search-go").addEventListener("click", searchPlace);
  new ResizeObserver(() => $("pick-dialog").open && drawPicker()).observe(el);
}
