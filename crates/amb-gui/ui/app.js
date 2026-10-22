// SPDX-License-Identifier: GPL-3.0-or-later
// Ambrotype editor UI. Plain JS, no build step: talks to the Rust side via
// window.__TAURI__.core.invoke. Every edit is a amb-protocol command, so the
// GUI behaves exactly like the CLI and AI clients.
"use strict";

const tauri = window.__TAURI__;
const invoke = (cmd, args) => tauri.core.invoke(cmd, args);
const $ = (id) => document.getElementById(id);

const ui = {
  panels: [],            // editor schema
  defs: new Map(),       // setting name -> its control definition (from the schema)
  historyNames: new Map(), // setting name -> its name in History, where the label alone is ambiguous
  looks: null,           // layouts and themes (src/looks.rs), with the active ids
  layout: null,          // the active layout (layout.js renders it)
  histogram: null,       // the last histogram, redrawn when the theme or layout changes
  keybindings: {},       // shortcuts the user changed (keys.js has the defaults)
  controls: new Map(),   // name -> { def, input, number, label, select, check }
  settings: {},          // committed settings (from the Rust state)
  info: null,            // photo info
  overrides: {},         // values being dragged, not yet committed
  active: null,          // the setting whose slider is held (holdWhileDragging)
  before: false,
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

// ---------------------------------------------------------------- helpers

function toast(text, error = false) {
  const t = $("toast");
  t.textContent = text;
  t.classList.toggle("error", error);
  t.hidden = false;
  clearTimeout(toast.timer);
  toast.timer = setTimeout(() => (t.hidden = true), error ? 6000 : 3000);
}

function status(text) { $("status").textContent = text; }

// Yes or no, in the system's own dialog. (The page's confirm() can't be used:
// the dialog plugin turns it into a call that fails.)
async function askYes(message) {
  try {
    return await window.__TAURI__.dialog.ask(message, { title: "Ambrotype", kind: "warning" });
  } catch (e) {
    toast(String(e), true);
    return false;
  }
}

function decimals(def) { return def.step < 1 ? 2 : 0; }

function format(def, v) {
  if (typeof v !== "number" || Number.isNaN(v)) return "";
  const s = v.toFixed(decimals(def));
  return def.min < 0 && v > 0 ? "+" + s : s;
}

// Temperature uses a log scale on the slider, like Lightroom's.
const isLog = (def) => def.name === "Temperature";
function toSlider(def, v) {
  if (!isLog(def)) return v;
  const t = (Math.log(v) - Math.log(def.min)) / (Math.log(def.max) - Math.log(def.min));
  return Math.round(t * 1000);
}
function fromSlider(def, s) {
  if (!isLog(def)) return Number(s);
  const t = Number(s) / 1000;
  return Math.round(Math.exp(Math.log(def.min) + t * (Math.log(def.max) - Math.log(def.min))) / 50) * 50;
}

function currentValue(name) {
  if (name in ui.overrides) return ui.overrides[name];
  if (name in ui.settings) return ui.settings[name];
  if (name === "CameraProfile" || name === "amb:LUT" || name === "amb:Effects") return "";
  const def = ui.controls.get(name)?.def;
  if (ui.info?.defaults && name in ui.info.defaults) return ui.info.defaults[name];
  if (name === "Temperature") return ui.info ? Math.round(ui.info.as_shot_temperature) : 5500;
  if (name === "Tint") return ui.info ? Math.round(ui.info.as_shot_tint) : 0;
  if (name === "WhiteBalance") return "As Shot";
  return def?.default ?? (def?.kind === "bool" ? false : 0);
}

function asBool(v) {
  return v === true || v === "True" || v === "true" || v === 1 || v === "1";
}

// ---------------------------------------------------------------- panels

const TRACKS = {
  Temperature: "linear-gradient(90deg,#4f7bd1,#d9d2b8,#e3b23c)",
  Tint: "linear-gradient(90deg,#4caf50,#d9d2c8,#c548b6)",
};
TRACKS.IncrementalTemperature = TRACKS.Temperature;
TRACKS.IncrementalTint = TRACKS.Tint;
const BAND_COLOURS = {
  Red: "#e04848", Orange: "#ec8a32", Yellow: "#e6d33a", Green: "#4fbf4f",
  Aqua: "#3fc6c6", Blue: "#4170e0", Purple: "#8c55d9", Magenta: "#d64fb5",
};
const RAINBOW = "linear-gradient(90deg,#e04848,#e6d33a,#4fbf4f,#3fc6c6,#4170e0,#d64fb5,#e04848)";

function trackFor(def) {
  if (TRACKS[def.name]) return TRACKS[def.name];
  if (/Hue$/.test(def.name) && def.name.startsWith("ColorGrade")) return RAINBOW;
  const band = def.name.match(/^(Hue|Saturation|Luminance)Adjustment(\w+)$/);
  if (band) {
    const c = BAND_COLOURS[band[2]];
    if (band[1] === "Saturation") return `linear-gradient(90deg,#808080,${c})`;
    if (band[1] === "Luminance") return `linear-gradient(90deg,#111,${c},#fff)`;
    return `linear-gradient(90deg,${c}aa,${c},${c}aa)`;
  }
  return null;
}

// Double-click resets a slider only when it was double-clicked where its
// value already was (Lightroom: double-click the knob). Two quick clicks that
// move it, to set a value and then nudge it, are not a reset. `position()` is
// the slider's position; `span` its full range.
function resetOnDoubleClick(el, position, span, reset) {
  const downs = [];
  el.addEventListener("pointerdown", () => {
    downs.push(position());
    if (downs.length > 2) downs.shift();
  }, true);
  el.addEventListener("dblclick", () => {
    const before = downs[0];
    if (before !== undefined && Math.abs(position() - before) <= Math.abs(span) * 0.03) reset();
    downs.length = 0;
  });
}

// Dragging a slider: holding Shift (from the start or partway) moves it a
// tenth as fast, for fine changes. The browser's own drag can't change speed
// partway, so this drives the drag and fires the same input and change events.
function fineDrag(input) {
  input.addEventListener("pointerdown", (e) => {
    if (e.button !== 0) return;
    e.preventDefault();
    input.setPointerCapture(e.pointerId);
    const lo = Number(input.min), hi = Number(input.max), span = hi - lo || 1;
    const r = input.getBoundingClientRect();
    const thumb = r.height >= 26 ? 18 : 12; // style.css: the knob's width
    const travel = Math.max(1, r.width - thumb);
    const start = input.value;
    let pos = Number(start);
    const set = (p) => {
      pos = Math.min(hi, Math.max(lo, p));
      const before = input.value;
      input.value = String(pos);
      if (input.value !== before) input.dispatchEvent(new Event("input", { bubbles: true }));
    };
    // As in the browser: pressing the knob grabs it, pressing the track jumps
    // there. With Shift, the value moves on from where it is.
    const knobX = r.left + thumb / 2 + ((pos - lo) / span) * travel;
    if (!e.shiftKey && Math.abs(e.clientX - knobX) > thumb / 2 + 2) {
      set(lo + ((e.clientX - r.left - thumb / 2) / travel) * span);
    }
    let x0 = e.clientX, p0 = pos, rate = e.shiftKey ? 0.1 : 1;
    const onMove = (ev) => {
      const r1 = ev.shiftKey ? 0.1 : 1;
      if (r1 !== rate) { x0 = ev.clientX; p0 = pos; rate = r1; }
      set(p0 + ((ev.clientX - x0) / travel) * span * rate);
    };
    const onUp = () => {
      input.removeEventListener("pointermove", onMove);
      input.removeEventListener("pointerup", onUp);
      input.removeEventListener("pointercancel", onUp);
      if (input.value !== start) input.dispatchEvent(new Event("change", { bubbles: true }));
    };
    input.addEventListener("pointermove", onMove);
    input.addEventListener("pointerup", onUp);
    input.addEventListener("pointercancel", onUp);
  });
}

// While a slider is held, refreshes from finished edits leave it alone
// (otherwise it would jump back to its old value under the pointer).
function holdWhileDragging(el, name) {
  el.addEventListener("pointerdown", () => {
    ui.active = name;
    // Released anywhere (the pointer may have left the slider); after the
    // slider's own change event, which commits the value.
    const release = () => {
      window.removeEventListener("pointerup", release, true);
      window.removeEventListener("pointercancel", release, true);
      setTimeout(() => { if (ui.active === name) ui.active = null; }, 0);
    };
    window.addEventListener("pointerup", release, true);
    window.addEventListener("pointercancel", release, true);
  });
}

// A value box selects its text when clicked into, so typing replaces it.
function selectOnFocus(box) {
  let fresh = false;
  box.addEventListener("focus", () => { box.select(); fresh = true; });
  box.addEventListener("mouseup", (e) => {
    if (fresh) e.preventDefault();
    fresh = false;
  });
}

function makeRow(def) {
  const row = document.createElement("div");
  const entry = { def };
  if (def.kind === "bool") {
    row.className = "row check";
    const label = document.createElement("label");
    const check = document.createElement("input");
    check.type = "checkbox";
    check.addEventListener("change", () => commit(def.name, check.checked));
    label.append(check, document.createTextNode(def.label));
    label.title = def.description;
    row.append(label);
    Object.assign(entry, { check, label });
  } else if (def.kind === "choice") {
    row.className = "row";
    const label = document.createElement("label");
    label.textContent = def.label;
    label.title = def.description;
    const select = document.createElement("select");
    for (const [value, text] of def.options) {
      const o = document.createElement("option");
      o.value = JSON.stringify(value);
      o.textContent = text;
      select.append(o);
    }
    select.addEventListener("change", () => commit(def.name, JSON.parse(select.value)));
    row.append(label, select);
    Object.assign(entry, { select, label });
  } else {
    row.className = "row";
    const label = document.createElement("label");
    label.textContent = def.label;
    label.title = def.description + (def.units ? ` (${def.units})` : "") + "\nDrag the slider (Shift: fine). Double-click to reset.";
    const input = document.createElement("input");
    input.type = "range";
    if (isLog(def)) { input.min = 0; input.max = 1000; input.step = 1; }
    else { input.min = def.min; input.max = def.max; input.step = def.step; }
    const track = trackFor(def);
    if (track) input.style.setProperty("--track", track);
    // A text box (not type=number) so values can show Lightroom's "+0.35" style.
    const number = document.createElement("input");
    number.type = "text";
    number.inputMode = "decimal";
    number.className = "value";
    number.spellcheck = false;
    input.addEventListener("input", () => {
      const v = fromSlider(def, input.value);
      ui.overrides[def.name] = v;
      number.value = format(def, v);
      requestPreview(true);
    });
    input.addEventListener("change", () => commit(def.name, fromSlider(def, input.value)));
    number.addEventListener("change", () => {
      const v = Number(number.value);
      if (!Number.isNaN(v)) commit(def.name, v);
    });
    selectOnFocus(number);
    const reset = () => run([{ cmd: "reset_param", name: def.name }]);
    label.addEventListener("dblclick", reset);
    resetOnDoubleClick(input, () => Number(input.value), input.max - input.min, reset);
    // After resetOnDoubleClick, which notes where the value was before a press.
    // A drag never focuses the slider, so ← → still move between photos (Tab
    // still reaches it for arrow-key changes).
    fineDrag(input);
    holdWhileDragging(input, def.name);
    row.append(label, input, number);
    Object.assign(entry, { input, number, label });
  }
  ui.controls.set(def.name, entry);
  return row;
}

// History entries in words: "set_param Exposure2012" → "Exposure" (the
// slider's label), "add_mask linear to mask 0" → "Mask 1: add linear".
function historyLabel(label) {
  const n = (i) => `Mask ${Number(i) + 1}`;
  const named = (tag) => ui.historyNames.get(tag) ?? ui.defs.get(tag)?.label ?? tag;
  return label
    .replace(/^set_param (\S+)$/, (_, t) => named(t))
    .replace(/^reset_param (\S+)$/, (_, t) => `Reset ${named(t)}`)
    .replace(/^adjust_param (\S+) (\S+)$/, (_, t, d) => `${named(t)} ${d}`)
    .replace(/^apply_preset .*[\\/]/, "preset ")
    .replace(/^add_mask (\w+) to mask (\d+)$/, (_, k, m) => `${n(m)}: add ${k}`)
    .replace(/^add_mask (\w+)$/, (_, k) => `New ${k} mask`)
    .replace(/^set_mask (\d+) part (\d+)$/, (_, m, p) => `${n(m)}: edit part ${Number(p) + 1}`)
    .replace(/^set_mask (\d+)$/, (_, m) => n(m))
    .replace(/^paint_mask new$/, "New brush mask")
    .replace(/^(paint|erase)_mask (\d+)$/, (_, v, m) => `${n(m)}: ${v === "paint" ? "brush" : "erase"}`)
    .replace(/^remove_mask (\d+) part (\d+)$/, (_, m, p) => `${n(m)}: remove part ${Number(p) + 1}`)
    .replace(/^remove_mask (\d+)$/, (_, m) => `Delete ${n(m).toLowerCase()}`)
    .replace(/^duplicate_mask (\d+)$/, (_, m) => `Duplicate ${n(m).toLowerCase()}`)
    .replace(/^move_mask (\d+) to (\d+)$/, (_, m, t) => `Move ${n(m).toLowerCase()} to ${Number(t) + 1}`)
    .replace(/^add_spot (\w+)$/, (_, k) => `${k === "clone" ? "Clone" : "Heal"} spot`)
    .replace(/^set_spot (\d+)$/, (_, s) => `Spot ${Number(s) + 1}`)
    .replace(/^remove_spot (\d+)$/, (_, s) => `Delete spot ${Number(s) + 1}`);
}

function syncControls() {
  for (const [name, c] of ui.controls) {
    if (name === ui.active) continue;
    const v = currentValue(name);
    const changed = name in ui.settings;
    if (c.label) c.label.classList.toggle("changed", changed);
    if (c.set) c.set(v);
    else if (c.check) c.check.checked = asBool(v);
    else if (c.select && name === "CameraProfile") {
      fillProfileOptions(c.select, v);
      c.select.value = JSON.stringify(v ?? "");
    } else if (c.select && name === "amb:LUT") {
      fillLutOptions(c.select, v);
      c.select.value = JSON.stringify(v ?? "");
    } else if (c.select) c.select.value = JSON.stringify(typeof v === "string" && /^\d+$/.test(v) ? Number(v) : v);
    else if (c.input) {
      const n = Number(v);
      c.input.value = toSlider(c.def, n);
      c.number.value = format(c.def, n);
    }
  }
  $("btn-undo").disabled = !ui.state?.can_undo;
  $("btn-redo").disabled = !ui.state?.can_redo;
  renderSnapshots();
  renderHistory();
  if (ui.curve) for (const tag of AmbCurve.TAGS) ui.curve.set(tag, currentValue(tag));
  if (ui.wheels) ui.wheels.set(Object.fromEntries(AmbWheels.TAGS.map((t) => [t, currentValue(t)])));
  const nr = ui.state?.not_rendered ?? [];
  $("status-right").textContent = nr.length ? `Not rendered yet: ${nr.length} setting(s)` : "";
  $("status-right").title = nr.join(", ");
  syncMenu();
  glowUnused();
}

// Snapshots: named copies of the edit, kept in the photo's sidecar. Click
// one to apply it (one undoable step); hover for update, rename and delete.
// The one matching the current settings is bold.
function renderSnapshots() {
  const list = $("snapshot-list");
  list.innerHTML = "";
  $("snapshot-add").disabled = !ui.state;
  for (const s of ui.state?.snapshots ?? []) {
    const li = document.createElement("li");
    li.classList.toggle("current", s.current);
    const name = document.createElement("span");
    name.className = "snap-name";
    name.textContent = s.name;
    name.title = s.current ? "The edit is this snapshot" : "Apply this snapshot (undoable)";
    name.addEventListener("click", () => {
      if (!s.current) run([{ cmd: "apply_snapshot", name: s.name }], `Snapshot: ${s.name}`);
    });
    const act = (text, title, fn) => {
      const b = document.createElement("button");
      b.type = "button";
      b.className = "snap-act";
      b.textContent = text;
      b.title = title;
      b.addEventListener("click", fn);
      return b;
    };
    li.append(
      name,
      act("↻", "Update with the current settings", () => run([{ cmd: "save_snapshot", name: s.name }])),
      act("✎", "Rename", async () => {
        const to = await askName("Rename snapshot", "", s.name);
        if (to && to !== s.name) run([{ cmd: "rename_snapshot", name: s.name, to }]);
      }),
      act("×", "Delete", async () => {
        if (await askYes(`Delete the snapshot “${s.name}”? The photo's edit stays as it is.`)) {
          run([{ cmd: "delete_snapshot", name: s.name }]);
        }
      }),
    );
    list.append(li);
  }
}

async function addSnapshot() {
  if (!ui.state) return;
  const d = new Date();
  const pad = (n) => String(n).padStart(2, "0");
  const stamp = `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
  const name = await askName("New snapshot", "The edit as it is now, under a name, to come back to. Kept in the photo's .xmp.", stamp);
  if (!name) return;
  const taken = (ui.state.snapshots ?? []).some((s) => s.name === name);
  if (taken && !await askYes(`Replace the snapshot “${name}” with the edit as it is now?`)) return;
  run([{ cmd: "save_snapshot", name }]);
}

// The History panel: Original, the steps done, then the steps undone (greyed)
// until a new edit replaces them. Clicking one goes back or forward to just
// after it, as one batch of undos or redos (so it is undoable too). History
// is kept in the sidecar; a very long one keeps its newest steps, and then
// starts at "Earlier edits".
function renderHistory() {
  const hist = $("history-list");
  hist.innerHTML = "";
  if (!ui.state) return;
  const done = ui.state.history ?? [];
  const undone = ui.state.redo_history ?? [];
  const first = ui.state.history_trimmed ? "Earlier edits" : "Original";
  const steps = [first, ...done.map(historyLabel), ...undone.map(historyLabel)];
  hist.start = 0;
  steps.forEach((text, i) => {
    const li = document.createElement("li");
    li.textContent = text;
    li.classList.toggle("current", i === done.length);
    li.classList.toggle("undone", i > done.length);
    if (i !== done.length) {
      li.title = i < done.length ? "Go back to this step" : "Go forward to this step";
      li.addEventListener("click", () => {
        const n = i - done.length;
        teach.fromHistory = true;
        run(Array.from({ length: Math.abs(n) }, () => ({ cmd: n < 0 ? "undo" : "redo" })));
      });
    }
    hist.append(li);
  });
  // Keep the current step in view inside the list only: scrollIntoView would
  // also scroll the column, and the panels above History with it.
  const cur = hist.querySelector(".current");
  if (cur) {
    const top = cur.offsetTop - hist.offsetTop;
    if (top < hist.scrollTop) hist.scrollTop = top;
    else if (top + cur.offsetHeight > hist.scrollTop + hist.clientHeight) hist.scrollTop = top + cur.offsetHeight - hist.clientHeight;
  }
}

// ---------------------------------------------------------------- state & commands

function setState(state) {
  const crop = cropKey(ui.settings);
  const prev = ui.state;
  const shown = teachShown();
  ui.state = state;
  ui.settings = state.settings ?? {};
  const held = ui.active && ui.active in ui.overrides ? { [ui.active]: ui.overrides[ui.active] } : {};
  ui.overrides = held;
  stripUpdateCurrent({ edited: Object.keys(ui.settings).length > 0 });
  syncControls();
  if (ui.crop.on && cropKey(ui.settings) !== crop) reloadCrop();
  masksStateChanged();
  spotsStateChanged();
  learnStateChanged();
  teachStateChanged(prev, shown);
}

// Another app or an AI agent (over MCP) wrote the open photo's sidecar:
// take its edits as one undo step, and its rating and keywords.
async function sidecarChanged() {
  if (!ui.info) return;
  try {
    const r = await invoke("reload_sidecar");
    if (!r) return;
    setState(r.state);
    requestPreview(false);
    loadInfo();
    if (r.changes) toast("The edits were changed outside Ambrotype and are shown now (Undo goes back).");
  } catch (e) {
    toast(`Couldn't read the changed sidecar: ${e}`, true);
  }
}

// ---------------------------------------------------------------- the selected slider

// Lightroom's , and . select the previous or next slider on screen, and
// + and - move the selected one (here = and -, Shift for ten steps).
function slidersOnScreen() {
  const out = [];
  for (const [name, c] of ui.controls) {
    const kind = c.def?.kind;
    if (kind === "bool" || kind === "choice" || !c.label?.isConnected || c.label.offsetParent === null) continue;
    out.push([name, c]);
  }
  return out.sort(([, a], [, b]) => (a.label.compareDocumentPosition(b.label) & Node.DOCUMENT_POSITION_FOLLOWING ? -1 : 1));
}

function showSelectedSlider() {
  for (const n of document.querySelectorAll(".selected-setting")) n.classList.remove("selected-setting");
  const c = ui.controls.get(ui.selectedSlider);
  const row = c?.label?.closest(".row, .knob, .bar-row") ?? c?.label;
  if (!row) return;
  row.classList.add("selected-setting");
  row.scrollIntoView({ block: "nearest" });
}

function stepSlider(dir) {
  const list = slidersOnScreen();
  if (!list.length) return;
  const at = list.findIndex(([name]) => name === ui.selectedSlider);
  const next = at < 0 ? (dir > 0 ? 0 : list.length - 1) : (at + dir + list.length) % list.length;
  ui.selectedSlider = list[next][0];
  showSelectedSlider();
  status(`${list[next][1].def.label} selected: = and - change it`);
}

function nudgeSlider(steps) {
  const c = ui.controls.get(ui.selectedSlider);
  if (!c?.label?.isConnected) return stepSlider(1);
  const def = c.def;
  const step = Number(def.step) > 0 ? Number(def.step) : 1;
  const now = Number(currentValue(def.name)) || 0;
  const v = Math.min(def.max ?? Infinity, Math.max(def.min ?? -Infinity, now + steps * step));
  commit(def.name, Number(v.toFixed(4)));
  showSelectedSlider();
}

// Run protocol commands; several commands with a `group` label are one undo step.
async function run(commands, group = null) {
  if (!ui.info) return;
  try {
    const r = await invoke("execute", { commands, group });
    setState(r.state);
    if (r.notes.length) toast(r.notes.join(" · "));
    requestPreview(false);
  } catch (e) {
    toast(String(e), true);
    ui.overrides = {};
    syncControls();
  }
}

// Picking an Upright mode clears the manual Transform sliders and the crop,
// as Lightroom does; Alt held keeps the crop.
const TRANSFORM_SLIDERS = ["PerspectiveVertical", "PerspectiveHorizontal", "PerspectiveRotate",
  "PerspectiveAspect", "PerspectiveScale", "PerspectiveX", "PerspectiveY"];
// Read from the click that opens the menu: the native menu takes the focus,
// so the page may not see Alt being released.
let altHeld = false;
for (const type of ["keydown", "keyup", "pointerdown"]) window.addEventListener(type, (e) => { altHeld = e.altKey; }, true);

function commit(name, value) {
  delete ui.overrides[name];
  if (name === "WhiteBalance" && value === "Auto") return autoFlow(false, true);
  // The default profile is no profile at all.
  if (name === "CameraProfile" && value === "") return run([{ cmd: "reset_param", name }]);
  if (name === "amb:LUT") return commitLut(value);
  if (name === "PerspectiveUpright" && value !== 0) {
    const cleared = TRANSFORM_SLIDERS.concat(altHeld ? [] : CROP_TAGS).filter((n) => n in ui.settings);
    if (cleared.length) {
      return run([{ cmd: "set_param", name, value }, ...cleared.map((n) => ({ cmd: "reset_param", name: n }))], "Upright");
    }
  }
  run([{ cmd: "set_param", name, value }]);
}

// The Profile menu: our default look, then the camera profiles (.dcp) found
// on this computer for the photo's camera, and the edit's own if it's
// missing here.
async function refreshProfiles(tries = 30) {
  let found = [];
  if (ui.info) {
    try {
      found = await invoke("camera_profiles");
    } catch {
      found = [];
    }
  }
  // The profile folders are read in the background at start-up.
  if (found === null) {
    if (tries > 0) setTimeout(() => refreshProfiles(tries - 1), 1000);
    found = [];
  }
  ui.profiles = found;
  syncControls();
}

// The LUT menu: none, the .cube files in the LUT folder, the edit's own if
// it's elsewhere, then choosing a file or opening the folder.
const LUT_PICK = "\u0000pick";
const LUT_FOLDER = "\u0000folder";
async function refreshLuts() {
  try {
    ui.luts = await invoke("luts_list");
  } catch {
    ui.luts = [];
  }
  syncControls();
}

function fillLutOptions(select, current) {
  const luts = ui.luts ?? [];
  const options = [["", "None"], ...luts.map((l) => [l.path, l.name])];
  if (current && !luts.some((l) => l.path === current)) options.push([current, fileName(String(current)).replace(/\.cube$/i, "")]);
  options.push([LUT_PICK, "Choose a .cube file…"], [LUT_FOLDER, "Open the LUT folder…"]);
  const want = options.map(([v, t]) => `${v}\u0001${t}`).join("\n");
  if (select.dataset.options !== want) {
    select.replaceChildren(...options.map(([v, t]) => Object.assign(document.createElement("option"), { value: JSON.stringify(v), textContent: t })));
    select.dataset.options = want;
  }
}

async function commitLut(value) {
  if (value === LUT_FOLDER) {
    syncControls();
    await invoke("open_luts_folder").catch((e) => toast(String(e), true));
    return;
  }
  if (value === LUT_PICK) {
    const path = await invoke("pick_lut");
    syncControls();
    if (!path) return;
    value = path;
  }
  if (!value) return run([{ cmd: "reset_param", name: "amb:LUT" }]);
  run([{ cmd: "set_param", name: "amb:LUT", value }]);
}

function fillProfileOptions(select, current) {
  const names = [...(ui.profiles ?? [])];
  const options = [["", "Default (Ambrotype)"], ...names.map((n) => [n, n])];
  if (current && !names.some((n) => n.toLowerCase() === String(current).toLowerCase())) options.push([current, `${current} (not found)`]);
  const want = options.map(([v, t]) => `${v}\u0000${t}`).join("\n");
  if (select.dataset.options !== want) {
    select.replaceChildren(...options.map(([v, t]) => Object.assign(document.createElement("option"), { value: JSON.stringify(v), textContent: t })));
    select.dataset.options = want;
  }
}

// ---------------------------------------------------------------- feedback

async function feedbackFlow() {
  const dlg = $("feedback-dialog");
  const f = $("feedback-form");
  const refresh = async () => {
    f.preview.value = await invoke("feedback_body", {
      kind: f.kind.value, text: f.text.value, includeContext: f.context.checked,
    });
  };
  f.oninput = refresh;
  f.onchange = refresh;
  await refresh();
  dlg.showModal();
  const choice = await new Promise((resolve) => dlg.addEventListener("close", () => resolve(dlg.returnValue), { once: true }));
  if (choice !== "ok") return;
  try {
    await invoke("open_feedback", {
      kind: f.kind.value,
      title: f.title.value || f.kind.value,
      body: f.preview.value,
    });
    toast("Opened GitHub in your browser. Thank you!");
  } catch (e) {
    toast(String(e), true);
  }
}

// ---------------------------------------------------------------- photo

async function openPath(path) {
  status(`Opening ${path}…`);
  $("busy").hidden = false;
  try {
    const r = await invoke("open_photo", { path });
    if (ui.crop.on) {
      setCropMode(false);
      ui.crop.tool.stop();
    }
    masksPhotoChanged();
    spotsPhotoChanged();
    const wasRendered = !!ui.info?.rendered;
    ui.info = r.info;
    framePhotoChanged();
    refreshProfiles();
    refreshLuts();
    // Raw and JPEG have different white balance sliders.
    if (wasRendered !== !!r.info.rendered && ui.layout) applyLayout(ui.layout);
    $("empty").hidden = true;
    $("photo-title").textContent = `${r.info.name}  —  ${r.info.camera}${r.info.lens ? "  ·  " + r.info.lens : ""}`;
    $("exif").textContent = [r.info.exposure, `${r.info.width} × ${r.info.height}`,
      r.info.sidecar_loaded ? "edits loaded from sidecar" : ""].filter(Boolean).join("  ·  ");
    for (const id of ["btn-save", "btn-export", "btn-before", "btn-crop", "btn-masks", "btn-spots", "btn-zoom", "btn-wb", "btn-clip", "btn-frame", "btn-save-preset", "btn-match", "btn-surprise"]) $(id).disabled = false;
    if (ui.zoom.on) setZoom(false);
    setState(r.state);
    if (r.saved_previous) toast(`Saved your edits to ${r.saved_previous.replace(/^.*[\\/]/, "")}`);
    applyStripVisibility();
    loadFolder(r.info.path);
    requestPreview(false);
    loadInfo();
    offerLensDb();
    return true;
  } catch (e) {
    toast(String(e), true);
    status("Ready");
    return false;
  } finally {
    $("busy").hidden = true;
  }
}

// A photo, or a saved collage (.ambcollage).
function openAny(path) {
  return /\.ambcollage$/i.test(path) ? openCollageFile(path) : openPath(path);
}

async function openDialog() {
  const path = await invoke("pick_photo");
  if (path) openPath(path);
}

// A small right-click menu: items are [label, action] (null for a line).
function contextMenu(e, items) {
  e.preventDefault();
  document.querySelector(".ctx-menu")?.remove();
  const menu = Object.assign(document.createElement("div"), { className: "ctx-menu", role: "menu" });
  // The button's tooltip would cover the menu.
  const owner = e.currentTarget instanceof Element ? e.currentTarget : null;
  const title = owner?.getAttribute("title");
  if (title) owner.removeAttribute("title");
  const close = () => {
    if (title) owner.setAttribute("title", title);
    menu.remove();
    removeEventListener("pointerdown", outside, true);
    removeEventListener("keydown", esc, true);
  };
  const outside = (ev) => { if (!menu.contains(ev.target)) close(); };
  const esc = (ev) => { if (ev.key === "Escape") { ev.stopPropagation(); close(); } };
  for (const item of items) {
    if (!item) { menu.append(document.createElement("hr")); continue; }
    const [label, action] = item;
    const b = Object.assign(document.createElement("button"), { type: "button", textContent: label, role: "menuitem" });
    b.addEventListener("click", () => { close(); Promise.resolve(action()).catch((err) => toast(String(err), true)); });
    menu.append(b);
  }
  document.body.append(menu);
  const r = menu.getBoundingClientRect();
  menu.style.left = `${Math.min(e.clientX, innerWidth - r.width - 4)}px`;
  menu.style.top = `${Math.min(e.clientY, innerHeight - r.height - 4)}px`;
  addEventListener("pointerdown", outside, true);
  addEventListener("keydown", esc, true);
}

// The photos in the open photo's folder (the filmstrip's list).
function folderPhotos() {
  return (strip.photos ?? []).map((p) => p.path);
}

// Apply a preset to every photo in the folder (the open one as one undo step).
async function freezeNegative() {
  if (!ui.info) return;
  try {
    run(await invoke("negative_freeze"), "Freeze negative analysis");
  } catch (e) {
    toast(String(e), true);
  }
}

async function cropToFrame() {
  if (!ui.info) return;
  try {
    const cmds = await invoke("negative_frame");
    if (!cmds) return toast("No film border found around the photo.");
    run(cmds, "Crop to frame");
  } catch (e) {
    toast(String(e), true);
  }
}

// Once, when no lens profiles are found at all: offer the download.
let lensDbOffered = false;
async function offerLensDb() {
  if (lensDbOffered) return;
  lensDbOffered = true;
  try {
    if (localStorage.getItem("lensDbOffered")) return;
  } catch {
    // No storage: asked once per session.
  }
  // The profiles load in the background at start.
  let st = null;
  for (let i = 0; i < 30; i++) {
    try {
      st = await invoke("lens_db_state");
    } catch {
      return;
    }
    if (st.lenses !== null) break;
    await new Promise((r) => setTimeout(r, 1000));
  }
  if (!st || st.lenses !== 0) return;
  try {
    localStorage.setItem("lensDbOffered", "1");
  } catch {
    // Asked once per session at worst.
  }
  if (await askYes("No lens profiles were found, so Lens Corrections can't fix distortion or vignetting yet. Download lensfun's lens database now (about 0.5 MB)? You can do it later under Lens Corrections → Lens database…")) {
    lensDbFetch();
  }
}

// lensfun's database: download it (or update it) and load it.
async function lensDbFlow() {
  let st;
  try {
    st = await invoke("lens_db_state");
  } catch (e) {
    return toast(String(e), true);
  }
  const when = st.timestamp ? new Date(st.timestamp * 1000).toLocaleDateString() : null;
  const have = st.lenses === null ? "The lens profiles are still loading." : `${st.lenses} lens profiles are loaded.`;
  const ask = when
    ? `${have} The downloaded database is from ${when}. Check for a newer one?`
    : `${have} Download lensfun's lens database (about 0.5 MB, CC BY-SA 3.0) into ${st.dir}?`;
  if (!await askYes(ask)) return;
  lensDbFetch();
}

async function lensDbFetch() {
  status("Downloading the lens database…");
  $("busy").hidden = false;
  try {
    const r = await invoke("fetch_lens_db");
    toast(`Lens database ready: ${r.lenses ?? 0} lens profiles`);
    status("Lens database ready");
    requestPreview(false);
  } catch (e) {
    toast(String(e), true);
  } finally {
    $("busy").hidden = true;
  }
}

async function save() {
  if (!ui.info) return;
  try {
    const path = await invoke("save_sidecar");
    toast(`Saved to ${path}`);
  } catch (e) {
    toast(String(e), true);
  }
}

function toggleBefore() {
  if (!ui.info) return;
  ui.before = !ui.before;
  $("before-badge").hidden = !ui.before;
  $("btn-before").classList.toggle("active", ui.before);
  requestPreview(false);
}

// ---------------------------------------------------------------- wiring

// A button whose handler fails says so, rather than seeming to do nothing.
window.addEventListener("unhandledrejection", (e) => {
  console.error(e.reason);
  toast(`Something went wrong: ${e.reason?.message ?? e.reason}`, true);
});
window.addEventListener("error", (e) => {
  toast(`Something went wrong: ${e.message}`, true);
});

async function init() {
  applyToolbarIcons();
  ui.panels = await invoke("editor_schema");
  for (const p of ui.panels) for (const s of p.sections) for (const def of s.controls) ui.defs.set(def.name, def);
  // A label several settings share ("Amount") gets its section in History.
  const seen = new Map();
  for (const p of ui.panels) for (const s of p.sections) for (const def of s.controls) seen.set(def.label, (seen.get(def.label) ?? 0) + 1);
  for (const p of ui.panels) for (const s of p.sections) for (const def of s.controls) {
    if (seen.get(def.label) > 1 && s.label) ui.historyNames.set(def.name, `${s.label}: ${def.label}`);
  }
  await loadLooks();
  await loadKeybindings();

  $("btn-open").addEventListener("click", openDialog);
  $("btn-library").addEventListener("click", () => toggleLibrary());
  $("btn-open-empty").addEventListener("click", openDialog);
  $("btn-sample-empty").addEventListener("click", () => sampleFlow());
  initLearn();
  wireKeyBadges();
  $("btn-save").addEventListener("click", save);
  // Not the click event as an argument: exportFlow(batch) would take it for a batch.
  $("btn-export").addEventListener("click", () => exportFlow());
  $("btn-undo").addEventListener("click", () => run([{ cmd: "undo" }]));
  $("snapshot-add").addEventListener("click", addSnapshot);
  $("btn-redo").addEventListener("click", () => run([{ cmd: "redo" }]));
  $("btn-before").addEventListener("click", toggleBefore);
  $("btn-zoom").addEventListener("click", () => setZoom(!ui.zoom.on, ui.zoom.cx, ui.zoom.cy));
  $("btn-wb").addEventListener("click", () => setPicking(!ui.picking));
  $("btn-clip").addEventListener("click", toggleClipping);
  $("proof-space").addEventListener("change", applyProof);
  $("proof-warn").addEventListener("change", applyProof);
  $("btn-feedback").addEventListener("click", feedbackFlow);
  $("btn-collage").addEventListener("click", collageFlow);
  $("btn-hub").addEventListener("click", () => browseFlow().catch((e) => toast(String(e), true)));
  $("btn-plugins").addEventListener("click", () => pluginsFlow().catch((e) => toast(String(e), true)));
  $("btn-looks").addEventListener("click", () => looksFlow().catch((e) => toast(String(e), true)));
  wireEditor();
  wireBorders();
  AmbPicker.enhance($("crop-aspect"), { open: ["Common ratios"] });
  AmbPicker.enhance($("export-form").size_preset, { open: ["Common"] });
  wireInfo();
  wireFilmstrip();
  for (const n of ["resize_mode", "resize_value"]) $("export-form")[n].addEventListener("input", drawBorderPreview);
  wireExportPresets();
  wireWatermark();
  $("btn-settings").addEventListener("click", () => settingsFlow().catch((e) => toast(String(e), true)));
  wireViewer();
  wireCrop();
  wireMasks();
  wireSpots();
  wireGuided();
  $("btn-add-presets").addEventListener("click", () => loadPresets(true));
  wirePresetLibrary();
  wireFrame();
  wireQuickTips();
  $("fold-left").addEventListener("click", () => toggleColumn("left"));
  const panels = new MutationObserver(() => { applySideCollapse(); if (ui.info) requestPreview(false); });
  document.querySelectorAll(".viewer > .preset-library").forEach((p) => panels.observe(p, { attributes: true, attributeFilter: ["hidden"] }));
  $("fold-right").addEventListener("click", () => toggleColumn("right"));
  $("btn-keys").addEventListener("click", showShortcuts);
  $("btn-save-preset").addEventListener("click", () => savePresetFlow());
  $("btn-match").addEventListener("click", () => matchFlow().catch((e) => toast(String(e), true)));
  $("btn-assistant").addEventListener("click", () => assistantFlow().catch((e) => toast(String(e), true)));
  $("btn-surprise").addEventListener("click", () => surpriseFlow().catch((e) => toast(String(e), true)));
  // Enter in a dialog's text box means its main button, not the first one (Cancel).
  for (const form of document.querySelectorAll('dialog form[method="dialog"]')) {
    form.addEventListener("keydown", (e) => {
      const ok = form.querySelector("button.primary");
      if (e.key !== "Enter" || !ok || e.target.tagName !== "INPUT" || ["checkbox", "radio"].includes(e.target.type)) return;
      e.preventDefault();
      form.requestSubmit(ok);
    });
  }
  $("preset-filter").addEventListener("input", renderPresets);
  $("preset-amount").addEventListener("input", (e) => ($("preset-amount-out").textContent = e.target.value));
  const f = $("export-form");
  f.format.addEventListener("change", updateExportForm);
  f.quality.addEventListener("input", () => (f.quality_out.value = f.quality.value));
  f.size_preset.addEventListener("change", pickExportSize);
  for (const input of [f.exact_w, f.exact_h]) {
    input.addEventListener("input", () => {
      // Typed sizes are a custom size unless they still match the preset.
      const p = ui.sizes.find((x) => `size:${x.id}` === f.size_preset.value);
      if (!p || p.width !== Number(f.exact_w.value) || p.height !== Number(f.exact_h.value)) {
        f.size_preset.value = f.exact_w.value || f.exact_h.value ? "custom" : "";
      }
      updateExportSize();
    });
  }
  $("export-swap").addEventListener("click", () => {
    [f.exact_w.value, f.exact_h.value] = [f.exact_h.value, f.exact_w.value];
    f.size_preset.value = f.exact_w.value || f.exact_h.value ? "custom" : "";
    updateExportSize();
  });

  // Keyboard shortcuts come from one table (keys.js), which the user can change in Settings.
  document.addEventListener("keydown", dispatchKey);

  let resizeTimer;
  window.addEventListener("resize", () => {
    clearTimeout(resizeTimer);
    resizeTimer = setTimeout(() => requestPreview(false), 250);
  });

  wireMenu();
  wireFolderTree();
  wireLibraryMenus();
  wireSearch();
  wirePresetOriginal();
  wireTasks();
  wireInterfaces();
  wireConnect();
  // Drag and drop a raw file onto the window.
  tauri.event.listen("folder-changed", () => folderChanged());
  tauri.event.listen("sidecar-changed", () => sidecarChanged());
  // A folder (no file extension) opens in the library instead.
  const dropZone = () => $(libraryOn() ? "library-empty" : "viewer");
  tauri.event.listen("tauri://drag-enter", () => dropZone().classList.add("dragover"));
  tauri.event.listen("tauri://drag-leave", () => dropZone().classList.remove("dragover"));
  tauri.event.listen("tauri://drag-drop", async (e) => {
    dropZone().classList.remove("dragover");
    const path = e.payload?.paths?.[0];
    if (!path) return;
    if (/\.[a-z0-9]{2,12}$/i.test(path)) return openAny(path);
    if (!libraryOn()) await switchTask("library");
    loadLibrary(path, []);
  });
  loadPresets();
  await loadSizes();
  // A photo given on the command line.
  const initial = await invoke("take_initial_path");
  if (initial) openAny(initial);
}

init().catch((e) => toast(String(e), true));
