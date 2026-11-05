// SPDX-License-Identifier: GPL-3.0-or-later
// Select Object (O): click a person, a car, the sky; the object selection
// model (SAM 2.1, Settings → AI models) outlines it, and the panel's
// buttons act on it: Adjust (a mask of it, its sliders in Masks), Brighten,
// Darken, Soften the background, Remove (one AI Remove spot) or Move (drag
// it: a clone at the new place, removed where it was). Shift-click adds
// more of it, Alt-click takes a part away; a drag draws a box round it.
// A shadow it casts shows in blue, and "Its shadow too" takes it along.
// Everything it makes is an ordinary mask or spot, one undo step each.
"use strict";

const sl = {
  on: false,
  frame: null,
  clicks: [],      // [x, y, 1 on it | 0 not it], frame fractions
  dabs: null,      // the outline: [x, y, radius] frame fractions
  shadowDabs: [],  // its shadow, when one was found (shown in blue, offered)
  shadow: false,   // take its shadow too (kept for the next selection)
  busy: false,
  drag: null,      // { start, path, box } while the pointer is down
  moving: false,   // Move: the next drag moves the outline
  offset: [0, 0],  // the outline's offset while it is dragged (pixels)
};

const slCanvas = (x, y) => toCanvas(x, y, sl.frame, "select-canvas");
const slFrame = (px, py) => toFrame(px, py, sl.frame, "select-canvas");

function drawSelect() {
  const c = $("select-canvas");
  if (!sl.on || c.hidden) return;
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
  if (!sl.frame) return;
  if (sl.dabs) {
    // The object, tinted: its dabs filled on a layer, then shown at once.
    const layer = document.createElement("canvas");
    layer.width = c.width;
    layer.height = c.height;
    const lg = layer.getContext("2d");
    lg.setTransform(dpr, 0, 0, dpr, 0, 0);
    const scale = sl.frame.frame_width * pxScale(sl.frame, "select-canvas");
    const [ox, oy] = sl.offset;
    const paint = (dabs, colour) => {
      lg.fillStyle = colour;
      lg.beginPath();
      for (const [x, y, r] of dabs) {
        const [px, py] = slCanvas(x, y);
        lg.moveTo(px + ox + r * scale, py + oy);
        lg.arc(px + ox, py + oy, Math.max(0.5, r * scale), 0, 7);
      }
      lg.fill();
    };
    paint(sl.dabs, "#ff3d7f");
    // Its shadow, offered (blue) until it's taken (then in the outline).
    if (!sl.shadow && sl.shadowDabs.length && !sl.moving) paint(sl.shadowDabs, "#3d8bff");
    g.save();
    g.setTransform(1, 0, 0, 1, 0, 0);
    g.globalAlpha = 0.45;
    g.drawImage(layer, 0, 0);
    g.restore();
  }
  for (const [x, y, on] of sl.clicks) {
    const [px, py] = slCanvas(x, y);
    g.beginPath();
    g.arc(px, py, 5, 0, 7);
    g.fillStyle = on ? "#fff" : "#222";
    g.strokeStyle = on ? "#222" : "#fff";
    g.lineWidth = 2;
    g.fill();
    g.stroke();
  }
  const d = sl.drag;
  if (d?.box) {
    const [x0, y0] = d.start;
    const [x1, y1] = d.path.at(-1);
    g.setLineDash([6, 4]);
    g.strokeStyle = "rgba(255,255,255,0.9)";
    g.lineWidth = 2;
    g.strokeRect(Math.min(x0, x1), Math.min(y0, y1), Math.abs(x1 - x0), Math.abs(y1 - y0));
    g.setLineDash([]);
  }
}

function renderSelectPanel() {
  const have = Boolean(sl.dabs);
  for (const b of document.querySelectorAll("#select-actions button")) b.disabled = !have || sl.busy;
  $("select-clear").disabled = !have && !sl.clicks.length;
  $("select-shadow-row").hidden = !have || !sl.shadowDabs.length;
  $("select-shadow").checked = sl.shadow;
  $("select-shadow").disabled = sl.busy;
  $("select-move").classList.toggle("active", sl.moving);
  $("select-hint").textContent = sl.moving
    ? "Drag the outline to where it should go."
    : have
      ? sl.shadowDabs.length && !sl.shadow
        ? "Its shadow is in blue: tick “Its shadow too” to take it along. Shift-click adds more, Alt-click takes a part away."
        : "Shift-click adds more of it, Alt-click takes a part away. Then pick what to do with it."
      : "Click the object to select it, or drag a box round it. Shift-click adds, Alt-click takes away.";
  drawSelect();
}

// Ask the model for the object under the clicks (or in a box).
async function selectAsk({ rect = null, part = false } = {}) {
  sl.busy = true;
  $("busy").hidden = false;
  status("Finding the object…");
  renderSelectPanel();
  try {
    const r = await invoke("select_object", { clicks: sl.clicks, part, rect, shadow: sl.shadow });
    gotSelection(r);
    status(`Selected (${Math.round(r.share * 1000) / 10} % of the ${rect ? "box" : "photo"})`);
  } catch (e) {
    toast(String(e), true);
    if (!sl.dabs) sl.clicks = [];
  } finally {
    sl.busy = false;
    $("busy").hidden = true;
    renderSelectPanel();
  }
}

function gotSelection(r) {
  sl.dabs = r.dabs;
  sl.shadowDabs = r.shadow_dabs ?? [];
}

// Take its shadow too, or not (no need to ask the model again).
async function setSelectShadow(on) {
  sl.shadow = on;
  if (!sl.dabs || sl.busy) return renderSelectPanel();
  sl.busy = true;
  renderSelectPanel();
  try {
    gotSelection(await invoke("selection_shadow", { on }));
  } catch (e) {
    toast(String(e), true);
  } finally {
    sl.busy = false;
    renderSelectPanel();
  }
}

function selectClear() {
  sl.clicks = [];
  sl.dabs = null;
  sl.shadowDabs = [];
  sl.moving = false;
  sl.offset = [0, 0];
  renderSelectPanel();
}

// An action on the selection, as one undo step.
async function selectApply(action, by = null) {
  if (!sl.dabs || sl.busy) return;
  sl.busy = true;
  renderSelectPanel();
  try {
    const r = await invoke("selection_apply", { action, by });
    setState(r.state);
    requestPreview(false);
    toast(r.notes.join(" "));
    if (["adjust", "brighten", "darken", "background"].includes(action)) {
      // Its mask, open with its sliders.
      exitSelect();
      await enterMasks();
      if (mk.on && maskList().length) selectMask(maskList().length - 1);
      return;
    }
    selectClear();
  } catch (e) {
    toast(String(e), true);
  } finally {
    sl.busy = false;
    if (sl.on) renderSelectPanel();
  }
}

async function enterSelect() {
  if (!ui.info || sl.on) return;
  if (ui.crop.on) await exitCrop(true);
  exitMasks();
  exitSpots();
  setPicking(false);
  try {
    sl.frame = await invoke("mask_frame");
  } catch (e) {
    toast(String(e), true);
    return;
  }
  sl.on = true;
  $("select-canvas").hidden = false;
  $("select-panel").hidden = false;
  $("btn-wb").disabled = true;
  status("Select object: click it");
  selectClear();
}

function exitSelect() {
  if (!sl.on) return;
  sl.on = false;
  sl.drag = null;
  $("select-canvas").hidden = true;
  $("select-panel").hidden = true;
  $("btn-wb").disabled = !ui.info;
  status("Ready");
}

function toggleSelect() {
  if (sl.on) exitSelect();
  else enterSelect();
}

function wireSelect() {
  const c = $("select-canvas");
  const at = (e) => {
    const r = c.getBoundingClientRect();
    return [e.clientX - r.left, e.clientY - r.top];
  };
  c.addEventListener("pointerdown", (e) => {
    if (e.button !== 0 || !sl.frame || sl.busy) return;
    e.preventDefault();
    try { c.setPointerCapture(e.pointerId); } catch { /* a synthetic pointer */ }
    const p = at(e);
    sl.drag = { start: p, path: [p], shift: e.shiftKey, alt: e.altKey, box: false };
  });
  c.addEventListener("pointermove", (e) => {
    const d = sl.drag;
    if (!d) return;
    const p = at(e);
    d.path.push(p);
    const far = Math.hypot(p[0] - d.start[0], p[1] - d.start[1]);
    if (sl.moving) {
      sl.offset = [p[0] - d.start[0], p[1] - d.start[1]];
    } else if (far > 8) {
      d.box = true;
    }
    drawSelect();
  });
  c.addEventListener("pointerup", (e) => {
    const d = sl.drag;
    sl.drag = null;
    if (!d) return;
    const p = at(e);
    if (sl.moving) {
      const [a, b] = [slFrame(...d.start), slFrame(...p)];
      sl.moving = false;
      sl.offset = [0, 0];
      if (Math.hypot(p[0] - d.start[0], p[1] - d.start[1]) > 4) selectApply("move", [b[0] - a[0], b[1] - a[1]]);
      else renderSelectPanel();
      return;
    }
    if (d.box) {
      const [a, b] = [slFrame(...d.start), slFrame(...p)];
      sl.clicks = [];
      selectAsk({ rect: [Math.min(a[0], b[0]), Math.min(a[1], b[1]), Math.max(a[0], b[0]), Math.max(a[1], b[1])] });
      return;
    }
    const pt = slFrame(...p);
    if (d.alt && sl.dabs) sl.clicks.push([pt[0], pt[1], 0]);
    else if (d.shift && sl.dabs) sl.clicks.push([pt[0], pt[1], 1]);
    else sl.clicks = [[pt[0], pt[1], 1]];
    selectAsk({ part: d.alt && !sl.dabs });
  });
  c.addEventListener("wheel", (e) => {
    // Ctrl + scroll zooms (the viewer's).
    if (!(e.ctrlKey || e.metaKey)) e.preventDefault();
  }, { passive: false });
  for (const b of document.querySelectorAll("#select-actions button[data-action]")) {
    b.addEventListener("click", () => selectApply(b.dataset.action));
  }
  $("select-move").addEventListener("click", () => {
    sl.moving = !sl.moving;
    renderSelectPanel();
  });
  $("select-clear").addEventListener("click", selectClear);
  $("select-shadow").addEventListener("change", () => setSelectShadow($("select-shadow").checked));
  $("select-done").addEventListener("click", exitSelect);
  $("preview").addEventListener("load", () => drawSelect());
  new ResizeObserver(() => drawSelect()).observe($("viewer"));
}
