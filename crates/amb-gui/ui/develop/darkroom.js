// SPDX-License-Identifier: GPL-3.0-or-later
// Darkroom aids. In the Film Negative panel: the negative's range and how
// much of the scan clipped, and, where the pointer rests on the photo, the
// zone and density there (the editor command negative_readout; the maths is
// in the engine's negative/readout.rs). On any photo: the zone overlay, each
// pixel painted by its zone, worked out in the page from the preview.
"use strict";

// The pointer is "at rest" after this long without moving, in ms.
const DARKROOM_REST_MS = 120;
// Clipping at the scan from this share of the scan, in percent, is a warning.
const DARKROOM_CLIP_WARN = 0.5;
// Ansel Adams' zone system: one stop per zone, V is 18 % grey.
const DARKROOM_GREY = 0.18;
const DARKROOM_MIDDLE = 5;
const DARKROOM_ZONES = ["0", "I", "II", "III", "IV", "V", "VI", "VII", "VIII", "IX", "X"];
// The overlay's colours, deep blue to grey at V to red at X. Data, not
// chrome: they mean the same in every theme.
const ZONE_COLOURS = ["#0a1a5c", "#12379a", "#1f60c8", "#3d92dc", "#86bde2", "#9a9a9a", "#e2c47c", "#ecaa4c", "#ef8a33", "#ee5a26", "#d8201c"];
// How much of the overlay's colour replaces the photo's, and the sample's long edge.
const DARKROOM_PAINT = 0.7;
const DARKROOM_EDGE = 640;

const darkroom = {
  row: null, range: null, probe: null, // the panel's lines (built with the panel)
  seq: 0,     // the newest readout asked for; older answers are dropped
  rest: 0,    // the timer of the pointer at rest
  zonesOn: false, canvas: null, legend: null, cells: [], map: null, mapW: 0, mapH: 0, under: -1,
  rgb: ZONE_COLOURS.map((c) => [1, 3, 5].map((i) => parseInt(c.slice(i, i + 2), 16))),
};

// The zone of a linear value (0..1).
function darkroomZoneOf(linear) {
  const zone = Math.round(Math.log2(Math.max(linear, 1e-9) / DARKROOM_GREY)) + DARKROOM_MIDDLE;
  return Math.min(10, Math.max(0, zone));
}

// The zone of each encoded luma, 0..255 (inverse sRGB first).
const DARKROOM_LUT = Uint8Array.from({ length: 256 }, (_, i) => {
  const v = i / 255;
  return darkroomZoneOf(v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4);
});

// ---------------------------------------------------------------- the panel's readouts

// The lines under the Conversion buttons of the Film Negative panel (the
// layout asks as it builds each section's name row); null for any other.
function darkroomRow(section, panel) {
  if (section !== "Conversion" || panel !== "negative") return null;
  darkroom.range = el("div", { id: "darkroom-range", class: "darkroom-line" });
  darkroom.probe = el("div", { id: "darkroom-probe", class: "darkroom-line" });
  darkroom.row = el("div", { id: "darkroom-readout", class: "darkroom-readout", hidden: true }, darkroom.range, darkroom.probe);
  // A new panel (a layout change) shows what the last render read once it is in the page.
  requestAnimationFrame(() => darkroomRefresh());
  return el("div", { class: "darkroom-block" }, darkroom.row, aidsButtons());
}

function darkroomConverting() {
  return Boolean(ui.info) && asBool(currentValue("amb:NegativeConvert")) && Boolean(darkroom.row?.isConnected);
}

// After each render: read the negative again (not for a draft, which moves while a slider does).
async function darkroomRefresh(draft = false) {
  if (draft || !darkroom.row?.isConnected) return;
  aidsSync();
  if (!darkroomConverting()) {
    darkroom.seq++;
    darkroom.row.hidden = true;
    return;
  }
  const seq = ++darkroom.seq;
  try {
    const r = await invoke("negative_readout", { at: null });
    if (seq === darkroom.seq) darkroomShow(r);
  } catch {
    darkroom.row.hidden = true;
  }
}

// Fill the lines from a readout: range and clipping, and the probe if it has one.
function darkroomShow(r) {
  if (!r) {
    darkroom.row.hidden = true;
    return;
  }
  darkroom.row.hidden = false;
  const [red, green, blue] = r.stats.range.map((v) => v.toFixed(2));
  const pct = r.stats.clipped * 100;
  const warn = pct > DARKROOM_CLIP_WARN;
  const clip = el("span", {
    class: warn ? "darkroom-clip warn" : "darkroom-clip",
    title: warn ? t("Rescan a stop darker, or lower the scanner's exposure") : t("The share of the scan at the sensor's white"),
  }, t("Clipped at scan {pct} %", { pct: pct.toFixed(1) }));
  darkroom.range.replaceChildren(t("Range R {r} · G {g} · B {b} D", { r: red, g: green, b: blue }), " · ", clip);
  if (r.probe) {
    const [d0, d1, d2] = r.probe.density.map((v) => v.toFixed(2));
    darkroom.probe.textContent = t("Under the pointer: Zone {zone} · D {r} / {g} / {b}", { zone: DARKROOM_ZONES[r.probe.zone], r: d0, g: d1, b: d2 });
  }
}

// The pointer on the photo as fractions of it, or null off it.
function darkroomAt(e) {
  const r = $("preview").getBoundingClientRect();
  if (!r.width || !r.height) return null;
  const u = (e.clientX - r.left) / r.width;
  const v = (e.clientY - r.top) / r.height;
  return u >= 0 && u <= 1 && v >= 0 && v <= 1 ? [u, v] : null;
}

// Whether a probe can mean something: the Film tab, the conversion on, the
// whole photo in one piece (not zoomed, split, or while a tool picks).
function darkroomProbing() {
  return darkroomConverting() && currentTask() === "film" && darkroomWhole();
}

// The whole photo in one piece on screen: not zoomed, split, or picked on.
function darkroomWhole() {
  return !ui.zoom.on && !ui.picking && !ui.before && !ui.split && !sbs.layout;
}

function darkroomClearProbe() {
  clearTimeout(darkroom.rest);
  darkroom.seq++;
  darkroom.probe?.replaceChildren();
}

async function darkroomProbe(at) {
  if (!darkroomProbing()) return;
  const seq = ++darkroom.seq;
  try {
    const r = await invoke("negative_readout", { at });
    if (seq === darkroom.seq) darkroomShow(r);
  } catch {
    darkroomClearProbe();
  }
}

function darkroomPointer(e) {
  darkroomUnder(e);
  clearTimeout(darkroom.rest);
  const at = darkroomProbing() ? darkroomAt(e) : null;
  if (!at) return darkroom.probe?.replaceChildren();
  darkroom.rest = setTimeout(() => darkroomProbe(at), DARKROOM_REST_MS);
}

// ---------------------------------------------------------------- the zone overlay

// Paint the preview by zone, with the legend; or take both away.
function darkroomZones() {
  const sample = darkroom.zonesOn ? previewSample(DARKROOM_EDGE) : null;
  const c = sample && (darkroom.canvas ??= el("canvas", { id: "darkroom-zones", class: "zone-overlay darkroom-zones" }));
  const pixels = c && previewPixels(c, sample);
  if (!pixels) {
    darkroom.canvas?.remove();
    darkroom.legend?.remove();
    darkroom.map = null;
    return;
  }
  const px = pixels.data.data;
  const map = new Uint8Array(sample.w * sample.h);
  const keep = 1 - DARKROOM_PAINT;
  for (let i = 0, j = 0; i < px.length; i += 4, j++) {
    const zone = DARKROOM_LUT[Math.round(0.2126 * px[i] + 0.7152 * px[i + 1] + 0.0722 * px[i + 2])];
    const colour = darkroom.rgb[zone];
    map[j] = zone;
    px[i] = px[i] * keep + colour[0] * DARKROOM_PAINT;
    px[i + 1] = px[i + 1] * keep + colour[1] * DARKROOM_PAINT;
    px[i + 2] = px[i + 2] * keep + colour[2] * DARKROOM_PAINT;
  }
  pixels.ctx.putImageData(pixels.data, 0, 0);
  [darkroom.map, darkroom.mapW, darkroom.mapH] = [map, sample.w, sample.h];
  placeOverPreview(c, sample);
  placeOverPreview(darkroomLegend(), sample, 10);
}

// The row of zones 0 to X, each in its colour.
function darkroomLegend() {
  if (darkroom.legend) return darkroom.legend;
  darkroom.cells = ZONE_COLOURS.map((colour, i) => {
    const cell = el("span", { class: "darkroom-cell" }, DARKROOM_ZONES[i]);
    cell.style.setProperty("--zone", colour);
    return cell;
  });
  darkroom.legend = el("div", {
    id: "darkroom-legend",
    class: "darkroom-legend",
    title: t("Zones 0 to X: one stop each, V is middle grey"),
  }, ...darkroom.cells);
  return darkroom.legend;
}

// Outline the zone under the pointer in the legend.
function darkroomUnder(e) {
  if (!darkroom.zonesOn || !darkroom.map) return;
  const at = darkroomAt(e);
  const zone = at ? darkroom.map[Math.min(darkroom.mapH - 1, Math.floor(at[1] * darkroom.mapH)) * darkroom.mapW + Math.min(darkroom.mapW - 1, Math.floor(at[0] * darkroom.mapW))] : -1;
  if (zone === darkroom.under) return;
  darkroom.under = zone;
  darkroom.cells.forEach((cell, i) => cell.classList.toggle("on", i === zone));
}

// The key action: off on start, and not saved.
function darkroomZonesToggle() {
  darkroom.zonesOn = !darkroom.zonesOn;
  darkroomZones();
}

function wireDarkroom() {
  const img = $("preview");
  // A new picture or a new size: paint again once fitPreview has placed it.
  const again = () => requestAnimationFrame(darkroomZones);
  img.addEventListener("load", again);
  new ResizeObserver(again).observe($("viewer"));
  $("viewer").addEventListener("pointermove", darkroomPointer);
  $("viewer").addEventListener("pointerleave", () => {
    darkroomClearProbe();
    darkroomUnder({ clientX: -1, clientY: -1 });
  });
}
