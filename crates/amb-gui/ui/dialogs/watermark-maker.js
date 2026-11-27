// SPDX-License-Identifier: GPL-3.0-or-later
// The Watermark maker (File → Watermark maker…, or Design… in Export): a
// text or logo watermark designed on a live preview of the open photo, kept
// by name, and picked again in Export ("Saved") or `amb render --watermark
// NAME`. The preview is the export's own code path (commands/watermarks.rs);
// the marks are amb_library::watermarks. Export's watermark fields are
// read and filled here, so dialogs/export.js stays small.
"use strict";

const WMK_DEBOUNCE_MS = 150;
// Field names in Export's form, and the maker's own ids, for the numbers
// whose range Export's markup already holds (one home).
const WMK_RANGED = [["wm_size", "wmk-size"], ["wm_opacity", "wmk-opacity"], ["wm_rotate", "wmk-rotate"]];

const wmk = { saved: [], image: "", seq: 0, timer: 0, url: "", fromExport: false };

const wmkField = (id) => $(`wmk-${id}`);

// The mark as the fields say it, or null with neither text nor a logo.
function wmkRead() {
  const text = wmkField("text").value.trim();
  if (!wmk.image && !text) return null;
  return {
    text: wmk.image ? "" : text,
    image: wmk.image || null,
    color: wmkField("color").value,
    size: Number(wmkField("size").value) || 20,
    opacity: Number(wmkField("opacity").value),
    anchor: wmkField("anchor").value,
    inset: Number(wmkField("inset").value) || 0,
    shadow: wmkField("shadow").checked,
    rotate: Number(wmkField("rotate").value) || 0,
    tile: wmkField("tile").checked,
    font: wmkField("font").value || "",
  };
}

// Fill the fields from a mark (a saved one, or Export's).
function wmkWrite(mark) {
  wmk.image = mark.image ?? "";
  wmkField("text").value = mark.text ?? "";
  wmkField("color").value = mark.color ?? "#ffffff";
  wmkField("size").value = mark.size ?? 20;
  wmkField("opacity").value = mark.opacity ?? 70;
  wmkField("anchor").value = mark.anchor ?? "bottom-right";
  wmkField("inset").value = mark.inset ?? 3;
  wmkField("shadow").checked = mark.shadow ?? true;
  wmkField("rotate").value = mark.rotate ?? 0;
  wmkField("tile").checked = Boolean(mark.tile);
  wmkField("font").value = mark.font || "sans";
  wmkForm();
}

// What the fields allow now: a logo replaces the text, a repeated mark
// covers the photo (no one place for it).
function wmkForm() {
  wmkField("text").disabled = Boolean(wmk.image);
  wmkField("text").placeholder = wmk.image ? t("Using a logo") : t("e.g. © Your Name");
  wmkField("logo").textContent = t("Logo…");
  wmkField("logo-clear").hidden = !wmk.image;
  wmkField("logo-name").textContent = wmk.image ? fileName(wmk.image) : "";
  for (const c of document.querySelectorAll("#wmk-dialog .wmk-colour")) c.hidden = Boolean(wmk.image);
  wmkField("anchor").disabled = wmkField("tile").checked;
  wmkField("inset").disabled = wmkField("tile").checked;
}

function wmkHint(text) {
  wmkField("hint").textContent = text;
  wmkField("hint").hidden = !text;
}

async function wmkRefresh() {
  const seq = ++wmk.seq;
  const mark = wmkRead();
  if (!ui.info) return wmkHint(t("Open a photo to see the preview"));
  if (!mark) return wmkHint(t("Type some text or choose a logo to see it on the photo"));
  try {
    const buf = await invoke("watermark_preview", { mark });
    if (seq !== wmk.seq) return;
    const url = URL.createObjectURL(new Blob([new Uint8Array(buf)], { type: "image/jpeg" }));
    wmkField("preview").src = url;
    wmkField("preview").hidden = false;
    wmkHint("");
    if (wmk.url) URL.revokeObjectURL(wmk.url);
    wmk.url = url;
  } catch (e) {
    if (seq === wmk.seq) wmkHint(t(String(e)));
  }
}

function wmkChanged() {
  wmkForm();
  clearTimeout(wmk.timer);
  wmk.timer = setTimeout(() => wmkRefresh(), WMK_DEBOUNCE_MS);
}

// A click on the preview puts the mark at the nearest of the nine places.
function wmkPlace(ev) {
  if (wmkField("tile").checked) return;
  const box = wmkField("preview").getBoundingClientRect();
  const at = (v, from, size) => Math.round(Math.max(0, Math.min(1, (v - from) / size)) * 2);
  const i = at(ev.clientY, box.top, box.height) * 3 + at(ev.clientX, box.left, box.width);
  wmkField("anchor").selectedIndex = i;
  wmkChanged();
}

// The saved watermarks in a select, `selected` kept where it still exists.
function wmkFillSelect(sel, selected = "") {
  sel.replaceChildren(el("option", { value: "" }, wmk.saved.length ? t("Choose…") : t("None saved")),
    ...wmk.saved.map((s) => el("option", { value: s.name, translate: "no" }, s.name)));
  sel.value = wmk.saved.some((s) => s.name === selected) ? selected : "";
}

async function wmkLoadSaved() {
  try {
    wmk.saved = await invoke("watermarks");
  } catch {
    wmk.saved = [];
  }
}

function wmkFillAll(selected = "") {
  wmkFillSelect(wmkField("saved"), selected);
  wmkFillSelect($("wm-saved"), $("wm-saved").value);
  wmkField("delete").disabled = !wmkField("saved").value;
}

// Export's dialog opening: its Saved list is current.
async function wmkFillExport() {
  await wmkLoadSaved();
  wmkFillSelect($("wm-saved"));
}

// Export's watermark fields as a mark (null when it has none).
const wmkFromExport = () => readWatermark();

// A mark into Export's watermark fields.
function wmkToExport(mark) {
  const f = $("export-form");
  f.wm_text.value = mark.text ?? "";
  f.wm_image.value = mark.image ?? "";
  f.wm_anchor.value = mark.anchor ?? "bottom-right";
  f.wm_size.value = mark.size ?? 20;
  f.wm_opacity.value = mark.opacity ?? 70;
  f.wm_rotate.value = mark.rotate ?? 0;
  f.wm_tile.checked = Boolean(mark.tile);
  f.wm_font.value = mark.font || "sans";
  f.wm_color.value = mark.color ?? "#ffffff";
  f.wm_shadow.checked = mark.shadow ?? true;
  f.wm_inset.value = mark.inset ?? 3;
  updateWatermarkForm();
}

async function wmkOpen(opts = {}) {
  await loadFonts();
  fillFontSelect(wmkField("font"), wmkField("font").value);
  for (const [from, to] of WMK_RANGED) {
    for (const a of ["min", "max", "step"]) wmkField(to.slice(4)).setAttribute(a, $("export-form")[from].getAttribute(a));
  }
  wmk.fromExport = Boolean(opts.fromExport);
  const start = wmk.fromExport ? wmkFromExport() : null;
  if (start) wmkWrite(start);
  else if (!wmkRead()) wmkWrite({ text: "" });
  wmkField("use").hidden = !wmk.fromExport;
  wmkField("status").textContent = "";
  await wmkLoadSaved();
  wmkFillAll();
  wmkForm();
  const dlg = $("wmk-dialog");
  if (!dlg.open) dlg.showModal();
  wmkRefresh();
}

function wmkClose() {
  wmk.seq++;
  clearTimeout(wmk.timer);
  if (wmk.url) URL.revokeObjectURL(wmk.url);
  wmk.url = "";
  wmkField("preview").hidden = true;
  wmkField("preview").removeAttribute("src");
  $("wmk-dialog").close();
}

async function wmkSave() {
  const name = wmkField("name").value.trim();
  const mark = wmkRead();
  if (!name) return void (wmkField("status").textContent = t("Give the watermark a name."));
  if (!mark) return void (wmkField("status").textContent = t("Type some text or choose a logo first."));
  try {
    wmk.saved = await invoke("save_watermark", { name, mark });
    wmkFillAll(wmk.saved.find((s) => s.name.toLowerCase() === name.toLowerCase())?.name);
    wmkField("status").textContent = t("Saved the watermark {name}. Export → Saved picks it, and amb render --watermark uses it.", { name });
    toast(t("Saved the watermark {name}", { name }));
  } catch (e) {
    wmkField("status").textContent = t(String(e));
  }
}

async function wmkDelete() {
  const name = wmkField("saved").value;
  if (!name || !(await askYes(t("Delete the watermark {name}?", { name })))) return;
  try {
    wmk.saved = await invoke("delete_watermark", { name });
    wmkFillAll();
  } catch (e) {
    wmkField("status").textContent = t(String(e));
  }
}

function wmkPick() {
  const s = wmk.saved.find((w) => w.name === wmkField("saved").value);
  wmkField("delete").disabled = !s;
  if (!s) return;
  wmkWrite(s.mark);
  wmkField("name").value = s.name;
  wmkChanged();
}

function wireWatermarkMaker() {
  for (const id of ["text", "color", "size", "opacity", "shadow", "anchor", "inset", "rotate", "tile", "font"]) {
    for (const ev of ["input", "change"]) wmkField(id).addEventListener(ev, wmkChanged);
  }
  wmkField("logo").addEventListener("click", async () => {
    const path = await invoke("pick_watermark_image");
    if (!path) return;
    wmk.image = path;
    wmkChanged();
  });
  wmkField("logo-clear").addEventListener("click", () => {
    wmk.image = "";
    wmkChanged();
  });
  wmkField("preview").addEventListener("click", wmkPlace);
  wmkField("saved").addEventListener("change", wmkPick);
  wmkField("save").addEventListener("click", () => wmkSave());
  wmkField("delete").addEventListener("click", () => wmkDelete());
  wmkField("close").addEventListener("click", wmkClose);
  $("wmk-dialog").addEventListener("cancel", wmkClose);
  wmkField("use").addEventListener("click", () => {
    const mark = wmkRead();
    if (mark) wmkToExport(mark);
    wmkClose();
  });
  $("wm-design").addEventListener("click", () => wmkOpen({ fromExport: true }).catch(showError));
  $("wm-saved").addEventListener("change", () => {
    const s = wmk.saved.find((w) => w.name === $("wm-saved").value);
    if (s) wmkToExport(s.mark);
  });
}
