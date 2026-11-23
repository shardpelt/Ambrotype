// SPDX-License-Identifier: GPL-3.0-or-later
// Zoom in the viewer (V, F in the library) and in the library's Compare, as
// in Develop: the mouse wheel or a pinch zooms at the pointer, a
// click in the middle goes to 100 % there and back, Ctrl + and − step, Ctrl 0
// fits. Zoomed in, dragging or two fingers on a trackpad move round the
// photo, and ← → still step (back to the whole photo). A bigger picture is
// fetched once zoomed past the first one's detail. In the viewer a
// right-drag zooms to a box. makeZoom is the shared part; Compare's two
// photos are one zoom, so they move together.
"use strict";

// `host` gets the events; `images()` are the <img>s that take the zoom,
// `all()` every one to clear on reset; `frame(cx, cy)` is the box (client
// px) the picture is fitted into, the one under the point if there are
// several; `sharper(img)` fetches a bigger picture; `active(e)` says if an
// event is ours; `box` turns on the right-drag box zoom.
function makeZoom({ host, images, all = images, frame, sharper = () => {}, active = () => true, box = false }) {
  const z = { s: 1, x: 0, y: 0, drag: null, box: null, big: null };
  const centre = (f) => [f.left + f.width / 2, f.top + f.height / 2];

  // The picture on screen and its fitted size (CSS px).
  z.fitted = () => {
    const img = images()[0];
    if (!img?.naturalWidth) return null;
    const f = frame();
    const k = Math.min(f.width / img.naturalWidth, f.height / img.naturalHeight);
    return { img, w: img.naturalWidth * k, h: img.naturalHeight * k };
  };

  z.apply = () => {
    const sh = z.fitted();
    if (!sh) return;
    const f = frame();
    // Keep the photo over the frame: no empty edge while it's larger.
    const mx = Math.max(0, (sh.w * z.s - f.width) / 2);
    const my = Math.max(0, (sh.h * z.s - f.height) / 2);
    z.x = Math.min(mx, Math.max(-mx, z.x));
    z.y = Math.min(my, Math.max(-my, z.y));
    const t = z.s === 1 ? "" : `translate(${z.x}px, ${z.y}px) scale(${z.s})`;
    for (const img of images()) img.style.transform = t;
    host.classList.toggle("zoomed", z.s > 1);
    if (z.s > 1.2) for (const img of images()) sharper(img);
  };

  // Back to the whole photo (stepping, or Ctrl 0).
  z.reset = () => {
    z.s = 1;
    z.x = z.y = 0;
    for (const img of all()) img.style.transform = "";
    host.classList.remove("zoomed");
  };

  // The zoom at which one of the photo's pixels is one of the screen's
  // (as far as the picture fetched goes).
  z.full = (sh) => sh.img.naturalWidth / (sh.w * (window.devicePixelRatio || 1));

  // Zoom by `f` keeping the point under (cx, cy) where it is; 1 to 8 times
  // the fitted size, or more when the photo has the detail for it.
  z.zoomAt = (f, cx, cy) => {
    const sh = z.fitted();
    if (!sh) return;
    const most = Math.max(8, z.full(sh) * 2);
    const s2 = Math.min(most, Math.max(1, z.s * f));
    if (s2 === 1) return z.reset();
    const [mx, my] = centre(frame(cx, cy));
    z.x = cx - mx - ((cx - mx - z.x) * s2) / z.s;
    z.y = cy - my - ((cy - my - z.y) * s2) / z.s;
    z.s = s2;
    z.apply();
  };

  // A click in the middle: 100 % at the pointer, or back to the whole photo.
  z.toggleFull = (cx, cy) => {
    if (z.s > 1) return z.reset();
    const sh = z.fitted();
    if (sh) z.zoomAt(Math.max(2, z.full(sh)), cx, cy);
  };

  // Ctrl + and − step from the middle.
  z.step = (dir) => z.zoomAt(dir > 0 ? 1.5 : 1 / 1.5, ...centre(frame()));

  z.key = (e) => {
    const combo = comboOf(e);
    if (["Ctrl+=", "Ctrl++", "Ctrl+Shift++"].includes(combo)) return z.step(1), true;
    if (combo === "Ctrl+-") return z.step(-1), true;
    if (combo === "Ctrl+0") return z.reset(), true;
    return false;
  };

  z.wire = () => {
    wirePinchZoom(host, {
      active,
      zoom: z.zoomAt,
      scroll: (dx, dy) => {
        if (z.s === 1) return false;
        z.x -= dx;
        z.y -= dy;
        z.apply();
        return true;
      },
    });
    // Dragging moves a zoomed photo; a right-drag zooms to the box drawn.
    host.addEventListener("pointerdown", (e) => {
      if (!active(e)) return;
      delete host.dataset.dragged;
      if (e.button === 0 && z.s > 1) {
        z.drag = { x: e.clientX, y: e.clientY, ox: z.x, oy: z.y, moved: false };
      } else if (e.button === 2 && box) {
        z.box = { x: e.clientX, y: e.clientY, el: Object.assign(document.createElement("div"), { className: "zoom-box" }), moved: false };
      }
    });
    host.addEventListener("pointermove", (e) => {
      if (z.drag) {
        const dx = e.clientX - z.drag.x;
        const dy = e.clientY - z.drag.y;
        if (Math.abs(dx) + Math.abs(dy) > 3) z.drag.moved = true;
        z.x = z.drag.ox + dx;
        z.y = z.drag.oy + dy;
        z.apply();
      } else if (z.box) {
        const b = z.box;
        if (!b.moved && Math.abs(e.clientX - b.x) + Math.abs(e.clientY - b.y) > 6) {
          b.moved = true;
          host.append(b.el);
        }
        Object.assign(b.el.style, { left: `${Math.min(b.x, e.clientX)}px`, top: `${Math.min(b.y, e.clientY)}px`, width: `${Math.abs(e.clientX - b.x)}px`, height: `${Math.abs(e.clientY - b.y)}px` });
      }
    });
    host.addEventListener("pointerup", (e) => {
      if (z.box) {
        const b = z.box;
        z.box = null;
        b.el.remove();
        if (b.moved) {
          const w = Math.max(8, Math.abs(e.clientX - b.x));
          const h = Math.max(8, Math.abs(e.clientY - b.y));
          const cx = (b.x + e.clientX) / 2;
          const cy = (b.y + e.clientY) / 2;
          const f = frame(cx, cy);
          // Zoom at the box's centre, then bring that centre to the middle.
          z.zoomAt(Math.min(f.width / w, f.height / h), cx, cy);
          const [mx, my] = centre(f);
          z.x += mx - cx;
          z.y += my - cy;
          z.apply();
          host.dataset.boxed = "1";
        }
      }
      if (z.drag) {
        if (z.drag.moved) host.dataset.dragged = "1";
        z.drag = null;
      }
    });
  };
  return z;
}

// The long edge of the bigger picture fetched once zoomed past the first one's
// detail (px): about a 24 MP photo whole.
const ZOOM_BIG_EDGE = 6144;

// A bigger picture of photo `p` than `img` shows (its edit's render when it
// has one), decoded and ready; null when there is none.
async function zoomBigger(p, img) {
  try {
    const url = await bigImage(p, ZOOM_BIG_EDGE, Boolean(p.edited)).catch(() => bigImage(p, ZOOM_BIG_EDGE, false));
    const probe = new Image();
    probe.src = url;
    await probe.decode();
    return probe.naturalWidth > img.naturalWidth ? url : null;
  } catch {
    return null; // the smaller picture stays
  }
}

// ---------------------------------------------------------------- the viewer

// The viewer's zoom, made when it opens.
const vz = { z: null };

const viewerFrame = () => ({ left: 0, top: 0, width: innerWidth, height: innerHeight });

function wireViewerZoom(el) {
  vz.z = makeZoom({
    host: el,
    images: () => [...(el.querySelectorAll(".ss-img.shown"))],
    all: () => el.querySelectorAll(".ss-img"),
    frame: viewerFrame,
    sharper: viewerSharper,
    active: (e) => !e.target.closest(".ss-info, .ss-bar"),
    box: true,
  });
  vz.z.wire();
}

// Zoomed past the picture's detail: fetch the photo larger, once.
async function viewerSharper(img) {
  const p = slides.list[slides.at];
  if (!p || vz.z.big === p.path) return;
  vz.z.big = p.path;
  const url = await zoomBigger(p, img);
  if (!url || slides.list[slides.at] !== p) return;
  // Same fitted size, so the zoom stays put.
  img.src = url;
  await img.decode().catch(() => {});
}
