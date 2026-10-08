// The collage maker: several photos on one canvas. A layout (template) is the
// shareable part: canvas size, background, gap, outer border, rounded corners
// and the cells. Photos come from the tray (the open photo's folder, or added
// photos and folders) and are dragged into cells, where they can be moved,
// zoomed, turned and opened in the editor. Export renders every photo from
// its raw with its edits (src/collages.rs); Batch fills page after page.
//
// "Arrange freely" turns the layout into frames placed by hand, like layers:
// drag to move, corner handles to resize, the top handle to turn to any
// angle, front and back to stack. The whole collage (layout, photos, how they
// sit, the tray) saves as a .ambcollage file to reopen later.
//
// The preview here mirrors the Rust layout code (amb-export/src/collage.rs):
// `cellRects` is `Template::cell_rects`, and `cropWindow` is `window`.
"use strict";

const collage = {
  templates: [],        // built-in layouts, then the user's ({ id, builtin, ...template })
  base: null,           // the layout the canvas started from
  t: null,              // the working template (a copy)
  modified: false,
  cells: [],            // per cell: { path, placement } or null
  selected: -1,
  tray: [],             // [{ path, name, rating, edited }]
  trayDir: null,        // the folder the tray was filled from (the editor's)
  small: new Map(),     // path -> { url, w, h }: tray thumbnails (also give the shape)
  big: new Map(),       // path -> Promise of { url, w, h }: canvas images
  editing: null,        // the photo opened from the collage with "Edit photo"
  docPath: null,        // the .ambcollage file this collage was opened from or saved to
  dirty: false,         // changed since it was opened or saved
  dragMode: "frame",    // free layouts: a drag moves the "frame" or the "photo" inside it
  gridCells: null,      // the grid cells before "Arrange freely", to go back to
  busy: false,
  observer: null,
  queue: [],
  loading: 0,
  wired: false,
};

const COLLAGE_THUMB = 256;
const COLLAGE_BIG = 1024;
const defaultPlacement = () => ({ cx: 0.5, cy: 0.5, zoom: 1, turns: 0 });
const MAX_CELLS = 64;
const round4 = (v) => Math.round(v * 10000) / 10000;

// ---------------------------------------------------------------- geometry

// Each cell's rectangle in canvas pixels, gaps and margin taken off (free
// layouts: fractions of the whole canvas, and an angle about the centre).
function cellRects(t) {
  if (t.free) {
    return t.cells.map(([x, y, w, h, angle = 0]) => {
      const x0 = Math.round(x * t.width);
      const y0 = Math.round(y * t.height);
      return {
        x: x0, y: y0, angle,
        w: Math.max(1, Math.round((x + w) * t.width) - x0),
        h: Math.max(1, Math.round((y + h) * t.height) - y0),
      };
    });
  }
  const short = Math.min(t.width, t.height);
  const margin = (t.margin / 100) * short;
  const half = ((t.gap / 100) * short) / 2;
  const iw = Math.max(1, t.width - 2 * margin);
  const ih = Math.max(1, t.height - 2 * margin);
  const edge = (v) => v <= 1e-6 || v >= 1 - 1e-6;
  return t.cells.map(([x, y, w, h]) => {
    const x0 = Math.max(0, Math.round(margin + x * iw + (edge(x) ? 0 : half)));
    const y0 = Math.max(0, Math.round(margin + y * ih + (edge(y) ? 0 : half)));
    const x1 = Math.round(margin + (x + w) * iw - (edge(x + w) ? 0 : half));
    const y1 = Math.round(margin + (y + h) * ih - (edge(y + h) ? 0 : half));
    return { x: x0, y: y0, w: Math.max(1, x1 - x0), h: Math.max(1, y1 - y0), angle: 0 };
  });
}

// The part of a (turned) pw × ph photo a cw × ch cell shows: cover, zoom, pan.
function cropWindow(pw, ph, cw, ch, p) {
  const target = cw / ch;
  let [w, h] = pw / ph > target ? [ph * target, ph] : [pw, pw / target];
  const zoom = Math.min(20, Math.max(1, p.zoom));
  w /= zoom;
  h /= zoom;
  const x = Math.min(Math.max(p.cx * pw - w / 2, 0), pw - w);
  const y = Math.min(Math.max(p.cy * ph - h / 2, 0), ph - h);
  return { x, y, w, h };
}

const turnedSize = (img, turns) => (turns % 2 ? [img.h, img.w] : [img.w, img.h]);

// ---------------------------------------------------------------- images

function blobUrl(buf) {
  return URL.createObjectURL(new Blob([new Uint8Array(buf)], { type: "image/jpeg" }));
}

function measure(url) {
  return new Promise((resolve, reject) => {
    const img = new Image();
    img.onload = () => resolve({ url, w: img.naturalWidth, h: img.naturalHeight });
    img.onerror = () => reject(new Error("not an image"));
    img.src = url;
  });
}

function trayPhoto(path) {
  return collage.tray.find((p) => p.path === path);
}

// The canvas image of a photo: with its edits (and crop) when it has any.
function bigImage(path) {
  if (!collage.big.has(path)) {
    const edited = path === ui.info?.path || trayPhoto(path)?.edited;
    const p = invoke(edited ? "edited_thumbnail" : "thumbnail", { path, edge: COLLAGE_BIG })
      .then((buf) => measure(blobUrl(buf)));
    p.catch(() => collage.big.delete(path));
    collage.big.set(path, p);
  }
  return collage.big.get(path);
}

function smallImage(path) {
  if (collage.small.has(path)) return Promise.resolve(collage.small.get(path));
  return invoke("thumbnail", { path, edge: COLLAGE_THUMB })
    .then((buf) => measure(blobUrl(buf)))
    .then((r) => {
      collage.small.set(path, r);
      return r;
    });
}

// ---------------------------------------------------------------- canvas

// The layout changed (so it differs from the saved layout it came from).
function markModified() {
  collage.modified = true;
  touch();
}

// Anything changed since the collage file was saved.
function touch() {
  collage.dirty = true;
  updateTitle();
}

const fileName = (p) => p.replace(/^.*[\\/]/, "");

function updateTitle() {
  const t = collage.t;
  const doc = collage.docPath ? `${fileName(collage.docPath)}${collage.dirty ? " •" : ""}` : "Not saved";
  const layout = `${t?.name || "Untitled"}${collage.modified ? " (changed)" : ""}`;
  $("col-title").textContent = `${doc} · ${layout} · ${t.width} × ${t.height} px · ${t.cells.length} ${t.free ? "frames" : "cells"}`;
}

function renderStage() {
  const t = collage.t;
  const wrap = $("col-stage-wrap");
  const stage = $("col-stage");
  const availW = Math.max(50, wrap.clientWidth - 32);
  // Room for the help line, and in free layouts the drag-mode buttons and turn handle.
  const availH = Math.max(50, wrap.clientHeight - (t.free ? 110 : 56));
  const scale = Math.min(availW / t.width, availH / t.height);
  stage.style.width = `${Math.round(t.width * scale)}px`;
  stage.style.height = `${Math.round(t.height * scale)}px`;
  stage.style.background = t.background;
  stage.classList.toggle("free", Boolean(t.free));
  stage.classList.toggle("photo-mode", Boolean(t.free) && collage.dragMode === "photo");
  stage.dataset.scale = scale;
  stage.replaceChildren();
  // Frames are clipped to the canvas (as exported); their handles are not.
  const layer = document.createElement("div");
  layer.className = "col-clip";
  stage.append(layer);
  const rects = cellRects(t);
  const radius = ((t.radius / 100) * Math.min(t.width, t.height)) * scale;
  rects.forEach((r, i) => {
    const cell = document.createElement("div");
    cell.className = "col-cell";
    cell.dataset.index = i;
    Object.assign(cell.style, {
      left: `${r.x * scale}px`, top: `${r.y * scale}px`,
      width: `${r.w * scale}px`, height: `${r.h * scale}px`,
      borderRadius: `${Math.min(radius, (Math.min(r.w, r.h) * scale) / 2)}px`,
      transform: r.angle ? `rotate(${r.angle}deg)` : "",
    });
    cell.classList.toggle("selected", i === collage.selected);
    const c = collage.cells[i];
    if (c?.path) {
      const holder = document.createElement("div");
      holder.className = "col-photo";
      const img = document.createElement("img");
      img.alt = "";
      img.draggable = false;
      holder.append(img);
      const grip = Object.assign(document.createElement("span"), { className: "col-grip", textContent: "⠿", title: "Drag to another cell to swap" });
      cell.append(holder, grip);
      cell.classList.add("loading");
      bigImage(c.path).then((info) => {
        if (collage.cells[i] !== c) return;
        img.src = info.url;
        cell.classList.remove("loading");
        placeImage(cell, i, info);
      }).catch(() => {
        cell.classList.remove("loading");
        cell.classList.add("broken");
      });
    } else {
      cell.append(Object.assign(document.createElement("span"), { className: "col-empty", textContent: "+" }));
    }
    layer.append(cell);
  });
  renderHandles();
  updateTitle();
  syncCellTools();
}

// Free layouts: the selected frame's handles, in a box that turns with it
// (outside the frame, which clips its photo).
function renderHandles() {
  $("col-stage").querySelector(".col-handles")?.remove();
  const t = collage.t;
  const i = collage.selected;
  if (!t.free || i < 0 || i >= t.cells.length) return;
  const scale = Number($("col-stage").dataset.scale);
  const r = cellRects(t)[i];
  const box = document.createElement("div");
  box.className = "col-handles";
  Object.assign(box.style, {
    left: `${r.x * scale}px`, top: `${r.y * scale}px`, width: `${r.w * scale}px`, height: `${r.h * scale}px`,
    transform: r.angle ? `rotate(${r.angle}deg)` : "",
  });
  for (const h of ["nw", "ne", "sw", "se"]) {
    box.append(Object.assign(document.createElement("span"), { className: `col-handle ${h}`, title: "Drag to resize (Shift keeps the shape)" }));
    box.lastChild.dataset.handle = h;
  }
  const rot = Object.assign(document.createElement("span"), { className: "col-rot", title: "Drag to turn (Shift: 15° steps)" });
  rot.dataset.handle = "rotate";
  box.append(rot);
  $("col-stage").append(box);
}

// Size and move the photo so the cell shows exactly what the export will.
function placeImage(cell, i, info) {
  const c = collage.cells[i];
  const holder = cell.querySelector(".col-photo");
  const img = holder?.querySelector("img");
  if (!c || !img || !info) return;
  const cw = cell.clientWidth;
  const ch = cell.clientHeight;
  const [pw, ph] = turnedSize(info, c.placement.turns);
  const win = cropWindow(pw, ph, cw, ch, c.placement);
  const s = cw / win.w;
  Object.assign(holder.style, {
    left: `${-win.x * s}px`, top: `${-win.y * s}px`, width: `${pw * s}px`, height: `${ph * s}px`,
  });
  Object.assign(img.style, {
    width: `${info.w * s}px`, height: `${info.h * s}px`,
    left: `${((pw - info.w) * s) / 2}px`, top: `${((ph - info.h) * s) / 2}px`,
    transform: `rotate(${(c.placement.turns % 4) * 90}deg)`,
  });
}

async function refreshCell(i) {
  const cell = $("col-stage").querySelector(`.col-cell[data-index="${i}"]`);
  const c = collage.cells[i];
  if (cell && c?.path) placeImage(cell, i, await bigImage(c.path));
}

// Keep a pan inside the photo: store the clamped centre.
function clampPlacement(c, info, cw, ch) {
  const [pw, ph] = turnedSize(info, c.placement.turns);
  const win = cropWindow(pw, ph, cw, ch, c.placement);
  c.placement.cx = round4((win.x + win.w / 2) / pw);
  c.placement.cy = round4((win.y + win.h / 2) / ph);
}

function selectCell(i) {
  collage.selected = i;
  for (const el of $("col-stage").querySelectorAll(".col-cell")) el.classList.toggle("selected", Number(el.dataset.index) === i);
  syncCellTools();
}

function syncCellTools() {
  const i = collage.selected;
  const has = i >= 0 && i < collage.t.cells.length;
  const photo = has && collage.cells[i]?.path;
  for (const id of ["col-rotate", "col-fit", "col-edit", "col-clear"]) $(id).disabled = !photo;
  const [, , w, h, angle = 0] = has ? collage.t.cells[i] : [0, 0, 0, 0];
  $("col-split-v").disabled = !has || w / 2 < 0.02 || collage.t.cells.length >= MAX_CELLS;
  $("col-split-h").disabled = !has || h / 2 < 0.02 || collage.t.cells.length >= MAX_CELLS;
  $("col-remove").disabled = !has || collage.t.cells.length < 2;
  const free = Boolean(collage.t.free);
  $("col-arrange").hidden = !free;
  $("col-drag-mode").hidden = !free;
  for (const id of ["col-angle", "col-straight", "col-front", "col-back"]) $(id).disabled = !has;
  $("col-add-frame").disabled = collage.t.cells.length >= MAX_CELLS;
  if (document.activeElement !== $("col-angle")) $("col-angle").value = has ? Math.round(angle * 10) / 10 : "";
  for (const b of $("col-drag-mode").querySelectorAll("button")) b.classList.toggle("active", b.dataset.mode === collage.dragMode);
  $("col-free").textContent = free ? "Back to a grid layout" : "Arrange freely";
  $("col-free").title = free
    ? "Put the photos back into the layout's grid (turns and free positions are dropped)"
    : "Move, resize, turn and overlap the photos by hand, like layers";
  for (const k of ["gap", "margin"]) $(`col-${k}`).disabled = free;
  $("col-help").textContent = free
    ? "Drag a frame to move it, its corners to resize, the round handle to turn it. Drop a photo on empty canvas for a new frame."
    : "Drag photos from below into the cells. In a cell: drag to move the photo, scroll to zoom, double-click to fit.";
  $("col-cell-hint").textContent = has
    ? `Cell ${i + 1}${photo ? ": " + (trayPhoto(photo)?.name ?? photo.replace(/^.*[\\/]/, "")) : " (empty)"}`
    : "Click a cell to change it.";
}

function putPhoto(i, path) {
  if (i < 0 || i >= collage.t.cells.length) return;
  collage.cells[i] = { path, placement: defaultPlacement() };
  touch();
  renderStage();
  renderTrayMarks();
}

// ---------------------------------------------------------------- free arrangement

// Grid → free: every cell keeps exactly where it is now. Free → grid: the
// layout it came from again (photos stay, in order).
function toggleFree() {
  const t = collage.t;
  if (t.free) {
    const n = t.cells.length;
    const base = collage.base && collage.templates.find((e) => e.id === collage.base.id);
    // The grid it came from, if it still has as many cells; else the frames
    // themselves if they happen to make a grid; else a fresh grid.
    const candidates = [collage.gridCells, base && !base.free ? templateOf(base).cells : null];
    const plain = t.cells.map(([x, y, w, h]) => [x, y, w, h].map(round4));
    const grid = candidates.find((c) => c?.length === n)
      ?? (makesGrid(plain) ? plain : null);
    const cols = Math.ceil(Math.sqrt(n));
    const rows = Math.ceil(n / cols);
    t.free = false;
    t.cells = grid ? JSON.parse(JSON.stringify(grid))
      : plain.map((_, k) => [(k % cols) / cols, Math.floor(k / cols) / rows, 1 / cols, 1 / rows].map(round4));
  } else {
    collage.gridCells = JSON.parse(JSON.stringify(t.cells));
    t.cells = cellRects(t).map((r) => [r.x / t.width, r.y / t.height, r.w / t.width, r.h / t.height].map(round4));
    t.free = true;
    collage.dragMode = "frame";
  }
  collage.cells = collage.cells.slice(0, t.cells.length);
  markModified();
  renderTemplates();
  renderStage();
}

// Straight rectangles inside the canvas that don't overlap.
function makesGrid(cells) {
  const inside = cells.every(([x, y, w, h]) => x >= 0 && y >= 0 && x + w <= 1 + 1e-6 && y + h <= 1 + 1e-6);
  const apart = cells.every((a, i) => cells.slice(i + 1).every((b) =>
    Math.min(a[0] + a[2], b[0] + b[2]) - Math.max(a[0], b[0]) <= 1e-3
    || Math.min(a[1] + a[3], b[1] + b[3]) - Math.max(a[1], b[1]) <= 1e-3));
  return inside && apart;
}

// Canvas pixels of a pointer event.
function canvasPoint(e) {
  const stage = $("col-stage");
  const box = stage.getBoundingClientRect();
  const scale = Number(stage.dataset.scale);
  return { x: (e.clientX - box.left) / scale, y: (e.clientY - box.top) / scale };
}

// Write a frame given in canvas pixels (centre, size, angle) back as fractions.
function setFrame(i, cx, cy, w, h, angle) {
  const t = collage.t;
  // Keep part of it on the canvas.
  const fw = w / t.width;
  const fh = h / t.height;
  let fx = (cx - w / 2) / t.width;
  let fy = (cy - h / 2) / t.height;
  fx = Math.min(0.98, Math.max(0.02 - fw, fx));
  fy = Math.min(0.98, Math.max(0.02 - fh, fy));
  let a = ((angle % 360) + 540) % 360 - 180;
  a = Math.round(a * 10) / 10;
  t.cells[i] = [round4(fx), round4(fy), round4(Math.min(3, fw)), round4(Math.min(3, fh))];
  if (a) t.cells[i].push(a);
}

function frameOf(i) {
  const r = cellRects(collage.t)[i];
  return { cx: r.x + r.w / 2, cy: r.y + r.h / 2, w: r.w, h: r.h, angle: r.angle };
}

// Live update of one frame while dragging (no full redraw).
function redrawFrame(i) {
  const stage = $("col-stage");
  const scale = Number(stage.dataset.scale);
  const r = cellRects(collage.t)[i];
  const style = {
    left: `${r.x * scale}px`, top: `${r.y * scale}px`, width: `${r.w * scale}px`, height: `${r.h * scale}px`,
    transform: r.angle ? `rotate(${r.angle}deg)` : "",
  };
  const cell = stage.querySelector(`.col-cell[data-index="${i}"]`);
  if (cell) Object.assign(cell.style, style);
  const handles = stage.querySelector(".col-handles");
  if (handles && i === collage.selected) Object.assign(handles.style, style);
  const c = collage.cells[i];
  if (cell && c?.path) collage.big.get(c.path)?.then((info) => placeImage(cell, i, info));
  if (document.activeElement !== $("col-angle")) $("col-angle").value = r.angle;
}

// Drag a frame (move), a corner handle (resize from the opposite corner, in
// the frame's own turned axes) or the turn handle.
function dragFrame(e, i, handle) {
  const stage = $("col-stage");
  const start = canvasPoint(e);
  const f0 = frameOf(i);
  const rad = (f0.angle * Math.PI) / 180;
  const ux = [Math.cos(rad), Math.sin(rad)];
  const uy = [-Math.sin(rad), Math.cos(rad)];
  const sx = handle?.includes("e") ? 1 : -1;
  const sy = handle?.includes("s") ? 1 : -1;
  // The corner that stays put while resizing.
  const fixed = {
    x: f0.cx - ux[0] * sx * f0.w / 2 - uy[0] * sy * f0.h / 2,
    y: f0.cy - ux[1] * sx * f0.w / 2 - uy[1] * sy * f0.h / 2,
  };
  const minPx = 0.02 * Math.min(collage.t.width, collage.t.height);
  stage.setPointerCapture(e.pointerId);
  let moved = false;
  const move = (ev) => {
    const p = canvasPoint(ev);
    if (!moved && Math.hypot(p.x - start.x, p.y - start.y) * Number(stage.dataset.scale) < 2) return;
    moved = true;
    if (!handle) {
      setFrame(i, f0.cx + p.x - start.x, f0.cy + p.y - start.y, f0.w, f0.h, f0.angle);
    } else if (handle === "rotate") {
      let a = (Math.atan2(p.y - f0.cy, p.x - f0.cx) * 180) / Math.PI + 90;
      if (ev.shiftKey) a = Math.round(a / 15) * 15;
      setFrame(i, f0.cx, f0.cy, f0.w, f0.h, a);
    } else {
      const d = [p.x - fixed.x, p.y - fixed.y];
      let w = Math.max(minPx, (d[0] * ux[0] + d[1] * ux[1]) * sx);
      let h = Math.max(minPx, (d[0] * uy[0] + d[1] * uy[1]) * sy);
      if (ev.shiftKey) {
        const k = Math.max(w / f0.w, h / f0.h);
        [w, h] = [f0.w * k, f0.h * k];
      }
      const cx = fixed.x + ux[0] * sx * w / 2 + uy[0] * sy * h / 2;
      const cy = fixed.y + ux[1] * sx * w / 2 + uy[1] * sy * h / 2;
      setFrame(i, cx, cy, w, h, f0.angle);
    }
    redrawFrame(i);
  };
  const up = () => {
    stage.removeEventListener("pointermove", move);
    stage.removeEventListener("pointerup", up);
    stage.removeEventListener("pointercancel", up);
    if (moved) {
      markModified();
      renderStage();
    }
  };
  stage.addEventListener("pointermove", move);
  stage.addEventListener("pointerup", up);
  stage.addEventListener("pointercancel", up);
}

function setAngle(i, angle) {
  if (i < 0 || !collage.t.free) return;
  const f = frameOf(i);
  setFrame(i, f.cx, f.cy, f.w, f.h, angle);
  markModified();
  renderStage();
}

// Stacking: later frames are drawn on top.
function restack(i, toFront) {
  const t = collage.t;
  if (i < 0 || t.cells.length < 2) return;
  const [cell] = t.cells.splice(i, 1);
  const [photo] = collage.cells.splice(i, 1);
  const to = toFront ? t.cells.length : 0;
  t.cells.splice(to, 0, cell);
  collage.cells.splice(to, 0, photo ?? null);
  collage.selected = to;
  markModified();
  renderStage();
}

// A new frame on top, centred at (cx, cy) canvas pixels, shaped like `aspect`.
function addFrame(path = null, aspect = 3 / 2, at = null) {
  const t = collage.t;
  if (!t.free || t.cells.length >= MAX_CELLS) return;
  const long = 0.45 * Math.min(t.width, t.height);
  const [w, h] = aspect >= 1 ? [long, long / aspect] : [long * aspect, long];
  const p = at ?? { x: t.width / 2, y: t.height / 2 };
  t.cells.push([]);
  const i = t.cells.length - 1;
  setFrame(i, p.x, p.y, w, h, 0);
  collage.cells[i] = path ? { path, placement: defaultPlacement() } : null;
  collage.selected = i;
  markModified();
  renderStage();
  renderTrayMarks();
}

function nudge(i, dx, dy) {
  const f = frameOf(i);
  setFrame(i, f.cx + dx, f.cy + dy, f.w, f.h, f.angle);
  markModified();
  redrawFrame(i);
}

function wireStage() {
  const stage = $("col-stage");
  stage.addEventListener("pointerdown", (e) => {
    if (e.button !== 0) return;
    const handle = e.target.dataset?.handle;
    if (handle && collage.selected >= 0) {
      e.preventDefault();
      dragFrame(e, collage.selected, handle);
      return;
    }
    const cell = e.target.closest(".col-cell");
    if (!cell) {
      // A click on the empty canvas lets go of the selection.
      if (collage.selected >= 0) {
        collage.selected = -1;
        renderStage();
      }
      return;
    }
    const i = Number(cell.dataset.index);
    if (collage.selected !== i) {
      collage.selected = i;
      renderStage();
    }
    const c = collage.cells[i];
    const movesFrame = collage.t.free && collage.dragMode === "frame" && !e.altKey;
    if (movesFrame && !e.target.closest(".col-grip")) {
      e.preventDefault();
      dragFrame(e, i, null);
      return;
    }
    if (!c?.path) return;
    e.preventDefault();
    if (e.target.closest(".col-grip")) {
      startDrag(e, c.path, i);
      return;
    }
    // Drag to move the photo in its cell (rotation aside: the pan follows the frame's axes).
    const live = $("col-stage").querySelector(`.col-cell[data-index="${i}"]`);
    const info = collage.big.get(c.path);
    const rad = ((collage.t.cells[i][4] ?? 0) * Math.PI) / 180;
    const start = { x: e.clientX, y: e.clientY, cx: c.placement.cx, cy: c.placement.cy };
    live.setPointerCapture(e.pointerId);
    const move = async (ev) => {
      const got = await info;
      if (!got) return;
      const [pw, ph] = turnedSize(got, c.placement.turns);
      const win = cropWindow(pw, ph, live.clientWidth, live.clientHeight, c.placement);
      const s = live.clientWidth / win.w;
      const [dx, dy] = [ev.clientX - start.x, ev.clientY - start.y];
      const [lx, ly] = [dx * Math.cos(rad) + dy * Math.sin(rad), -dx * Math.sin(rad) + dy * Math.cos(rad)];
      c.placement.cx = start.cx - lx / (pw * s);
      c.placement.cy = start.cy - ly / (ph * s);
      clampPlacement(c, got, live.clientWidth, live.clientHeight);
      placeImage(live, i, got);
    };
    const up = () => {
      live.removeEventListener("pointermove", move);
      live.removeEventListener("pointerup", up);
      live.removeEventListener("pointercancel", up);
      touch();
    };
    live.addEventListener("pointermove", move);
    live.addEventListener("pointerup", up);
    live.addEventListener("pointercancel", up);
  });
  // Scroll to zoom around the pointer.
  stage.addEventListener("wheel", async (e) => {
    const cell = e.target.closest(".col-cell");
    const i = Number(cell?.dataset.index);
    const c = collage.cells[i];
    if (!c?.path) return;
    e.preventDefault();
    const info = await collage.big.get(c.path);
    if (!info) return;
    // The pointer in the frame's own (unturned) coordinates.
    const rect = cell.getBoundingClientRect();
    const rad = ((collage.t.cells[i][4] ?? 0) * Math.PI) / 180;
    const [dx, dy] = [e.clientX - (rect.left + rect.width / 2), e.clientY - (rect.top + rect.height / 2)];
    const box = { width: cell.clientWidth, height: cell.clientHeight };
    const fx = (dx * Math.cos(rad) + dy * Math.sin(rad)) / box.width + 0.5;
    const fy = (-dx * Math.sin(rad) + dy * Math.cos(rad)) / box.height + 0.5;
    const [pw, ph] = turnedSize(info, c.placement.turns);
    const before = cropWindow(pw, ph, box.width, box.height, c.placement);
    const px = before.x + fx * before.w;
    const py = before.y + fy * before.h;
    c.placement.zoom = round4(Math.min(20, Math.max(1, c.placement.zoom * Math.exp(-e.deltaY * 0.0015))));
    const after = cropWindow(pw, ph, box.width, box.height, { ...c.placement, cx: 0.5, cy: 0.5 });
    c.placement.cx = (px - fx * after.w + after.w / 2) / pw;
    c.placement.cy = (py - fy * after.h + after.h / 2) / ph;
    clampPlacement(c, info, box.width, box.height);
    placeImage(cell, i, info);
    touch();
  }, { passive: false });
  stage.addEventListener("dblclick", (e) => {
    const i = Number(e.target.closest(".col-cell")?.dataset.index);
    if (collage.cells[i]?.path) fitCell(i);
  });
}

function fitCell(i) {
  const c = collage.cells[i];
  if (!c) return;
  c.placement = { ...defaultPlacement(), turns: c.placement.turns };
  touch();
  refreshCell(i);
}

function rotateCell(i) {
  const c = collage.cells[i];
  if (!c?.path) return;
  c.placement = { ...defaultPlacement(), turns: (c.placement.turns + 1) % 4, zoom: c.placement.zoom };
  touch();
  refreshCell(i);
}

function clearCell(i) {
  if (!collage.cells[i]) return;
  collage.cells[i] = null;
  touch();
  renderStage();
  renderTrayMarks();
}

function splitCell(i, vertical) {
  if (collage.t.cells.length >= MAX_CELLS) return;
  const [x, y, w, h, angle] = collage.t.cells[i];
  const parts = vertical
    ? [[x, y, round4(w / 2), h], [round4(x + w / 2), y, round4(w - round4(w / 2)), h]]
    : [[x, y, w, round4(h / 2)], [x, round4(y + h / 2), w, round4(h - round4(h / 2))]];
  // (A turned frame splits along its own axes only when straight; keep it simple.)
  if (angle) for (const p of parts) p.push(angle);
  collage.t.cells.splice(i, 1, ...parts);
  collage.cells.splice(i + 1, 0, null);
  markModified();
  renderStage();
}

function removeCell(i) {
  if (collage.t.cells.length < 2) return;
  collage.t.cells.splice(i, 1);
  collage.cells.splice(i, 1);
  collage.selected = -1;
  markModified();
  renderStage();
  renderTrayMarks();
}

// A photo dragged from the tray, or a cell's grip, to a cell.
function startDrag(e, path, fromCell = -1) {
  const dlg = $("collage-dialog");
  const start = { x: e.clientX, y: e.clientY };
  let ghost = null;
  let target = null;
  let lastEvent = e;
  const move = (ev) => {
    lastEvent = ev;
    if (!ghost) {
      if (Math.hypot(ev.clientX - start.x, ev.clientY - start.y) < 5) return;
      ghost = document.createElement("div");
      ghost.className = "col-ghost";
      const small = collage.small.get(path);
      if (small) ghost.append(Object.assign(document.createElement("img"), { src: small.url, alt: "" }));
      else ghost.textContent = path.replace(/^.*[\\/]/, "");
      dlg.append(ghost);
    }
    ghost.style.left = `${ev.clientX + 8}px`;
    ghost.style.top = `${ev.clientY + 8}px`;
    const hit = document.elementFromPoint(ev.clientX, ev.clientY);
    // Free layouts: dropping on the empty canvas makes a new frame there.
    const over = hit?.closest(".col-cell") ?? (collage.t.free && fromCell < 0 ? hit?.closest("#col-stage") : null);
    if (over !== target) {
      target?.classList.remove("drop");
      target = over;
      target?.classList.add("drop");
    }
  };
  const up = () => {
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", up);
    window.removeEventListener("pointercancel", up);
    target?.classList.remove("drop");
    if (!ghost) {
      // A click on a tray photo: into the selected cell, or the first empty one.
      if (fromCell < 0) {
        let i = collage.selected >= 0 ? collage.selected : collage.cells.findIndex((c, n) => !c?.path && n < collage.t.cells.length);
        if (i < 0 && collage.cells.length < collage.t.cells.length) i = collage.cells.length;
        if (i >= 0) putPhoto(i, path);
      }
      return;
    }
    ghost.remove();
    if (!target) return;
    if (target.id === "col-stage") {
      const p = canvasPoint(lastEvent);
      const small = collage.small.get(path);
      addFrame(path, small ? small.w / small.h : 3 / 2, p);
      return;
    }
    const to = Number(target.dataset.index);
    if (fromCell >= 0) {
      [collage.cells[fromCell], collage.cells[to]] = [collage.cells[to], collage.cells[fromCell]];
      for (const c of [collage.cells[fromCell], collage.cells[to]]) if (c) c.placement = { ...defaultPlacement(), turns: c.placement.turns };
      collage.selected = to;
      touch();
      renderStage();
    } else {
      selectCell(to);
      putPhoto(to, path);
    }
  };
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", up);
  window.addEventListener("pointercancel", up);
}

// ---------------------------------------------------------------- layouts

function templateOf(entry) {
  const { id, builtin, ...t } = entry;
  return JSON.parse(JSON.stringify(t));
}

function layoutIcon(t) {
  const W = 46;
  const H = Math.round((W * t.height) / t.width);
  const h = Math.min(H, 58);
  const w = Math.round((h * t.width) / t.height);
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.setAttribute("viewBox", `0 0 ${w} ${h}`);
  svg.setAttribute("width", w);
  svg.setAttribute("height", h);
  const pad = 2;
  for (const [x, y, cw, ch, angle = 0] of t.cells) {
    const r = document.createElementNS("http://www.w3.org/2000/svg", "rect");
    const [rx, ry] = [pad + x * (w - 2 * pad) + 0.75, pad + y * (h - 2 * pad) + 0.75];
    const [rw, rh] = [Math.max(1, cw * (w - 2 * pad) - 1.5), Math.max(1, ch * (h - 2 * pad) - 1.5)];
    r.setAttribute("x", rx);
    r.setAttribute("y", ry);
    r.setAttribute("width", rw);
    r.setAttribute("height", rh);
    r.setAttribute("rx", 1);
    if (angle) r.setAttribute("transform", `rotate(${angle} ${rx + rw / 2} ${ry + rh / 2})`);
    svg.append(r);
  }
  return svg;
}

function renderTemplates() {
  const list = $("col-templates");
  list.replaceChildren();
  for (const entry of collage.templates) {
    const b = document.createElement("button");
    b.type = "button";
    b.className = "col-template";
    b.classList.toggle("on", entry.id === collage.base?.id);
    b.title = entry.builtin ? `${entry.name} (only the shape is used)` : `${entry.name}: your layout (${entry.width} × ${entry.height})`;
    b.append(layoutIcon(entry), Object.assign(document.createElement("span"), { textContent: entry.name }));
    if (!entry.builtin) b.classList.add("own");
    b.addEventListener("click", () => applyTemplate(entry));
    list.append(b);
  }
  list.querySelector(".on")?.scrollIntoView({ block: "nearest" });
}

// A built-in layout changes only the shape; one of yours brings everything.
function applyTemplate(entry) {
  const t = templateOf(entry);
  if (entry.builtin && collage.t) {
    collage.t.cells = t.cells;
    collage.t.name = t.name;
    collage.t.free = false;
  } else {
    collage.t = t;
  }
  collage.base = entry;
  collage.gridCells = null;
  collage.modified = false;
  collage.cells = collage.cells.slice(0, collage.t.cells.length);
  if (collage.selected >= collage.t.cells.length) collage.selected = -1;
  for (const c of collage.cells) if (c) c.placement = { ...defaultPlacement(), turns: c.placement.turns };
  touch();
  $("col-name").value = entry.builtin ? "" : entry.name;
  $("col-delete").disabled = entry.builtin;
  syncCanvasForm();
  renderTemplates();
  renderStage();
  renderTrayMarks();
}

async function loadTemplates() {
  collage.templates = (await invoke("collage_templates")).map((e) => ({ ...e }));
}

async function saveTemplate() {
  const name = $("col-name").value.trim();
  if (!name) {
    $("col-name").focus();
    return toast("Give the layout a name first.", true);
  }
  // The same name as the layout of yours it came from updates that one.
  const id = collage.base && !collage.base.builtin && collage.base.name === name ? collage.base.id : "";
  try {
    const [list, saved] = await invoke("save_collage_template", { template: { id, builtin: false, ...collage.t, name } });
    collage.templates = list;
    collage.base = list.find((e) => e.id === saved) ?? null;
    collage.t.name = name;
    collage.modified = false;
    $("col-delete").disabled = !collage.base || collage.base.builtin;
    renderTemplates();
    updateTitle();
    toast(`Saved the layout “${name}”`);
  } catch (e) {
    toast(String(e), true);
  }
}

async function deleteTemplate() {
  if (!collage.base || collage.base.builtin) return;
  try {
    collage.templates = await invoke("delete_collage_template", { id: collage.base.id });
    toast(`Deleted “${collage.base.name}”`);
    collage.base = null;
    $("col-delete").disabled = true;
    renderTemplates();
  } catch (e) {
    toast(String(e), true);
  }
}

async function shareTemplate() {
  try {
    const name = $("col-name").value.trim() || collage.t.name || "Collage";
    const path = await invoke("share_collage_template", { template: { ...collage.t, name } });
    if (path) toast(`Saved ${path}`);
  } catch (e) {
    toast(String(e), true);
  }
}

async function importTemplate() {
  try {
    const t = await invoke("import_collage_template");
    if (!t) return;
    const [list, saved] = await invoke("save_collage_template", { template: { id: "", builtin: false, ...t, name: t.name || "Imported" } });
    collage.templates = list;
    const entry = list.find((e) => e.id === saved);
    if (entry) applyTemplate(entry);
    toast(`Added the layout “${entry?.name ?? t.name}”`);
  } catch (e) {
    toast(String(e), true);
  }
}

// ---------------------------------------------------------------- canvas form

function fillSizeSelect() {
  const sel = $("col-size");
  sel.replaceChildren();
  const option = (parent, value, text) => parent.append(Object.assign(document.createElement("option"), { value, textContent: text }));
  option(sel, "custom", "Custom size");
  const groups = new Map();
  for (const p of ui.sizes) {
    if (!groups.has(p.group)) groups.set(p.group, []);
    groups.get(p.group).push(p);
  }
  for (const [group, items] of groups) {
    const og = document.createElement("optgroup");
    og.label = group || "Yours";
    for (const p of items) option(og, `size:${p.id}`, `${p.name} (${p.width} × ${p.height})`);
    sel.append(og);
  }
}

function matchSize() {
  const p = ui.sizes.find((x) => x.width === collage.t.width && x.height === collage.t.height);
  $("col-size").value = p ? `size:${p.id}` : "custom";
}

function syncCanvasForm() {
  const t = collage.t;
  $("col-w").value = t.width;
  $("col-h").value = t.height;
  matchSize();
  for (const k of ["gap", "margin", "radius"]) {
    $(`col-${k}`).value = t[k];
    $(`col-${k}-out`).textContent = `${t[k]}%`;
  }
  $("col-bg").value = t.background.length === 7 ? t.background : "#ffffff";
  for (const b of $("col-swatches").querySelectorAll("button")) b.classList.toggle("on", b.dataset.color === t.background.toLowerCase());
}

function setCanvas(change) {
  Object.assign(collage.t, change);
  markModified();
  syncCanvasForm();
  renderStage();
}

function wireCanvasForm() {
  $("col-size").addEventListener("change", () => {
    const p = ui.sizes.find((x) => `size:${x.id}` === $("col-size").value);
    if (p) setCanvas({ width: p.width, height: p.height });
  });
  for (const id of ["col-w", "col-h"]) {
    $(id).addEventListener("change", () => {
      const w = Math.round(Number($("col-w").value));
      const h = Math.round(Number($("col-h").value));
      if (w >= 16 && h >= 16 && w <= 20000 && h <= 20000) setCanvas({ width: w, height: h });
      else syncCanvasForm();
    });
  }
  $("col-swap").addEventListener("click", () => setCanvas({ width: collage.t.height, height: collage.t.width }));
  for (const b of $("col-swatches").querySelectorAll("button")) b.addEventListener("click", () => setCanvas({ background: b.dataset.color }));
  $("col-bg").addEventListener("input", () => setCanvas({ background: $("col-bg").value }));
  for (const k of ["gap", "margin", "radius"]) {
    $(`col-${k}`).addEventListener("input", () => setCanvas({ [k]: Number($(`col-${k}`).value) }));
  }
  $("col-format").addEventListener("change", syncExportForm);
  $("col-quality").addEventListener("input", () => ($("col-quality-out").textContent = $("col-quality").value));
  $("col-batch-filter").addEventListener("change", updateBatchNote);
}

function syncExportForm() {
  const f = $("col-format").value;
  $("col-quality-row").style.visibility = ["jpeg", "avif"].includes(f) ? "visible" : "hidden";
  $("col-space").disabled = f === "avif";
  if (f === "avif") $("col-space").value = "srgb";
}

// ---------------------------------------------------------------- tray

function setTray(photos, append = false) {
  const seen = new Set(append ? collage.tray.map((p) => p.path) : []);
  const add = photos.filter((p) => !seen.has(p.path) && seen.add(p.path));
  collage.tray = append ? [...collage.tray, ...add] : add;
  // Photos you picked aren't replaced when the editor moves to another folder.
  collage.trayDir = null;
  renderTray();
}

function renderTray() {
  const list = $("col-tray");
  collage.observer?.disconnect();
  collage.queue = [];
  list.replaceChildren();
  collage.observer = new IntersectionObserver((entries) => {
    for (const e of entries) {
      if (!e.isIntersecting) continue;
      collage.observer.unobserve(e.target);
      queueTrayThumb(e.target);
    }
  }, { root: list, rootMargin: "0px 400px" });
  for (const p of collage.tray) {
    const item = document.createElement("div");
    item.className = "tray-item";
    item.dataset.path = p.path;
    item.title = `${p.name}\nDrag into a cell, or click to fill the selected (or next empty) cell.`;
    const img = Object.assign(document.createElement("img"), { alt: "", draggable: false });
    const stars = Object.assign(document.createElement("span"), { className: "tray-stars", textContent: p.rating ? "★".repeat(p.rating) : "" });
    const name = Object.assign(document.createElement("span"), { className: "tray-name", textContent: p.name });
    const used = Object.assign(document.createElement("span"), { className: "tray-used", title: "In the collage" });
    const remove = Object.assign(document.createElement("button"), { type: "button", className: "tray-remove", textContent: "×", title: "Take out of the tray" });
    remove.addEventListener("pointerdown", (e) => e.stopPropagation());
    remove.addEventListener("click", () => {
      collage.tray = collage.tray.filter((x) => x.path !== p.path);
      item.remove();
      updateTrayCount();
    });
    item.append(img, stars, name, used, remove);
    item.addEventListener("pointerdown", (e) => {
      if (e.button !== 0) return;
      e.preventDefault();
      startDrag(e, p.path);
    });
    const small = collage.small.get(p.path);
    if (small) img.src = small.url;
    else collage.observer.observe(item);
    list.append(item);
  }
  renderTrayMarks();
  updateTrayCount();
}

function queueTrayThumb(item) {
  collage.queue.push(item);
  pumpTray();
}

function pumpTray() {
  while (collage.loading < 3 && collage.queue.length) {
    const item = collage.queue.shift();
    collage.loading++;
    smallImage(item.dataset.path)
      .then((r) => (item.querySelector("img").src = r.url))
      .catch(() => item.classList.add("no-thumb"))
      .finally(() => {
        collage.loading--;
        pumpTray();
      });
  }
}

function renderTrayMarks() {
  const used = new Set(collage.cells.filter((c) => c?.path).map((c) => c.path));
  for (const item of $("col-tray").children) item.classList.toggle("used", used.has(item.dataset.path));
}

function batchPhotos() {
  const min = Number($("col-batch-filter").value);
  return collage.tray.filter((p) => (p.rating ?? 0) >= min);
}

function updateTrayCount() {
  $("col-tray-count").textContent = collage.tray.length ? `${collage.tray.length} photos` : "Empty: add photos or a folder";
  updateBatchNote();
}

function updateBatchNote() {
  const n = batchPhotos().length;
  const per = collage.t?.cells.length ?? 1;
  const pages = Math.ceil(n / per);
  $("col-batch-note").textContent = n
    ? `${n} photo${n === 1 ? "" : "s"} → ${pages} page${pages === 1 ? "" : "s"} of ${per} cells.`
    : "No photos to batch.";
  $("col-batch").disabled = !n || collage.busy;
}

function fillEmpty() {
  const used = new Set(collage.cells.filter((c) => c?.path).map((c) => c.path));
  const free = collage.tray.filter((p) => !used.has(p.path));
  for (let i = 0; i < collage.t.cells.length && free.length; i++) {
    if (!collage.cells[i]?.path) collage.cells[i] = { path: free.shift().path, placement: defaultPlacement() };
  }
  renderStage();
  renderTrayMarks();
}

async function addPhotos(folder) {
  try {
    const photos = await invoke(folder ? "pick_photo_folder" : "pick_photos");
    if (photos?.length) setTray(photos, true);
    else if (photos) toast("No photos there.", true);
  } catch (e) {
    toast(String(e), true);
  }
}

// ---------------------------------------------------------------- export

function exportOptions() {
  return {
    format: $("col-format").value,
    quality: Number($("col-quality").value),
    bits: null,
    color_space: $("col-space").value,
    long_edge: null,
    exact: null,
    sharpen: null,
    metadata: "none",
    max_kb: null,
  };
}

function templateForExport() {
  return { ...collage.t, name: $("col-name").value.trim() || collage.t.name || "Collage" };
}

function setBusy(on, text = "") {
  collage.busy = on;
  $("col-export").disabled = on;
  $("col-progress").textContent = text;
  updateBatchNote();
}

async function exportCollage() {
  if (!collage.cells.some((c) => c?.path)) return toast("Put some photos in the cells first.", true);
  const t = templateForExport();
  const options = exportOptions();
  const path = await invoke("pick_export_path", { suggested: suggestedName(), format: options.format });
  if (!path) return;
  setBusy(true, "Rendering…");
  try {
    const cells = t.cells.map((_, i) => ({ path: collage.cells[i]?.path ?? null, placement: collage.cells[i]?.placement ?? defaultPlacement() }));
    const r = await invoke("export_collage", { template: t, cells, path, options });
    toast(`Exported the collage, ${r.width}×${r.height}, ${(r.bytes / 1e6).toFixed(1)} MB`);
    status(`Exported ${r.path}`);
    // The collage file next to the image, to reopen and edit it later.
    if ($("col-with-project").checked) await saveProject(false, path.replace(/\.[^.\\/]+$/, "") + ".ambcollage");
  } catch (e) {
    toast(String(e), true);
  } finally {
    setBusy(false);
  }
}

async function exportBatch() {
  const photos = batchPhotos();
  if (!photos.length) return;
  setBusy(true, "Measuring photos…");
  try {
    // The page planner matches photo shapes to cell shapes.
    let done = 0;
    const shapes = new Array(photos.length);
    const queue = photos.map((p, i) => [p, i]);
    const worker = async () => {
      while (queue.length) {
        const [p, i] = queue.shift();
        try {
          const r = await smallImage(p.path);
          shapes[i] = r.w / r.h;
        } catch {
          shapes[i] = 1.5;
        }
        $("col-progress").textContent = `Measuring photos… ${++done} / ${photos.length}`;
      }
    };
    await Promise.all([worker(), worker(), worker(), worker()]);
    $("col-progress").textContent = "Choose a folder…";
    const written = await invoke("export_collage_batch", {
      template: templateForExport(),
      photos: photos.map((p, i) => ({ path: p.path, aspect: shapes[i] })),
      options: exportOptions(),
    });
    if (written) {
      toast(`Wrote ${written.length} pages`);
      status(`Wrote ${written.length} collage pages to ${dirOf(written[0] ?? "")}`);
    }
  } catch (e) {
    toast(String(e), true);
  } finally {
    setBusy(false);
  }
}

// ---------------------------------------------------------------- collage files

function projectIn() {
  const t = templateForExport();
  return {
    template: t,
    cells: t.cells.map((_, i) => ({ path: collage.cells[i]?.path ?? null, placement: collage.cells[i]?.placement ?? defaultPlacement() })),
    tray: collage.tray.map((p) => p.path),
    export: { format: $("col-format").value, quality: Number($("col-quality").value), color_space: $("col-space").value, with_project: $("col-with-project").checked },
  };
}

function suggestedName() {
  const name = collage.docPath ? fileName(collage.docPath).replace(/\.ambcollage$/i, "") : templateForExport().name;
  return name.replace(/[^\p{L}\p{N}]+/gu, "-").replace(/^-|-$/g, "") || "collage";
}

// Save to the collage file (asking where the first time, or always with `as`).
async function saveProject(as = false, path = null) {
  try {
    const saved = await invoke("save_collage_project", {
      project: projectIn(),
      path: path ?? (as ? null : collage.docPath),
      suggested: suggestedName(),
    });
    if (!saved) return false;
    if (!path || !collage.docPath) collage.docPath = saved;
    if (saved === collage.docPath) collage.dirty = false;
    updateTitle();
    toast(`Saved ${fileName(saved)}`);
    return true;
  } catch (e) {
    toast(String(e), true);
    return false;
  }
}

async function openProject(path = null) {
  if (collage.dirty && collage.cells.some((c) => c?.path) && !confirm("This collage has changes that aren't saved. Open another one anyway?")) return;
  try {
    const o = await invoke("open_collage_project", { path });
    if (!o) return;
    collage.t = o.template;
    // The layout it was made from, by name (one of yours first).
    collage.base = collage.templates.find((e) => e.name === o.template.name && !e.builtin)
      ?? collage.templates.find((e) => e.name === o.template.name) ?? null;
    collage.gridCells = null;
    collage.modified = false;
    collage.cells = o.cells.map((c) => (c ? { path: c[0], placement: c[1] } : null));
    collage.selected = -1;
    collage.dragMode = "frame";
    setTray(o.tray);
    const ex = o.export ?? {};
    if (ex.format) $("col-format").value = ex.format;
    if (ex.quality) $("col-quality").value = ex.quality;
    $("col-quality-out").textContent = $("col-quality").value;
    if (ex.color_space) $("col-space").value = ex.color_space;
    if (typeof ex.with_project === "boolean") $("col-with-project").checked = ex.with_project;
    syncExportForm();
    collage.docPath = o.path;
    collage.dirty = false;
    $("col-name").value = o.template.name ?? "";
    syncCanvasForm();
    renderTemplates();
    renderStage();
    if (o.missing.length) {
      toast(`${o.missing.length} photo${o.missing.length === 1 ? " wasn't" : "s weren't"} found: ${o.missing.map(fileName).join(", ")}`, true);
    }
  } catch (e) {
    toast(String(e), true);
  }
}

// A .ambcollage file opened from outside (dropped on the window, or given on the command line).
async function openCollageFile(path) {
  await collageFlow();
  await openProject(path);
}

// ---------------------------------------------------------------- open, close, edit

function closeCollage() {
  $("collage-dialog").close();
}

// "Edit photo": the editor opens it; Back to collage returns with the edits.
async function editInEditor(i) {
  const path = collage.cells[i]?.path;
  if (!path) return;
  closeCollage();
  collage.editing = path;
  if (path !== ui.info?.path && !(await openPath(path))) {
    collage.editing = null;
    return collageFlow();
  }
  $("btn-back-collage").hidden = false;
}

function wireCollage() {
  if (collage.wired) return;
  collage.wired = true;
  wireStage();
  wireCanvasForm();
  fillSizeSelect();
  AmbPicker.enhance($("col-size"), { open: ["Common"] });
  $("col-close").addEventListener("click", closeCollage);
  $("col-export").addEventListener("click", exportCollage);
  $("col-batch").addEventListener("click", exportBatch);
  $("col-save").addEventListener("click", saveTemplate);
  $("col-delete").addEventListener("click", deleteTemplate);
  $("col-share").addEventListener("click", shareTemplate);
  $("col-import").addEventListener("click", importTemplate);
  $("col-fill").addEventListener("click", fillEmpty);
  $("col-add").addEventListener("click", () => addPhotos(false));
  $("col-add-folder").addEventListener("click", () => addPhotos(true));
  $("col-tray-clear").addEventListener("click", () => setTray([]));
  $("col-rotate").addEventListener("click", () => rotateCell(collage.selected));
  $("col-fit").addEventListener("click", () => fitCell(collage.selected));
  $("col-clear").addEventListener("click", () => clearCell(collage.selected));
  $("col-edit").addEventListener("click", () => editInEditor(collage.selected));
  $("col-split-v").addEventListener("click", () => splitCell(collage.selected, true));
  $("col-split-h").addEventListener("click", () => splitCell(collage.selected, false));
  $("col-remove").addEventListener("click", () => removeCell(collage.selected));
  $("btn-back-collage").addEventListener("click", () => collageFlow());
  $("col-open").addEventListener("click", () => openProject());
  $("col-save-doc").addEventListener("click", () => saveProject(false));
  $("col-save-as").addEventListener("click", () => saveProject(true));
  $("col-free").addEventListener("click", toggleFree);
  for (const b of $("col-drag-mode").querySelectorAll("button")) {
    b.addEventListener("click", () => {
      collage.dragMode = b.dataset.mode;
      renderStage();
    });
  }
  $("col-angle").addEventListener("change", () => {
    const a = Number($("col-angle").value);
    if (Number.isFinite(a)) setAngle(collage.selected, a);
  });
  $("col-straight").addEventListener("click", () => setAngle(collage.selected, 0));
  $("col-front").addEventListener("click", () => restack(collage.selected, true));
  $("col-back").addEventListener("click", () => restack(collage.selected, false));
  $("col-add-frame").addEventListener("click", () => addFrame());
  $("col-with-project").addEventListener("change", touch);
  $("collage-dialog").addEventListener("keydown", (e) => {
    const ctrl = e.ctrlKey || e.metaKey;
    const key = e.key.toLowerCase();
    if (ctrl && key === "s") saveProject(e.shiftKey);
    else if (ctrl && key === "o") openProject();
    else if (e.target.matches("input, select, textarea") || collage.selected < 0) return;
    else if (e.key === "Delete" || e.key === "Backspace") clearCell(collage.selected);
    else if (key === "r" && !ctrl) rotateCell(collage.selected);
    else if (collage.t.free && e.key.startsWith("Arrow")) {
      const step = e.shiftKey ? 10 : 1;
      const d = { ArrowLeft: [-step, 0], ArrowRight: [step, 0], ArrowUp: [0, -step], ArrowDown: [0, step] }[e.key];
      nudge(collage.selected, ...d);
    } else if (collage.t.free && (e.key === "]" || e.key === "[")) restack(collage.selected, e.key === "]");
    else return;
    e.preventDefault();
  });
  new ResizeObserver(() => {
    if ($("collage-dialog").open) renderStage();
  }).observe($("col-stage-wrap"));
  tauri.event.listen("collage-progress", (e) => {
    const p = e.payload;
    if (!collage.busy) return;
    $("col-progress").textContent = p.pages > 1
      ? `Page ${p.page} of ${p.pages} · photo ${p.done} of ${p.total}`
      : `Photo ${p.done} of ${p.total}`;
  });
  syncExportForm();
}

async function collageFlow() {
  try {
    wireCollage();
    if (!collage.templates.length) await loadTemplates();
    if (!collage.t) {
      applyTemplate(collage.templates[0]);
      collage.dirty = false;
    }
    // The open photo's folder fills an empty tray (or follows the editor to a new folder).
    const dir = ui.info ? dirOf(ui.info.path) : null;
    if (dir && strip.dir === dir && strip.photos.length && (!collage.tray.length || (collage.trayDir && collage.trayDir !== dir))) {
      setTray(strip.photos.map((p) => ({ ...p })));
      collage.trayDir = dir;
    }
    // Edits made since the canvas last showed a photo.
    for (const path of [ui.info?.path, collage.editing]) {
      if (!path) continue;
      collage.big.delete(path);
      const p = trayPhoto(path);
      if (p && path === collage.editing) p.edited = true;
    }
    collage.editing = null;
    $("btn-back-collage").hidden = true;
    renderTemplates();
    updateTrayCount();
    $("collage-dialog").showModal();
    requestAnimationFrame(renderStage);
  } catch (e) {
    toast(String(e), true);
  }
}
