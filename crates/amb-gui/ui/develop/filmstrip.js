// SPDX-License-Identifier: GPL-3.0-or-later
// The filmstrip: the other photos in the open photo's folder, under the photo.
// Click one, or use ← →, to open it; edits to the photo you leave are saved
// to its sidecar first (src/commands/photo.rs `open_photo`). Thumbnails are the
// previews cameras embed (src/folder.rs), then, for photos with edits, a render
// of the edit (cached until the sidecar changes); the open photo's own cell
// shows its current edit. Ctrl- (⌘-) and Shift-click select photos instead of
// opening one; right-click then applies a preset, pastes settings, exports or
// converts them all.
"use strict";

const strip = {
  dir: null,
  photos: [],          // [{ path, name, rating, edited }]
  cells: new Map(),    // path -> cell element
  edited: new Map(),   // path -> data URL of its last edited preview (this session)
  queue: [],
  editedQueue: [],     // edited photos whose render replaces the camera preview
  loading: 0,
  row: null,           // the photoRow (core/photo-row.js), made by wireFilmstrip
  current: null,       // the open photo's path in the strip
  hidden: false,
  selected: new Set(), // paths picked with Ctrl- or Shift-click
  anchor: null,        // where a Shift-click range starts
};

const stripSelected = () => strip.photos.filter((p) => strip.selected.has(p.path));

// Ctrl-click toggles a photo, Shift-click picks the range from the last one;
// the open photo joins a new selection.
function stripSelect(path, how) {
  if (!strip.selected.size && ui.info?.path && path !== ui.info.path) strip.selected.add(ui.info.path);
  const order = strip.photos.map((p) => p.path);
  if (how === "range") {
    const from = Math.max(0, order.indexOf(strip.anchor ?? ui.info?.path));
    const to = order.indexOf(path);
    for (let i = Math.min(from, to); i <= Math.max(from, to); i++) strip.selected.add(order[i]);
  } else if (strip.selected.has(path)) strip.selected.delete(path);
  else strip.selected.add(path);
  strip.anchor = path;
  markSelected();
}

function stripClearSelection() {
  strip.selected.clear();
  strip.anchor = null;
  markSelected();
}

function markSelected() {
  stripMark();
  const n = strip.selected.size;
  $("filmstrip-count").textContent = n ? `${n} selected` : strip.photos.length ? `${strip.photos.length} photos` : "";
}

// Right-click a filmstrip photo: actions for the selection (or that photo).
function stripMenu(e, p) {
  e.preventDefault();
  if (!strip.selected.has(p.path)) {
    strip.selected.clear();
    strip.selected.add(p.path);
    strip.anchor = p.path;
    markSelected();
  }
  const paths = stripSelected().map((x) => x.path);
  const n = paths.length;
  const many = (one, more) => (n > 1 ? more.replace("N", n) : one);
  contextMenu(e, [
    p.path !== ui.info?.path && ["Open", () => openPath(p.path)],
    [many("Apply a preset…", "Apply a preset to N photos…"), () => presetsOnPhotos(paths)],
    lib.copied && [many("Paste settings…", "Paste settings onto N photos…"), () => libraryPasteSettings()],
    [many("Export…", "Export N photos…"), () => libraryExport()],
    [many("Convert to JPEG…", "Convert N raws to JPEGs…"), () => convertRawsFlow()],
    null,
    ["Copy the open photo's settings", () => libraryCopySettings()],
    n > 1 && ["Clear the selection", () => stripClearSelection()],
  ].filter((x) => x !== false && x !== undefined));
}

function stripVisible() {
  return Boolean(ui.info) && !strip.hidden && ui.layout?.filmstrip !== false;
}

function applyStripVisibility() {
  $("filmstrip").hidden = !stripVisible();
}

// The library bar's "JPEG if there is one" holds here too: a raw shot
// with a JPEG is its JPEG in the filmstrip, so ← → go through the JPEGs.
async function stripPhotos(photos) {
  if (!$("lib-prefer-jpeg")?.checked) return photos;
  const raws = photos.filter((p) => p.copy == null && !RASTER_FILE.test(p.path)).map((p) => p.path);
  if (!raws.length) return photos;
  try {
    const twins = await invoke("library_twin_jpegs", { paths: raws });
    const stem = (path) => path.replace(/\.[^./\\]+$/, "").toLowerCase();
    const byStem = new Map(twins.map((t) => [stem(t.path), t]));
    return photos.map((p) => (p.copy == null && byStem.has(stem(p.path)) && !RASTER_FILE.test(p.path) ? byStem.get(stem(p.path)) : p));
  } catch (e) {
    console.warn("JPEG twins:", e);
    return photos;
  }
}

// The switch is in the library bar and the filmstrip bar, wired at start
// (not when the library first opens), and remembered on this computer.
function wirePreferJpeg() {
  const inLib = $("lib-prefer-jpeg");
  const inStrip = $("strip-prefer-jpeg");
  let on = false;
  on = store.text("amb.preferJpeg") === "1";
  inLib.checked = inStrip.checked = on;
  const changed = (from) => {
    inLib.checked = inStrip.checked = from.checked;
    store.setText("amb.preferJpeg", from.checked ? "1" : "0");
    if (libraryOn()) renderLibrary();
    if (ui.info?.path) loadFolder(ui.info.path, true);
  };
  inLib.addEventListener("change", () => changed(inLib));
  inStrip.addEventListener("change", () => changed(inStrip));
}

document.addEventListener("DOMContentLoaded", wirePreferJpeg);

async function loadFolder(path, force = false) {
  const dir = dirOf(path);
  if (!force && dir === strip.dir && strip.photos.some((p) => p.path === path)) return markCurrent();
  strip.dir = dir;
  try {
    strip.photos = await stripPhotos(await invoke("folder_photos", { path }));
  } catch (e) {
    strip.photos = [];
    toast(`Couldn't list the folder: ${e}`, true);
  }
  renderStrip();
  watchShownFolders();
}

// Watch the folders on screen, so photos added or removed outside the app
// (a card copy, another program) show up without ↻.
function watchShownFolders() {
  const dirs = new Set([strip.dir, ...(lib.collection || lib.smart ? [] : [lib.dir, ...(lib.more ?? [])])].filter(Boolean));
  invoke("watch_folders", { dirs: [...dirs], recursive: $("lib-subfolders").checked }).catch(() => {});
}

async function folderChanged() {
  if (libraryOn() && lib.dir && !lib.collection && !lib.smart) await loadLibrary(lib.dir);
  if (ui.info?.path) await loadFolder(ui.info.path, true);
}

function starText(n) {
  return n ? "★".repeat(n) : "";
}

// Settings → General: a filmstrip photo's stars can be clicked to rate it
// (hovering shows the missing ones in grey); on unless turned off.
function stripStarsClickable() {
  return store.text("amb.stripStars") !== "0";
}

function renderStars(box, rating) {
  const n = rating ?? 0;
  box.classList.toggle("editable", stripStarsClickable());
  if (!stripStarsClickable()) {
    box.textContent = starText(n);
    return;
  }
  box.replaceChildren(...[1, 2, 3, 4, 5].map((k) => {
    const s = el("span", { title: plural(k, "star") }, "★");
    s.dataset.stars = k;
    s.classList.toggle("on", k <= n);
    return s;
  }));
}

// A star on a filmstrip photo: that rating (the same one again clears it).
async function rateFromStrip(p, k) {
  const rating = (p.rating ?? 0) === k ? 0 : k;
  if (p.path === ui.info?.path) return setRating(rating);
  try {
    const [u] = await setInfo([p.path], { rating });
    stripUpdate(p.path, { rating: u?.info.rating ?? (rating || null) });
    status(rating ? `Rated ${p.name} ${"★".repeat(rating)}` : `Rating cleared: ${p.name}`);
  } catch (e) {
    showError(e);
  }
}

// One photo's cell: preview, flag, stars, label, "edited" dot and name.
function stripCell(p) {
  const stars = el("span", { class: "film-stars" });
  renderStars(stars, p.rating);
  stars.addEventListener("click", (e) => {
    const k = Number(e.target.dataset?.stars);
    if (!k) return;
    e.stopPropagation();
    rateFromStrip(p, k);
  });
  const label = el("span", { class: "film-label", hidden: !p.label, title: p.label ? `${p.label} label` : "" });
  if (p.label) label.style.background = LABEL_COLOURS[p.label] ?? "var(--muted)";
  const flag = el("span", { class: "film-flag" });
  showFlag(flag, p.flag);
  const img = el("img", { alt: "", draggable: "false" });
  if (strip.edited.has(p.path)) img.src = strip.edited.get(p.path);
  const cell = el("button", { type: "button", class: "film-cell", title: p.name },
    img,
    el("span", { class: "film-meta" }, flag, stars, label, el("span", { class: "film-edited", title: "Has edits", hidden: !p.edited })),
    el("span", { class: "film-name" }, p.name));
  cell.classList.toggle("rejected", p.flag === "reject");
  return cell;
}

// A click on a photo opens it (the selection ends); one that won't open is marked.
async function stripOpen(p) {
  if (strip.selected.size) stripClearSelection();
  if (p.path === ui.info?.path) return;
  if (!(await openPath(p.path))) {
    p.broken = true;
    strip.cells.get(p.path)?.classList.add("broken");
  }
}

function renderStrip() {
  strip.queue = [];
  strip.editedQueue = [];
  strip.row.set(strip.photos);
  // A new folder starts with nothing selected.
  for (const path of strip.selected) if (!strip.cells.has(path)) strip.selected.delete(path);
  markSelected();
  markCurrent();
}

// At most three thumbnails at a time, nearest first.
function queueThumb(path) {
  strip.queue.push(path);
  pumpThumbs();
}

// A photo was turned: fetch its thumbnail again.
function stripTurned(path) {
  if (!strip.cells.has(path)) return;
  strip.edited.delete(path);
  strip.queue.unshift(path);
  pumpThumbs();
}

function pumpThumbs() {
  while (strip.loading < 3 && strip.queue.length) {
    const path = strip.queue.shift();
    const cell = strip.cells.get(path);
    if (!cell || strip.edited.has(path)) continue;
    strip.loading++;
    invoke("thumbnail", { path })
      .then((buf) => {
        const img = cell.querySelector("img");
        if (strip.edited.has(path)) return;
        img.src = URL.createObjectURL(new Blob([new Uint8Array(buf)], { type: "image/jpeg" }));
        if (strip.photos.find((p) => p.path === path)?.edited) strip.editedQueue.push(path);
      })
      .catch(() => cell.classList.add("no-thumb"))
      .finally(() => {
        strip.loading--;
        pumpThumbs();
      });
  }
  // Renders of edits (slower: the raw is decoded) one at a time, after the previews.
  if (!strip.queue.length && strip.loading === 0 && strip.editedQueue.length) {
    const path = strip.editedQueue.shift();
    const cell = strip.cells.get(path);
    if (!cell || strip.edited.has(path) || path === ui.info?.path) return pumpThumbs();
    strip.loading++;
    invoke("thumbnail", { path, edited: true })
      .then((buf) => {
        if (!strip.edited.has(path)) cell.querySelector("img").src = URL.createObjectURL(new Blob([new Uint8Array(buf)], { type: "image/jpeg" }));
      })
      .catch(() => {})
      .finally(() => {
        strip.loading--;
        pumpThumbs();
      });
  }
}

// Where the open photo is in the filmstrip: itself, or (a raw shown by its
// JPEG, or the other way round) its twin of the same name.
function stripIndexOf(path) {
  const i = strip.photos.findIndex((p) => p.path === path);
  if (i >= 0 || !path) return i;
  const stem = path.replace(/\.[^./\\]+$/, "").toLowerCase();
  return strip.photos.findIndex((p) => p.copy == null && p.path.replace(/\.[^./\\]+$/, "").toLowerCase() === stem);
}

// The row's "selected" and "current" classes, from the two sets of state.
function stripMark() {
  strip.current = strip.photos[stripIndexOf(ui.info?.path)]?.path;
  strip.row.mark();
}

function markCurrent() {
  stripMark();
  if (strip.current) strip.row.reveal(strip.current);
  const i = stripIndexOf(strip.current);
  $("filmstrip-pos").textContent = i >= 0 ? `${i + 1} / ${strip.photos.length}` : "";
}

// The open photo's cell shows its edit: a small copy of each finished preview.
function stripShowPreview(img) {
  const path = ui.info?.path;
  const cell = path && strip.cells.get(path);
  if (!cell || !img.naturalWidth) return;
  const k = 256 / Math.max(img.naturalWidth, img.naturalHeight);
  const c = document.createElement("canvas");
  c.width = Math.max(1, Math.round(img.naturalWidth * k));
  c.height = Math.max(1, Math.round(img.naturalHeight * k));
  c.getContext("2d").drawImage(img, 0, 0, c.width, c.height);
  const url = c.toDataURL("image/jpeg", 0.8);
  strip.edited.set(path, url);
  cell.querySelector("img").src = url;
}

// Keep the strip's rating, label and "edited" marks in step with the open photo.
function stripUpdateCurrent(changes = {}) {
  if (ui.info) stripUpdate(ui.info.path, changes);
}

function stripUpdate(path, { rating, label, edited, keywords, flag } = {}) {
  const p = strip.photos.find((x) => x.path === path);
  const cell = p && strip.cells.get(p.path);
  if (!cell) return;
  if (rating !== undefined) {
    p.rating = rating;
    renderStars(cell.querySelector(".film-stars"), rating);
  }
  if (label !== undefined) {
    p.label = label;
    const mark = cell.querySelector(".film-label");
    mark.hidden = !label;
    mark.style.background = label ? LABEL_COLOURS[label] ?? "var(--muted)" : "";
  }
  if (edited !== undefined) {
    p.edited = edited;
    cell.querySelector(".film-edited").hidden = !edited;
  }
  if (keywords !== undefined) p.keywords = keywords;
  if (flag !== undefined) {
    p.flag = flag;
    showFlag(cell.querySelector(".film-flag"), flag);
    cell.classList.toggle("rejected", flag === "reject");
  }
}

// Files that wouldn't open are skipped (and marked) when stepping.
async function stepPhoto(delta) {
  if (strip.stepping) return;
  strip.stepping = true;
  try {
    let i = stripIndexOf(ui.info?.path);
    if (i < 0) return;
    for (i += delta; i >= 0 && i < strip.photos.length; i += delta) {
      const p = strip.photos[i];
      if (p.broken) continue;
      if (await openPath(p.path)) return;
      p.broken = true;
      strip.cells.get(p.path)?.classList.add("broken");
    }
  } finally {
    strip.stepping = false;
  }
}

function toggleFilmstrip() {
  strip.hidden = !strip.hidden;
  applyStripVisibility();
  if (stripVisible()) markCurrent();
  requestPreview(false);
}

function wireFilmstrip() {
  const pref = $("settings-strip-stars");
  pref.checked = stripStarsClickable();
  pref.addEventListener("change", () => {
    store.setText("amb.stripStars", pref.checked ? "1" : "0");
    for (const p of strip.photos) {
      const el = strip.cells.get(p.path)?.querySelector(".film-stars");
      if (el) renderStars(el, p.rating);
    }
  });
  strip.row = photoRow($("filmstrip-list"), {
    cell: stripCell,
    thumb: (p) => { if (!strip.edited.has(p.path)) queueThumb(p.path); },
    open: stripOpen,
    select: (p, how) => stripSelect(p.path, how),
    isSelected: (p) => strip.selected.has(p.path),
    isCurrent: (p) => p.path === strip.current,
    menu: stripMenu,
  });
  strip.cells = strip.row.cells;
  $("filmstrip-prev").addEventListener("click", () => stepPhoto(-1));
  $("filmstrip-next").addEventListener("click", () => stepPhoto(1));
  $("preview").addEventListener("load", () => {
    if (!ui.preview.draft && !ui.preview.uncropped && !ui.before && !ui.zoom.on && !ui.clipping && !ui.preview.overlay) stripShowPreview($("preview"));
  });
}
