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
  if (name === "CameraProfile") return "";
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
  const named = (tag) => ui.defs.get(tag)?.label ?? tag;
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
      act("×", "Delete", () => {
        if (confirm(`Delete the snapshot “${s.name}”? The photo's edit stays as it is.`)) {
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
  if (taken && !confirm(`Replace the snapshot “${name}” with the edit as it is now?`)) return;
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
        run(Array.from({ length: Math.abs(n) }, () => ({ cmd: n < 0 ? "undo" : "redo" })));
      });
    }
    hist.append(li);
  });
  hist.querySelector(".current")?.scrollIntoView({ block: "nearest" });
}

// ---------------------------------------------------------------- state & commands

function setState(state) {
  const crop = cropKey(ui.settings);
  ui.state = state;
  ui.settings = state.settings ?? {};
  const held = ui.active && ui.active in ui.overrides ? { [ui.active]: ui.overrides[ui.active] } : {};
  ui.overrides = held;
  stripUpdateCurrent({ edited: Object.keys(ui.settings).length > 0 });
  syncControls();
  if (ui.crop.on && cropKey(ui.settings) !== crop) reloadCrop();
  masksStateChanged();
  spotsStateChanged();
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

// ---------------------------------------------------------------- preview

function previewSize(draft) {
  const v = $("viewer");
  const px = Math.max(v.clientWidth, v.clientHeight) * (window.devicePixelRatio || 1);
  const full = Math.min(2560, Math.max(512, Math.ceil(px)));
  return draft ? Math.max(512, Math.round(full * 0.6)) : full;
}

function requestPreview(draft) {
  if (!ui.info) return;
  ui.preview.dirty = true;
  ui.preview.draft = draft;
  if (!ui.preview.inFlight) renderLoop();
}

async function renderLoop() {
  ui.preview.inFlight = true;
  $("busy").hidden = false;
  while (ui.preview.dirty) {
    ui.preview.dirty = false;
    const draft = ui.preview.draft;
    const uncropped = ui.crop.on || ui.before;
    try {
      if (ui.zoom.on) {
        // At 100% only committed changes render (a full-size render per drag step is too slow).
        if (!draft) await renderRegion();
        continue;
      }
      const overrides = ui.before ? {} : { ...ui.overrides };
      const maskOverlay = ui.before ? null : maskOverlayIndex();
      // The crop tool draws its rectangle on the whole photo.
      if (ui.crop.on) overrides.HasCrop = false;
      const buf = await invoke("preview", {
        maxEdge: previewSize(draft),
        overrides,
        before: ui.before,
        clipping: ui.clipping,
        maskOverlay,
      });
      const { meta, jpeg } = splitFramed(buf);
      // A draft is smaller than the viewer; keep the image at its settled size
      // so the photo doesn't shrink while dragging.
      const img = $("preview");
      if (draft && !img.hidden && !img.style.width) {
        const r = img.getBoundingClientRect();
        img.style.width = `${r.width}px`;
        img.style.height = `${r.height}px`;
      } else if (!draft) {
        img.style.width = img.style.height = "";
      }
      showJpeg(jpeg);
      ui.preview.uncropped = uncropped;
      ui.preview.overlay = maskOverlay != null;
      drawHistogram(meta.histogram);
      ui.curve?.setHistogram(meta.histogram);
      status(`${meta.width}×${meta.height} preview in ${meta.millis} ms` +
        (meta.gpu ? " on the GPU" : "") +
        (meta.lens_profile ? `  ·  Lens: ${meta.lens_profile}` : ""));
      $("status").title = meta.gpu ? `Developed on ${meta.gpu}` :
        meta.cpu_reason ? `Developed on the CPU: ${meta.cpu_reason}` : "Developed on the CPU (no GPU found)";
    } catch (e) {
      toast(String(e), true);
    }
  }
  $("busy").hidden = true;
  ui.preview.inFlight = false;
}

function drawHistogram(h) {
  ui.histogram = h;
  const c = $("histogram");
  const dpr = window.devicePixelRatio || 1;
  c.width = c.clientWidth * dpr;
  c.height = c.clientHeight * dpr;
  const g = c.getContext("2d");
  g.clearRect(0, 0, c.width, c.height);
  const max = Math.max(1, ...h.flatMap((ch) => ch.slice(2, 254)));
  g.globalCompositeOperation = "lighter";
  const colours = ["rgba(230,70,70,0.75)", "rgba(80,200,90,0.75)", "rgba(80,120,240,0.75)"];
  h.forEach((ch, i) => {
    g.fillStyle = colours[i];
    g.beginPath();
    g.moveTo(0, c.height);
    ch.forEach((n, x) => g.lineTo((x / 255) * c.width, c.height - Math.min(1, n / max) * c.height * 0.95));
    g.lineTo(c.width, c.height);
    g.fill();
  });
  g.globalCompositeOperation = "source-over";
}

// ---------------------------------------------------------------- 100% view, pickers

function splitFramed(buf) {
  const bytes = new Uint8Array(buf);
  const len = new DataView(bytes.buffer, bytes.byteOffset, 4).getUint32(0, true);
  const meta = JSON.parse(new TextDecoder().decode(bytes.subarray(4, 4 + len)));
  return { meta, jpeg: bytes.subarray(4 + len) };
}

function showJpeg(jpeg) {
  const url = URL.createObjectURL(new Blob([jpeg], { type: "image/jpeg" }));
  const img = $("preview");
  img.src = url;
  img.hidden = ui.crop.on;
  if (ui.preview.url) URL.revokeObjectURL(ui.preview.url);
  ui.preview.url = url;
}

// 1:1 crop of the full-size render around (cx, cy); 1.5× the viewport so a
// short pan doesn't show empty space before the next crop arrives.
async function renderRegion() {
  const v = $("viewer");
  const dpr = window.devicePixelRatio || 1;
  status("Rendering 100%…");
  const { meta, jpeg } = splitFramed(await invoke("region", {
    cx: ui.zoom.cx, cy: ui.zoom.cy,
    width: Math.round(v.clientWidth * dpr * 1.5),
    height: Math.round(v.clientHeight * dpr * 1.5),
    before: ui.before,
    clipping: ui.clipping,
  }));
  // 100% was switched off (or another photo opened) while this rendered.
  if (!ui.zoom.on) {
    requestPreview(false);
    return;
  }
  ui.zoom.meta = meta;
  showJpeg(jpeg);
  $("viewer").classList.add("actual");
  placeRegion();
  status(`100%: ${meta.full_width}×${meta.full_height} in ${meta.millis} ms` +
    (Object.keys(ui.overrides).length ? " (slider changes show at 100% when released)" : ""));
}

// Position the 1:1 crop so (cx, cy) sits in the viewport centre.
function placeRegion(dx = 0, dy = 0) {
  const m = ui.zoom.meta;
  if (!m) return;
  const v = $("viewer");
  const dpr = window.devicePixelRatio || 1;
  const img = $("preview");
  img.style.width = `${m.width / dpr}px`;
  img.style.height = `${m.height / dpr}px`;
  const centreX = (ui.zoom.cx * m.full_width - m.x) / dpr;
  const centreY = (ui.zoom.cy * m.full_height - m.y) / dpr;
  img.style.left = `${v.clientWidth / 2 - centreX + dx}px`;
  img.style.top = `${v.clientHeight / 2 - centreY + dy}px`;
}

function setZoom(on, u = 0.5, v = 0.5) {
  ui.zoom.on = on;
  ui.zoom.cx = u;
  ui.zoom.cy = v;
  ui.zoom.meta = null;
  const img = $("preview");
  // The "actual" layout is applied once the 1:1 crop arrives (renderRegion).
  if (!on) $("viewer").classList.remove("actual");
  $("btn-zoom").classList.toggle("active", on);
  if (!on) for (const k of ["width", "height", "left", "top"]) img.style[k] = "";
  requestPreview(false);
}

// Where a click lands, as fractions (u, v) of the upright photo.
function photoPoint(e) {
  const img = $("preview");
  const r = img.getBoundingClientRect();
  const fx = (e.clientX - r.left) / r.width;
  const fy = (e.clientY - r.top) / r.height;
  if (ui.zoom.on && ui.zoom.meta) {
    const m = ui.zoom.meta;
    return [(m.x + fx * m.width) / m.full_width, (m.y + fy * m.height) / m.full_height];
  }
  return [fx, fy];
}

async function pickWhite(u, v) {
  setPicking(false);
  try {
    const r = await invoke("pick_white", { u, v });
    setState(r.state);
    toast(r.notes.join(" "));
    requestPreview(false);
  } catch (e) {
    toast(String(e), true);
  }
}

function setPicking(on) {
  ui.picking = on && Boolean(ui.info);
  $("viewer").classList.toggle("picking", ui.picking);
  $("btn-wb").classList.toggle("active", ui.picking);
  if (ui.picking) status("Click a neutral grey or white spot (Esc to cancel)");
}

function toggleClipping() {
  if (!ui.info) return;
  ui.clipping = !ui.clipping;
  $("btn-clip").classList.toggle("active", ui.clipping);
  requestPreview(false);
}

function wireViewer() {
  const img = $("preview");
  let drag = null;
  img.addEventListener("mousedown", (e) => {
    if (e.button !== 0 || !ui.info) return;
    e.preventDefault();
    drag = { x: e.clientX, y: e.clientY, moved: false };
  });
  window.addEventListener("mousemove", (e) => {
    if (!drag) return;
    const dx = e.clientX - drag.x;
    const dy = e.clientY - drag.y;
    if (Math.abs(dx) + Math.abs(dy) > 3) drag.moved = true;
    if (ui.zoom.on && drag.moved) {
      img.classList.add("panning");
      placeRegion(dx, dy);
    }
  });
  window.addEventListener("mouseup", (e) => {
    if (!drag) return;
    const d = drag;
    drag = null;
    img.classList.remove("panning");
    if (ui.zoom.on && d.moved && ui.zoom.meta) {
      // Move the centre by the drag distance (in full-size pixels) and fetch a new crop.
      const dpr = window.devicePixelRatio || 1;
      const m = ui.zoom.meta;
      ui.zoom.cx = Math.min(1, Math.max(0, ui.zoom.cx - ((e.clientX - d.x) * dpr) / m.full_width));
      ui.zoom.cy = Math.min(1, Math.max(0, ui.zoom.cy - ((e.clientY - d.y) * dpr) / m.full_height));
      requestPreview(false);
      return;
    }
    if (d.moved) return;
    const [u, v] = photoPoint(e);
    if (ui.picking) pickWhite(u, v);
    else setZoom(!ui.zoom.on, u, v);
  });
}

// ---------------------------------------------------------------- crop & straighten

const CROP_TAGS = ["HasCrop", "CropLeft", "CropTop", "CropRight", "CropBottom", "CropAngle"];
const ASPECTS = ["1:1", "3:2", "2:3", "4:5", "4:3", "3:4", "16:9", "9:16"];

function cropKey(settings) {
  return JSON.stringify(CROP_TAGS.map((t) => settings[t] ?? null));
}

// "3:2" for 1.5; "1.91:1" when no small whole-number ratio is close.
function ratioText(a) {
  for (let n = 1; n <= 16; n++) {
    const m = Math.round(a * n);
    if (m > 0 && Math.abs(m / n - a) / a < 0.004) return `${m}:${n}`;
  }
  return `${a.toFixed(2)}:1`;
}

// Width / height of an aspect choice before the swap, or null (free).
function aspectRatio(value) {
  if (value === "original") return ui.info ? ui.info.width / ui.info.height : null;
  if (value.startsWith("size:")) {
    const p = ui.sizes.find((x) => x.id === value.slice(5));
    return p ? p.width / p.height : null;
  }
  const m = value.match(/^(\d+):(\d+)$/);
  return m ? Number(m[1]) / Number(m[2]) : null;
}

function lockedRatio() {
  const r = aspectRatio(ui.crop.aspect);
  return r && ui.crop.swapped ? 1 / r : r;
}

// Keep the aspect lock when the rectangle has it (either way round);
// otherwise Original for an uncropped photo, else Free.
function chooseAspect(rect, enabled) {
  const r = rect.width / rect.height;
  const close = (a, b) => Math.abs(Math.log(a / b)) < 0.005;
  for (const value of [ui.crop.aspect, "original"]) {
    const base = aspectRatio(value);
    if (!base) continue;
    if (close(base, r) || close(1 / base, r)) {
      ui.crop.aspect = value;
      ui.crop.swapped = !close(base, r);
      return;
    }
  }
  ui.crop.aspect = enabled ? "free" : "original";
  ui.crop.swapped = false;
}

function fillSizeSelects() {
  const groups = new Map();
  for (const p of ui.sizes) {
    if (!groups.has(p.group)) groups.set(p.group, []);
    groups.get(p.group).push(p);
  }
  const option = (parent, value, text) => {
    const o = document.createElement("option");
    o.value = value;
    o.textContent = text;
    parent.append(o);
  };
  // Site groups (and yours) after the common ones; the picker shows them collapsed.
  const addSizes = (sel, skip = []) => {
    for (const [group, items] of groups) {
      if (skip.includes(group)) continue;
      const og = document.createElement("optgroup");
      og.label = group || "Yours";
      for (const p of items) option(og, `size:${p.id}`, `${p.name} (${p.width} × ${p.height})`);
      sel.append(og);
    }
  };
  const crop = $("crop-aspect");
  crop.innerHTML = "";
  option(crop, "original", "Original");
  option(crop, "free", "Free");
  const ratios = document.createElement("optgroup");
  ratios.label = "Common ratios";
  for (const r of ASPECTS) option(ratios, r, r.replace(":", " : "));
  crop.append(ratios);
  // The crop's own ratio list covers the common sizes.
  addSizes(crop, ["Common"]);
  if (!aspectRatio(ui.crop.aspect) && ui.crop.aspect !== "free") ui.crop.aspect = "original";
  crop.value = ui.crop.aspect;

  const sel = $("export-form").size_preset;
  const keep = sel.value;
  sel.innerHTML = "";
  option(sel, "", "None");
  option(sel, "custom", "Custom size");
  addSizes(sel);
  sel.value = [...sel.options].some((o) => o.value === keep) ? keep : "";
}

async function loadSizes() {
  try {
    ui.sizes = await invoke("size_presets");
  } catch (e) {
    toast(String(e), true);
  }
  fillSizeSelects();
}

function syncCropPanel(rect) {
  if (!rect) return;
  const a = rect.angle;
  $("crop-angle").value = a;
  $("crop-angle-num").value = (a > 0 ? "+" : "") + a.toFixed(2);
  $("crop-size").textContent = `${Math.round(rect.width)} × ${Math.round(rect.height)} px  ·  ${ratioText(rect.width / rect.height)}`;
  $("crop-aspect").value = ui.crop.aspect;
}

function setCropMode(on) {
  ui.crop.on = on;
  $("crop-canvas").hidden = !on;
  $("crop-panel").hidden = !on;
  $("preview").hidden = on || !ui.preview.url;
  $("btn-crop").classList.toggle("active", on);
  // No 100% view or white-balance picking on the uncropped view.
  for (const id of ["btn-zoom", "btn-wb"]) $(id).disabled = on || !ui.info;
}

async function enterCrop() {
  if (!ui.info || ui.crop.on) return;
  exitMasks();
  exitSpots();
  if (ui.zoom.on) setZoom(false);
  setPicking(false);
  let st;
  try {
    st = await invoke("crop_state");
  } catch (e) {
    toast(String(e), true);
    return;
  }
  ui.crop.start = st.rect;
  ui.crop.enabled = st.enabled;
  ui.crop.angleSign = st.angle_sign;
  chooseAspect(st.rect, st.enabled);
  setCropMode(true);
  // No crop yet: start by drawing one, like a screenshot selection.
  ui.crop.tool.setRatio(lockedRatio());
  ui.crop.tool.start({ width: ui.info.width, height: ui.info.height, angleSign: st.angle_sign, rect: st.rect, draw: !st.enabled });
  syncCropPanel(st.rect);
  requestPreview(false);
}

// The committed crop changed while the tool is open (undo, redo, reset).
async function reloadCrop() {
  try {
    const st = await invoke("crop_state");
    if (!ui.crop.on) return;
    ui.crop.start = st.rect;
    ui.crop.enabled = st.enabled;
    chooseAspect(st.rect, st.enabled);
    ui.crop.tool.setRatio(lockedRatio());
    ui.crop.tool.setRect(st.rect);
  } catch (e) {
    toast(String(e), true);
  }
}

function cropChanged(rect) {
  const s = ui.crop.start;
  if (!s || !rect) return false;
  return ["cx", "cy", "width", "height"].some((k) => Math.abs(rect[k] - s[k]) > 0.25) ||
    Math.abs(rect.angle - s.angle) > 0.005;
}

// Leave the crop tool; with `commit`, its rectangle becomes one undo step.
async function exitCrop(commit) {
  if (!ui.crop.on) return;
  const rect = ui.crop.tool.rect();
  setCropMode(false);
  ui.crop.tool.stop();
  status("Ready");
  if (commit && cropChanged(rect)) {
    try {
      const commands = await invoke("crop_commands", { rect });
      await run(commands, "crop");
      return;
    } catch (e) {
      toast(String(e), true);
    }
  }
  requestPreview(false);
}

function toggleCrop() {
  if (ui.crop.on) exitCrop(true);
  else enterCrop();
}

// Reset: no crop, no straighten (one undo step when there was a crop).
async function resetCrop() {
  if (!ui.crop.on) return;
  const full = { cx: ui.info.width / 2, cy: ui.info.height / 2, width: ui.info.width, height: ui.info.height, angle: 0 };
  ui.crop.aspect = "original";
  ui.crop.swapped = false;
  ui.crop.tool.setRatio(lockedRatio());
  ui.crop.tool.setRect(full);
  if (ui.crop.enabled) {
    try {
      // reloadCrop (via setState) then picks up the cleared crop.
      await run(await invoke("crop_commands", { rect: full }), "reset crop");
    } catch (e) {
      toast(String(e), true);
    }
  } else {
    ui.crop.start = full;
  }
}

function setCropAspect(value) {
  ui.crop.aspect = value;
  ui.crop.swapped = false;
  ui.crop.tool.setRatio(lockedRatio());
  ui.crop.tool.fitRatio();
}

function swapCrop() {
  if (!ui.crop.on) return;
  if (aspectRatio(ui.crop.aspect)) ui.crop.swapped = !ui.crop.swapped;
  ui.crop.tool.swap();
  ui.crop.tool.setRatio(lockedRatio());
}

function wireCrop() {
  const img = $("preview");
  ui.crop.tool = AmbCrop.create($("crop-canvas"), {
    // Only the whole-photo preview, once loaded (not a cropped one still showing).
    image: () => (ui.preview.uncropped && img.complete && img.naturalWidth ? img : null),
    onChange: (rect) => syncCropPanel(rect),
    onDrawMode: (on) => {
      $("crop-draw").classList.toggle("active", on);
      const hint = on
        ? "Press at one corner of the crop and drag to the opposite corner. Or adjust the frame with the controls here."
        : "Drag the edges or corners, inside to move, outside the frame to straighten. Draw ⌖ (D) starts a new one. Enter applies, Esc cancels.";
      $("crop-hint").textContent = hint;
      if (ui.crop.on) status(on ? "Crop: drag from one corner to the other" : "Crop: adjust the frame; Enter applies, Esc cancels");
    },
  });
  $("crop-draw").addEventListener("click", () => ui.crop.tool.setDrawMode(!ui.crop.tool.drawMode()));
  img.addEventListener("load", () => { if (ui.crop.on) ui.crop.tool.draw(); });
  $("btn-crop").addEventListener("click", toggleCrop);
  $("btn-masks").addEventListener("click", toggleMasks);
  $("btn-spots").addEventListener("click", toggleSpots);
  $("crop-done").addEventListener("click", () => exitCrop(true));
  $("crop-cancel").addEventListener("click", () => exitCrop(false));
  $("crop-reset").addEventListener("click", resetCrop);
  $("crop-swap").addEventListener("click", swapCrop);
  $("crop-sizes").addEventListener("click", sizesFlow);
  $("crop-aspect").addEventListener("change", (e) => {
    setCropAspect(e.target.value);
    // Let Enter apply the crop again rather than reopen the list.
    e.target.blur();
  });
  const angle = $("crop-angle");
  angle.addEventListener("input", () => ui.crop.tool.setAngle(Number(angle.value)));
  angle.addEventListener("change", () => ui.crop.tool.endAngle());
  const straight = () => { ui.crop.tool.setAngle(0); ui.crop.tool.endAngle(); };
  angle.addEventListener("dblclick", straight);
  angle.previousElementSibling.addEventListener("dblclick", straight);
  $("crop-angle-num").addEventListener("change", (e) => {
    const v = Number(e.target.value);
    if (Number.isFinite(v)) { ui.crop.tool.setAngle(v); ui.crop.tool.endAngle(); }
  });
}

// ---------------------------------------------------------------- size presets

async function sizesFlow() {
  const dlg = $("sizes-dialog");
  const f = $("sizes-form");
  let editing = "";  // id of the user preset being edited; "" adds a new one
  const fill = (p, id) => {
    editing = id;
    f.name.value = p ? (id ? p.name : `${p.name} (copy)`) : "";
    f.group.value = p ? p.group : "";
    f.width.value = p ? p.width : "";
    f.height.value = p ? p.height : "";
    $("sizes-editing").textContent = id ? `Editing "${p.name}"` : p ? `New size from "${p.name}"` : "New size";
  };
  const list = () => {
    const root = $("sizes-list");
    root.innerHTML = "";
    let group = null;
    for (const p of ui.sizes) {
      if (p.group !== group) {
        group = p.group;
        const h = document.createElement("div");
        h.className = "size-group";
        h.textContent = group + (p.builtin ? "" : " (yours)");
        root.append(h);
      }
      const row = document.createElement("div");
      row.className = "size-row";
      const name = document.createElement("span");
      name.textContent = p.name;
      const dims = document.createElement("span");
      dims.className = "dims";
      dims.textContent = `${p.width} × ${p.height}`;
      const buttons = document.createElement("span");
      buttons.className = "buttons";
      const button = (text, title, action) => {
        const b = document.createElement("button");
        b.type = "button";
        b.className = "small";
        b.textContent = text;
        b.title = title;
        b.addEventListener("click", action);
        buttons.append(b);
      };
      button("Clone", "Start a new size from this one", () => fill(p, ""));
      if (!p.builtin) {
        button("Edit", "Change this size", () => fill(p, p.id));
        button("Delete", "Delete this size", async () => {
          try {
            ui.sizes = await invoke("delete_size_preset", { id: p.id });
            if (editing === p.id) fill(null, "");
            fillSizeSelects();
            list();
          } catch (e) {
            toast(String(e), true);
          }
        });
      }
      row.append(name, dims, buttons);
      root.append(row);
    }
  };
  const save = async () => {
    const preset = {
      id: editing,
      name: f.name.value,
      group: f.group.value,
      width: Number(f.width.value) || 0,
      height: Number(f.height.value) || 0,
    };
    try {
      ui.sizes = await invoke("save_size_preset", { preset });
      fillSizeSelects();
      list();
      toast(`Saved size "${preset.name.trim()}"`);
      fill(null, "");
    } catch (e) {
      toast(String(e), true);
    }
  };
  f.onsubmit = (e) => {
    if (e.submitter?.value === "close") return;
    e.preventDefault();
    save();
  };
  $("sizes-new").onclick = () => fill(null, "");
  list();
  fill(null, "");
  dlg.showModal();
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

// ---------------------------------------------------------------- photo, presets, export

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
    refreshProfiles();
    // Raw and JPEG have different white balance sliders.
    if (wasRendered !== !!r.info.rendered && ui.layout) applyLayout(ui.layout);
    $("empty").hidden = true;
    $("photo-title").textContent = `${r.info.name}  —  ${r.info.camera}${r.info.lens ? "  ·  " + r.info.lens : ""}`;
    $("exif").textContent = [r.info.exposure, `${r.info.width} × ${r.info.height}`,
      r.info.sidecar_loaded ? "edits loaded from sidecar" : ""].filter(Boolean).join("  ·  ");
    for (const id of ["btn-save", "btn-export", "btn-before", "btn-crop", "btn-masks", "btn-spots", "btn-zoom", "btn-wb", "btn-clip", "btn-save-preset", "btn-match"]) $(id).disabled = false;
    if (ui.zoom.on) setZoom(false);
    setState(r.state);
    if (r.saved_previous) toast(`Saved your edits to ${r.saved_previous.replace(/^.*[\\/]/, "")}`);
    applyStripVisibility();
    loadFolder(r.info.path);
    requestPreview(false);
    loadInfo();
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

function renderPresets() {
  const list = $("preset-list");
  const filter = $("preset-filter").value.trim().toLowerCase();
  const shown = ui.presets.filter((p) => !filter || (p.name + " " + p.group).toLowerCase().includes(filter));
  list.innerHTML = "";
  if (!shown.length) {
    list.innerHTML = `<p class="hint">${ui.presets.length ? "No match." : "No presets yet. Lightroom's preset folder is used automatically when it exists; or add a folder."}</p>`;
    return;
  }
  const groups = new Map();
  for (const p of shown) {
    if (!groups.has(p.group)) groups.set(p.group, []);
    groups.get(p.group).push(p);
  }
  for (const [group, items] of groups) {
    const d = document.createElement("details");
    d.className = "preset-group";
    d.open = groups.size < 6 || Boolean(filter);
    const s = document.createElement("summary");
    s.textContent = `${group || "Presets"} (${items.length})`;
    d.append(s);
    for (const p of items) {
      const b = document.createElement("button");
      b.className = "preset";
      b.textContent = p.name;
      b.title = p.path;
      b.addEventListener("click", () =>
        run([{ cmd: "apply_preset", path: p.path, amount: Number($("preset-amount").value) }]));
      d.append(b);
    }
    list.append(d);
  }
}

// The open photo's edit as a one-click preset (src/session.rs `save_preset`).
async function savePresetFlow(name = "") {
  if (!ui.info) return toast("Open a photo first.", true);
  const dlg = $("save-preset-dialog");
  const f = dlg.querySelector("form");
  f.preset_name.value = name;
  dlg.showModal();
  f.preset_name.focus();
  const choice = await new Promise((resolve) => dlg.addEventListener("close", () => resolve(dlg.returnValue), { once: true }));
  if (choice !== "ok") return;
  try {
    ui.presets = await invoke("save_preset", { name: f.preset_name.value, group: f.preset_group.value });
    renderPresets();
    toast(`Saved the preset “${f.preset_name.value.trim()}” (in ${f.preset_group.value.trim() || "Yours"})`);
  } catch (e) {
    toast(String(e), true);
  }
}

// Auto, as Lightroom's: the tone sliders (Basic's Auto button) or the
// white balance (WB: Auto), worked out on a small copy, one undo step.
async function autoFlow(tone, white) {
  if (!ui.info) return;
  status(tone ? "Auto tone…" : "Auto white balance…");
  $("busy").hidden = false;
  try {
    const commands = await invoke("auto_settings", { tone, white });
    if (white && !commands.some((c) => c.name === "Temperature" || c.name === "IncrementalTemperature")) {
      // Too little grey to judge the light by: keep the camera's.
      commands.push({ cmd: "set_param", name: "WhiteBalance", value: "Auto" });
      if (!tone) toast("Too little grey in this photo to judge the light by, so Auto keeps the camera's white balance.");
    }
    await run(commands, tone ? "Auto" : "Auto white balance");
  } catch (e) {
    toast(String(e), true);
  } finally {
    $("busy").hidden = true;
    status("");
  }
}

// Match look: pick reference photos, find settings for their look, show
// them on the photo, and apply them as one undo step.
async function matchFlow() {
  if (!ui.info) return toast("Open a photo first.", true);
  const refs = await invoke("pick_references");
  if (!refs.length) return;
  const dlg = $("match-dialog");
  const names = refs.map(fileName);
  $("match-refs").textContent = `${refs.length} reference${refs.length > 1 ? "s" : ""}: ${names.join(", ")}`;
  const table = $("match-lines");
  table.hidden = true;
  table.replaceChildren();
  $("match-apply").disabled = true;
  $("match-find").disabled = false;
  $("match-status").textContent = "Measures how light, warm and colourful the references are, then finds the settings that give this photo the same. The photo shows the result before you apply it.";
  let proposal = null;
  const preview = (commands) => {
    ui.overrides = {};
    for (const c of commands) ui.overrides[c.name] = c.value;
    requestPreview(false);
  };
  const find = async () => {
    const parts = [...dlg.querySelectorAll(".match-parts input:checked")].map((i) => i.value);
    if (!parts.length) return toast("Choose at least one part to match.", true);
    $("match-find").disabled = true;
    $("match-apply").disabled = true;
    $("match-status").textContent = "Finding settings…";
    try {
      proposal = await invoke("match_look", { references: refs, parts });
      table.replaceChildren(...proposal.lines.map((l) => {
        const tr = document.createElement("tr");
        for (const t of [l.label, l.before, "→", l.after]) {
          const td = document.createElement("td");
          td.textContent = t;
          tr.append(td);
        }
        return tr;
      }));
      table.hidden = !proposal.lines.length;
      const closer = proposal.before > 0 ? Math.round(100 * (1 - proposal.after / proposal.before)) : 0;
      $("match-status").textContent = proposal.lines.length
        ? `${proposal.lines.length} setting${proposal.lines.length > 1 ? "s" : ""}; the look is ${closer}% closer (${proposal.renders} test renders).`
        : "The photo already matches: nothing to change.";
      $("match-apply").disabled = !proposal.commands.length;
      preview(proposal.commands);
    } catch (e) {
      $("match-status").textContent = String(e);
    } finally {
      $("match-find").disabled = false;
    }
  };
  $("match-find").onclick = find;
  dlg.showModal();
  find();
  const choice = await new Promise((resolve) => dlg.addEventListener("close", () => resolve(dlg.returnValue), { once: true }));
  ui.overrides = {};
  if (choice === "ok" && proposal) await run(proposal.commands, "Match look");
  else requestPreview(false);
}

async function loadPresets(add = false) {
  try {
    ui.presets = await invoke(add ? "add_preset_folder" : "presets");
    renderPresets();
  } catch (e) {
    toast(String(e), true);
  }
}

const DEPTHS = { jpeg: [8], png: [8, 16], tiff: [16, 8, 32], webp: [8], avif: [8], exr: [32] };

function updateExportForm() {
  const f = $("export-form");
  const format = f.format.value;
  f.bits.innerHTML = DEPTHS[format].map((b) => `<option value="${b}">${b === 32 ? "32-bit float" : b + "-bit"}</option>`).join("");
  f.quality.closest("label").style.visibility = ["jpeg", "avif"].includes(format) ? "visible" : "hidden";
  f.max_kb.closest("label").style.visibility = format === "jpeg" ? "visible" : "hidden";
  f.color_space.disabled = format === "avif";
  if (format === "avif") f.color_space.value = "srgb";
}

// Exact size: say what will be trimmed when its aspect differs from the crop.
function updateExportSize() {
  // The border preview follows the size.
  queueMicrotask(drawBorderPreview);
  const f = $("export-form");
  const w = Number(f.exact_w.value);
  const h = Number(f.exact_h.value);
  const exact = w > 0 && h > 0;
  f.long_edge.disabled = exact;
  const note = $("export-size-note");
  note.classList.remove("warn");
  if (!exact) {
    note.textContent = f.exact_w.value || f.exact_h.value ? "Give both a width and a height for an exact size." : "";
    return;
  }
  const a = ui.exportCropAspect;
  const t = w / h;
  if (borderOn() && f.b_fit.checked) {
    note.textContent = `Exactly ${w} × ${h} px, with the whole ${ui.crop.enabled ? "crop" : "photo"} inside the frame.`;
  } else if (Math.abs(Math.log(a / t)) > 0.005) {
    note.classList.add("warn");
    note.textContent = `Exactly ${w} × ${h} px. The ${ui.crop.enabled ? "crop" : "photo"} is ${ratioText(a)} ` +
      `and this size is ${ratioText(t)}, so the ${a > t ? "left and right" : "top and bottom"} edges are trimmed (centred). ` +
      "To choose what is kept, crop with this size preset first.";
  } else {
    note.textContent = `Exactly ${w} × ${h} px; the crop already has this shape.`;
  }
}

function pickExportSize() {
  const f = $("export-form");
  const v = f.size_preset.value;
  if (v.startsWith("size:")) {
    const p = ui.sizes.find((x) => x.id === v.slice(5));
    if (p) {
      f.exact_w.value = p.width;
      f.exact_h.value = p.height;
    }
  } else if (v === "") {
    f.exact_w.value = f.exact_h.value = "";
  }
  updateExportSize();
}

// `batch`: many photos from the library ({ paths, aspect, run(options) }),
// each exported with its own edit; otherwise the open photo.
async function exportFlow(batch = null) {
  if (!batch && libraryOn()) return libraryExport();
  if (!batch && !ui.info) return;
  // Export what the crop tool shows.
  if (ui.crop.on) await exitCrop(true);
  const dlg = $("export-dialog");
  updateExportForm();
  dlg.querySelector("h2").textContent = batch ? `Export ${batch.paths.length} photo${batch.paths.length === 1 ? "" : "s"}` : "Export";
  $("export-go").textContent = batch ? "Choose a folder…" : "Export…";
  if (batch) {
    ui.crop.enabled = false;
    ui.exportCropAspect = batch.aspect;
    ui.exportCropSize = [Math.round(3000 * Math.min(1, batch.aspect)), Math.round(3000 / Math.max(1, batch.aspect))];
  } else {
    try {
      const st = await invoke("crop_state");
      ui.crop.enabled = st.enabled;
      ui.exportCropAspect = st.rect.width / st.rect.height;
      ui.exportCropSize = [st.rect.width, st.rect.height];
    } catch {
      ui.exportCropAspect = ui.info.width / ui.info.height;
      ui.exportCropSize = [ui.info.width, ui.info.height];
    }
  }
  if (!ui.borders) await loadBorders();
  fillBorderSelect();
  updateExportSize();
  dlg.showModal();
  const choice = await new Promise((resolve) => dlg.addEventListener("close", () => resolve(dlg.returnValue), { once: true }));
  if (choice !== "ok") return;
  const f = $("export-form");
  const options = {
    format: f.format.value,
    quality: Number(f.quality.value),
    bits: Number(f.bits.value),
    color_space: f.color_space.value,
    long_edge: f.long_edge.value && f.long_edge.disabled === false ? Number(f.long_edge.value) : null,
    exact: Number(f.exact_w.value) > 0 && Number(f.exact_h.value) > 0
      ? [Number(f.exact_w.value), Number(f.exact_h.value)] : null,
    sharpen: f.sharpen.value || null,
    metadata: f.metadata.value,
    max_kb: f.max_kb.value ? Number(f.max_kb.value) : null,
    border: borderOn() ? readBorder() : null,
  };
  if (batch) return batch.run(options);
  const base = ui.info.name.replace(/\.[^.]+$/, "");
  const path = await invoke("pick_export_path", { suggested: base, format: f.format.value });
  if (!path) return;
  status(`Exporting ${path}…`);
  $("busy").hidden = false;
  try {
    const r = await invoke("export", { path, options });
    toast(`Exported ${r.width}×${r.height}, ${(r.bytes / 1e6).toFixed(1)} MB${options.format === "jpeg" ? `, quality ${r.quality}` : ""}`);
    status(`Exported ${r.path}`);
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

async function init() {
  ui.panels = await invoke("editor_schema");
  for (const p of ui.panels) for (const s of p.sections) for (const def of s.controls) ui.defs.set(def.name, def);
  await loadLooks();
  await loadKeybindings();

  $("btn-open").addEventListener("click", openDialog);
  $("btn-library").addEventListener("click", () => toggleLibrary());
  $("btn-open-empty").addEventListener("click", openDialog);
  $("btn-save").addEventListener("click", save);
  $("btn-export").addEventListener("click", exportFlow);
  $("btn-undo").addEventListener("click", () => run([{ cmd: "undo" }]));
  $("snapshot-add").addEventListener("click", addSnapshot);
  $("btn-redo").addEventListener("click", () => run([{ cmd: "redo" }]));
  $("btn-before").addEventListener("click", toggleBefore);
  $("btn-zoom").addEventListener("click", () => setZoom(!ui.zoom.on, ui.zoom.cx, ui.zoom.cy));
  $("btn-wb").addEventListener("click", () => setPicking(!ui.picking));
  $("btn-clip").addEventListener("click", toggleClipping);
  $("btn-feedback").addEventListener("click", feedbackFlow);
  $("btn-collage").addEventListener("click", collageFlow);
  $("btn-plugins").addEventListener("click", () => pluginsFlow().catch((e) => toast(String(e), true)));
  $("btn-looks").addEventListener("click", () => looksFlow().catch((e) => toast(String(e), true)));
  wireEditor();
  wireBorders();
  AmbPicker.enhance($("crop-aspect"), { open: ["Common ratios"] });
  AmbPicker.enhance($("export-form").size_preset, { open: ["Common"] });
  wireInfo();
  wireFilmstrip();
  $("export-form").long_edge.addEventListener("input", drawBorderPreview);
  $("btn-settings").addEventListener("click", () => settingsFlow().catch((e) => toast(String(e), true)));
  wireViewer();
  wireCrop();
  wireMasks();
  wireSpots();
  $("btn-add-presets").addEventListener("click", () => loadPresets(true));
  $("btn-save-preset").addEventListener("click", () => savePresetFlow());
  $("btn-match").addEventListener("click", () => matchFlow().catch((e) => toast(String(e), true)));
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

  // Drag and drop a raw file onto the window.
  tauri.event.listen("tauri://drag-enter", () => $("viewer").classList.add("dragover"));
  tauri.event.listen("tauri://drag-leave", () => $("viewer").classList.remove("dragover"));
  tauri.event.listen("tauri://drag-drop", (e) => {
    $("viewer").classList.remove("dragover");
    const paths = e.payload?.paths ?? [];
    if (paths.length) openAny(paths[0]);
  });
  loadPresets();
  await loadSizes();
  // A photo given on the command line.
  const initial = await invoke("take_initial_path");
  if (initial) openAny(initial);
}

init().catch((e) => toast(String(e), true));
