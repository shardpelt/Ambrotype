// SPDX-License-Identifier: GPL-3.0-or-later
// Spot removal (Lightroom's Remove tool, heal and clone). Plain JS, no build step.
//
// Every change is a protocol command (add_spot, set_spot, remove_spot) on
// Lightroom's RetouchAreas. Click the photo to add a spot: the engine suggests
// a donor (spot_source) that you can drag elsewhere. Drag a spot to move it;
// the wheel or [ ] resize the selected spot (or the next one). While a spot or
// its source is dragged, the preview renders an edited copy of the tag.
// Coordinates go through the mask tool's map (mask_frame, masks.js helpers).
"use strict";

const SPOT_TAG = "RetouchAreas";

const sp = {
  on: false,
  frame: null,
  selected: null,       // spot index
  drag: null,
  kind: "heal",         // for new spots
  size: 1.5,            // radius, % of the photo's width, for new spots
  feather: 50,
  opacity: 100,
  busy: false,
  // Visualise spots: the photo as white edges on black, to find dust.
  visualise: false,
  threshold: 50,
  edges: null,          // { key, data } cached for one preview and size
};

const spotList = () => ui.state?.spots?.spots ?? [];
const selectedSpot = () => (sp.selected == null ? null : spotList().find((s) => s.index === sp.selected) ?? null);
const sCanvas = (x, y) => toCanvas(x, y, sp.frame, "spot-canvas");
const sFrame = (px, py) => toFrame(px, py, sp.frame, "spot-canvas");
const spotRadiusPx = (r) => r * sp.frame.frame_width * pxScale(sp.frame, "spot-canvas");

// ---------------------------------------------------------------- drawing

function drawSpots() {
  const c = $("spot-canvas");
  if (!sp.on || c.hidden) return;
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
  if (!sp.frame || !ui.preview.url) return;
  if (sp.visualise) drawEdges(c, dpr);
  const accent = getComputedStyle(document.documentElement).getPropertyValue("--accent").trim() || "#f0b45a";
  const circle = ([x, y], r, colour, dash = []) => {
    g.setLineDash(dash);
    g.beginPath();
    g.arc(x, y, r, 0, 2 * Math.PI);
    g.strokeStyle = "rgba(0,0,0,0.55)";
    g.lineWidth = 3;
    g.stroke();
    g.strokeStyle = colour;
    g.lineWidth = 1.5;
    g.stroke();
    g.setLineDash([]);
  };
  for (const s of shownSpots()) {
    if (s.shape !== "round" || !s.active) continue;
    const sel = s.index === sp.selected;
    const at = sCanvas(s.at[0], s.at[1]);
    const r = spotRadiusPx(s.radius);
    const colour = sel ? accent : "#fff";
    if (s.source && sel) {
      const src = sCanvas(s.source[0], s.source[1]);
      // A line between the circles' edges, with the source dashed.
      const dx = at[0] - src[0];
      const dy = at[1] - src[1];
      const d = Math.hypot(dx, dy) || 1;
      if (d > 2 * r) {
        g.beginPath();
        g.moveTo(src[0] + (dx / d) * r, src[1] + (dy / d) * r);
        g.lineTo(at[0] - (dx / d) * r, at[1] - (dy / d) * r);
        g.strokeStyle = "rgba(255,255,255,0.8)";
        g.lineWidth = 1;
        g.stroke();
      }
      circle(src, r, colour, [5, 4]);
    }
    circle(at, r, colour);
  }
  // The next spot's size, under the pointer.
  if (sp.hover && !sp.drag) circle(sp.hover, spotRadiusPx(sp.size / 100), "rgba(255,255,255,0.6)", [2, 3]);
}

// Lightroom's Visualize Spots: edges of the preview, white on black, so dust
// and specks stand out. Higher thresholds show only stronger edges.
function drawEdges(c, dpr) {
  const img = $("preview");
  if (!img.naturalWidth) return;
  const r = viewRect("spot-canvas");
  const [x0, y0] = [Math.round(r.left * dpr), Math.round(r.top * dpr)];
  const [w, h] = [Math.max(1, Math.round(r.width * dpr)), Math.max(1, Math.round(r.height * dpr))];
  const key = `${ui.preview.url}|${w}x${h}|${sp.threshold}`;
  if (sp.edges?.key !== key) {
    const off = new OffscreenCanvas(w, h);
    const og = off.getContext("2d", { willReadFrequently: true });
    og.drawImage(img, 0, 0, w, h);
    const src = og.getImageData(0, 0, w, h).data;
    const lum = new Float32Array(w * h);
    for (let i = 0; i < w * h; i++) lum[i] = 0.299 * src[4 * i] + 0.587 * src[4 * i + 1] + 0.114 * src[4 * i + 2];
    const out = og.createImageData(w, h);
    // Sobel edges; the threshold (0..100) takes away the weaker ones.
    const cut = sp.threshold * 1.2;
    const at = (x, y) => lum[Math.min(h - 1, Math.max(0, y)) * w + Math.min(w - 1, Math.max(0, x))];
    for (let y = 0; y < h; y++) {
      for (let x = 0; x < w; x++) {
        const gx = at(x + 1, y - 1) + 2 * at(x + 1, y) + at(x + 1, y + 1) - at(x - 1, y - 1) - 2 * at(x - 1, y) - at(x - 1, y + 1);
        const gy = at(x - 1, y + 1) + 2 * at(x, y + 1) + at(x + 1, y + 1) - at(x - 1, y - 1) - 2 * at(x, y - 1) - at(x + 1, y - 1);
        const v = Math.max(0, Math.min(255, (Math.hypot(gx, gy) - cut) * 2));
        const o = 4 * (y * w + x);
        out.data[o] = out.data[o + 1] = out.data[o + 2] = v;
        out.data[o + 3] = 255;
      }
    }
    sp.edges = { key, data: out };
  }
  c.getContext("2d").putImageData(sp.edges.data, x0, y0);
}

function setVisualise(on) {
  sp.visualise = on;
  $("spot-visualise").checked = on;
  $("spot-threshold").disabled = !on;
  drawSpots();
}

// The spots with a drag applied, so the circles follow the pointer.
function shownSpots() {
  const d = sp.drag;
  if (!d?.moved) return spotList();
  return spotList().map((s) => (s.index !== d.spot ? s : { ...s, ...d.patch }));
}

// ---------------------------------------------------------------- panel

function renderSpotPanel() {
  if (!sp.on) return;
  const list = spotList();
  if (sp.selected != null && !list.some((s) => s.index === sp.selected)) sp.selected = null;
  const s = selectedSpot();
  for (const b of document.querySelectorAll("#spot-kind button")) {
    const kind = s ? s.kind : sp.kind;
    b.classList.toggle("active", b.dataset.kind === kind);
  }
  const set = (id, v) => {
    $(id).value = v;
    $(`${id}-num`).value = id === "spot-size" ? Number(v).toFixed(1) : Math.round(v);
  };
  set("spot-size", s?.radius != null ? s.radius * 100 : sp.size);
  set("spot-feather", s ? s.feather : sp.feather);
  set("spot-opacity", s ? s.opacity : sp.opacity);
  $("spot-delete").disabled = !s;
  $("spot-clear").disabled = !list.length;
  const legacy = ui.state?.spots?.legacy;
  const made = list.filter((x) => x.note).length;
  $("spot-count").textContent = `${list.length} spot(s)` + (legacy ? `, ${legacy} older` : "");
  $("spot-hint").textContent = s
    ? (s.note ? `${s.note}. ` : "") + "Drag the spot or its source to move them. Delete removes it; click elsewhere for a new one."
    : "Click a blemish to remove it. Drag the dashed source circle to pick where it copies from. [ and ] or the wheel change the size."
      + (made ? ` ${made} spot(s) were filled by Lightroom itself; they are healed from a nearby area here.` : "");
  drawSpots();
}

function spotValue(key, v) {
  const s = selectedSpot();
  if (key === "size") {
    if (s) run([{ cmd: "set_spot", spot: s.index, values: { radius: v / 100 } }]);
    else sp.size = v;
  } else if (s) {
    run([{ cmd: "set_spot", spot: s.index, values: { [key]: v } }]);
  } else {
    sp[key] = v;
  }
  renderSpotPanel();
}

function setSpotKind(kind) {
  const s = selectedSpot();
  if (s) run([{ cmd: "set_spot", spot: s.index, values: { kind } }]);
  else sp.kind = kind;
  renderSpotPanel();
}

function spotSize(dir) {
  const s = selectedSpot();
  const cur = s?.radius != null ? s.radius * 100 : sp.size;
  const next = Math.min(10, Math.max(0.2, cur * (dir > 0 ? 1.15 : 1 / 1.15)));
  spotValue("size", Math.round(next * 100) / 100);
}

// ---------------------------------------------------------------- pointer

function hitSpot(px, py) {
  // The selected spot's source first, then any spot's circle (topmost last).
  const s = selectedSpot();
  if (s?.source && s.shape === "round") {
    const [x, y] = sCanvas(s.source[0], s.source[1]);
    if (Math.hypot(px - x, py - y) <= Math.max(8, spotRadiusPx(s.radius))) return { spot: s.index, part: "source" };
  }
  for (const t of [...spotList()].reverse()) {
    if (t.shape !== "round") continue;
    const [x, y] = sCanvas(t.at[0], t.at[1]);
    if (Math.hypot(px - x, py - y) <= Math.max(8, spotRadiusPx(t.radius))) return { spot: t.index, part: "at" };
  }
  return null;
}

// Preview the spot list with one spot's position or source changed.
function previewSpotDrag(d) {
  const areas = JSON.parse(JSON.stringify(ui.settings[SPOT_TAG] ?? []));
  const a = areas[d.spot];
  if (!a) return;
  const f6 = (v) => String(Math.round(v * 1e6) / 1e6);
  if (d.part === "source") {
    a.SourceX = f6(d.patch.source[0]);
    a.OffsetY = f6(d.patch.source[1]);
  } else {
    const e = (a.Masks ?? []).find((m) => m.What === "Mask/Ellipse");
    if (!e) return;
    e.X = f6(d.patch.at[0]);
    e.Y = f6(d.patch.at[1]);
    if (d.patch.source) {
      a.SourceX = f6(d.patch.source[0]);
      a.OffsetY = f6(d.patch.source[1]);
    }
  }
  ui.overrides[SPOT_TAG] = areas;
  requestPreview(true);
}

async function addSpotAt(pt) {
  if (sp.busy) return;
  sp.busy = true;
  try {
    const radius = sp.size / 100;
    const source = await invoke("spot_source", { x: pt[0], y: pt[1], radius });
    const before = spotList().length;
    await run([{
      cmd: "add_spot", at: pt, radius, kind: sp.kind, source, source_auto: true,
      feather: sp.feather, opacity: sp.opacity,
    }]);
    if (spotList().length > before) sp.selected = spotList()[spotList().length - 1].index;
    renderSpotPanel();
  } catch (e) {
    toast(String(e), true);
  } finally {
    sp.busy = false;
  }
}

function wireSpots() {
  $("spot-visualise").addEventListener("change", (e) => setVisualise(e.target.checked));
  $("spot-threshold").addEventListener("input", (e) => {
    sp.threshold = Number(e.target.value);
    drawSpots();
  });
  const c = $("spot-canvas");
  const at = (e) => {
    const r = c.getBoundingClientRect();
    return [e.clientX - r.left, e.clientY - r.top];
  };
  c.addEventListener("pointerdown", (e) => {
    if (e.button !== 0 || !sp.frame) return;
    e.preventDefault();
    c.setPointerCapture(e.pointerId);
    const [px, py] = at(e);
    const hit = hitSpot(px, py);
    if (hit) {
      sp.selected = hit.spot;
      const s = selectedSpot();
      sp.drag = { ...hit, start: sFrame(px, py), orig: s, moved: false, patch: {} };
      renderSpotPanel();
    } else {
      sp.drag = { add: true, start: [px, py] };
    }
  });
  c.addEventListener("pointermove", (e) => {
    const [px, py] = at(e);
    sp.hover = [px, py];
    const d = sp.drag;
    if (d && !d.add) {
      const pt = sFrame(px, py);
      const dx = pt[0] - d.start[0];
      const dy = pt[1] - d.start[1];
      if (!d.moved && Math.hypot(dx, dy) < 0.002) return;
      d.moved = true;
      const o = d.orig;
      if (d.part === "source") {
        d.patch = { source: [o.source[0] + dx, o.source[1] + dy] };
      } else {
        // Moving a spot keeps its source where it was relative to it.
        d.patch = { at: [o.at[0] + dx, o.at[1] + dy] };
        if (o.source) d.patch.source = [o.source[0] + dx, o.source[1] + dy];
      }
      previewSpotDrag(d);
    }
    drawSpots();
  });
  c.addEventListener("pointerleave", () => {
    sp.hover = null;
    drawSpots();
  });
  c.addEventListener("pointerup", (e) => {
    const d = sp.drag;
    sp.drag = null;
    if (!d) return;
    if (d.add) {
      const [px, py] = at(e);
      if (Math.hypot(px - d.start[0], py - d.start[1]) < 6) {
        if (sp.selected != null) {
          // A click off every spot first drops the selection.
          sp.selected = null;
          renderSpotPanel();
        }
        addSpotAt(sFrame(px, py));
      }
      return;
    }
    delete ui.overrides[SPOT_TAG];
    if (!d.moved) return;
    const values = d.part === "source" ? { source: d.patch.source } : { at: d.patch.at };
    if (d.part !== "source" && d.patch.source && !d.orig.source_auto) values.source = d.patch.source;
    run([{ cmd: "set_spot", spot: d.spot, values }]);
  });
  c.addEventListener("wheel", (e) => {
    e.preventDefault();
    spotSize(e.deltaY < 0 ? 1 : -1);
  }, { passive: false });
  for (const b of document.querySelectorAll("#spot-kind button")) b.addEventListener("click", () => setSpotKind(b.dataset.kind));
  const slider = (id, key, digits) => {
    const range = $(id);
    const num = $(`${id}-num`);
    range.addEventListener("input", () => (num.value = Number(range.value).toFixed(digits)));
    range.addEventListener("change", () => spotValue(key, Number(range.value)));
    num.addEventListener("change", () => {
      const v = Math.min(Number(range.max), Math.max(Number(range.min), Number(num.value.replace(",", "."))));
      if (Number.isFinite(v)) spotValue(key, v);
    });
  };
  slider("spot-size", "size", 1);
  slider("spot-feather", "feather", 0);
  slider("spot-opacity", "opacity", 0);
  $("spot-delete").addEventListener("click", deleteSpot);
  $("spot-clear").addEventListener("click", () => {
    const n = spotList().length;
    if (!n) return;
    const cmds = spotList().map((s) => s.index).sort((a, b) => b - a).map((spot) => ({ cmd: "remove_spot", spot }));
    run(cmds, "remove all spots");
    sp.selected = null;
  });
  $("spot-done").addEventListener("click", () => exitSpots());
  $("preview").addEventListener("load", () => drawSpots());
  new ResizeObserver(() => drawSpots()).observe($("viewer"));
}

function deleteSpot() {
  const s = selectedSpot();
  if (!s) return;
  sp.selected = null;
  run([{ cmd: "remove_spot", spot: s.index }]);
}

// ---------------------------------------------------------------- enter / leave

async function enterSpots() {
  if (!ui.info || sp.on) return;
  if (ui.crop.on) await exitCrop(true);
  exitMasks();
  exitGuided();
  if (ui.zoom.on) setZoom(false);
  setPicking(false);
  try {
    sp.frame = await invoke("mask_frame");
  } catch (e) {
    toast(String(e), true);
    return;
  }
  sp.on = true;
  $("spot-canvas").hidden = false;
  $("spot-panel").hidden = false;
  $("btn-spots").classList.add("active");
  for (const id of ["btn-zoom", "btn-wb"]) $(id).disabled = true;
  status("Spot removal: click a blemish");
  renderSpotPanel();
}

function exitSpots() {
  if (!sp.on) return;
  sp.on = false;
  sp.drag = null;
  delete ui.overrides[SPOT_TAG];
  $("spot-canvas").hidden = true;
  $("spot-panel").hidden = true;
  $("btn-spots").classList.remove("active");
  for (const id of ["btn-zoom", "btn-wb"]) $(id).disabled = !ui.info;
  status("Ready");
  requestPreview(false);
}

function toggleSpots() {
  if (sp.on) exitSpots();
  else enterSpots();
}

function spotsStateChanged() {
  if (!sp.on) return;
  invoke("mask_frame").then((f) => {
    sp.frame = f;
    renderSpotPanel();
  }).catch(() => renderSpotPanel());
}

function spotsPhotoChanged() {
  sp.selected = null;
  if (sp.on) exitSpots();
}
