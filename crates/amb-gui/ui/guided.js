// SPDX-License-Identifier: GPL-3.0-or-later
// Guided Upright: draw two to four lines that should be vertical or
// horizontal, and the Transform sliders straighten them (Vertical,
// Horizontal and Rotate, solved in amb_engine::upright::guided). Lightroom
// keeps its guides in a form we can't read, so ours go in amb:UprightGuides
// and the result is in the sliders, which Lightroom reads too.
//
// While the tool is open the photo shows uncropped and without Transform,
// so the guides are drawn on the frame the sliders start from. Guides are
// fractions (u, v) of that upright photo.
"use strict";

const GUIDE_MAX = 4;
const gu = { on: false, guides: [], drag: null, hover: null };

// The preview overrides while the tool is open: no crop, no Transform.
function guidedOverrides() {
  if (!gu.on) return {};
  return {
    HasCrop: false, PerspectiveUpright: 0, PerspectiveVertical: 0, PerspectiveHorizontal: 0, PerspectiveRotate: 0,
    PerspectiveAspect: 0, PerspectiveScale: 100, PerspectiveX: 0, PerspectiveY: 0,
  };
}

function guideRect() {
  return viewRect("guide-canvas");
}

function guideToCanvas([u, v]) {
  const r = guideRect();
  return [r.left + u * r.width, r.top + v * r.height];
}

function canvasToGuide(px, py) {
  const r = guideRect();
  return [(px - r.left) / r.width, (py - r.top) / r.height];
}

function drawGuides() {
  const c = $("guide-canvas");
  if (!gu.on || c.hidden) return;
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
  if (!ui.preview.url) return;
  const accent = getComputedStyle(document.documentElement).getPropertyValue("--accent").trim() || "#f0b45a";
  const lines = gu.drag?.add ? [...gu.guides, gu.drag.line] : gu.guides;
  lines.forEach((l, i) => {
    const a = guideToCanvas([l[0], l[1]]);
    const b = guideToCanvas([l[2], l[3]]);
    const vertical = Math.abs(l[3] - l[1]) * guideRect().height >= Math.abs(l[2] - l[0]) * guideRect().width;
    for (const [colour, width] of [["rgba(0,0,0,0.6)", 4], [accent, 1.5]]) {
      g.beginPath();
      g.moveTo(...a);
      g.lineTo(...b);
      g.strokeStyle = colour;
      g.lineWidth = width;
      g.stroke();
    }
    for (const p of [a, b]) {
      g.beginPath();
      g.arc(p[0], p[1], 5, 0, 2 * Math.PI);
      g.fillStyle = i === gu.hover?.index ? accent : "rgba(0,0,0,0.6)";
      g.fill();
      g.strokeStyle = accent;
      g.lineWidth = 1.5;
      g.stroke();
    }
    g.fillStyle = accent;
    g.font = "11px system-ui, sans-serif";
    g.fillText(vertical ? "vertical" : "horizontal", (a[0] + b[0]) / 2 + 8, (a[1] + b[1]) / 2);
  });
}

// The end of a guide under (px, py), if any.
function hitGuideEnd(px, py) {
  for (let i = gu.guides.length - 1; i >= 0; i--) {
    const l = gu.guides[i];
    for (const end of [0, 1]) {
      const [x, y] = guideToCanvas([l[2 * end], l[2 * end + 1]]);
      if (Math.hypot(px - x, py - y) <= 8) return { index: i, end };
    }
  }
  return null;
}

function renderGuidedPanel() {
  const n = gu.guides.length;
  $("guided-count").textContent = `${n} of ${GUIDE_MAX}`;
  $("guided-apply").disabled = n === 0;
  $("guided-clear").disabled = n === 0;
  $("guided-hint").textContent = n === 0
    ? "Drag along a line that should be vertical (a wall, a door frame) or horizontal (a horizon). Two verticals fix converging verticals; two horizontals fix the horizontal perspective."
    : n < GUIDE_MAX
      ? "Add another guide, or drag a guide's end to adjust it. Right-click a guide's end to remove it."
      : "That's the most guides. Drag an end to adjust, right-click it to remove.";
  drawGuides();
}

async function enterGuided() {
  if (!ui.info || gu.on) return;
  if (ui.crop.on) await exitCrop(true);
  exitMasks();
  exitSpots();
  if (ui.zoom.on) setZoom(false);
  setPicking(false);
  try {
    gu.guides = await invoke("upright_guides");
  } catch {
    gu.guides = [];
  }
  gu.on = true;
  $("guide-canvas").hidden = false;
  $("guided-panel").hidden = false;
  for (const id of ["btn-zoom", "btn-wb"]) $(id).disabled = true;
  status("Guided Upright: draw lines that should be straight");
  renderGuidedPanel();
  requestPreview(false);
}

function exitGuided() {
  if (!gu.on) return;
  gu.on = false;
  gu.drag = null;
  $("guide-canvas").hidden = true;
  $("guided-panel").hidden = true;
  for (const id of ["btn-zoom", "btn-wb"]) $(id).disabled = !ui.info;
  status("Ready");
  requestPreview(false);
}

async function applyGuided() {
  if (!gu.guides.length) return;
  try {
    const r = await invoke("guided_upright", { guides: gu.guides });
    // As with the other Upright modes, the crop goes (Alt keeps it).
    const crop = altHeld ? [] : CROP_TAGS.filter((n) => n in ui.settings).map((name) => ({ cmd: "reset_param", name }));
    exitGuided();
    await run([...r.commands, ...crop], "Guided Upright");
    if (r.worst_degrees > 0.5) {
      toast(`The guides don't all fit one correction; the worst is still ${r.worst_degrees.toFixed(1)}° off.`, true);
    }
  } catch (e) {
    toast(String(e), true);
  }
}

function wireGuided() {
  const c = $("guide-canvas");
  const at = (e) => {
    const r = c.getBoundingClientRect();
    return [e.clientX - r.left, e.clientY - r.top];
  };
  c.addEventListener("contextmenu", (e) => {
    e.preventDefault();
    const hit = hitGuideEnd(...at(e));
    if (hit) {
      gu.guides.splice(hit.index, 1);
      gu.hover = null;
      renderGuidedPanel();
    }
  });
  c.addEventListener("pointerdown", (e) => {
    if (e.button !== 0) return;
    e.preventDefault();
    c.setPointerCapture(e.pointerId);
    const [px, py] = at(e);
    const hit = hitGuideEnd(px, py);
    if (hit) {
      gu.drag = { ...hit };
    } else if (gu.guides.length < GUIDE_MAX) {
      const p = canvasToGuide(px, py);
      gu.drag = { add: true, line: [p[0], p[1], p[0], p[1]] };
    }
  });
  c.addEventListener("pointermove", (e) => {
    const [px, py] = at(e);
    const d = gu.drag;
    const p = canvasToGuide(px, py).map((v) => Math.min(1, Math.max(0, v)));
    if (d?.add) {
      d.line[2] = p[0];
      d.line[3] = p[1];
    } else if (d) {
      gu.guides[d.index][2 * d.end] = p[0];
      gu.guides[d.index][2 * d.end + 1] = p[1];
    } else {
      gu.hover = hitGuideEnd(px, py);
      c.style.cursor = gu.hover ? "move" : gu.guides.length < GUIDE_MAX ? "crosshair" : "default";
    }
    drawGuides();
  });
  c.addEventListener("pointerup", () => {
    const d = gu.drag;
    gu.drag = null;
    if (d?.add) {
      const [a, b] = [guideToCanvas([d.line[0], d.line[1]]), guideToCanvas([d.line[2], d.line[3]])];
      // A click, not a line.
      if (Math.hypot(a[0] - b[0], a[1] - b[1]) >= 12) gu.guides.push(d.line);
    }
    renderGuidedPanel();
  });
  $("guided-apply").addEventListener("click", applyGuided);
  $("guided-clear").addEventListener("click", () => {
    gu.guides = [];
    renderGuidedPanel();
  });
  $("guided-cancel").addEventListener("click", exitGuided);
  window.addEventListener("keydown", (e) => {
    if (!gu.on) return;
    if (e.key === "Escape") exitGuided();
    else if (e.key === "Enter") applyGuided();
    else return;
    e.preventDefault();
    e.stopImmediatePropagation();
  }, true);
  new ResizeObserver(() => drawGuides()).observe($("viewer"));
}
