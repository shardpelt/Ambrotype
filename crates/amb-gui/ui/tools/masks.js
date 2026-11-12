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
    // Ctrl + scroll zooms (the viewer's).
    if (mk.tool !== "brush" || e.ctrlKey || e.metaKey) return;
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
    showError(e);
  }
}

async function enterMasks() {
  if (!ui.info || mk.on) return;
  if (typeof exitSelect === "function") exitSelect();
  if (ui.crop.on) await exitCrop(true);
  exitSpots();
  exitGuided();
  setPicking(false);
  if (!mk.sliders.length) {
    try {
      mk.sliders = await invoke("mask_sliders");
    } catch (e) {
      showError(e);
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
  // Zoom stays: retouching wants a close look.
  $("btn-wb").disabled = true;
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
  $("btn-wb").disabled = !ui.info;
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
