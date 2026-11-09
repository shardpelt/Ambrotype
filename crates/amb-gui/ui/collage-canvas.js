// SPDX-License-Identifier: GPL-3.0-or-later
// The collage's canvas: the page geometry, loading the images, and drawing
// the canvas. Split out of collage.js.
"use strict";

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
function collageImage(path) {
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
      collageImage(c.path).then((info) => {
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
  if (cell && c?.path) placeImage(cell, i, await collageImage(c.path));
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
    ? `Cell ${i + 1}${photo ? ": " + (trayPhoto(photo)?.name ?? fileName(photo)) : " (empty)"}`
    : "Click a cell to change it.";
}

function putPhoto(i, path) {
  if (i < 0 || i >= collage.t.cells.length) return;
  collage.cells[i] = { path, placement: defaultPlacement() };
  touch();
  renderStage();
  renderTrayMarks();
}
