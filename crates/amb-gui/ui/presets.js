// SPDX-License-Identifier: GPL-3.0-or-later
// The Presets panel (favourites, hover preview, right-click menu), saving a
// preset, Auto, Surprise me and Match look.
"use strict";

// One choice for every preset: on top of the edit (they chain, as in
// Lightroom), or from the original. "The original" resets the look groups
// first and keeps what belongs to this photo: crop, spots, masks, geometry
// and a film negative's conversion. Kept on this computer.
function presetsFromOriginal() {
  return $("preset-from-original").checked;
}

// The commands that apply a preset, by that choice; one undo step.
async function presetCommands(path, extra = {}) {
  const apply = { cmd: "apply_preset", path, amount: Number($("preset-amount").value), ...extra };
  if (!presetsFromOriginal()) return [apply];
  if (!ui.pasteGroups) ui.pasteGroups = await invoke("paste_groups");
  const resets = ui.pasteGroups.filter((g) => g.default && g.name !== "negative").map((g) => ({ cmd: "reset_group", group: g.name }));
  return [...resets, apply];
}

async function applyPreset(p) {
  run(await presetCommands(p.path), `Preset: ${p.name}`);
}

function wirePresetOriginal() {
  const box = $("preset-from-original");
  try { box.checked = localStorage.getItem("amb.presetsFromOriginal") === "1"; } catch { /* default: on top */ }
  box.addEventListener("change", () => {
    try { localStorage.setItem("amb.presetsFromOriginal", box.checked ? "1" : ""); } catch { /* not kept */ }
  });
}

function renderPresets() {
  const list = $("preset-list");
  const filter = $("preset-filter").value.trim().toLowerCase();
  const shown = ui.presets.filter((p) => !filter || (p.name + " " + p.group).toLowerCase().includes(filter));
  list.innerHTML = "";
  if (!shown.length) {
    list.innerHTML = `<p class="hint">${ui.presets.length ? "No match." : "No presets yet. Lightroom's preset folder is used automatically when it exists; or add a folder."}</p>`;
    return;
  }
  // Starred presets first, in their own group (they stay in theirs too).
  const favs = ui.presetFavourites ?? new Set();
  const groups = new Map();
  const starred = shown.filter((p) => favs.has(p.path));
  if (starred.length) groups.set("★ Favourites", starred);
  for (const p of shown) {
    if (!groups.has(p.group)) groups.set(p.group, []);
    groups.get(p.group).push(p);
  }
  for (const [group, items] of groups) {
    const d = document.createElement("details");
    d.className = "preset-group";
    d.open = groups.size < 6 || Boolean(filter) || group === "★ Favourites";
    const s = document.createElement("summary");
    s.textContent = `${group || "Presets"} (${items.length})`;
    d.append(s);
    for (const p of items) {
      const row = document.createElement("div");
      row.className = "preset-row";
      const b = document.createElement("button");
      b.className = "preset";
      b.textContent = p.name;
      b.title = `${p.path}\nRight-click to apply it to the whole folder, or only some of its settings`;
      b.addEventListener("click", () => {
        presetHover(null);
        applyPreset(p);
      });
      b.addEventListener("contextmenu", (e) => {
        presetHover(null);
        const n = folderPhotos().length;
        contextMenu(e, [
          ["Apply", () => applyPreset(p)],
          ["Apply some of its settings…", () => applyPresetPartly(p)],
          ui.info && n > 1 ? [`Apply to all ${n} photos in the folder`, () => applyPresetToFolder(p)] : null,
        ].filter(Boolean));
      });
      b.addEventListener("mouseenter", () => presetHover(p));
      b.addEventListener("mouseleave", () => presetHover(null));
      const star = document.createElement("button");
      star.type = "button";
      star.className = "preset-star";
      star.classList.toggle("on", favs.has(p.path));
      star.textContent = favs.has(p.path) ? "★" : "☆";
      star.title = favs.has(p.path) ? "Remove from favourites" : "Add to favourites";
      star.addEventListener("click", () => togglePresetFavourite(p));
      row.append(b, star);
      d.append(row);
    }
    list.append(d);
  }
}

async function loadPresetFavourites() {
  try {
    ui.presetFavourites = new Set(await invoke("preset_favourites"));
  } catch {
    ui.presetFavourites = new Set();
  }
}

async function togglePresetFavourite(p) {
  const on = !ui.presetFavourites?.has(p.path);
  try {
    await invoke("set_preset_favourite", { path: p.path, on });
    await loadPresetFavourites();
    renderPresets();
  } catch (e) {
    toast(String(e), true);
  }
}

async function applyPresetToFolder(p) {
  const paths = folderPhotos();
  if (!paths.length) return;
  status(`Applying "${p.name}" to ${paths.length} photos…`);
  const r = await invoke("apply_to_photos", {
    paths, label: `preset ${p.name} on the folder`,
    commands: await presetCommands(p.path),
  });
  if (r.state) setState(r.state);
  requestPreview(false);
  loadFolder(ui.info.path, true);
  toast(`"${p.name}" applied to ${r.changed.length} photo${r.changed.length === 1 ? "" : "s"}${r.errors.length ? `; ${r.errors.length} failed: ${r.errors[0]}` : ""}`, r.errors.length > 0);
}

// Apply a preset's settings from some groups only (Lightroom's "Apply
// preset settings: …"); the other groups stay as they are.
async function applyPresetPartly(p) {
  if (!ui.info) return;
  if (!ui.pasteGroups) ui.pasteGroups = await invoke("paste_groups");
  const dlg = $("paste-dialog");
  dlg.querySelector("h2").textContent = `Apply “${p.name}”`;
  dlg.querySelector("button.primary").textContent = "Apply";
  $("paste-what").textContent = "Only the ticked groups' settings come from the preset; the rest of the edit stays as it is.";
  const list = $("paste-groups");
  list.replaceChildren(...ui.pasteGroups.map((g) => {
    const row = Object.assign(document.createElement("label"), { className: "inline-check" });
    const box = Object.assign(document.createElement("input"), { type: "checkbox", value: g.name, checked: ui.presetGroups?.has(g.name) ?? true });
    row.append(box, Object.assign(document.createElement("span"), { textContent: g.label }));
    return row;
  }));
  dlg.showModal();
  const choice = await new Promise((resolve) => dlg.addEventListener("close", () => resolve(dlg.returnValue), { once: true }));
  dlg.querySelector("h2").textContent = "Paste settings";
  dlg.querySelector("button.primary").textContent = "Paste";
  if (choice !== "ok") return;
  const groups = [...list.querySelectorAll("input:checked")].map((b) => b.value);
  ui.presetGroups = new Set(groups);
  if (!groups.length) return;
  run([{ cmd: "apply_preset", path: p.path, amount: Number($("preset-amount").value), include_groups: groups }], `Preset (part): ${p.name}`);
}

// Hovering a preset shows the photo with it, as Lightroom's Navigator does;
// nothing is applied until it's clicked.
let hoverPreset = { timer: 0, shown: false, token: 0 };
function presetHover(p) {
  clearTimeout(hoverPreset.timer);
  const token = ++hoverPreset.token;
  if (!p) {
    if (hoverPreset.shown) {
      hoverPreset.shown = false;
      ui.overrides = {};
      requestPreview(false);
    }
    return;
  }
  if (!ui.info || ui.active || ui.crop.on) return;
  hoverPreset.timer = setTimeout(async () => {
    try {
      const overrides = await invoke("trial_overrides", { commands: await presetCommands(p.path) });
      if (token !== hoverPreset.token) return;
      ui.overrides = overrides;
      hoverPreset.shown = true;
      requestPreview(false);
    } catch {
      // A preset that can't be read just isn't previewed.
    }
  }, 180);
}

// The open photo's edit as a one-click preset (src/session.rs `save_preset`).
async function savePresetFlow(name = "") {
  if (!ui.info) return toast("Open a photo first.", true);
  const dlg = $("save-preset-dialog");
  const f = dlg.querySelector("form");
  f.preset_name.value = name;
  dlg.showModal();
  f.preset_name.focus();
  const choice = await new Promise((resolve) => dlg.addEventListener("close", () => resolve(dlg.returnValue), { once: true }));
  if (choice !== "ok") return;
  try {
    ui.presets = await invoke("save_preset", { name: f.preset_name.value, group: f.preset_group.value });
    renderPresets();
    toast(`Saved the preset “${f.preset_name.value.trim()}” (in ${f.preset_group.value.trim() || "Yours"})`);
  } catch (e) {
    toast(String(e), true);
  }
}

// Auto, as Lightroom's: the tone sliders (Basic's Auto button) or the
// white balance (WB: Auto), worked out on a small copy, one undo step.
async function autoFlow(tone, white) {
  if (!ui.info) return;
  status(tone ? "Auto tone…" : "Auto white balance…");
  $("busy").hidden = false;
  try {
    const commands = await invoke("auto_settings", { tone, white });
    if (white && !commands.some((c) => c.name === "Temperature" || c.name === "IncrementalTemperature")) {
      // Too little grey to judge the light by: keep the camera's.
      commands.push({ cmd: "set_param", name: "WhiteBalance", value: "Auto" });
      if (!tone) toast("Too little grey in this photo to judge the light by, so Auto keeps the camera's white balance.");
    }
    await run(commands, tone ? "Auto" : "Auto white balance");
  } catch (e) {
    toast(String(e), true);
  } finally {
    $("busy").hidden = true;
    status("");
  }
}

// Surprise me: six random looks from the Random look plugin as
// thumbnails; clicking one applies it (one undo step). Seeds are shown so
// a look can be found again.
async function surpriseFlow() {
  if (!ui.info) return toast("Open a photo first.", true);
  const dlg = $("surprise-dialog");
  const grid = $("surprise-grid");
  let seed = Math.floor(Math.random() * 1e9);
  let busy = false;
  const roll = async () => {
    if (busy) return;
    busy = true;
    $("surprise-more").disabled = true;
    $("surprise-status").textContent = "Making looks…";
    try {
      const variants = await invoke("surprise", {
        seed,
        count: 6,
        params: {
          style: $("surprise-style").value,
          strength: Number($("surprise-strength").value),
          colour: $("surprise-colour").checked,
        },
      });
      seed += variants.length;
      grid.replaceChildren(...variants.map((v) => {
        const b = document.createElement("button");
        b.type = "button";
        b.title = "Apply this look";
        const img = document.createElement("img");
        img.src = v.image;
        img.alt = `Look ${v.seed}`;
        const label = document.createElement("span");
        label.textContent = `seed ${v.seed}`;
        b.append(img, label);
        b.addEventListener("click", async () => {
          dlg.close("cancel");
          await run(v.commands, `Surprise me (seed ${v.seed})`);
        });
        return b;
      }));
      $("surprise-status").textContent = "Click a look to apply it. Each is an ordinary edit you can keep tweaking, and Undo takes it back.";
    } catch (e) {
      $("surprise-status").textContent = String(e);
    } finally {
      busy = false;
      $("surprise-more").disabled = false;
    }
  };
  $("surprise-more").onclick = roll;
  for (const id of ["surprise-style", "surprise-colour"]) $(id).onchange = roll;
  $("surprise-strength").onchange = roll;
  grid.replaceChildren();
  dlg.showModal();
  roll();
}

// Match look: pick reference photos, find settings for their look, show
// them on the photo, and apply them as one undo step.
async function matchFlow() {
  if (!ui.info) return toast("Open a photo first.", true);
  const refs = await invoke("pick_references");
  if (!refs.length) return;
  const dlg = $("match-dialog");
  const names = refs.map(fileName);
  $("match-refs").textContent = `${refs.length} reference${refs.length > 1 ? "s" : ""}: ${names.join(", ")}`;
  const table = $("match-lines");
  table.hidden = true;
  table.replaceChildren();
  $("match-apply").disabled = true;
  $("match-find").disabled = false;
  $("match-status").textContent = "Measures how light, warm and colourful the references are, then finds the settings that give this photo the same. The photo shows the result before you apply it.";
  let proposal = null;
  const preview = (commands) => {
    ui.overrides = {};
    for (const c of commands) ui.overrides[c.name] = c.value;
    requestPreview(false);
  };
  const find = async () => {
    const parts = [...dlg.querySelectorAll(".match-parts input:checked")].map((i) => i.value);
    if (!parts.length) return toast("Choose at least one part to match.", true);
    $("match-find").disabled = true;
    $("match-apply").disabled = true;
    $("match-status").textContent = "Finding settings…";
    try {
      proposal = await invoke("match_look", { references: refs, parts });
      table.replaceChildren(...proposal.lines.map((l) => {
        const tr = document.createElement("tr");
        for (const t of [l.label, l.before, "→", l.after]) {
          const td = document.createElement("td");
          td.textContent = t;
          tr.append(td);
        }
        return tr;
      }));
      table.hidden = !proposal.lines.length;
      const closer = proposal.before > 0 ? Math.round(100 * (1 - proposal.after / proposal.before)) : 0;
      $("match-status").textContent = proposal.lines.length
        ? `${proposal.lines.length} setting${proposal.lines.length > 1 ? "s" : ""}; the look is ${closer}% closer (${proposal.renders} test renders).`
        : "The photo already matches: nothing to change.";
      $("match-apply").disabled = !proposal.commands.length;
      preview(proposal.commands);
    } catch (e) {
      $("match-status").textContent = String(e);
    } finally {
      $("match-find").disabled = false;
    }
  };
  $("match-find").onclick = find;
  dlg.showModal();
  find();
  const choice = await new Promise((resolve) => dlg.addEventListener("close", () => resolve(dlg.returnValue), { once: true }));
  ui.overrides = {};
  if (choice === "ok" && proposal) await run(proposal.commands, "Match look");
  else requestPreview(false);
}

async function loadPresets(add = false) {
  try {
    ui.presets = await invoke(add ? "add_preset_folder" : "presets");
    if (!ui.presetFavourites) await loadPresetFavourites();
    renderPresets();
  } catch (e) {
    toast(String(e), true);
  }
}
