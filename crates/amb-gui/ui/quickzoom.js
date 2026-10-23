// SPDX-License-Identifier: GPL-3.0-or-later
// Quick ways to check detail. Right-drag a box on the photo and the view
// zooms to fill the window with it; Z or Space goes back to the whole photo
// (Settings can turn the box off, and a right click then exports as before).
// The loupe (I) shows the part under the pointer at 100% in a circle, so an
// edit can be checked anywhere without zooming in; the wheel changes its
// magnification.
"use strict";

const qz = { box: null, loupe: null, mag: 1, timer: null, busy: false, at: null };

function boxZoomWanted() {
  try { return localStorage.getItem("amb.boxZoom") !== "0"; } catch { return true; }
}

// Fractions (u, v) of the upright photo under a point, clamped to it.
function photoAt(x, y) {
  const r = $("preview").getBoundingClientRect();
  const fx = Math.min(1, Math.max(0, (x - r.left) / r.width));
  const fy = Math.min(1, Math.max(0, (y - r.top) / r.height));
  return photoPoint({ clientX: r.left + fx * r.width, clientY: r.top + fy * r.height });
}

function wireBoxZoom() {
  const viewer = $("viewer");
  const img = $("preview");
  viewer.addEventListener("mousedown", (e) => {
    if (e.button !== 2 || !ui.info || !boxZoomWanted() || e.target !== img || ui.picking) return;
    e.preventDefault();
    const el = Object.assign(document.createElement("div"), { className: "zoom-box" });
    qz.box = { x: e.clientX, y: e.clientY, el, moved: false };
  });
  window.addEventListener("mousemove", (e) => {
    const b = qz.box;
    if (!b) return;
    const w = e.clientX - b.x, h = e.clientY - b.y;
    if (!b.moved && Math.abs(w) + Math.abs(h) < 6) return;
    if (!b.moved) {
      b.moved = true;
      document.body.append(b.el);
    }
    Object.assign(b.el.style, {
      left: `${Math.min(b.x, e.clientX)}px`, top: `${Math.min(b.y, e.clientY)}px`,
      width: `${Math.abs(w)}px`, height: `${Math.abs(h)}px`,
    });
  });
  window.addEventListener("mouseup", (e) => {
    const b = qz.box;
    if (!b || e.button !== 2) return;
    qz.box = null;
    b.el.remove();
    // A right click without a drag: what right-click did before.
    if (!b.moved) return exportFlow();
    const [u0, v0] = photoAt(b.x, b.y);
    const [u1, v1] = photoAt(e.clientX, e.clientY);
    const w = Math.abs(u1 - u0), h = Math.abs(v1 - v0);
    if (w < 0.002 || h < 0.002) return;
    setZoom(true, (u0 + u1) / 2, (v0 + v1) / 2, { w, h });
  });
  const steps = $("settings-wheel-photos");
  steps.checked = wheelSteps();
  steps.addEventListener("change", () => {
    try { localStorage.setItem("amb.wheelPhotos", steps.checked ? "1" : "0"); } catch { /* not kept */ }
  });
  const pref = $("settings-box-zoom");
  if (pref) {
    pref.checked = boxZoomWanted();
    pref.addEventListener("change", () => {
      try { localStorage.setItem("amb.boxZoom", pref.checked ? "1" : "0"); } catch { /* not kept */ }
    });
  }
}

// ------------------------------------------------------------------ loupe

function toggleLoupe(on = !qz.loupe) {
  if (!on) {
    qz.loupe?.remove();
    qz.loupe = null;
    clearTimeout(qz.timer);
    $("viewer").classList.remove("loupe-on");
    status("Ready");
    return true;
  }
  if (!ui.info) return false;
  const el = Object.assign(document.createElement("div"), { className: "loupe", hidden: true });
  el.append(document.createElement("img"), Object.assign(document.createElement("span"), { className: "loupe-mag" }));
  $("viewer").append(el);
  $("viewer").classList.add("loupe-on");
  qz.loupe = el;
  status("Loupe: point at the photo to see it at 100% · scroll to magnify · I or Esc closes");
  return true;
}

const LOUPE = 220;

function moveLoupe(e) {
  const el = qz.loupe;
  const img = $("preview");
  if (!el || ui.zoom.on || img.hidden) return;
  const r = img.getBoundingClientRect();
  const inside = e.clientX >= r.left && e.clientX <= r.right && e.clientY >= r.top && e.clientY <= r.bottom;
  el.hidden = !inside;
  if (!inside) return;
  const v = $("viewer").getBoundingClientRect();
  Object.assign(el.style, { left: `${e.clientX - v.left - LOUPE / 2}px`, top: `${e.clientY - v.top - LOUPE / 2}px` });
  qz.at = photoPoint(e);
  // At once: the preview itself, enlarged; then the real pixels.
  const k = 3;
  el.style.backgroundImage = `url(${img.src})`;
  el.style.backgroundSize = `${r.width * k}px ${r.height * k}px`;
  el.style.backgroundPosition = `${LOUPE / 2 - (e.clientX - r.left) * k}px ${LOUPE / 2 - (e.clientY - r.top) * k}px`;
  el.querySelector("img").style.visibility = "hidden";
  clearTimeout(qz.timer);
  qz.timer = setTimeout(loupeRender, 90);
}

async function loupeRender() {
  if (!qz.loupe || !qz.at || qz.busy) return;
  qz.busy = true;
  const at = qz.at;
  const dpr = window.devicePixelRatio || 1;
  const side = Math.round((LOUPE * dpr) / qz.mag);
  try {
    const { meta, jpeg } = splitFramed(await invoke("region", { cx: at[0], cy: at[1], width: side, height: side, before: ui.before, clipping: ui.clipping }));
    if (!qz.loupe) return;
    const pic = qz.loupe.querySelector("img");
    const url = URL.createObjectURL(new Blob([jpeg], { type: "image/jpeg" }));
    pic.onload = () => URL.revokeObjectURL(url);
    pic.src = url;
    // The crop may be cut by the photo's edge: keep the point in the middle.
    const k = qz.mag / dpr;
    Object.assign(pic.style, {
      width: `${meta.width * k}px`, height: `${meta.height * k}px`,
      left: `${LOUPE / 2 - (at[0] * meta.full_width - meta.x) * k}px`,
      top: `${LOUPE / 2 - (at[1] * meta.full_height - meta.y) * k}px`,
      visibility: "visible",
    });
    qz.loupe.querySelector(".loupe-mag").textContent = `${qz.mag * 100}%`;
  } catch {
    /* the next move tries again */
  } finally {
    qz.busy = false;
    if (qz.at !== at) loupeRender();
  }
}

function wireLoupe() {
  window.addEventListener("mousemove", moveLoupe);
  $("viewer").addEventListener("wheel", (e) => {
    if (!qz.loupe || ui.zoom.on) return;
    e.preventDefault();
    qz.mag = e.deltaY < 0 ? Math.min(4, qz.mag * 2) : Math.max(1, qz.mag / 2);
    loupeRender();
  }, { passive: false });
  window.addEventListener("keydown", (e) => {
    if (e.key === "Escape" && qz.loupe && !document.querySelector("dialog[open]")) {
      e.stopPropagation();
      toggleLoupe(false);
    }
  }, true);
}
