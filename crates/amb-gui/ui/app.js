// Ambrotype editor UI. Plain JS, no build step: talks to the Rust side via
// window.__TAURI__.core.invoke. Every edit is a amb-protocol command, so the
// GUI behaves exactly like the CLI and AI clients.
"use strict";

const tauri = window.__TAURI__;
const invoke = (cmd, args) => tauri.core.invoke(cmd, args);
const $ = (id) => document.getElementById(id);

const ui = {
  panels: [],            // editor schema
  controls: new Map(),   // name -> { def, input, number, label, select, check }
  settings: {},          // committed settings (from the Rust state)
  info: null,            // photo info
  overrides: {},         // values being dragged, not yet committed
  before: false,
  hslTab: "Hue",
  preview: { inFlight: false, dirty: false, draft: false, url: null },
  presets: [],
  zoom: { on: false, cx: 0.5, cy: 0.5, meta: null, pan: null },
  picking: false,
  clipping: false,
  curve: null,
  wheels: null,
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
  const def = ui.controls.get(name)?.def;
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
    label.title = def.description + (def.units ? ` (${def.units})` : "") + "\nDouble-click to reset.";
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
    const reset = () => run([{ cmd: "reset_param", name: def.name }]);
    label.addEventListener("dblclick", reset);
    input.addEventListener("dblclick", reset);
    row.append(label, input, number);
    Object.assign(entry, { input, number, label });
  }
  ui.controls.set(def.name, entry);
  return row;
}

function buildPanels() {
  const root = $("panels");
  ui.curve?.destroy();
  ui.wheels?.destroy();
  ui.curve = ui.wheels = null;
  root.innerHTML = "";
  ui.controls.clear();
  const isGradeWheel = (s) => ["Shadow", "Midtone", "Highlight", "Global"].includes(s.label);
  for (const panel of ui.panels) {
    const details = document.createElement("details");
    details.className = "panel";
    details.open = ["basic"].includes(panel.id);
    const summary = document.createElement("summary");
    const title = document.createElement("span");
    title.textContent = panel.label;
    const reset = document.createElement("button");
    reset.className = "small reset";
    reset.textContent = "Reset";
    reset.title = `Reset ${panel.label}`;
    reset.addEventListener("click", (e) => {
      e.preventDefault();
      run(panel.groups.map((g) => ({ cmd: "reset_group", group: g })), `reset ${panel.label}`);
    });
    summary.append(title, reset);
    const body = document.createElement("div");
    body.className = "panel-body";
    if (panel.id === "tone_curve") {
      const host = document.createElement("div");
      body.append(host);
      ui.curve = AmbCurve.create(host, {
        onInput(tag, points) { ui.overrides[tag] = points; requestPreview(true); },
        onCommit(tag, points) {
          delete ui.overrides[tag];
          const channel = tag.replace("ToneCurvePV2012", "").toLowerCase();
          run([{ cmd: "set_param", name: tag, value: points }], channel ? `tone curve (${channel})` : "tone curve");
        },
      });
    }
    if (panel.id === "color_grading") {
      const host = document.createElement("div");
      body.append(host);
      ui.wheels = AmbWheels.create(host, {
        onInput(changes) { Object.assign(ui.overrides, changes); requestPreview(true); },
        onCommit(changes) {
          for (const name of Object.keys(changes)) delete ui.overrides[name];
          run(Object.entries(changes).map(([name, value]) => ({ cmd: "set_param", name, value })), "color grading");
        },
      });
    }
    // Canvases inside a closed panel have no size; redraw when it opens.
    details.addEventListener("toggle", () => {
      if (details.open) { ui.curve?.refresh(); ui.wheels?.refresh(); }
    });
    for (const section of panel.sections) {
      if (panel.id === "color_grading" && isGradeWheel(section)) continue;
      if (panel.sections.length > 1 || section.layout === "sliders") {
        const l = document.createElement("div");
        l.className = "section-label";
        l.textContent = section.label;
        body.append(l);
      }
      if (section.layout === "hsl") {
        const tabs = document.createElement("div");
        tabs.className = "tabs";
        const holder = document.createElement("div");
        const rows = {};
        for (const def of section.controls) {
          const kind = def.name.match(/^(Hue|Saturation|Luminance)/)[1];
          const band = def.name.replace(/^(Hue|Saturation|Luminance)Adjustment/, "");
          const row = makeRow({ ...def, label: band });
          (rows[kind] ||= []).push(row);
        }
        for (const kind of ["Hue", "Saturation", "Luminance"]) {
          const b = document.createElement("button");
          b.textContent = kind;
          b.addEventListener("click", () => {
            ui.hslTab = kind;
            for (const [k, rs] of Object.entries(rows)) rs.forEach((r) => (r.hidden = k !== kind));
            tabs.querySelectorAll("button").forEach((x) => x.classList.toggle("active", x === b));
          });
          tabs.append(b);
          rows[kind].forEach((r) => { r.hidden = kind !== ui.hslTab; holder.append(r); });
          if (kind === ui.hslTab) b.classList.add("active");
        }
        body.append(tabs, holder);
      } else {
        for (const def of section.controls) body.append(makeRow(def));
      }
    }
    details.append(summary, body);
    root.append(details);
  }
}

function syncControls() {
  for (const [name, c] of ui.controls) {
    const v = currentValue(name);
    const changed = name in ui.settings;
    if (c.label) c.label.classList.toggle("changed", changed);
    if (c.check) c.check.checked = asBool(v);
    else if (c.select) c.select.value = JSON.stringify(typeof v === "string" && /^\d+$/.test(v) ? Number(v) : v);
    else if (c.input) {
      const n = Number(v);
      c.input.value = toSlider(c.def, n);
      c.number.value = format(c.def, n);
    }
  }
  $("btn-undo").disabled = !ui.state?.can_undo;
  $("btn-redo").disabled = !ui.state?.can_redo;
  const hist = $("history-list");
  hist.innerHTML = "";
  for (const label of ui.state?.history ?? []) {
    const li = document.createElement("li");
    li.textContent = label.replace(/^set_param /, "").replace(/^apply_preset .*[\\/]/, "preset ");
    hist.append(li);
  }
  if (ui.curve) for (const tag of AmbCurve.TAGS) ui.curve.set(tag, currentValue(tag));
  if (ui.wheels) ui.wheels.set(Object.fromEntries(AmbWheels.TAGS.map((t) => [t, currentValue(t)])));
  const nr = ui.state?.not_rendered ?? [];
  $("status-right").textContent = nr.length ? `Not rendered yet: ${nr.length} setting(s)` : "";
  $("status-right").title = nr.join(", ");
}

// ---------------------------------------------------------------- state & commands

function setState(state) {
  ui.state = state;
  ui.settings = state.settings ?? {};
  ui.overrides = {};
  syncControls();
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

function commit(name, value) {
  delete ui.overrides[name];
  run([{ cmd: "set_param", name, value }]);
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
    try {
      if (ui.zoom.on) {
        // At 100% only committed changes render (a full-size render per drag step is too slow).
        if (!draft) await renderRegion();
        continue;
      }
      const buf = await invoke("preview", {
        maxEdge: previewSize(draft),
        overrides: ui.before ? {} : { ...ui.overrides },
        before: ui.before,
        clipping: ui.clipping,
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
      drawHistogram(meta.histogram);
      ui.curve?.setHistogram(meta.histogram);
      status(`${meta.width}×${meta.height} preview in ${meta.millis} ms` +
        (meta.lens_profile ? `  ·  Lens: ${meta.lens_profile}` : ""));
    } catch (e) {
      toast(String(e), true);
    }
  }
  $("busy").hidden = true;
  ui.preview.inFlight = false;
}

function drawHistogram(h) {
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
  img.hidden = false;
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
    ui.info = r.info;
    $("empty").hidden = true;
    $("photo-title").textContent = `${r.info.name}  —  ${r.info.camera}${r.info.lens ? "  ·  " + r.info.lens : ""}`;
    $("exif").textContent = [r.info.exposure, `${r.info.width} × ${r.info.height}`,
      r.info.sidecar_loaded ? "edits loaded from sidecar" : ""].filter(Boolean).join("  ·  ");
    for (const id of ["btn-save", "btn-export", "btn-before", "btn-zoom", "btn-wb", "btn-clip"]) $(id).disabled = false;
    if (ui.zoom.on) setZoom(false);
    setState(r.state);
    requestPreview(false);
  } catch (e) {
    toast(String(e), true);
    status("Ready");
  } finally {
    $("busy").hidden = true;
  }
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

async function exportFlow() {
  if (!ui.info) return;
  const dlg = $("export-dialog");
  updateExportForm();
  dlg.showModal();
  const choice = await new Promise((resolve) => dlg.addEventListener("close", () => resolve(dlg.returnValue), { once: true }));
  if (choice !== "ok") return;
  const f = $("export-form");
  const base = ui.info.name.replace(/\.[^.]+$/, "");
  const path = await invoke("pick_export_path", { suggested: base, format: f.format.value });
  if (!path) return;
  const options = {
    format: f.format.value,
    quality: Number(f.quality.value),
    bits: Number(f.bits.value),
    color_space: f.color_space.value,
    long_edge: f.long_edge.value ? Number(f.long_edge.value) : null,
    sharpen: f.sharpen.value || null,
    metadata: f.metadata.value,
    max_kb: f.max_kb.value ? Number(f.max_kb.value) : null,
  };
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
  buildPanels();
  syncControls();

  $("btn-open").addEventListener("click", openDialog);
  $("btn-open-empty").addEventListener("click", openDialog);
  $("btn-save").addEventListener("click", save);
  $("btn-export").addEventListener("click", exportFlow);
  $("btn-undo").addEventListener("click", () => run([{ cmd: "undo" }]));
  $("btn-redo").addEventListener("click", () => run([{ cmd: "redo" }]));
  $("btn-before").addEventListener("click", toggleBefore);
  $("btn-zoom").addEventListener("click", () => setZoom(!ui.zoom.on, ui.zoom.cx, ui.zoom.cy));
  $("btn-wb").addEventListener("click", () => setPicking(!ui.picking));
  $("btn-clip").addEventListener("click", toggleClipping);
  $("btn-feedback").addEventListener("click", feedbackFlow);
  wireViewer();
  $("btn-add-presets").addEventListener("click", () => loadPresets(true));
  $("preset-filter").addEventListener("input", renderPresets);
  $("preset-amount").addEventListener("input", (e) => ($("preset-amount-out").textContent = e.target.value));
  const f = $("export-form");
  f.format.addEventListener("change", updateExportForm);
  f.quality.addEventListener("input", () => (f.quality_out.value = f.quality.value));

  document.addEventListener("keydown", (e) => {
    if (e.target.matches("input[type=number], input[type=search], input[type=text], textarea, input.value")) return;
    const mod = e.ctrlKey || e.metaKey;
    const k = e.key.toLowerCase();
    if (mod && k === "z" && !e.shiftKey) { e.preventDefault(); run([{ cmd: "undo" }]); }
    else if (mod && ((k === "z" && e.shiftKey) || k === "y")) { e.preventDefault(); run([{ cmd: "redo" }]); }
    else if (mod && k === "s") { e.preventDefault(); save(); }
    else if (mod && k === "e") { e.preventDefault(); exportFlow(); }
    else if (mod && k === "o") { e.preventDefault(); openDialog(); }
    else if (e.key === "\\") { e.preventDefault(); toggleBefore(); }
    else if (mod) return;
    else if (k === "z" && ui.info) { e.preventDefault(); setZoom(!ui.zoom.on, ui.zoom.cx, ui.zoom.cy); }
    else if (k === "w") { e.preventDefault(); setPicking(!ui.picking); }
    else if (k === "j") { e.preventDefault(); toggleClipping(); }
    else if (e.key === "Escape") setPicking(false);
  });

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
    if (paths.length) openPath(paths[0]);
  });
  loadPresets();
  // A photo given on the command line.
  const initial = await invoke("take_initial_path");
  if (initial) openPath(initial);
}

init().catch((e) => toast(String(e), true));
