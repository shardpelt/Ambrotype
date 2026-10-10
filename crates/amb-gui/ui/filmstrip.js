// SPDX-License-Identifier: GPL-3.0-or-later
// The filmstrip: the other photos in the open photo's folder, under the photo.
// Click one, or use ← →, to open it; edits to the photo you leave are saved
// to its sidecar first (src/main.rs `open_photo`). Thumbnails are the
// previews cameras embed (src/folder.rs), then, for photos with edits, a render
// of the edit (cached until the sidecar changes); the open photo's own cell
// shows its current edit.
"use strict";

const strip = {
  dir: null,
  photos: [],          // [{ path, name, rating, edited }]
  cells: new Map(),    // path -> cell element
  edited: new Map(),   // path -> data URL of its last edited preview (this session)
  queue: [],
  editedQueue: [],     // edited photos whose render replaces the camera preview
  loading: 0,
  observer: null,
  hidden: false,
};

const dirOf = (p) => p.replace(/[\\/][^\\/]*$/, "");

function stripVisible() {
  return Boolean(ui.info) && !strip.hidden && ui.layout?.filmstrip !== false;
}

function applyStripVisibility() {
  $("filmstrip").hidden = !stripVisible();
}

async function loadFolder(path) {
  const dir = dirOf(path);
  if (dir === strip.dir && strip.photos.some((p) => p.path === path)) return markCurrent();
  strip.dir = dir;
  try {
    strip.photos = await invoke("folder_photos", { path });
  } catch (e) {
    strip.photos = [];
    toast(`Couldn't list the folder: ${e}`, true);
  }
  renderStrip();
}

function starText(n) {
  return n ? "★".repeat(n) : "";
}

function renderStrip() {
  const list = $("filmstrip-list");
  strip.observer?.disconnect();
  strip.queue = [];
  strip.editedQueue = [];
  strip.cells.clear();
  list.replaceChildren();
  strip.observer = new IntersectionObserver((entries) => {
    for (const e of entries) {
      if (!e.isIntersecting) continue;
      strip.observer.unobserve(e.target);
      queueThumb(e.target.dataset.path);
    }
  }, { root: list, rootMargin: "0px 400px" });
  for (const p of strip.photos) {
    const cell = document.createElement("button");
    cell.type = "button";
    cell.className = "film-cell";
    cell.dataset.path = p.path;
    cell.title = p.name;
    const img = document.createElement("img");
    img.alt = "";
    img.draggable = false;
    const meta = document.createElement("span");
    meta.className = "film-meta";
    const stars = Object.assign(document.createElement("span"), { className: "film-stars", textContent: starText(p.rating) });
    const dot = Object.assign(document.createElement("span"), { className: "film-edited", title: "Has edits", hidden: !p.edited });
    const label = Object.assign(document.createElement("span"), { className: "film-label", hidden: !p.label, title: p.label ? `${p.label} label` : "" });
    if (p.label) label.style.background = LABEL_COLOURS[p.label] ?? "var(--muted)";
    const flag = Object.assign(document.createElement("span"), { className: "film-flag" });
    showFlag(flag, p.flag);
    cell.classList.toggle("rejected", p.flag === "reject");
    meta.append(flag, stars, label, dot);
    const name = Object.assign(document.createElement("span"), { className: "film-name", textContent: p.name });
    cell.append(img, meta, name);
    cell.addEventListener("click", async () => {
      if (p.path === ui.info?.path) return;
      if (!(await openPath(p.path))) {
        p.broken = true;
        cell.classList.add("broken");
      }
    });
    if (strip.edited.has(p.path)) img.src = strip.edited.get(p.path);
    else strip.observer.observe(cell);
    strip.cells.set(p.path, cell);
    list.append(cell);
  }
  $("filmstrip-count").textContent = strip.photos.length ? `${strip.photos.length} photos` : "";
  markCurrent();
}

// At most three thumbnails at a time, nearest first.
function queueThumb(path) {
  strip.queue.push(path);
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

function markCurrent() {
  const current = ui.info?.path;
  for (const [path, cell] of strip.cells) {
    const on = path === current;
    cell.classList.toggle("current", on);
    cell.setAttribute("aria-current", on ? "true" : "false");
    if (on) cell.scrollIntoView({ block: "nearest", inline: "nearest" });
  }
  const i = strip.photos.findIndex((p) => p.path === current);
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
    cell.querySelector(".film-stars").textContent = starText(rating);
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
    let i = strip.photos.findIndex((p) => p.path === ui.info?.path);
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
  // A vertical wheel scrolls the strip sideways.
  const list = $("filmstrip-list");
  list.addEventListener("wheel", (e) => {
    if (Math.abs(e.deltaY) > Math.abs(e.deltaX)) {
      e.preventDefault();
      list.scrollLeft += e.deltaY;
    }
  }, { passive: false });
  $("filmstrip-prev").addEventListener("click", () => stepPhoto(-1));
  $("filmstrip-next").addEventListener("click", () => stepPhoto(1));
  $("preview").addEventListener("load", () => {
    if (!ui.preview.draft && !ui.preview.uncropped && !ui.before && !ui.zoom.on && !ui.clipping && !ui.preview.overlay) stripShowPreview($("preview"));
  });
}
