// SPDX-License-Identifier: GPL-3.0-or-later
// Speed Edit: hold Ctrl and drag left or right on the photo to change the
// slider you moved last, without going back to its panel (Shift: a tenth as
// fast). Touch (T) picks the setting from the colour under the pointer; this
// always changes the last slider. One drag is one undo step.
"use strict";

// Pixels dragged to cover a slider's whole range.
const SPEED_PX_FULL = 400;

const speed = { last: null, drag: null };

// rows.js calls this whenever a slider or its number box is changed.
function speedNote(name) {
  speed.last = name;
}

function speedHint() {
  toast(t("Move a slider first; Ctrl+drag on the photo then changes it"));
}

// The setting's position on its slider, and the slider's ends (a log slider
// runs 0 to 1000, as rows.js builds it).
function speedPos(def, v) {
  return isLog(def) ? toSlider(def, v) : v;
}

function speedValue(def, pos) {
  if (isLog(def)) return fromSlider(def, pos);
  return Number((Math.round(pos / def.step) * def.step).toFixed(4));
}

function speedMove(e) {
  const d = speed.drag;
  const rate = e.shiftKey ? 0.1 : 1;
  if (rate !== d.rate) Object.assign(d, { x: e.clientX, from: d.pos, rate });
  const [lo, hi] = isLog(d.def) ? [0, 1000] : [d.def.min, d.def.max];
  d.pos = Math.min(hi, Math.max(lo, d.from + ((e.clientX - d.x) * (hi - lo) * rate) / SPEED_PX_FULL));
  const v = speedValue(d.def, d.pos);
  d.value = v;
  ui.overrides[d.def.name] = v;
  const row = ui.controls.get(d.def.name);
  if (row?.number) row.number.value = format(d.def, v);
  if (row?.input) row.input.value = String(speedPos(d.def, v));
  status(t("{label} {value}", { label: t(d.def.label), value: format(d.def, v) }), true);
  requestPreview(true);
}

function speedEnd() {
  const d = speed.drag;
  speed.drag = null;
  if (d.value === undefined || d.value === d.start) {
    delete ui.overrides[d.def.name];
    requestPreview(false);
  } else {
    commit(d.def.name, d.value);
  }
  status(t("Ready"));
}

function wireSpeedEdit() {
  // Before the photo's own handlers (click to zoom, drag to pan).
  $("viewer").addEventListener("mousedown", (e) => {
    if (e.button !== 0 || !e.ctrlKey || e.target !== $("preview") || !ui.info || touchState.on || ui.picking) return;
    e.preventDefault();
    e.stopPropagation();
    const def = speed.last && ui.defs.get(speed.last);
    if (!def) return speedHint();
    const start = Number(currentValue(def.name)) || 0;
    speed.drag = { def, x: e.clientX, start, from: speedPos(def, start), pos: speedPos(def, start), rate: 1 };
  }, true);
  window.addEventListener("mousemove", (e) => {
    if (speed.drag) speedMove(e);
  });
  window.addEventListener("mouseup", (e) => {
    if (speed.drag && e.button === 0) speedEnd();
  });
}
