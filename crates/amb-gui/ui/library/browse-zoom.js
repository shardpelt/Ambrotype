// SPDX-License-Identifier: GPL-3.0-or-later
// Zoom in Timeline and Insights: Ctrl + wheel, a pinch and Ctrl + / − / 0
// scale the view's content (the calendar's cells and thumbnails, the charts).
// One scale per view, 0.6 to 2.5, remembered on this computer and applied as
// `--browse-scale` on the view's box (style.css reads it). A plain wheel
// still scrolls. The Map has its own zoom (map-touch.js).
"use strict";

const BROWSE_SCALE_MIN = 0.6;
const BROWSE_SCALE_MAX = 2.5;
const BROWSE_SCALE_STEP = 1.25;   // Ctrl + / −: one step
const BROWSE_ZOOMED = ["timeline", "insights"];

const browseZoom = { scale: { timeline: 1, insights: 1 } };

// Is a zoomable browse view what's showing?
const browseZoomOn = () => lib.on && BROWSE_ZOOMED.includes(cull.view);

// Put the current view's scale on its box (1 for the Map).
function browseApplyScale() {
  const scale = browseZoom.scale[cull.view] ?? 1;
  $("library-view").style.setProperty("--browse-scale", String(scale));
}

function browseSetScale(view, scale) {
  const wanted = Number.isFinite(scale) ? scale : 1;
  const s = Math.round(Math.min(BROWSE_SCALE_MAX, Math.max(BROWSE_SCALE_MIN, wanted)) * 100) / 100;
  browseZoom.scale[view] = s;
  store.setText(`amb.browseScale.${view}`, String(s));
  if (cull.view === view) browseApplyScale();
}

// Ctrl + = − 0 in Timeline and Insights (true when one was showing).
function browseZoomKey(dir) {
  if (!browseZoomOn()) return false;
  const now = browseZoom.scale[cull.view];
  browseSetScale(cull.view, dir > 0 ? now * BROWSE_SCALE_STEP : dir < 0 ? now / BROWSE_SCALE_STEP : 1);
  return true;
}

function wireBrowseZoom() {
  for (const view of BROWSE_ZOOMED) browseZoom.scale[view] = Number(store.text(`amb.browseScale.${view}`, "1")) || 1;
  // (A pinch on the Map reaches the Map's own listener first; `active` leaves it alone here.)
  wirePinchZoom($("library-view"), {
    wheelScrolls: true,
    active: () => browseZoomOn(),
    zoom: (factor) => browseSetScale(cull.view, browseZoom.scale[cull.view] * factor),
  });
}
