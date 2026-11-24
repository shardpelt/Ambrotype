// SPDX-License-Identifier: GPL-3.0-or-later
// Snapping while a frame of a free collage is dragged: its left, centre and
// right (top, middle and bottom) hold to the same lines of the other frames
// and of the canvas, and a thin guide shows where. Alt (held during the
// drag) turns it off.
"use strict";

const COLLAGE_SNAP_PX = 6; // screen pixels within which a line holds

// The lines of a frame's bounding box (turned frames included), in canvas
// pixels: x for left, centre, right; y for top, middle, bottom.
function snapLines(r) {
  const rad = ((r.angle ?? 0) * Math.PI) / 180;
  const [cos, sin] = [Math.abs(Math.cos(rad)), Math.abs(Math.sin(rad))];
  const [hw, hh] = [(r.w * cos + r.h * sin) / 2, (r.w * sin + r.h * cos) / 2];
  const [cx, cy] = [r.x + r.w / 2, r.y + r.h / 2];
  return { x: [cx - hw, cx, cx + hw], y: [cy - hh, cy, cy + hh] };
}

// The closest pull, as { d, at }, of one of `mine` to one of `theirs`.
function snapPull(mine, theirs, reach) {
  let best = null;
  for (const m of mine) {
    for (const g of theirs) {
      const d = g - m;
      if (Math.abs(d) <= reach && (!best || Math.abs(d) < Math.abs(best.d))) best = { d, at: g };
    }
  }
  return best;
}

// Frame `i` (size and angle from `f`) wanted centred at (cx, cy): where it
// ends up, and the guides for the lines it holds to.
function collageSnap(i, f, cx, cy) {
  const t = collage.t;
  const reach = COLLAGE_SNAP_PX / Number($("col-stage").dataset.scale);
  const to = { x: [0, t.width / 2, t.width], y: [0, t.height / 2, t.height] };
  cellRects(t).forEach((r, k) => {
    if (k === i) return;
    const l = snapLines(r);
    to.x.push(...l.x);
    to.y.push(...l.y);
  });
  const mine = snapLines({ x: cx - f.w / 2, y: cy - f.h / 2, w: f.w, h: f.h, angle: f.angle });
  const sx = snapPull(mine.x, to.x, reach);
  const sy = snapPull(mine.y, to.y, reach);
  collage.snap = { x: sx?.at ?? null, y: sy?.at ?? null };
  collageGuides();
  return { cx: cx + (sx?.d ?? 0), cy: cy + (sy?.d ?? 0) };
}

// No guides: the drag ended, or snapping is off.
function collageSnapClear() {
  collage.snap = { x: null, y: null };
  collageGuides();
}

function collageGuides() {
  const stage = $("col-stage");
  const scale = Number(stage.dataset.scale);
  stage.querySelectorAll(".col-guide").forEach((g) => g.remove());
  for (const [axis, at] of [["v", collage.snap.x], ["h", collage.snap.y]]) {
    if (at == null) continue;
    const g = el("div", { class: `col-guide ${axis}` });
    g.style[axis === "v" ? "left" : "top"] = `${at * scale}px`;
    stage.append(g);
  }
}
