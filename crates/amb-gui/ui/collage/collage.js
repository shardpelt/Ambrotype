// SPDX-License-Identifier: GPL-3.0-or-later
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
  groups: null,         // "Group alike": [[path, ...], ...] best first, or null
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
  const path = await invoke("pick_export_path", { suggested: suggestedName(), format: options.format, dir: lastExportDir() });
  if (!path) return;
  rememberExportDir(path);
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
      groups: batchGroups(),
      options: exportOptions(),
      dir: lastExportDir(),
    });
    // The pages' folder: next time's start.
    if (written?.length) store.setText("amb.exportDir", dirOf(written[0]));
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
  if (collage.dirty && collage.cells.some((c) => c?.path) && !await askYes("This collage has changes that aren't saved. Open another one anyway?")) return;
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
  $("col-import").addEventListener("click", importCollageTemplate);
  $("col-fill").addEventListener("click", fillEmpty);
  $("col-group").addEventListener("click", groupTray);
  $("col-batch-groups").addEventListener("change", updateBatchNote);
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
    // Its tab lights up, however it was opened (C, the button, the tab).
    if (typeof renderTasks === "function") renderTasks();
    requestAnimationFrame(renderStage);
  } catch (e) {
    toast(String(e), true);
  }
}
