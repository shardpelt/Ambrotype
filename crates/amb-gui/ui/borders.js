// SPDX-License-Identifier: GPL-3.0-or-later
// Borders in the export dialog: pick a preset (built-in or yours), adjust it,
// save it, and see a small preview of the framed photo. The border itself is
// drawn by the export (amb_export::border); borderPlan() below mirrors its
// layout maths so the preview matches the file.
"use strict";

const BORDER_FORMAT = "ambrotype-border/1";

async function loadBorders() {
  try {
    ui.borders = await invoke("border_presets");
  } catch (e) {
    ui.borders = [];
    toast(`Couldn't load border presets: ${e}`, true);
  }
}

function fillBorderSelect(selected = ui.borderChoice ?? "") {
  const sel = $("export-form").border_preset;
  sel.replaceChildren(Object.assign(document.createElement("option"), { value: "", textContent: "None" }));
  for (const [label, builtin] of [["Built-in", true], ["Yours", false]]) {
    const items = ui.borders.filter((b) => b.builtin === builtin);
    if (!items.length) continue;
    const group = Object.assign(document.createElement("optgroup"), { label });
    for (const b of items) group.append(Object.assign(document.createElement("option"), { value: b.id, textContent: b.name }));
    sel.append(group);
  }
  sel.value = ui.borders.some((b) => b.id === selected) ? selected : "";
}

// The border the fields describe.
function readBorder() {
  const f = $("export-form");
  const num = (input, fallback = 0) => (input.value === "" ? fallback : Number(input.value));
  const b = {
    format: BORDER_FORMAT,
    name: f.b_name.value.trim() || "Border",
    width: num(f.b_width),
    unit: f.b_unit.value,
    color: f.b_auto.checked ? "auto" : f.b_color.value,
    radius: num(f.b_radius),
    fit: f.b_fit.checked,
  };
  if (f.b_bottom.value !== "") b.bottom = num(f.b_bottom);
  if (f.b_key.value !== "" && Number(f.b_key.value) > 0) b.keyline = { width: Number(f.b_key.value), color: f.b_key_color.value };
  const caption = f.b_caption.value.trim();
  if (caption) {
    b.caption = { ...(ui.borderCaptionRest ?? {}), text: caption, align: f.b_caption_align.value };
  }
  return b;
}

function writeBorder(b) {
  const f = $("export-form");
  f.b_name.value = b.name ?? "";
  f.b_width.value = b.width ?? 0;
  f.b_unit.value = b.unit ?? "percent";
  f.b_bottom.value = b.bottom ?? "";
  f.b_auto.checked = b.color === "auto";
  f.b_color.value = b.color && b.color !== "auto" ? toHex6(b.color) : "#ffffff";
  f.b_color.disabled = false;
  markSwatch();
  f.b_key.value = b.keyline?.width ?? "";
  f.b_key_color.value = toHex6(b.keyline?.color ?? "#000000");
  f.b_radius.value = b.radius ?? 0;
  f.b_fit.checked = Boolean(b.fit);
  // The caption's size and colour aren't in the fields: kept as they were.
  const { text = "", align = "center", ...rest } = b.caption ?? {};
  ui.borderCaptionRest = rest;
  f.b_caption.value = text;
  f.b_caption_align.value = align;
}

// The caption filled in for the open photo (by the export's own code),
// fetched when the template changes; the preview draws it.
async function refreshCaption() {
  const template = $("export-form").b_caption.value.trim();
  if (!template) {
    ui.captionPreview = null;
    $("border-caption-tokens").hidden = true;
    return;
  }
  try {
    const [text, tokens] = await invoke("caption_preview", { template });
    ui.captionPreview = { template, text };
    const hint = $("border-caption-tokens");
    hint.textContent = `Fills in: ${tokens.map(([t]) => t).join(" ")}`;
    hint.title = tokens.map(([t, what]) => `${t}: ${what}`).join("\n");
    hint.hidden = false;
  } catch {
    ui.captionPreview = null;
  }
  drawBorderPreview();
}

const borderOn = () => Boolean($("export-form").border_preset.value);

// Where the photo and frame go (crates/amb-export/src/border.rs `plan`).
function borderPlan(b, exact, longEdge, w, h) {
  const px = (v, short) => (b.unit === "px" ? Math.round(v) : Math.round((v / 100) * short));
  const keyline = (short) => (b.keyline ? Math.max(1, px(b.keyline.width, short)) : 0);
  const clamp = (v, lo, hi) => Math.min(hi, Math.max(lo, v));
  if (exact) {
    const [cw, ch] = exact;
    const short = Math.min(cw, ch);
    const side = px(b.width, short);
    const bottom = px(b.bottom ?? b.width, short);
    const iw = Math.max(1, cw - 2 * side);
    const ih = Math.max(1, ch - side - bottom);
    let photo, crop = null;
    if (b.fit) {
      const k = Math.min(iw / w, ih / h);
      photo = [clamp(Math.round(w * k), 1, iw), clamp(Math.round(h * k), 1, ih)];
    } else {
      photo = [iw, ih];
      const t = iw / ih;
      const [cwid, chei] = w / h > t ? [h * t, h] : [w, w / t];
      crop = [(w - cwid) / 2, (h - chei) / 2, cwid, chei];
    }
    return {
      canvas: [cw, ch], photo, crop, keyline: keyline(short),
      at: [side + Math.floor((iw - photo[0]) / 2), side + Math.floor((ih - photo[1]) / 2)],
      radius: Math.round((b.radius / 100) * Math.min(...photo)),
    };
  }
  let [pw, ph] = [w, h];
  if (longEdge && Math.max(w, h) > longEdge) {
    const k = longEdge / Math.max(w, h);
    [pw, ph] = [Math.round(w * k), Math.round(h * k)];
  }
  const short = Math.min(pw, ph);
  const side = px(b.width, short);
  const bottom = px(b.bottom ?? b.width, short);
  return {
    canvas: [pw + 2 * side, ph + side + bottom], photo: [pw, ph], at: [side, side], crop: null,
    keyline: keyline(short), radius: Math.round((b.radius / 100) * short),
  };
}

function exportSizeChoice() {
  const f = $("export-form");
  const w = Number(f.exact_w.value), h = Number(f.exact_h.value);
  return {
    exact: w > 0 && h > 0 ? [w, h] : null,
    longEdge: resizeLongEdge(),
  };
}

// The frame and photo, scaled into the preview canvas.
function drawBorderPreview() {
  const canvas = $("border-preview");
  const img = $("preview");
  canvas.hidden = !borderOn();
  $("border-edit").hidden = !borderOn();
  if (!borderOn() || !img.naturalWidth || !ui.exportCropSize) return;
  const b = readBorder();
  const [w, h] = ui.exportCropSize;
  const { exact, longEdge } = exportSizeChoice();
  const p = borderPlan(b, exact, longEdge, w, h);
  const maxW = 260, maxH = 200;
  const s = Math.min(maxW / p.canvas[0], maxH / p.canvas[1]);
  const dpr = window.devicePixelRatio || 1;
  const cw = Math.max(1, Math.round(p.canvas[0] * s)), ch = Math.max(1, Math.round(p.canvas[1] * s));
  canvas.style.width = `${cw}px`;
  canvas.style.height = `${ch}px`;
  canvas.width = cw * dpr;
  canvas.height = ch * dpr;
  const g = canvas.getContext("2d");
  g.setTransform(dpr * s, 0, 0, dpr * s, 0, 0);
  let frame = b.color;
  if (frame === "auto") {
    // The photo's average colour, from the preview.
    const one = document.createElement("canvas");
    one.width = one.height = 1;
    const o = one.getContext("2d");
    o.drawImage(img, 0, 0, 1, 1);
    const [r, gg, bl] = o.getImageData(0, 0, 1, 1).data;
    frame = `rgb(${r},${gg},${bl})`;
  }
  g.fillStyle = frame;
  g.fillRect(0, 0, p.canvas[0], p.canvas[1]);
  const [x, y] = p.at, [pw, ph] = p.photo;
  if (p.keyline && b.keyline) {
    g.fillStyle = b.keyline.color;
    g.fillRect(x - p.keyline, y - p.keyline, pw + 2 * p.keyline, ph + 2 * p.keyline);
  }
  // The preview image is the whole (cropped) photo; map the export's crop onto it.
  const kx = img.naturalWidth / w, ky = img.naturalHeight / h;
  const [sx, sy, sw, sh] = p.crop ?? [0, 0, w, h];
  g.save();
  g.beginPath();
  if (g.roundRect) g.roundRect(x, y, pw, ph, p.radius);
  else g.rect(x, y, pw, ph);
  g.clip();
  g.drawImage(img, sx * kx, sy * ky, sw * kx, sh * ky, x, y, pw, ph);
  g.restore();
  // The caption, as border.rs `draw_caption` sets it (near enough).
  const text = b.caption && ui.captionPreview?.template === b.caption.text ? ui.captionPreview.text : "";
  const top = y + ph + p.keyline, band = p.canvas[1] - top;
  if (text && band >= 6) {
    let size = band * ((b.caption.size ?? 30) / 100) / 0.72;
    g.font = `bold ${size}px sans-serif`;
    const room = p.canvas[0] * 0.92, wide = g.measureText(text).width;
    if (wide > room) {
      size *= room / wide;
      g.font = `bold ${size}px sans-serif`;
    }
    const light = (() => {
      const m = /^#?([0-9a-f]{6})$/i.exec(toHex6(frame.startsWith("rgb") ? "#808080" : frame));
      if (!m) return true;
      const n = parseInt(m[1], 16);
      return (0.2126 * (n >> 16) + 0.7152 * ((n >> 8) & 255) + 0.0722 * (n & 255)) / 255 > 0.5;
    })();
    g.fillStyle = b.caption.color ?? (light ? "#3a3a3a" : "#e6e6e6");
    g.textBaseline = "middle";
    const align = b.caption.align ?? "center";
    g.textAlign = align;
    const tx = align === "left" ? x : align === "right" ? x + pw : p.canvas[0] / 2;
    g.fillText(text, tx, top + band / 2);
  }
  $("border-size").textContent = `${p.canvas[0]} × ${p.canvas[1]} px`;
}

// Show which swatch matches the colour.
function markSwatch() {
  const f = $("export-form");
  const current = f.b_auto.checked ? "auto" : f.b_color.value.toLowerCase();
  for (const b of $("border-swatches").querySelectorAll("button")) b.classList.toggle("on", b.dataset.color === current);
}

function wireBorders() {
  const f = $("export-form");
  for (const b of $("border-swatches").querySelectorAll("button")) {
    b.addEventListener("click", () => {
      const auto = b.dataset.color === "auto";
      f.b_auto.checked = auto;
      if (!auto) f.b_color.value = b.dataset.color;
      f.b_color.disabled = false;
      markSwatch();
      updateExportSize();
      drawBorderPreview();
    });
  }
  f.border_preset.addEventListener("change", () => {
    ui.borderChoice = f.border_preset.value;
    const p = ui.borders.find((b) => b.id === f.border_preset.value);
    if (p) writeBorder(p);
    refreshCaption();
    $("border-delete").disabled = !p || p.builtin;
    updateExportSize();
    drawBorderPreview();
  });
  let captionTimer = null;
  f.b_caption.addEventListener("input", () => {
    clearTimeout(captionTimer);
    captionTimer = setTimeout(refreshCaption, 250);
  });
  for (const name of ["b_width", "b_unit", "b_bottom", "b_color", "b_auto", "b_key", "b_key_color", "b_radius", "b_fit", "b_caption_align"]) {
    f[name].addEventListener("input", () => {
      // Picking a colour by hand turns "auto" off.
      if (name === "b_color") f.b_auto.checked = false;
      f.b_color.disabled = false;
      markSwatch();
      updateExportSize();
      drawBorderPreview();
    });
  }
  $("border-save").addEventListener("click", async () => {
    const current = ui.borders.find((b) => b.id === f.border_preset.value);
    const border = readBorder();
    // Same name as one of yours: update it; otherwise a new preset (built-ins stay as they are).
    const id = current && !current.builtin && current.name === border.name ? current.id : "";
    try {
      const [list, saved] = await invoke("save_border_preset", { preset: { id, ...border } });
      ui.borders = list;
      ui.borderChoice = saved;
      fillBorderSelect(saved);
      $("border-delete").disabled = false;
      toast(`Saved border "${border.name}"`);
    } catch (e) {
      toast(String(e), true);
    }
  });
  $("border-delete").addEventListener("click", async () => {
    const id = f.border_preset.value;
    try {
      ui.borders = await invoke("delete_border_preset", { id });
      ui.borderChoice = "";
      fillBorderSelect("");
      drawBorderPreview();
      updateExportSize();
    } catch (e) {
      toast(String(e), true);
    }
  });
}
