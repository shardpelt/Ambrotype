// SPDX-License-Identifier: GPL-3.0-or-later
// Preset library: free develop presets by style, from the hub (or the ones
// built into the app). Hover one to see it on the photo (presetHover in
// app.js), apply it, or add it to the presets list.
"use strict";

const plib = { items: [], group: "" };

async function openPresetLibrary() {
  const panel = $("preset-library");
  panel.hidden = false;
  $("presetlib-search").focus();
  try {
    plib.items = await invoke("preset_library");
  } catch (e) {
    plib.items = [];
    toast(`Couldn't load the preset library: ${e}`, true);
  }
  renderPresetLibrary();
}

function closePresetLibrary() {
  $("preset-library").hidden = true;
  presetHover(null);
}

function renderPresetLibrary() {
  const groups = [...new Set(plib.items.map((p) => p.group))];
  const chips = $("presetlib-groups");
  chips.replaceChildren(...["", ...groups].map((g) => {
    const b = Object.assign(document.createElement("button"), { type: "button", className: "small", textContent: g || "All" });
    b.classList.toggle("active", g === plib.group);
    b.addEventListener("click", () => { plib.group = g; renderPresetLibrary(); });
    return b;
  }));
  const q = $("presetlib-search").value.trim().toLowerCase();
  const shown = plib.items.filter((p) => (!plib.group || p.group === plib.group)
    && (!q || `${p.name} ${p.group} ${p.description}`.toLowerCase().includes(q)));
  const list = $("presetlib-list");
  if (!shown.length) {
    list.innerHTML = `<p class="hint">${plib.items.length ? "No match." : "No presets found."}</p>`;
    return;
  }
  list.replaceChildren(...shown.map((p) => {
    const row = Object.assign(document.createElement("div"), { className: "presetlib-item" });
    const text = Object.assign(document.createElement("div"), { className: "presetlib-text" });
    text.append(Object.assign(document.createElement("strong"), { textContent: p.name }),
      Object.assign(document.createElement("span"), { className: "hint", textContent: p.description || p.group }));
    const apply = Object.assign(document.createElement("button"), { type: "button", className: "small", textContent: "Apply" });
    apply.disabled = !ui.info;
    apply.addEventListener("click", () => {
      presetHover(null);
      run([{ cmd: "apply_preset", path: p.path, amount: Number($("preset-amount").value) }], `Preset: ${p.name}`);
    });
    const add = Object.assign(document.createElement("button"), { type: "button", className: "small",
      textContent: p.installed ? "Added" : "Add", title: "Keep it in your presets list" });
    add.disabled = p.installed;
    add.addEventListener("click", async () => {
      try {
        ui.presets = await invoke("library_install_preset", { id: p.id, path: p.path });
        p.installed = true;
        renderPresets();
        renderPresetLibrary();
        status(`Added "${p.name}" to your presets.`);
      } catch (e) {
        toast(String(e), true);
      }
    });
    row.append(text, apply, add);
    row.addEventListener("mouseenter", () => presetHover(p));
    return row;
  }));
}

function wirePresetLibrary() {
  $("btn-preset-library").addEventListener("click", openPresetLibrary);
  $("presetlib-close").addEventListener("click", closePresetLibrary);
  $("presetlib-search").addEventListener("input", renderPresetLibrary);
  $("presetlib-list").addEventListener("mouseleave", () => presetHover(null));
  $("preset-library").addEventListener("keydown", (e) => {
    if (e.key === "Escape") closePresetLibrary();
  });
}
