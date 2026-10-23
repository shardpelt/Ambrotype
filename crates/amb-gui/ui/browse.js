// SPDX-License-Identifier: GPL-3.0-or-later
// Other ways to browse the library: by date and by place.
//
// - Timeline: the years of the photos shown, and for one year a calendar of
//   each month with the days that have photos (a thumbnail and a count).
//   Clicking a day or a month shows just those photos in the grid; the date
//   chip in the bar takes the date filter off again.
// - The grid's scrubber: sorted by capture time, a strip at the grid's right
//   edge with the years; drag along it to jump through them.
// - Map: the photos with a place, as pins (grouped when close together).
//   Photos without one can be dragged onto the map, or placed from a GPX
//   track; the place goes into their sidecars as Lightroom writes it.
//   Map images come from OpenStreetMap only when turned on (Settings →
//   General), since fetching them tells openstreetmap.org which area you
//   look at; off, the map is a plain grid of latitudes and longitudes.
"use strict";

const BROWSE_VIEWS = ["timeline", "map"];
const browseOn = () => lib.on && BROWSE_VIEWS.includes(cull.view);
const MONTHS = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];

const browse = {
  year: null,
  map: { z: 2, x: 512, y: 512, fitted: false }, // zoom, and the centre in world pixels at that zoom
  observer: null,
};

function renderBrowse() {
  const box = $("library-view");
  box.className = `library-view browse ${cull.view}`;
  if (cull.view === "timeline") renderTimeline(box);
  else renderMap(box);
  renderCounts();
  renderPanel();
}

// ---------------------------------------------------------------- date filter

// "2026", "2026-07" or "2026-07-14" as words.
function dateWords(d) {
  const [y, m, day] = d.split("-");
  if (!m) return y;
  return day ? `${Number(day)} ${MONTHS[m - 1]} ${y}` : `${MONTHS[m - 1]} ${y}`;
}

// Show only the photos taken on `date` (a prefix), in the grid.
function setDateFilter(date) {
  lib.date = date;
  const chip = $("lib-date");
  chip.hidden = !date;
  chip.textContent = date ? `${dateWords(date)} ×` : "";
  renderLibrary();
  if (date) setLibraryView("grid");
}

// ---------------------------------------------------------------- timeline

function photosByDay(photos) {
  const days = new Map();
  let undated = 0;
  for (const p of photos) {
    const d = p.captured?.slice(0, 10);
    if (!d || d.length < 10) {
      undated++;
      continue;
    }
    if (!days.has(d)) days.set(d, []);
    days.get(d).push(p);
  }
  return { days, undated };
}

function renderTimeline(box) {
  browse.observer?.disconnect();
  const { days, undated } = photosByDay(lib.beforeDate ?? lib.shown);
  const years = new Map();
  for (const [d, ps] of days) years.set(d.slice(0, 4), (years.get(d.slice(0, 4)) ?? 0) + ps.length);
  const yearList = [...years.keys()].sort();
  if (!yearList.length) {
    box.replaceChildren(Object.assign(document.createElement("p"), { className: "hint browse-empty", textContent: "None of the photos shown has a capture date." }));
    return;
  }
  if (!years.has(browse.year)) browse.year = lib.date?.slice(0, 4) ?? yearList[yearList.length - 1];
  // The years, as bars as tall as their share.
  const most = Math.max(...years.values());
  const strip = Object.assign(document.createElement("div"), { className: "tl-years" });
  for (const y of yearList) {
    const b = Object.assign(document.createElement("button"), { type: "button", className: "tl-year", title: `${years.get(y)} photos in ${y}` });
    b.classList.toggle("on", y === browse.year);
    const bar = Object.assign(document.createElement("span"), { className: "tl-bar" });
    bar.style.height = `${Math.max(8, (years.get(y) / most) * 100)}%`;
    b.append(bar, Object.assign(document.createElement("span"), { textContent: y }));
    b.addEventListener("click", () => {
      browse.year = y;
      renderTimeline(box);
    });
    b.addEventListener("dblclick", () => setDateFilter(y));
    strip.append(b);
  }
  const head = Object.assign(document.createElement("div"), { className: "tl-head" });
  const all = Object.assign(document.createElement("button"), { type: "button", className: "small", textContent: `Show all of ${browse.year}` });
  all.addEventListener("click", () => setDateFilter(browse.year));
  head.append(Object.assign(document.createElement("b"), { textContent: `${browse.year} · ${years.get(browse.year)} photos` }), all);
  if (undated) head.append(Object.assign(document.createElement("span"), { className: "hint", textContent: `${undated} without a date` }));
  // The year's months as calendars.
  const months = Object.assign(document.createElement("div"), { className: "tl-months" });
  browse.observer = new IntersectionObserver((entries) => {
    for (const e of entries) {
      if (!e.isIntersecting) continue;
      browse.observer.unobserve(e.target);
      const p = libPhoto(e.target.dataset.path);
      if (p) bigImage(p, 256, false).then((u) => { e.target.style.backgroundImage = `url(${u})`; }).catch(() => {});
    }
  }, { root: box, rootMargin: "200px" });
  for (let m = 1; m <= 12; m++) months.append(monthCalendar(browse.year, m, days));
  box.replaceChildren(strip, head, months);
}

function monthCalendar(year, month, days) {
  const mm = String(month).padStart(2, "0");
  const prefix = `${year}-${mm}`;
  const count = [...days].filter(([d]) => d.startsWith(prefix)).reduce((n, [, ps]) => n + ps.length, 0);
  const block = Object.assign(document.createElement("section"), { className: "tl-month" });
  block.classList.toggle("empty", !count);
  const title = Object.assign(document.createElement("button"), { type: "button", className: "tl-month-title", disabled: !count });
  title.append(Object.assign(document.createElement("b"), { textContent: MONTHS[month - 1] }), count ? ` ${count}` : "");
  title.title = count ? `Show the ${count} photos of ${MONTHS[month - 1]} ${year}` : "";
  title.addEventListener("click", () => setDateFilter(prefix));
  const grid = Object.assign(document.createElement("div"), { className: "tl-days" });
  for (const d of ["M", "T", "W", "T", "F", "S", "S"]) grid.append(Object.assign(document.createElement("span"), { className: "tl-dow", textContent: d }));
  // Monday first.
  const lead = (new Date(Date.UTC(year, month - 1, 1)).getUTCDay() + 6) % 7;
  for (let i = 0; i < lead; i++) grid.append(document.createElement("span"));
  const length = new Date(Date.UTC(year, month, 0)).getUTCDate();
  for (let day = 1; day <= length; day++) {
    const date = `${prefix}-${String(day).padStart(2, "0")}`;
    const ps = days.get(date);
    const cell = Object.assign(document.createElement(ps ? "button" : "span"), { className: "tl-day" });
    cell.append(Object.assign(document.createElement("span"), { className: "tl-num", textContent: day }));
    if (ps) {
      cell.type = "button";
      cell.classList.add("has");
      cell.title = `${dateWords(date)}: ${ps.length} photo${ps.length === 1 ? "" : "s"}`;
      cell.append(Object.assign(document.createElement("span"), { className: "tl-count", textContent: ps.length }));
      cell.dataset.path = ps[0].path;
      browse.observer.observe(cell);
      cell.addEventListener("click", () => setDateFilter(date));
    }
    grid.append(cell);
  }
  block.append(title, grid);
  return block;
}

// ---------------------------------------------------------------- scrubber

// The years along the grid's right edge, when sorted by capture time.
function renderScrubber() {
  const strip = $("lib-scrubber");
  const photos = lib.shown;
  const on = lib.on && cull.view === "grid" && $("lib-sort").value === "captured" && photos.length > 40;
  strip.hidden = !on;
  if (!on) return;
  const marks = [];
  let last = null;
  photos.forEach((p, i) => {
    const y = p.captured?.slice(0, 4);
    if (y && y !== last) {
      marks.push([y, i]);
      last = y;
    }
  });
  strip.replaceChildren(...marks.map(([y, i]) => {
    const m = Object.assign(document.createElement("span"), { className: "scrub-mark", textContent: y });
    m.style.top = `${(i / photos.length) * 100}%`;
    return m;
  }), Object.assign(document.createElement("span"), { className: "scrub-tip", hidden: true }));
}

function wireScrubber() {
  const strip = $("lib-scrubber");
  const at = (e) => {
    const r = strip.getBoundingClientRect();
    const f = Math.min(1, Math.max(0, (e.clientY - r.top) / r.height));
    const i = Math.min(lib.shown.length - 1, Math.floor(f * lib.shown.length));
    const { cols, rowH } = gridMetrics();
    $("library-grid").scrollTop = Math.floor(i / cols) * rowH;
    const tip = strip.querySelector(".scrub-tip");
    const c = lib.shown[i]?.captured;
    tip.hidden = !c;
    tip.textContent = c ? dateWords(c.slice(0, 7)) : "";
    tip.style.top = `${f * 100}%`;
  };
  strip.addEventListener("pointerdown", (e) => {
    strip.setPointerCapture(e.pointerId);
    strip.classList.add("dragging");
    at(e);
  });
  strip.addEventListener("pointermove", (e) => {
    if (strip.hasPointerCapture(e.pointerId)) at(e);
  });
  const end = () => {
    strip.classList.remove("dragging");
    const tip = strip.querySelector(".scrub-tip");
    if (tip) tip.hidden = true;
  };
  strip.addEventListener("pointerup", end);
  strip.addEventListener("pointercancel", end);
}

// ---------------------------------------------------------------- map

const TILE = 256;
const mapTilesWanted = () => {
  try { return localStorage.getItem("amb.mapTiles") === "1"; } catch { return false; }
};

// Web Mercator: degrees to world pixels at zoom `z`, and back.
function project(lat, lon, z) {
  const w = TILE * 2 ** z;
  const s = Math.sin((Math.max(-85.05, Math.min(85.05, lat)) * Math.PI) / 180);
  return [((lon + 180) / 360) * w, (0.5 - Math.log((1 + s) / (1 - s)) / (4 * Math.PI)) * w];
}

function unproject(x, y, z) {
  const w = TILE * 2 ** z;
  const lon = (x / w) * 360 - 180;
  const n = Math.PI - (2 * Math.PI * y) / w;
  return [(180 / Math.PI) * Math.atan(Math.sinh(n)), ((lon + 540) % 360) - 180];
}

// A photo's place from its sidecar change, or the camera's.
function placeOf(p) {
  return p.gps ?? p.camera_gps ?? null;
}

function renderMap(box) {
  browse.observer?.disconnect();
  const placed = lib.shown.filter((p) => placeOf(p));
  const unplaced = lib.shown.filter((p) => !placeOf(p));
  if (!browse.map.fitted) fitMap(placed, box);
  const map = Object.assign(document.createElement("div"), { className: "map", tabIndex: 0 });
  map.append(
    Object.assign(document.createElement("div"), { className: "map-tiles" }),
    Object.assign(document.createElement("div"), { className: "map-pins" }),
  );
  const bar = Object.assign(document.createElement("div"), { className: "map-bar" });
  const button = (text, title, fn) => {
    const b = Object.assign(document.createElement("button"), { type: "button", className: "small", textContent: text, title });
    b.addEventListener("click", fn);
    bar.append(b);
    return b;
  };
  button("+", "Zoom in", () => zoomMap(1));
  button("−", "Zoom out", () => zoomMap(-1));
  button("Fit", "Show every placed photo", () => {
    browse.map.fitted = false;
    renderMap(box);
  });
  button("Match GPX track…", "Place photos from a GPS track recorded while shooting (the selected ones, or all shown)", matchGpx);
  const clear = button("Remove place", "Take the place out of the selected photos' sidecars (a camera's own GPS stays)", () => placePhotos(selectedPhotos().map((p) => p.path), ""));
  clear.hidden = !selectedPhotos().some((p) => p.gps);
  bar.append(Object.assign(document.createElement("span"), { className: "hint", textContent: `${placed.length} placed${unplaced.length ? ` · ${unplaced.length} without a place` : ""}` }));
  const credit = Object.assign(document.createElement("button"), { type: "button", className: "map-credit" });
  if (mapTilesWanted()) {
    credit.textContent = "© OpenStreetMap contributors";
    credit.title = "Map data and images: OpenStreetMap (open the copyright page)";
    credit.addEventListener("click", () => invoke("open_osm_copyright").catch((e) => toast(String(e), true)));
  } else {
    credit.textContent = "No map images · turn them on";
    credit.title = "Show OpenStreetMap's map images behind the pins. The map then asks openstreetmap.org for the area shown.";
    credit.addEventListener("click", () => {
      $("settings-map-tiles").checked = true;
      setMapTiles(true);
    });
  }
  map.append(credit);
  // Photos without a place: drag them onto the map.
  const tray = Object.assign(document.createElement("div"), { className: "map-tray" });
  if (unplaced.length) {
    tray.append(Object.assign(document.createElement("span"), { className: "hint", textContent: "Drag onto the map to place (the selection moves together):" }));
    browse.observer = new IntersectionObserver((entries) => {
      for (const e of entries) {
        if (!e.isIntersecting) continue;
        browse.observer.unobserve(e.target);
        const p = libPhoto(e.target.dataset.path);
        if (p) bigImage(p, 256, false).then((u) => { e.target.src = u; }).catch(() => {});
      }
    }, { root: tray, rootMargin: "300px" });
    for (const p of unplaced.slice(0, 500)) {
      const img = Object.assign(document.createElement("img"), { className: "map-thumb", alt: p.name, title: p.name, draggable: true });
      img.dataset.path = p.path;
      img.classList.toggle("selected", lib.selected.has(p.path));
      img.addEventListener("dragstart", (e) => e.dataTransfer.setData("text/x-amb-photo", p.path));
      img.addEventListener("click", (e) => {
        select(p.path, e.shiftKey ? "range" : e.ctrlKey || e.metaKey ? "toggle" : "only");
        for (const t of tray.querySelectorAll(".map-thumb")) t.classList.toggle("selected", lib.selected.has(t.dataset.path));
        renderPanel();
      });
      browse.observer.observe(img);
      tray.append(img);
    }
  }
  box.replaceChildren(bar, map, tray);
  tray.hidden = !unplaced.length;
  wireMap(map);
  drawMap();
}

// Zoom and centre to show every placed photo.
function fitMap(placed, box) {
  const m = browse.map;
  m.fitted = true;
  if (!placed.length) {
    Object.assign(m, { z: 2 }, { x: TILE * 2, y: TILE * 2 * 0.62 });
    return;
  }
  const w = Math.max(200, box.clientWidth - 40), h = Math.max(200, box.clientHeight - 160);
  for (let z = 16; z >= 1; z--) {
    const pts = placed.map((p) => project(...placeOf(p), z));
    const xs = pts.map((q) => q[0]), ys = pts.map((q) => q[1]);
    const [x0, x1, y0, y1] = [Math.min(...xs), Math.max(...xs), Math.min(...ys), Math.max(...ys)];
    if (x1 - x0 <= w * 0.8 && y1 - y0 <= h * 0.8) {
      Object.assign(m, { z, x: (x0 + x1) / 2, y: (y0 + y1) / 2 });
      return;
    }
  }
  Object.assign(m, { z: 1, x: TILE, y: TILE });
}

function zoomMap(step, at = null) {
  const m = browse.map;
  const el = document.querySelector("#library-view .map");
  if (!el) return;
  const z = Math.min(18, Math.max(1, m.z + step));
  if (z === m.z) return;
  // Keep the point under `at` (or the middle) where it is.
  const r = el.getBoundingClientRect();
  const [ox, oy] = at ? [at[0] - r.left - r.width / 2, at[1] - r.top - r.height / 2] : [0, 0];
  const k = 2 ** (z - m.z);
  m.x = (m.x + ox) * k - ox;
  m.y = (m.y + oy) * k - oy;
  m.z = z;
  drawMap();
}

function drawMap() {
  const el = document.querySelector("#library-view .map");
  if (!el) return;
  const m = browse.map;
  const W = el.clientWidth, H = el.clientHeight;
  const world = TILE * 2 ** m.z;
  m.y = Math.min(world - H / 2, Math.max(H / 2, m.y));
  if (world < H) m.y = world / 2;
  const left = m.x - W / 2, top = m.y - H / 2;
  const tiles = el.querySelector(".map-tiles");
  tiles.replaceChildren();
  if (mapTilesWanted()) {
    const n = 2 ** m.z;
    for (let ty = Math.floor(top / TILE); ty * TILE < top + H; ty++) {
      if (ty < 0 || ty >= n) continue;
      for (let tx = Math.floor(left / TILE); tx * TILE < left + W; tx++) {
        const img = Object.assign(document.createElement("img"), { alt: "", draggable: false });
        img.src = `https://tile.openstreetmap.org/${m.z}/${((tx % n) + n) % n}/${ty}.png`;
        img.style.left = `${tx * TILE - left}px`;
        img.style.top = `${ty * TILE - top}px`;
        tiles.append(img);
      }
    }
  } else {
    tiles.append(graticule(left, top, W, H, m.z));
  }
  // Pins, grouped when closer than about a pin's width.
  const cell = 44;
  const groups = new Map();
  for (const p of lib.shown) {
    const at = placeOf(p);
    if (!at) continue;
    let [x, y] = project(at[0], at[1], m.z);
    x -= left;
    // The nearest copy of the world, when it wraps.
    x -= Math.round((x - W / 2) / world) * world;
    y -= top;
    if (x < -cell || y < -cell || x > W + cell || y > H + cell) continue;
    const key = `${Math.floor(x / cell)}:${Math.floor(y / cell)}`;
    const g = groups.get(key) ?? { x: 0, y: 0, photos: [] };
    g.x += x;
    g.y += y;
    g.photos.push(p);
    groups.set(key, g);
  }
  // Neighbouring cells' groups that would overlap become one.
  const merged = [];
  for (const g of groups.values()) {
    const near = merged.find((h) => Math.hypot(h.x / h.photos.length - g.x / g.photos.length, h.y / h.photos.length - g.y / g.photos.length) < 30);
    if (!near) {
      merged.push(g);
      continue;
    }
    near.x += g.x;
    near.y += g.y;
    near.photos.push(...g.photos);
  }
  const pins = el.querySelector(".map-pins");
  pins.replaceChildren(...merged.map((g) => {
    const n = g.photos.length;
    const pin = Object.assign(document.createElement("button"), { type: "button", className: "map-pin", textContent: n > 1 ? n : "" });
    pin.style.left = `${g.x / n}px`;
    pin.style.top = `${g.y / n}px`;
    pin.classList.toggle("selected", g.photos.some((p) => lib.selected.has(p.path)));
    pin.classList.toggle("one", n === 1);
    pin.title = n === 1 ? g.photos[0].name : `${n} photos: ${g.photos.slice(0, 3).map((p) => p.name).join(", ")}${n > 3 ? "…" : ""}`;
    pin.addEventListener("pointerenter", () => showPinPeek(pin, g.photos[0]));
    pin.addEventListener("pointerleave", hidePinPeek);
    pin.addEventListener("click", (e) => {
      e.stopPropagation();
      if (!(e.ctrlKey || e.metaKey)) lib.selected.clear();
      for (const p of g.photos) lib.selected.add(p.path);
      lib.focus = g.photos[0].path;
      renderPanel();
      renderCounts();
      drawMap();
      status(`${n} photo${n === 1 ? "" : "s"} here selected · double-click a pin to ${n > 1 ? "zoom in" : "open it"}`);
    });
    pin.addEventListener("dblclick", (e) => {
      e.stopPropagation();
      if (n === 1) return libraryOpen(g.photos[0].path);
      zoomMap(2, [e.clientX, e.clientY]);
    });
    return pin;
  }));
}

// Lines every few degrees, for when there are no map images.
function graticule(left, top, W, H, z) {
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.setAttribute("class", "map-grid");
  svg.setAttribute("width", W);
  svg.setAttribute("height", H);
  const step = [30, 30, 20, 10, 5, 2, 1, 1, 0.5, 0.2, 0.1, 0.05, 0.02, 0.01, 0.005, 0.002, 0.001, 0.001, 0.0005][z] ?? 0.001;
  // Longitudes not wrapped, so a view across the date line still counts up.
  const world = TILE * 2 ** z;
  const lonLeft = (left / world) * 360 - 180, lonRight = ((left + W) / world) * 360 - 180;
  const latTop = unproject(0, Math.max(0, top), z)[0], latBottom = unproject(0, Math.min(world, top + H), z)[0];
  const wrap = (lon) => ((((lon + 180) % 360) + 360) % 360) - 180;
  const line = (x1, y1, x2, y2, label, lx, ly) => {
    const l = document.createElementNS(svg.namespaceURI, "line");
    Object.entries({ x1, y1, x2, y2 }).forEach(([k, v]) => l.setAttribute(k, v));
    const t = document.createElementNS(svg.namespaceURI, "text");
    t.setAttribute("x", lx);
    t.setAttribute("y", ly);
    t.textContent = label;
    svg.append(l, t);
  };
  const fmt = (v, pos, neg) => `${Math.abs(+v.toFixed(4))}°${v < 0 ? neg : pos}`;
  for (let lon = Math.ceil(lonLeft / step) * step; lon <= lonRight; lon += step) {
    const x = ((lon + 180) / 360) * world - left;
    line(x, 0, x, H, fmt(wrap(lon), "E", "W"), x + 3, H - 6);
  }
  for (let lat = Math.ceil(latBottom / step) * step; lat <= latTop; lat += step) {
    const y = project(lat, 0, z)[1] - top;
    line(0, y, W, y, fmt(lat, "N", "S"), 4, y - 3);
  }
  return svg;
}

let pinPeek = null;
function showPinPeek(pin, p) {
  pinPeek ??= Object.assign(document.createElement("img"), { className: "map-peek", alt: "" });
  pinPeek.removeAttribute("src");
  const r = pin.getBoundingClientRect();
  Object.assign(pinPeek.style, { left: `${r.left + r.width / 2}px`, top: `${r.top}px` });
  document.body.append(pinPeek);
  bigImage(p, 256, p.edited).then((u) => { if (pinPeek.isConnected) pinPeek.src = u; }).catch(() => {});
}
const hidePinPeek = () => pinPeek?.remove();

function wireMap(el) {
  const m = browse.map;
  let drag = null;
  el.addEventListener("pointerdown", (e) => {
    if (e.button !== 0 || e.target.closest(".map-pin, .map-credit")) return;
    drag = { x: e.clientX, y: e.clientY, mx: m.x, my: m.y };
    el.setPointerCapture(e.pointerId);
  });
  el.addEventListener("pointermove", (e) => {
    if (!drag) return;
    m.x = drag.mx - (e.clientX - drag.x);
    m.y = drag.my - (e.clientY - drag.y);
    drawMap();
  });
  el.addEventListener("pointerup", () => { drag = null; });
  el.addEventListener("wheel", (e) => {
    e.preventDefault();
    zoomMap(e.deltaY < 0 ? 1 : -1, [e.clientX, e.clientY]);
  }, { passive: false });
  el.addEventListener("dblclick", (e) => {
    if (!e.target.closest(".map-pin")) zoomMap(1, [e.clientX, e.clientY]);
  });
  // Dropped photos are placed where they land.
  el.addEventListener("dragover", (e) => {
    if (e.dataTransfer.types.includes("text/x-amb-photo")) e.preventDefault();
  });
  el.addEventListener("drop", (e) => {
    const path = e.dataTransfer.getData("text/x-amb-photo");
    if (!path) return;
    e.preventDefault();
    const paths = lib.selected.has(path) ? [...lib.selected] : [path];
    placePhotos(paths, mapPlaceAt(el, e.clientX, e.clientY));
  });
  // Right-click: put the selected photos here.
  el.addEventListener("contextmenu", (e) => {
    e.preventDefault();
    const sel = selectedPhotos();
    if (!sel.length) return status("Select photos first, then right-click where they were taken");
    placePhotos(sel.map((p) => p.path), mapPlaceAt(el, e.clientX, e.clientY));
  });
}

// "latitude,longitude" of a point on the map.
function mapPlaceAt(el, cx, cy) {
  const m = browse.map;
  const r = el.getBoundingClientRect();
  const [lat, lon] = unproject(m.x - r.width / 2 + (cx - r.left), m.y - r.height / 2 + (cy - r.top), m.z);
  return `${lat.toFixed(6)},${lon.toFixed(6)}`;
}

// Set (or with "" remove) the place of `paths`, in their sidecars.
async function placePhotos(paths, gps) {
  if (!paths.length) return;
  try {
    const updates = await invoke("library_set_info", { paths, change: { gps } });
    takePlaces(updates);
    status(gps ? `Placed ${updates.length} photo${updates.length === 1 ? "" : "s"} at ${gps}` : `Place removed from ${updates.length} photo${updates.length === 1 ? "" : "s"}`);
    renderBrowse();
  } catch (e) {
    toast(String(e), true);
  }
}

function takePlaces(updates) {
  for (const u of updates) {
    const p = libPhoto(u.path);
    if (!p) continue;
    const at = u.info.gps?.split(",").map(Number);
    p.gps = at?.length === 2 && at.every(Number.isFinite) ? at : null;
  }
}

async function matchGpx() {
  const gpx = await invoke("pick_gpx");
  if (!gpx) return;
  const targets = selectedPhotos().length ? selectedPhotos() : lib.shown;
  // The camera's clock was most likely on this computer's time zone then.
  const first = targets.find((p) => p.captured)?.captured;
  const guess = first ? -new Date(first.replace(" ", "T")).getTimezoneOffset() / 60 : -new Date().getTimezoneOffset() / 60;
  const answer = await askName("Camera clock",
    "How many hours the camera's clock was ahead of UTC (GPS tracks keep UTC): 2 for Amsterdam in summer, −5 for New York in winter, 0 if the camera was set to UTC.",
    String(guess));
  if (answer === null) return;
  const hours = Number(answer.replace("−", "-").replace(",", "."));
  if (!Number.isFinite(hours) || Math.abs(hours) > 14) return toast("That isn't a number of hours", true);
  status("Matching the track…");
  try {
    const r = await invoke("library_match_gpx", { gpx, paths: targets.map((p) => p.path), offsetMinutes: Math.round(hours * 60), replace: false });
    takePlaces(r.updates);
    const parts = [`Placed ${r.updates.length} photo${r.updates.length === 1 ? "" : "s"}`];
    if (r.kept) parts.push(`${r.kept} already had a place`);
    if (r.unplaced) parts.push(`${r.unplaced} taken while the track wasn't recording (it runs ${r.track[0]} to ${r.track[1]} UTC)`);
    toast(parts.join(" · "), !r.updates.length);
    browse.map.fitted = false;
    renderBrowse();
  } catch (e) {
    toast(String(e), true);
  }
}

function setMapTiles(on) {
  try { localStorage.setItem("amb.mapTiles", on ? "1" : "0"); } catch { /* not kept */ }
  if (browseOn() && cull.view === "map") renderBrowse();
}

function wireBrowse() {
  wireScrubber();
  $("lib-date").addEventListener("click", () => setDateFilter(null));
  const pref = $("settings-map-tiles");
  pref.checked = mapTilesWanted();
  pref.addEventListener("change", () => setMapTiles(pref.checked));
  new ResizeObserver(() => browseOn() && cull.view === "map" && drawMap()).observe($("library-view"));
}
