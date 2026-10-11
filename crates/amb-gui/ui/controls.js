// SPDX-License-Identifier: GPL-3.0-or-later
// Other ways to show one setting, chosen per setting by the layout
// (layout.js). The default slider row is makeRow() in app.js. Each maker
// returns { el, entry }; `entry` goes into ui.controls like makeRow's, with
// set(value) so syncControls() can update it.
//
//   number settings: knob (drag up/down), bar (drag across), number (type, or drag the label)
//   bool settings:   toggle (a switch)
//   choices:         segmented (a row of buttons)
"use strict";

const AmbControls = (() => {
  const clamp = (v, lo, hi) => Math.min(hi, Math.max(lo, v));
  // Positions are in the slider's domain (Temperature is log-scaled: 0..1000).
  const domain = (def) => (isLog(def) ? [0, 1000] : [def.min, def.max]);
  function valueAt(def, pos) {
    if (isLog(def)) return fromSlider(def, Math.round(pos));
    return Number((Math.round(pos / def.step) * def.step).toFixed(6));
  }
  // Where the "unchanged" end of a fill starts: 0 when in range, else the minimum.
  const origin = (def) => (isLog(def) ? 0 : clamp(0, def.min, def.max));

  function hooks(def) {
    return {
      input(v) { ui.overrides[def.name] = v; requestPreview(true); },
      commit(v) { commit(def.name, v); },
      reset() { run([{ cmd: "reset_param", name: def.name }]); },
    };
  }

  // Pointer drag with capture: move(dx, dy, event), end(moved).
  function drag(el, { start, move, end }) {
    el.addEventListener("pointerdown", (e) => {
      if (e.button !== 0) return;
      e.preventDefault();
      el.setPointerCapture(e.pointerId);
      const x0 = e.clientX, y0 = e.clientY;
      let moved = false;
      start?.(e);
      const onMove = (ev) => { moved = true; move(ev.clientX - x0, ev.clientY - y0, ev); };
      const onUp = () => {
        el.removeEventListener("pointermove", onMove);
        el.removeEventListener("pointerup", onUp);
        el.removeEventListener("pointercancel", onUp);
        end?.(moved);
      };
      el.addEventListener("pointermove", onMove);
      el.addEventListener("pointerup", onUp);
      el.addEventListener("pointercancel", onUp);
    });
  }

  // Arrow keys step the value (Shift: ×10) and commit.
  function keys(el, def, get, setPos, h) {
    el.tabIndex = 0;
    el.addEventListener("keydown", (e) => {
      const dir = { ArrowUp: 1, ArrowRight: 1, ArrowDown: -1, ArrowLeft: -1 }[e.key];
      if (!dir) return;
      e.preventDefault();
      e.stopPropagation();
      const [lo, hi] = domain(def);
      const step = (isLog(def) ? 5 : def.step) * (e.shiftKey ? 10 : 1);
      setPos(clamp(get() + dir * step, lo, hi));
      h.commit(valueAt(def, get()));
    });
  }

  function knob(def, labelText) {
    const h = hooks(def);
    const [lo, hi] = domain(def);
    const span = hi - lo || 1;
    const cell = document.createElement("div");
    cell.className = "knob";
    cell.title = `${def.description}\nDrag up or down (Shift: fine). Double-click to reset.`;
    const NS = "http://www.w3.org/2000/svg";
    const svg = document.createElementNS(NS, "svg");
    svg.setAttribute("viewBox", "0 0 40 40");
    const path = (cls) => {
      const p = document.createElementNS(NS, "path");
      p.setAttribute("class", cls);
      svg.append(p);
      return p;
    };
    const track = path("knob-track"), fill = path("knob-fill"), tick = path("knob-tick");
    const label = document.createElement("div");
    label.className = "knob-label";
    label.textContent = labelText;
    const value = document.createElement("div");
    value.className = "knob-value";
    cell.append(svg, label, value);

    // A 270° dial from bottom-left round to bottom-right (SVG angles, y down).
    const A0 = 135, SWEEP = 270, R = 15;
    const at = (deg, r) => [20 + r * Math.cos((deg * Math.PI) / 180), 20 + r * Math.sin((deg * Math.PI) / 180)];
    const arc = (d0, d1) => {
      if (d1 < d0) [d0, d1] = [d1, d0];
      const [x0, y0] = at(d0, R), [x1, y1] = at(d1, R);
      return `M${x0.toFixed(2)} ${y0.toFixed(2)} A${R} ${R} 0 ${d1 - d0 > 180 ? 1 : 0} 1 ${x1.toFixed(2)} ${y1.toFixed(2)}`;
    };
    track.setAttribute("d", arc(A0, A0 + SWEEP - 0.01));
    const zero = A0 + (SWEEP * (origin(def) - lo)) / span;
    let pos = lo, val = 0; // dial position, and the value it stands for
    function draw() {
      const a = A0 + (SWEEP * (pos - lo)) / span;
      fill.setAttribute("d", Math.abs(a - zero) < 0.5 ? "" : arc(zero, a));
      const [x0, y0] = at(a, 5), [x1, y1] = at(a, 12);
      tick.setAttribute("d", `M${x0.toFixed(2)} ${y0.toFixed(2)} L${x1.toFixed(2)} ${y1.toFixed(2)}`);
      value.textContent = format(def, val);
    }
    let startPos = lo;
    drag(svg, {
      start() { startPos = pos; cell.classList.add("dragging"); },
      move(dx, dy, e) {
        pos = clamp(startPos - (dy / (e.shiftKey ? 2500 : 250)) * span, lo, hi);
        val = valueAt(def, pos);
        draw();
        h.input(val);
      },
      end(moved) {
        cell.classList.remove("dragging");
        if (moved) h.commit(valueAt(def, pos));
      },
    });
    svg.addEventListener("dblclick", h.reset);
    label.addEventListener("dblclick", h.reset);
    holdWhileDragging(svg, def.name);
    keys(svg, def, () => pos, (p) => { pos = p; val = valueAt(def, p); draw(); }, h);
    draw();
    const set = (v) => { val = Number(v); pos = clamp(toSlider(def, val), lo, hi); draw(); };
    return { el: cell, entry: { def, label, set } };
  }

  // A darktable-style bar: label and value inside, the fill shows the amount.
  function bar(def, labelText) {
    const h = hooks(def);
    const [lo, hi] = domain(def);
    const span = hi - lo || 1;
    const el = document.createElement("div");
    el.className = "bar";
    el.title = `${def.description}\nDrag across (Shift: fine). Double-click to reset.`;
    const fill = document.createElement("div");
    fill.className = "bar-fill";
    const label = document.createElement("span");
    label.className = "bar-label";
    label.textContent = labelText;
    const value = document.createElement("span");
    value.className = "bar-value";
    el.append(fill, label, value);
    const z = (origin(def) - lo) / span;
    let pos = lo, val = 0;
    function draw() {
      const t = (pos - lo) / span;
      fill.style.left = `${Math.min(t, z) * 100}%`;
      fill.style.width = `${Math.abs(t - z) * 100}%`;
      value.textContent = format(def, val);
    }
    let startPos = lo;
    drag(el, {
      start(e) {
        // A plain click jumps to the spot; Shift-drag refines from the current value.
        const r = el.getBoundingClientRect();
        if (!e.shiftKey) pos = clamp(lo + ((e.clientX - r.left) / r.width) * span, lo, hi);
        startPos = pos;
        val = valueAt(def, pos);
        el.classList.add("dragging");
        draw();
        h.input(val);
      },
      move(dx, _dy, e) {
        const w = el.getBoundingClientRect().width || 1;
        pos = clamp(startPos + (dx / w) * span * (e.shiftKey ? 0.1 : 1), lo, hi);
        val = valueAt(def, pos);
        draw();
        h.input(val);
      },
      end() {
        el.classList.remove("dragging");
        h.commit(val);
      },
    });
    resetOnDoubleClick(el, () => pos, span, h.reset);
    holdWhileDragging(el, def.name);
    keys(el, def, () => pos, (p) => { pos = p; val = valueAt(def, p); draw(); }, h);
    draw();
    const set = (v) => { val = Number(v); pos = clamp(toSlider(def, val), lo, hi); draw(); };
    return { el, entry: { def, label, set } };
  }

  // A number box; drag the label left or right to scrub.
  function number(def, labelText) {
    const h = hooks(def);
    const [lo, hi] = domain(def);
    const row = document.createElement("div");
    row.className = "row number-row";
    const label = document.createElement("label");
    label.className = "scrub";
    label.textContent = labelText;
    label.title = `${def.description}\nType a value, or drag this label sideways. Double-click to reset.`;
    const box = document.createElement("input");
    box.type = "text";
    box.inputMode = "decimal";
    box.className = "value";
    box.spellcheck = false;
    row.append(label, box);
    let pos = lo, val = 0;
    const show = () => (box.value = format(def, val));
    let startPos = lo;
    drag(label, {
      start() { startPos = pos; },
      move(dx, _dy, e) {
        pos = clamp(startPos + (dx / (e.shiftKey ? 2000 : 200)) * (hi - lo), lo, hi);
        val = valueAt(def, pos);
        show();
        h.input(val);
      },
      end(moved) { if (moved) h.commit(val); },
    });
    box.addEventListener("change", () => {
      const v = Number(box.value);
      if (!Number.isNaN(v)) h.commit(clamp(v, def.min, def.max));
    });
    selectOnFocus(box);
    holdWhileDragging(label, def.name);
    label.addEventListener("dblclick", h.reset);
    const set = (v) => { val = Number(v); pos = clamp(toSlider(def, val), lo, hi); show(); };
    return { el: row, entry: { def, label, set } };
  }

  function toggle(def, labelText) {
    const row = document.createElement("div");
    row.className = "row check";
    const label = document.createElement("label");
    label.className = "toggle";
    label.title = def.description;
    const check = document.createElement("input");
    check.type = "checkbox";
    check.setAttribute("role", "switch");
    check.addEventListener("change", () => commit(def.name, check.checked));
    label.append(document.createTextNode(labelText), check);
    row.append(label);
    return { el: row, entry: { def, check, label } };
  }

  function segmented(def, labelText) {
    const row = document.createElement("div");
    row.className = "row segmented-row";
    const label = document.createElement("label");
    label.textContent = labelText;
    label.title = def.description;
    const group = document.createElement("div");
    group.className = "segmented";
    group.setAttribute("role", "radiogroup");
    const buttons = def.options.map(([value, text]) => {
      const b = document.createElement("button");
      b.type = "button";
      b.textContent = text;
      b.setAttribute("role", "radio");
      b.addEventListener("click", () => commit(def.name, value));
      group.append(b);
      return [JSON.stringify(value), b];
    });
    row.append(label, group);
    const set = (v) => {
      const key = JSON.stringify(typeof v === "string" && /^\d+$/.test(v) ? Number(v) : v);
      for (const [k, b] of buttons) {
        b.classList.toggle("active", k === key);
        b.setAttribute("aria-checked", String(k === key));
      }
    };
    return { el: row, entry: { def, label, set } };
  }

  const MAKERS = { knob, bar, number, toggle, segmented };

  // The widgets each kind of setting can use; the first is the default.
  const WIDGETS = {
    number: ["slider", "knob", "bar", "number"],
    bool: ["checkbox", "toggle"],
    choice: ["select", "segmented"],
  };

  return { MAKERS, WIDGETS };
})();
