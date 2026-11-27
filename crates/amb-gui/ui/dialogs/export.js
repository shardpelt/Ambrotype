// SPDX-License-Identifier: GPL-3.0-or-later
// The Export dialog: formats, sizes, watermarks, export presets, and Edit
// in another app.
"use strict";

// A photo on the clipboard, with its edit (3072 pixels on the long side),
// to paste into a chat, mail or document.
async function copyImage(path) {
  status(t("Copying the photo…"));
  try {
    const [w, h] = await invoke("copy_image", { path });
    toast(t("Copied the photo ({w} × {h}): paste it anywhere", { w, h }));
    status(t("Copied the photo"));
  } catch (e) {
    showError(e);
  }
}

async function copyPath(paths) {
  try {
    await invoke("copy_text", { text: paths.join("\n") });
    toast(paths.length === 1 ? t("Copied the file's path") : t("Copied {n} paths", { n: paths.length }));
  } catch (e) {
    showError(e);
  }
}

// Right-click on the photo: copy it, its path, find it, export it.
function photoMenu(e) {
  if (!ui.info) return;
  const path = ui.info.path;
  contextMenu(e, [
    ...aidsPlaceItems(e),
    [t("Copy image"), () => copyImage(path)],
    [t("Copy file path"), () => copyPath([path])],
    [t("Show in folder"), () => invoke("reveal_file", { path }).catch(showError)],
    [t("Export…"), () => exportFlow()],
    [t("Share before and after…"), () => shareCompareFlow()],
    [t("Export Edit Replay (video)…"), () => replayFlow()],
    [t("Make a preset from this edit…"), () => savePresetFlow()],
    [t("Convert this folder's raws to JPEGs…"), () => convertRawsFlow()],
  ]);
}

// A web page with the photo before and after its edit (src/commands/export.rs
// `share_compare`).
async function shareCompareFlow() {
  if (!ui.info) return;
  status(t("Making the before/after page…"));
  $("busy").hidden = false;
  try {
    const path = await invoke("share_compare", { dir: lastExportDir() });
    if (path) {
      rememberExportDir(path);
      toast(t("Saved the before/after page: {path}", { path }));
    }
  } catch (e) {
    showError(e);
  } finally {
    $("busy").hidden = true;
    status(t("Ready"));
  }
}

// File → Export Edit Replay…: a video of the edit from its History
// (src/commands/export.rs `export_replay`), with frames counted in the status.
async function replayFlow() {
  if (!ui.info) return;
  if (!(ui.state?.history ?? []).length) return toast(t("This photo has no History to replay yet: edit it first."), true);
  status(t("Making the Edit Replay…"));
  $("busy").hidden = false;
  const off = await tauri.event.listen("replay-progress", (e) => {
    const [done, total] = e.payload;
    status(t("Making the Edit Replay: frame {done} of {total}", { done, total }));
  });
  try {
    const r = await invoke("export_replay", { dir: lastExportDir() });
    if (r) {
      rememberExportDir(r.path);
      toast(t("Saved the Edit Replay ({steps} steps, {seconds} s): {path}", { steps: r.steps, seconds: r.seconds.toFixed(1), path: r.path }));
    }
  } catch (e) {
    showError(e);
  } finally {
    off();
    $("busy").hidden = true;
    status(t("Ready"));
  }
}

// The folder the last export went to, on this computer: the save dialogs
// start there (else next to the photo).
function lastExportDir() {
  return store.text("amb.exportDir") || null;
}

function rememberExportDir(path) {
  if (!path) return;
  store.setText("amb.exportDir", dirOf(path));
}

const DEPTHS = { jpeg: [8], png: [8, 16], tiff: [16, 8, 32], webp: [8], avif: [8], exr: [32], jxl: [8, 16] };

function updateExportForm() {
  const f = $("export-form");
  const format = f.format.value;
  f.bits.replaceChildren(...DEPTHS[format].map((b) => el("option", { value: b }, b === 32 ? t("32-bit float") : t("{n}-bit", { n: b }))));
  f.quality.closest("label").style.visibility = ["jpeg", "avif"].includes(format) ? "visible" : "hidden";
  f.max_mb.closest("label").style.visibility = format === "jpeg" ? "visible" : "hidden";
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
    note.textContent = f.exact_w.value || f.exact_h.value ? t("Give both a width and a height for an exact size.") : "";
    return;
  }
  const a = ui.exportCropAspect;
  const target = w / h;
  const crop = ui.crop.enabled;
  if (borderOn() && f.b_fit.checked) {
    note.textContent = crop
      ? t("Exactly {w} × {h} px, with the whole crop inside the frame.", { w, h })
      : t("Exactly {w} × {h} px, with the whole photo inside the frame.", { w, h });
  } else if (Math.abs(Math.log(a / target)) > 0.005) {
    note.classList.add("warn");
    const v = { w, h, a: ratioText(a), b: ratioText(target) };
    const sides = a > target
      ? crop
        ? t("Exactly {w} × {h} px. The crop is {a} and this size is {b}, so the left and right edges are trimmed (centred).", v)
        : t("Exactly {w} × {h} px. The photo is {a} and this size is {b}, so the left and right edges are trimmed (centred).", v)
      : crop
        ? t("Exactly {w} × {h} px. The crop is {a} and this size is {b}, so the top and bottom edges are trimmed (centred).", v)
        : t("Exactly {w} × {h} px. The photo is {a} and this size is {b}, so the top and bottom edges are trimmed (centred).", v);
    note.textContent = t("{sides} To choose what is kept, crop with this size preset first.", { sides });
  } else {
    note.textContent = t("Exactly {w} × {h} px; the crop already has this shape.", { w, h });
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
  "sharpen", "metadata", "max_mb", "border_preset", "name_template",
  "wm_text", "wm_image", "wm_anchor", "wm_size", "wm_opacity", "wm_color", "wm_shadow", "wm_rotate", "wm_tile",
  "subsampling", "progressive", "avif_speed", "after", "small_copy", "small_mb"];

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
    inset: f.wm_inset.value === "" ? 3 : Number(f.wm_inset.value),
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
  f.wm_text.placeholder = image ? t("Using a logo") : t("None (e.g. © Your Name)");
  $("wm-logo").textContent = image ? t("No logo") : t("Logo…");
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
  const option = (value, text) => el("option", { value: value }, text);
  f.export_preset.replaceChildren(option("", ui.exportPresets.length ? t("— choose —") : t("None saved yet")),
    ...ui.exportPresets.map((p) => el("option", { value: p.name, translate: "no" }, p.name)));
  f.export_preset.value = selected ?? "";
  $("export-preset-delete").disabled = !f.export_preset.value;
}

// The smaller copy's size only matters once it is on.
function updateSmallCopy() {
  const f = $("export-form");
  f.small_mb.disabled = !f.small_copy.checked;
}

// With a size limit, the quality is the most it gets: it comes down to fit.
function updateQualityName() {
  const f = $("export-form");
  const limited = f.format.value === "jpeg" && Number(f.max_mb.value) > 0;
  const name = f.quality.closest("label").querySelector(".q-name");
  name.textContent = limited ? t("Quality (at most)") : t("Quality");
  name.title = limited ? t("Lowered only as far as it must for the file to stay under {mb} MB", { mb: f.max_mb.value }) : "";
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
  // Presets saved before the limit was in MB kept it in KB.
  if (!("max_mb" in p.fields) && p.fields.max_kb) f.max_mb.value = Number(p.fields.max_kb) / 1000;
  updateWatermarkForm();
  updateSmallCopy();
  updateQualityName();
  f.quality.dispatchEvent(new Event("input"));
  f.border_preset.dispatchEvent(new Event("change"));
  f.size_preset.value = "";
  updateExportSize();
}

function wireExportPresets() {
  const f = $("export-form");
  f.export_preset.addEventListener("change", () => applyExportPreset(f.export_preset.value));
  f.small_copy.addEventListener("change", updateSmallCopy);
  f.max_mb_pick.addEventListener("change", () => {
    const v = f.max_mb_pick.value;
    if (v) f.max_mb.value = v === "none" ? "" : v;
    f.max_mb_pick.value = "";
    updateQualityName();
  });
  f.max_mb.addEventListener("input", updateQualityName);
  f.format.addEventListener("change", updateQualityName);
  $("export-preset-save").addEventListener("click", async () => {
    const name = await askName(t("Save export preset"), t("Keeps the format, size, sharpening, metadata and border choices."), f.export_preset.value);
    if (!name) return;
    const fields = Object.fromEntries(EXPORT_FIELDS.filter((n) => f[n]).map((n) => [n, fieldValue(f[n])]));
    try {
      await invoke("save_export_preset", { name, fields });
      await fillExportPresets(name);
      toast(t("Saved the export preset “{name}”", { name }));
    } catch (e) {
      showError(e);
    }
  });
  $("export-preset-delete").addEventListener("click", async () => {
    const name = f.export_preset.value;
    if (!name) return;
    try {
      await invoke("delete_export_preset", { name });
      await fillExportPresets();
    } catch (e) {
      showError(e);
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
  status(t("Writing the TIFF for the other app…"));
  try {
    const path = await invoke("edit_in");
    status(t("Opened {name} in the other app", { name: fileName(path) }));
    toast(t("Wrote {name} next to the photo and opened it. Save it there; it shows in the folder.", { name: fileName(path) }));
  } catch (e) {
    showError(e);
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
    showError(e);
  }
}

async function exportFlow(batch = null) {
  if (!batch && libraryOn()) return libraryExport();
  if (!batch && !ui.info) return;
  // Export what the crop tool shows.
  if (ui.crop.on) await exitCrop(true);
  const dlg = $("export-dialog");
  updateExportForm();
  dlg.querySelector("h2").textContent = batch ? t("Export {photos}", { photos: plural(batch.paths.length, "photo") }) : t("Export");
  $("export-go").textContent = batch ? t("Choose a folder…") : t("Export…");
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
  await wmkFillExport();
  updateWatermarkForm();
  updateSmallCopy();
  updateQualityName();
  updateExportSize();
  aiFreeExportNote(!batch);
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
    max_kb: Number(f.max_mb.value) > 0 ? Math.round(Number(f.max_mb.value) * 1000) : null,
    border: borderOn() ? readBorder() : null,
    name_template: batch && f.name_template.value.trim() ? f.name_template.value.trim() : null,
    watermark: readWatermark(),
    subsampling: f.subsampling.value || null,
    progressive: f.progressive.checked,
    avif_speed: Number(f.avif_speed.value) || null,
    small_mb: f.small_copy.checked && Number(f.small_mb.value) > 0 ? Number(f.small_mb.value) : null,
  };
  const after = f.after.value;
  if (batch) return batch.run(options, after);
  // "-edited": a JPEG named like its raw is hidden as the raw's pair.
  const base = `${ui.info.name.replace(/\.[^.]+$/, "")}-edited`;
  const path = await invoke("pick_export_path", { suggested: base, format: f.format.value, dir: lastExportDir() ?? dirOf(ui.info.path) });
  if (!path) return;
  rememberExportDir(path);
  status(t("Exporting {path}…", { path }));
  $("busy").hidden = false;
  try {
    const r = await invoke("export", { path, options });
    const v = { size: `${r.width}×${r.height}`, mb: (r.bytes / 1e6).toFixed(1), quality: r.quality, smallMb: r.small ? (r.small[1] / 1e6).toFixed(1) : "", name: r.small ? fileName(r.small[0]) : "" };
    const jpeg = options.format === "jpeg";
    const done = jpeg
      ? r.small ? t("Exported {size}, {mb} MB, quality {quality}, and a {smallMb} MB copy ({name})", v) : t("Exported {size}, {mb} MB, quality {quality}", v)
      : r.small ? t("Exported {size}, {mb} MB, and a {smallMb} MB copy ({name})", v) : t("Exported {size}, {mb} MB", v);
    // What AI-free mode left out of the file; the parts' names are shown as they are.
    toast(r.left_out.length ? `${done}; ${t("left out for AI-free mode: {parts}", { parts: r.left_out.join(", ") })}` : done);
    status(t("Exported {path}", { path: r.path }));
    await afterExport([r.path], after);
  } catch (e) {
    showError(e);
  } finally {
    $("busy").hidden = true;
  }
}
