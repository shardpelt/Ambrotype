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
  expanded: new Set(), // stack ids shown open
  stackSizes: new Map(), // stack id -> photos in it
  copied: null,        // the photo "Copy settings" took its edit from
  groups: null,        // setting groups for Paste settings
  wired: false,
};

const libraryOn = () => lib.on;
const libPhoto = (path) => lib.photos.find((p) => p.path === path);
const selectedPhotos = () => lib.shown.filter((p) => lib.selected.has(p.path));
const VIEWER_BUTTONS = ["btn-before", "btn-crop", "btn-masks", "btn-spots", "btn-zoom", "btn-wb", "btn-clip"];

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
  $("workspace").hidden = on;
  $("library").hidden = !on;
  document.body.classList.toggle("library-mode", on);
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
      lib.cells.get(ui.info.path)?.scrollIntoView({ block: "center" });
    }
    $("library-grid").focus({ preventScroll: true });
  } else {
    requestPreview(false);
    if (ui.info) loadInfo();
  }
}

// Show a folder; `more` folders are shown with it (kept on a reload of the
// same folder unless given).
async function loadLibrary(dir, more = dir === lib.dir ? lib.more : []) {
  lib.dir = dir;
  lib.more = more.filter((d) => d !== dir);
  lib.collection = null;
  lib.smart = null;
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
  if (typeof renderCollectionControls === "function") renderCollectionControls();
}

async function chooseLibraryFolder() {
  const dir = await invoke("library_pick_folder");
  if (!dir) return;
  lib.selected.clear();
  await loadLibrary(dir, []);
}

// Show another folder alongside the ones shown.
async function addLibraryFolder() {
  const dir = await invoke("library_pick_folder");
  if (!dir) return;
  if (!lib.dir || lib.collection) return loadLibrary(dir, []);
  if (dir !== lib.dir && !lib.more.includes(dir)) await loadLibrary(lib.dir, [...lib.more, dir]);
}

// ---------------------------------------------------------------- filter, sort, grid

function applyView() {
  const f = {
    rating: $("lib-filter-rating").value,
    label: $("lib-filter-label").value,
    flag: $("lib-filter-flag").value,
    edited: $("lib-filter-edited").value,
    text: $("lib-search").value.trim().toLowerCase(),
  };
  let shown = lib.photos.filter((p) => {
    if (f.rating === "unrated" ? p.rating : (p.rating ?? 0) < Number(f.rating)) return false;
    if (f.label === "none" ? p.label : f.label !== "all" && p.label !== f.label) return false;
    if (f.flag === "unflagged" ? p.flag : f.flag === "kept" ? p.flag === "reject" : f.flag !== "all" && p.flag !== f.flag) return false;
    if (f.edited === "edited" && !p.edited) return false;
    if (f.edited === "unedited" && p.edited) return false;
    if (f.text && !(p.name.toLowerCase().includes(f.text) || p.keywords.some((k) => k.toLowerCase().includes(f.text)))) return false;
    return true;
  });
  const sort = $("lib-sort").value;
  const byName = (a, b) => lib.photos.indexOf(a) - lib.photos.indexOf(b);
  if (sort === "captured") shown.sort((a, b) => (a.captured ?? "~").localeCompare(b.captured ?? "~") || byName(a, b));
  else if (sort === "rating") shown.sort((a, b) => (b.rating ?? 0) - (a.rating ?? 0) || byName(a, b));
  else if (sort === "edited") shown.sort((a, b) => Number(b.edited) - Number(a.edited) || byName(a, b));
  if ($("lib-reverse").checked) shown.reverse();
  // A closed stack shows only its top photo (or its first one shown).
  lib.stackSizes = new Map();
  for (const p of lib.photos) if (p.stack) lib.stackSizes.set(p.stack.id, (lib.stackSizes.get(p.stack.id) ?? 0) + 1);
  const shownTop = new Map();
  for (const p of shown) {
    if (!p.stack || lib.expanded.has(p.stack.id)) continue;
    const have = shownTop.get(p.stack.id);
    if (!have || (p.stack.top && !have.stack.top)) shownTop.set(p.stack.id, p);
  }
  lib.shown = shown.filter((p) => !p.stack || lib.expanded.has(p.stack.id) || shownTop.get(p.stack.id) === p);
}

// Open or close a stack in the grid.
function toggleStack(id) {
  if (lib.expanded.has(id)) lib.expanded.delete(id);
  else lib.expanded.add(id);
  renderLibrary();
}

// Selected photos into one stack, the first (or the focused) on top.
async function stackSelection() {
  const sel = libraryTargets();
  if (sel.length < 2) return toast("Select two or more photos to stack.", true);
  const top = sel.find((p) => p.path === lib.focus) ?? sel[0];
  const id = `s${Date.now().toString(36)}`;
  await setStacks([[top.path], sel.filter((p) => p !== top).map((p) => p.path)], id);
  lib.expanded.delete(id);
  status(`Stacked ${sel.length} photos`);
  renderLibrary();
}

// Take the selected photos out of their stacks (all of a closed stack).
async function unstackSelection() {
  const ids = new Set(libraryTargets().map((p) => p.stack?.id).filter(Boolean));
  if (!ids.size) return toast("None of the selected photos is in a stack.", true);
  const paths = lib.photos.filter((p) => p.stack && ids.has(p.stack.id)).map((p) => p.path);
  await setStacks([[], paths], "");
  status(`Unstacked ${paths.length} photos`);
  renderLibrary();
}

// Write stacks: `[tops, rest]` path lists into stack `id` ("" for none).
async function setStacks([tops, rest], id) {
  for (const [paths, top] of [[tops, true], [rest, false]]) {
    if (!paths.length) continue;
    try {
      const updates = await invoke("library_set_info", { paths, change: { stack: { id, top: id ? top : false } } });
      for (const u of updates) {
        const p = libPhoto(u.path);
        if (p) p.stack = u.info.stack ?? null;
      }
    } catch (e) {
      toast(String(e), true);
    }
  }
}

// Stack the shown photos taken within `seconds` of the one before, by capture time.
async function autoStack() {
  const answer = await askName("Auto-stack by capture time", "Photos taken within this many seconds of the one before go into one stack (bursts, brackets). Photos already in a stack are left alone.", "5");
  const gap = Number(answer);
  if (answer === null || !(gap > 0)) return;
  const time = (p) => (p.captured ? Date.parse(p.captured.replace(" ", "T")) : NaN);
  const photos = lib.photos.filter((p) => !p.stack && !Number.isNaN(time(p))).sort((a, b) => time(a) - time(b));
  const groups = [];
  for (const p of photos) {
    const g = groups[groups.length - 1];
    if (g && time(p) - time(g[g.length - 1]) <= gap * 1000) g.push(p);
    else groups.push([p]);
  }
  const stacks = groups.filter((g) => g.length > 1);
  for (const [i, g] of stacks.entries()) {
    await setStacks([[g[0].path], g.slice(1).map((p) => p.path)], `s${Date.now().toString(36)}${i}`);
  }
  status(stacks.length ? `Made ${stacks.length} stack${stacks.length === 1 ? "" : "s"} of ${stacks.reduce((n, g) => n + g.length, 0)} photos` : "No photos close enough in time to stack");
  renderLibrary();
}

function renderLibrary() {
  applyView();
  const grid = $("library-grid");
  lib.observer?.disconnect();
  lib.queue = [];
  lib.editedQueue = [];
  lib.cells.clear();
  grid.replaceChildren();
  $("library-empty").hidden = Boolean(lib.dir || lib.collection);
  // What just scrolled into view loads first: after a fast scroll the photos
  // passed on the way wait behind the ones on screen.
  lib.observer = new IntersectionObserver((entries) => {
    const seen = [];
    for (const e of entries) {
      if (!e.isIntersecting) continue;
      lib.observer.unobserve(e.target);
      seen.push(e.target.dataset.path);
    }
    lib.queue = seen.concat(lib.queue);
    pumpLibraryThumbs();
  }, { root: grid, rootMargin: "300px 0px" });
  for (const p of lib.shown) {
    const cell = document.createElement("div");
    cell.className = "lib-cell";
    cell.dataset.path = p.path;
    cell.title = [p.name, p.captured, p.keywords.join(", ")].filter(Boolean).join("\n");
    const frame = Object.assign(document.createElement("div"), { className: "lib-frame" });
    frame.append(Object.assign(document.createElement("img"), { alt: "", draggable: false }));
    const meta = Object.assign(document.createElement("div"), { className: "lib-meta" });
    meta.append(
      Object.assign(document.createElement("span"), { className: "lib-flag" }),
      Object.assign(document.createElement("span"), { className: "lib-stars" }),
      Object.assign(document.createElement("span"), { className: "lib-edited", title: "Has edits" }),
    );
    const name = Object.assign(document.createElement("div"), { className: "lib-name", textContent: p.name });
    cell.append(frame, meta, name);
    if (p.stack) {
      const open = lib.expanded.has(p.stack.id);
      const badge = Object.assign(document.createElement("button"), {
        type: "button",
        className: `lib-stack${open ? " open" : ""}${p.stack.top ? " top" : ""}`,
        textContent: `⧉ ${lib.stackSizes.get(p.stack.id) ?? 1}`,
        title: open ? "Close this stack" : "Open this stack",
      });
      badge.addEventListener("click", (e) => {
        e.stopPropagation();
        toggleStack(p.stack.id);
      });
      frame.append(badge);
    }
    lib.cells.set(p.path, cell);
    updateCell(p);
    const url = cachedThumb(p);
    if (url) cell.querySelector("img").src = url;
    lib.observer.observe(cell);
    grid.append(cell);
  }
  renderCounts();
  renderSelection();
}

function updateCell(p) {
  const cell = lib.cells.get(p.path);
  if (!cell) return;
  cell.querySelector(".lib-stars").textContent = p.rating ? "★".repeat(p.rating) : "";
  cell.querySelector(".lib-edited").hidden = !p.edited;
  showFlag(cell.querySelector(".lib-flag"), p.flag);
  cell.classList.toggle("rejected", p.flag === "reject");
  cell.style.setProperty("--label", p.label ? LABEL_COLOURS[p.label] ?? "var(--muted)" : "transparent");
  cell.classList.toggle("labelled", Boolean(p.label));
  cell.classList.toggle("open", p.path === ui.info?.path);
}

function renderCounts() {
  const n = lib.photos.length;
  const shown = lib.shown.length;
  const sel = selectedPhotos().length;
  $("library-count").textContent = !lib.dir ? "" : [
    `${n} photo${n === 1 ? "" : "s"}`,
    shown !== n ? `${shown} shown` : "",
    sel ? `${sel} selected` : "",
  ].filter(Boolean).join(" · ");
}

// ---------------------------------------------------------------- thumbnails

function thumbEdge() {
  return Number($("lib-size").value) * (window.devicePixelRatio || 1) > 300 ? 512 : 256;
}

function cachedThumb(p) {
  const edge = thumbEdge();
  return lib.urls.get(`${p.path}|${edge}|true`) ?? lib.urls.get(`${p.path}|${edge}|false`);
}

// Camera previews first (quick), then renders of edited photos one at a time.
function pumpLibraryThumbs() {
  const edge = thumbEdge();
  while (lib.loading < 4 && lib.queue.length) {
    const path = lib.queue.shift();
    const p = libPhoto(path);
    const cell = lib.cells.get(path);
    if (!p || !cell) continue;
    if (p.edited && lib.urls.has(`${path}|${edge}|true`)) continue;
    const key = `${path}|${edge}|false`;
    if (lib.urls.has(key)) {
      if (p.edited) lib.editedQueue.push(path);
      continue;
    }
    lib.loading++;
    invoke("thumbnail", { path, edge })
      .then((buf) => {
        const url = URL.createObjectURL(new Blob([new Uint8Array(buf)], { type: "image/jpeg" }));
        lib.urls.set(key, url);
        if (!lib.urls.has(`${path}|${edge}|true`)) cell.querySelector("img").src = url;
        if (p.edited) lib.editedQueue.push(path);
      })
      .catch(() => cell.classList.add("no-thumb"))
      .finally(() => {
        lib.loading--;
        pumpLibraryThumbs();
      });
  }
  if (!lib.queue.length && lib.loading === 0 && lib.editedQueue.length) {
    const path = lib.editedQueue.shift();
    const key = `${path}|${edge}|true`;
    const cell = lib.cells.get(path);
    if (!cell || lib.urls.has(key)) return pumpLibraryThumbs();
    lib.loading++;
    // The open photo's current edit lives in its session.
    const call = path === ui.info?.path
      ? invoke("edited_thumbnail", { path, edge })
      : invoke("thumbnail", { path, edge, edited: true });
    call
      .then((buf) => {
        const url = URL.createObjectURL(new Blob([new Uint8Array(buf)], { type: "image/jpeg" }));
        lib.urls.set(key, url);
        cell.querySelector("img").src = url;
      })
      .catch(() => {})
      .finally(() => {
        lib.loading--;
        pumpLibraryThumbs();
      });
  }
}

// A photo's edit changed: show the new render when it comes.
function refreshThumb(path) {
  for (const key of [...lib.urls.keys()]) if (key.startsWith(`${path}|`) && key.endsWith("|true")) lib.urls.delete(key);
  lib.editedQueue.push(path);
  pumpLibraryThumbs();
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
  const grid = $("library-grid");
  const first = grid.firstElementChild;
  const cols = first ? Math.max(1, Math.round(grid.clientWidth / (first.offsetWidth + 10))) : 1;
  const at = Math.max(0, order.indexOf(lib.focus));
  const step = { ArrowLeft: -1, ArrowRight: 1, ArrowUp: -cols, ArrowDown: cols }[key];
  const next = order[Math.min(order.length - 1, Math.max(0, at + step))];
  select(next, extend ? "range" : "only");
  lib.cells.get(next)?.scrollIntoView({ block: "nearest" });
}

// ---------------------------------------------------------------- the panel: rate, label, keywords

function renderPanel() {
  // In compare and survey the panel is about the active photo.
  const sel = libraryTargets();
  const n = sel.length;
  const active = cullOn() && cull.view !== "loupe";
  $("lib-sel-count").textContent = active ? (sel[0] ? `${sel[0].name} (active)` : "No active photo")
    : n ? `${n} photo${n === 1 ? "" : "s"} selected` : "Nothing selected";
  $("lib-hint").textContent = active
    ? "Changes go to the active photo (the framed one). Click a photo to make it active; G or Esc goes back to the grid."
    : "Click to select, Shift for a range, Ctrl (⌘) for more. Changes go to every selected photo's .xmp at once.";
  for (const el of $("library-panel").querySelectorAll("[data-needs]")) {
    const need = el.dataset.needs;
    el.disabled = need === "one" ? n !== 1 : need === "many" ? n < 2
      : need === "paste" ? !lib.copied || !sel.some((p) => p.path !== lib.copied) : n === 0;
  }
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
    const updates = await invoke("library_set_info", { paths, change });
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
  if (lib.on) return libraryChange({ rating: n });
  return ui.info ? setRating(n) : false;
}

function labelKey(label) {
  if (lib.on) {
    const sel = libraryTargets();
    if (!sel.length) return false;
    return libraryChange({ label: sel.every((p) => p.label === label) ? "" : label });
  }
  return ui.info ? setLabel(label) : false;
}

function flagKey(flag) {
  if (lib.on) return libraryTargets().length ? libraryChange({ flag }) : false;
  return ui.info ? setFlag(flag) : false;
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

// ---------------------------------------------------------------- open, copy and paste, export, collage

async function libraryOpen(path) {
  if (!path) return;
  await toggleLibrary(false);
  if (path !== ui.info?.path) await openPath(path);
}

function libraryOpenFocused() {
  return libraryOpen((cullOn() && cull.active) || lib.focus || selectedPhotos()[0]?.path);
}

function libraryCopySettings() {
  const sel = selectedPhotos();
  if (sel.length !== 1) return toast("Select the one photo to copy the settings from.", true);
  lib.copied = sel[0].path;
  toast(`Copied the settings of ${sel[0].name}`);
  renderPanel();
}

// A virtual copy of one photo: a second edit, kept as its own sidecar.
async function makeVirtualCopy(path) {
  try {
    const copy = await invoke("create_virtual_copy", { path });
    status(`Made a virtual copy of ${fileName(path)}`);
    return copy;
  } catch (e) {
    toast(String(e), true);
    return null;
  }
}

// Ctrl+': in the library, of the selected photo; in the editor, of the open
// photo, which then opens.
async function virtualCopyKey() {
  if (lib.on) {
    const sel = libraryTargets();
    if (sel.length !== 1) return toast("Select the one photo to make a virtual copy of.", true);
    const copy = await makeVirtualCopy(sel[0].path);
    if (!copy) return;
    if (lib.collection) await addToCollection([copy], lib.collection);
    else await loadLibrary(lib.dir);
    select(copy, "only");
    lib.cells.get(copy)?.scrollIntoView({ block: "nearest" });
    return;
  }
  if (!ui.info) return false;
  const copy = await makeVirtualCopy(ui.info.path);
  if (copy) await openPath(copy);
}

async function libraryDeleteCopies() {
  const copies = libraryTargets().filter((p) => p.copy);
  if (!copies.length) return;
  const what = copies.length === 1 ? copies[0].name : `${copies.length} virtual copies`;
  if (!confirm(`Delete ${what}? Only the copy's edit goes; the photo stays.`)) return;
  // The editor can't keep a deleted copy open: switch to its photo first.
  const open = copies.find((p) => p.path === ui.info?.path);
  if (open) {
    const master = lib.photos.find((p) => !p.copy && open.name.startsWith(`${p.name} ·`));
    if (master) await openPath(master.path);
  }
  let failed = 0;
  for (const p of copies) {
    try {
      await invoke("delete_virtual_copy", { path: p.path });
    } catch (e) {
      failed++;
      toast(String(e), true);
    }
  }
  status(`Deleted ${copies.length - failed} virtual cop${copies.length - failed === 1 ? "y" : "ies"}`);
  lib.selected.clear();
  if (lib.smart) await showSmartCollection(lib.smart);
  else if (lib.collection) await showCollection(lib.collection);
  else await loadLibrary(lib.dir);
}

// One conversion for a roll of negatives: the median of each frame's analysis.
async function syncNegativeRoll() {
  const paths = selectedPhotos().map((p) => p.path);
  if (paths.length < 2) return toast("Select the frames of the roll.", true);
  status(`Analysing ${paths.length} negatives…`);
  $("busy").hidden = false;
  try {
    const r = await invoke("negative_sync", { paths });
    for (const path of r.changed) {
      const p = libPhoto(path);
      if (p) {
        p.edited = true;
        updateCell(p);
        stripUpdate(path, { edited: true });
      }
      refreshThumb(path);
    }
    if (r.state) {
      setState(r.state);
      requestPreview(false);
    }
    toast(`Synced ${r.changed.length} negative${r.changed.length === 1 ? "" : "s"}${r.errors.length ? `; ${r.errors.length} failed: ${r.errors[0]}` : ""}`, r.errors.length > 0);
    status(`One conversion for ${r.changed.length} frames`);
  } catch (e) {
    toast(String(e), true);
  } finally {
    $("busy").hidden = true;
  }
}

async function libraryPasteSettings() {
  if (!lib.copied) return toast("First copy the settings of one photo (Ctrl+Shift+C).", true);
  const targets = selectedPhotos().map((p) => p.path).filter((p) => p !== lib.copied);
  if (!targets.length) return toast("Select the photos to paste onto.", true);
  if (!lib.groups) lib.groups = await invoke("paste_groups");
  const dlg = $("paste-dialog");
  $("paste-what").textContent = `From ${fileName(lib.copied)} to ${targets.length} photo${targets.length === 1 ? "" : "s"}. Ticked groups are made the same as the source (settings it doesn't have go back to their defaults); the rest stay as they are.`;
  const list = $("paste-groups");
  list.replaceChildren(...lib.groups.map((g) => {
    const row = Object.assign(document.createElement("label"), { className: "inline-check" });
    const box = Object.assign(document.createElement("input"), { type: "checkbox", value: g.name, checked: lib.pasteChoice?.has(g.name) ?? g.default });
    row.append(box, Object.assign(document.createElement("span"), { textContent: g.label }));
    return row;
  }));
  dlg.showModal();
  const choice = await new Promise((resolve) => dlg.addEventListener("close", () => resolve(dlg.returnValue), { once: true }));
  if (choice !== "ok") return;
  const groups = [...list.querySelectorAll("input:checked")].map((b) => b.value);
  lib.pasteChoice = new Set(groups);
  if (!groups.length) return;
  status(`Pasting settings onto ${targets.length} photos…`);
  try {
    const r = await invoke("paste_settings", { source: lib.copied, targets, groups });
    for (const path of targets) {
      const p = libPhoto(path);
      if (p) {
        p.edited = true;
        updateCell(p);
        stripUpdate(path, { edited: true });
      }
      refreshThumb(path);
    }
    if (r.state) {
      setState(r.state);
      requestPreview(false);
    }
    toast(`Pasted onto ${r.pasted} photo${r.pasted === 1 ? "" : "s"}${r.errors.length ? `; ${r.errors.length} failed: ${r.errors[0]}` : ""}`, r.errors.length > 0);
    status(`Pasted settings onto ${r.pasted} photos`);
  } catch (e) {
    toast(String(e), true);
  }
}

// The export dialog, for many photos: same choices, one file each.
function libraryExport() {
  const sel = selectedPhotos();
  if (!sel.length) return toast("Select the photos to export.", true);
  const img = lib.cells.get(sel[0].path)?.querySelector("img");
  const aspect = img?.naturalWidth ? img.naturalWidth / img.naturalHeight : 1.5;
  return exportFlow({
    paths: sel.map((p) => p.path),
    aspect,
    run: async (options) => {
      const off = await tauri.event.listen("library-progress", (e) => {
        const p = e.payload;
        status(p.done < p.total ? `Exporting ${p.done + 1} of ${p.total}: ${p.name}` : "Exported");
      });
      $("busy").hidden = false;
      try {
        const r = await invoke("export_photos", { paths: sel.map((p) => p.path), options });
        if (!r) return;
        const failed = r.errors.length ? `; ${r.errors.length} failed: ${r.errors[0]}` : "";
        toast(`Exported ${r.written.length} photo${r.written.length === 1 ? "" : "s"}${failed}`, r.errors.length > 0);
        if (r.written.length) status(`Exported ${r.written.length} photos to ${dirOf(r.written[0])}`);
      } finally {
        off();
        $("busy").hidden = true;
      }
    },
  });
}

// Photo Merge > HDR: the selected brackets into one DNG, which then opens.
async function libraryMergeHdr() {
  const sel = selectedPhotos().filter((p) => !p.copy);
  if (sel.length < 2) return toast("Select two or more raws of one scene, shot at different exposures.", true);
  const dlg = $("hdr-dialog");
  $("hdr-what").textContent = `${sel.length} photos: ${sel.map((p) => p.name).join(", ")}`;
  dlg.showModal();
  const choice = await new Promise((resolve) => dlg.addEventListener("close", () => resolve(dlg.returnValue), { once: true }));
  if (choice !== "ok") return;
  status(`Merging ${sel.length} photos to HDR…`);
  $("busy").hidden = false;
  try {
    const r = await invoke("merge_hdr", {
      paths: sel.map((p) => p.path),
      align: $("hdr-align").checked,
      deghost: $("hdr-deghost").value,
      autoTone: $("hdr-tone").checked,
    });
    const stops = r.stops.map((s) => `${s >= 0 ? "+" : ""}${s.toFixed(1)}`).join(", ");
    status(`Merged into ${fileName(r.output)} (exposures ${stops} EV)`);
    if (lib.collection) await addToCollection([r.output], lib.collection);
    else await loadLibrary(lib.dir);
    await libraryOpen(r.output);
  } catch (e) {
    toast(String(e), true);
    status("HDR merge failed");
  } finally {
    $("busy").hidden = true;
  }
}

// Photo Merge > Panorama: the selected frames into one DNG, which then opens.
async function libraryMergePano() {
  const sel = selectedPhotos().filter((p) => !p.copy);
  if (sel.length < 2) return toast("Select two or more overlapping raws.", true);
  const dlg = $("pano-dialog");
  $("pano-what").textContent = `${sel.length} photos: ${sel.map((p) => p.name).join(", ")}`;
  dlg.showModal();
  const choice = await new Promise((resolve) => dlg.addEventListener("close", () => resolve(dlg.returnValue), { once: true }));
  if (choice !== "ok") return;
  status(`Stitching ${sel.length} photos into a panorama…`);
  $("busy").hidden = false;
  try {
    const r = await invoke("merge_pano", {
      paths: sel.map((p) => p.path),
      projection: $("pano-projection").value,
      autoCrop: $("pano-crop").checked,
      lensCorrections: $("pano-lens").checked,
      autoTone: $("pano-tone").checked,
    });
    const corrected = r.lens_profiles.filter(Boolean).length;
    const lens = $("pano-lens").checked && corrected < r.lens_profiles.length
      ? `; no lens profile for ${r.lens_profiles.length - corrected} of them`
      : "";
    status(`Stitched into ${fileName(r.output)} (${r.projection}, ${Math.round(r.field[0])}° across${lens})`);
    if (lib.collection) await addToCollection([r.output], lib.collection);
    else await loadLibrary(lib.dir);
    await libraryOpen(r.output);
  } catch (e) {
    toast(String(e), true);
    status("Panorama merge failed");
  } finally {
    $("busy").hidden = true;
  }
}

async function libraryCollage() {
  const sel = selectedPhotos();
  if (!sel.length) return;
  await collageFlow();
  setTray(sel.map((p) => ({ ...p })));
  fillEmpty();
}

// ---------------------------------------------------------------- wiring

function wireLibrary() {
  if (lib.wired) return;
  lib.wired = true;
  // Title, caption, creator and copyright for the selection, when a field is left.
  $("lib-stack").addEventListener("click", stackSelection);
  $("lib-unstack").addEventListener("click", unstackSelection);
  $("lib-autostack").addEventListener("click", autoStack);
  for (const f of $("lib-meta").querySelectorAll("[data-meta]")) {
    f.addEventListener("change", () => libraryChange({ [f.dataset.meta]: f.value.trim() }));
    f.addEventListener("keydown", (e) => e.stopPropagation());
  }
  const grid = $("library-grid");
  grid.addEventListener("click", (e) => {
    const cell = e.target.closest(".lib-cell");
    if (!cell) {
      if (e.target === grid) librarySelectNone();
      return;
    }
    select(cell.dataset.path, e.shiftKey ? "range" : e.ctrlKey || e.metaKey ? "toggle" : "only");
  });
  // Shift-click selects photos, not text.
  grid.addEventListener("mousedown", (e) => {
    if (e.shiftKey || e.detail > 1) e.preventDefault();
  });
  grid.addEventListener("dblclick", (e) => {
    const cell = e.target.closest(".lib-cell");
    if (cell) libraryOpen(cell.dataset.path);
  });
  document.addEventListener("keydown", (e) => {
    if (!lib.on || document.querySelector("dialog[open]") || e.target.matches("input, select, textarea")) return;
    if (["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown"].includes(e.key) && !e.ctrlKey && !e.metaKey) {
      e.preventDefault();
      e.stopPropagation();
      if (cullOn()) cullArrow(e.key);
      else moveFocus(e.key, e.shiftKey);
    }
  }, true);
  for (const b of $("lib-flags").querySelectorAll("button")) b.addEventListener("click", () => libraryChange({ flag: b.dataset.flag }));
  for (const id of ["lib-filter-rating", "lib-filter-label", "lib-filter-flag", "lib-filter-edited", "lib-sort", "lib-reverse"]) $(id).addEventListener("change", renderLibrary);
  let searchTimer;
  $("lib-search").addEventListener("input", () => {
    clearTimeout(searchTimer);
    searchTimer = setTimeout(renderLibrary, 150);
  });
  $("lib-size").addEventListener("input", () => {
    grid.style.setProperty("--thumb", `${$("lib-size").value}px`);
    for (const cell of lib.cells.values()) {
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
  $("lib-add-folder").addEventListener("click", () => addLibraryFolder());
  $("lib-drop-folders").addEventListener("click", () => loadLibrary(lib.dir, []));
  for (const b of $("lib-rating").querySelectorAll("button[data-stars]")) {
    b.addEventListener("click", () => libraryChange({ rating: Number(b.dataset.stars) }));
  }
  $("lib-rating-clear").addEventListener("click", () => libraryChange({ rating: 0 }));
  for (const l of LABELS) {
    const b = Object.assign(document.createElement("button"), { type: "button", title: `${l} label${LABELS.indexOf(l) < 4 ? ` (${6 + LABELS.indexOf(l)})` : ""}` });
    b.dataset.label = l;
    b.style.background = LABEL_COLOURS[l];
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
