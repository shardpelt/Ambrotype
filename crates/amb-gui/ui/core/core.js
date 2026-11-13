// SPDX-License-Identifier: GPL-3.0-or-later
// The editor's shared helpers, loaded before every other script: the
// backend (`invoke`), elements by id (`$`), the shared state (`ui`), notes
// (`toast`, `status`), questions (`askYes`), building elements (`el`) and
// paths (`fileName`, `folderName`, `dirOf`).
"use strict";

const tauri = window.__TAURI__;
const invoke = (cmd, args) => tauri.core.invoke(cmd, args);
const $ = (id) => document.getElementById(id);

const ui = {
  panels: [],            // editor schema
  defs: new Map(),       // setting name -> its control definition (from the schema)
  historyNames: new Map(), // setting name -> its name in History, where the label alone is ambiguous
  looks: null,           // layouts and themes (src/looks/), with the active ids
  layout: null,          // the active layout (layout.js renders it)
  histogram: null,       // the last histogram, redrawn when the theme or layout changes
  keybindings: {},       // shortcuts the user changed (keys.js has the defaults)
  controls: new Map(),   // name -> { def, input, number, label, select, check }
  settings: {},          // committed settings (from the Rust state)
  info: null,            // photo info
  overrides: {},         // values being dragged, not yet committed
  active: null,          // the setting whose slider is held (holdWhileDragging)
  before: false,
  // Before / after split with a draggable bar (split.js).
  split: false,
  hslTab: "Hue",
  // `uncropped`: the shown preview is the whole photo (what the crop tool draws on).
  preview: { inFlight: false, dirty: false, draft: false, url: null, uncropped: false },
  presets: [],
  zoom: { on: false, cx: 0.5, cy: 0.5, meta: null, pan: null },
  picking: false,
  clipping: false,
  curve: null,
  wheels: null,
  // Crop tool: `start` is the committed rectangle when the tool opened;
  // `aspect` the lock ("free", "original", "4:5", "size:<id>"), `swapped` its X.
  crop: { on: false, tool: null, start: null, enabled: false, aspect: "original", swapped: false, angleSign: 1 },
  sizes: [],             // size presets (built-in, then the user's)
  exportCropAspect: 1,
};

function toast(text, error = false) {
  const t = $("toast");
  t.textContent = text;
  t.classList.toggle("error", error);
  t.hidden = false;
  clearTimeout(toast.timer);
  toast.timer = setTimeout(() => (t.hidden = true), error ? 6000 : 3000);
}

// An error from a command, as a red toast.
function showError(e) {
  toast(`${e}`, true);
}

// Run `fn` with `button` disabled; an error shows in `line` (the dialog's status line).
// Gives true when `fn` finished, false when it threw.
async function withBusy(button, line, fn) {
  button.disabled = true;
  try {
    await fn();
    return true;
  } catch (e) {
    line.textContent = String(e);
    return false;
  } finally {
    button.disabled = false;
  }
}

// "1 photo", "3 photos": `many` defaults to `one` + "s".
function plural(n, one, many = `${one}s`) {
  return `${n} ${n === 1 ? one : many}`;
}

// The status bar's text. With the bar hidden (Settings → General), a
// message shows briefly over the photo instead, unless it's `routine`
// (preview sizes and times).
function status(text, routine = false) {
  $("status").textContent = text;
  if (routine || !document.body.classList.contains("status-off") || !text) return;
  const note = $("status-note");
  note.textContent = text;
  note.hidden = false;
  clearTimeout(note.timer);
  note.timer = setTimeout(() => (note.hidden = true), 3500);
}

// Yes or no, in the system's own dialog. (The page's confirm() can't be used:
// the dialog plugin turns it into a call that fails.)
async function askYes(message) {
  // Counted, so a test can tell nothing asks at the first start.
  ui.asked = (ui.asked ?? 0) + 1;
  try {
    return await window.__TAURI__.dialog.ask(message, { title: "Ambrotype", kind: "warning" });
  } catch (e) {
    showError(e);
    return false;
  }
}

// A change to photos' info (rating, label, flag, keywords, …), sent one at
// a time in the order made: each answer carries the photos' whole info, so a
// slower answer landing after a newer one would show older values (two keys
// pressed quickly). Resolves to library_set_info's answer.
let infoWrites = Promise.resolve();
function setInfo(paths, change) {
  const write = infoWrites.then(() => invoke("library_set_info", { paths, change }));
  infoWrites = write.catch(() => {});
  return write;
}

// The page's own storage (localStorage), which can be missing or full:
// reads give `fallback` when there's nothing or it can't be read, and
// failed writes are dropped. get/set store JSON; text/setText plain text.
const store = {
  get(key, fallback = null) {
    try {
      const text = localStorage.getItem(key);
      return text === null ? fallback : JSON.parse(text);
    } catch {
      return fallback;
    }
  },
  set(key, value) {
    try { localStorage.setItem(key, JSON.stringify(value)); } catch {}
  },
  text(key, fallback = null) {
    try { return localStorage.getItem(key) ?? fallback; } catch { return fallback; }
  },
  setText(key, text) {
    try { localStorage.setItem(key, text); } catch {}
  },
  remove(key) {
    try { localStorage.removeItem(key); } catch {}
  },
};

function el(tag, attrs = {}, ...children) {
  const e = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (k === "class") e.className = v;
    else if (k.startsWith("on")) e.addEventListener(k.slice(2), v);
    else if (v === true) e.setAttribute(k, "");
    else if (v !== false && v != null) e.setAttribute(k, v);
  }
  for (const c of children) if (c != null) e.append(c);
  return e;
}

const fileName = (p) => p.replace(/^.*[\\/]/, "");
const dirOf = (p) => p.replace(/[\\/][^\\/]*$/, "");
// A folder's own name, also when the path ends with a slash.
const folderName = (p) => p.split(/[\\/]/).filter(Boolean).pop();

// A scroll bar shows for a moment while its box scrolls (style.css), also
// with the wheel when the pointer isn't over the bar's box any more.
const scrollTimers = new WeakMap();
document.addEventListener("scroll", (e) => {
  const box = e.target === document ? document.documentElement : e.target;
  if (!(box instanceof Element)) return;
  box.classList.add("scrolling");
  clearTimeout(scrollTimers.get(box));
  scrollTimers.set(box, setTimeout(() => box.classList.remove("scrolling"), 800));
}, { capture: true, passive: true });
