// SPDX-License-Identifier: GPL-3.0-or-later
// The Export dialog: formats, sizes, watermarks, export presets, and Edit
// in another app.
"use strict";

// A photo on the clipboard, with its edit (3072 pixels on the long side),
// to paste into a chat, mail or document.
async function copyImage(path) {
  status("Copying the photo…");
  try {
    const [w, h] = await invoke("copy_image", { path });
    toast(`Copied the photo (${w} × ${h}): paste it anywhere`);
    status("Copied the photo");
  } catch (e) {
    toast(String(e), true);
  }
}

async function copyPath(paths) {
  try {
    await invoke("copy_text", { text: paths.join("\n") });
    toast(paths.length === 1 ? "Copied the file's path" : `Copied ${paths.length} paths`);
  } catch (e) {
    toast(String(e), true);
  }
}

// Right-click on the photo: copy it, its path, find it, export it.
function photoMenu(e) {
  if (!ui.info) return;
  const path = ui.info.path;
  contextMenu(e, [
    ["Copy image", () => copyImage(path)],
    ["Copy file path", () => copyPath([path])],
    ["Show in folder", () => invoke("reveal_file", { path }).catch((err) => toast(String(err), true))],
    ["Export…", () => exportFlow()],
  ]);
}

// The folder the last export went to, on this computer: the save dialogs
// start there (else next to the photo).
function lastExportDir() {
  try { return localStorage.getItem("amb.exportDir") || null; } catch { return null; }
}

function rememberExportDir(path) {
  if (!path) return;
  try { localStorage.setItem("amb.exportDir", dirOf(path)); } catch { /* not kept */ }
}

const DEPTHS = { jpeg: [8], png: [8, 16], tiff: [16, 8, 32], webp: [8], avif: [8], exr: [32], jxl: [8, 16] };

function updateExportForm() {
  const f = $("export-form");
  const format = f.format.value;
  f.bits.innerHTML = DEPTHS[format].map((b) => `<option value="${b}">${b === 32 ? "32-bit float" : b + "-bit"}</option>`).join("");
  f.quality.closest("label").style.visibility = ["jpeg", "avif"].includes(format) ? "visible" : "hidden";
  f.max_kb.closest("label").style.visibility = format === "jpeg" ? "visible" : "hidden";
  for (const el of f.querySelectorAll(".jpeg-only")) el.hidden = format !== "jpeg";
  for (const el of f.querySelectorAll(".avif-only")) el.hidden = format !== "avif";
  // JPEG XL can't carry an ICC profile yet: sRGB only. AVIF names its
  // colour space by code, which exists for sRGB, Display P3 and Rec. 2020.
  f.color_space.disabled = format === "jxl";
  const allowed = format === "jxl" ? ["srgb"] : format === "avif" ? ["srgb", "p3", "rec2020"] : null;
  for (const o of f.color_space.options) o.disabled = Boolean(allowed) && !allowed.includes(o.value);
  if (f.color_space.selectedOptions[0]?.disabled) f.color_space.value = "srgb";
}

// Exact size: say what will be trimmed when its aspect differs from the crop.
function updateExportSize() {
  // The border preview follows the size.
  queueMicrotask(drawBorderPreview);
  const f = $("export-form");
  const w = Number(f.exact_w.value);
  const h = Number(f.exact_h.value);
  const exact = w > 0 && h > 0;
  f.resize_mode.disabled = f.resize_value.disabled = exact;
  const note = $("export-size-note");
  note.classList.remove("warn");
  if (!exact) {
    note.textContent = f.exact_w.value || f.exact_h.value ? "Give both a width and a height for an exact size." : "";
    return;
  }
  const a = ui.exportCropAspect;
  const t = w / h;
  if (borderOn() && f.b_fit.checked) {
    note.textContent = `Exactly ${w} × ${h} px, with the whole ${ui.crop.enabled ? "crop" : "photo"} inside the frame.`;
  } else if (Math.abs(Math.log(a / t)) > 0.005) {
    note.classList.add("warn");
    note.textContent = `Exactly ${w} × ${h} px. The ${ui.crop.enabled ? "crop" : "photo"} is ${ratioText(a)} ` +
      `and this size is ${ratioText(t)}, so the ${a > t ? "left and right" : "top and bottom"} edges are trimmed (centred). ` +
      "To choose what is kept, crop with this size preset first.";
  } else {
    note.textContent = `Exactly ${w} × ${h} px; the crop already has this shape.`;
  }
}

function pickExportSize() {
  const f = $("export-form");
  const v = f.size_preset.value;
  if (v.startsWith("size:")) {
    const p = ui.sizes.find((x) => x.id === v.slice(5));
    if (p) {
      f.exact_w.value = p.width;
      f.exact_h.value = p.height;
    }
  } else if (v === "") {
    f.exact_w.value = f.exact_h.value = "";
  }
  updateExportSize();
}

// The Resize choice ({ mode, value }), or null for full size (or an exact size).
function exportResize() {
  const f = $("export-form");
  const value = Number(f.resize_value.value);
  if (f.resize_mode.disabled || !f.resize_mode.value || !(value > 0)) return null;
  return { mode: f.resize_mode.value, value };
}

// The long edge the Resize choice gives the export (for the border preview).
function resizeLongEdge() {
  const r = exportResize();
  if (!r || !ui.exportCropSize) return null;
  const [w, h] = ui.exportCropSize;
  const scale = {
    long: r.value / Math.max(w, h),
    short: r.value / Math.min(w, h),
    width: r.value / w,
    height: r.value / h,
    mp: Math.sqrt((r.value * 1e6) / (w * h)),
    percent: r.value / 100,
  }[r.mode];
  return scale > 0 ? Math.round(Math.max(w, h) * Math.min(1, scale)) : null;
}

// Export presets: the dialog's choices under a name (kept in the app's
// settings, src/commands/export.rs `export_presets`).
const EXPORT_FIELDS = ["format", "quality", "bits", "color_space", "exact_w", "exact_h", "resize_mode", "resize_value",
  "sharpen", "metadata", "max_kb", "border_preset", "name_template",
  "wm_text", "wm_image", "wm_anchor", "wm_size", "wm_opacity", "wm_color", "wm_shadow", "wm_rotate", "wm_tile",
  "subsampling", "progressive", "avif_speed", "after"];

// A field's value as an export preset keeps it (checkboxes as true/false).
const fieldValue = (el) => (el.type === "checkbox" ? el.checked : el.value);

// The watermark as the export expects it, or null.
function readWatermark() {
  const f = $("export-form");
  const image = f.wm_image.value || null;
  const text = f.wm_text.value.trim();
  if (!image && !text) return null;
  return {
    text: image ? "" : text,
    image,
    color: f.wm_color.value,
    size: Number(f.wm_size.value) || 20,
    opacity: Number(f.wm_opacity.value),
    anchor: f.wm_anchor.value,
    inset: 3,
    shadow: f.wm_shadow.checked,
    rotate: Math.max(-180, Math.min(180, Number(f.wm_rotate.value) || 0)),
    tile: f.wm_tile.checked,
    font: f.wm_font.value || "",
  };
}

// Show the watermark's options once there is one; a logo replaces the text.
function updateWatermarkForm() {
  const f = $("export-form");
  const image = f.wm_image.value;
  f.wm_text.disabled = Boolean(image);
  f.wm_text.placeholder = image ? "Using a logo" : "None (e.g. © Your Name)";
  $("wm-logo").textContent = image ? "No logo" : "Logo…";
  $("wm-image-name").textContent = image ? fileName(image) : "";
  for (const c of document.querySelectorAll(".wm-color, .wm-shadow")) c.hidden = Boolean(image);
  document.querySelector(".watermark-section .wm-fields").hidden = !image && !f.wm_text.value.trim();
  // A repeated mark covers the photo: no one place for it.
  f.wm_anchor.disabled = f.wm_tile.checked;
}

function wireWatermark() {
  const f = $("export-form");
  f.wm_text.addEventListener("input", updateWatermarkForm);
  f.wm_tile.addEventListener("change", updateWatermarkForm);
  $("wm-logo").addEventListener("click", async () => {
    if (f.wm_image.value) f.wm_image.value = "";
    else f.wm_image.value = (await invoke("pick_watermark_image")) ?? "";
    updateWatermarkForm();
  });
}

async function fillExportPresets(selected = null) {
  const f = $("export-form");
  try {
    ui.exportPresets = await invoke("export_presets");
  } catch {
    ui.exportPresets = [];
  }
  const option = (value, text) => Object.assign(document.createElement("option"), { value, textContent: text });
  f.export_preset.replaceChildren(option("", ui.exportPresets.length ? "— choose —" : "None saved yet"),
    ...ui.exportPresets.map((p) => option(p.name, p.name)));
  f.export_preset.value = selected ?? "";
  $("export-preset-delete").disabled = !f.export_preset.value;
}

function applyExportPreset(name) {
  const f = $("export-form");
  const p = ui.exportPresets?.find((x) => x.name === name);
  $("export-preset-delete").disabled = !p;
  if (!p) return;
  const set = (n) => {
    if (!(n in p.fields) || !f[n]) return;
    if (f[n].type === "checkbox") f[n].checked = p.fields[n] === true;
    else f[n].value = p.fields[n];
  };
  set("format");
  updateExportForm();
  for (const n of EXPORT_FIELDS.filter((n) => n !== "format")) set(n);
  updateWatermarkForm();
  f.quality.dispatchEvent(new Event("input"));
  f.border_preset.dispatchEvent(new Event("change"));
  f.size_preset.value = "";
  updateExportSize();
}

function wireExportPresets() {
  const f = $("export-form");
  f.export_preset.addEventListener("change", () => applyExportPreset(f.export_preset.value));
  $("export-preset-save").addEventListener("click", async () => {
    const name = await askName("Save export preset", "Keeps the format, size, sharpening, metadata and border choices.", f.export_preset.value);
    if (!name) return;
    const fields = Object.fromEntries(EXPORT_FIELDS.filter((n) => f[n]).map((n) => [n, fieldValue(f[n])]));
    try {
      await invoke("save_export_preset", { name, fields });
      await fillExportPresets(name);
      toast(`Saved the export preset “${name}”`);
    } catch (e) {
      toast(String(e), true);
    }
  });
  $("export-preset-delete").addEventListener("click", async () => {
    const name = f.export_preset.value;
    if (!name) return;
    try {
      await invoke("delete_export_preset", { name });
      await fillExportPresets();
    } catch (e) {
      toast(String(e), true);
    }
  });
}

// `batch`: many photos from the library ({ paths, aspect, run(options) }),
// each exported with its own edit; otherwise the open photo.
// Edit in another app (Settings → General picks it): the photo, as edited,
// is written as a TIFF next to it and opened there; it then shows in the
// folder like any photo.
async function editInFlow() {
  if (!ui.info) return;
  $("busy").hidden = false;
  status("Writing the TIFF for the other app…");
  try {
    const path = await invoke("edit_in");
    status(`Opened ${fileName(path)} in the other app`);
    toast(`Wrote ${fileName(path)} next to the photo and opened it. Save it there; it shows in the folder.`);
  } catch (e) {
    toast(String(e), true);
  } finally {
    $("busy").hidden = true;
  }
}

// Show the exported files in their folder, or open them (the dialog's
// After export choice).
async function afterExport(paths, action) {
  if (!action || !paths.length) return;
  try {
    await invoke("after_export", { paths, action });
  } catch (e) {
    toast(String(e), true);
  }
}

async function exportFlow(batch = null) {
  if (!batch && libraryOn()) return libraryExport();
  if (!batch && !ui.info) return;
  // Export what the crop tool shows.
  if (ui.crop.on) await exitCrop(true);
  const dlg = $("export-dialog");
  updateExportForm();
  dlg.querySelector("h2").textContent = batch ? `Export ${batch.paths.length} photo${batch.paths.length === 1 ? "" : "s"}` : "Export";
  $("export-go").textContent = batch ? "Choose a folder…" : "Export…";
  // Names from a template only for a batch (one photo is named in the save dialog).
  $("export-form").querySelector(".batch-name").hidden = !batch;
  if (batch) {
    ui.crop.enabled = false;
    ui.exportCropAspect = batch.aspect;
    ui.exportCropSize = [Math.round(3000 * Math.min(1, batch.aspect)), Math.round(3000 / Math.max(1, batch.aspect))];
  } else {
    try {
      const st = await invoke("crop_state");
      ui.crop.enabled = st.enabled;
      ui.exportCropAspect = st.rect.width / st.rect.height;
      ui.exportCropSize = [st.rect.width, st.rect.height];
    } catch {
      ui.exportCropAspect = ui.info.width / ui.info.height;
      ui.exportCropSize = [ui.info.width, ui.info.height];
    }
  }
  if (!ui.borders) await loadBorders();
  fillBorderSelect();
  await loadFonts();
  fillFontSelect($("export-form").b_font, $("export-form").b_font.value);
  fillFontSelect($("export-form").wm_font, $("export-form").wm_font.value);
  if (!batch) frameToExport();
  await fillExportPresets($("export-form").export_preset.value || null);
  updateWatermarkForm();
  updateExportSize();
  dlg.showModal();
  const choice = await new Promise((resolve) => dlg.addEventListener("close", () => resolve(dlg.returnValue), { once: true }));
  if (choice !== "ok") return;
  const f = $("export-form");
  const options = {
    format: f.format.value,
    quality: Number(f.quality.value),
    bits: Number(f.bits.value),
    color_space: f.color_space.value,
    resize: exportResize(),
    exact: Number(f.exact_w.value) > 0 && Number(f.exact_h.value) > 0
      ? [Number(f.exact_w.value), Number(f.exact_h.value)] : null,
    sharpen: f.sharpen.value || null,
    metadata: f.metadata.value,
    max_kb: f.max_kb.value ? Number(f.max_kb.value) : null,
    border: borderOn() ? readBorder() : null,
    name_template: batch && f.name_template.value.trim() ? f.name_template.value.trim() : null,
    watermark: readWatermark(),
    subsampling: f.subsampling.value || null,
    progressive: f.progressive.checked,
    avif_speed: Number(f.avif_speed.value) || null,
  };
  const after = f.after.value;
  if (batch) return batch.run(options, after);
  // "-edited": a JPEG named like its raw is hidden as the raw's pair.
  const base = `${ui.info.name.replace(/\.[^.]+$/, "")}-edited`;
  const path = await invoke("pick_export_path", { suggested: base, format: f.format.value, dir: lastExportDir() ?? dirOf(ui.info.path) });
  if (!path) return;
  rememberExportDir(path);
  status(`Exporting ${path}…`);
  $("busy").hidden = false;
  try {
    const r = await invoke("export", { path, options });
    toast(`Exported ${r.width}×${r.height}, ${(r.bytes / 1e6).toFixed(1)} MB${options.format === "jpeg" ? `, quality ${r.quality}` : ""}`);
    status(`Exported ${r.path}`);
    await afterExport([r.path], after);
  } catch (e) {
    toast(String(e), true);
  } finally {
    $("busy").hidden = true;
  }
}
