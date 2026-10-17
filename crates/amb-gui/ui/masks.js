// SPDX-License-Identifier: GPL-3.0-or-later
// Masks (Lightroom's Masking panel). Plain JS, no build step.
//
// Every change is an amb-protocol mask command (add_mask, set_mask,
// paint_mask, remove_mask), so masks made here are Lightroom's own
// MaskGroupBasedCorrections. While a handle or slider is dragged, the preview
// renders an edited copy of that tag (an override) and the command runs on
// release, as one undo step.
//
// Coordinates: masks live in fractions of the uncropped photo in its stored
// orientation. `mask_frame` gives the affine map from the shown (upright,
// cropped) preview to that frame; the canvas draws through its inverse.
"use strict";

const MASK_TAG = "MaskGroupBasedCorrections";
const NEW_KINDS = [
  { kind: "linear", label: "Linear", key: "L", hint: "Drag from where the effect is full to where it ends." },
  { kind: "radial", label: "Radial", key: "Shift+R", hint: "Drag from the centre outwards." },
  { kind: "brush", label: "Brush", key: "B", hint: "Paint on the photo. Hold Alt to erase; [ and ] change the size." },
  { kind: "luminance", label: "Luminance", key: "", hint: "Selects by brightness; set the range below." },
  { kind: "subject", label: "Subject", key: "", hint: "Selects the main subject, found by an AI model on this computer (Settings → AI models picks which; the small default downloads by itself, about 5 MB)." },
  { kind: "background", label: "Background", key: "", hint: "Selects everything but the main subject (the subject mask, inverted)." },
  { kind: "people", label: "People", key: "", hint: "Selects everyone in the photo, found by an AI model on this computer (install one in Settings → AI models, about 180 MB)." },
  { kind: "sky", label: "Sky", key: "", hint: "Selects the sky: bright blue or grey areas joined to the top edge (our own rules, not Adobe's AI)." },
  { kind: "color", label: "Color", key: "", hint: "Click a colour in the photo; Shift-click to add more." },
];

const mk = {
  on: false,
  selected: null,       // mask index
  part: null,           // part index in the selected mask
  tool: null,           // creating: "linear" | "radial" | "brush" | "color"
  addMode: "add",       // how a new part joins the selected mask
  overlay: true,
  frame: null,          // mask_frame result
  sliders: [],          // mask_sliders result
  effects: null,        // mask_effects result: { builtin, mine }
  drag: null,
  hover: null,          // canvas point (CSS px) for the brush cursor
  stroke: null,         // points being painted (frame fractions)
  brush: { size: 5, feather: 50, flow: 100, density: 100, erase: false },
  draft: null,          // edited copy of the corrections being previewed
};

// ---------------------------------------------------------------- state

function maskList() {
  return ui.state?.masks?.masks ?? [];
}

function selectedMask() {
  return mk.selected == null ? null : maskList()[mk.selected] ?? null;
}

function selectedPart() {
  const m = selectedMask();
  return m && mk.part != null ? m.parts[mk.part] ?? null : null;
}

// The overlay index for previews (preview.js renderLoop).
function maskOverlayIndex() {
  return mk.on && mk.overlay && mk.selected != null ? mk.selected : null;
}

// A deep copy of the stored corrections (strings, as the state holds them).
function correctionsCopy() {
  return JSON.parse(JSON.stringify(ui.settings[MASK_TAG] ?? []));
}

// The raw components of the selected part (brush parts have several).
function partEntries(corrections, mask, part) {
  const c = corrections[mask];
  const comps = c?.CorrectionMasks ?? [];
  const m = maskList()[mask];
  if (!m) return [];
  // Parts are listed in order; an older file's brush spans several entries.
  let i = 0;
  for (const p of m.parts) {
    const n = p.entries ?? 1;
    if (p.index === part) return comps.slice(i, i + n);
    i += n;
  }
  return [];
}

function previewDraft(corrections) {
  mk.draft = corrections;
  ui.overrides[MASK_TAG] = corrections;
  requestPreview(true);
}

function endDraft() {
  mk.draft = null;
  delete ui.overrides[MASK_TAG];
}

const fmt6 = (v) => String(Math.round(v * 1e6) / 1e6);

// ---------------------------------------------------------------- geometry

// These helpers serve the spot tool too (spots.js), with its own frame and canvas.
function viewRect(canvas = "mask-canvas") {
  const img = $("preview");
  const r = img.getBoundingClientRect();
  const c = $(canvas).getBoundingClientRect();
  return { left: r.left - c.left, top: r.top - c.top, width: r.width, height: r.height };
}

// Canvas CSS px → frame fractions.
function toFrame(px, py, f = mk.frame, canvas = "mask-canvas") {
  const r = viewRect(canvas);
  const u = (px - r.left) / r.width;
  const v = (py - r.top) / r.height;
  const m = f.matrix;
  return [m[0] * u + m[1] * v + m[2], m[3] * u + m[4] * v + m[5]];
}

// Frame fractions → canvas CSS px.
function toCanvas(x, y, f = mk.frame, canvas = "mask-canvas") {
  const m = f.matrix;
  const det = m[0] * m[4] - m[1] * m[3];
  const dx = x - m[2];
  const dy = y - m[5];
  const u = (m[4] * dx - m[1] * dy) / det;
  const v = (m[0] * dy - m[3] * dx) / det;
  const r = viewRect(canvas);
  return [r.left + u * r.width, r.top + v * r.height];
}

// Screen px per full-size frame pixel.
function pxScale(f = mk.frame, canvas = "mask-canvas") {
  return viewRect(canvas).width / f.view_width;
}

// Brush sizes are fractions of the frame's width, as Lightroom's.
function frameWidth() {
  return mk.frame.frame_width;
}

// A frame-pixel offset (dx, dy) from frame point (x, y), as a canvas point.
function framePixel(x, y, dx, dy) {
  return toCanvas(x + dx / mk.frame.frame_width, y + dy / mk.frame.frame_height);
}

// The ellipse of a radial part, as canvas points (`k` scales the axes).
function ellipsePoints(p, k = 1) {
  const W = mk.frame.frame_width;
  const H = mk.frame.frame_height;
  const cx = (p.Left + p.Right) / 2;
  const cy = (p.Top + p.Bottom) / 2;
  const a = (Math.abs(p.Right - p.Left) / 2) * W * k;
  const b = (Math.abs(p.Bottom - p.Top) / 2) * H * k;
  const t = ((p.Angle ?? 0) * Math.PI) / 180;
  const [cos, sin] = [Math.cos(t), Math.sin(t)];
  const pts = [];
  for (let i = 0; i <= 64; i++) {
    const th = (i / 64) * 2 * Math.PI;
    const u = a * Math.cos(th);
    const v = b * Math.sin(th);
    // The engine's inverse rotation (src/mask.rs).
    pts.push(framePixel(cx, cy, u * cos + v * sin, -u * sin + v * cos));
  }
  return pts;
}

function radialHandles(p) {
  const W = mk.frame.frame_width;
  const H = mk.frame.frame_height;
  const cx = (p.Left + p.Right) / 2;
  const cy = (p.Top + p.Bottom) / 2;
  const a = (Math.abs(p.Right - p.Left) / 2) * W;
  const b = (Math.abs(p.Bottom - p.Top) / 2) * H;
  const t = ((p.Angle ?? 0) * Math.PI) / 180;
  const [cos, sin] = [Math.cos(t), Math.sin(t)];
  const at = (u, v) => framePixel(cx, cy, u * cos + v * sin, -u * sin + v * cos);
  return { center: toCanvas(cx, cy), a: at(a, 0), a2: at(-a, 0), b: at(0, b), b2: at(0, -b) };
}

function linearHandles(p) {
  return { zero: toCanvas(p.ZeroX, p.ZeroY), full: toCanvas(p.FullX, p.FullY) };
}

// ---------------------------------------------------------------- drawing

function drawMasks() {
  const c = $("mask-canvas");
  if (!mk.on || c.hidden) return;
  const dpr = window.devicePixelRatio || 1;
  const w = c.clientWidth;
  const h = c.clientHeight;
  if (c.width !== Math.round(w * dpr) || c.height !== Math.round(h * dpr)) {
    c.width = Math.round(w * dpr);
    c.height = Math.round(h * dpr);
  }
  const g = c.getContext("2d");
  g.setTransform(dpr, 0, 0, dpr, 0, 0);
  g.clearRect(0, 0, w, h);
  if (!mk.frame || !ui.preview.url) return;
  const accent = getComputedStyle(document.documentElement).getPropertyValue("--accent").trim() || "#f0b45a";
  g.lineWidth = 1.5;
  const line = (pts, dash = []) => {
    g.setLineDash(dash);
    g.beginPath();
    pts.forEach(([x, y], i) => (i ? g.lineTo(x, y) : g.moveTo(x, y)));
    g.strokeStyle = "rgba(0,0,0,0.55)";
    g.lineWidth = 3;
    g.stroke();
    g.strokeStyle = "#fff";
    g.lineWidth = 1.5;
    g.stroke();
    g.setLineDash([]);
  };
  const dot = ([x, y], fill = "#fff", r = 5) => {
    g.beginPath();
    g.arc(x, y, r, 0, 2 * Math.PI);
    g.fillStyle = fill;
    g.fill();
    g.strokeStyle = "rgba(0,0,0,0.6)";
    g.lineWidth = 1.5;
    g.stroke();
  };

  // The part being edited (or created).
  const p = mk.drag?.part ?? selectedPart();
  if (p && p.kind === "linear") {
    const { zero, full } = linearHandles(p);
    const dx = zero[0] - full[0];
    const dy = zero[1] - full[1];
    const len = Math.hypot(dx, dy) || 1;
    const nx = -dy / len;
    const ny = dx / len;
    const span = Math.hypot(w, h);
    const across = ([x, y]) => [[x - nx * span, y - ny * span], [x + nx * span, y + ny * span]];
    line(across(full));
    line(across(zero), [6, 5]);
    line([full, zero], [2, 4]);
    dot(full, accent);
    dot(zero);
  } else if (p && p.kind === "radial") {
    line(ellipsePoints(p));
    const f = (p.Feather ?? 50) / 100;
    if (f > 0.02 && f < 0.98) line(ellipsePoints(p, 1 - f), [5, 5]);
    const hs = radialHandles(p);
    dot(hs.center, accent);
    for (const k of ["a", "a2", "b", "b2"]) dot(hs[k], "#fff", 4);
  } else if (p && p.kind === "color") {
    for (const [x, y] of p.ColorSamples ?? []) {
      const [cx, cy] = toCanvas(x, y);
      g.beginPath();
      g.arc(cx, cy, 7, 0, 2 * Math.PI);
      g.strokeStyle = "rgba(0,0,0,0.6)";
      g.lineWidth = 3;
      g.stroke();
      g.strokeStyle = "#fff";
      g.lineWidth = 1.5;
      g.stroke();
      line([[cx - 11, cy], [cx - 4, cy]]);
      line([[cx + 4, cy], [cx + 11, cy]]);
    }
  }

  // A stroke being painted.
  const radius = (mk.brush.size / 100) * frameWidth() * pxScale();
  if (mk.stroke) {
    g.fillStyle = mk.brush.erase || mk.stroke.erase ? "rgba(80,140,255,0.35)" : "rgba(230,60,60,0.35)";
    for (const [x, y] of mk.stroke.points) {
      const [cx, cy] = toCanvas(x, y);
      g.beginPath();
      g.arc(cx, cy, radius, 0, 2 * Math.PI);
      g.fill();
    }
  }
  // The brush cursor: outer size and inner (unfeathered) circle.
  if (mk.tool === "brush" && mk.hover) {
    const [x, y] = mk.hover;
    g.beginPath();
    g.arc(x, y, radius, 0, 2 * Math.PI);
    g.strokeStyle = "rgba(0,0,0,0.6)";
    g.lineWidth = 3;
    g.stroke();
    g.strokeStyle = "#fff";
    g.lineWidth = 1.25;
    g.stroke();
    const inner = radius * (1 - mk.brush.feather / 100);
    if (inner > 2) {
      g.setLineDash([3, 4]);
      g.beginPath();
      g.arc(x, y, inner, 0, 2 * Math.PI);
      g.stroke();
      g.setLineDash([]);
    }
    if (mk.brush.erase || mk.hover.alt) line([[x - 5, y], [x + 5, y]]);
  }
  // Radial being drawn.
  if (mk.drag?.kind === "new-radial" && mk.drag.part) {
    line(ellipsePoints(mk.drag.part));
  }
}

// ---------------------------------------------------------------- panel

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

function sliderRow({ label, min, max, step, value, title, onInput, onChange, disabled }) {
  const decimals = step < 1 ? 2 : 0;
  const show = (v) => (min < 0 && v > 0 ? "+" : "") + Number(v).toFixed(decimals);
  const range = el("input", { type: "range", min, max, step, value, disabled });
  const num = el("input", { class: "value", type: "text", inputmode: "decimal", spellcheck: "false", value: show(value), disabled });
  const name = el("label", { title }, label);
  if (Number(value) !== 0 && min < 0) name.classList.add("changed");
  range.addEventListener("input", () => {
    num.value = show(range.value);
    onInput?.(Number(range.value));
  });
  range.addEventListener("change", () => onChange(Number(range.value)));
  range.addEventListener("dblclick", () => {
    const d = min < 0 ? 0 : Number(range.defaultValue ?? min);
    range.value = d;
    num.value = show(d);
    onChange(d);
  });
  num.addEventListener("change", () => {
    const v = Math.min(max, Math.max(min, Number(num.value.replace(",", "."))));
    if (!Number.isFinite(v)) return;
    range.value = v;
    num.value = show(v);
    onChange(v);
  });
  return el("div", { class: "row mask-slider" }, name, range, num);
}

function renderMaskPanel() {
  if (!mk.on) return;
  const list = maskList();
  if (mk.selected != null && mk.selected >= list.length) {
    mk.selected = list.length ? list.length - 1 : null;
    mk.part = null;
  }
  const m = selectedMask();
  if (m && mk.part != null && mk.part >= m.parts.length) mk.part = null;

  // New-mask buttons.
  const newRow = $("mask-new");
  newRow.replaceChildren(...NEW_KINDS.map((k) => {
    const active = mk.tool === k.kind && mk.creatingNew;
    return el("button", {
      type: "button",
      class: "small" + (active ? " active" : ""),
      title: `New ${k.label.toLowerCase()} mask${k.key ? ` (${k.key})` : ""}. ${k.hint}`,
      onclick: () => startTool(k.kind, true),
    }, k.label);
  }));

  // The mask list.
  const rows = list.map((mask) => {
    const sel = mask.index === mk.selected;
    const eye = el("input", { type: "checkbox", title: "Show this mask's effect", checked: mask.active });
    eye.addEventListener("change", () => run([{ cmd: "set_mask", mask: mask.index, values: { active: eye.checked } }]));
    const name = el("span", { class: "mask-name", title: "Double-click to rename" }, mask.name);
    name.addEventListener("dblclick", (e) => {
      e.stopPropagation();
      const input = el("input", { type: "text", value: mask.name, spellcheck: "false" });
      name.replaceWith(input);
      input.focus();
      input.select();
      const done = (save) => {
        if (save && input.value.trim() && input.value !== mask.name) {
          run([{ cmd: "set_mask", mask: mask.index, values: { name: input.value.trim() } }]);
        } else renderMaskPanel();
      };
      input.addEventListener("keydown", (ev) => {
        if (ev.key === "Enter") done(true);
        if (ev.key === "Escape") done(false);
        ev.stopPropagation();
      });
      input.addEventListener("blur", () => done(true));
    });
    const del = el("button", { type: "button", class: "small icon-x", title: "Delete this mask" }, "×");
    del.addEventListener("click", (e) => {
      e.stopPropagation();
      run([{ cmd: "remove_mask", mask: mask.index }]);
    });
    const kinds = [...new Set(mask.parts.map((p) => p.label))].join(", ");
    const row = el("div", { class: "mask-row" + (sel ? " selected" : ""), title: `${kinds}. Drag to reorder.`, draggable: "true" },
      eye, name, el("span", { class: "hint mask-kinds" }, kinds), del);
    row.addEventListener("click", () => selectMask(mask.index));
    // Drag a row onto another to move it there.
    row.addEventListener("dragstart", (e) => {
      e.dataTransfer.setData("text/x-amb-mask", String(mask.index));
      e.dataTransfer.effectAllowed = "move";
    });
    row.addEventListener("dragover", (e) => {
      if (!e.dataTransfer.types.includes("text/x-amb-mask")) return;
      e.preventDefault();
      row.classList.add("drop");
    });
    row.addEventListener("dragleave", () => row.classList.remove("drop"));
    row.addEventListener("drop", (e) => {
      e.preventDefault();
      row.classList.remove("drop");
      const from = Number(e.dataTransfer.getData("text/x-amb-mask"));
      moveMask(from, mask.index);
    });
    return row;
  });
  $("mask-list").replaceChildren(...rows);
  if (!list.length) $("mask-list").append(el("p", { class: "hint" }, "No masks yet. Pick a kind above, then draw on the photo."));
  if (ui.state?.masks?.legacy) {
    $("mask-list").append(el("p", { class: "hint" },
      "This photo also has older Lightroom masks. They are shown in the photo, but can't be edited here yet."));
  }

  const detail = $("mask-detail");
  detail.hidden = !m;
  if (m) renderMaskDetail(m);
  renderToolOptions();
  drawMasks();
}

function renderMaskDetail(m) {
  // Duplicate and move.
  const count = maskList().length;
  const action = (text, title, disabled, onclick) => el("button", { type: "button", class: "small", title, disabled, onclick }, text);
  $("mask-actions").replaceChildren(
    action("Duplicate", "Copy this mask, sliders and all", false, () => duplicateMask(m.index)),
    action("↑", "Move up the list", m.index === 0, () => moveMask(m.index, m.index - 1)),
    action("↓", "Move down the list", m.index >= count - 1, () => moveMask(m.index, m.index + 1)),
  );
  // Parts.
  const parts = m.parts.map((p) => {
    const sel = p.index === mk.part;
    const mode = el("select", { title: "How this part joins the ones above it", disabled: p.index === 0 && p.mode === "add" });
    for (const v of ["add", "subtract", "intersect"]) mode.append(el("option", { value: v }, v[0].toUpperCase() + v.slice(1)));
    mode.value = p.mode;
    mode.addEventListener("click", (e) => e.stopPropagation());
    mode.addEventListener("change", () => run([{ cmd: "set_mask", mask: m.index, part: p.index, values: { mode: mode.value } }]));
    const inv = el("input", { type: "checkbox", title: "Invert this part", checked: p.invert });
    inv.addEventListener("click", (e) => e.stopPropagation());
    inv.addEventListener("change", () => run([{ cmd: "set_mask", mask: m.index, part: p.index, values: { invert: inv.checked } }]));
    const del = el("button", { type: "button", class: "small icon-x", title: "Remove this part" }, "×");
    del.addEventListener("click", (e) => {
      e.stopPropagation();
      run([{ cmd: "remove_mask", mask: m.index, part: p.index }]);
    });
    // Parts combine in order: move one up or down.
    const move = (dir) => {
      const b = el("button", { type: "button", class: "small", title: dir < 0 ? "Move this part up" : "Move this part down",
        disabled: m.parts.length < 2 || (dir < 0 ? p.index === 0 : p.index === m.parts.length - 1) }, dir < 0 ? "↑" : "↓");
      b.addEventListener("click", (e) => {
        e.stopPropagation();
        movePart(m, p.index, p.index + dir);
      });
      return b;
    };
    const label = p.label + (p.rendered ? "" : " (not rendered yet)");
    const row = el("div", { class: "mask-part" + (sel ? " selected" : "") },
      el("span", { class: "mask-name" }, label), mode, el("label", { class: "inline-check", title: "Invert" }, inv, "Inv"),
      move(-1), move(1), del);
    row.addEventListener("click", () => selectPart(p.index));
    return row;
  });
  $("mask-parts").replaceChildren(...parts);

  // Add a part.
  const addRow = $("mask-add");
  const modeSel = el("select", { title: "How the new part joins this mask" });
  for (const v of ["add", "subtract", "intersect"]) modeSel.append(el("option", { value: v }, v[0].toUpperCase() + v.slice(1)));
  modeSel.value = mk.addMode;
  modeSel.addEventListener("change", () => (mk.addMode = modeSel.value));
  addRow.replaceChildren(modeSel, ...NEW_KINDS.map((k) => el("button", {
    type: "button",
    class: "small" + (mk.tool === k.kind && !mk.creatingNew ? " active" : ""),
    title: `${k.label} as a new part of this mask. ${k.hint}`,
    onclick: () => startTool(k.kind, false),
  }, k.label)));

  // Part settings.
  const p = selectedPart();
  const box = $("mask-part-settings");
  box.replaceChildren();
  const setPart = (values) => run([{ cmd: "set_mask", mask: m.index, part: p.index, values }]);
  if (p?.kind === "radial") {
    box.append(
      sliderRow({ label: "Feather", min: 0, max: 100, step: 1, value: p.Feather ?? 50, onInput: (v) => draftPart({ Feather: v }), onChange: (v) => setPart({ Feather: v }) }),
      sliderRow({ label: "Angle", min: -180, max: 180, step: 1, value: p.Angle ?? 0, title: "Turns the ellipse", onInput: (v) => draftPart({ Angle: v }), onChange: (v) => setPart({ Angle: v }) }),
    );
  } else if (p?.kind === "luminance") {
    const r = p.LumRange ?? [0.5, 0.6, 1, 1];
    const from = Math.round(r[1] * 100);
    const to = Math.round(r[2] * 100);
    const smooth = Math.round(Math.max(r[1] - r[0], r[3] - r[2]) * 100);
    const range = (f, t, s) => {
      const lo = Math.min(f, t) / 100;
      const hi = Math.max(f, t) / 100;
      const sm = s / 100;
      return [Math.max(0, lo - sm), lo, hi, Math.min(1, hi + sm)];
    };
    let cur = { f: from, t: to, s: smooth };
    const draft = () => draftPart({ LumRange: range(cur.f, cur.t, cur.s) });
    const commit = () => setPart({ LumRange: range(cur.f, cur.t, cur.s) });
    box.append(
      sliderRow({ label: "From", min: 0, max: 100, step: 1, value: from, title: "Darkest brightness selected", onInput: (v) => { cur.f = v; draft(); }, onChange: (v) => { cur.f = v; commit(); } }),
      sliderRow({ label: "To", min: 0, max: 100, step: 1, value: to, title: "Brightest brightness selected", onInput: (v) => { cur.t = v; draft(); }, onChange: (v) => { cur.t = v; commit(); } }),
      sliderRow({ label: "Smoothness", min: 0, max: 50, step: 1, value: smooth, title: "How gradually the selection fades out", onInput: (v) => { cur.s = v; draft(); }, onChange: (v) => { cur.s = v; commit(); } }),
    );
  } else if (p?.kind === "color") {
    box.append(
      sliderRow({ label: "Refine", min: 0, max: 100, step: 1, value: p.ColorAmount ?? 50, title: "Lower selects only colours close to the samples", onInput: (v) => draftPart({ ColorAmount: v }), onChange: (v) => setPart({ ColorAmount: v }) }),
      el("div", { class: "crop-line" },
        el("button", { type: "button", class: "small", onclick: () => startTool("color", false, true) }, "Add a sample"),
        el("button", { type: "button", class: "small", onclick: () => setPart({ ColorSamples: [] }) }, "Clear samples"),
        el("span", { class: "hint" }, `${(p.ColorSamples ?? []).length} sample(s)`)),
    );
  } else if (p?.kind === "brush") {
    box.append(el("div", { class: "crop-line" },
      el("button", { type: "button", class: "small" + (mk.tool === "brush" && mk.continuePart ? " active" : ""), onclick: () => startTool("brush", false, true) }, "Paint more"),
      el("span", { class: "hint" }, `${p.strokes} stroke(s)`)));
  }

  // The mask's sliders, and the Effect menu that sets them all at once.
  const sl = $("mask-sliders");
  sl.replaceChildren(effectRow(m));
  sl.append(sliderRow({
    label: "Amount", min: 0, max: 100, step: 1, value: m.amount, title: "Scales all of this mask's sliders",
    onInput: (v) => draftCorrection({ CorrectionAmount: fmt6(v / 100) }),
    onChange: (v) => run([{ cmd: "set_mask", mask: m.index, values: { amount: v } }]),
  }));
  let section = null;
  for (const s of mk.sliders) {
    if (s.section !== section) {
      section = s.section;
      sl.append(el("div", { class: "section-label" }, section));
    }
    const value = m.sliders[s.tag] ?? (s.tag === "LocalToningHue" ? 0 : 0);
    const scale = s.tag === "LocalExposure2012" ? 4 : s.tag === "LocalToningHue" ? 1 : 100;
    sl.append(sliderRow({
      label: s.label, min: s.min, max: s.max, step: s.step, value,
      title: s.rendered ? s.tag : `${s.tag}: kept in the file, not rendered yet`,
      disabled: !s.rendered && !value,
      onInput: (v) => draftCorrection({ [s.tag]: fmt6(v / scale) }),
      onChange: (v) => run([{ cmd: "set_mask", mask: m.index, values: { [s.tag]: v } }]),
    }));
  }
}

// Effect: named slider settings, ours and the user's. Picking one sets every
// slider (the ones it doesn't name go to 0), as Lightroom's Effect menu does.
function effectRow(m) {
  const fx = mk.effects ?? { builtin: [], mine: [] };
  const select = el("select", { title: "Set this mask's sliders from a saved effect" });
  select.append(el("option", { value: "" }, "Effect…"));
  const group = (label, list, prefix) => {
    if (!list.length) return;
    const g = el("optgroup", { label });
    for (const e of list) g.append(el("option", { value: prefix + e.name }, e.name));
    select.append(g);
  };
  group("Ambrotype", fx.builtin, "b:");
  group("Yours", fx.mine, "u:");
  // The last one picked stays shown, so it can be deleted.
  if ([...select.options].some((o) => o.value === mk.effect)) select.value = mk.effect;
  select.addEventListener("change", () => {
    const v = select.value;
    mk.effect = v;
    const e = (v.startsWith("b:") ? fx.builtin : fx.mine).find((x) => x.name === v.slice(2));
    if (e) applyEffect(m, e);
  });
  const save = el("button", { type: "button", class: "small", title: "Keep this mask's sliders as an effect of your own" }, "Save…");
  save.addEventListener("click", async () => {
    const name = await askName("Save as an effect", "This mask's sliders, to use on other masks.");
    if (!name) return;
    const values = Object.fromEntries(Object.entries(m.sliders).filter(([, v]) => Number(v) !== 0));
    try {
      mk.effects = await invoke("save_mask_effect", { name, values });
      mk.effect = `u:${name}`;
      toast(`Saved effect "${name}"`);
      renderMaskPanel();
    } catch (err) {
      toast(String(err), true);
    }
  });
  const mine = select.value.startsWith("u:");
  const del = el("button", { type: "button", class: "small", title: mine ? "Delete this effect of yours" : "Pick one of your own effects to delete it", disabled: !mine }, "Delete");
  del.addEventListener("click", async () => {
    mk.effects = await invoke("delete_mask_effect", { name: select.value.slice(2) }).catch((err) => (toast(String(err), true), mk.effects));
    mk.effect = "";
    renderMaskPanel();
  });
  return el("div", { class: "crop-line mask-effect" }, select, save, del);
}

function applyEffect(m, effect) {
  const values = {};
  for (const s of mk.sliders) {
    if (!s.rendered && !(s.tag in effect.values)) continue;
    values[s.tag] = effect.values[s.tag] ?? 0;
  }
  run([{ cmd: "set_mask", mask: m.index, values }], `Effect: ${effect.name}`);
}

function renderToolOptions() {
  const box = $("mask-tool");
  box.replaceChildren();
  const k = NEW_KINDS.find((n) => n.kind === mk.tool);
  $("mask-hint").textContent = k
    ? `${k.label}: ${k.hint} Esc stops.`
    : maskList().length
      ? "Click a mask to edit it; drag its handles on the photo. O shows or hides the overlay."
      : "";
  if (mk.tool !== "brush") return;
  const b = mk.brush;
  box.append(
    sliderRow({ label: "Size", min: 0.5, max: 25, step: 0.5, value: b.size, title: "Radius, % of the photo's width ([ and ])", onInput: (v) => { b.size = v; drawMasks(); }, onChange: (v) => { b.size = v; drawMasks(); } }),
    sliderRow({ label: "Feather", min: 0, max: 100, step: 1, value: b.feather, onInput: (v) => { b.feather = v; drawMasks(); }, onChange: (v) => (b.feather = v) }),
    sliderRow({ label: "Flow", min: 1, max: 100, step: 1, value: b.flow, title: "How fast paint builds up", onChange: (v) => (b.flow = v) }),
    sliderRow({ label: "Density", min: 1, max: 100, step: 1, value: b.density, title: "Most strength the brush reaches", onChange: (v) => (b.density = v) }),
  );
  const erase = el("input", { type: "checkbox", checked: b.erase });
  erase.addEventListener("change", () => (b.erase = erase.checked));
  box.append(el("label", { class: "inline-check", title: "Or hold Alt while painting" }, erase, "Erase"));
}

// ---------------------------------------------------------------- drafts

function draftCorrection(fields) {
  if (mk.selected == null) return;
  const cs = mk.draft ?? correctionsCopy();
  Object.assign(cs[mk.selected], fields);
  previewDraft(cs);
}

// Edit the selected part's stored fields in a draft (UI values → stored).
function draftPart(values) {
  const p = selectedPart();
  if (!p) return;
  const cs = mk.draft ?? correctionsCopy();
  const [e] = partEntries(cs, mk.selected, p.index);
  if (!e) return;
  for (const [k, v] of Object.entries(values)) {
    if (k === "LumRange") (e.CorrectionRangeMask ??= {}).LumRange = v.map((x) => x.toFixed(6)).join(" ");
    else if (k === "ColorAmount") (e.CorrectionRangeMask ??= {}).ColorAmount = fmt6(v / 100);
    else e[k] = fmt6(v);
  }
  previewDraft(cs);
}

// ---------------------------------------------------------------- tools

async function duplicateMask(i) {
  await run([{ cmd: "duplicate_mask", mask: i }]);
  selectMask(i + 1);
}

async function movePart(m, from, to) {
  // The first part has to add: subtract and intersect act on the ones before.
  const first = to === 0 ? m.parts[from] : from === 0 ? m.parts[1] : null;
  if (first && first.mode !== "add") return toast("The first part has to add; change its mode first", true);
  const sel = mk.part;
  await run([{ cmd: "move_mask", mask: m.index, part: from, to }]);
  // The selected part stays selected.
  if (sel === from) selectPart(to);
  else if (sel === to) selectPart(from);
}

async function moveMask(from, to) {
  if (from === to || !Number.isInteger(from)) return;
  // The selected mask stays selected.
  const sel = mk.selected;
  await run([{ cmd: "move_mask", mask: from, to }]);
  if (sel == null) return;
  if (sel === from) selectMask(to);
  else if (from < sel && sel <= to) selectMask(sel - 1);
  else if (to <= sel && sel < from) selectMask(sel + 1);
}

function selectMask(i) {
  if (mk.selected !== i) mk.part = null;
  mk.selected = i;
  const m = selectedMask();
  if (m && mk.part == null && m.parts.length) mk.part = 0;
  stopTool(false);
  renderMaskPanel();
  requestPreview(false);
}

function selectPart(i) {
  mk.part = i;
  stopTool(false);
  renderMaskPanel();
}

// Start creating: a new mask (`fresh`) or a new part of the selected one;
// `continuePart` adds to the selected brush or colour part instead.
function startTool(kind, fresh, continuePart = false) {
  if (!mk.on) return;
  if (!fresh && mk.selected == null) fresh = true;
  if (["luminance", "sky", "subject", "background", "people"].includes(kind)) {
    // Nothing to draw: make it (then set a luminance range in the panel).
    const cmd = fresh
      ? { cmd: "add_mask", kind }
      : { cmd: "add_mask", kind, mask: mk.selected, mode: mk.addMode };
    created(cmd, fresh);
    return;
  }
  mk.tool = kind;
  mk.creatingNew = fresh;
  mk.continuePart = continuePart;
  $("mask-canvas").classList.toggle("painting", kind === "brush");
  renderMaskPanel();
  status(`Masks: ${NEW_KINDS.find((n) => n.kind === kind).hint}`);
}

function stopTool(redraw = true) {
  mk.tool = null;
  mk.creatingNew = false;
  mk.continuePart = false;
  mk.stroke = null;
  $("mask-canvas").classList.remove("painting");
  if (redraw) renderMaskPanel();
}

// Run a command that makes a mask or part, then select what it made.
async function created(cmd, fresh) {
  const before = maskList().length;
  const parts = selectedMask()?.parts.length ?? 0;
  mk.overlay = true;
  $("mask-overlay").checked = true;
  await run([cmd]);
  if (fresh) {
    if (maskList().length > before) {
      mk.selected = maskList().length - 1;
      mk.part = 0;
    }
  } else {
    const m = selectedMask();
    if (m && m.parts.length > parts) mk.part = m.parts.length - 1;
  }
  renderMaskPanel();
  requestPreview(false);
}

function hitHandle(px, py) {
  const p = selectedPart();
  if (!p || !mk.frame) return null;
  const near = ([x, y], r = 10) => Math.hypot(px - x, py - y) <= r;
  if (p.kind === "linear") {
    const h = linearHandles(p);
    if (near(h.full)) return "full";
    if (near(h.zero)) return "zero";
    // Anywhere along the line between them moves both.
    const [ax, ay] = h.full;
    const [bx, by] = h.zero;
    const L = Math.hypot(bx - ax, by - ay) || 1;
    const t = ((px - ax) * (bx - ax) + (py - ay) * (by - ay)) / (L * L);
    const d = Math.abs((px - ax) * (by - ay) - (py - ay) * (bx - ax)) / L;
    if (t > 0 && t < 1 && d < 8) return "move";
  } else if (p.kind === "radial") {
    const h = radialHandles(p);
    for (const k of ["a", "a2", "b", "b2"]) if (near(h[k], 8)) return k;
    if (near(h.center, 12)) return "move";
  }
  return null;
}

function wireMasks() {
  const c = $("mask-canvas");
  c.addEventListener("pointerdown", (e) => {
    if (e.button !== 0 || !mk.frame) return;
    e.preventDefault();
    c.setPointerCapture(e.pointerId);
    const r = c.getBoundingClientRect();
    const px = e.clientX - r.left;
    const py = e.clientY - r.top;
    const pt = toFrame(px, py);
    if (mk.tool === "brush") {
      mk.stroke = { points: [pt], erase: mk.brush.erase || e.altKey };
      drawMasks();
      return;
    }
    if (mk.tool === "color") {
      const p = selectedPart();
      if (!mk.creatingNew && p?.kind === "color" && (mk.continuePart || e.shiftKey)) {
        run([{ cmd: "set_mask", mask: mk.selected, part: p.index, values: { ColorSamples: [...(p.ColorSamples ?? []), pt] } }]);
        if (!e.shiftKey) stopTool();
      } else {
        const cmd = { cmd: "add_mask", kind: "color", values: { ColorSamples: [pt] } };
        if (!mk.creatingNew) Object.assign(cmd, { mask: mk.selected, mode: mk.addMode });
        const fresh = mk.creatingNew;
        stopTool(false);
        created(cmd, fresh);
      }
      return;
    }
    if (mk.tool === "linear") {
      mk.drag = { kind: "new-linear", start: pt, part: null };
      return;
    }
    if (mk.tool === "radial") {
      mk.drag = { kind: "new-radial", start: pt, startPx: [px, py], part: null };
      return;
    }
    const handle = hitHandle(px, py);
    if (handle) {
      const p = selectedPart();
      mk.drag = { kind: "handle", handle, start: pt, orig: { ...p }, part: { ...p } };
    }
  });
  c.addEventListener("pointermove", (e) => {
    const r = c.getBoundingClientRect();
    const px = e.clientX - r.left;
    const py = e.clientY - r.top;
    if (mk.tool === "brush") {
      mk.hover = [px, py];
      mk.hover.alt = e.altKey;
    }
    if (!mk.frame) return;
    const pt = toFrame(px, py);
    if (mk.stroke) {
      mk.stroke.points.push(pt);
      drawMasks();
      return;
    }
    const d = mk.drag;
    if (!d) {
      c.style.cursor = mk.tool ? "crosshair" : hitHandle(px, py) ? "move" : "default";
      if (mk.tool === "brush") drawMasks();
      return;
    }
    if (d.kind === "new-linear") {
      // Full where the drag started, none where it ends.
      d.part = { kind: "linear", FullX: d.start[0], FullY: d.start[1], ZeroX: pt[0], ZeroY: pt[1] };
    } else if (d.kind === "new-radial") {
      // Centre at the start; the box is the drag's extent on screen.
      const [sx, sy] = d.startPx;
      const corner = toFrame(sx + Math.abs(px - sx), sy + Math.abs(py - sy));
      const hx = Math.abs(corner[0] - d.start[0]);
      const hy = Math.abs(corner[1] - d.start[1]);
      const [ox, oy] = toFrame(sx + Math.abs(px - sx), sy - Math.abs(py - sy));
      const hx2 = Math.max(hx, Math.abs(ox - d.start[0]));
      const hy2 = Math.max(hy, Math.abs(oy - d.start[1]));
      d.part = { kind: "radial", Left: d.start[0] - hx2, Right: d.start[0] + hx2, Top: d.start[1] - hy2, Bottom: d.start[1] + hy2, Angle: 0, Feather: 50 };
    } else if (d.kind === "handle") {
      const o = d.orig;
      const dx = pt[0] - d.start[0];
      const dy = pt[1] - d.start[1];
      const p = { ...o };
      if (o.kind === "linear") {
        if (d.handle !== "zero") { p.FullX = o.FullX + dx; p.FullY = o.FullY + dy; }
        if (d.handle !== "full") { p.ZeroX = o.ZeroX + dx; p.ZeroY = o.ZeroY + dy; }
      } else if (d.handle === "move") {
        p.Left = o.Left + dx; p.Right = o.Right + dx; p.Top = o.Top + dy; p.Bottom = o.Bottom + dy;
      } else {
        // An axis handle: that half-axis follows the pointer, measured along it.
        const W = mk.frame.frame_width;
        const H = mk.frame.frame_height;
        const cx = (o.Left + o.Right) / 2;
        const cy = (o.Top + o.Bottom) / 2;
        const t = ((o.Angle ?? 0) * Math.PI) / 180;
        const qx = (pt[0] - cx) * W;
        const qy = (pt[1] - cy) * H;
        const u = qx * Math.cos(t) - qy * Math.sin(t);
        const v = qx * Math.sin(t) + qy * Math.cos(t);
        if (d.handle === "a" || d.handle === "a2") {
          const a = Math.max(4, Math.abs(u)) / W;
          p.Left = cx - a; p.Right = cx + a;
        } else {
          const b = Math.max(4, Math.abs(v)) / H;
          p.Top = cy - b; p.Bottom = cy + b;
        }
      }
      d.part = p;
      const keys = o.kind === "linear" ? ["ZeroX", "ZeroY", "FullX", "FullY"] : ["Left", "Right", "Top", "Bottom"];
      const cs = mk.draft ?? correctionsCopy();
      const [entry] = partEntries(cs, mk.selected, o.index);
      if (entry) {
        for (const k of keys) entry[k] = fmt6(p[k]);
        previewDraft(cs);
      }
    }
    drawMasks();
  });
  c.addEventListener("pointerleave", () => {
    mk.hover = null;
    drawMasks();
  });
  const finish = () => {
    if (mk.stroke) {
      const s = mk.stroke;
      mk.stroke = null;
      const p = selectedPart();
      const cmd = {
        cmd: "paint_mask",
        points: s.points,
        size: mk.brush.size / 100,
        feather: mk.brush.feather,
        flow: mk.brush.flow,
        density: mk.brush.density,
      };
      if (s.erase) cmd.erase = true;
      if (!mk.creatingNew && mk.selected != null) {
        cmd.mask = mk.selected;
        // Keep painting into the brush part being painted.
        if (p?.kind === "brush" && (mk.continuePart || mk.paintedPart === p.index)) cmd.part = p.index;
        else if (s.erase) {
          toast("Pick a brush part to erase from", true);
          drawMasks();
          return;
        } else cmd.mode = mk.addMode;
      } else if (s.erase) {
        toast("Paint something first, then erase", true);
        drawMasks();
        return;
      }
      const fresh = mk.creatingNew || mk.selected == null;
      const hadPart = cmd.part != null;
      created(cmd, fresh).then(() => {
        // Later strokes go into the same brush part.
        mk.creatingNew = false;
        mk.continuePart = true;
        if (!hadPart) mk.paintedPart = mk.part;
        renderMaskPanel();
      });
      return;
    }
    const d = mk.drag;
    mk.drag = null;
    if (!d) return;
    if (d.kind === "new-linear" || d.kind === "new-radial") {
      const p = d.part;
      if (!p) return;
      const values = d.kind === "new-linear"
        ? { ZeroX: p.ZeroX, ZeroY: p.ZeroY, FullX: p.FullX, FullY: p.FullY }
        : { Top: p.Top, Left: p.Left, Bottom: p.Bottom, Right: p.Right };
      const tiny = d.kind === "new-linear"
        ? Math.hypot(p.ZeroX - p.FullX, p.ZeroY - p.FullY) < 0.005
        : p.Right - p.Left < 0.005 || p.Bottom - p.Top < 0.005;
      if (tiny) {
        drawMasks();
        return;
      }
      const cmd = { cmd: "add_mask", kind: d.kind === "new-linear" ? "linear" : "radial", values };
      if (!mk.creatingNew) Object.assign(cmd, { mask: mk.selected, mode: mk.addMode });
      const fresh = mk.creatingNew;
      stopTool(false);
      created(cmd, fresh);
    } else if (d.kind === "handle") {
      const p = d.part;
      const keys = p.kind === "linear" ? ["ZeroX", "ZeroY", "FullX", "FullY"] : ["Left", "Right", "Top", "Bottom"];
      const values = Object.fromEntries(keys.map((k) => [k, p[k]]));
      endDraft();
      run([{ cmd: "set_mask", mask: mk.selected, part: p.index, values }]);
    }
  };
  c.addEventListener("pointerup", finish);
  c.addEventListener("pointercancel", () => {
    mk.drag = mk.stroke = null;
    endDraft();
    drawMasks();
  });
  c.addEventListener("wheel", (e) => {
    if (mk.tool !== "brush") return;
    e.preventDefault();
    brushSize(e.deltaY < 0 ? 1 : -1);
  }, { passive: false });
  $("mask-overlay").addEventListener("change", () => {
    mk.overlay = $("mask-overlay").checked;
    requestPreview(false);
  });
  $("mask-done").addEventListener("click", () => exitMasks());
  $("preview").addEventListener("load", () => drawMasks());
  new ResizeObserver(() => drawMasks()).observe($("viewer"));
}

function brushSize(dir) {
  const b = mk.brush;
  b.size = Math.min(25, Math.max(0.5, b.size + dir * (b.size < 3 ? 0.5 : 1)));
  renderToolOptions();
  drawMasks();
}

// ---------------------------------------------------------------- enter / leave

async function loadMaskFrame() {
  try {
    mk.frame = await invoke("mask_frame");
  } catch (e) {
    mk.frame = null;
    toast(String(e), true);
  }
}

async function enterMasks() {
  if (!ui.info || mk.on) return;
  if (ui.crop.on) await exitCrop(true);
  exitSpots();
  exitGuided();
  if (ui.zoom.on) setZoom(false);
  setPicking(false);
  if (!mk.sliders.length) {
    try {
      mk.sliders = await invoke("mask_sliders");
    } catch (e) {
      toast(String(e), true);
    }
  }
  if (!mk.effects) mk.effects = await invoke("mask_effects").catch(() => ({ builtin: [], mine: [] }));
  await loadMaskFrame();
  mk.on = true;
  mk.overlay = $("mask-overlay").checked;
  if (mk.selected == null && maskList().length) selectMask(0);
  $("mask-canvas").hidden = false;
  $("mask-panel").hidden = false;
  $("btn-masks").classList.toggle("active", true);
  for (const id of ["btn-zoom", "btn-wb"]) $(id).disabled = true;
  renderMaskPanel();
  requestPreview(false);
}

function exitMasks() {
  if (!mk.on) return;
  stopTool(false);
  endDraft();
  mk.on = false;
  mk.drag = null;
  $("mask-canvas").hidden = true;
  $("mask-panel").hidden = true;
  $("btn-masks").classList.toggle("active", false);
  for (const id of ["btn-zoom", "btn-wb"]) $(id).disabled = !ui.info;
  status("Ready");
  requestPreview(false);
}

function toggleMasks() {
  if (mk.on) exitMasks();
  else enterMasks();
}

// Esc: stop the tool first, then leave.
function maskEscape() {
  if (mk.tool) stopTool();
  else exitMasks();
}

function toggleMaskOverlay() {
  mk.overlay = !mk.overlay;
  $("mask-overlay").checked = mk.overlay;
  requestPreview(false);
}

function deleteMaskPart() {
  const m = selectedMask();
  if (!m) return;
  if (mk.part != null && m.parts.length > 1) run([{ cmd: "remove_mask", mask: m.index, part: mk.part }]);
  else run([{ cmd: "remove_mask", mask: m.index }]);
}

// The state changed (any command, undo, redo, a new photo).
function masksStateChanged() {
  if (!mk.on) return;
  endDraft();
  loadMaskFrame().then(() => renderMaskPanel());
}

// A different photo: start over.
function masksPhotoChanged() {
  mk.selected = null;
  mk.part = null;
  mk.paintedPart = null;
  if (mk.on) exitMasks();
}
