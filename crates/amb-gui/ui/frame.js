// SPDX-License-Identifier: GPL-3.0-or-later
// Frame (Shift+B): a border and text shown around the photo in the editor. It is
// drawn around the preview (padding and a label), not through the edit, so
// edits never change it. Export takes it as its Border (and the text, on
// the photo, as its watermark).
"use strict";

const fr = {
  on: false, color: "#ffffff", width: 4, bottom: 4, text: "", font: "handwritten",
  place: "frame", size: 30, textColor: "#3a3a3a", specs: false, specsText: "", fontsLoaded: false,
};
const SPECS = "{camera} · {exposure}";

// Each photo has its own frame, or none; remembered on this computer. A new
// frame starts from the settings used last.
const FRAME_FIELDS = { color: "frame-color", width: "frame-width", bottom: "frame-bottom", text: "frame-text", font: "frame-font",
  place: "frame-place", size: "frame-size", textColor: "frame-text-color", specs: "frame-specs" };
const framed = (() => {
  try { return JSON.parse(localStorage.getItem("frames")) ?? {}; } catch { return {}; }
})();

function rememberFrame() {
  const path = ui.info?.path;
  if (!path) return;
  if (fr.on) framed[path] = Object.fromEntries(Object.keys(FRAME_FIELDS).map((k) => [k, fr[k]]));
  else delete framed[path];
  try { localStorage.setItem("frames", JSON.stringify(framed)); } catch {}
}

// Another photo is open: show its frame, or none.
function framePhotoChanged() {
  const saved = framed[ui.info?.path];
  if (saved) Object.assign(fr, saved);
  for (const [k, id] of Object.entries(FRAME_FIELDS)) {
    if (typeof fr[k] === "boolean") $(id).checked = fr[k];
    else $(id).value = fr[k];
  }
  $("frame-width-out").value = fr.width;
  $("frame-bottom-out").value = fr.bottom;
  fr.on = Boolean(saved);
  $("btn-frame")?.classList.toggle("active", fr.on);
  if (!fr.on) $("frame-panel").hidden = true;
  drawFrame();
}

// The bundled fonts, for the previews (the export uses the same files).
async function loadFonts() {
  if (fr.fontsLoaded) return ui.fonts;
  fr.fontsLoaded = true;
  try {
    ui.fonts = await invoke("fonts");
  } catch {
    ui.fonts = [["sans", "Sans"]];
  }
  await Promise.all(ui.fonts.map(async ([id]) => {
    try {
      const bytes = await invoke("font_data", { id });
      // The bytes may come as an array or a view; FontFace wants a buffer.
      const face = new FontFace(`amb-${id}`, new Uint8Array(bytes).slice().buffer);
      document.fonts.add(await face.load());
    } catch (e) {
      // The preview falls back to the page's font.
      console.warn(`font ${id}: ${e}`);
      ui.fontProblems = [...(ui.fontProblems ?? []), `${id}: ${e}`];
    }
  }));
  if (ui.fontProblems?.length) status(`Fonts not loaded for the preview: ${ui.fontProblems.join("; ")}`);
  return ui.fonts;
}

function fillFontSelect(sel, value) {
  sel.replaceChildren(...(ui.fonts ?? [["sans", "Sans"]]).map(([id, name]) => {
    const o = Object.assign(document.createElement("option"), { value: id, textContent: name });
    o.style.fontFamily = `amb-${id}`;
    return o;
  }));
  sel.value = value || "sans";
}

// The text with the photo's details, as a template the export fills in.
function frameTemplate() {
  return [fr.text.trim(), fr.specs ? SPECS : ""].filter(Boolean).join(" · ");
}

// The Border the export uses.
function frameBorder() {
  const b = { format: BORDER_FORMAT, name: "Frame", width: fr.width, bottom: fr.bottom, unit: "percent", color: fr.color, radius: 0, fit: false };
  const t = frameTemplate();
  if (t && fr.place === "frame") b.caption = { text: t, size: fr.size, align: "center", color: fr.textColor, font: fr.font };
  return b;
}

// The Frame button turns the frame on and off, like Before/After. Turning it
// on opens its settings; closing them keeps the frame (and export uses it).
async function toggleFrame(on = !fr.on) {
  if (on && !ui.info) return;
  fr.on = on;
  rememberFrame();
  $("btn-frame")?.classList.toggle("active", on);
  await showFrameSettings(on);
}

async function showFrameSettings(open) {
  $("frame-panel").hidden = !open;
  if (open) {
    $("preset-library").hidden = true;
    await loadFonts();
    fillFontSelect($("frame-font"), fr.font);
    if (!ui.borders) await loadBorders();
    const sel = $("frame-preset");
    sel.replaceChildren(Object.assign(document.createElement("option"), { value: "", textContent: "— a border preset —" }),
      ...ui.borders.map((b) => Object.assign(document.createElement("option"), { value: b.id, textContent: b.name })));
    await refreshFrameSpecs();
  }
  drawFrame();
}

async function refreshFrameSpecs() {
  try {
    [fr.specsText] = await invoke("caption_preview", { template: SPECS });
  } catch {
    fr.specsText = "";
  }
}

// Tools that line their canvases up with the photo, and the 100% view.
const frameBlocked = () => ui.crop.on || mk.on || sp.on || ui.zoom.on || (typeof gu !== "undefined" && gu.on);

function drawFrame() {
  const img = $("preview");
  const label = $("frame-label");
  for (const k of ["padding", "background", "width", "height", "boxSizing"]) img.style[k] = "";
  label.hidden = true;
  if (!fr.on || img.hidden || !img.naturalWidth || frameBlocked()) return;
  // Another photo: its own details.
  if (fr.specs && fr.photo !== ui.info?.path) {
    fr.photo = ui.info?.path;
    refreshFrameSpecs().then(drawFrame);
  }
  // The photo's place without the frame, then everything scaled to fit it.
  const r = img.getBoundingClientRect();
  const short = Math.min(r.width, r.height);
  const side = (fr.width / 100) * short;
  const bottom = (fr.bottom / 100) * short;
  const k = Math.min(r.width / (r.width + 2 * side), r.height / (r.height + side + bottom));
  Object.assign(img.style, {
    boxSizing: "border-box",
    width: `${(r.width + 2 * side) * k}px`,
    height: `${(r.height + side + bottom) * k}px`,
    padding: `${side * k}px ${side * k}px ${bottom * k}px`,
    background: fr.color,
  });
  const text = [fr.text.trim(), fr.specs ? fr.specsText : ""].filter(Boolean).join(" · ");
  if (!text) return;
  const v = $("viewer").getBoundingClientRect();
  const f = img.getBoundingClientRect();
  const photo = { x: f.left - v.left + side * k, y: f.top - v.top + side * k, w: r.width * k, h: r.height * k };
  label.textContent = text;
  Object.assign(label.style, { fontFamily: `amb-${fr.font}`, color: fr.textColor, width: "", left: "", top: "", textAlign: "" });
  label.hidden = false;
  if (fr.place === "frame") {
    // Centred in the bottom band, as border.rs draw_caption sets it.
    const band = bottom * k;
    let px = (band * fr.size) / 100 / 0.72;
    label.style.fontSize = `${px}px`;
    const room = (photo.w + 2 * side * k) * 0.92;
    if (label.scrollWidth > room) label.style.fontSize = `${(px * room) / label.scrollWidth}px`;
    Object.assign(label.style, { left: `${f.left - v.left}px`, width: `${f.width}px`, textAlign: "center", top: `${photo.y + photo.h + band / 2 - label.offsetHeight / 2}px` });
  } else {
    // On the photo: its width a share of the photo's, in a corner, as a watermark.
    label.style.fontSize = "100px";
    const px = (100 * (photo.w * fr.size) / 100) / Math.max(1, label.scrollWidth);
    label.style.fontSize = `${Math.max(6, px)}px`;
    const inset = 0.03 * Math.min(photo.w, photo.h);
    const left = fr.place.endsWith("l") ? photo.x + inset : photo.x + photo.w - inset - label.offsetWidth;
    const top = fr.place.includes("-t") ? photo.y + inset : photo.y + photo.h - inset - label.offsetHeight;
    Object.assign(label.style, { left: `${left}px`, top: `${top}px` });
  }
}

// Export: start from the frame (Border "Frame", and on-photo text as the watermark).
function frameToExport() {
  if (!fr.on) return;
  const f = $("export-form");
  const sel = f.border_preset;
  sel.prepend(Object.assign(document.createElement("option"), { value: "frame", textContent: "Frame (as in the editor)" }));
  sel.value = "frame";
  writeBorder(frameBorder());
  const t = frameTemplate();
  if (t && fr.place !== "frame") {
    f.wm_text.value = t;
    f.wm_image.value = "";
    f.wm_font.value = fr.font;
    f.wm_color.value = fr.textColor;
    f.wm_size.value = fr.size;
    f.wm_anchor.value = { "photo-br": "bottom-right", "photo-bl": "bottom-left", "photo-tl": "top-left" }[fr.place];
    f.wm_opacity.value = 100;
    f.wm_shadow.checked = false;
  }
}

function wireFrame() {
  const sync = () => {
    fr.color = $("frame-color").value;
    fr.width = Number($("frame-width").value);
    fr.bottom = Number($("frame-bottom").value);
    fr.text = $("frame-text").value;
    fr.font = $("frame-font").value;
    fr.place = $("frame-place").value;
    fr.size = Number($("frame-size").value);
    fr.textColor = $("frame-text-color").value;
    fr.specs = $("frame-specs").checked;
    $("frame-width-out").value = fr.width;
    $("frame-bottom-out").value = fr.bottom;
    rememberFrame();
    drawFrame();
  };
  for (const id of ["frame-color", "frame-width", "frame-bottom", "frame-text", "frame-font", "frame-place", "frame-size", "frame-text-color", "frame-specs"]) {
    $(id).addEventListener("input", sync);
  }
  for (const b of $("frame-swatches").querySelectorAll("button")) {
    b.addEventListener("click", () => {
      $("frame-color").value = b.dataset.color;
      // Dark text on a light frame, light on a dark one.
      const n = parseInt(b.dataset.color.slice(1), 16);
      $("frame-text-color").value = ((n >> 16) * 0.3 + ((n >> 8) & 255) * 0.59 + (n & 255) * 0.11) > 128 ? "#3a3a3a" : "#e6e6e6";
      sync();
    });
  }
  $("frame-preset").addEventListener("change", () => {
    const b = ui.borders.find((x) => x.id === $("frame-preset").value);
    if (!b || b.unit === "px") return;
    $("frame-width").value = b.width;
    $("frame-bottom").value = b.bottom ?? b.width;
    if (b.color !== "auto") $("frame-color").value = toHex6(b.color);
    if (b.caption) {
      $("frame-specs").checked = /\{(camera|exposure)/.test(b.caption.text);
      $("frame-place").value = "frame";
      if (b.caption.font) $("frame-font").value = b.caption.font;
    }
    sync();
  });
  $("frame-close").addEventListener("click", () => showFrameSettings(false));
  $("frame-remove").addEventListener("click", () => toggleFrame(false));
  $("btn-frame")?.addEventListener("click", () => toggleFrame());
  $("preview").addEventListener("load", drawFrame);
  new ResizeObserver(() => drawFrame()).observe($("viewer"));
}

// Shortcuts (?): every keyboard shortcut on one card.
function showShortcuts() {
  const dlg = $("shortcuts-dialog");
  const groups = new Map();
  for (const a of KEY_ACTIONS) {
    const keys = keysFor(a);
    if (!keys.length) continue;
    groups.set(a.group, [...(groups.get(a.group) ?? []), [a.label, keys]]);
  }
  $("shortcuts-list").replaceChildren(...[...groups].map(([group, rows]) => {
    const box = Object.assign(document.createElement("section"), { className: "shortcuts-group" });
    box.append(Object.assign(document.createElement("h3"), { textContent: group }));
    for (const [label, keys] of rows) {
      const row = Object.assign(document.createElement("div"), { className: "shortcuts-row" });
      const k = document.createElement("span");
      k.append(...keys.map((c) => Object.assign(document.createElement("kbd"), { textContent: keyLabel(c) })));
      row.append(Object.assign(document.createElement("span"), { textContent: label }), k);
      box.append(row);
    }
    return box;
  }));
  if (!dlg.open) dlg.showModal();
}
