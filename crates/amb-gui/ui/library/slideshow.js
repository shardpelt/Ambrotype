// SPDX-License-Identifier: GPL-3.0-or-later
// Slideshow, in the library (Ctrl+Enter or ▶ Slideshow in its bar; `amb
// slideshow` from a terminal): the photos shown (or the selected ones),
// each with its edit, full screen on black. Settings: seconds per photo, in order or shuffled, full screen or
// in the window, and a change effect (none, fade, slide, slow zoom, through
// black, or a random one each time). Kept on this computer.
//
// While it runs: Space pauses, ← → step, Esc (or a click) ends it; moving
// the mouse shows a small bar with the same buttons.
//
// The viewer (V, ⛶ View in the library bar or the filmstrip's) is the same
// screen as an image viewer: no timer, starting at the photo you're on;
// ← → (or Space, Home, End) and a click on the left or right of the screen
// step; it zooms as Develop does (library/viewer-zoom.js); Esc ends it on the photo you got to (selected in the library,
// open in Develop). 0–5 rate, P X U flag, 6–9 label, as in the library;
// I or a right-click opens the info panel: the shot (camera, lens,
// exposure, date, size) and the rating, label, flag and keywords, to
// change there. A mode, not a tab (docs/design/tabs.md).
"use strict";

const SLIDE_EFFECTS = ["fade", "slide", "zoom", "black"];
const slides = { on: false, list: [], at: 0, timer: 0, paused: false, hideBar: 0, wasFullscreen: false, el: null, layer: 0 };

function slideshowSettings() {
  const d = { seconds: 4, shuffle: false, fullscreen: true, effect: "fade" };
  return { ...d, ...store.get("amb.slideshow", {}) };
}

// The library's selected photos (two or more), else all it shows.
// In Develop, the filmstrip's.
function slideshowPhotos() {
  if (!libraryOn()) return strip.photos;
  const sel = selectedPhotos();
  return sel.length > 1 ? sel : lib.shown;
}

function slideshowFlow() {
  const photos = slideshowPhotos();
  if (!photos.length) return toast(libraryOn() ? t("No photos shown in the library.") : t("Open a photo first."), true);
  const s = slideshowSettings();
  const f = $("slideshow-form");
  f.elements.seconds.value = s.seconds;
  f.elements.shuffle.checked = s.shuffle;
  f.elements.fullscreen.checked = s.fullscreen;
  f.elements.effect.value = s.effect;
  $("slideshow-count").textContent = libraryOn() && selectedPhotos().length > 1 ? t("{photos} selected", { photos: plural(photos.length, "photo") }) : plural(photos.length, "photo");
  $("slideshow-dialog").showModal();
  return true;
}

// `remember`: keep the settings for next time (not for `amb slideshow`'s);
// `done(photo)`: called with the photo shown last when it ends.
async function startSlideshow(photos, settings, { remember = true, done = null, start = 0 } = {}) {
  if (remember) {
    store.set("amb.slideshow", settings);
  }
  slides.done = done;
  slides.settings = settings;
  slides.list = settings.shuffle ? shuffled(photos) : [...photos];
  slides.at = start;
  slides.paused = false;
  slides.on = true;
  const viewer = Boolean(settings.viewer);
  const barTitles = { left: t("Rotate left (Ctrl+[)"), right: t("Rotate right (Ctrl+])"), pause: t("Pause (Space)"), prev: t("Previous (←)"), next: t("Next (→)"), end: t("End (Esc)") };
  const barButton = (what, text) => el("button", { type: "button", "data-do": what, title: barTitles[what] }, text);
  const bar = el("div", { class: "ss-bar", hidden: true },
    barButton("prev", "‹"),
    ...(viewer ? [barButton("left", "⟲"), barButton("right", "⟳")] : [barButton("pause", "❚❚")]),
    barButton("next", "›"), el("span", { class: "ss-where" }), barButton("end", "✕"));
  const show = el("div", { class: `slideshow${viewer ? " viewer" : ""}`, tabindex: -1 },
    el("img", { class: "ss-img", alt: "" }), el("img", { class: "ss-img", alt: "" }),
    viewer ? el("aside", { class: "ss-info", hidden: true }) : null, bar);
  document.body.append(show);
  slides.el = show;
  // The viewer's thirds: left steps back, right on, the middle only shows
  // the bar.
  const side = (e) => {
    const x = e.clientX / innerWidth;
    return x < 0.3 ? -1 : x > 0.7 ? 1 : 0;
  };
  show.addEventListener("contextmenu", (e) => {
    e.preventDefault();
    // A right-drag zoomed to a box (viewer-zoom.js); a right click opens the panel.
    if (show.dataset.boxed) return delete show.dataset.boxed;
    if (viewer) toggleViewerInfo();
  });
  if (viewer) wireViewerZoom(show);
  show.addEventListener("click", (e) => {
    if (e.target.closest(".ss-info")) return;
    const b = e.target.closest("[data-do]");
    if (b) {
      e.stopPropagation();
      ({ prev: () => stepSlide(-1), next: () => stepSlide(1), pause: togglePause, end: endSlideshow, left: () => viewerTurn({ rotate: -1 }), right: () => viewerTurn({ rotate: 1 }) })[b.dataset.do]();
      return;
    }
    if (!viewer) return endSlideshow();
    // The end of a drag round a zoomed photo isn't a click.
    if (show.dataset.dragged) return delete show.dataset.dragged;
    const d = vz.z?.s > 1 ? 0 : side(e);
    if (d) stepSlide(d);
    else {
      vz.z.toggleFull(e.clientX, e.clientY);
      showSlideBar();
    }
  });
  show.addEventListener("mousemove", (e) => {
    showSlideBar();
    if (viewer) {
      show.classList.toggle("at-left", side(e) < 0);
      show.classList.toggle("at-right", side(e) > 0);
    }
  });
  show.focus();
  if (settings.fullscreen) {
    slides.wasFullscreen = await invoke("window_fullscreen", { on: true }).catch(() => true);
  }
  if (viewer) {
    let open = false;
    open = store.text("amb.viewerInfo") === "1";
    if (open) show.querySelector(".ss-info").hidden = false;
  }
  // Steps taken while the window went full screen count.
  await showSlide(slides.at, true);
  schedule();
}

// The viewer: the library's photos as shown (from the focused one), or in
// Develop the filmstrip's (from the open one).
function viewerFlow() {
  let list = [];
  let start = 0;
  let done;
  if (libraryOn()) {
    list = lib.shown;
    start = Math.max(0, list.findIndex((p) => p.path === lib.focus));
    done = (p) => {
      if (!p) return;
      select(p.path);
      revealPhoto(p.path);
    };
  } else if (ui.info) {
    list = strip.photos;
    start = Math.max(0, stripIndexOf(ui.info.path));
    done = (p) => {
      if (p && strip.photos[stripIndexOf(ui.info?.path)]?.path !== p.path) openPath(p.path);
    };
  }
  if (!list.length) return toast(libraryOn() ? t("No photos shown in the library.") : t("Open a photo first."), true);
  return startSlideshow(list, { seconds: 0, shuffle: false, fullscreen: true, effect: "none", viewer: true }, { remember: false, done, start });
}

function shuffled(list) {
  const a = [...list];
  for (let i = a.length - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1));
    [a[i], a[j]] = [a[j], a[i]];
  }
  return a;
}

// A photo's large picture, with its edit: the camera's preview first if the
// render isn't ready yet.
async function slideImage(p) {
  const edge = Math.min(2048, Math.max(1024, Math.round(Math.max(screen.width, screen.height) * (window.devicePixelRatio || 1))));
  const size = edge > 1024 ? 2048 : 1024;
  try {
    return await bigImage(p, size, Boolean(p.edited));
  } catch {
    return bigImage(p, size, false);
  }
}

async function showSlide(i, first = false) {
  const n = slides.list.length;
  slides.at = ((i % n) + n) % n;
  const p = slides.list[slides.at];
  const url = await slideImage(p).catch(() => null);
  if (!slides.on) return;
  const imgs = slides.el.querySelectorAll(".ss-img");
  const old = imgs[slides.layer];
  const next = imgs[1 - slides.layer];
  slides.layer = 1 - slides.layer;
  let effect = slides.settings.effect;
  if (effect === "random") effect = SLIDE_EFFECTS[Math.floor(Math.random() * SLIDE_EFFECTS.length)];
  if (first || effect === "none") effect = first ? "fade" : "none";
  next.className = "ss-img";
  next.src = url ?? "";
  if (url) await next.decode().catch(() => {});
  if (!slides.on) return;
  // The incoming picture jumps to its start, then moves to its end with the
  // effect's timing while the outgoing one leaves.
  next.style.setProperty("--ss-seconds", `${slides.settings.seconds}s`);
  next.style.transition = "none";
  next.classList.add(`in-${effect}`);
  void next.offsetWidth;
  next.style.transition = "";
  old.className = `ss-img out-${effect}`;
  next.classList.add("shown");
  showWhere();
  if (slides.settings.viewer) renderViewerInfo();
  // Fetch the next one meanwhile (and, viewing, the one before).
  slideImage(slides.list[(slides.at + 1) % n]).catch(() => {});
  if (slides.settings.viewer && n > 2) slideImage(slides.list[(slides.at + n - 1) % n]).catch(() => {});
}

// The bar's text: where in the list, the name, and (viewing) the rating
// and flag.
function showWhere() {
  const n = slides.list.length;
  const p = slides.list[slides.at];
  const parts = [`${slides.at + 1} / ${n}`, p.name ?? fileName(p.path)];
  if (slides.settings.viewer) {
    if (p.rating) parts.push("★".repeat(p.rating));
    if (p.label) parts.push(t(p.label));
    if (p.flag) parts.push(p.flag === "pick" ? t("⚑ Pick") : t("✕ Rejected"));
  }
  slides.el.querySelector(".ss-where").textContent = parts.join(" · ");
}

// A change to the photo on screen (rating, label, flag, keywords), written
// as the library writes it (through the editor when it's the open photo),
// and shown in the library and filmstrip too.
async function viewerChange(change) {
  const p = slides.list[slides.at];
  try {
    const [u] = await setInfo([p.path], change);
    const info = u?.info ?? {};
    const at = info.gps?.split(",").map(Number);
    const fields = { rating: info.rating ?? null, label: info.label ?? null, flag: info.flag ?? null, keywords: info.keywords ?? [], gps: at?.length === 2 && at.every(Number.isFinite) ? at : null };
    photoChanged(p.path, fields);
    if (p.path === ui.info?.path) slides.openChanged = true;
    if (slides.on && slides.list[slides.at] === p) {
      showWhere();
      renderViewerInfo();
      showSlideBar();
    }
  } catch (e) {
    showError(e);
  }
}

// Turn the photo on screen (rotate or flip), as Photo → Rotate does, and
// show it again turned.
async function viewerTurn(change) {
  const p = slides.list[slides.at];
  try {
    await setInfo([p.path], change);
  } catch (e) {
    return showError(e);
  }
  turnedThumb(p.path);
  stripTurned(p.path);
  if (p.path === ui.info?.path) requestPreview(false);
  if (slides.on && slides.list[slides.at] === p) await showSlide(slides.at);
}

// The viewer's own copy of the Photo menu's turn keys (nothing else hears
// keys while it runs).
function viewerTurnKey(e) {
  const combo = comboOf(e);
  const turns = { "rotate-left": { rotate: -1 }, "rotate-right": { rotate: 1 }, "flip-horizontal": { flip: true } };
  const a = KEY_ACTIONS.find((x) => turns[x.id] && keysFor(x).includes(combo));
  return a ? turns[a.id] : null;
}

function toggleViewerInfo(on) {
  const panel = slides.el?.querySelector(".ss-info");
  if (!panel) return;
  panel.hidden = on === undefined ? !panel.hidden : !on;
  store.setText("amb.viewerInfo", panel.hidden ? "0" : "1");
  renderViewerInfo();
}

// The info panel: the shot, then what can be changed.
async function renderViewerInfo() {
  const panel = slides.el?.querySelector(".ss-info");
  if (!panel || panel.hidden) return;
  const p = slides.list[slides.at];
  const seq = (slides.infoSeq = (slides.infoSeq ?? 0) + 1);
  let d = p;
  try {
    d = { ...p, ...(await invoke("photo_details", { path: p.path })) };
  } catch { /* what the list knows */ }
  if (seq !== slides.infoSeq || !slides.on) return;
  const shutter = (s) => (s >= 1 ? `${Math.round(s * 10) / 10} s` : `1/${Math.round(1 / s)} s`);
  const rows = [
    [tKey("File"), d.name ?? fileName(d.path)],
    [tKey("Taken"), d.captured],
    [tKey("Camera"), d.camera],
    [tKey("Lens"), d.lens],
    [tKey("Exposure"), [d.shutter && shutter(d.shutter), d.aperture && `f/${Math.round(d.aperture * 10) / 10}`, d.iso && `ISO ${d.iso}`].filter(Boolean).join("  ·  ")],
    [tKey("Focal length"), d.focal && `${Math.round(d.focal)} mm`],
    [tKey("Size"), d.bytes && `${(d.bytes / 1e6).toFixed(1)} MB`],
    [tKey("Title"), d.title],
    [tKey("Folder"), dirOf(d.path)],
  ].filter(([, v]) => v);
  const stars = el("div", { class: "ss-stars" }, ...[1, 2, 3, 4, 5].map((k) => el("button", {
    type: "button", title: t("{stars} ({n})", { stars: plural(k, "star"), n: k }), class: k <= (d.rating ?? 0) ? "on" : "",
    onclick: () => viewerChange({ rating: (d.rating ?? 0) === k ? 0 : k }),
  }, "★")));
  const labels = el("div", { class: "ss-labels" }, ...Object.keys(LABEL_COLOURS).map((l, i) => {
    const b = el("button", { type: "button", title: i < 4 ? t("{label} ({key})", { label: t(l), key: 6 + i }) : t(l), class: d.label === l ? "on" : "", onclick: () => viewerChange({ label: d.label === l ? "" : l }) });
    b.style.setProperty("--swatch", LABEL_COLOURS[l]);
    return b;
  }));
  const flag = (f, text, key) => el("button", { type: "button", title: t("{flag} ({key})", { flag: t(text), key }), class: (d.flag ?? "") === f ? "on" : "", onclick: () => viewerChange({ flag: f }) }, t(text));
  const flags = el("div", { class: "ss-flags" }, flag("pick", tKey("Pick"), "P"), flag("reject", tKey("Reject"), "X"), flag("", tKey("Neither"), "U"));
  const keywords = el("div", { class: "ss-keywords" }, ...(d.keywords ?? []).map((k) => el("span", { class: "ss-chip", translate: "no" }, k, el("button", {
    type: "button", title: t("Remove {name}", { name: k }), onclick: () => viewerChange({ remove_keywords: [k] }),
  }, "×"))));
  const add = el("input", { type: "text", placeholder: t("Add keywords (Enter; commas between)"), spellcheck: "false" });
  add.addEventListener("keydown", (e) => {
    if (e.key === "Enter" && add.value.trim()) {
      viewerChange({ add_keywords: add.value.split(",").map((k) => k.trim()).filter(Boolean) });
      add.value = "";
    }
  });
  panel.replaceChildren(
    el("dl", {}, ...rows.flatMap(([k, v]) => [el("dt", {}, t(k)), el("dd", { title: v, translate: "no" }, v)])),
    el("h4", {}, t("Rating")), stars,
    el("h4", {}, t("Label")), labels,
    el("h4", {}, t("Flag")), flags,
    el("h4", {}, t("Keywords")), keywords, add,
    el("h4", {}, t("Place")), viewerPlaceRow(d),
    el("p", { class: "ss-info-hint" }, t("I or right-click hides this · 0–5 rate · P X U flag · 6–9 label · Ctrl+[ ] turn")),
  );
}

function schedule() {
  clearTimeout(slides.timer);
  if (!slides.on || slides.paused || slides.settings.viewer) return;
  slides.timer = setTimeout(async () => {
    await showSlide(slides.at + 1);
    schedule();
  }, slides.settings.seconds * 1000);
}

async function stepSlide(d) {
  vz.z?.reset();
  await showSlide(slides.at + d);
  schedule();
}

function togglePause() {
  if (slides.settings.viewer) return;
  slides.paused = !slides.paused;
  slides.el.querySelector('[data-do="pause"]').textContent = slides.paused ? "▶" : "❚❚";
  showSlideBar();
  schedule();
}

function showSlideBar() {
  const bar = slides.el?.querySelector(".ss-bar");
  if (!bar) return;
  bar.hidden = false;
  slides.el.classList.add("pointer");
  clearTimeout(slides.hideBar);
  slides.hideBar = setTimeout(() => {
    if (slides.paused) return;
    bar.hidden = true;
    slides.el?.classList.remove("pointer");
  }, 2000);
}

async function endSlideshow() {
  if (!slides.on) return;
  slides.on = false;
  clearTimeout(slides.timer);
  clearTimeout(slides.hideBar);
  slides.el.remove();
  slides.el = null;
  slides.done?.(slides.list[slides.at]);
  // The open photo was rated or tagged here: its panels catch up.
  if (slides.openChanged) {
    slides.openChanged = false;
    loadInfo();
  }
  if (slides.settings.fullscreen && !slides.wasFullscreen) await invoke("window_fullscreen", { on: false }).catch(() => {});
}

function wireSlideshow() {
  document.addEventListener("photo-changed", ({ detail: { path, changes } }) => {
    for (const x of slides.list) if (x.path === path) Object.assign(x, changes);
  });
  // Start, or Enter in the dialog.
  $("slideshow-form").addEventListener("submit", (e) => {
    e.preventDefault();
    const f = $("slideshow-form").elements;
    const seconds = Math.min(600, Math.max(1, Number(f.seconds.value) || 4));
    const settings = { seconds, shuffle: f.shuffle.checked, fullscreen: f.fullscreen.checked, effect: f.effect.value };
    const photos = slideshowPhotos();
    $("slideshow-dialog").close();
    startSlideshow(photos, settings);
  });
  $("slideshow-cancel").addEventListener("click", () => $("slideshow-dialog").close());
  $("lib-slideshow").addEventListener("click", slideshowFlow);
  $("lib-view").addEventListener("click", viewerFlow);
  $("strip-view").addEventListener("click", viewerFlow);
  $("strip-slideshow").addEventListener("click", slideshowFlow);
  // Its own keys while it runs; nothing else hears them.
  window.addEventListener("keydown", (e) => {
    if (!slides.on) return;
    // A dialog over it (choosing a place) has its own keys.
    if (e.target.closest?.("dialog[open]")) return;
    // Typing keywords in the viewer's info panel: the box has the keys
    // (Esc leaves it).
    if (e.target.closest?.(".ss-info") && e.target.matches("input")) {
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopImmediatePropagation();
        e.target.blur();
        slides.el.focus();
      }
      // The app's own keys leave text boxes alone.
      return;
    }
    e.preventDefault();
    e.stopImmediatePropagation();
    const viewing = slides.settings.viewer;
    const k = e.key.length === 1 ? e.key.toLowerCase() : e.key;
    if (viewing && vz.z.key(e)) return;
    const turn = viewing && viewerTurnKey(e);
    if (turn) return viewerTurn(turn);
    if (viewing && !e.ctrlKey && !e.metaKey && !e.altKey) {
      if (/^[0-5]$/.test(k)) return viewerChange({ rating: Number(k) });
      if (/^[6-9]$/.test(k)) {
        const l = Object.keys(LABEL_COLOURS)[Number(k) - 6];
        return viewerChange({ label: slides.list[slides.at].label === l ? "" : l });
      }
      if (k === "p") return viewerChange({ flag: "pick" });
      if (k === "x") return viewerChange({ flag: "reject" });
      if (k === "u") return viewerChange({ flag: "" });
      if (k === "i") return toggleViewerInfo();
    }
    if (e.key === "Escape") endSlideshow();
    else if (slides.settings.viewer && [" ", "PageDown"].includes(e.key)) stepSlide(1);
    else if (slides.settings.viewer && e.key === "PageUp") stepSlide(-1);
    else if (slides.settings.viewer && e.key === "Home") stepSlide(-slides.at);
    else if (slides.settings.viewer && e.key === "End") stepSlide(slides.list.length - 1 - slides.at);
    else if (e.key === " ") togglePause();
    else if (e.key === "ArrowRight") stepSlide(1);
    else if (e.key === "ArrowLeft") stepSlide(-1);
    showSlideBar();
  }, true);
}
