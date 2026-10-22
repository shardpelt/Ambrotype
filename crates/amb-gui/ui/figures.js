// SPDX-License-Identifier: GPL-3.0-or-later
// Small drawings for Photography basics (basics.js): what a photo of your
// own can't show, such as aperture or depth of field. Each is an SVG string
// drawn in the theme's colours; a term or chapter names one with "figure".
"use strict";

const FIG_W = 320;

// A text label in the figure.
const figText = (x, y, text, anchor = "middle", size = 10) =>
  `<text x="${x}" y="${y}" text-anchor="${anchor}" font-size="${size}" fill="currentColor">${text}</text>`;

// A seeded random, so speckle looks the same every time.
function figRandom(seed) {
  let s = seed;
  return () => ((s = (s * 16807) % 2147483647) / 2147483647);
}

// A tone strip from black to white with one part marked.
function toneStrip(from, to, label) {
  const x0 = 20 + from * 280, x1 = 20 + to * 280;
  return `<defs><linearGradient id="fg-tone"><stop offset="0" stop-color="#000"/><stop offset="1" stop-color="#fff"/></linearGradient></defs>
    <rect x="20" y="20" width="280" height="34" fill="url(#fg-tone)" stroke="currentColor" stroke-opacity="0.3"/>
    <rect x="${x0}" y="16" width="${x1 - x0}" height="42" fill="none" stroke="var(--accent)" stroke-width="2.5" rx="2"/>
    ${figText((x0 + x1) / 2, 74, label)}
    ${figText(20, 90, "black", "start", 9)}${figText(300, 90, "white", "end", 9)}`;
}

const FIGURES = {
  aperture: () => {
    const lens = (cx, r, label, light) => `
      <circle cx="${cx}" cy="48" r="34" fill="none" stroke="currentColor" stroke-opacity="0.5"/>
      <circle cx="${cx}" cy="48" r="34" fill="currentColor" fill-opacity="0.12"/>
      <circle cx="${cx}" cy="48" r="${r}" fill="#fff" stroke="currentColor"/>
      ${figText(cx, 98, label, "middle", 11)}${figText(cx, 112, light, "middle", 9)}`;
    return lens(60, 28, "f/1.8", "a lot of light") + lens(160, 15, "f/4", "less") + lens(260, 6, "f/11", "little light");
  },
  "depth-of-field": () => {
    // Near tree, subject, far tree; side by side, blurred or not.
    const scene = (x, blur, label, sub) => `
      <g transform="translate(${x} 0)">
        <g filter="${blur ? "url(#fg-blur)" : ""}"><path d="M8 78 l14 -40 l14 40 z" fill="currentColor" fill-opacity="0.5"/></g>
        <circle cx="76" cy="50" r="9" fill="var(--accent)"/><rect x="69" y="59" width="14" height="19" rx="3" fill="var(--accent)"/>
        <g filter="${blur ? "url(#fg-blur)" : ""}"><path d="M118 78 l10 -26 l10 26 z" fill="currentColor" fill-opacity="0.5"/></g>
        <line x1="0" y1="78" x2="146" y2="78" stroke="currentColor" stroke-opacity="0.3"/>
        ${figText(73, 96, label, "middle", 10)}${figText(73, 110, sub, "middle", 9)}
      </g>`;
    return `<defs><filter id="fg-blur"><feGaussianBlur stdDeviation="3"/></filter></defs>` +
      scene(8, true, "f/1.8: shallow", "only the subject sharp") + scene(166, false, "f/11: deep", "front to back sharp");
  },
  shutter: () => `
    <defs><linearGradient id="fg-streak"><stop offset="0" stop-color="var(--accent)" stop-opacity="0"/><stop offset="1" stop-color="var(--accent)"/></linearGradient></defs>
    <circle cx="80" cy="44" r="14" fill="var(--accent)"/>
    ${figText(80, 86, "1/1000 s: frozen")}
    <rect x="170" y="30" width="110" height="28" rx="14" fill="url(#fg-streak)"/>
    ${figText(225, 86, "1/15 s: movement blurs")}`,
  iso: () => {
    const rnd = figRandom(7);
    let dots = "";
    for (let i = 0; i < 420; i++) {
      const v = Math.round(rnd() * 255);
      dots += `<rect x="${(180 + rnd() * 116).toFixed(1)}" y="${(14 + rnd() * 56).toFixed(1)}" width="2" height="2" fill="rgb(${v},${v},${v})" opacity="0.7"/>`;
    }
    return `<defs><linearGradient id="fg-sky" x2="0" y2="1"><stop offset="0" stop-color="#6d8fb3"/><stop offset="1" stop-color="#2c3e55"/></linearGradient></defs>
      <rect x="24" y="14" width="116" height="56" fill="url(#fg-sky)"/>
      <rect x="180" y="14" width="116" height="56" fill="url(#fg-sky)"/>${dots}
      ${figText(82, 88, "ISO 100: smooth")}${figText(238, 88, "ISO 12800: noisy")}`;
  },
  stop: () => {
    let out = "";
    [1, 2, 4, 8].forEach((n, i) => {
      const x = 20 + i * 76;
      for (let k = 0; k < n; k++) out += `<rect x="${x + (k % 4) * 15}" y="${52 - Math.floor(k / 4) * 15}" width="12" height="12" fill="var(--accent)"/>`;
      out += figText(x + 28, 86, i ? `+${i} stop${i > 1 ? "s" : ""}` : "start");
    });
    return out + figText(160, 104, "each stop doubles the light", "middle", 9);
  },
  histogram: () => {
    const ys = [];
    for (let i = 0; i <= 56; i++) {
      const x = i / 56;
      ys.push(Math.exp(-((x - 0.3) ** 2) / 0.02) * 0.9 + Math.exp(-((x - 0.72) ** 2) / 0.01) * 0.5 + 0.04);
    }
    const pts = ys.map((y, i) => `${20 + i * 5},${70 - y * 50}`).join(" ");
    const zones = [["blacks", 0, 0.06], ["shadows", 0.06, 0.3], ["midtones", 0.3, 0.7], ["highlights", 0.7, 0.94], ["whites", 0.94, 1]];
    return `<polygon points="20,70 ${pts} 300,70" fill="currentColor" fill-opacity="0.35"/>
      <rect x="296" y="18" width="4" height="52" fill="var(--accent)"/>
      ${figText(296, 12, "clipped", "end", 9)}
      <line x1="20" y1="70" x2="300" y2="70" stroke="currentColor"/>` +
      zones.map(([n, a, b]) => `<line x1="${20 + b * 280}" y1="70" x2="${20 + b * 280}" y2="76" stroke="currentColor"/>${figText(20 + ((a + b) / 2) * 280, 90, n, "middle", n.length > 7 && b - a < 0.1 ? 7 : 9)}`).join("") +
      figText(160, 106, "how many pixels at each brightness, dark → bright", "middle", 9);
  },
  clipping: () => `
    <defs><linearGradient id="fg-clip"><stop offset="0" stop-color="#555"/><stop offset="0.7" stop-color="#fff"/><stop offset="1" stop-color="#fff"/></linearGradient></defs>
    <rect x="20" y="20" width="280" height="40" fill="url(#fg-clip)" stroke="currentColor" stroke-opacity="0.3"/>
    <rect x="216" y="16" width="84" height="48" fill="none" stroke="var(--accent)" stroke-width="2.5" stroke-dasharray="4 3"/>
    ${figText(300, 80, "all pure white: no detail left", "end")}`,
  highlights: () => toneStrip(0.7, 0.94, "highlights"),
  midtones: () => toneStrip(0.3, 0.7, "midtones"),
  shadows: () => toneStrip(0.06, 0.3, "shadows"),
  contrast: () => `
    <defs><linearGradient id="fg-lo"><stop offset="0" stop-color="#555"/><stop offset="1" stop-color="#aaa"/></linearGradient>
    <linearGradient id="fg-hi"><stop offset="0" stop-color="#000"/><stop offset="0.35" stop-color="#222"/><stop offset="0.65" stop-color="#ddd"/><stop offset="1" stop-color="#fff"/></linearGradient></defs>
    <rect x="20" y="14" width="280" height="26" fill="url(#fg-lo)"/>${figText(160, 54, "low contrast: soft, grey, calm")}
    <rect x="20" y="64" width="280" height="26" fill="url(#fg-hi)"/>${figText(160, 104, "high contrast: deep blacks, bright whites")}`,
  "dynamic-range": () => `
    <rect x="20" y="16" width="280" height="18" fill="currentColor" fill-opacity="0.25"/>${figText(160, 29, "the scene: deep shade to bright sky", "middle", 9)}
    <rect x="70" y="46" width="180" height="18" fill="var(--accent)" fill-opacity="0.8"/>${figText(160, 59, "what one shot holds", "middle", 9)}
    ${figText(45, 84, "→ black", "middle", 9)}${figText(275, 84, "→ white", "middle", 9)}
    ${figText(160, 100, "outside the range, detail clips", "middle", 9)}`,
  "white-balance": () => FIGURES.kelvin(),
  kelvin: () => `
    <defs><linearGradient id="fg-k"><stop offset="0" stop-color="#ff8a2a"/><stop offset="0.35" stop-color="#ffd9a8"/><stop offset="0.55" stop-color="#fff"/><stop offset="1" stop-color="#9cc0ff"/></linearGradient></defs>
    <rect x="20" y="20" width="280" height="24" fill="url(#fg-k)" stroke="currentColor" stroke-opacity="0.3"/>
    ${[["candle", 0.02, "1900 K"], ["bulb", 0.2, "3000 K"], ["daylight", 0.52, "5500 K"], ["shade", 0.9, "7500 K"]]
      .map(([n, t, k]) => `<line x1="${20 + t * 280}" y1="44" x2="${20 + t * 280}" y2="52" stroke="currentColor"/>${figText(20 + t * 280, 64, n, t < 0.1 ? "start" : t > 0.85 ? "end" : "middle", 9)}${figText(20 + t * 280, 76, k, t < 0.1 ? "start" : t > 0.85 ? "end" : "middle", 8)}`).join("")}
    ${figText(160, 98, "the colour of the light; white balance takes it out", "middle", 9)}`,
  saturation: () => {
    let out = "";
    for (let i = 0; i < 6; i++) out += `<rect x="${20 + i * 48}" y="18" width="40" height="40" rx="4" fill="hsl(10 ${i * 18}% 50%)"/>`;
    return out + figText(40, 76, "grey", "middle", 9) + figText(280, 76, "vivid", "middle", 9) + figText(160, 96, "the same colour, more and more saturated", "middle", 9);
  },
  hue: () => {
    const names = ["red", "orange", "yellow", "green", "aqua", "blue", "purple", "magenta"];
    const hues = [0, 30, 55, 120, 180, 220, 275, 315];
    return names.map((n, i) => `<rect x="${20 + i * 36}" y="18" width="30" height="34" rx="4" fill="hsl(${hues[i]} 75% 50%)"/>${figText(35 + i * 36, 68, n, "middle", 8)}`).join("") +
      figText(160, 92, "hue is which colour; the HSL panel moves each one", "middle", 9);
  },
  luminance: () => `${[30, 45, 60, 75].map((l, i) => `<rect x="${40 + i * 64}" y="18" width="48" height="40" rx="4" fill="hsl(215 70% ${l}%)"/>`).join("")}
    ${figText(160, 80, "one blue, darker to lighter: its luminance", "middle", 9)}`,
  noise: () => FIGURES.iso(),
  halo: () => `
    <rect x="20" y="10" width="280" height="80" fill="#9cc0e8"/>
    <path d="M160 12 L182 44 L172 44 L192 70 L178 70 L196 90 L124 90 L142 70 L128 70 L148 44 L138 44 Z" fill="none" stroke="#fff" stroke-width="6" stroke-opacity="0.85" stroke-linejoin="round"/>
    <path d="M160 12 L182 44 L172 44 L192 70 L178 70 L196 90 L124 90 L142 70 L128 70 L148 44 L138 44 Z" fill="#22301f"/>
    ${figText(160, 106, "a bright rim along the edge: too much Clarity or sharpening", "middle", 9)}`,
  vignette: () => `
    <defs><radialGradient id="fg-vig"><stop offset="0.5" stop-color="#000" stop-opacity="0"/><stop offset="1" stop-color="#000" stop-opacity="0.75"/></radialGradient></defs>
    <rect x="60" y="8" width="200" height="84" fill="#c9b79a"/><circle cx="160" cy="50" r="14" fill="#6b4b2a"/>
    <rect x="60" y="8" width="200" height="84" fill="url(#fg-vig)"/>
    ${figText(160, 106, "darker corners lead the eye to the middle", "middle", 9)}`,
  crop: () => {
    const r = (x, w, h, label) => `<rect x="${x}" y="${70 - h}" width="${w}" height="${h}" fill="currentColor" fill-opacity="0.15" stroke="currentColor"/>${figText(x + w / 2, 86, label, "middle", 9)}`;
    return r(14, 72, 48, "3:2 camera") + r(100, 46, 58, "4:5 Instagram") + r(160, 56, 56, "1:1 square") + r(230, 80, 45, "16:9 screen");
  },
  "exposure-triangle": () => {
    const box = (x, title, a, b) => `<rect x="${x}" y="10" width="92" height="74" rx="6" fill="currentColor" fill-opacity="0.08" stroke="currentColor" stroke-opacity="0.4"/>
      ${figText(x + 46, 30, title, "middle", 11)}${figText(x + 46, 52, a, "middle", 9)}${figText(x + 46, 68, b, "middle", 9)}`;
    return box(12, "Aperture", "more light", "less in focus") + box(114, "Shutter", "more light", "more blur") + box(216, "ISO", "brighter", "more noise") +
      figText(160, 102, "each one: one stop more light, and its price", "middle", 9);
  },
};

// The figure's SVG element, or null.
function figure(id) {
  const draw = FIGURES[id];
  if (!draw) return null;
  const wrap = document.createElement("figure");
  wrap.className = "basics-figure";
  wrap.innerHTML = `<svg viewBox="0 0 ${FIG_W} 116" role="img" aria-hidden="true">${draw()}</svg>`;
  return wrap;
}
