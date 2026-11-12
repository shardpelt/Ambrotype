// SPDX-License-Identifier: GPL-3.0-or-later
// The collage's layouts, the canvas form (size, margins, background) and the
// photo tray. Split out of collage.js.
"use strict";

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
    showError(e);
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
    showError(e);
  }
}

async function shareTemplate() {
  try {
    const name = $("col-name").value.trim() || collage.t.name || "Collage";
    const path = await invoke("share_collage_template", { template: { ...collage.t, name } });
    if (path) toast(`Saved ${path}`);
  } catch (e) {
    showError(e);
  }
}

async function importCollageTemplate() {
  try {
    const t = await invoke("import_collage_template");
    if (!t) return;
    const [list, saved] = await invoke("save_collage_template", { template: { id: "", builtin: false, ...t, name: t.name || "Imported" } });
    collage.templates = list;
    const entry = list.find((e) => e.id === saved);
    if (entry) applyTemplate(entry);
    toast(`Added the layout “${entry?.name ?? t.name}”`);
  } catch (e) {
    showError(e);
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
  $("col-no-borders").addEventListener("click", () => setCanvas({ gap: 0, margin: 0 }));
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
  collage.groups = null;
  renderTray();
}

// "Group alike": alike pictures and bursts in the tray (the library's
// Duplicates finder, looser), each group best first and together at the
// front, the rest after. A group's "2 × 2" button lays out its best four.
async function groupTray() {
  if (collage.groups) {
    collage.groups = null;
    return renderTray();
  }
  if (collage.tray.length < 2) return toast("Add some photos to the tray first.", true);
  $("col-tray-count").textContent = "Grouping…";
  $("col-group").disabled = true;
  try {
    const found = await invoke("library_find_duplicates", { paths: collage.tray.map((p) => p.path), burstSeconds: 2, maxBits: 10 });
    // Exact copies aren't a group to lay out; one of each is in the others.
    const groups = found.filter((g) => g.kind !== "same").map((g) => g.photos);
    if (!groups.length) {
      toast("No alike photos or bursts in the tray.");
      return updateTrayCount();
    }
    const byPath = new Map(collage.tray.map((p) => [p.path, p]));
    const grouped = new Set(groups.flat());
    collage.tray = [...groups.flat().map((p) => byPath.get(p)).filter(Boolean), ...collage.tray.filter((p) => !grouped.has(p.path))];
    collage.groups = groups;
    renderTray();
  } catch (e) {
    showError(e);
    updateTrayCount();
  } finally {
    $("col-group").disabled = false;
  }
}

// The built-in square grid, square canvas, a group's best four in it. Drag
// inside a cell to line its photo up; scroll to zoom.
function groupGrid(paths) {
  const grid = collage.templates.find((e) => e.builtin && templateOf(e).cells.length === 4
    && templateOf(e).cells.every(([, , w, h]) => w === 0.5 && h === 0.5));
  if (!grid) return toast("The 2 × 2 grid layout is missing.", true);
  applyTemplate(grid);
  const side = Math.max(collage.t.width, collage.t.height);
  setCanvas({ width: side, height: side });
  collage.cells = paths.slice(0, 4).map((path) => ({ path, placement: defaultPlacement() }));
  collage.selected = -1;
  touch();
  renderStage();
  renderTrayMarks();
  status("Drag inside a square to line its photo up; scroll to zoom");
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
  const groupOf = new Map((collage.groups ?? []).flatMap((g, n) => g.map((path) => [path, n])));
  let lastGroup = -1;
  for (const p of collage.tray) {
    const n = groupOf.get(p.path) ?? -1;
    if (collage.groups && n !== lastGroup) {
      // A head before each group, and before the photos in none.
      const head = Object.assign(document.createElement("div"), { className: "tray-group" });
      if (n >= 0) {
        const g = collage.groups[n];
        head.append(Object.assign(document.createElement("span"), { textContent: `Group ${n + 1} · ${g.length}` }));
        const make = Object.assign(document.createElement("button"), { type: "button", className: "small", textContent: "2 × 2", title: "A square of this group's best four photos" });
        make.addEventListener("click", () => groupGrid(g));
        head.append(make);
      } else {
        head.append(Object.assign(document.createElement("span"), { textContent: "Not grouped" }));
      }
      list.append(head);
      lastGroup = n;
    }
    const item = document.createElement("div");
    item.className = n >= 0 ? `tray-item grouped g${n % 2}` : "tray-item";
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
      if (collage.groups) {
        collage.groups = collage.groups.map((g) => g.filter((x) => x !== p.path)).filter((g) => g.length > 1);
        return renderTray();
      }
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
  const groups = collage.groups ? `, ${plural(collage.groups.length, "group")}` : "";
  $("col-tray-count").textContent = collage.tray.length ? `${collage.tray.length} photos${groups}` : "Empty: add photos or a folder";
  $("col-group").textContent = collage.groups ? "Ungroup" : "Group alike";
  $("col-batch-groups-row").hidden = !collage.groups;
  updateBatchNote();
}

// With "One page per group": each group's photos that pass the filter, as
// indexes into batchPhotos().
function batchGroups() {
  if (!collage.groups || !$("col-batch-groups").checked) return null;
  const at = new Map(batchPhotos().map((p, i) => [p.path, i]));
  return collage.groups.map((g) => g.filter((p) => at.has(p)).map((p) => at.get(p)));
}

function updateBatchNote() {
  const n = batchPhotos().length;
  const per = collage.t?.cells.length ?? 1;
  const groups = batchGroups();
  if (groups) {
    const pages = groups.filter((g) => g.length >= per).length;
    const small = groups.length - pages;
    $("col-batch-note").textContent = `${plural(pages, "page")}, one per group: its best ${per}.`
      + (small ? ` ${small} group${small === 1 ? " has" : "s have"} fewer than ${per} photos and ${small === 1 ? "is" : "are"} left out.` : "");
    $("col-batch").disabled = !pages || collage.busy;
    return;
  }
  const pages = Math.ceil(n / per);
  $("col-batch-note").textContent = n
    ? `${plural(n, "photo")} → ${plural(pages, "page")} of ${per} cells.`
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
    showError(e);
  }
}
