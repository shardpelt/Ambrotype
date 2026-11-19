// SPDX-License-Identifier: GPL-3.0-or-later
// Point-curve editor widget (Lightroom "Point Curve"). Plain JS, no build step.
//
//   const curve = AmbCurve.create(container, { onInput(tag, points), onCommit(tag, points) });
//   curve.set("ToneCurvePV2012", [[0,0],[64,56],[192,200],[255,255]]);
//
// Points are [input, output] integer pairs on Lightroom's 0..255 axes, sorted by
// strictly increasing input. The curve is drawn with the same natural cubic
// spline the engine renders with (crates/amb-engine/src/curve.rs), so what you
// see is what the preview shows.
"use strict";

(function () {
  const TAGS = ["ToneCurvePV2012", "ToneCurvePV2012Red", "ToneCurvePV2012Green", "ToneCurvePV2012Blue"];
  const CHANNELS = [
    { tag: TAGS[0], label: "RGB", colour: "#e6e6e8", hist: [0, 1, 2] },
    { tag: TAGS[1], label: "Red", colour: "#e04848", hist: [0] },
    { tag: TAGS[2], label: "Green", colour: "#4fbf4f", hist: [1] },
    { tag: TAGS[3], label: "Blue", colour: "#4170e0", hist: [2] },
  ];
  const HIST_FILL = ["rgba(230,70,70,0.22)", "rgba(80,200,90,0.22)", "rgba(80,120,240,0.22)"];
  const IDENTITY = [[0, 0], [255, 255]];
  const PAD = 7;             // CSS px around the plot so end-point handles are not clipped
  const HIT = 9;             // grab radius around a point, CSS px
  const SNAP = 14;           // a click this close to the curve adds a point on the curve
  const REMOVE_OUTSIDE = 28; // dragging an inner point this far out of the plot removes it

  // ------------------------------------------------------------ spline

  // Natural cubic spline, mirroring amb_engine::curve::Spline::from_points_255:
  // 0..1 axes, sorted, duplicate inputs keep the later point, zero second
  // derivative at the ends, flat outside the end points, output clamped to 0..1.
  // Returns an evaluator on 0..255 axes, or null for fewer than two distinct points.
  function spline(points) {
    const sorted = points.map(([x, y]) => [x / 255, y / 255]).sort((a, b) => a[0] - b[0]);
    const pts = [];
    for (const p of sorted) {
      if (pts.length && Math.abs(p[0] - pts[pts.length - 1][0]) < 1e-9) pts[pts.length - 1] = p;
      else pts.push(p);
    }
    const n = pts.length;
    if (n < 2) return null;
    const xs = pts.map((p) => p[0]);
    const ys = pts.map((p) => p[1]);
    const m = new Float64Array(n);
    if (n > 2) {
      // Tridiagonal system for the interior second derivatives (Thomas algorithm).
      const c = new Float64Array(n);
      const d = new Float64Array(n);
      for (let i = 1; i < n - 1; i++) {
        const h0 = xs[i] - xs[i - 1];
        const h1 = xs[i + 1] - xs[i];
        const rhs = 6 * ((ys[i + 1] - ys[i]) / h1 - (ys[i] - ys[i - 1]) / h0);
        const denom = 2 * (h0 + h1) - h0 * c[i - 1];
        c[i] = h1 / denom;
        d[i] = (rhs - h0 * d[i - 1]) / denom;
      }
      for (let i = n - 2; i >= 1; i--) m[i] = d[i] - c[i] * m[i + 1];
    }
    const clamp01 = (v) => Math.min(1, Math.max(0, v));
    return function evaluate(x255) {
      const x = x255 / 255;
      if (x <= xs[0]) return clamp01(ys[0]) * 255;
      if (x >= xs[n - 1]) return clamp01(ys[n - 1]) * 255;
      let i = 0;
      while (i < n - 2 && xs[i + 1] <= x) i++;
      const h = xs[i + 1] - xs[i];
      const a = (xs[i + 1] - x) / h;
      const b = (x - xs[i]) / h;
      const y = a * ys[i] + b * ys[i + 1] + ((a * a * a - a) * m[i] + (b * b * b - b) * m[i + 1]) * h * h / 6;
      return clamp01(y) * 255;
    };
  }

  // ------------------------------------------------------------ values

  // Accept every form a curve can arrive in from the app state: [[x,y],...],
  // [{x,y},...], XMP-style ["x, y", ...] strings, or a flat Lua-style [x0,y0,x1,y1,...].
  // Anything unusable (missing, 0, malformed) is the identity curve.
  function parse(value) {
    if (!Array.isArray(value)) return copy(IDENTITY);
    let raw;
    if (value.every((v) => typeof v === "number")) {
      raw = [];
      for (let i = 0; i + 1 < value.length; i += 2) raw.push([value[i], value[i + 1]]);
    } else {
      raw = value.map((p) => {
        if (Array.isArray(p)) return [Number(p[0]), Number(p[1])];
        if (typeof p === "string") return p.split(",").map(Number);
        if (p && typeof p === "object") return [Number(p.x), Number(p.y)];
        return [NaN, NaN];
      });
    }
    return normalise(raw);
  }

  // Integers on 0..255, sorted, strictly increasing inputs (a duplicate input keeps
  // the later point, like the engine), at least two points.
  function normalise(raw) {
    const clean = raw
      .filter((p) => p.length === 2 && Number.isFinite(p[0]) && Number.isFinite(p[1]))
      .map(([x, y]) => [clampByte(x), clampByte(y)])
      .sort((a, b) => a[0] - b[0]);
    const out = [];
    for (const p of clean) {
      if (out.length && out[out.length - 1][0] === p[0]) out[out.length - 1] = p;
      else out.push(p);
    }
    return out.length >= 2 ? out : copy(IDENTITY);
  }

  function clampByte(v) { return Math.min(255, Math.max(0, Math.round(v))); }
  function copy(points) { return points.map((p) => [p[0], p[1]]); }
  function same(a, b) { return a.length === b.length && a.every((p, i) => p[0] === b[i][0] && p[1] === b[i][1]); }
  function isIdentity(points) { return points.length === 2 && same(points, IDENTITY); }

  function cssVar(el, name, fallback) {
    const v = getComputedStyle(el).getPropertyValue(name).trim();
    return v || fallback;
  }

  // ------------------------------------------------------------ geometry

  // The functions below take the widget's state (`cv`, see create) and, to
  // draw, its canvas context.

  const curvePlot = (cv) => cv.size - 2 * PAD;
  const curveToPx = (cv, x, y) => [PAD + (x / 255) * curvePlot(cv), PAD + (1 - y / 255) * curvePlot(cv)];
  const curveFromPx = (cv, px, py) => [((px - PAD) / curvePlot(cv)) * 255, (1 - (py - PAD) / curvePlot(cv)) * 255];

  // The active channel's point within grab distance of (px, py), or −1.
  function curvePointNear(cv, px, py) {
    let best = -1;
    let bestD = HIT;
    cv.curves[cv.active].forEach(([x, y], i) => {
      const [qx, qy] = curveToPx(cv, x, y);
      const d = Math.hypot(qx - px, qy - py);
      if (d <= bestD) { best = i; bestD = d; }
    });
    return best;
  }

  // Points as currently shown: while an inner point is dragged out of the plot it
  // is hidden (and removed on release), like Lightroom.
  function curveShown(cv, tag) {
    const pts = cv.curves[tag];
    return cv.drag && cv.drag.tag === tag && cv.drag.removing ? pts.filter((_, i) => i !== cv.drag.index) : pts;
  }

  // ------------------------------------------------------------ drawing

  function curveDraw(cv, g, root) {
    if (!cv.size) return;
    const dpr = g.canvas.width / cv.size;
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    g.clearRect(0, 0, cv.size, cv.size);
    const p = curvePlot(cv);
    const line = cssVar(root, "--line", "#34363c");
    const faint = cssVar(root, "--faint", "#6c6f77");

    g.fillStyle = cssVar(root, "--bg-deep", "#111214");
    g.fillRect(PAD, PAD, p, p);
    curveDrawHistogram(cv, g, p);

    // 4×4 grid and frame, on half pixels so 1px lines stay crisp.
    g.lineWidth = 1;
    g.strokeStyle = line;
    g.beginPath();
    for (let i = 1; i < 4; i++) {
      const v = Math.round(PAD + (i * p) / 4) + 0.5;
      g.moveTo(v, PAD); g.lineTo(v, PAD + p);
      g.moveTo(PAD, v); g.lineTo(PAD + p, v);
    }
    g.stroke();
    g.strokeRect(PAD + 0.5, PAD + 0.5, p - 1, p - 1);

    // Identity diagonal.
    g.setLineDash([3, 4]);
    g.strokeStyle = faint;
    g.beginPath();
    g.moveTo(PAD, PAD + p);
    g.lineTo(PAD + p, PAD);
    g.stroke();
    g.setLineDash([]);

    // The RGB curve is drawn in the theme's text colour, so it shows on light themes too.
    const colourOf = (c) => (c.tag === TAGS[0] ? cssVar(root, "--text", c.colour) : c.colour);
    // Other channels faintly, so a colour curve is not forgotten.
    for (const ch of CHANNELS) {
      if (ch.tag === cv.active || isIdentity(cv.curves[ch.tag])) continue;
      g.globalAlpha = 0.35;
      curveStroke(cv, g, curveShown(cv, ch.tag), colourOf(ch), 1.25);
      g.globalAlpha = 1;
    }

    const ch = CHANNELS.find((c) => c.tag === cv.active);
    const pts = curveShown(cv, cv.active);
    const colour = colourOf(ch);
    curveStroke(cv, g, pts, colour, 2);
    pts.forEach(([x, y], i) => {
      const [qx, qy] = curveToPx(cv, x, y);
      const hot = (cv.drag && cv.drag.tag === cv.active && !cv.drag.removing && cv.drag.index === i) ||
        (!cv.drag && cv.hover && cv.hover.index === i);
      g.beginPath();
      g.arc(qx, qy, hot ? 5 : 4, 0, Math.PI * 2);
      g.fillStyle = hot ? colour : cssVar(root, "--bg-deep", "#111214");
      g.fill();
      g.lineWidth = 1.5;
      g.strokeStyle = colour;
      g.stroke();
    });
  }

  function curveStroke(cv, g, points, colour, width) {
    const f = spline(points);
    if (!f) return;
    const p = curvePlot(cv);
    const steps = Math.max(64, Math.ceil(p));
    g.beginPath();
    for (let s = 0; s <= steps; s++) {
      const x = (s / steps) * 255;
      const [qx, qy] = curveToPx(cv, x, f(x));
      if (s === 0) g.moveTo(qx, qy); else g.lineTo(qx, qy);
    }
    g.lineWidth = width;
    g.strokeStyle = colour;
    g.lineJoin = "round";
    g.stroke();
  }

  function curveDrawHistogram(cv, g, p) {
    if (!cv.histogram) return;
    const ch = CHANNELS.find((c) => c.tag === cv.active);
    const max = Math.max(1, ...ch.hist.flatMap((i) => Array.from(cv.histogram[i] || []).slice(2, 254)));
    g.save();
    g.beginPath();
    g.rect(PAD, PAD, p, p);
    g.clip();
    g.globalCompositeOperation = "lighter";
    for (const i of ch.hist) {
      const counts = cv.histogram[i];
      if (!counts || !counts.length) continue;
      g.fillStyle = HIST_FILL[i];
      g.beginPath();
      g.moveTo(PAD, PAD + p);
      for (let x = 0; x < counts.length; x++) {
        const h = Math.min(1, counts[x] / max) * p * 0.9;
        g.lineTo(PAD + (x / (counts.length - 1)) * p, PAD + p - h);
      }
      g.lineTo(PAD + p, PAD + p);
      g.fill();
    }
    g.restore();
  }

  const fmt = ([x, y]) => `${x} → ${y}`;

  // The line under the curve (the point under the pointer, or what a click does),
  // the Reset button and which channel tabs have a curve.
  function curveReadout(cv, dom) {
    const pts = cv.curves[cv.active];
    let text = "";
    if (cv.drag && cv.drag.tag === cv.active) {
      text = cv.drag.removing ? "Release to remove point" : fmt(pts[cv.drag.index]);
    } else if (cv.hover && cv.hover.index !== undefined) {
      text = fmt(pts[cv.hover.index]);
    } else if (cv.hover && cv.hover.x !== undefined) {
      const x = clampByte(cv.hover.x);
      text = fmt([x, Math.round(spline(pts)(x))]);
    }
    dom.readout.textContent = text || "Click adds · double-click removes";
    dom.readout.classList.toggle("idle", !text);
    dom.resetBtn.disabled = isIdentity(pts);
    for (const [t, b] of dom.tabButtons) b.classList.toggle("changed", !isIdentity(cv.curves[t]));
  }

  // ------------------------------------------------------------ interaction

  // A press at (px, py): the point it grabs, adding one when it is between the
  // end points and on no point. Sets cv.drag; false when it grabs nothing.
  function curvePress(cv, px, py) {
    const pts = cv.curves[cv.active];
    let index = curvePointNear(cv, px, py);
    const start = copy(pts);
    if (index < 0) {
      const [vx, vy] = curveFromPx(cv, px, py);
      const x = clampByte(vx);
      if (x <= pts[0][0] || x >= pts[pts.length - 1][0]) return false; // outside the end points
      const existing = pts.findIndex((q) => q[0] === x);
      if (existing >= 0) {
        index = existing;
      } else {
        // Near the curve: add on it (shape unchanged until dragged); elsewhere add at the click.
        const onCurve = spline(pts)(x);
        const [, cy] = curveToPx(cv, x, onCurve);
        const y = Math.abs(cy - py) <= SNAP ? Math.round(onCurve) : clampByte(vy);
        pts.push([x, y]);
        pts.sort((a, b) => a[0] - b[0]);
        index = pts.findIndex((q) => q[0] === x);
      }
    }
    cv.drag = { tag: cv.active, index, start, removing: false };
    return true;
  }

  // The dragged point to (px, py), or marked for removal when far outside the
  // plot. True when the points shown changed.
  function curveDragTo(cv, px, py) {
    const pts = cv.curves[cv.drag.tag];
    const i = cv.drag.index;
    const last = pts.length - 1;
    const outside = px < -REMOVE_OUTSIDE || py < -REMOVE_OUTSIDE ||
      px > cv.size + REMOVE_OUTSIDE || py > cv.size + REMOVE_OUTSIDE;
    const removing = i > 0 && i < last && outside;
    const before = copy(curveShown(cv, cv.drag.tag));
    cv.drag.removing = removing;
    if (!removing) {
      const [vx, vy] = curveFromPx(cv, px, py);
      // Keep inputs strictly increasing: a point cannot pass its neighbours.
      const lo = i === 0 ? 0 : pts[i - 1][0] + 1;
      const hi = i === last ? 255 : pts[i + 1][0] - 1;
      pts[i] = [Math.min(hi, Math.max(lo, Math.round(vx))), clampByte(vy)];
    }
    return !same(before, curveShown(cv, cv.drag.tag));
  }

  // DOM: channel tabs, square canvas, readout line.
  function curveDom(onTab, onReset) {
    const root = document.createElement("div");
    root.className = "amb-curve";
    const tabs = document.createElement("div");
    tabs.className = "amb-tabs";
    const tabButtons = new Map();
    for (const ch of CHANNELS) {
      const b = document.createElement("button");
      b.type = "button";
      b.dataset.tag = ch.tag;
      b.title = `${ch.label} channel (${ch.tag})`;
      const dot = document.createElement("span");
      dot.className = "amb-dot";
      dot.style.background = ch.colour;
      b.append(dot, document.createTextNode(ch.label));
      b.addEventListener("click", () => onTab(ch.tag));
      tabButtons.set(ch.tag, b);
      tabs.append(b);
    }
    const wrap = document.createElement("div");
    wrap.className = "amb-curve-plot";
    const canvas = document.createElement("canvas");
    canvas.title = "Click to add a point · drag to move · double-click or drag out of the graph to remove";
    wrap.append(canvas);
    const foot = document.createElement("div");
    foot.className = "amb-curve-foot";
    const readout = document.createElement("span");
    readout.className = "amb-readout";
    const resetBtn = document.createElement("button");
    resetBtn.type = "button";
    resetBtn.className = "amb-link";
    resetBtn.textContent = "Reset channel";
    resetBtn.title = "Reset the active channel to a straight line";
    resetBtn.addEventListener("click", onReset);
    foot.append(readout, resetBtn);
    root.append(tabs, wrap, foot);
    return { root, tabButtons, wrap, canvas, readout, resetBtn };
  }

  // ------------------------------------------------------------ widget

  function create(container, options = {}) {
    const onInput = options.onInput || (() => {});
    const onCommit = options.onCommit || (() => {});
    const cv = {
      curves: Object.fromEntries(TAGS.map((t) => [t, copy(IDENTITY)])),
      active: TAGS.includes(options.channel) ? options.channel : TAGS[0],
      histogram: null,
      hover: null,   // { index } over a point, or { x } over the plot
      drag: null,    // { tag, index, start, removing, changed }
      size: 0,       // plot side incl. padding, CSS px
    };

    const dom = curveDom((tag) => channel(tag), () => {
      if (isIdentity(cv.curves[cv.active])) return;
      cv.curves[cv.active] = copy(IDENTITY);
      draw();
      onCommit(cv.active, copy(IDENTITY));
    });
    const { root, wrap, canvas } = dom;
    container.append(root);
    const g = canvas.getContext("2d");
    const draw = () => curveDraw(cv, g, root);
    const updateReadout = () => curveReadout(cv, dom);

    function pointerPos(e) {
      const r = canvas.getBoundingClientRect();
      return [e.clientX - r.left, e.clientY - r.top];
    }

    function resize() {
      const w = Math.floor(wrap.clientWidth);
      if (!w) return;
      const dpr = window.devicePixelRatio || 1;
      cv.size = w;
      canvas.style.width = canvas.style.height = w + "px";
      canvas.width = Math.round(w * dpr);
      canvas.height = Math.round(w * dpr);
      draw();
    }

    canvas.addEventListener("pointerdown", (e) => {
      if (e.button !== 0 || !cv.size) return;
      const [px, py] = pointerPos(e);
      if (!curvePress(cv, px, py)) return;
      canvas.setPointerCapture(e.pointerId);
      canvas.classList.add("dragging");
      const pts = cv.curves[cv.active];
      if (!same(cv.drag.start, pts)) onInput(cv.active, copy(pts));
      draw();
      updateReadout();
      e.preventDefault();
    });

    canvas.addEventListener("pointermove", (e) => {
      const [px, py] = pointerPos(e);
      if (!cv.drag) {
        const index = curvePointNear(cv, px, py);
        const inside = px >= PAD && px <= cv.size - PAD && py >= PAD && py <= cv.size - PAD;
        cv.hover = index >= 0 ? { index } : inside ? { x: curveFromPx(cv, px, py)[0] } : null;
        canvas.style.cursor = index >= 0 ? "grab" : "crosshair";
      } else if (curveDragTo(cv, px, py)) {
        onInput(cv.drag.tag, copy(curveShown(cv, cv.drag.tag)));
      }
      draw();
      updateReadout();
    });

    function endDrag() {
      if (!cv.drag) return;
      const { tag, index, start, removing } = cv.drag;
      if (removing) cv.curves[tag].splice(index, 1);
      cv.drag = null;
      canvas.classList.remove("dragging");
      if (!same(start, cv.curves[tag])) onCommit(tag, copy(cv.curves[tag]));
      draw();
      updateReadout();
    }
    canvas.addEventListener("pointerup", endDrag);
    canvas.addEventListener("pointercancel", endDrag);
    canvas.addEventListener("pointerleave", () => {
      if (cv.drag) return;
      cv.hover = null;
      draw();
      updateReadout();
    });

    canvas.addEventListener("dblclick", (e) => {
      const [px, py] = pointerPos(e);
      const pts = cv.curves[cv.active];
      const index = curvePointNear(cv, px, py);
      if (index <= 0 || index >= pts.length - 1) return; // end points stay
      pts.splice(index, 1);
      cv.hover = null;
      draw();
      updateReadout();
      onCommit(cv.active, copy(pts));
    });

    // ---------------------------------------------------- controller

    function channel(tag) {
      if (tag === undefined) return cv.active;
      if (!TAGS.includes(tag)) throw new Error(`AmbCurve: unknown channel ${tag}`);
      cv.active = tag;
      for (const [t, b] of dom.tabButtons) b.classList.toggle("active", t === tag);
      root.dataset.channel = tag;
      cv.hover = null;
      draw();
      updateReadout();
      return cv.active;
    }

    function set(tag, points) {
      if (!TAGS.includes(tag)) return;
      if (cv.drag && cv.drag.tag === tag) return; // never yank a point from under the pointer
      cv.curves[tag] = parse(points);
      draw();
      updateReadout();
    }

    // Next frame: resizing the canvas inside the callback loops the observer.
    const observer = new ResizeObserver(() => requestAnimationFrame(resize));
    observer.observe(wrap);
    channel(cv.active);
    resize();

    return {
      element: root,
      /** Load a channel's points from the app state (any accepted form; missing = identity). */
      set,
      /** Load several channels at once: { ToneCurvePV2012: [...], ... }. */
      setAll(values) { for (const t of TAGS) set(t, values ? values[t] : undefined); },
      /** Current points of a channel (default: the active one), as [[x, y], ...] integers. */
      get(tag = cv.active) { return copy(cv.curves[tag] || IDENTITY); },
      /** Reset a channel (default: the active one) to identity without firing callbacks. */
      reset(tag = cv.active) { set(tag, IDENTITY); },
      /** Active channel tag; with an argument, switch to that channel. */
      channel,
      /** Faint histogram behind the curve: [r, g, b] arrays of 256 counts (null hides it). */
      setHistogram(h) { cv.histogram = Array.isArray(h) && h.length >= 3 ? h : null; draw(); },
      /** Redraw after the container was hidden (e.g. a collapsed <details>) and shown again. */
      refresh: resize,
      destroy() { observer.disconnect(); root.remove(); },
    };
  }

  window.AmbCurve = { create, TAGS, IDENTITY: copy(IDENTITY), spline, parse };
})();
