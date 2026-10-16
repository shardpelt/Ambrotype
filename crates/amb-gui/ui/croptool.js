// SPDX-License-Identifier: GPL-3.0-or-later
// The crop and straighten panel (crop.js draws the overlay), and the size
// presets dialog.
"use strict";

const CROP_TAGS = ["HasCrop", "CropLeft", "CropTop", "CropRight", "CropBottom", "CropAngle"];
const ASPECTS = ["1:1", "3:2", "2:3", "4:5", "4:3", "3:4", "16:9", "9:16"];

function cropKey(settings) {
  return JSON.stringify(CROP_TAGS.map((t) => settings[t] ?? null));
}

// "3:2" for 1.5; "1.91:1" when no small whole-number ratio is close.
function ratioText(a) {
  for (let n = 1; n <= 16; n++) {
    const m = Math.round(a * n);
    if (m > 0 && Math.abs(m / n - a) / a < 0.004) return `${m}:${n}`;
  }
  return `${a.toFixed(2)}:1`;
}

// Width / height of an aspect choice before the swap, or null (free).
function aspectRatio(value) {
  if (value === "original") return ui.info ? ui.info.width / ui.info.height : null;
  if (value.startsWith("size:")) {
    const p = ui.sizes.find((x) => x.id === value.slice(5));
    return p ? p.width / p.height : null;
  }
  const m = value.match(/^(\d+):(\d+)$/);
  return m ? Number(m[1]) / Number(m[2]) : null;
}

function lockedRatio() {
  const r = aspectRatio(ui.crop.aspect);
  return r && ui.crop.swapped ? 1 / r : r;
}

// Keep the aspect lock when the rectangle has it (either way round);
// otherwise Original for an uncropped photo, else Free.
function chooseAspect(rect, enabled) {
  const r = rect.width / rect.height;
  const close = (a, b) => Math.abs(Math.log(a / b)) < 0.005;
  for (const value of [ui.crop.aspect, "original"]) {
    const base = aspectRatio(value);
    if (!base) continue;
    if (close(base, r) || close(1 / base, r)) {
      ui.crop.aspect = value;
      ui.crop.swapped = !close(base, r);
      return;
    }
  }
  ui.crop.aspect = enabled ? "free" : "original";
  ui.crop.swapped = false;
}

function fillSizeSelects() {
  const groups = new Map();
  for (const p of ui.sizes) {
    if (!groups.has(p.group)) groups.set(p.group, []);
    groups.get(p.group).push(p);
  }
  const option = (parent, value, text) => {
    const o = document.createElement("option");
    o.value = value;
    o.textContent = text;
    parent.append(o);
  };
  // Site groups (and yours) after the common ones; the picker shows them collapsed.
  const addSizes = (sel, skip = []) => {
    for (const [group, items] of groups) {
      if (skip.includes(group)) continue;
      const og = document.createElement("optgroup");
      og.label = group || "Yours";
      for (const p of items) option(og, `size:${p.id}`, `${p.name} (${p.width} × ${p.height})`);
      sel.append(og);
    }
  };
  const crop = $("crop-aspect");
  crop.innerHTML = "";
  option(crop, "original", "Original");
  option(crop, "free", "Free");
  const ratios = document.createElement("optgroup");
  ratios.label = "Common ratios";
  for (const r of ASPECTS) option(ratios, r, r.replace(":", " : "));
  crop.append(ratios);
  // The crop's own ratio list covers the common sizes.
  addSizes(crop, ["Common"]);
  if (!aspectRatio(ui.crop.aspect) && ui.crop.aspect !== "free") ui.crop.aspect = "original";
  crop.value = ui.crop.aspect;

  const sel = $("export-form").size_preset;
  const keep = sel.value;
  sel.innerHTML = "";
  option(sel, "", "None");
  option(sel, "custom", "Custom size");
  addSizes(sel);
  sel.value = [...sel.options].some((o) => o.value === keep) ? keep : "";
}

async function loadSizes() {
  try {
    ui.sizes = await invoke("size_presets");
  } catch (e) {
    toast(String(e), true);
  }
  fillSizeSelects();
}

function syncCropPanel(rect) {
  if (!rect) return;
  const a = rect.angle;
  $("crop-angle").value = a;
  $("crop-angle-num").value = (a > 0 ? "+" : "") + a.toFixed(2);
  $("crop-size").textContent = `${Math.round(rect.width)} × ${Math.round(rect.height)} px  ·  ${ratioText(rect.width / rect.height)}`;
  $("crop-aspect").value = ui.crop.aspect;
}

function setCropMode(on) {
  ui.crop.on = on;
  $("crop-canvas").hidden = !on;
  $("crop-panel").hidden = !on;
  $("preview").hidden = on || !ui.preview.url;
  $("btn-crop").classList.toggle("active", on);
  // No 100% view or white-balance picking on the uncropped view.
  for (const id of ["btn-zoom", "btn-wb"]) $(id).disabled = on || !ui.info;
}

async function enterCrop() {
  if (!ui.info || ui.crop.on) return;
  exitMasks();
  exitSpots();
  exitGuided();
  if (ui.zoom.on) setZoom(false);
  setPicking(false);
  let st;
  try {
    st = await invoke("crop_state");
  } catch (e) {
    toast(String(e), true);
    return;
  }
  ui.crop.start = st.rect;
  ui.crop.enabled = st.enabled;
  ui.crop.angleSign = st.angle_sign;
  chooseAspect(st.rect, st.enabled);
  setCropMode(true);
  // No crop yet: start by drawing one, like a screenshot selection.
  ui.crop.tool.setRatio(lockedRatio());
  ui.crop.tool.start({ width: ui.info.width, height: ui.info.height, angleSign: st.angle_sign, rect: st.rect, draw: !st.enabled });
  syncCropPanel(st.rect);
  requestPreview(false);
}

// The committed crop changed while the tool is open (undo, redo, reset).
async function reloadCrop() {
  try {
    const st = await invoke("crop_state");
    if (!ui.crop.on) return;
    ui.crop.start = st.rect;
    ui.crop.enabled = st.enabled;
    chooseAspect(st.rect, st.enabled);
    ui.crop.tool.setRatio(lockedRatio());
    ui.crop.tool.setRect(st.rect);
  } catch (e) {
    toast(String(e), true);
  }
}

function cropChanged(rect) {
  const s = ui.crop.start;
  if (!s || !rect) return false;
  return ["cx", "cy", "width", "height"].some((k) => Math.abs(rect[k] - s[k]) > 0.25) ||
    Math.abs(rect.angle - s.angle) > 0.005;
}

// Leave the crop tool; with `commit`, its rectangle becomes one undo step.
async function exitCrop(commit) {
  if (!ui.crop.on) return;
  const rect = ui.crop.tool.rect();
  setCropMode(false);
  ui.crop.tool.stop();
  status("Ready");
  if (commit && cropChanged(rect)) {
    try {
      const commands = await invoke("crop_commands", { rect });
      await run(commands, "crop");
      return;
    } catch (e) {
      toast(String(e), true);
    }
  }
  requestPreview(false);
}

function toggleCrop() {
  if (ui.crop.on) exitCrop(true);
  else enterCrop();
}

// Reset: no crop, no straighten (one undo step when there was a crop).
async function resetCrop() {
  if (!ui.crop.on) return;
  const full = { cx: ui.info.width / 2, cy: ui.info.height / 2, width: ui.info.width, height: ui.info.height, angle: 0 };
  ui.crop.aspect = "original";
  ui.crop.swapped = false;
  ui.crop.tool.setRatio(lockedRatio());
  ui.crop.tool.setRect(full);
  if (ui.crop.enabled) {
    try {
      // reloadCrop (via setState) then picks up the cleared crop.
      await run(await invoke("crop_commands", { rect: full }), "reset crop");
    } catch (e) {
      toast(String(e), true);
    }
  } else {
    ui.crop.start = full;
  }
}

function setCropAspect(value) {
  ui.crop.aspect = value;
  ui.crop.swapped = false;
  ui.crop.tool.setRatio(lockedRatio());
  ui.crop.tool.fitRatio();
}

function swapCrop() {
  if (!ui.crop.on) return;
  if (aspectRatio(ui.crop.aspect)) ui.crop.swapped = !ui.crop.swapped;
  ui.crop.tool.swap();
  ui.crop.tool.setRatio(lockedRatio());
}

function wireCrop() {
  const img = $("preview");
  ui.crop.tool = AmbCrop.create($("crop-canvas"), {
    // Only the whole-photo preview, once loaded (not a cropped one still showing).
    image: () => (ui.preview.uncropped && img.complete && img.naturalWidth ? img : null),
    onChange: (rect) => syncCropPanel(rect),
    onDrawMode: (on) => {
      $("crop-draw").classList.toggle("active", on);
      const hint = on
        ? "Press at one corner of the crop and drag to the opposite corner. Or adjust the frame with the controls here."
        : "Drag the edges or corners, inside to move, outside the frame to straighten. Draw ⌖ (D) starts a new one. Enter applies, Esc cancels.";
      $("crop-hint").textContent = hint;
      if (ui.crop.on) status(on ? "Crop: drag from one corner to the other" : "Crop: adjust the frame; Enter applies, Esc cancels");
    },
  });
  $("crop-draw").addEventListener("click", () => ui.crop.tool.setDrawMode(!ui.crop.tool.drawMode()));
  img.addEventListener("load", () => { if (ui.crop.on) ui.crop.tool.draw(); });
  $("btn-crop").addEventListener("click", toggleCrop);
  $("btn-masks").addEventListener("click", toggleMasks);
  $("btn-spots").addEventListener("click", toggleSpots);
  $("crop-done").addEventListener("click", () => exitCrop(true));
  $("crop-cancel").addEventListener("click", () => exitCrop(false));
  $("crop-reset").addEventListener("click", resetCrop);
  $("crop-swap").addEventListener("click", swapCrop);
  $("crop-sizes").addEventListener("click", sizesFlow);
  $("crop-aspect").addEventListener("change", (e) => {
    setCropAspect(e.target.value);
    // Let Enter apply the crop again rather than reopen the list.
    e.target.blur();
  });
  const angle = $("crop-angle");
  angle.addEventListener("input", () => ui.crop.tool.setAngle(Number(angle.value)));
  angle.addEventListener("change", () => ui.crop.tool.endAngle());
  const straight = () => { ui.crop.tool.setAngle(0); ui.crop.tool.endAngle(); };
  angle.addEventListener("dblclick", straight);
  angle.previousElementSibling.addEventListener("dblclick", straight);
  $("crop-angle-num").addEventListener("change", (e) => {
    const v = Number(e.target.value);
    if (Number.isFinite(v)) { ui.crop.tool.setAngle(v); ui.crop.tool.endAngle(); }
  });
}

// ---------------------------------------------------------------- size presets

async function sizesFlow() {
  const dlg = $("sizes-dialog");
  const f = $("sizes-form");
  let editing = "";  // id of the user preset being edited; "" adds a new one
  const fill = (p, id) => {
    editing = id;
    f.name.value = p ? (id ? p.name : `${p.name} (copy)`) : "";
    f.group.value = p ? p.group : "";
    f.width.value = p ? p.width : "";
    f.height.value = p ? p.height : "";
    $("sizes-editing").textContent = id ? `Editing "${p.name}"` : p ? `New size from "${p.name}"` : "New size";
  };
  const list = () => {
    const root = $("sizes-list");
    root.innerHTML = "";
    let group = null;
    for (const p of ui.sizes) {
      if (p.group !== group) {
        group = p.group;
        const h = document.createElement("div");
        h.className = "size-group";
        h.textContent = group + (p.builtin ? "" : " (yours)");
        root.append(h);
      }
      const row = document.createElement("div");
      row.className = "size-row";
      const name = document.createElement("span");
      name.textContent = p.name;
      const dims = document.createElement("span");
      dims.className = "dims";
      dims.textContent = `${p.width} × ${p.height}`;
      const buttons = document.createElement("span");
      buttons.className = "buttons";
      const button = (text, title, action) => {
        const b = document.createElement("button");
        b.type = "button";
        b.className = "small";
        b.textContent = text;
        b.title = title;
        b.addEventListener("click", action);
        buttons.append(b);
      };
      button("Clone", "Start a new size from this one", () => fill(p, ""));
      if (!p.builtin) {
        button("Edit", "Change this size", () => fill(p, p.id));
        button("Delete", "Delete this size", async () => {
          try {
            ui.sizes = await invoke("delete_size_preset", { id: p.id });
            if (editing === p.id) fill(null, "");
            fillSizeSelects();
            list();
          } catch (e) {
            toast(String(e), true);
          }
        });
      }
      row.append(name, dims, buttons);
      root.append(row);
    }
  };
  const save = async () => {
    const preset = {
      id: editing,
      name: f.name.value,
      group: f.group.value,
      width: Number(f.width.value) || 0,
      height: Number(f.height.value) || 0,
    };
    try {
      ui.sizes = await invoke("save_size_preset", { preset });
      fillSizeSelects();
      list();
      toast(`Saved size "${preset.name.trim()}"`);
      fill(null, "");
    } catch (e) {
      toast(String(e), true);
    }
  };
  f.onsubmit = (e) => {
    if (e.submitter?.value === "close") return;
    e.preventDefault();
    save();
  };
  $("sizes-new").onclick = () => fill(null, "");
  list();
  fill(null, "");
  dlg.showModal();
}
