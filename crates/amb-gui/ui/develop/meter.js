// SPDX-License-Identifier: GPL-3.0-or-later
// Film negative: the box Auto measures in, drawn by hand (Meter area). It is
// kept like the crop, in the photo's stored orientation: the editor commands
// meter_area and meter_area_shown turn it from and to the photo as shown.
"use strict";

const METER_TAGS = ["amb:NegativeMeterLeft", "amb:NegativeMeterTop", "amb:NegativeMeterRight", "amb:NegativeMeterBottom"];
const meterArea = { shown: null }; // the stored box on the photo as shown, while the tool is active

// The button: start drawing; with Alt, back to measuring the whole crop.
async function meterAreaFlow(whole = false) {
  if (!ui.info) return;
  if (whole) {
    setPicking(false);
    run(METER_TAGS.map((name) => ({ cmd: "reset_param", name })), "Meter the whole crop");
    return;
  }
  if (ui.picking === "meter") return setPicking(false);
  meterArea.shown = await invoke("meter_area_shown").catch(() => null);
  setPicking("meter");
}

// The box was dragged between two points (fractions of the photo as shown).
async function meterAreaDrawn(from, to) {
  setPicking(false);
  try {
    const r = await invoke("meter_area", { shown: [...from, ...to] });
    setState(r.state);
    toast(r.notes.join(" "));
    requestPreview(false);
  } catch (e) {
    showError(e);
  }
}

// While the tool is active: the stored box, or the one being dragged (`drag`
// and the mouse event); otherwise hidden.
function showMeterBox(drag = null, e = null) {
  let box = $("meter-box");
  if (!box) {
    box = el("div", { id: "meter-box", hidden: true });
    document.body.append(box);
  }
  box.hidden = ui.picking !== "meter" || (!drag && (ui.zoom.on || !meterArea.shown));
  if (box.hidden) return;
  const r = $("preview").getBoundingClientRect();
  let x0, y0, x1, y1;
  if (drag) {
    const at = (v, lo, hi) => Math.min(hi, Math.max(lo, v));
    [x0, x1] = [drag.x, e.clientX].map((v) => at(v, r.left, r.right)).sort((a, b) => a - b);
    [y0, y1] = [drag.y, e.clientY].map((v) => at(v, r.top, r.bottom)).sort((a, b) => a - b);
  } else {
    const [left, top, right, bottom] = meterArea.shown;
    [x0, y0, x1, y1] = [r.left + left * r.width, r.top + top * r.height, r.left + right * r.width, r.top + bottom * r.height];
  }
  Object.assign(box.style, { left: `${x0}px`, top: `${y0}px`, width: `${x1 - x0}px`, height: `${y1 - y0}px` });
}
