// SPDX-License-Identifier: GPL-3.0-or-later
// The library's culling views, as in Lightroom: loupe (E, one photo large),
// compare (C, the select beside a candidate) and survey (N, the selection
// tiled to fit). G or Esc goes back to the grid.
//
// In compare and survey, rating, label and flag keys act on the active photo
// (the one with the accent frame), not on the whole selection.
"use strict";

const cull = {
  view: "grid",     // "grid", "loupe", "compare", "survey", or browse.js's "timeline" or "map"
  active: null,     // the photo keys act on
  select: null,     // compare: the left photo
  candidate: null,  // compare: the right photo
  survey: [],       // survey: the photos shown
  loads: new Map(), // `${path}|${edge}|${edited}` -> Promise<url>
};

const cullOn = () => lib.on && ["loupe", "compare", "survey"].includes(cull.view);

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
  } else if (view === "survey") {
    cull.survey = sel.length > 1 ? sel : order.slice(Math.max(0, order.indexOf(focus)), order.indexOf(focus) + 12);
    cull.active = cull.survey.includes(focus) ? focus : cull.survey[0];
  }
  cull.view = view;
  $("library-view").hidden = view === "grid";
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
  box.className = `library-view ${cull.view}`;
  if (cull.view === "loupe") {
    box.replaceChildren(cullTile(cull.active, { big: true }));
  } else if (cull.view === "compare") {
    box.replaceChildren(
      cullTile(cull.select, { tag: "Select" }),
      cullTile(cull.candidate, { tag: "Candidate" }),
    );
  } else if (cull.view === "survey") {
    box.replaceChildren(...cull.survey.map((p) => cullTile(p, { removable: true })));
    layoutSurvey();
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

function cullTile(path, { tag, big, removable } = {}) {
  const p = libPhoto(path);
  const tile = Object.assign(document.createElement("figure"), { className: "cull-tile" });
  if (!p) return tile;
  tile.dataset.path = path;
  tile.classList.toggle("active", path === cull.active);
  tile.classList.toggle("rejected", p.flag === "reject");
  const frame = Object.assign(document.createElement("div"), { className: "cull-frame" });
  const img = Object.assign(document.createElement("img"), { alt: p.name, draggable: false });
  const url = cachedThumb(p);
  if (url) img.src = url;
  frame.append(img);
  const cap = Object.assign(document.createElement("figcaption"), { className: "cull-caption" });
  const flag = Object.assign(document.createElement("span"), { className: "lib-flag" });
  showFlag(flag, p.flag);
  const stars = Object.assign(document.createElement("span"), { className: "lib-stars", textContent: p.rating ? "★".repeat(p.rating) : "" });
  const label = Object.assign(document.createElement("span"), { className: "film-label", hidden: !p.label });
  if (p.label) label.style.background = LABEL_COLOURS[p.label] ?? "var(--muted)";
  const name = Object.assign(document.createElement("span"), { className: "cull-name", textContent: [tag, p.name].filter(Boolean).join(": ") });
  cap.append(flag, stars, label, name);
  if (removable) {
    const x = Object.assign(document.createElement("button"), { type: "button", className: "cull-remove", textContent: "×", title: "Take out of the survey" });
    x.addEventListener("click", (e) => {
      e.stopPropagation();
      surveyRemove(path);
    });
    cap.append(x);
  }
  tile.append(frame, cap);
  tile.addEventListener("click", () => {
    cull.active = path;
    for (const t of $("library-view").children) t.classList.toggle("active", t.dataset.path === path);
    renderPanel();
  });
  tile.addEventListener("dblclick", () => libraryOpen(path));
  // A large camera preview, then the edit's render when the photo has one.
  const edge = cullEdge(big);
  let rendered = false;
  bigImage(p, edge, false).then((u) => {
    if (!rendered) img.src = u;
  }).catch(() => {});
  if (p.edited) {
    bigImage(p, edge, true).then((u) => {
      rendered = true;
      img.src = u;
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

// As many columns as make the tiles largest.
function layoutSurvey() {
  const box = $("library-view");
  const n = Math.max(1, cull.survey.length);
  const w = box.clientWidth - 24;
  const h = box.clientHeight - 24;
  let best = { cols: 1, size: 0 };
  for (let cols = 1; cols <= n; cols++) {
    const rows = Math.ceil(n / cols);
    // Tiles hold a 3:2 photo plus a caption line.
    const tw = (w - (cols - 1) * 10) / cols;
    const th = (h - (rows - 1) * 10) / rows - 24;
    const size = Math.min(tw, th * 1.5);
    if (size > best.size) best = { cols, size };
  }
  box.style.setProperty("--cols", best.cols);
  box.style.setProperty("--rows", Math.ceil(n / best.cols));
}

function updateCullTile(path) {
  if (!cullOn()) return;
  const tile = [...$("library-view").children].find((t) => t.dataset.path === path);
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
  } else if (cull.view === "survey" && step) {
    const i = cull.survey.indexOf(cull.active);
    cull.active = cull.survey[Math.min(cull.survey.length - 1, Math.max(0, i + step))];
    for (const t of $("library-view").children) t.classList.toggle("active", t.dataset.path === cull.active);
    renderPanel();
    return;
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

function surveyRemove(path) {
  cull.survey = cull.survey.filter((p) => p !== path);
  lib.selected.delete(path);
  if (!cull.survey.length) return setLibraryView("grid");
  if (cull.active === path) cull.active = cull.survey[0];
  renderCull();
}

// Esc and G: back to the grid first.
function cullLeave() {
  return lib.on && cull.view !== "grid" ? setLibraryView("grid") : false;
}

function wireCull() {
  for (const b of document.querySelectorAll("#lib-views button")) b.addEventListener("click", () => setLibraryView(b.dataset.view));
  new ResizeObserver(() => cull.view === "survey" && layoutSurvey()).observe($("library-view"));
}
