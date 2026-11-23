// SPDX-License-Identifier: GPL-3.0-or-later
// Guided dust search in the spot tool: Alt-drag a box, or brush a stroke with
// "Find in stroke" on, and the dust search looks harder there than Find dust
// does on the whole scan (docs/design/dust.md). Each is one undo step.
"use strict";

const dustGuided = {
  brush: false,   // a brushed stroke searches along itself instead of becoming a spot
};

// Run `command` and say what it did. Nothing found: say so, with what to do.
async function dustGuidedRun(command, args) {
  const before = spotList().length;
  try {
    const r = await invoke(command, args);
    setState(r.state);
    toast(spotList().length > before ? r.notes.join(" ") : t("No dust found there. A plain spot heals it by hand."));
    requestPreview(false);
    drawSpots();
  } catch (err) {
    showError(err);
  }
}

// The box dragged with Alt, as frame fractions (corners in any order).
function dustFindIn(a, b) {
  return dustGuidedRun("find_dust_in", {
    area: [Math.min(a[0], b[0]), Math.min(a[1], b[1]), Math.max(a[0], b[0]), Math.max(a[1], b[1])],
  });
}

// The brushed stroke (frame fractions) with the brush's size.
function dustFindAlong(points) {
  return dustGuidedRun("find_dust_along", { points, radius: sp.size / 100 });
}

function dustSetBrush(on) {
  dustGuided.brush = on;
  $("spot-dust-brush").checked = on;
  status(on ? t("Find in stroke: brush over where dust is") : t("Spot removal: click a blemish"));
}

function wireDustGuided() {
  $("spot-dust-brush").addEventListener("change", (e) => dustSetBrush(e.target.checked));
}
