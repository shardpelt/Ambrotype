// SPDX-License-Identifier: GPL-3.0-or-later
// Darkroom aids. In the Film Negative panel: the negative's range and how
// much of the scan clipped (the editor command negative_readout; the maths is
// in the engine's negative/readout.rs).
"use strict";

// Clipping at the scan from this share of the scan, in percent, is a warning.
const DARKROOM_CLIP_WARN = 0.5;

const darkroom = {
  row: null, range: null, // the panel's line (built with the panel)
  seq: 0,                 // the newest readout asked for; older answers are dropped
};

// ---------------------------------------------------------------- the panel's readouts

// The lines under the Conversion buttons of the Film Negative panel (the
// layout asks as it builds each section's name row); null for any other.
function darkroomRow(section, panel) {
  if (section !== "Conversion" || panel !== "negative") return null;
  darkroom.range = el("div", { id: "darkroom-range", class: "darkroom-line" });
  darkroom.row = el("div", { id: "darkroom-readout", class: "darkroom-readout", hidden: true }, darkroom.range);
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

// Fill the line from a readout: the range and the clipping.
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
}
