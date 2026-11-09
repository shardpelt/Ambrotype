// SPDX-License-Identifier: GPL-3.0-or-later
// Showing a term on the photo itself: while the pointer is on "midtones" (a
// term in a help card) or on the "?" of Highlights, the pixels it means light
// up on the preview and the rest dims, with how much of the photo they are.
// Worked out in the page from the preview image, so it costs no render.
"use strict";

// Brightness is the preview's own (display-encoded) luma, 0..1.
const ZONES = {
  blacks: { label: "Blacks", test: (l) => l < 0.06 },
  shadows: { label: "Shadows", test: (l) => l >= 0.06 && l < 0.3 },
  midtones: { label: "Midtones", test: (l) => l >= 0.3 && l < 0.7 },
  highlights: { label: "Highlights", test: (l) => l >= 0.7 && l < 0.94 },
  whites: { label: "Whites", test: (l) => l >= 0.94 },
  clipping: { label: "Clipped", test: (l, r, g, b) => Math.max(r, g, b) >= 254 || Math.max(r, g, b) <= 1 },
  saturated: { label: "Strong colours", test: (_l, r, g, b) => sat(r, g, b) > 0.55 },
  muted: { label: "Muted colours", test: (_l, r, g, b) => { const s = sat(r, g, b); return s > 0.08 && s < 0.35; } },
};

// Which zone a term or a setting's "?" shows.
const ZONE_OF = {
  shadows: "shadows", midtones: "midtones", highlights: "highlights", clipping: "clipping", saturation: "saturated",
  Blacks2012: "blacks", Shadows2012: "shadows", Highlights2012: "highlights", Whites2012: "whites",
  Saturation: "saturated", Vibrance: "muted",
};

function sat(r, g, b) {
  const max = Math.max(r, g, b), min = Math.min(r, g, b);
  return max ? (max - min) / max : 0;
}

const zones = { canvas: null, badge: null, shown: null };

function showZone(key) {
  const zone = ZONES[key];
  const img = $("preview");
  const viewer = $("viewer");
  if (!zone || !img || img.hidden || !img.naturalWidth || !viewer) return;
  const r = img.getBoundingClientRect();
  const v = viewer.getBoundingClientRect();
  if (!r.width || !r.height) return;
  // Small is enough to see where the zone is, and fast.
  const scale = Math.min(1, 640 / Math.max(img.naturalWidth, img.naturalHeight));
  const w = Math.max(1, Math.round(img.naturalWidth * scale));
  const h = Math.max(1, Math.round(img.naturalHeight * scale));
  const c = zones.canvas ??= Object.assign(document.createElement("canvas"), { className: "zone-overlay" });
  c.width = w;
  c.height = h;
  const ctx = c.getContext("2d", { willReadFrequently: true });
  ctx.drawImage(img, 0, 0, w, h);
  let data;
  try {
    data = ctx.getImageData(0, 0, w, h);
  } catch {
    return;
  }
  const px = data.data;
  let hits = 0;
  for (let i = 0; i < px.length; i += 4) {
    const red = px[i], green = px[i + 1], blue = px[i + 2];
    const l = (0.2126 * red + 0.7152 * green + 0.0722 * blue) / 255;
    if (zone.test(l, red, green, blue)) {
      hits++;
      // The zone keeps its pixels, tinted towards the accent so it reads at a glance.
      px[i] = red * 0.55 + 232 * 0.45;
      px[i + 1] = green * 0.55 + 176 * 0.45;
      px[i + 2] = blue * 0.55 + 92 * 0.45;
    } else {
      // The rest goes to a dark grey, so only the zone stands out.
      const grey = 30 + l * 50;
      px[i] = px[i + 1] = px[i + 2] = grey;
    }
  }
  ctx.putImageData(data, 0, 0);
  Object.assign(c.style, { left: `${r.left - v.left}px`, top: `${r.top - v.top}px`, width: `${r.width}px`, height: `${r.height}px` });
  viewer.append(c);
  const badge = zones.badge ??= Object.assign(document.createElement("div"), { className: "zone-badge" });
  const share = Math.round((hits / (w * h)) * 100);
  badge.textContent = `${zone.label}: ${share < 1 && hits ? "under 1" : share}% of this photo`;
  Object.assign(badge.style, { left: `${r.left - v.left + 10}px`, top: `${r.top - v.top + 10}px` });
  viewer.append(badge);
  zones.shown = key;
}

function hideZone() {
  zones.canvas?.remove();
  zones.badge?.remove();
  zones.shown = null;
}

// Hovering a term or a setting's "?" shows its zone, if it has one.
function wireZones() {
  document.addEventListener("mouseover", (e) => {
    const el = e.target.closest?.(".term, .help-mark");
    const key = el && ZONE_OF[el.dataset.term ?? el.dataset.setting];
    if (key && key !== zones.shown) showZone(key);
  });
  document.addEventListener("mouseout", (e) => {
    if (e.target.closest?.(".term, .help-mark")) hideZone();
  });
  window.addEventListener("pointerdown", hideZone, true);
}
