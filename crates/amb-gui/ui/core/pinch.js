// SPDX-License-Identifier: GPL-3.0-or-later
// Zooming a picture the way other apps do, shared by the editor's photo and
// the viewer: a mouse wheel, a trackpad pinch and a trackpad's two-finger
// scroll each do their usual thing.
//
// - A pinch reaches the webview as Ctrl+wheel (Linux, Windows) or as
//   WebKit's gesture events (macOS); both zoom at the fingers.
// - A mouse wheel zooms at the pointer, a notch a step.
// - Two fingers on a trackpad scroll: they move a zoomed picture.
// A trackpad's scroll and a mouse wheel arrive as the same event; a wheel
// moves in whole lines or big even steps with nothing sideways.
"use strict";

function isMouseWheel(e) {
  if (e.deltaMode === 1) return true;
  return e.deltaX === 0 && Number.isInteger(e.deltaY) && Math.abs(e.deltaY) >= 40;
}

// Wire `el`: `zoom(factor, x, y)` zooms by `factor` at client point (x, y);
// `scroll(dx, dy, e)` gets a trackpad scroll (true if it used it);
// `wheel(e)` may take a mouse wheel first (true if it used it: stepping
// photos, say). Each returns quickly; zoom is called often.
function wirePinchZoom(el, { zoom, scroll = () => false, wheel = () => false, active = () => true }) {
  let gesture = null;
  el.addEventListener("wheel", (e) => {
    if (!active(e)) return;
    if (e.ctrlKey || e.metaKey) {
      // A pinch (or Ctrl + a wheel): small steps, smoothly.
      e.preventDefault();
      if (gesture) return;
      const d = Math.max(-50, Math.min(50, e.deltaMode === 1 ? e.deltaY * 16 : e.deltaY));
      zoom(Math.exp(-d * 0.01), e.clientX, e.clientY);
      return;
    }
    if (isMouseWheel(e)) {
      e.preventDefault();
      if (wheel(e)) return;
      zoom(e.deltaY < 0 ? 1.25 : 0.8, e.clientX, e.clientY);
      return;
    }
    const k = e.deltaMode === 1 ? 16 : 1;
    if (scroll(e.deltaX * k, e.deltaY * k, e)) e.preventDefault();
  }, { passive: false });
  // macOS: WebKit's own pinch events, with the scale since it began.
  el.addEventListener("gesturestart", (e) => {
    if (!active(e)) return;
    e.preventDefault();
    gesture = { scale: 1, x: e.clientX, y: e.clientY };
  });
  el.addEventListener("gesturechange", (e) => {
    if (!gesture) return;
    e.preventDefault();
    const f = e.scale / gesture.scale;
    gesture.scale = e.scale;
    if (Number.isFinite(f) && f > 0) zoom(f, gesture.x || e.clientX, gesture.y || e.clientY);
  });
  el.addEventListener("gestureend", (e) => {
    if (!gesture) return;
    e.preventDefault();
    gesture = null;
  });
}
