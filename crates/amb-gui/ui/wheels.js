// SPDX-License-Identifier: GPL-3.0-or-later
// Colour-grading wheels widget (Lightroom "Color Grading"). Plain JS, no build step.
//
//   const wheels = AmbWheels.create(container, { onInput(changes), onCommit(changes) });
//   wheels.set({ ColorGradeShadowHue: 210, ColorGradeShadowSat: 35 });   // missing = 0
//
// Each wheel edits ColorGrade{Shadow|Midtone|Highlight|Global}{Hue|Sat|Lum}:
// hue 0..359 is the angle (0 = red pointing right, increasing counter-clockwise:
// yellow 60, green 120, cyan 180, blue 240, magenta 300 — the same HSV hues as
// the app's rainbow hue slider), saturation 0..100 is the distance from the
// centre, luminance −100..100 is the slider under the wheel.
"use strict";

(function () {
  const REGIONS = [
    { key: "Midtone", label: "Midtones", short: "Mid" },
    { key: "Shadow", label: "Shadows", short: "Shadows" },
    { key: "Highlight", label: "Highlights", short: "Highs" },
    { key: "Global", label: "Global", short: "Global" },
  ];
  const VIEWS = [
    { key: "3way", label: "3-Way", title: "Shadows, midtones and highlights together" },
    { key: "Shadow", label: "Shadows", title: "Shadows" },
    { key: "Midtone", label: "Mid", title: "Midtones" },
    { key: "Highlight", label: "Highs", title: "Highlights" },
    { key: "Global", label: "Global", title: "Global" },
  ];
  const TAGS = REGIONS.flatMap((r) => ["Hue", "Sat", "Lum"].map((k) => `ColorGrade${r.key}${k}`));
  const RAINBOW = "linear-gradient(90deg,#e04848,#e6d33a,#4fbf4f,#3fc6c6,#4170e0,#d64fb5,#e04848)";
  const LUM_TRACK = "linear-gradient(90deg,#111,#888,#eee)";
  const WHEEL_VALUE = 0.8;  // HSV value of the painted wheel: bright but not glaring on a dark UI
  const GRAB = 10;          // CSS px around the puck that grabs it without jumping

  // ------------------------------------------------------------ colour helpers

  // HSV (h in degrees, s and v 0..1) to [r, g, b] 0..255.
  function hsv(h, s, v) {
    const f = (n) => {
      const k = (n + h / 60) % 6;
      return v - v * s * Math.max(0, Math.min(k, 4 - k, 1));
    };
    return [f(5) * 255, f(3) * 255, f(1) * 255];
  }
  const rgbCss = ([r, g, b]) => `rgb(${Math.round(r)},${Math.round(g)},${Math.round(b)})`;

  // The wheel bitmap for a diameter in device pixels: hue around, saturation outwards,
  // anti-aliased rim. Cached, since every wheel of the same size looks the same.
  const wheelCache = new Map();
  function wheelImage(d) {
    if (wheelCache.has(d)) return wheelCache.get(d);
    const c = document.createElement("canvas");
    c.width = c.height = d;
    const g = c.getContext("2d");
    const img = g.createImageData(d, d);
    const r = d / 2;
    for (let y = 0; y < d; y++) {
      for (let x = 0; x < d; x++) {
        const dx = x + 0.5 - r;
        const dy = r - (y + 0.5);       // y up, so hue increases counter-clockwise
        const dist = Math.hypot(dx, dy);
        const alpha = Math.min(1, Math.max(0, r - dist));
        if (!alpha) continue;
        const hue = (Math.atan2(dy, dx) * 180 / Math.PI + 360) % 360;
        const [R, G, B] = hsv(hue, Math.min(1, dist / r), WHEEL_VALUE);
        const i = (y * d + x) * 4;
        img.data[i] = R; img.data[i + 1] = G; img.data[i + 2] = B; img.data[i + 3] = alpha * 255;
      }
    }
    g.putImageData(img, 0, 0);
    wheelCache.set(d, c);
    return c;
  }

  const num = (v) => {
    const n = Number(v);
    return Number.isFinite(n) ? n : 0;
  };
  const signed = (v) => (v > 0 ? "+" + v : String(v));

  // ------------------------------------------------------------ one wheel

  function makeWheel(region, emit) {
    const tag = (k) => `ColorGrade${region.key}${k}`;
    const value = { Hue: 0, Sat: 0, Lum: 0 };
    let drag = null; // { start, offset: [dx, dy] | null }
    let diameter = 0;

    const el = document.createElement("div");
    el.className = "amb-wheel";
    el.dataset.region = region.key;
    const head = document.createElement("div");
    head.className = "amb-wheel-head";
    const name = document.createElement("span");
    name.className = "amb-wheel-name";
    name.textContent = region.label;
    const readout = document.createElement("span");
    readout.className = "amb-wheel-val";
    head.append(name, readout);

    const disc = document.createElement("div");
    disc.className = "amb-wheel-disc";
    const canvas = document.createElement("canvas");
    canvas.title = "Drag to set hue and saturation · Shift-drag: saturation only · double-click to reset";
    disc.append(canvas);
    const g = canvas.getContext("2d");

    const rows = {
      Hue: slider("Hue", 0, 359, RAINBOW),
      Sat: slider("Saturation", 0, 100, null),
      Lum: slider("Luminance", -100, 100, LUM_TRACK),
    };
    rows.Hue.row.classList.add("amb-detail");
    rows.Sat.row.classList.add("amb-detail");
    el.append(head, disc, rows.Hue.row, rows.Sat.row, rows.Lum.row);

    // A labelled range slider with a numeric readout; drag = input, release = commit,
    // double-click = reset to 0.
    function slider(label, min, max, track) {
      const row = document.createElement("label");
      row.className = "amb-wheel-row";
      const text = document.createElement("span");
      text.textContent = label;
      const input = document.createElement("input");
      input.type = "range";
      input.min = min; input.max = max; input.step = 1;
      if (track) input.style.setProperty("--track", track);
      const out = document.createElement("output");
      row.append(text, input, out);
      const key = { Hue: "Hue", Saturation: "Sat", Luminance: "Lum" }[label];
      let start = null;
      input.addEventListener("pointerdown", () => (start = { ...value }));
      input.addEventListener("input", () => {
        if (!start) start = { ...value };
        value[key] = Number(input.value);
        render();
        emit("input", { [tag(key)]: value[key] });
      });
      input.addEventListener("change", () => {
        value[key] = Number(input.value);
        render();
        if (!start || start[key] !== value[key]) emit("commit", { [tag(key)]: value[key] });
        start = null;
      });
      input.addEventListener("dblclick", () => {
        if (value[key] === 0) return;
        value[key] = 0;
        render();
        emit("commit", { [tag(key)]: 0 });
      });
      return { row, input, out };
    }

    // ---------------------------------------------------- drawing

    function resize() {
      const w = Math.floor(disc.clientWidth);
      if (!w) return;
      const dpr = window.devicePixelRatio || 1;
      diameter = w;
      canvas.style.width = canvas.style.height = w + "px";
      canvas.width = canvas.height = Math.round(w * dpr);
      draw();
    }

    const radius = () => diameter / 2 - 1;
    function puckPos(hue = value.Hue, sat = value.Sat) {
      const a = (hue * Math.PI) / 180;
      const r = (sat / 100) * radius();
      return [diameter / 2 + Math.cos(a) * r, diameter / 2 - Math.sin(a) * r];
    }

    function draw() {
      if (!diameter) return;
      const dpr = canvas.width / diameter;
      g.setTransform(1, 0, 0, 1, 0, 0);
      g.clearRect(0, 0, canvas.width, canvas.height);
      const inset = Math.round(dpr); // keep the rim 1 CSS px inside the canvas
      g.drawImage(wheelImage(canvas.width - 2 * inset), inset, inset);
      g.setTransform(dpr, 0, 0, dpr, 0, 0);
      const c = diameter / 2;
      // Centre mark, so "no tint" is easy to find.
      g.strokeStyle = "rgba(0,0,0,0.35)";
      g.lineWidth = 1;
      g.beginPath();
      g.moveTo(c - 4, c); g.lineTo(c + 4, c);
      g.moveTo(c, c - 4); g.lineTo(c, c + 4);
      g.stroke();
      // Puck: the chosen colour, with a light ring and a dark halo for contrast.
      const [px, py] = puckPos();
      const pr = diameter > 150 ? 7 : 5.5;
      g.beginPath();
      g.arc(px, py, pr + 1.5, 0, Math.PI * 2);
      g.fillStyle = "rgba(0,0,0,0.55)";
      g.fill();
      g.beginPath();
      g.arc(px, py, pr, 0, Math.PI * 2);
      g.fillStyle = rgbCss(hsv(value.Hue, value.Sat / 100, WHEEL_VALUE));
      g.fill();
      g.lineWidth = 2;
      g.strokeStyle = drag ? "#fff" : "#e6e6e8";
      g.stroke();
    }

    // Sync the sliders, readout and canvas with `value`.
    function render() {
      for (const k of ["Hue", "Sat", "Lum"]) {
        rows[k].input.value = value[k];
        rows[k].out.textContent = k === "Lum" ? signed(value[k]) : String(value[k]);
      }
      rows.Sat.input.style.setProperty("--track",
        `linear-gradient(90deg,#808080,${rgbCss(hsv(value.Hue, 1, 0.9))})`);
      readout.textContent = `H ${value.Hue} · S ${value.Sat}`;
      name.classList.toggle("changed", value.Sat !== 0 || value.Lum !== 0);
      draw();
    }

    // ---------------------------------------------------- interaction

    function local(e) {
      const r = canvas.getBoundingClientRect();
      return [e.clientX - r.left, e.clientY - r.top];
    }

    // Move the puck to a canvas position. Shift keeps the hue (Lightroom), changing
    // saturation only.
    function moveTo(x, y, keepHue) {
      const dx = x - diameter / 2;
      const dy = diameter / 2 - y;
      const sat = Math.min(100, Math.round((Math.hypot(dx, dy) / radius()) * 100));
      const hue = keepHue ? value.Hue : Math.round((Math.atan2(dy, dx) * 180 / Math.PI + 360) % 360) % 360;
      if (hue === value.Hue && sat === value.Sat) return false;
      value.Hue = hue;
      value.Sat = sat;
      return true;
    }

    const hueSat = () => ({ [tag("Hue")]: value.Hue, [tag("Sat")]: value.Sat });

    canvas.addEventListener("pointerdown", (e) => {
      if (e.button !== 0 || !diameter) return;
      const [x, y] = local(e);
      const [px, py] = puckPos();
      const c = diameter / 2;
      if (Math.hypot(x - c, y - c) > radius() + GRAB && Math.hypot(x - px, y - py) > GRAB) return;
      // Grabbing the puck keeps its offset so it does not jump; elsewhere it jumps to the pointer.
      const onPuck = Math.hypot(x - px, y - py) <= GRAB;
      drag = { start: { ...value }, offset: onPuck ? [px - x, py - y] : [0, 0] };
      canvas.setPointerCapture(e.pointerId);
      el.classList.add("dragging");
      if (!onPuck && moveTo(x, y, e.shiftKey)) { render(); emit("input", hueSat()); } else draw();
      e.preventDefault();
    });

    canvas.addEventListener("pointermove", (e) => {
      if (!drag) return;
      const [x, y] = local(e);
      if (moveTo(x + drag.offset[0], y + drag.offset[1], e.shiftKey)) {
        render();
        emit("input", hueSat());
      }
    });

    function endDrag() {
      if (!drag) return;
      const { start } = drag;
      drag = null;
      el.classList.remove("dragging");
      draw();
      const changes = {};
      if (start.Hue !== value.Hue) changes[tag("Hue")] = value.Hue;
      if (start.Sat !== value.Sat) changes[tag("Sat")] = value.Sat;
      if (Object.keys(changes).length) emit("commit", changes);
    }
    canvas.addEventListener("pointerup", endDrag);
    canvas.addEventListener("pointercancel", endDrag);

    canvas.addEventListener("dblclick", () => {
      if (value.Hue === 0 && value.Sat === 0) return;
      const changes = {};
      if (value.Hue !== 0) changes[tag("Hue")] = 0;
      if (value.Sat !== 0) changes[tag("Sat")] = 0;
      value.Hue = 0;
      value.Sat = 0;
      render();
      emit("commit", changes);
    });

    const observer = new ResizeObserver(resize);
    observer.observe(disc);
    render();

    return {
      el,
      region: region.key,
      dragging: () => Boolean(drag),
      set(values) {
        for (const k of ["Hue", "Sat", "Lum"]) value[k] = Math.round(num(values ? values[tag(k)] : 0));
        value.Hue = ((value.Hue % 360) + 360) % 360;
        render();
      },
      get: () => ({ [tag("Hue")]: value.Hue, [tag("Sat")]: value.Sat, [tag("Lum")]: value.Lum }),
      refresh: resize,
      destroy: () => observer.disconnect(),
    };
  }

  // ------------------------------------------------------------ widget

  function create(container, options = {}) {
    const onInput = options.onInput || (() => {});
    const onCommit = options.onCommit || (() => {});
    const emit = (kind, changes) => (kind === "input" ? onInput : onCommit)({ ...changes });

    const root = document.createElement("div");
    root.className = "amb-wheels";
    const tabs = document.createElement("div");
    tabs.className = "amb-tabs";
    const body = document.createElement("div");
    body.className = "amb-wheels-body";
    const wheels = REGIONS.map((r) => makeWheel(r, emit));
    for (const w of wheels) body.append(w.el);
    const tabButtons = new Map();
    for (const v of VIEWS) {
      const b = document.createElement("button");
      b.type = "button";
      b.textContent = v.label;
      b.title = v.title;
      b.addEventListener("click", () => view(v.key));
      tabButtons.set(v.key, b);
      tabs.append(b);
    }
    root.append(tabs, body);
    container.append(root);

    let current = null;
    function view(key) {
      if (key === undefined) return current;
      if (!tabButtons.has(key)) throw new Error(`AmbWheels: unknown view ${key}`);
      current = key;
      body.dataset.view = key;
      for (const [k, b] of tabButtons) b.classList.toggle("active", k === key);
      for (const w of wheels) w.el.hidden = key === "3way" ? w.region === "Global" : w.region !== key;
      // Hidden wheels have no size; the ResizeObserver redraws them as they appear.
      return current;
    }
    view(VIEWS.some((v) => v.key === options.view) ? options.view : "3way");

    return {
      element: root,
      /** Load values from the app state: { ColorGradeShadowHue: 210, ... }; missing = 0.
       *  A wheel being dragged keeps its values. */
      set(values) { for (const w of wheels) if (!w.dragging()) w.set(values || {}); },
      /** All twelve values. */
      get() { return Object.assign({}, ...wheels.map((w) => w.get())); },
      /** Current view ("3way", "Shadow", "Midtone", "Highlight", "Global"); with an argument, switch. */
      view,
      refresh() { for (const w of wheels) w.refresh(); },
      destroy() { for (const w of wheels) w.destroy(); root.remove(); },
    };
  }

  window.AmbWheels = { create, TAGS, hsv };
})();
