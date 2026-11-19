// SPDX-License-Identifier: GPL-3.0-or-later
// Crop and straighten overlay (Lightroom's Crop tool). Plain JS, no build step.
//
//   const tool = AmbCrop.create(canvas, { image: () => img | null, onChange(rect, dragging) });
//   tool.start({ width, height, angleSign, rect });
//
// Everything is in the upright (EXIF-oriented), uncropped photo's full-size
// pixels: `rect` is { cx, cy, width, height, angle } as the `crop_state` /
// `crop_commands` commands use it (amb_export::geometry::UprightCrop). The
// rectangle's x axis runs at angleSign × angle degrees over the photo (y down),
// so a positive angle turns the photo counter-clockwise.
//
// The photo is drawn turned by the straighten angle so the crop rectangle stays
// level on screen, like Lightroom. Drag an edge or corner to resize, inside to
// move, outside to straighten. The rectangle is kept inside the photo: dragging
// stops at the edges and straightening shrinks it as needed.
//
// Draw mode (like a macOS screenshot): the cursor is a cross; press at one
// corner and drag to the opposite one to draw a new rectangle, at the current
// angle and aspect lock. It is on when a photo has no crop yet (start({ draw })),
// or when asked (setDrawMode), and ends once a rectangle is drawn.
"use strict";

(function () {
  const HANDLE = 12;   // corner grab distance, CSS px
  const EDGE = 8;      // edge grab distance, CSS px
  const PAD = 28;      // CSS px around the turned photo
  const MAX_ANGLE = 45;
  const SNAP = 0.6;    // degrees
  const SHADE = "rgba(10,10,12,0.62)";
  const CURSORS = {
    n: "ns-resize", s: "ns-resize", e: "ew-resize", w: "ew-resize",
    nw: "nwse-resize", se: "nwse-resize", ne: "nesw-resize", sw: "nesw-resize",
    move: "move", rotate: "grab",
  };

  // ------------------------------------------------------------ geometry

  // The rectangle's x axis (cos, sin) over the photo; its y axis is (−sin, cos).
  function axes(angle, sign) {
    const t = (sign * angle * Math.PI) / 180;
    return [Math.cos(t), Math.sin(t)];
  }

  // Top-left, top-right, bottom-right, bottom-left, in photo pixels.
  function corners(r, sign) {
    const [c, s] = axes(r.angle, sign);
    const hw = r.width / 2;
    const hh = r.height / 2;
    return [[-1, -1], [1, -1], [1, 1], [-1, 1]].map(([a, b]) =>
      [r.cx + a * hw * c - b * hh * s, r.cy + a * hw * s + b * hh * c]);
  }

  function inside(r, W, H, sign) {
    const eps = 1e-9 * (W + H);
    return corners(r, sign).every(([x, y]) => x >= -eps && x <= W + eps && y >= -eps && y <= H + eps);
  }

  // Largest scale (at most `limit`) of r about its centre that stays inside W×H.
  function fitScale(r, W, H, sign, limit = 1) {
    let k = limit;
    for (const [x, y] of corners(r, sign)) {
      const ox = x - r.cx;
      const oy = y - r.cy;
      if (ox > 1e-12) k = Math.min(k, (W - r.cx) / ox);
      if (ox < -1e-12) k = Math.min(k, r.cx / -ox);
      if (oy > 1e-12) k = Math.min(k, (H - r.cy) / oy);
      if (oy < -1e-12) k = Math.min(k, r.cy / -oy);
    }
    return Math.max(0, k);
  }

  function lerp(a, b, t) {
    return {
      cx: a.cx + (b.cx - a.cx) * t,
      cy: a.cy + (b.cy - a.cy) * t,
      width: a.width + (b.width - a.width) * t,
      height: a.height + (b.height - a.height) * t,
      angle: b.angle,
    };
  }

  // The rectangle furthest from `a` towards `b` (same angle) that stays inside.
  // Rectangles inside a convex photo form a convex set, so bisection finds it.
  // A start already outside (hand-written values) is not held back.
  function towards(a, b, W, H, sign) {
    if (inside(b, W, H, sign) || !inside(a, W, H, sign)) return b;
    let lo = 0;
    let hi = 1;
    for (let i = 0; i < 32; i++) {
      const m = (lo + hi) / 2;
      if (inside(lerp(a, b, m), W, H, sign)) lo = m;
      else hi = m;
    }
    return lerp(a, b, lo);
  }

  // `base` turned to `angle` about its centre, shrunk to stay inside.
  function straighten(base, angle, W, H, sign) {
    const n = { ...base, angle };
    if (!inside(base, W, H, sign)) return n;
    const k = fitScale(n, W, H, sign);
    return { ...n, width: n.width * k, height: n.height * k };
  }

  // The largest rectangle of this ratio and angle, as close to (cx, cy) as fits.
  function largest(ratio, angle, cx, cy, W, H, sign) {
    const big = { cx: W / 2, cy: H / 2, width: 4 * (W + H) * Math.max(1, ratio), angle };
    big.height = big.width / ratio;
    const k = fitScale(big, W, H, sign);
    const r = { ...big, width: big.width * k, height: big.height * k };
    return towards(r, { ...r, cx, cy }, W, H, sign);
  }

  // A rectangle from edges l, r, t, b in `s`'s own frame (relative to its centre).
  function fromLocal(s, l, r, t, b, sign) {
    const [c, sn] = axes(s.angle, sign);
    const mx = (l + r) / 2;
    const my = (t + b) / 2;
    return { cx: s.cx + mx * c - my * sn, cy: s.cy + mx * sn + my * c, width: r - l, height: b - t, angle: s.angle };
  }

  // ------------------------------------------------------------ view and drag

  // The angle that makes guide `l` level (or upright, when it's steep).
  function cropLevelling(cs, l) {
    const phi = (Math.atan2(l[3] - l[1], l[2] - l[0]) * 180) / Math.PI;
    let a = ((phi + 90) % 180 + 180) % 180 - 90; // the line's slant, −90..90
    if (a > 45) a -= 90;
    else if (a < -45) a += 90;
    return a / cs.sign;
  }

  function cropSnapped(cs, angle, free) {
    if (free || !cs.snaps.length) return angle;
    let best = null;
    for (const s of cs.snaps) if (Math.abs(s - angle) < SNAP && (best == null || Math.abs(s - angle) < Math.abs(best - angle))) best = s;
    return best ?? angle;
  }

  // View: the photo turned by −θ about its centre (so the crop is level),
  // fitted to the canvas.
  function cropLayout(cs, canvas) {
    const t = (cs.sign * (cs.rect ? cs.rect.angle : 0) * Math.PI) / 180;
    const c = Math.cos(t);
    const s = Math.sin(t);
    const bw = cs.W * Math.abs(c) + cs.H * Math.abs(s);
    const bh = cs.W * Math.abs(s) + cs.H * Math.abs(c);
    const vw = canvas.clientWidth;
    const vh = canvas.clientHeight;
    const S = Math.max(1e-6, Math.min((vw - 2 * PAD) / bw, (vh - 2 * PAD) / bh));
    return { t, c, s, S, vx: vw / 2, vy: vh / 2 };
  }

  function cropToView(cs, L, x, y) {
    const dx = x - cs.W / 2;
    const dy = y - cs.H / 2;
    return [L.vx + L.S * (dx * L.c + dy * L.s), L.vy + L.S * (-dx * L.s + dy * L.c)];
  }

  // Screen point -> photo pixels (the inverse of toView).
  function cropFromView(cs, L, X, Y) {
    const u = (X - L.vx) / L.S;
    const v = (Y - L.vy) / L.S;
    return [cs.W / 2 + u * L.c - v * L.s, cs.H / 2 + u * L.s + v * L.c];
  }

  // The crop rectangle on screen (level): x0, y0, x1, y1 in CSS px.
  function cropViewRect(cs, L) {
    const [X, Y] = cropToView(cs, L, cs.rect.cx, cs.rect.cy);
    const hw = (L.S * cs.rect.width) / 2;
    const hh = (L.S * cs.rect.height) / 2;
    return { x0: X - hw, y0: Y - hh, x1: X + hw, y1: Y + hh };
  }

  function cropDraw(cs, canvas, g, img) {
    const dpr = window.devicePixelRatio || 1;
    const w = canvas.clientWidth;
    const h = canvas.clientHeight;
    if (canvas.width !== Math.round(w * dpr) || canvas.height !== Math.round(h * dpr)) {
      canvas.width = Math.round(w * dpr);
      canvas.height = Math.round(h * dpr);
    }
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    g.clearRect(0, 0, w, h);
    if (!cs.rect) return;
    const L = cropLayout(cs, canvas);
    g.save();
    g.translate(L.vx, L.vy);
    g.scale(L.S, L.S);
    g.rotate(-L.t);
    g.translate(-cs.W / 2, -cs.H / 2);
    if (img) {
      g.imageSmoothingQuality = "high";
      g.drawImage(img, 0, 0, cs.W, cs.H);
    } else {
      g.fillStyle = "#2a2c31";
      g.fillRect(0, 0, cs.W, cs.H);
    }
    // The straight lines while straightening; the ones level now stand out.
    if (cs.guides.length && ((cs.drag && cs.drag.kind === "rotate") || cs.angleBase)) {
      for (const l of cs.guides) {
        const on = Math.abs(cropLevelling(cs, l) - cs.rect.angle) < 0.05;
        g.strokeStyle = on ? "rgba(120, 230, 140, 0.95)" : "rgba(255, 210, 90, 0.55)";
        g.lineWidth = (on ? 2.5 : 1.5) / L.S;
        g.beginPath();
        g.moveTo(l[0], l[1]);
        g.lineTo(l[2], l[3]);
        g.stroke();
      }
    }
    g.restore();

    // Draw mode, before the first drag: just the photo, waiting for the cross.
    if (cs.drawing && !(cs.drag && cs.drag.kind === "draw" && cs.drag.moved)) return;
    const v = cropViewRect(cs, L);
    g.fillStyle = SHADE;
    g.beginPath();
    g.rect(0, 0, w, h);
    g.rect(v.x0, v.y0, v.x1 - v.x0, v.y1 - v.y0);
    g.fill("evenodd");

    // Rule of thirds while dragging; a finer grid while straightening.
    if (cs.drag) {
      const n = cs.drag.kind === "rotate" ? 8 : 3;
      g.strokeStyle = "rgba(255,255,255,0.45)";
      g.lineWidth = 1;
      g.beginPath();
      for (let i = 1; i < n; i++) {
        const x = Math.round(v.x0 + ((v.x1 - v.x0) * i) / n) + 0.5;
        const y = Math.round(v.y0 + ((v.y1 - v.y0) * i) / n) + 0.5;
        g.moveTo(x, v.y0);
        g.lineTo(x, v.y1);
        g.moveTo(v.x0, y);
        g.lineTo(v.x1, y);
      }
      g.stroke();
    }
    g.strokeStyle = "rgba(255,255,255,0.9)";
    g.lineWidth = 1;
    g.strokeRect(v.x0, v.y0, v.x1 - v.x0, v.y1 - v.y0);
    // Corner and edge handles.
    const len = Math.min(20, (v.x1 - v.x0) / 3, (v.y1 - v.y0) / 3);
    g.lineWidth = 3;
    g.strokeStyle = "#fff";
    g.beginPath();
    for (const [x, y, dx, dy] of [[v.x0, v.y0, 1, 1], [v.x1, v.y0, -1, 1], [v.x1, v.y1, -1, -1], [v.x0, v.y1, 1, -1]]) {
      g.moveTo(x + dx * len, y);
      g.lineTo(x, y);
      g.lineTo(x, y + dy * len);
    }
    const mx = (v.x0 + v.x1) / 2;
    const my = (v.y0 + v.y1) / 2;
    g.moveTo(mx - len / 2, v.y0); g.lineTo(mx + len / 2, v.y0);
    g.moveTo(mx - len / 2, v.y1); g.lineTo(mx + len / 2, v.y1);
    g.moveTo(v.x0, my - len / 2); g.lineTo(v.x0, my + len / 2);
    g.moveTo(v.x1, my - len / 2); g.lineTo(v.x1, my + len / 2);
    g.stroke();
    // The size next to the pointer while drawing or resizing, like a screenshot.
    if (cs.drag && cs.drag.kind !== "move" && cs.drag.kind !== "rotate" && cs.drag.pointer) {
      const text = `${Math.round(cs.rect.width)} × ${Math.round(cs.rect.height)}`;
      g.font = "12px system-ui, sans-serif";
      const tw = g.measureText(text).width;
      const [px, py] = cs.drag.pointer;
      // Beside the pointer, but never off the canvas (the pointer can be outside).
      const bx = Math.max(4, Math.min(px + 14, w - tw - 14));
      const by = Math.max(4, Math.min(py + 18, h - 24));
      g.fillStyle = "rgba(0,0,0,0.7)";
      g.fillRect(bx, by, tw + 10, 20);
      g.fillStyle = "#fff";
      g.fillText(text, bx + 5, by + 14);
    }
  }

  function cropHit(cs, canvas, X, Y) {
    const v = cropViewRect(cs, cropLayout(cs, canvas));
    const nx0 = Math.abs(X - v.x0) <= HANDLE;
    const nx1 = Math.abs(X - v.x1) <= HANDLE;
    const ny0 = Math.abs(Y - v.y0) <= HANDLE;
    const ny1 = Math.abs(Y - v.y1) <= HANDLE;
    if ((nx0 || nx1) && (ny0 || ny1)) return (ny0 ? "n" : "s") + (nx0 ? "w" : "e");
    const inX = X > v.x0 - EDGE && X < v.x1 + EDGE;
    const inY = Y > v.y0 - EDGE && Y < v.y1 + EDGE;
    if (inY && Math.abs(X - v.x0) <= EDGE) return "w";
    if (inY && Math.abs(X - v.x1) <= EDGE) return "e";
    if (inX && Math.abs(Y - v.y0) <= EDGE) return "n";
    if (inX && Math.abs(Y - v.y1) <= EDGE) return "s";
    if (X > v.x0 && X < v.x1 && Y > v.y0 && Y < v.y1) return "move";
    return "rotate";
  }

  // Move edges `kind` (n, s, e, w or a corner) of `base` by (ux, uy) photo
  // pixels along its own axes, keeping a minimum size and the locked ratio.
  function cropResized(cs, base, kind, ux, uy, minSize) {
    let l = -base.width / 2;
    let r = base.width / 2;
    let t = -base.height / 2;
    let b = base.height / 2;
    const hasW = kind.includes("w");
    const hasE = kind.includes("e");
    const hasN = kind.includes("n");
    const hasS = kind.includes("s");
    if (hasW) l = Math.min(l + ux, r - minSize);
    if (hasE) r = Math.max(r + ux, l + minSize);
    if (hasN) t = Math.min(t + uy, b - minSize);
    if (hasS) b = Math.max(b + uy, t + minSize);
    if (cs.ratio) {
      let w = r - l;
      let h = b - t;
      if ((hasW || hasE) && (hasN || hasS)) {
        // A corner: the side that moved further leads; the opposite corner stays.
        if (w / h > cs.ratio) h = w / cs.ratio;
        else w = h * cs.ratio;
        if (hasW) l = r - w; else r = l + w;
        if (hasN) t = b - h; else b = t + h;
      } else if (hasW || hasE) {
        h = w / cs.ratio;
        const mid = (t + b) / 2;
        t = mid - h / 2;
        b = mid + h / 2;
      } else {
        w = h * cs.ratio;
        const mid = (l + r) / 2;
        l = mid - w / 2;
        r = mid + w / 2;
      }
    }
    return fromLocal(base, l, r, t, b, cs.sign);
  }

  function cropUpdate(cs, X, Y) {
    const s = cs.drag.start;
    const L = cs.drag.L;
    const ux = (X - cs.drag.X) / L.S;
    const uy = (Y - cs.drag.Y) / L.S;
    const kind = cs.drag.kind;
    if (kind === "draw") {
      // A tiny rectangle at the anchor, grown towards the pointer through its
      // far corner, so the corner logic (aspect lock, edges) applies as is.
      const m = cs.drag.m;
      const [AX, AY] = cropToView(cs, L, cs.drag.anchor[0], cs.drag.anchor[1]);
      const sx = X < AX ? -1 : 1;
      const sy = Y < AY ? -1 : 1;
      const [c, sn] = axes(s.angle, cs.sign);
      const hx = (sx * m) / 2;
      const hy = (sy * m) / 2;
      const tiny = { cx: cs.drag.anchor[0] + hx * c - hy * sn, cy: cs.drag.anchor[1] + hx * sn + hy * c, width: m, height: m, angle: s.angle };
      const corner = (sy > 0 ? "s" : "n") + (sx > 0 ? "e" : "w");
      const dx = (X - AX) / L.S - sx * m;
      const dy = (Y - AY) / L.S - sy * m;
      if (cs.ratio) {
        cs.rect = towards(tiny, cropResized(cs, tiny, corner, dx, dy, m), cs.W, cs.H, cs.sign);
      } else {
        const a = towards(tiny, cropResized(cs, tiny, corner[1], dx, 0, m), cs.W, cs.H, cs.sign);
        cs.rect = towards(a, cropResized(cs, a, corner[0], 0, dy, m), cs.W, cs.H, cs.sign);
      }
      return;
    }
    if (kind === "rotate") {
      const [px, py] = cs.drag.centre;
      let d = Math.atan2(Y - py, X - px) - Math.atan2(cs.drag.Y - py, cs.drag.X - px);
      d = Math.atan2(Math.sin(d), Math.cos(d));
      // The photo follows the pointer: a clockwise drag turns it clockwise.
      const deg = s.angle - (d * 180) / Math.PI / cs.sign;
      // Snaps to a line coming level; Alt turns freely.
      const angle = cropSnapped(cs, Math.round(Math.min(MAX_ANGLE, Math.max(-MAX_ANGLE, deg)) * 100) / 100, cs.drag.free);
      cs.rect = straighten(s, angle, cs.W, cs.H, cs.sign);
    } else if (kind === "move") {
      const [c, sn] = axes(s.angle, cs.sign);
      // Slide along each axis separately so the rectangle runs along an edge.
      const a = towards(s, { ...s, cx: s.cx + ux * c, cy: s.cy + ux * sn }, cs.W, cs.H, cs.sign);
      cs.rect = towards(a, { ...a, cx: a.cx - uy * sn, cy: a.cy + uy * c }, cs.W, cs.H, cs.sign);
    } else {
      const minSize = Math.max(16 / L.S, 0.01 * Math.min(cs.W, cs.H));
      if (cs.ratio || kind.length === 1) {
        cs.rect = towards(s, cropResized(cs, s, kind, ux, uy, minSize), cs.W, cs.H, cs.sign);
      } else {
        // Free corner: width and height stop at the photo's edges separately.
        const across = kind[1];
        const a = towards(s, cropResized(cs, s, across, ux, 0, minSize), cs.W, cs.H, cs.sign);
        cs.rect = towards(a, cropResized(cs, a, kind[0], 0, uy, minSize), cs.W, cs.H, cs.sign);
      }
    }
  }

  // ------------------------------------------------------------ widget

  function create(canvas, opts = {}) {
    const g = canvas.getContext("2d");
    const cs = {
      W: 1,
      H: 1,
      sign: 1,
      rect: null,
      ratio: null,       // locked width / height, or null (free)
      drag: null,
      angleBase: null,   // the rectangle before a slider straighten
      drawing: false,    // draw mode: the next drag draws a new rectangle
      // The photo's straight lines ([x0, y0, x1, y1] in photo pixels), shown
      // while straightening; a turn that brings one level or upright snaps to it.
      guides: [],
      snaps: [],         // the angles (degrees, the tool's) that level a guide
    };

    const draw = () => cropDraw(cs, canvas, g, opts.image ? opts.image() : null);

    function local(e) {
      const b = canvas.getBoundingClientRect();
      return [e.clientX - b.left, e.clientY - b.top];
    }

    function changed(dragging) {
      draw();
      if (opts.onChange) opts.onChange({ ...cs.rect }, dragging);
    }

    canvas.addEventListener("pointerdown", (e) => {
      if (e.button !== 0 || !cs.rect) return;
      e.preventDefault();
      const [X, Y] = local(e);
      const L = cropLayout(cs, canvas);
      cs.angleBase = null;
      if (cs.drawing) {
        // The anchor is where the press was, kept a little inside the photo so
        // the first tiny rectangle fits whichever way the drag goes.
        const m = Math.max(16 / L.S, 0.01 * Math.min(cs.W, cs.H));
        const [px, py] = cropFromView(cs, L, X, Y);
        const inset = 1.5 * m;
        const anchor = [Math.min(cs.W - inset, Math.max(inset, px)), Math.min(cs.H - inset, Math.max(inset, py))];
        cs.drag = { kind: "draw", X, Y, L, m, anchor, start: { ...cs.rect }, moved: false, pointer: [X, Y] };
      } else {
        cs.drag = { kind: cropHit(cs, canvas, X, Y), X, Y, L, start: { ...cs.rect }, centre: cropToView(cs, L, cs.rect.cx, cs.rect.cy), pointer: [X, Y] };
      }
      if (cs.drag.kind === "rotate") canvas.style.cursor = "grabbing";
      try { canvas.setPointerCapture(e.pointerId); } catch { /* a pointer the page made up (tests) */ }
      draw();
    });
    canvas.addEventListener("pointermove", (e) => {
      if (!cs.rect) return;
      const [X, Y] = local(e);
      if (!cs.drag) {
        canvas.style.cursor = cs.drawing ? "crosshair" : CURSORS[cropHit(cs, canvas, X, Y)];
        return;
      }
      cs.drag.pointer = [X, Y];
      cs.drag.free = e.altKey;
      if (cs.drag.kind === "draw") {
        // A click (no real drag) draws nothing.
        if (!cs.drag.moved && Math.hypot(X - cs.drag.X, Y - cs.drag.Y) < 4) return;
        cs.drag.moved = true;
      }
      cropUpdate(cs, X, Y);
      changed(true);
    });
    const end = () => {
      if (!cs.drag) return;
      const drew = cs.drag.kind === "draw" && cs.drag.moved;
      const clicked = cs.drag.kind === "draw" && !cs.drag.moved;
      cs.drag = null;
      if (clicked) return;
      if (drew) setDrawing(false);
      canvas.style.cursor = "";
      changed(false);
    };

    function setDrawing(on) {
      cs.drawing = Boolean(on);
      canvas.style.cursor = cs.drawing ? "crosshair" : "";
      if (opts.onDrawMode) opts.onDrawMode(cs.drawing);
      draw();
    }
    canvas.addEventListener("pointerup", end);
    canvas.addEventListener("pointercancel", end);

    const observer = new ResizeObserver(() => draw());
    observer.observe(canvas);

    return {
      /** Begin on a W×H photo (upright full-size pixels) with a starting rectangle. */
      start(o) {
        cs.W = o.width;
        cs.H = o.height;
        cs.sign = o.angleSign || 1;
        cs.rect = { ...o.rect };
        cs.drag = null;
        cs.angleBase = null;
        cs.guides = [];
        cs.snaps = [];
        setDrawing(Boolean(o.draw));
      },
      stop() { cs.rect = null; cs.drag = null; cs.drawing = false; draw(); },
      /** Draw mode: the next drag draws a new rectangle (a cross cursor). */
      setDrawMode(on) { if (cs.rect) setDrawing(on); },
      drawMode() { return cs.drawing; },
      /** The current rectangle. */
      rect() { return cs.rect ? { ...cs.rect } : null; },
      setRect(r) { cs.rect = { ...r }; cs.angleBase = null; changed(false); },
      /** Lock width / height to `r` (null: free). Call fitRatio() to reshape. */
      setRatio(r) { cs.ratio = r > 0 && Number.isFinite(r) ? r : null; },
      ratio() { return cs.ratio; },
      /** Reshape to the locked ratio: the largest that fits, near the current centre. */
      fitRatio() {
        if (!cs.rect || !cs.ratio) return;
        const cur = cs.rect.width / cs.rect.height;
        if (Math.abs(Math.log(cur / cs.ratio)) < 1e-4) return;
        cs.rect = largest(cs.ratio, cs.rect.angle, cs.rect.cx, cs.rect.cy, cs.W, cs.H, cs.sign);
        changed(false);
      },
      /** Portrait ⇄ landscape (Lightroom's X). */
      swap() {
        if (!cs.rect) return;
        if (cs.ratio) cs.ratio = 1 / cs.ratio;
        const n = { ...cs.rect, width: cs.rect.height, height: cs.rect.width };
        cs.rect = inside(n, cs.W, cs.H, cs.sign) ? n : largest(n.width / n.height, n.angle, n.cx, n.cy, cs.W, cs.H, cs.sign);
        cs.angleBase = null;
        changed(false);
      },
      /** Straighten from the slider: live while dragging; endAngle() when released. */
      setAngle(deg) {
        if (!cs.rect) return;
        if (!cs.angleBase) cs.angleBase = { ...cs.rect };
        const angle = Math.min(MAX_ANGLE, Math.max(-MAX_ANGLE, deg));
        cs.rect = straighten(cs.angleBase, angle, cs.W, cs.H, cs.sign);
        changed(true);
      },
      endAngle() { cs.angleBase = null; if (cs.rect) changed(false); },
      /** The photo's straight lines to show and snap to while straightening. */
      setGuides(lines) {
        cs.guides = lines ?? [];
        cs.snaps = [...new Set(cs.guides.map((l) => Math.round(cropLevelling(cs, l) * 100) / 100))].filter((a) => Math.abs(a) <= MAX_ANGLE);
        draw();
      },
      draw,
      destroy() { observer.disconnect(); },
    };
  }

  window.AmbCrop = { create, corners, inside, fitScale, straighten, largest, MAX_ANGLE };
})();
