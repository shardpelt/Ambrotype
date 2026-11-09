// SPDX-License-Identifier: GPL-3.0-or-later
// The collage's free arrangement: moving, sizing and rotating the cells by
// hand. Split out of collage.js.
"use strict";

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
      else ghost.textContent = fileName(path);
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
