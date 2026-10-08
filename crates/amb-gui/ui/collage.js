// The collage maker: several photos on one canvas. A layout (template) is the
// shareable part: canvas size, background, gap, outer border, rounded corners
// and the cells. Photos come from the tray (the open photo's folder, or added
// photos and folders) and are dragged into cells, where they can be moved,
// zoomed, turned and opened in the editor. Export renders every photo from
// its raw with its edits (src/collages.rs); Batch fills page after page.
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
  busy: false,
  observer: null,
  queue: [],
  loading: 0,
  wired: false,
};

const COLLAGE_THUMB = 256;
const COLLAGE_BIG = 1024;
const defaultPlacement = () => ({ cx: 0.5, cy: 0.5, zoom: 1, turns: 0 });
const round4 = (v) => Math.round(v * 10000) / 10000;

// ---------------------------------------------------------------- geometry

// Each cell's rectangle in canvas pixels, gaps and margin taken off.
function cellRects(t) {
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
    return { x: x0, y: y0, w: Math.max(1, x1 - x0), h: Math.max(1, y1 - y0) };
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

function markModified() {
  collage.modified = true;
  updateTitle();
}

function updateTitle() {
  const name = collage.t?.name || "Untitled";
  $("col-title").textContent = `${name}${collage.modified ? " (changed)" : ""} · ${collage.t.width} × ${collage.t.height} px · ${collage.t.cells.length} cells`;
}

function renderStage() {
  const t = collage.t;
  const wrap = $("col-stage-wrap");
  const stage = $("col-stage");
  const availW = Math.max(50, wrap.clientWidth - 32);
  const availH = Math.max(50, wrap.clientHeight - 56);
  const scale = Math.min(availW / t.width, availH / t.height);
  stage.style.width = `${Math.round(t.width * scale)}px`;
  stage.style.height = `${Math.round(t.height * scale)}px`;
  stage.style.background = t.background;
  stage.replaceChildren();
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
    stage.append(cell);
  });
  updateTitle();
  syncCellTools();
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
  const [, , w, h] = has ? collage.t.cells[i] : [0, 0, 0, 0];
  $("col-split-v").disabled = !has || w / 2 < 0.02;
  $("col-split-h").disabled = !has || h / 2 < 0.02;
  $("col-remove").disabled = !has || collage.t.cells.length < 2;
  $("col-cell-hint").textContent = has
    ? `Cell ${i + 1}${photo ? ": " + (trayPhoto(photo)?.name ?? photo.replace(/^.*[\\/]/, "")) : " (empty)"}`
    : "Click a cell to change it.";
}

function putPhoto(i, path) {
  if (i < 0 || i >= collage.t.cells.length) return;
  collage.cells[i] = { path, placement: defaultPlacement() };
  renderStage();
  renderTrayMarks();
}

function wireStage() {
  const stage = $("col-stage");
  stage.addEventListener("pointerdown", (e) => {
    const cell = e.target.closest(".col-cell");
    if (!cell || e.button !== 0) return;
    const i = Number(cell.dataset.index);
    selectCell(i);
    const c = collage.cells[i];
    if (!c?.path) return;
    e.preventDefault();
    if (e.target.closest(".col-grip")) {
      startDrag(e, c.path, i);
      return;
    }
    // Drag to move the photo in its cell.
    const info = collage.big.get(c.path);
    const start = { x: e.clientX, y: e.clientY, cx: c.placement.cx, cy: c.placement.cy };
    cell.setPointerCapture(e.pointerId);
    const move = async (ev) => {
      const got = await info;
      if (!got) return;
      const [pw, ph] = turnedSize(got, c.placement.turns);
      const win = cropWindow(pw, ph, cell.clientWidth, cell.clientHeight, c.placement);
      const s = cell.clientWidth / win.w;
      c.placement.cx = start.cx - (ev.clientX - start.x) / (pw * s);
      c.placement.cy = start.cy - (ev.clientY - start.y) / (ph * s);
      clampPlacement(c, got, cell.clientWidth, cell.clientHeight);
      placeImage(cell, i, got);
    };
    const up = () => {
      cell.removeEventListener("pointermove", move);
      cell.removeEventListener("pointerup", up);
      cell.removeEventListener("pointercancel", up);
    };
    cell.addEventListener("pointermove", move);
    cell.addEventListener("pointerup", up);
    cell.addEventListener("pointercancel", up);
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
    const box = cell.getBoundingClientRect();
    const fx = (e.clientX - box.left) / box.width;
    const fy = (e.clientY - box.top) / box.height;
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
  refreshCell(i);
}

function rotateCell(i) {
  const c = collage.cells[i];
  if (!c?.path) return;
  c.placement = { ...defaultPlacement(), turns: (c.placement.turns + 1) % 4, zoom: c.placement.zoom };
  refreshCell(i);
}

function clearCell(i) {
  if (!collage.cells[i]) return;
  collage.cells[i] = null;
  renderStage();
  renderTrayMarks();
}

function splitCell(i, vertical) {
  const [x, y, w, h] = collage.t.cells[i];
  const parts = vertical
    ? [[x, y, round4(w / 2), h], [round4(x + w / 2), y, round4(w - round4(w / 2)), h]]
    : [[x, y, w, round4(h / 2)], [x, round4(y + h / 2), w, round4(h - round4(h / 2))]];
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
  const move = (ev) => {
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
    const over = document.elementFromPoint(ev.clientX, ev.clientY)?.closest(".col-cell");
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
    const to = Number(target.dataset.index);
    if (fromCell >= 0) {
      [collage.cells[fromCell], collage.cells[to]] = [collage.cells[to], collage.cells[fromCell]];
      for (const c of [collage.cells[fromCell], collage.cells[to]]) if (c) c.placement = { ...defaultPlacement(), turns: c.placement.turns };
      selectCell(to);
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
  for (const [x, y, cw, ch] of t.cells) {
    const r = document.createElementNS("http://www.w3.org/2000/svg", "rect");
    r.setAttribute("x", pad + x * (w - 2 * pad) + 0.75);
    r.setAttribute("y", pad + y * (h - 2 * pad) + 0.75);
    r.setAttribute("width", Math.max(1, cw * (w - 2 * pad) - 1.5));
    r.setAttribute("height", Math.max(1, ch * (h - 2 * pad) - 1.5));
    r.setAttribute("rx", 1);
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
  } else {
    collage.t = t;
  }
  collage.base = entry;
  collage.modified = false;
  collage.cells = collage.cells.slice(0, collage.t.cells.length);
  if (collage.selected >= collage.t.cells.length) collage.selected = -1;
  for (const c of collage.cells) if (c) c.placement = { ...defaultPlacement(), turns: c.placement.turns };
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
    else if (photos) toast("No raw photos there.", true);
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
  const suggested = t.name.replace(/[^\p{L}\p{N}]+/gu, "-").replace(/^-|-$/g, "") || "collage";
  const path = await invoke("pick_export_path", { suggested, format: options.format });
  if (!path) return;
  setBusy(true, "Rendering…");
  try {
    const cells = t.cells.map((_, i) => ({ path: collage.cells[i]?.path ?? null, placement: collage.cells[i]?.placement ?? defaultPlacement() }));
    const r = await invoke("export_collage", { template: t, cells, path, options });
    toast(`Exported the collage, ${r.width}×${r.height}, ${(r.bytes / 1e6).toFixed(1)} MB`);
    status(`Exported ${r.path}`);
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
  $("collage-dialog").addEventListener("keydown", (e) => {
    if (e.target.matches("input, select, textarea") || collage.selected < 0) return;
    if (e.key === "Delete" || e.key === "Backspace") clearCell(collage.selected);
    else if (e.key === "r" || e.key === "R") rotateCell(collage.selected);
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
    if (!collage.t) applyTemplate(collage.templates[0]);
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
