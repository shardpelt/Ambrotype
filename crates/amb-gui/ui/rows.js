// SPDX-License-Identifier: GPL-3.0-or-later
// The inspector's slider rows: the colour tracks and the row builders. Split
// out of app.js.
"use strict";

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
  row.dataset.name = def.name;
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
  addHelpMark(entry, def);
  return row;
}
