// SPDX-License-Identifier: GPL-3.0-or-later
// The library's culling views, as in Lightroom: loupe (E, one photo large)
// and compare (C, the select beside a candidate, split left and right or
// top and bottom, either way round). G or Esc goes back to the grid.
//
// In compare, rating, label and flag keys act on the active photo (the one
// with the accent frame), not on the whole selection.
"use strict";

const cull = {
  view: "grid",     // "grid", "loupe", "compare", browse.js's "timeline" or "map", or "insights"
  active: null,     // the photo keys act on
  select: null,     // compare: the left photo
  candidate: null,  // compare: the right photo
  // compare: where the candidate is, beside the select: "right", "left",
  // "down" or "up". Kept on this computer.
  split: ["right", "left", "down", "up"].includes(store.text("amb.compareSplit")) ? store.text("amb.compareSplit") : "right",
  loads: new Map(), // `${path}|${edge}|${edited}` -> Promise<url>
  zoom: null,       // the zoom of loupe and compare's photos (viewer-zoom.js's makeZoom), made once in wireCull
};

const cullOn = () => lib.on && ["loupe", "compare"].includes(cull.view);

// The photos the panel and the rating keys act on.
function libraryTargets() {
  if (!cullOn() || cull.view === "loupe") return selectedPhotos();
  const p = libPhoto(cull.active);
  return p ? [p] : [];
}

function setLibraryView(view) {
  if (!lib.on) return false;
  const order = lib.shown.map((p) => p.path);
  if (!order.length && view !== "grid" && !BROWSE_VIEWS.includes(view)) return false;
  const sel = selectedPhotos().map((p) => p.path);
  // Going from one view to another keeps the active photo.
  const focus = cullOn() && order.includes(cull.active) ? cull.active
    : lib.focus && order.includes(lib.focus) ? lib.focus : sel[0] ?? order[0];
  if (view === "loupe") {
    cull.active = focus;
  } else if (view === "compare") {
    cull.select = sel[0] ?? focus;
    const after = order[order.indexOf(cull.select) + 1] ?? order[order.indexOf(cull.select) - 1];
    cull.candidate = sel.find((p) => p !== cull.select) ?? after ?? cull.select;
    cull.active = cull.candidate;
  }
  cull.view = view;
  navNote();
  // Not over the empty library's "Drop a folder" (no folder yet).
  $("library-view").hidden = view === "grid" || !$("library-empty").hidden;
  $("library-grid").hidden = view !== "grid";
  for (const b of document.querySelectorAll("#lib-views button")) b.classList.toggle("on", b.dataset.view === view);
  if (view === "grid") {
    if (cull.active && order.includes(cull.active)) {
      lib.focus = cull.active;
      renderSelection();
      revealPhoto(cull.active);
    }
    $("library-grid").focus({ preventScroll: true });
  } else if (BROWSE_VIEWS.includes(view)) {
    renderBrowse();
  } else {
    renderCull();
  }
  renderScrubber();
  return true;
}

// ---------------------------------------------------------------- drawing

function renderCull() {
  const box = $("library-view");
  cull.zoom?.reset();
  box.className = `library-view ${cull.view}`;
  if (cull.view === "loupe") {
    box.replaceChildren(cullTile(cull.active, { big: true }));
  } else if (cull.view === "compare") {
    const tiles = [cullTile(cull.select, { tag: tKey("Select") }), cullTile(cull.candidate, { tag: tKey("Candidate") })];
    if (cull.split === "left" || cull.split === "up") tiles.reverse();
    box.dataset.split = cull.split;
    box.replaceChildren(...tiles, compareSplitBar());
  }
  // Keep the grid's selection and focus on what's being looked at.
  if (cull.view === "loupe") {
    lib.selected = new Set([cull.active]);
    lib.focus = cull.active;
  }
  renderCounts();
  renderPanel();
  $("library-view").focus({ preventScroll: true });
}

// Which way compare splits: the candidate right, left, below or above.
const COMPARE_SPLITS = [
  ["right", "◧", tKey("Side by side, the candidate on the right")],
  ["left", "◨", tKey("Side by side, the candidate on the left")],
  ["down", "⬒", tKey("Above and below, the candidate below")],
  ["up", "⬓", tKey("Above and below, the candidate above")],
];

function compareSplitBar() {
  const bar = el("div", { class: "compare-split", role: "group", "aria-label": t("Split") });
  for (const [split, icon, title] of COMPARE_SPLITS) {
    const b = el("button", { type: "button", class: "small", title: t(title) }, icon);
    b.dataset.split = split;
    b.classList.toggle("on", split === cull.split);
    b.addEventListener("click", (e) => {
      e.stopPropagation();
      setCompareSplit(split);
    });
    bar.append(b);
  }
  return bar;
}

function setCompareSplit(split) {
  cull.split = split;
  store.setText("amb.compareSplit", split);
  if (cull.view === "compare") renderCull();
}

function cullTile(path, { tag, big } = {}) {
  const p = libPhoto(path);
  const tile = el("figure", { class: "cull-tile" });
  if (!p) return tile;
  tile.dataset.path = path;
  tile.classList.toggle("active", path === cull.active);
  tile.classList.toggle("rejected", p.flag === "reject");
  const frame = el("div", { class: "cull-frame" });
  const img = el("img", { alt: p.name, draggable: "false" });
  const url = cachedThumb(p);
  if (url) img.src = url;
  frame.append(img);
  const cap = el("figcaption", { class: "cull-caption" });
  const flag = el("span", { class: "lib-flag" });
  showFlag(flag, p.flag);
  const stars = el("span", { class: "lib-stars" }, p.rating ? "★".repeat(p.rating) : "");
  const label = el("span", { class: "film-label", hidden: !p.label });
  if (p.label) label.style.background = LABEL_COLOURS[p.label] ?? "var(--muted)";
  const name = el("span", { class: "cull-name", translate: tag ? null : "no" }, tag ? t("{tag}: {name}", { tag: t(tag), name: p.name }) : p.name);
  cap.append(flag, stars, label, name);
  tile.append(frame, cap);
  tile.addEventListener("click", (e) => {
    // The end of a drag round a zoomed photo isn't a click.
    if ($("library-view").dataset.dragged) return delete $("library-view").dataset.dragged;
    const was = cull.active === path;
    cull.active = path;
    for (const other of $("library-view").querySelectorAll(".cull-tile")) other.classList.toggle("active", other.dataset.path === path);
    renderPanel();
    // A click on the photo: 100 % there and back (in compare, once it's the active one).
    if ((big || was) && e.target.closest(".cull-frame")) cull.zoom?.toggleFull(e.clientX, e.clientY);
  });
  tile.addEventListener("dblclick", () => libraryOpen(path));
  // A large camera preview, then the edit's render when the photo has one.
  const edge = cullEdge(big);
  let rendered = false;
  bigImage(p, edge, false).then((u) => {
    if (!rendered && !img.dataset.sharp) img.src = u;
  }).catch(() => {});
  if (p.edited) {
    bigImage(p, edge, true).then((u) => {
      rendered = true;
      if (!img.dataset.sharp) img.src = u;
    }).catch(() => {});
  }
  return tile;
}

// Cached sizes: 1024 or 2048 pixels on the long edge.
function cullEdge(big) {
  const box = $("library-view");
  const want = (big ? Math.max(box.clientWidth, box.clientHeight) : Math.max(box.clientWidth / 2, box.clientHeight)) * (window.devicePixelRatio || 1);
  return want > 1100 ? 2048 : 1024;
}

function bigImage(p, edge, edited) {
  const key = `${p.path}|${edge}|${edited}`;
  if (lib.urls.has(key)) return Promise.resolve(lib.urls.get(key));
  if (!cull.loads.has(key)) {
    const call = edited && p.path === ui.info?.path
      ? invoke("edited_thumbnail", { path: p.path, edge })
      : invoke("thumbnail", { path: p.path, edge, edited });
    cull.loads.set(key, call.then((buf) => {
      const url = URL.createObjectURL(new Blob([new Uint8Array(buf)], { type: "image/jpeg" }));
      lib.urls.set(key, url);
      return url;
    }).finally(() => cull.loads.delete(key)));
  }
  return cull.loads.get(key);
}

function updateCullTile(path) {
  if (!cullOn()) return;
  const tile = [...$("library-view").querySelectorAll(".cull-tile")].find((t) => t.dataset.path === path);
  if (!tile) return;
  const p = libPhoto(path);
  showFlag(tile.querySelector(".lib-flag"), p.flag);
  tile.querySelector(".lib-stars").textContent = p.rating ? "★".repeat(p.rating) : "";
  const label = tile.querySelector(".film-label");
  label.hidden = !p.label;
  label.style.background = p.label ? LABEL_COLOURS[p.label] ?? "var(--muted)" : "";
  tile.classList.toggle("rejected", p.flag === "reject");
}

// ---------------------------------------------------------------- moving

function cullArrow(key) {
  cull.zoom?.reset();
  const order = lib.shown.map((p) => p.path);
  const step = key === "ArrowLeft" ? -1 : key === "ArrowRight" ? 1 : 0;
  if (cull.view === "loupe" && step) {
    const i = order.indexOf(cull.active);
    cull.active = order[Math.min(order.length - 1, Math.max(0, i + step))];
  } else if (cull.view === "compare") {
    if (step) {
      // The candidate steps through the shown photos, past the select.
      let i = order.indexOf(cull.candidate) + step;
      if (order[i] === cull.select) i += step;
      if (i < 0 || i >= order.length) return;
      cull.candidate = order[i];
      cull.active = cull.candidate;
    } else if (key === "ArrowDown") {
      [cull.select, cull.candidate] = [cull.candidate, cull.select];
      cull.active = cull.candidate;
    } else if (key === "ArrowUp") {
      compareMakeSelect();
      return;
    }
  } else {
    return;
  }
  renderCull();
}

// The candidate becomes the select; the next photo the candidate.
function compareMakeSelect() {
  const order = lib.shown.map((p) => p.path);
  cull.select = cull.candidate;
  let i = order.indexOf(cull.select) + 1;
  if (i >= order.length) i = order.indexOf(cull.select) - 1;
  cull.candidate = order[Math.max(0, i)] ?? cull.select;
  cull.active = cull.candidate;
  renderCull();
}

// Esc and G: back to the grid first.
function cullLeave() {
  cull.zoom?.reset();
  return lib.on && cull.view !== "grid" ? setLibraryView("grid") : false;
}

// Ctrl + − 0 in loupe and compare (true when they were the ones to use it).
function cullZoomKey(dir) {
  if (!cullOn() || !cull.zoom) return false;
  if (dir) cull.zoom.step(dir);
  else cull.zoom.reset();
  return true;
}

// Zoomed past the picture's detail: fetch the photo larger, once.
async function cullSharper(img) {
  if (img.dataset.sharp) return;
  img.dataset.sharp = "1";
  const p = libPhoto(img.closest(".cull-tile")?.dataset.path);
  if (!p) return;
  try {
    const url = await bigImage(p, 6144, Boolean(p.edited)).catch(() => bigImage(p, 6144, false));
    const probe = new Image();
    probe.src = url;
    await probe.decode();
    if (img.isConnected && probe.naturalWidth > img.naturalWidth) img.src = url;
  } catch {
    /* the smaller picture stays */
  }
}

// Wheel, pinch, drag and keys zoom loupe's photo, or compare's two together.
function wireCull() {
  const box = $("library-view");
  cull.zoom = makeZoom({
    host: box,
    images: () => [...box.querySelectorAll(".cull-frame img")],
    // The photo's box: the one under the point (compare has two), else the first.
    frame: (cx, cy) => {
      const frames = [...box.querySelectorAll(".cull-frame")].map((f) => f.getBoundingClientRect());
      return frames.find((r) => cx >= r.left && cx <= r.right && cy >= r.top && cy <= r.bottom) ?? frames[0] ?? { left: 0, top: 0, width: 1, height: 1 };
    },
    sharper: cullSharper,
    active: (e) => cullOn() && !e.target.closest(".compare-split, .cull-caption"),
  });
  cull.zoom.wire();
  for (const b of document.querySelectorAll("#lib-views button")) b.addEventListener("click", () => setLibraryView(b.dataset.view));
}
