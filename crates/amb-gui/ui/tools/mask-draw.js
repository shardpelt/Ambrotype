// SPDX-License-Identifier: GPL-3.0-or-later
// Masks: the geometry of each mask shape and drawing the masks on the photo.
// Split out of masks.js.
"use strict";

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
    // The engine's inverse rotation (src/mask/geometry.rs).
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
