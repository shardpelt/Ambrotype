// SPDX-License-Identifier: GPL-3.0-or-later
// Touch (T): drag up or down on the photo to change what is under the
// pointer, as Lightroom's targeted adjustment does. Tone moves the
// parametric curve's region for that brightness; Hue, Saturation and
// Luminance move the two colour bands nearest that colour (the B&W mix for a
// black-and-white photo). One drag is one undo step.
"use strict";

const touchState = { on: false, mode: "tone", drag: null, pixels: null, pixelsOf: null };

// Colour Mixer bands and their hues (degrees), in order round the wheel.
const TOUCH_BANDS = [["Red", 0], ["Orange", 30], ["Yellow", 60], ["Green", 120], ["Aqua", 180], ["Blue", 240], ["Purple", 270], ["Magenta", 300]];
const TOUCH_REGIONS = [["ParametricShadows", "Shadows"], ["ParametricDarks", "Darks"], ["ParametricLights", "Lights"], ["ParametricHighlights", "Highlights"]];
// Slider units per pixel dragged: the full range of 200 in about 400 px.
const TOUCH_GAIN = 0.5;

function toggleTouch(on = !touchState.on) {
  if (on && !ui.info) return;
  touchState.on = on;
  $("touch-bar").hidden = !on;
  $("viewer").classList.toggle("touching", on);
  if (on) {
    if (ui.zoom.on) setZoom(false);
    setPicking(false);
    status("Touch: drag up or down on the photo (T or Esc to stop)");
  } else {
    status("Ready");
  }
  syncTouchBar();
}

function syncTouchBar() {
  const mono = Boolean(currentValue("ConvertToGrayscale"));
  for (const b of document.querySelectorAll("#touch-bar [data-touch]")) {
    const m = b.dataset.touch;
    b.classList.toggle("active", m === touchState.mode);
    b.disabled = mono && (m === "hue" || m === "saturation");
  }
  if (mono && (touchState.mode === "hue" || touchState.mode === "saturation")) touchState.mode = "luminance";
}

// The shown photo's pixels, read once per image.
function touchPixel(e) {
  const img = $("preview");
  if (!img.naturalWidth) return null;
  if (touchState.pixelsOf !== img.src) {
    const c = document.createElement("canvas");
    [c.width, c.height] = [img.naturalWidth, img.naturalHeight];
    const g = c.getContext("2d", { willReadFrequently: true });
    g.drawImage(img, 0, 0);
    touchState.pixels = g.getImageData(0, 0, c.width, c.height);
    touchState.pixelsOf = img.src;
  }
  const r = img.getBoundingClientRect();
  const x = Math.floor(((e.clientX - r.left) / r.width) * img.naturalWidth);
  const y = Math.floor(((e.clientY - r.top) / r.height) * img.naturalHeight);
  if (x < 0 || y < 0 || x >= img.naturalWidth || y >= img.naturalHeight) return null;
  const i = (y * img.naturalWidth + x) * 4;
  return [0, 1, 2].map((k) => touchState.pixels.data[i + k] / 255);
}

// What a drag at this colour changes: [setting, weight] pairs and a label.
function touchTargets(rgb, mode = touchState.mode) {
  const [r, g, b] = rgb;
  if (mode === "tone") {
    const l = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    const [name, label] = TOUCH_REGIONS[Math.min(3, Math.floor(l * 4))];
    return { targets: [[name, 1]], label };
  }
  // The hue, then the two bands either side of it, weighted by nearness.
  const max = Math.max(r, g, b), min = Math.min(r, g, b);
  let h = 0;
  if (max > min) {
    const d = max - min;
    h = max === r ? ((g - b) / d) % 6 : max === g ? (b - r) / d + 2 : (r - g) / d + 4;
    h = (h * 60 + 360) % 360;
  }
  let i = TOUCH_BANDS.length - 1;
  while (i > 0 && TOUCH_BANDS[i][1] > h) i--;
  const [a, from] = TOUCH_BANDS[i];
  const [bName, to] = TOUCH_BANDS[(i + 1) % TOUCH_BANDS.length];
  const span = ((to - from + 360) % 360) || 360;
  const t = ((h - from + 360) % 360) / span;
  const mono = Boolean(currentValue("ConvertToGrayscale"));
  const kind = mono ? "GrayMixer" : { hue: "HueAdjustment", saturation: "SaturationAdjustment", luminance: "LuminanceAdjustment" }[mode];
  const pairs = [[`${kind}${a}`, 1 - t], [`${kind}${bName}`, t]].filter(([, w]) => w > 0.05);
  const label = `${pairs.map(([n]) => n.replace(kind, "")).join(" & ")} ${mono ? "in the B&W mix" : mode}`;
  return { targets: pairs, label };
}

function wireTouch() {
  for (const b of document.querySelectorAll("#touch-bar [data-touch]")) {
    b.addEventListener("click", () => { touchState.mode = b.dataset.touch; syncTouchBar(); });
  }
  $("touch-close").addEventListener("click", () => toggleTouch(false));
  const viewer = $("viewer");
  // Before the photo's own handlers (click to zoom, drag to pan).
  viewer.addEventListener("mousedown", (e) => {
    if (!touchState.on || e.button !== 0 || e.target !== $("preview")) return;
    e.preventDefault();
    e.stopPropagation();
    const rgb = touchPixel(e);
    if (!rgb) return;
    const { targets, label } = touchTargets(rgb);
    const start = Object.fromEntries(targets.map(([n]) => [n, Number(currentValue(n)) || 0]));
    touchState.drag = { y: e.clientY, targets, label, start, changed: {} };
  }, true);
  viewer.addEventListener("mousemove", (e) => {
    if (!touchState.on || touchState.drag || e.target !== $("preview")) return;
    const rgb = touchPixel(e);
    if (rgb) status(`Touch: drag up or down for ${touchTargets(rgb).label}`, true);
  });
  window.addEventListener("mousemove", (e) => {
    const d = touchState.drag;
    if (!d) return;
    const delta = (d.y - e.clientY) * TOUCH_GAIN;
    for (const [name, w] of d.targets) {
      const def = ui.defs.get(name);
      const v = Math.round(Math.min(def?.max ?? 100, Math.max(def?.min ?? -100, d.start[name] + delta * w)));
      ui.overrides[name] = v;
      d.changed[name] = v;
    }
    status(`Touch: ${d.label} ${delta >= 0 ? "+" : ""}${Math.round(delta)}`, true);
    requestPreview(true);
  });
  window.addEventListener("mouseup", () => {
    const d = touchState.drag;
    if (!d) return;
    touchState.drag = null;
    const cmds = Object.entries(d.changed)
      .filter(([n, v]) => v !== d.start[n])
      .map(([name, value]) => ({ cmd: "set_param", name, value }));
    for (const n of Object.keys(d.changed)) delete ui.overrides[n];
    if (cmds.length) run(cmds, `Touch: ${d.label}`);
    else requestPreview(false);
  });
}
