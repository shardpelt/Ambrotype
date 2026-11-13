// SPDX-License-Identifier: GPL-3.0-or-later
// Duplicate any preset (a copy in the app's own folder, the user's to change)
// and Edit… one of the user's own: name, group, the settings it holds, and
// its values taken again from the open photo. The file work is in
// src/preset_files.rs (`duplicate_preset`, `update_preset`).
"use strict";

const presetEditState = { path: "" };

async function presetDuplicate(p) {
  try {
    const r = await invoke("duplicate_preset", { path: p.path });
    ui.presets = r.presets;
    renderPresets();
    toast(`Copied as “${ui.presets.find((x) => x.path === r.path)?.name ?? `${p.name} copy`}”`);
  } catch (e) {
    showError(e);
  }
}

// The dialog: what the preset holds, ticked, by group; the open photo's other
// settings follow, unticked, once its values are the choice.
async function presetEdit(p) {
  let held;
  try {
    held = await invoke("preset_contents", { path: p.path });
    if (!ui.pasteGroups) ui.pasteGroups = await invoke("paste_groups");
  } catch (e) {
    showError(e);
    return;
  }
  presetEditState.path = p.path;
  $("preset-edit-name").value = p.name;
  $("preset-edit-group").value = p.group;
  const groups = [...new Set((ui.presets ?? []).map((x) => x.group).filter(Boolean))];
  $("preset-edit-groups").replaceChildren(...groups.map((g) => el("option", { value: g })));
  $("preset-edit-keep").checked = true;
  $("preset-edit-photo").disabled = !ui.info;
  $("preset-edit-nophoto").hidden = !!ui.info;
  presetEditError("");
  presetEditList(held);
  presetEditExtras();
  $("preset-edit-dialog").showModal();
  $("preset-edit-name").focus();
}

function presetEditError(text) {
  $("preset-edit-error").textContent = text;
  $("preset-edit-error").hidden = !text;
}

function presetEditList(contents) {
  const items = [...contents.held.map((h) => [h, false]), ...contents.photo.map((h) => [h, true])];
  const rows = [];
  for (const g of [...ui.pasteGroups, { name: "", label: "Other" }]) {
    const mine = items.filter(([h]) => h.group === g.name);
    if (!mine.length) continue;
    const head = presetGroupRow(g, mine.some(([, extra]) => !extra));
    head.classList.add("preset-edit-group");
    if (mine.every(([, extra]) => extra)) head.dataset.extra = "1";
    const boxes = mine.map(([h, extra]) => {
      const row = el("label", { class: "inline-check preset-edit-item" }, el("input", { type: "checkbox", value: h.name, checked: !extra }), el("span", {}, h.label));
      if (extra) row.dataset.extra = "1";
      return row;
    });
    head.querySelector("input").addEventListener("change", (e) => {
      for (const row of boxes) if (!row.hidden) row.querySelector("input").checked = e.target.checked;
    });
    rows.push(head, ...boxes);
  }
  $("preset-edit-list").replaceChildren(...rows);
}

// The photo's settings are listed only while its values are taken.
function presetEditExtras() {
  const on = $("preset-edit-photo").checked;
  for (const row of $("preset-edit-list").querySelectorAll("[data-extra]")) {
    row.hidden = !on;
    if (!on) row.querySelector("input").checked = false;
  }
}

async function presetEditSave(e) {
  e.preventDefault();
  const keep = [...$("preset-edit-list").querySelectorAll(".preset-edit-item input:checked")].map((b) => b.value);
  try {
    ui.presets = await invoke("update_preset", {
      path: presetEditState.path,
      name: $("preset-edit-name").value,
      group: $("preset-edit-group").value,
      keep,
      fromPhoto: $("preset-edit-photo").checked,
    });
  } catch (err) {
    presetEditError(String(err));
    return;
  }
  $("preset-edit-dialog").close("ok");
  await loadPresetFavourites();
  await loadPresetHidden();
  renderPresets();
  toast(`Saved “${$("preset-edit-name").value.trim()}”`);
}

function wirePresetEdit() {
  $("preset-edit-save").addEventListener("click", presetEditSave);
  for (const id of ["preset-edit-keep", "preset-edit-photo"]) $(id).addEventListener("change", presetEditExtras);
}
