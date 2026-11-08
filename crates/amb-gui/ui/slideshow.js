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
// step; Esc ends it on the photo you got to (selected in the library,
// open in Develop). A mode, not a tab (docs/design/tabs.md).
"use strict";

const SLIDE_EFFECTS = ["fade", "slide", "zoom", "black"];
const slides = { on: false, list: [], at: 0, timer: 0, paused: false, hideBar: 0, wasFullscreen: false, el: null, layer: 0 };

function slideshowSettings() {
  const d = { seconds: 4, shuffle: false, fullscreen: true, effect: "fade" };
  try {
    return { ...d, ...JSON.parse(localStorage.getItem("amb.slideshow") || "{}") };
  } catch {
    return d;
  }
}

// The library's selected photos (two or more), else all it shows.
function slideshowPhotos() {
  const sel = selectedPhotos();
  return sel.length > 1 ? sel : lib.shown;
}

function slideshowFlow() {
  const photos = slideshowPhotos();
  if (!photos.length) return toast("No photos shown in the library.", true);
  const s = slideshowSettings();
  const f = $("slideshow-form");
  f.elements.seconds.value = s.seconds;
  f.elements.shuffle.checked = s.shuffle;
  f.elements.fullscreen.checked = s.fullscreen;
  f.elements.effect.value = s.effect;
  $("slideshow-count").textContent = `${photos.length} photo${photos.length === 1 ? "" : "s"}${selectedPhotos().length > 1 ? " selected" : ""}`;
  $("slideshow-dialog").showModal();
  return true;
}

// `remember`: keep the settings for next time (not for `amb slideshow`'s);
// `done(photo)`: called with the photo shown last when it ends.
async function startSlideshow(photos, settings, { remember = true, done = null, start = 0 } = {}) {
  if (remember) {
    try { localStorage.setItem("amb.slideshow", JSON.stringify(settings)); } catch { /* not kept */ }
  }
  slides.done = done;
  slides.settings = settings;
  slides.list = settings.shuffle ? shuffled(photos) : [...photos];
  slides.at = start;
  slides.paused = false;
  slides.on = true;
  const viewer = Boolean(settings.viewer);
  const el = Object.assign(document.createElement("div"), { className: `slideshow${viewer ? " viewer" : ""}`, tabIndex: -1 });
  const pause = viewer ? "" : '<button type="button" data-do="pause" title="Pause (Space)">❚❚</button>';
  el.innerHTML = `<img class="ss-img" alt=""><img class="ss-img" alt=""><div class="ss-bar" hidden><button type="button" data-do="prev" title="Previous (←)">‹</button>${pause}<button type="button" data-do="next" title="Next (→)">›</button><span class="ss-where"></span><button type="button" data-do="end" title="End (Esc)">✕</button></div>`;
  document.body.append(el);
  slides.el = el;
  // The viewer's thirds: left steps back, right on, the middle only shows
  // the bar.
  const side = (e) => {
    const x = e.clientX / innerWidth;
    return x < 0.3 ? -1 : x > 0.7 ? 1 : 0;
  };
  el.addEventListener("click", (e) => {
    const b = e.target.closest("[data-do]");
    if (b) {
      e.stopPropagation();
      ({ prev: () => stepSlide(-1), next: () => stepSlide(1), pause: togglePause, end: endSlideshow })[b.dataset.do]();
      return;
    }
    if (!viewer) return endSlideshow();
    const d = side(e);
    if (d) stepSlide(d);
    else showSlideBar();
  });
  el.addEventListener("mousemove", (e) => {
    showSlideBar();
    if (viewer) {
      el.classList.toggle("at-left", side(e) < 0);
      el.classList.toggle("at-right", side(e) > 0);
    }
  });
  el.focus();
  if (settings.fullscreen) {
    slides.wasFullscreen = await invoke("window_fullscreen", { on: true }).catch(() => true);
  }
  await showSlide(start, true);
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
  if (!list.length) return toast(libraryOn() ? "No photos shown in the library." : "Open a photo first.", true);
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
  slides.el.querySelector(".ss-where").textContent = `${slides.at + 1} / ${n} · ${p.name ?? fileName(p.path)}`;
  // Fetch the next one meanwhile (and, viewing, the one before).
  slideImage(slides.list[(slides.at + 1) % n]).catch(() => {});
  if (slides.settings.viewer && n > 2) slideImage(slides.list[(slides.at + n - 1) % n]).catch(() => {});
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
  if (slides.settings.fullscreen && !slides.wasFullscreen) await invoke("window_fullscreen", { on: false }).catch(() => {});
}

function wireSlideshow() {
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
  // Its own keys while it runs; nothing else hears them.
  window.addEventListener("keydown", (e) => {
    if (!slides.on) return;
    e.preventDefault();
    e.stopImmediatePropagation();
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
