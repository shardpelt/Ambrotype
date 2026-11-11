// SPDX-License-Identifier: GPL-3.0-or-later
// Zoom in the viewer (V, F in the library), as in Develop: the mouse wheel
// or a pinch zooms at the pointer, a click in the middle goes to 100 % there
// and back, a right-drag zooms to a box, Ctrl + and − step, Ctrl 0 fits.
// Zoomed in, dragging or two fingers on a trackpad move round the photo,
// and ← → still step (back to the whole photo). A bigger picture is
// fetched once zoomed past the first one's detail.
"use strict";

const vz = { s: 1, x: 0, y: 0, img: null, big: null, drag: null, box: null };

// The picture on screen and its fitted size (CSS px).
function vzShown() {
  const img = slides.el?.querySelector(".ss-img.shown");
  if (!img?.naturalWidth) return null;
  const k = Math.min(innerWidth / img.naturalWidth, innerHeight / img.naturalHeight);
  return { img, w: img.naturalWidth * k, h: img.naturalHeight * k };
}

function vzApply() {
  const sh = vzShown();
  if (!sh) return;
  // Keep the photo over the screen: no empty edge while it's larger.
  const mx = Math.max(0, (sh.w * vz.s - innerWidth) / 2);
  const my = Math.max(0, (sh.h * vz.s - innerHeight) / 2);
  vz.x = Math.min(mx, Math.max(-mx, vz.x));
  vz.y = Math.min(my, Math.max(-my, vz.y));
  sh.img.style.transform = vz.s === 1 ? "" : `translate(${vz.x}px, ${vz.y}px) scale(${vz.s})`;
  slides.el.classList.toggle("zoomed", vz.s > 1);
  if (vz.s > 1.2) vzSharper(sh);
}

// Back to the whole photo (stepping, or Ctrl 0).
function vzReset() {
  vz.s = 1;
  vz.x = vz.y = 0;
  for (const img of slides.el?.querySelectorAll(".ss-img") ?? []) img.style.transform = "";
  slides.el?.classList.remove("zoomed");
}

// Zoom by `f` keeping the point under (cx, cy) where it is; 1 to 8 times
// the fitted size, or more when the photo has the detail for it.
function vzZoom(f, cx, cy) {
  const sh = vzShown();
  if (!sh) return;
  const most = Math.max(8, vzFull(sh) * 2);
  const s2 = Math.min(most, Math.max(1, vz.s * f));
  if (s2 === 1) return vzReset();
  const px = cx - innerWidth / 2;
  const py = cy - innerHeight / 2;
  vz.x = px - ((px - vz.x) * s2) / vz.s;
  vz.y = py - ((py - vz.y) * s2) / vz.s;
  vz.s = s2;
  vzApply();
}

// The zoom at which one of the photo's pixels is one of the screen's
// (as far as the picture fetched goes).
function vzFull(sh) {
  const p = slides.list[slides.at];
  const natural = vz.big?.path === p?.path ? vz.big.w : sh.img.naturalWidth;
  return natural / (sh.w * (window.devicePixelRatio || 1));
}

// Zoomed past the picture's detail: fetch the photo larger, once.
async function vzSharper(sh) {
  const p = slides.list[slides.at];
  if (!p || vz.big?.path === p.path) return;
  vz.big = { path: p.path, w: sh.img.naturalWidth };
  try {
    const url = await bigImage(p, 6144, Boolean(p.edited)).catch(() => bigImage(p, 6144, false));
    if (slides.list[slides.at] !== p) return;
    const img = vzShown()?.img;
    if (!img) return;
    const probe = new Image();
    probe.src = url;
    await probe.decode();
    if (slides.list[slides.at] !== p || probe.naturalWidth <= img.naturalWidth) return;
    // Same fitted size, so the zoom stays put.
    img.src = url;
    await img.decode().catch(() => {});
    vz.big.w = probe.naturalWidth;
  } catch {
    /* the smaller picture stays */
  }
}

// A click in the middle: 100 % at the pointer, or back to the whole photo.
function vzClick(e) {
  if (vz.s > 1) return vzReset();
  const sh = vzShown();
  if (!sh) return;
  vzZoom(Math.max(2, vzFull(sh)), e.clientX, e.clientY);
}

function vzKey(e) {
  const combo = comboOf(e);
  const mid = [innerWidth / 2, innerHeight / 2];
  if (["Ctrl+=", "Ctrl++", "Ctrl+Shift++"].includes(combo)) return vzZoom(1.5, ...mid), true;
  if (combo === "Ctrl+-") return vzZoom(1 / 1.5, ...mid), true;
  if (combo === "Ctrl+0") return vzReset(), true;
  return false;
}

function wireViewerZoom(el) {
  vzReset();
  vz.big = null;
  wirePinchZoom(el, {
    active: (e) => !e.target.closest(".ss-info, .ss-bar"),
    zoom: vzZoom,
    scroll: (dx, dy) => {
      if (vz.s === 1) return false;
      vz.x -= dx;
      vz.y -= dy;
      vzApply();
      return true;
    },
  });
  // Dragging moves a zoomed photo; a right-drag zooms to the box drawn.
  el.addEventListener("pointerdown", (e) => {
    if (e.target.closest(".ss-info, .ss-bar")) return;
    if (e.button === 0 && vz.s > 1) {
      vz.drag = { x: e.clientX, y: e.clientY, ox: vz.x, oy: vz.y, moved: false };
    } else if (e.button === 2) {
      const box = Object.assign(document.createElement("div"), { className: "zoom-box" });
      vz.box = { x: e.clientX, y: e.clientY, el: box, moved: false };
    }
  });
  el.addEventListener("pointermove", (e) => {
    if (vz.drag) {
      const dx = e.clientX - vz.drag.x;
      const dy = e.clientY - vz.drag.y;
      if (Math.abs(dx) + Math.abs(dy) > 3) vz.drag.moved = true;
      vz.x = vz.drag.ox + dx;
      vz.y = vz.drag.oy + dy;
      vzApply();
    } else if (vz.box) {
      const b = vz.box;
      if (!b.moved && Math.abs(e.clientX - b.x) + Math.abs(e.clientY - b.y) > 6) {
        b.moved = true;
        el.append(b.el);
      }
      Object.assign(b.el.style, { left: `${Math.min(b.x, e.clientX)}px`, top: `${Math.min(b.y, e.clientY)}px`, width: `${Math.abs(e.clientX - b.x)}px`, height: `${Math.abs(e.clientY - b.y)}px` });
    }
  });
  el.addEventListener("pointerup", (e) => {
    if (vz.box) {
      const b = vz.box;
      vz.box = null;
      b.el.remove();
      if (b.moved) {
        b.used = true;
        const w = Math.max(8, Math.abs(e.clientX - b.x));
        const h = Math.max(8, Math.abs(e.clientY - b.y));
        const f = Math.min(innerWidth / w, innerHeight / h);
        // Zoom at the box's centre, then bring that centre to the middle.
        const cx = (b.x + e.clientX) / 2;
        const cy = (b.y + e.clientY) / 2;
        vzZoom(f, cx, cy);
        vz.x += innerWidth / 2 - cx;
        vz.y += innerHeight / 2 - cy;
        vzApply();
        el.dataset.boxed = "1";
      }
    }
    if (vz.drag) {
      if (vz.drag.moved) el.dataset.dragged = "1";
      vz.drag = null;
    }
  });
}
