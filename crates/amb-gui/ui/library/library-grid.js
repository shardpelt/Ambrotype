// SPDX-License-Identifier: GPL-3.0-or-later
// The library's grid: filtering and sorting the photos, drawing the tiles,
// and loading their thumbnails. Split out of library.js.
"use strict";

// ---------------------------------------------------------------- filter, sort, grid

// Image files a camera writes besides its raws (a DNG counts as a raw).
const RASTER_FILE = /\.(jpe?g|png|tiff?|webp|heic|heif|avif|jxl|gif|bmp)$/i;

// Raws and their JPEG twins are one photo in the lists, shown as the raw.
// JPEG only shows the twins instead, and "JPEG if there is one" shows each
// raw's twin in its place; the twins are asked for once per listing. A
// virtual copy (IMG.copy-2.xmp, photo.jpg.copy-2.xmp) goes with its photo.
function photosOfFileKind(kind, preferJpeg = false) {
  const raster = (p) => RASTER_FILE.test(p.path.replace(/\.copy-\d+\.xmp$/i, ""));
  if (kind === "raw") return lib.photos.filter((p) => !raster(p));
  if (kind !== "jpeg" && !preferJpeg) return lib.photos;
  if (lib.twins?.of !== lib.photos) {
    const of = lib.photos;
    lib.twins = { of, photos: [], byRaw: new Map() };
    const raws = of.filter((p) => p.copy == null && !raster(p)).map((p) => p.path);
    if (raws.length) {
      invoke("library_twin_jpegs", { paths: raws }).then((photos) => {
        if (lib.twins?.of !== of) return;
        // Each twin by its raw: the same folder and name.
        const stem = (path) => path.replace(/\.[^./\\]+$/, "").toLowerCase();
        const twinOf = new Map(photos.map((t) => [stem(t.path), t]));
        lib.twins.photos = photos;
        for (const r of raws) if (twinOf.has(stem(r))) lib.twins.byRaw.set(r, twinOf.get(stem(r)));
        renderLibrary();
      }).catch((e) => console.warn("JPEG twins:", e));
    }
  }
  if (kind === "jpeg") return [...lib.photos.filter(raster), ...lib.twins.photos];
  return lib.photos.map((p) => lib.twins.byRaw.get(p.path) ?? p);
}

function applyView() {
  const f = {
    rating: $("lib-filter-rating").value,
    label: $("lib-filter-label").value,
    flag: $("lib-filter-flag").value,
    edited: $("lib-filter-edited").value,
    text: $("lib-search").value.trim().toLowerCase(),
  };
  let shown = photosOfFileKind($("lib-filter-file").value, $("lib-prefer-jpeg").checked).filter((p) => {
    if (f.rating === "unrated" ? p.rating : (p.rating ?? 0) < Number(f.rating)) return false;
    if (f.label === "none" ? p.label : f.label !== "all" && p.label !== f.label) return false;
    if (f.flag === "unflagged" ? p.flag : f.flag === "kept" ? p.flag === "reject" : f.flag !== "all" && p.flag !== f.flag) return false;
    if (f.edited === "edited" && !p.edited) return false;
    if (f.edited === "unedited" && p.edited) return false;
    if (f.text && !lib.found && ![p.name, ...p.keywords, p.title, p.caption, p.camera, p.lens].some((t) => t?.toLowerCase().includes(f.text))) return false;
    return true;
  });
  // A smart search shows its finds, best first.
  // ...for the photos it searched: another folder or collection drops it.
  if (lib.found && lib.found.of !== lib.photos) {
    lib.found = null;
    $("lib-smart-chip").hidden = true;
  }
  if (lib.found) {
    const rank = lib.found.rank;
    shown = shown.filter((p) => rank.has(p.path)).sort((a, b) => rank.get(a.path) - rank.get(b.path));
  }
  lib.beforeDate = shown;
  if (lib.date) shown = shown.filter((p) => p.captured?.startsWith(lib.date));
  const sort = $("lib-sort").value;
  // In the listing's order; a JPEG twin (not in it) by its name.
  const at = new Map(lib.photos.map((p, i) => [p, i]));
  const twins = $("lib-filter-file").value === "jpeg" || $("lib-prefer-jpeg").checked;
  const byName = twins
    ? (a, b) => a.name.localeCompare(b.name, undefined, { numeric: true })
    : (a, b) => at.get(a) - at.get(b);
  if (lib.found) { /* best first */ } else if (sort === "captured") shown.sort((a, b) => (a.captured ?? "~").localeCompare(b.captured ?? "~") || byName(a, b));
  else if (sort === "rating") shown.sort((a, b) => (b.rating ?? 0) - (a.rating ?? 0) || byName(a, b));
  else if (sort === "edited") shown.sort((a, b) => Number(b.edited) - Number(a.edited) || byName(a, b));
  else if (twins) shown.sort(byName);
  if ($("lib-reverse").checked && !lib.found) shown.reverse();
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
      const updates = await setInfo(paths, { stack: { id, top: id ? top : false } });
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

// The grid draws only the rows on screen and a few around them, between two
// spacers that stand in for the rest, so a folder of 100 000 photos scrolls
// like one of 100. Every cell is a square frame plus a caption, so a row's
// place follows from the column count.
const GRID_GAP = 10;
const GRID_PAD = 12;
const GRID_OVERSCAN = 4;

// The folders inside the one shown come first in the grid, as tiles (the
// folder above it as ".."); a click opens one.
async function renderSubfolders() {
  const dir = lib.dir;
  lib.dirs = [];
  renderRange(true);
  // Only for one folder on its own (not a collection, All photos or a search).
  if (!dir || lib.collection || lib.smart || lib.more.length) return;
  let subs = [];
  try {
    subs = await invoke("library_subfolders", { dir });
  } catch { /* none shown */ }
  if (lib.dir !== dir || lib.collection || lib.smart) return;
  const sep = dir.includes("\\") ? "\\" : "/";
  const trimmed = dir.replace(/[\\/]+$/, "");
  const parent = trimmed.includes(sep) ? trimmed.slice(0, trimmed.lastIndexOf(sep)) || sep : null;
  lib.dirs = [...(parent ? [{ path: parent, name: "..", up: true }] : []), ...subs];
  renderRange(true);
}

function makeDirCell(d) {
  const cell = Object.assign(document.createElement("div"), { className: "lib-cell lib-dir" });
  cell.dataset.dir = d.path;
  cell.title = d.up ? `Up to ${d.path}` : `${d.path}\n${d.photos} photo${d.photos === 1 ? "" : "s"}${d.folders ? ", and folders" : ""}: click to open`;
  const frame = Object.assign(document.createElement("div"), { className: "lib-frame" });
  frame.append(Object.assign(document.createElement("span"), { className: "lib-dir-icon", textContent: d.up ? "↑" : "📁" }));
  const meta = Object.assign(document.createElement("div"), { className: "lib-meta" });
  if (!d.up) meta.append(Object.assign(document.createElement("span"), { className: "lib-dir-count", textContent: `${d.photos} photo${d.photos === 1 ? "" : "s"}${d.folders ? " +" : ""}` }));
  cell.append(frame, meta, Object.assign(document.createElement("div"), { className: "lib-name", textContent: d.up ? ".. (up)" : d.name }));
  return cell;
}

function renderLibrary() {
  applyView();
  if (lib.collection || lib.smart) lib.dirs = [];
  const grid = $("library-grid");
  lib.observer?.disconnect();
  lib.queue = [];
  lib.editedQueue = [];
  lib.cells.clear();
  lib.rangeKey = null;
  $("library-empty").hidden = Boolean(lib.dir || lib.collection);
  // Timeline, Map or a cull view has nothing to show over it either.
  if (!$("library-empty").hidden) $("library-view").hidden = true;
  else $("library-view").hidden = cull.view === "grid";
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
  lib.spacers = [0, 1].map(() => Object.assign(document.createElement("div"), { className: "lib-spacer" }));
  grid.replaceChildren(...lib.spacers);
  renderRange(true);
  // The caption's height, measured once, places every row.
  const cell = lib.cells.values().next().value;
  if (cell && !lib.cellExtra) {
    lib.cellExtra = cell.offsetHeight - cell.querySelector(".lib-frame").offsetHeight;
    renderRange(true);
  }
  renderCounts();
  renderSelection();
  renderScrubber();
  if (browseOn()) renderBrowse();
}

function gridMetrics() {
  const grid = $("library-grid");
  const thumb = Number($("lib-size").value);
  const width = Math.max(thumb, grid.clientWidth - 2 * GRID_PAD);
  const cols = Math.max(1, Math.floor((width + GRID_GAP) / (thumb + GRID_GAP)));
  const cellW = (width - (cols - 1) * GRID_GAP) / cols;
  return { cols, rowH: cellW + (lib.cellExtra ?? 44) + GRID_GAP };
}

// Draw the rows in view; cells already drawn are kept, with their pictures.
function renderRange(force = false) {
  const grid = $("library-grid");
  if (!lib.spacers) return;
  const { cols, rowH } = gridMetrics();
  // Folder tiles first, then the photos.
  const dirs = lib.dirs ?? [];
  const rows = Math.ceil((dirs.length + lib.shown.length) / cols);
  const first = Math.max(0, Math.floor((grid.scrollTop - GRID_PAD) / rowH) - GRID_OVERSCAN);
  const last = Math.min(rows, Math.ceil((grid.scrollTop + grid.clientHeight) / rowH) + GRID_OVERSCAN);
  const key = `${first}:${last}:${cols}:${Math.round(rowH)}:${dirs.length}`;
  if (!force && key === lib.rangeKey) return;
  lib.rangeKey = key;
  // A spacer is a grid row too: the gap after it is part of its height.
  const [top, bottom] = lib.spacers;
  top.hidden = first === 0;
  top.style.height = `${first * rowH - GRID_GAP}px`;
  bottom.hidden = last >= rows;
  bottom.style.height = `${(rows - last) * rowH - GRID_GAP}px`;
  const cells = new Map();
  for (let i = first * cols; i < Math.min(last * cols, dirs.length + lib.shown.length); i++) {
    if (i < dirs.length) {
      const key = `dir:${dirs[i].path}:${dirs[i].up ? 1 : 0}`;
      cells.set(key, lib.cells.get(key) ?? makeDirCell(dirs[i]));
    } else {
      const p = lib.shown[i - dirs.length];
      cells.set(p.path, lib.cells.get(p.path) ?? makeCell(p));
    }
  }
  lib.cells = cells;
  grid.replaceChildren(top, ...cells.values(), bottom);
  lib.observer.disconnect();
  for (const cell of cells.values()) if (cell.querySelector("img") && !cell.querySelector("img").getAttribute("src")) lib.observer.observe(cell);
}

function makeCell(p) {
  const cell = document.createElement("div");
  cell.className = "lib-cell";
  cell.dataset.path = p.path;
  cell.title = [p.name, p.captured, p.keywords.join(", "), p.offline && "Offline: its drive isn't plugged in"].filter(Boolean).join("\n");
  cell.classList.toggle("offline", Boolean(p.offline));
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
  cell.classList.toggle("selected", lib.selected.has(p.path));
  cell.classList.toggle("focus", p.path === lib.focus);
  const url = cachedThumb(p);
  if (url) cell.querySelector("img").src = url;
  return cell;
}

// Scroll so a photo is in view, drawing it if it wasn't.
function revealPhoto(path, block = "nearest") {
  const grid = $("library-grid");
  const at = lib.shown.findIndex((p) => p.path === path);
  if (at < 0) return;
  const i = at + (lib.dirs?.length ?? 0);
  const { cols, rowH } = gridMetrics();
  const y = GRID_PAD + Math.floor(i / cols) * rowH;
  const h = rowH - GRID_GAP;
  if (block === "center") grid.scrollTop = y - (grid.clientHeight - h) / 2;
  else if (y < grid.scrollTop) grid.scrollTop = y - GRID_PAD;
  else if (y + h > grid.scrollTop + grid.clientHeight) grid.scrollTop = y + h - grid.clientHeight + GRID_PAD;
  renderRange();
}

// Pictures are kept for the photos seen lately; older ones are let go (and
// fetched again from the disk cache if they come back into view).
const THUMB_URLS_KEPT = 3000;

function keepThumbUrl(key, url) {
  lib.urls.set(key, url);
  if (lib.urls.size <= THUMB_URLS_KEPT) return;
  for (const [k, u] of lib.urls) {
    if (lib.urls.size <= THUMB_URLS_KEPT * 0.8) break;
    if (lib.cells.has(k.split("|").slice(0, -2).join("|"))) continue;
    URL.revokeObjectURL(u);
    lib.urls.delete(k);
  }
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
  cullCell(cell, p);
}

function renderCounts() {
  const n = lib.photos.length;
  const shown = lib.shown.length;
  const sel = selectedPhotos().length;
  $("library-count").textContent = !lib.dir && !lib.collection ? "" : [
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
        keepThumbUrl(key, url);
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
        keepThumbUrl(key, url);
        cell.querySelector("img").src = url;
      })
      .catch(() => {})
      .finally(() => {
        lib.loading--;
        pumpLibraryThumbs();
      });
  }
}

// A photo was turned: its camera preview and its render both turn.
function turnedThumb(path) {
  for (const key of [...lib.urls.keys()]) if (key.startsWith(`${path}|`)) lib.urls.delete(key);
  if (lib.cells.has(path)) lib.queue.unshift(path);
  pumpLibraryThumbs();
}

// A photo's edit changed: show the new render when it comes.
function refreshThumb(path) {
  for (const key of [...lib.urls.keys()]) if (key.startsWith(`${path}|`) && key.endsWith("|true")) lib.urls.delete(key);
  lib.editedQueue.push(path);
  pumpLibraryThumbs();
}
