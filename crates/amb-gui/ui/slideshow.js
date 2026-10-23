// SPDX-License-Identifier: GPL-3.0-or-later
// Slideshow (Ctrl+Enter, the toolbar's ▶ or the library's Slideshow): the
// photos shown in the library (or the selected ones), or in the editor the
// folder's photos from the open one on, each with its edit, full screen on
// black. Settings: seconds per photo, in order or shuffled, full screen or
// in the window, and a change effect (none, fade, slide, slow zoom, through
// black, or a random one each time). Kept on this computer.
//
// While it runs: Space pauses, ← → step, Esc (or a click) ends it; moving
// the mouse shows a small bar with the same buttons.
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

// The photos a slideshow shows, from where you are.
function slideshowPhotos() {
  if (lib.on) {
    const sel = selectedPhotos();
    return sel.length > 1 ? sel : lib.shown;
  }
  if (!ui.info) return [];
  // The open photo shows its edit as it is now, saved or not.
  const all = (strip.photos.length ? strip.photos : [{ path: ui.info.path }])
    .map((p) => (p.path === ui.info.path ? { ...p, edited: true } : p));
  const i = Math.max(0, all.findIndex((p) => p.path === ui.info.path));
  return [...all.slice(i), ...all.slice(0, i)];
}

function slideshowFlow() {
  const photos = slideshowPhotos();
  if (!photos.length) return toast("Open a photo or a folder first.", true);
  const s = slideshowSettings();
  const f = $("slideshow-form");
  f.elements.seconds.value = s.seconds;
  f.elements.shuffle.checked = s.shuffle;
  f.elements.fullscreen.checked = s.fullscreen;
  f.elements.effect.value = s.effect;
  $("slideshow-count").textContent = `${photos.length} photo${photos.length === 1 ? "" : "s"}${lib.on && selectedPhotos().length > 1 ? " selected" : ""}`;
  $("slideshow-dialog").showModal();
  return true;
}

// `remember`: keep the settings for next time (not for `amb slideshow`'s);
// `done(photo)`: called with the photo shown last when it ends.
async function startSlideshow(photos, settings, { remember = true, done = null } = {}) {
  if (remember) {
    try { localStorage.setItem("amb.slideshow", JSON.stringify(settings)); } catch { /* not kept */ }
  }
  slides.done = done;
  slides.settings = settings;
  slides.list = settings.shuffle ? shuffled(photos) : [...photos];
  slides.at = 0;
  slides.paused = false;
  slides.on = true;
  const el = Object.assign(document.createElement("div"), { className: "slideshow", tabIndex: -1 });
  el.innerHTML = '<img class="ss-img" alt=""><img class="ss-img" alt=""><div class="ss-bar" hidden><button type="button" data-do="prev" title="Previous (←)">‹</button><button type="button" data-do="pause" title="Pause (Space)">❚❚</button><button type="button" data-do="next" title="Next (→)">›</button><span class="ss-where"></span><button type="button" data-do="end" title="End (Esc)">✕</button></div>';
  document.body.append(el);
  slides.el = el;
  el.addEventListener("click", (e) => {
    const b = e.target.closest("[data-do]");
    if (!b) return endSlideshow();
    e.stopPropagation();
    ({ prev: () => stepSlide(-1), next: () => stepSlide(1), pause: togglePause, end: endSlideshow })[b.dataset.do]();
  });
  el.addEventListener("mousemove", showSlideBar);
  el.focus();
  if (settings.fullscreen) {
    slides.wasFullscreen = await invoke("window_fullscreen", { on: true }).catch(() => true);
  }
  await showSlide(0, true);
  schedule();
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
  // Fetch the next one meanwhile.
  slideImage(slides.list[(slides.at + 1) % n]).catch(() => {});
}

function schedule() {
  clearTimeout(slides.timer);
  if (!slides.on || slides.paused) return;
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
  $("btn-slideshow").addEventListener("click", slideshowFlow);
  // Its own keys while it runs; nothing else hears them.
  window.addEventListener("keydown", (e) => {
    if (!slides.on) return;
    e.preventDefault();
    e.stopImmediatePropagation();
    if (e.key === "Escape") endSlideshow();
    else if (e.key === " ") togglePause();
    else if (e.key === "ArrowRight") stepSlide(1);
    else if (e.key === "ArrowLeft") stepSlide(-1);
    showSlideBar();
  }, true);
}
