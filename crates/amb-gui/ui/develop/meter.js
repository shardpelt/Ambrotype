// SPDX-License-Identifier: GPL-3.0-or-later
// Film negative: the box Auto measures in, drawn by hand (Meter area).

const METER_TAGS = ["amb:NegativeMeterLeft", "amb:NegativeMeterTop", "amb:NegativeMeterRight", "amb:NegativeMeterBottom"];

// The button: start drawing; with Alt, back to measuring the whole crop.
function meterAreaFlow(whole = false) {
  if (!ui.info) return;
  if (whole) {
    setPicking(false);
    run(METER_TAGS.map((name) => ({ cmd: "reset_param", name })), "Meter the whole crop");
    return;
  }
  setPicking(ui.picking === "meter" ? false : "meter");
}

// The box was drawn between two points (fractions of the photo as shown).
function meterAreaDrawn(from, to) {
  setPicking(false);
  const c = (v) => Math.round(Math.min(1, Math.max(0, v)) * 10000) / 10000;
  const [l, r] = [c(Math.min(from[0], to[0])), c(Math.max(from[0], to[0]))];
  const [t, b] = [c(Math.min(from[1], to[1])), c(Math.max(from[1], to[1]))];
  if (r - l < 0.02 || b - t < 0.02) return toast("That box is too small to measure.");
  const set = (name, value) => ({ cmd: "set_param", name, value });
  run([...[l, t, r, b].map((v, i) => set(METER_TAGS[i], v)), set("amb:NegativeConvert", true)], "Meter area");
}

// While the tool is active: the stored box, or the one being dragged (`drag`
// and the mouse event); otherwise hidden.
function showMeterBox(drag = null, e = null) {
  let box = $("meter-box");
  if (!box) {
    box = document.createElement("div");
    box.id = "meter-box";
    box.hidden = true;
    document.body.append(box);
  }
  box.hidden = ui.picking !== "meter";
  if (box.hidden) return;
  const r = $("preview").getBoundingClientRect();
  let x0, y0, x1, y1;
  if (drag) {
    const at = (v, lo, hi) => Math.min(hi, Math.max(lo, v));
    [x0, x1] = [drag.x, e.clientX].map((v) => at(v, r.left, r.right)).sort((a, b) => a - b);
    [y0, y1] = [drag.y, e.clientY].map((v) => at(v, r.top, r.bottom)).sort((a, b) => a - b);
  } else if (ui.zoom.on) {
    box.hidden = true;
    return;
  } else {
    const [l, t, rt, bt] = METER_TAGS.map((n, i) => Number(currentValue(n) ?? (i < 2 ? 0 : 1)));
    [x0, y0, x1, y1] = [r.left + l * r.width, r.top + t * r.height, r.left + rt * r.width, r.top + bt * r.height];
  }
  Object.assign(box.style, { left: `${x0}px`, top: `${y0}px`, width: `${x1 - x0}px`, height: `${y1 - y0}px` });
}
