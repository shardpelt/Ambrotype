// SPDX-License-Identifier: GPL-3.0-or-later
// The histogram panel's second view: a vectorscope. Hue is the angle, saturation
// the distance from the centre, drawn from the displayed photo (the counts come
// with each preview, amb_engine::scopes). A skin-tone line and the colour
// targets help read it; a ring shows where the colour under the pointer lands.
"use strict";

const SCOPE_KEY = "amb.scope";
// Bins on a side: amb_engine::scopes::VECTORSCOPE_SIZE.
const SCOPE_SIZE = 128;
// The skin-tone line, degrees from +Cb counter-clockwise: amb_engine::scopes::SKIN_LINE_DEG.
const SCOPE_SKIN_DEG = 123;
// BT.709 luma weights and chroma divisors, as in amb_engine::scopes.
const SCOPE_LUMA = [0.2126, 0.7152, 0.0722];
const SCOPE_CB_DIV = 1.8556;
const SCOPE_CR_DIV = 1.5748;
// The six colour targets, at 75% of each primary or secondary.
const SCOPE_TARGETS = [["R", 1, 0, 0], ["Mg", 1, 0, 1], ["B", 0, 0, 1], ["Cy", 0, 1, 1], ["G", 0, 1, 0], ["Yl", 1, 1, 0]];
const SCOPE_TARGET_LEVEL = 0.75;
// The luma the bins are tinted at, so every hue shows on the dark plot.
const SCOPE_TINT_LUMA = 0.7;
// Lowest opacity of a bin that has any pixel in it; the rest scales with its density.
const SCOPE_MIN_ALPHA = 50;
// The plot's side as a fraction of the canvas' shorter side.
const SCOPE_FILL = 0.94;

const scope = {
  mode: store.text(SCOPE_KEY) === "vectorscope" ? "vectorscope" : "histogram",
  data: null,   // the last vectorscope counts, SCOPE_SIZE² of them
  dot: null,    // the colour under the pointer, [r, g, b] in 0..1, or null
  frame: null,  // a pending animation frame for the dot
};

// Where a colour (r, g, b in 0..1) lands: 0..1 across and down the plot.
function scopePoint(r, g, b) {
  const y = SCOPE_LUMA[0] * r + SCOPE_LUMA[1] * g + SCOPE_LUMA[2] * b;
  return [(b - y) / SCOPE_CB_DIV + 0.5, 0.5 - (r - y) / SCOPE_CR_DIV];
}

// Called with each new preview's data: keep the vectorscope and draw the active view.
function scopeShow(meta) {
  scope.data = meta.vectorscope ?? null;
  drawHistogram(meta.histogram);
}

// A theme colour, read when drawing so every theme works.
function scopeColour(name) {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim() || "#888";
}

// The counts as a small picture: each bin tinted by the colour it stands for,
// more opaque the more pixels it holds (on a log scale, so a few strong colours
// don't hide the rest).
function scopeBins() {
  const n = SCOPE_SIZE;
  const bins = scope.data;
  const img = new ImageData(n, n);
  let max = 1;
  for (const v of bins) if (v > max) max = v;
  const norm = Math.log1p(max);
  for (let i = 0; i < bins.length; i++) {
    if (!bins[i]) continue;
    const cb = ((i % n) + 0.5) / n - 0.5;
    const cr = 0.5 - (Math.floor(i / n) + 0.5) / n;
    const r = SCOPE_TINT_LUMA + SCOPE_CR_DIV * cr;
    const b = SCOPE_TINT_LUMA + SCOPE_CB_DIV * cb;
    const g = (SCOPE_TINT_LUMA - SCOPE_LUMA[0] * r - SCOPE_LUMA[2] * b) / SCOPE_LUMA[1];
    const d = Math.log1p(bins[i]) / norm;
    img.data.set([r, g, b].map((c) => Math.round(Math.min(1, Math.max(0, c)) * 255)), i * 4);
    img.data[i * 4 + 3] = Math.round(SCOPE_MIN_ALPHA + (255 - SCOPE_MIN_ALPHA) * d);
  }
  const c = el("canvas", { width: n, height: n });
  c.getContext("2d").putImageData(img, 0, 0);
  return c;
}

// The vectorscope on the histogram's canvas.
function scopeDraw() {
  const c = $("histogram");
  const dpr = window.devicePixelRatio || 1;
  c.width = c.clientWidth * dpr;
  c.height = c.clientHeight * dpr;
  const g = c.getContext("2d");
  g.clearRect(0, 0, c.width, c.height);
  const side = Math.min(c.width, c.height) * SCOPE_FILL;
  const [ox, oy] = [(c.width - side) / 2, (c.height - side) / 2];
  const at = ([x, y]) => [ox + x * side, oy + y * side];
  const [cx, cy] = at([0.5, 0.5]);
  const line = Math.max(1, dpr);
  g.lineWidth = line;
  g.strokeStyle = scopeColour("--line");
  g.strokeRect(ox, oy, side, side);
  g.beginPath();
  g.moveTo(ox, cy);
  g.lineTo(ox + side, cy);
  g.moveTo(cx, oy);
  g.lineTo(cx, oy + side);
  g.stroke();
  // A ring at the targets' mean distance, which is 75% saturation.
  const targets = SCOPE_TARGETS.map(([name, r, gr, b]) => ({
    name,
    p: at(scopePoint(r * SCOPE_TARGET_LEVEL, gr * SCOPE_TARGET_LEVEL, b * SCOPE_TARGET_LEVEL)),
  }));
  const ring = targets.reduce((s, t) => s + Math.hypot(t.p[0] - cx, t.p[1] - cy), 0) / targets.length;
  g.beginPath();
  g.arc(cx, cy, ring, 0, Math.PI * 2);
  g.stroke();
  if (scope.data) g.drawImage(scopeBins(), ox, oy, side, side);
  // The skin-tone line, out to the edge of the plot.
  const a = (SCOPE_SKIN_DEG * Math.PI) / 180;
  g.strokeStyle = scopeColour("--accent");
  g.beginPath();
  g.moveTo(cx, cy);
  g.lineTo(cx + Math.cos(a) * side / 2, cy - Math.sin(a) * side / 2);
  g.stroke();
  // Target ticks and their names.
  g.fillStyle = g.strokeStyle = scopeColour("--muted");
  g.font = `${Math.round(9 * dpr)}px sans-serif`;
  g.textAlign = "center";
  g.textBaseline = "middle";
  for (const { name, p } of targets) {
    g.strokeRect(p[0] - 3 * dpr, p[1] - 3 * dpr, 6 * dpr, 6 * dpr);
    const away = Math.hypot(p[0] - cx, p[1] - cy) || 1;
    g.fillText(name, p[0] + ((p[0] - cx) / away) * 11 * dpr, p[1] + ((p[1] - cy) / away) * 11 * dpr);
  }
  if (scope.dot) {
    const [x, y] = at(scopePoint(...scope.dot));
    g.lineWidth = 2 * line;
    g.strokeStyle = scopeColour("--text");
    g.beginPath();
    g.arc(x, y, 5 * dpr, 0, Math.PI * 2);
    g.stroke();
  }
}

// Histogram or vectorscope, remembered.
function scopeToggle(to = scope.mode === "vectorscope" ? "histogram" : "vectorscope") {
  scope.mode = to;
  store.setText(SCOPE_KEY, to);
  scope.dot = null;
  scopeSync();
  if (ui.histogram) drawHistogram(ui.histogram);
}

// The switch names the view it leads to.
function scopeSync() {
  const vec = scope.mode === "vectorscope";
  const b = $("scope-switch");
  b.textContent = vec ? t("Histogram") : t("Vectorscope");
  b.title = vec ? t("Back to the histogram") : t("Show the colours as a wheel: hue as the angle, saturation as the distance from the centre");
  b.setAttribute("aria-pressed", String(vec));
}

function wireScope() {
  $("scope-switch").addEventListener("click", () => scopeToggle());
  scopeSync();
  // The colour under the pointer, from the shown photo's pixels (touch.js reads them).
  const viewer = $("viewer");
  viewer.addEventListener("mousemove", (e) => {
    if (scope.mode !== "vectorscope" || scope.frame || e.target !== $("preview")) return;
    scope.frame = requestAnimationFrame(() => {
      scope.frame = null;
      scope.dot = touchPixel(e);
      if (scope.mode === "vectorscope" && ui.histogram) scopeDraw();
    });
  });
  viewer.addEventListener("mouseleave", () => {
    if (!scope.dot) return;
    scope.dot = null;
    if (scope.mode === "vectorscope" && ui.histogram) scopeDraw();
  });
}
