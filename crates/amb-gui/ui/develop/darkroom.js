// SPDX-License-Identifier: GPL-3.0-or-later
// Darkroom aids. In the Film Negative panel: the negative's range and how
// much of the scan clipped, and, where the pointer rests on the photo, the
// zone and density there (the editor command negative_readout; the maths is
// in the engine's negative/readout.rs).
"use strict";

// The pointer is "at rest" after this long without moving, in ms.
const DARKROOM_REST_MS = 120;
// Clipping at the scan from this share of the scan, in percent, is a warning.
const DARKROOM_CLIP_WARN = 0.5;
const DARKROOM_ZONES = ["0", "I", "II", "III", "IV", "V", "VI", "VII", "VIII", "IX", "X"];

const darkroom = {
  row: null, range: null, probe: null, // the panel's lines (built with the panel)
  seq: 0,     // the newest readout asked for; older answers are dropped
  rest: 0,    // the timer of the pointer at rest
};

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
  return darkroom.row;
}

function darkroomConverting() {
  return Boolean(ui.info) && asBool(currentValue("amb:NegativeConvert")) && Boolean(darkroom.row?.isConnected);
}

// After each render: read the negative again (not for a draft, which moves while a slider does).
async function darkroomRefresh(draft = false) {
  if (draft || !darkroom.row?.isConnected) return;
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
  return darkroomConverting() && currentTask() === "film" && !ui.zoom.on && !ui.picking && !ui.before && !ui.split && !sbs.layout;
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
  clearTimeout(darkroom.rest);
  const at = darkroomProbing() ? darkroomAt(e) : null;
  if (!at) return darkroom.probe?.replaceChildren();
  darkroom.rest = setTimeout(() => darkroomProbe(at), DARKROOM_REST_MS);
}

function wireDarkroom() {
  $("viewer").addEventListener("pointermove", darkroomPointer);
  $("viewer").addEventListener("pointerleave", darkroomClearProbe);
}
