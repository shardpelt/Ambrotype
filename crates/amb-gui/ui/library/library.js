// SPDX-License-Identifier: GPL-3.0-or-later
// The library: the raws of a folder as a grid, to sort, filter, select many,
// and rate, label and keyword them at once, paste one photo's edit onto
// others, export them, or start a collage with them. G switches between the
// library and the editor; Enter or a double-click opens a photo.
//
// It reads and writes the photos' XMP sidecars through src/library.rs; the
// open photo's changes go through its session, so the editor stays in step.
"use strict";

const lib = {
  dirs: [], // folder tiles shown before the photos (renderSubfolders)
  on: false,
  dir: null,
  more: [],            // other folders shown with it
  photos: [],          // [{ path, name, rating, label, keywords, flag, edited, captured }]
  shown: [],           // after filter and sort
  selected: new Set(), // paths
  anchor: null,        // where a Shift-click range starts
  focus: null,         // the photo arrow keys move from
  cells: new Map(),    // path -> cell element
  urls: new Map(),     // `${path}|${edge}|${edited}` -> object URL
  queue: [],
  editedQueue: [],
  loading: 0,
  observer: null,
  collection: null,    // the collection shown instead of the folder, by name
  smart: null,         // ... when it's a smart collection, its name too
  // A Lightroom catalogue's collection shown ({ catalogue, name }; lightroom.js).
  lightroom: null,
  expanded: new Set(), // stack ids shown open
  stackSizes: new Map(), // stack id -> photos in it
  copied: null,        // the photo "Copy settings" took its edit from
  groups: null,        // setting groups for Paste settings
  pick: null,          // only the photos in one Insights bar: { words, test } (insights.js)
  beforePick: null,    // what the filters show before it
  date: null,          // only photos taken then: "2026", "2026-07" or "2026-07-14" (browse.js)
  beforeDate: null,    // what the filters show before the date filter
  wired: false,
};

const libraryOn = () => lib.on;
const libPhoto = (path) => lib.photos.find((p) => p.path === path);
// The photos an action is for: the library's selection, or in the editor
// the filmstrip's (Ctrl- and Shift-click there).
const selectedPhotos = () => (lib.on ? lib.shown.filter((p) => lib.selected.has(p.path)) : stripSelected());
const VIEWER_BUTTONS = ["btn-before", "btn-crop", "btn-masks", "btn-spots", "btn-zoom", "btn-wb", "btn-clip", "btn-frame", "btn-auto", "btn-select"];

// ---------------------------------------------------------------- show, load

async function toggleLibrary(on = !lib.on) {
  wireLibrary();
  if (on === lib.on) return;
  if (on && ui.crop.on) await exitCrop(true);
  if (on) {
    exitMasks();
    exitSpots();
  }
  if (!on) setLibraryView("grid");
  lib.on = on;
  // The library is only about organising: the editor's top bar goes, and
  // its name and task switcher come into the library's own bar.
  const tasks = $("task-bar");
  const brand = document.querySelector(".brand");
  if (on) $("lib-task-slot").append(brand, tasks);
  else {
    document.querySelector(".topbar").prepend(brand);
    brand.after(tasks);
  }
  $("workspace").hidden = on;
  $("library").hidden = !on;
  document.body.classList.toggle("library-mode", on);
  renderTasks();
  $("btn-library")?.classList.toggle("active", on);
  for (const id of VIEWER_BUTTONS) $(id).disabled = on || !ui.info;
  $("btn-export").disabled = !on && !ui.info;
  if (on) {
    const dir = ui.info ? dirOf(ui.info.path) : lib.dir;
    if (!lib.collection && dir && ((dir !== lib.dir && !lib.more.includes(dir)) || !lib.photos.length)) await loadLibrary(dir);
    else renderLibrary();
    // The open photo may have been edited since: its mark and thumbnail.
    const open = ui.info && libPhoto(ui.info.path);
    if (open) {
      const edited = Object.keys(ui.settings).length > 0;
      if (edited || open.edited) refreshThumb(open.path);
      open.edited = edited;
      updateCell(open);
    }
    if (ui.info && libPhoto(ui.info.path)) {
      if (!lib.selected.size) select(ui.info.path, "only");
      revealPhoto(ui.info.path, "center");
    }
    $("library-grid").focus({ preventScroll: true });
  } else {
    requestPreview(false);
    if (ui.info) loadInfo();
  }
}

// The folders shown with each folder last time, kept between runs.
const SAVED_MORE = "amb.lib.more";

function savedMore(dir) {
  try {
    return JSON.parse(localStorage.getItem(SAVED_MORE) || "{}")[dir] ?? [];
  } catch {
    return [];
  }
}

function saveMore(dir, more) {
  try {
    const all = JSON.parse(localStorage.getItem(SAVED_MORE) || "{}");
    delete all[dir];
    if (more.length) all[dir] = more;
    // The most recent few dozen folders.
    const keys = Object.keys(all);
    for (const k of keys.slice(0, Math.max(0, keys.length - 40))) delete all[k];
    localStorage.setItem(SAVED_MORE, JSON.stringify(all));
  } catch {
    /* not kept */
  }
}

// Show a folder; `more` folders are shown with it (kept on a reload of the
// same folder, and the ones shown with it last time otherwise, unless given).
async function loadLibrary(dir, more = dir === lib.dir ? lib.more : savedMore(dir)) {
  lib.dir = dir;
  lib.more = more.filter((d) => d !== dir);
  if (dir) saveMore(dir, lib.more);
  lib.collection = null;
  lib.smart = null;
  lib.lightroom = null;
  const folder = $("library-folder");
  folder.textContent = dir ?? "";
  folder.title = [dir, ...lib.more].filter(Boolean).join("\n");
  $("lib-drop-folders").hidden = !lib.more.length;
  $("lib-drop-folders").textContent = `${lib.more.length} more ×`;
  $("library-count").textContent = "Reading the folder…";
  try {
    lib.photos = await invoke("library_list", { dir, more: lib.more, subfolders: $("lib-subfolders").checked });
  } catch (e) {
    lib.photos = [];
    toast(`Couldn't read the folder: ${e}`, true);
  }
  const keep = new Set(lib.photos.map((p) => p.path));
  lib.selected = new Set([...lib.selected].filter((p) => keep.has(p)));
  renderLibrary();
  renderSubfolders();
  renderFolderTree();
  watchShownFolders();
  if (typeof renderCollectionControls === "function") renderCollectionControls();
}

async function chooseLibraryFolder() {
  const dir = await invoke("library_pick_folder");
  if (!dir) return;
  lib.selected.clear();
  await loadLibrary(dir);
}

// Show another folder alongside the ones shown.
async function addLibraryFolder() {
  const dir = await invoke("library_pick_folder");
  if (!dir) return;
  if (!lib.dir || lib.collection) return loadLibrary(dir, []);
  if (dir !== lib.dir && !lib.more.includes(dir)) await loadLibrary(lib.dir, [...lib.more, dir]);
}

// ---------------------------------------------------------------- selection

// how: "only" (a plain click), "toggle" (Ctrl/Cmd), "range" (Shift).
function select(path, how = "only") {
  if (how === "range" && lib.anchor) {
    const order = lib.shown.map((p) => p.path);
    const [a, b] = [order.indexOf(lib.anchor), order.indexOf(path)].sort((x, y) => x - y);
    if (a >= 0 && b >= 0) for (const p of order.slice(a, b + 1)) lib.selected.add(p);
  } else if (how === "toggle") {
    if (lib.selected.has(path)) lib.selected.delete(path);
    else lib.selected.add(path);
    lib.anchor = path;
  } else {
    lib.selected = new Set([path]);
    lib.anchor = path;
  }
  lib.focus = path;
  renderSelection();
}

function librarySelectAll() {
  lib.selected = new Set(lib.shown.map((p) => p.path));
  renderSelection();
}

function librarySelectNone() {
  lib.selected.clear();
  renderSelection();
}

function renderSelection() {
  for (const [path, cell] of lib.cells) {
    cell.classList.toggle("selected", lib.selected.has(path));
    cell.classList.toggle("focus", path === lib.focus);
  }
  renderCounts();
  renderPanel();
}

// Arrow keys move through the grid (Shift extends the selection).
function moveFocus(key, extend) {
  const order = lib.shown.map((p) => p.path);
  if (!order.length) return;
  const { cols } = gridMetrics();
  const at = Math.max(0, order.indexOf(lib.focus));
  const step = { ArrowLeft: -1, ArrowRight: 1, ArrowUp: -cols, ArrowDown: cols }[key];
  const next = order[Math.min(order.length - 1, Math.max(0, at + step))];
  select(next, extend ? "range" : "only");
  revealPhoto(next);
}

// ---------------------------------------------------------------- the panel: rate, label, keywords

function renderPanel() {
  // In compare the panel is about the active photo.
  const sel = libraryTargets();
  const n = sel.length;
  const active = cullOn() && cull.view !== "loupe";
  $("lib-sel-count").textContent = active ? (sel[0] ? `${sel[0].name} (active)` : "No active photo")
    : n ? `${n} photo${n === 1 ? "" : "s"} selected` : "Nothing selected";
  $("lib-hint").textContent = active
    ? "Changes go to the active photo (the framed one). Click a photo to make it active; G or Esc goes back to the grid."
    : "Click to select, Shift for a range, Ctrl (⌘) for more. Changes go to every selected photo's .xmp at once.";
  // What can't be done for this selection is left out rather than greyed.
  for (const el of $("library-panel").querySelectorAll("[data-needs]")) {
    const need = el.dataset.needs;
    el.classList.toggle("needs-off", need === "one" ? n !== 1 : need === "many" ? n < 2
      : need === "paste" ? !lib.copied || !sel.some((p) => p.path !== lib.copied) : n === 0);
  }
  $("library-panel").classList.toggle("empty", !n && !active);
  $("lib-hint").hidden = Boolean(n) && !active;
  // What the selection has in common.
  const same = (f) => (n && sel.every((p) => f(p) === f(sel[0])) ? f(sel[0]) : undefined);
  const rating = same((p) => p.rating ?? 0);
  for (const b of $("lib-rating").querySelectorAll("button[data-stars]")) {
    b.classList.toggle("on", rating !== undefined && Number(b.dataset.stars) <= rating);
    b.classList.toggle("mixed", n > 1 && rating === undefined);
  }
  const label = same((p) => p.label ?? null);
  for (const b of $("lib-labels").querySelectorAll("button")) b.classList.toggle("on", label !== undefined && b.dataset.label === label);
  const flag = same((p) => p.flag ?? "");
  for (const b of $("lib-flags").querySelectorAll("button")) b.classList.toggle("on", flag !== undefined && b.dataset.flag === flag);
  // Keywords: on all of them (× removes), or on some (+ adds to the rest).
  const counts = new Map();
  for (const p of sel) for (const k of p.keywords) {
    const key = k.toLowerCase();
    counts.set(key, { name: counts.get(key)?.name ?? k, n: (counts.get(key)?.n ?? 0) + 1 });
  }
  const chips = $("lib-keywords");
  chips.replaceChildren(...[...counts.values()].sort((a, b) => b.n - a.n || a.name.localeCompare(b.name)).map(({ name, n: count }) => {
    const all = count === n;
    const chip = Object.assign(document.createElement("span"), { className: `tag-chip${all ? "" : " partial"}`, textContent: name, title: all ? "On every selected photo" : `On ${count} of ${n}` });
    if (!all) {
      const add = Object.assign(document.createElement("button"), { type: "button", textContent: "+", title: `Add "${name}" to all ${n}` });
      add.addEventListener("click", () => libraryChange({ add_keywords: [name] }));
      chip.append(add);
    }
    const x = Object.assign(document.createElement("button"), { type: "button", textContent: "×", title: `Remove "${name}" from the selection` });
    x.addEventListener("click", () => libraryChange({ remove_keywords: [name] }));
    chip.append(x);
    return chip;
  }));
  $("lib-tag-input").disabled = n === 0;
  // Recent keywords not on every selected photo yet: one click adds.
  const recent = n ? infoState.recent.filter((k) => counts.get(k.toLowerCase())?.n !== n).slice(0, 12) : [];
  $("lib-tag-recent").replaceChildren(...recent.map((k) => {
    const b = Object.assign(document.createElement("button"), { type: "button", className: "tag-recent", textContent: `+ ${k}`, title: `Add "${k}" to the ${n} selected` });
    b.addEventListener("click", () => libraryChange({ add_keywords: [k] }));
    return b;
  }));
  renderPlaceField(sel);
  // Title and the like: the shared text, or empty with "(mixed)".
  for (const f of $("lib-meta").querySelectorAll("[data-meta]")) {
    const v = same((p) => p[f.dataset.meta] ?? "");
    if (document.activeElement !== f) f.value = v ?? "";
    f.placeholder = n > 1 && v === undefined ? "(mixed)" : "";
  }
  $("lib-tag-suggest").replaceChildren(...infoState.recent.slice(0, 60).map((k) => Object.assign(document.createElement("option"), { value: k })));
  $("lib-copied").textContent = lib.copied ? `Copied from ${fileName(lib.copied)}` : "";
  $("lib-export").textContent = n > 1 ? `Export ${n} photos…` : "Export…";
  $("lib-vcopy-delete").hidden = !(n && sel.every((p) => p.copy));
  $("lib-paste").textContent = n ? `Paste settings to ${sel.filter((p) => p.path !== lib.copied).length}…` : "Paste settings…";
}

// Rate, label or keyword the whole selection.
async function libraryChange(change) {
  const paths = libraryTargets().map((p) => p.path);
  if (!paths.length) return;
  try {
    const updates = await setInfo(paths, change);
    for (const u of updates) {
      const p = libPhoto(u.path);
      if (!p) continue;
      Object.assign(p, { rating: u.info.rating, label: u.info.label, keywords: u.info.keywords, flag: u.info.flag, stack: u.info.stack ?? null,
        title: u.info.title, caption: u.info.caption, creator: u.info.creator, copyright: u.info.copyright });
      updateCell(p);
      updateCullTile(u.path);
      stripUpdate(u.path, { rating: u.info.rating, label: u.info.label, keywords: u.info.keywords, flag: u.info.flag ?? null });
    }
    if (ui.info && paths.includes(ui.info.path)) loadInfo();
    // Keywords just used come first among the recent ones.
    for (const k of [...(change.add_keywords ?? [])].reverse()) {
      infoState.recent = [k, ...infoState.recent.filter((r) => r.toLowerCase() !== k.toLowerCase())];
    }
    const what = change.rating !== undefined ? (change.rating ? `Rated ${"★".repeat(change.rating)}` : "Rating cleared")
      : change.label !== undefined ? (change.label ? `${change.label} label` : "Label removed")
      : change.flag !== undefined ? FLAG_DONE[change.flag]
      : change.add_keywords || change.remove_keywords ? "Keywords changed"
      : "Saved";
    status(`${what}: ${updates.length} photo${updates.length === 1 ? "" : "s"}`);
    applyViewKeepingCells();
  } catch (e) {
    toast(String(e), true);
  }
}

// After a change, the filter may hide photos: redraw only if the shown set changed.
function applyViewKeepingCells() {
  const before = lib.shown.map((p) => p.path).join("\n");
  applyView();
  if (lib.shown.map((p) => p.path).join("\n") !== before) renderLibrary();
  else renderSelection();
}

// Keys: in the library they act on the selection; in the editor on the open photo.
function rateKey(n) {
  if (lib.on) return advancing(libraryChange({ rating: n }));
  return ui.info ? advancing(setRating(n)) : false;
}

function labelKey(label) {
  if (lib.on) {
    const sel = libraryTargets();
    if (!sel.length) return false;
    return advancing(libraryChange({ label: sel.every((p) => p.label === label) ? "" : label }));
  }
  return ui.info ? advancing(setLabel(label)) : false;
}

function flagKey(flag) {
  if (lib.on) return libraryTargets().length ? advancing(libraryChange({ flag })) : false;
  return ui.info ? advancing(setFlag(flag)) : false;
}

// Auto-advance (Settings → General): after rating one photo from the
// keyboard, go to the next, to cull a card quickly. Not with several
// selected, nor in compare.
function autoAdvanceOn() {
  return store.text("amb.autoAdvance") === "1";
}

function advancing(change) {
  if (!autoAdvanceOn()) return change;
  let next = null;
  if (!lib.on) next = "editor";
  else if (!(cullOn() && cull.view !== "loupe") && libraryTargets().length === 1) {
    // Chosen before the change: a filter may hide the photo just rated.
    const order = lib.shown.map((p) => p.path);
    next = order[order.indexOf(lib.focus) + 1] ?? null;
  }
  Promise.resolve(change).then(() => {
    if (next === "editor") stepPhoto(1);
    else if (next && lib.on) {
      select(next, "only");
      revealPhoto(next);
    }
  });
  return change;
}

function keywordsKey() {
  if (lib.on) return $("lib-tag-input").disabled ? false : $("lib-tag-input").focus();
  return ui.info ? $("tag-input").focus() : false;
}

function addLibraryTyped() {
  const input = $("lib-tag-input");
  const parts = input.value.split(",").map((k) => k.trim()).filter(Boolean);
  input.value = "";
  if (parts.length) libraryChange({ add_keywords: parts });
}

// ---------------------------------------------------------------- wiring

function wireLibrary() {
  const advance = $("settings-auto-advance");
  advance.checked = autoAdvanceOn();
  advance.addEventListener("change", () => {
    store.setText("amb.autoAdvance", advance.checked ? "1" : "0");
  });
  if (lib.wired) return;
  lib.wired = true;
  wireBrowse();
  wireDuplicates();
  wireCleanup();
  wireBursts();
  wireOrganise();
  $("lib-settings").addEventListener("click", () => $("btn-settings").click());
  // Title, caption, creator and copyright for the selection, when a field is left.
  $("lib-stack").addEventListener("click", stackSelection);
  $("lib-unstack").addEventListener("click", unstackSelection);
  $("lib-autostack").addEventListener("click", autoStack);
  wirePlaceField();
  wirePlacePicker();
  for (const f of $("lib-meta").querySelectorAll("[data-meta]")) {
    f.addEventListener("change", () => libraryChange({ [f.dataset.meta]: f.value.trim() }));
    f.addEventListener("keydown", (e) => e.stopPropagation());
  }
  const grid = $("library-grid");
  // Rows come and go as the grid scrolls or changes width.
  grid.addEventListener("scroll", () => requestAnimationFrame(() => renderRange()), { passive: true });
  // A frame later: a scroll bar coming or going changes the width again.
  new ResizeObserver(() => requestAnimationFrame(() => renderRange())).observe(grid);
  grid.addEventListener("click", (e) => {
    const cell = e.target.closest(".lib-cell");
    if (!cell) {
      if (e.target === grid) librarySelectNone();
      return;
    }
    if (cell.dataset.dir) return loadLibrary(cell.dataset.dir, []);
    select(cell.dataset.path, e.shiftKey ? "range" : e.ctrlKey || e.metaKey ? "toggle" : "only");
  });
  // Press and drag the grid up and down, as on a touch screen.
  dragToScroll(grid, "y", () => cull.view === "grid");
  // Shift-click selects photos, not text.
  grid.addEventListener("mousedown", (e) => {
    if (e.shiftKey || e.detail > 1) e.preventDefault();
  });
  // Right-click a photo: copy it, its path, find it, open it, export.
  grid.addEventListener("contextmenu", (e) => {
    const cell = e.target.closest(".lib-cell");
    if (!cell || cell.dataset.dir) return;
    const path = cell.dataset.path;
    if (!lib.selected.has(path)) select(path, "only");
    const sel = selectedPhotos();
    contextMenu(e, [
      ["Copy image", () => copyImage(path)],
      [sel.length > 1 ? `Copy ${sel.length} file paths` : "Copy file path", () => copyPath(sel.map((p) => p.path))],
      ["Show in folder", () => invoke("reveal_file", { path }).catch((err) => toast(String(err), true))],
      ["Open in the editor", () => libraryOpen(path)],
      ["Find similar photos", () => smartSearch(path)],
      null,
      [sel.length > 1 ? `Apply a preset to ${sel.length} photos…` : "Apply a preset…", () => presetsOnPhotos(sel.map((p) => p.path))],
      lib.copied ? [sel.length > 1 ? `Paste settings onto ${sel.length} photos…` : "Paste settings…", () => libraryPasteSettings()] : false,
      [sel.length > 1 ? `Export ${sel.length} photos…` : "Export…", () => libraryExport()],
    ].filter((x) => x !== false));
  });
  grid.addEventListener("dblclick", (e) => {
    const cell = e.target.closest(".lib-cell");
    if (cell?.dataset.path) libraryOpen(cell.dataset.path);
  });
  document.addEventListener("keydown", (e) => {
    if (!lib.on || document.querySelector("dialog[open]") || e.target.matches("input, select, textarea")) return;
    if (["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown"].includes(e.key) && !e.ctrlKey && !e.metaKey && !e.altKey) {
      e.preventDefault();
      e.stopPropagation();
      if (cullOn()) cullArrow(e.key);
      else moveFocus(e.key, e.shiftKey);
    }
  }, true);
  for (const b of $("lib-flags").querySelectorAll("button")) b.addEventListener("click", () => libraryChange({ flag: b.dataset.flag }));
  for (const id of ["lib-filter-rating", "lib-filter-label", "lib-filter-flag", "lib-filter-edited", "lib-filter-file", "lib-sort", "lib-reverse"]) $(id).addEventListener("change", renderLibrary);
  let searchTimer;
  $("lib-search").addEventListener("input", () => {
    clearTimeout(searchTimer);
    searchTimer = setTimeout(renderLibrary, 150);
  });
  $("lib-size").addEventListener("input", () => {
    grid.style.setProperty("--thumb", `${$("lib-size").value}px`);
    renderRange(true);
    for (const cell of lib.cells.values()) {
      if (cell.dataset.dir) continue;
      const url = cachedThumb(libPhoto(cell.dataset.path));
      if (url) cell.querySelector("img").src = url;
      else lib.queue.push(cell.dataset.path);
    }
    pumpLibraryThumbs();
  });
  $("lib-choose").addEventListener("click", chooseLibraryFolder);
  $("lib-choose-empty").addEventListener("click", chooseLibraryFolder);
  $("lib-reload").addEventListener("click", () => {
    if (lib.smart) showSmartCollection(lib.smart);
    else if (lib.collection) showCollection(lib.collection);
    else if (lib.dir) loadLibrary(lib.dir);
  });
  $("lib-import").addEventListener("click", () => importFlow());
  $("lib-lrcat").addEventListener("click", () => lrcatFlow());
  $("lib-add-folder").addEventListener("click", () => addLibraryFolder());
  $("lib-drop-folders").addEventListener("click", () => loadLibrary(lib.dir, []));
  for (const b of $("lib-rating").querySelectorAll("button[data-stars]")) {
    b.addEventListener("click", () => libraryChange({ rating: Number(b.dataset.stars) }));
  }
  $("lib-rating-clear").addEventListener("click", () => libraryChange({ rating: 0 }));
  for (const l of LABELS) {
    const b = Object.assign(document.createElement("button"), { type: "button", title: `${l} label${LABELS.indexOf(l) < 4 ? ` (${6 + LABELS.indexOf(l)})` : ""}` });
    b.dataset.label = l;
    b.style.setProperty("--swatch", LABEL_COLOURS[l]);
    b.addEventListener("click", () => labelKey(l));
    $("lib-labels").append(b);
  }
  $("lib-label-clear").addEventListener("click", () => libraryChange({ label: "" }));
  const input = $("lib-tag-input");
  input.addEventListener("keydown", (e) => {
    if (e.key === "Enter" || e.key === ",") {
      e.preventDefault();
      addLibraryTyped();
    } else if (e.key === "Escape") {
      input.value = "";
      input.blur();
    }
  });
  input.addEventListener("input", (e) => {
    if (e.inputType === "insertReplacementText" || (!e.inputType && infoState.recent.includes(input.value))) addLibraryTyped();
  });
  $("lib-open").addEventListener("click", libraryOpenFocused);
  $("lib-copy").addEventListener("click", libraryCopySettings);
  $("lib-paste").addEventListener("click", libraryPasteSettings);
  $("lib-negative-sync").addEventListener("click", () => syncNegativeRoll());
  $("lib-contact").addEventListener("click", () => contactSheetFlow());
  $("lib-convert").addEventListener("click", () => convertRawsFlow());
  tauri.event.listen("convert-progress", (e) => {
    const [done, total] = e.payload;
    status(`Converting: ${done} of ${total}…`, true);
  });
  $("lib-smart").addEventListener("click", () => smartSearch());
  $("lib-smart-chip").addEventListener("click", clearSmartSearch);
  $("lib-search").addEventListener("keydown", (e) => {
    // Typing filters by name and keywords as you go; Enter searches what is in the photos.
    if (e.key === "Enter") smartSearch();
  });
  tauri.event.listen("smart-progress", (e) => {
    const [done, total] = e.payload;
    status(`Indexing photos for smart search (once): ${done} of ${total}…`, true);
  });
  $("lib-plugin").addEventListener("click", () => pluginsFlow().catch((e) => toast(String(e), true)));
  $("lib-vcopy").addEventListener("click", virtualCopyKey);
  $("lib-vcopy-delete").addEventListener("click", libraryDeleteCopies);
  $("lib-export").addEventListener("click", libraryExport);
  $("lib-collage").addEventListener("click", libraryCollage);
  $("lib-hdr").addEventListener("click", libraryMergeHdr);
  $("lib-pano").addEventListener("click", libraryMergePano);
  grid.style.setProperty("--thumb", `${$("lib-size").value}px`);
  wireCull();
  wireCollections();
}
