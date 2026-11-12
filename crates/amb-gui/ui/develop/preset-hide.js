// SPDX-License-Identifier: GPL-3.0-or-later
// Hiding presets and groups, and deleting presets saved in the app. Hidden
// ones are left out of every list (`presetsShown`) until "Show" is on at the
// foot of the Presets tab; nothing is deleted by hiding. Deleting (only
// presets in the app's own folder) moves the file to the trash
// (src/commands/presets.rs `delete_preset`).
"use strict";

const presetHide = { presets: new Set(), groups: new Set(), showing: false };

const presetGroupOf = (p) => p.group || "Presets";

async function loadPresetHidden() {
  try {
    const h = await invoke("preset_hidden");
    presetHide.presets = new Set(h.presets);
    presetHide.groups = new Set(h.groups);
  } catch {
    presetHide.presets = new Set();
    presetHide.groups = new Set();
  }
}

const presetIsHidden = (p) => presetHide.presets.has(p.path) || presetHide.groups.has(presetGroupOf(p));

// The one list every picker reads: the presets, without the hidden ones.
// The Presets tab asks for them too while "Show" is on.
function presetsShown(withHidden = false) {
  const all = ui.presets ?? [];
  return withHidden ? all : all.filter((p) => !presetIsHidden(p));
}

// A hidden preset or group count for the foot: how many presets are out of sight.
const presetHiddenCount = () => (ui.presets ?? []).filter(presetIsHidden).length;

async function presetSetHidden(p, on) {
  try {
    await invoke("set_preset_hidden", { path: p.path, on });
    await loadPresetHidden();
    renderPresets();
    if (on) toast("Hidden");
  } catch (e) {
    showError(e);
  }
}

async function presetSetGroupHidden(group, on) {
  try {
    await invoke("set_preset_group_hidden", { group, on });
    await loadPresetHidden();
    if (!presetHiddenCount()) presetHide.showing = false;
    renderPresets();
    if (on) toast("Hidden");
  } catch (e) {
    showError(e);
  }
}

async function presetUnhideAll() {
  try {
    await invoke("unhide_all_presets");
    await loadPresetHidden();
    presetHide.showing = false;
    renderPresets();
  } catch (e) {
    showError(e);
  }
}

function presetToggleShowHidden() {
  if (!presetHiddenCount() && !presetHide.showing) {
    toast("No presets are hidden");
    return;
  }
  presetHide.showing = !presetHide.showing;
  renderPresets();
}

// The tile menu's items for hiding and deleting.
function presetHideItems(p) {
  const items = [];
  if (presetHide.presets.has(p.path)) items.push(["Unhide", () => presetSetHidden(p, false)]);
  else if (!presetHide.groups.has(presetGroupOf(p))) items.push(["Hide", () => presetSetHidden(p, true)]);
  if (p.deletable) items.push(["Delete…", () => presetDelete(p)]);
  return items;
}

async function presetDelete(p) {
  if (!(await askYes(`Move “${p.name}” to the trash?`))) return;
  try {
    await invoke("delete_preset", { path: p.path });
    ui.presets = await invoke("presets");
    await loadPresetFavourites();
    await loadPresetHidden();
    renderPresets();
    toast("Moved to the trash");
  } catch (e) {
    showError(e);
  }
}

// "N hidden", Show / Done and Unhide all: at the foot of the list, only
// when something is hidden.
function presetHiddenFoot() {
  const n = presetHiddenCount();
  if (!n && !presetHide.groups.size) return null;
  return el("div", { class: "preset-hidden-foot" },
    el("span", { class: "hint" }, `${n} hidden`),
    el("button", { type: "button", class: "small", id: "preset-hidden-show", onclick: presetToggleShowHidden }, presetHide.showing ? "Done" : "Show"),
    el("button", { type: "button", class: "small", id: "preset-hidden-all", onclick: presetUnhideAll }, "Unhide all"));
}

// A group chip's right-click menu.
function presetGroupMenu(e, group) {
  const hidden = presetHide.groups.has(group);
  contextMenu(e, [hidden
    ? ["Unhide this group", () => presetSetGroupHidden(group, false)]
    : ["Hide this group", () => presetSetGroupHidden(group, true)]]);
}

function wirePresetHide() {
  loadPresetHidden();
}
