// SPDX-License-Identifier: GPL-3.0-or-later
// Ambrotype editor UI. Plain JS, no build step: talks to the Rust side via
// window.__TAURI__.core.invoke. Every edit is a amb-protocol command, so the
// GUI behaves exactly like the CLI and AI clients.
"use strict";

// ---------------------------------------------------------------- helpers

function setStatusBar(show) {
  document.body.classList.toggle("status-off", !show);
  $("settings-statusbar").checked = show;
  store.setText("amb.statusbar", show ? "1" : "0");
  if (ui.info) requestPreview(false);
}

function wireStatusBar() {
  let show = false;
  show = store.text("amb.statusbar") === "1";
  setStatusBar(show);
  $("settings-statusbar").addEventListener("change", (e) => setStatusBar(e.target.checked));
}

function decimals(def) { return def.step < 1 ? 2 : 0; }

function format(def, v) {
  if (typeof v !== "number" || Number.isNaN(v)) return "";
  const s = v.toFixed(decimals(def));
  return def.min < 0 && v > 0 ? "+" + s : s;
}

// Temperature uses a log scale on the slider, like Lightroom's.
const isLog = (def) => def.name === "Temperature";
function toSlider(def, v) {
  if (!isLog(def)) return v;
  const t = (Math.log(v) - Math.log(def.min)) / (Math.log(def.max) - Math.log(def.min));
  return Math.round(t * 1000);
}
function fromSlider(def, s) {
  if (!isLog(def)) return Number(s);
  const t = Number(s) / 1000;
  return Math.round(Math.exp(Math.log(def.min) + t * (Math.log(def.max) - Math.log(def.min))) / 50) * 50;
}

function currentValue(name) {
  if (name in ui.overrides) return ui.overrides[name];
  if (name in ui.settings) return ui.settings[name];
  if (name === "CameraProfile" || name === "amb:LUT" || name === "amb:Effects") return "";
  const def = ui.controls.get(name)?.def;
  if (ui.info?.defaults && name in ui.info.defaults) return ui.info.defaults[name];
  if (name === "Temperature") return ui.info ? Math.round(ui.info.as_shot_temperature) : 5500;
  if (name === "Tint") return ui.info ? Math.round(ui.info.as_shot_tint) : 0;
  if (name === "WhiteBalance") return "As Shot";
  return def?.default ?? (def?.kind === "bool" ? false : 0);
}

function asBool(v) {
  return v === true || v === "True" || v === "true" || v === 1 || v === "1";
}

// ---------------------------------------------------------------- state & commands

function setState(state) {
  const crop = cropKey(ui.settings);
  const prev = ui.state;
  const shown = teachShown();
  ui.state = state;
  ui.settings = state.settings ?? {};
  const held = ui.active && ui.active in ui.overrides ? { [ui.active]: ui.overrides[ui.active] } : {};
  ui.overrides = held;
  stripUpdateCurrent({ edited: Object.keys(ui.settings).length > 0 });
  syncControls();
  if (ui.crop.on && cropKey(ui.settings) !== crop) reloadCrop();
  masksStateChanged();
  spotsStateChanged();
  learnStateChanged();
  teachStateChanged(prev, shown);
}

// Another app or an AI agent (over MCP) wrote the open photo's sidecar:
// take its edits as one undo step, and its rating and keywords.
async function sidecarChanged() {
  if (!ui.info) return;
  try {
    const r = await invoke("reload_sidecar");
    if (!r) return;
    setState(r.state);
    requestPreview(false);
    loadInfo();
    if (r.changes) toast("The edits were changed outside Ambrotype and are shown now (Undo goes back).");
  } catch (e) {
    toast(`Couldn't read the changed sidecar: ${e}`, true);
  }
}

// ---------------------------------------------------------------- the selected slider

// Lightroom's , and . select the previous or next slider on screen, and
// + and - move the selected one (here = and -, Shift for ten steps).
function slidersOnScreen() {
  const out = [];
  for (const [name, c] of ui.controls) {
    const kind = c.def?.kind;
    if (kind === "bool" || kind === "choice" || !c.label?.isConnected || c.label.offsetParent === null) continue;
    out.push([name, c]);
  }
  return out.sort(([, a], [, b]) => (a.label.compareDocumentPosition(b.label) & Node.DOCUMENT_POSITION_FOLLOWING ? -1 : 1));
}

function showSelectedSlider() {
  for (const n of document.querySelectorAll(".selected-setting")) n.classList.remove("selected-setting");
  const c = ui.controls.get(ui.selectedSlider);
  const row = c?.label?.closest(".row, .knob, .bar-row") ?? c?.label;
  if (!row) return;
  row.classList.add("selected-setting");
  row.scrollIntoView({ block: "nearest" });
}

function stepSlider(dir) {
  const list = slidersOnScreen();
  if (!list.length) return;
  const at = list.findIndex(([name]) => name === ui.selectedSlider);
  const next = at < 0 ? (dir > 0 ? 0 : list.length - 1) : (at + dir + list.length) % list.length;
  ui.selectedSlider = list[next][0];
  showSelectedSlider();
  status(`${list[next][1].def.label} selected: = and - change it`);
}

function nudgeSlider(steps) {
  const c = ui.controls.get(ui.selectedSlider);
  if (!c?.label?.isConnected) return stepSlider(1);
  const def = c.def;
  const step = Number(def.step) > 0 ? Number(def.step) : 1;
  const now = Number(currentValue(def.name)) || 0;
  const v = Math.min(def.max ?? Infinity, Math.max(def.min ?? -Infinity, now + steps * step));
  commit(def.name, Number(v.toFixed(4)));
  showSelectedSlider();
}

// The protocol's notes are written for scripts and AI clients, with every
// setting named. People get a few words, and only when something didn't
// happen: values out of range, or nothing changing at all. (Settings not
// supported yet are kept in the sidecar for Lightroom; nothing to do.)
function notesForPeople(notes) {
  const out = [];
  for (const n of notes) {
    let m;
    if ((m = n.match(/^skipped (\d+) invalid preset value/))) out.push(`${m[1]} values out of range, skipped`);
    else if (n === "no change") out.push("No change");
  }
  return out.join(" · ");
}

// Run protocol commands; several commands with a `group` label are one undo step.
// `quiet`: no note on screen afterwards.
async function run(commands, group = null, { quiet = false } = {}) {
  if (!ui.info) return;
  try {
    const r = await invoke("execute", { commands, group });
    setState(r.state);
    const said = !quiet && notesForPeople(r.notes);
    if (said) toast(said);
    // The full notes (written for scripts and AI clients) stay at hand.
    $("status").title = r.notes.join("\n");
    requestPreview(false);
  } catch (e) {
    toast(String(e), true);
    ui.overrides = {};
    syncControls();
  }
}

// Picking an Upright mode clears the manual Transform sliders and the crop,
// as Lightroom does; Alt held keeps the crop.
const TRANSFORM_SLIDERS = ["PerspectiveVertical", "PerspectiveHorizontal", "PerspectiveRotate",
  "PerspectiveAspect", "PerspectiveScale", "PerspectiveX", "PerspectiveY"];
// Read from the click that opens the menu: the native menu takes the focus,
// so the page may not see Alt being released.
let altHeld = false;
for (const type of ["keydown", "keyup", "pointerdown"]) window.addEventListener(type, (e) => { altHeld = e.altKey; }, true);

function commit(name, value) {
  delete ui.overrides[name];
  if (name === "WhiteBalance" && value === "Auto") return autoFlow(false, true);
  // The default profile is no profile at all.
  if (name === "CameraProfile" && value === "") return run([{ cmd: "reset_param", name }]);
  if (name === "amb:LUT") return commitLut(value);
  if (name === "PerspectiveUpright" && value !== 0) {
    const cleared = TRANSFORM_SLIDERS.concat(altHeld ? [] : CROP_TAGS).filter((n) => n in ui.settings);
    if (cleared.length) {
      return run([{ cmd: "set_param", name, value }, ...cleared.map((n) => ({ cmd: "reset_param", name: n }))], "Upright");
    }
  }
  run([{ cmd: "set_param", name, value }]);
}

// The Profile menu: our default look, then the camera profiles (.dcp) found
// on this computer for the photo's camera, and the edit's own if it's
// missing here.
async function refreshProfiles(tries = 30) {
  let found = [];
  if (ui.info) {
    try {
      found = await invoke("camera_profiles");
    } catch {
      found = [];
    }
  }
  // The profile folders are read in the background at start-up.
  if (found === null) {
    if (tries > 0) setTimeout(() => refreshProfiles(tries - 1), 1000);
    found = [];
  }
  ui.profiles = found;
  syncControls();
}

// The LUT menu: none, the .cube files in the LUT folder, the edit's own if
// it's elsewhere, then choosing a file or opening the folder.
const LUT_PICK = "\u0000pick";
const LUT_FOLDER = "\u0000folder";
async function refreshLuts() {
  try {
    ui.luts = await invoke("luts_list");
  } catch {
    ui.luts = [];
  }
  syncControls();
}

function fillLutOptions(select, current) {
  const luts = ui.luts ?? [];
  const options = [["", "None"], ...luts.map((l) => [l.path, l.name])];
  if (current && !luts.some((l) => l.path === current)) options.push([current, fileName(String(current)).replace(/\.cube$/i, "")]);
  options.push([LUT_PICK, "Choose a .cube file…"], [LUT_FOLDER, "Open the LUT folder…"]);
  const want = options.map(([v, t]) => `${v}\u0001${t}`).join("\n");
  if (select.dataset.options !== want) {
    select.replaceChildren(...options.map(([v, t]) => Object.assign(document.createElement("option"), { value: JSON.stringify(v), textContent: t })));
    select.dataset.options = want;
  }
}

async function commitLut(value) {
  if (value === LUT_FOLDER) {
    syncControls();
    await invoke("open_luts_folder").catch((e) => toast(String(e), true));
    return;
  }
  if (value === LUT_PICK) {
    const path = await invoke("pick_lut");
    syncControls();
    if (!path) return;
    value = path;
  }
  if (!value) return run([{ cmd: "reset_param", name: "amb:LUT" }]);
  run([{ cmd: "set_param", name: "amb:LUT", value }]);
}

function fillProfileOptions(select, current) {
  const names = [...(ui.profiles ?? [])];
  const options = [["", "Default (Ambrotype)"], ...names.map((n) => [n, n])];
  if (current && !names.some((n) => n.toLowerCase() === String(current).toLowerCase())) options.push([current, `${current} (not found)`]);
  const want = options.map(([v, t]) => `${v}\u0000${t}`).join("\n");
  if (select.dataset.options !== want) {
    select.replaceChildren(...options.map(([v, t]) => Object.assign(document.createElement("option"), { value: JSON.stringify(v), textContent: t })));
    select.dataset.options = want;
  }
}

// ---------------------------------------------------------------- feedback

async function feedbackFlow() {
  const dlg = $("feedback-dialog");
  const f = $("feedback-form");
  const refresh = async () => {
    f.preview.value = await invoke("feedback_body", {
      kind: f.kind.value, text: f.text.value, includeContext: f.context.checked,
    });
  };
  f.oninput = refresh;
  f.onchange = refresh;
  await refresh();
  dlg.showModal();
  const choice = await new Promise((resolve) => dlg.addEventListener("close", () => resolve(dlg.returnValue), { once: true }));
  const title = f.title.value || f.kind.value;
  if (choice === "copy") return copyText(`[${f.kind.value}] ${title}\n\n${f.preview.value}`, "The report");
  if (choice !== "ok" && choice !== "email") return;
  const email = choice === "email";
  try {
    await invoke("open_feedback", { kind: f.kind.value, title, body: f.preview.value, email });
    toast(email ? "Opened your mail app with the report. Thank you!" : "Opened GitHub in your browser. Thank you!");
  } catch (e) {
    toast(String(e), true);
  }
}

// ---------------------------------------------------------------- wiring

// A button whose handler fails says so, rather than seeming to do nothing.
window.addEventListener("unhandledrejection", (e) => {
  console.error(e.reason);
  toast(`Something went wrong: ${e.reason?.message ?? e.reason}`, true);
});
window.addEventListener("error", (e) => {
  toast(`Something went wrong: ${e.message}`, true);
});

async function init() {
  applyToolbarIcons();
  ui.panels = await invoke("editor_schema");
  for (const p of ui.panels) for (const s of p.sections) for (const def of s.controls) ui.defs.set(def.name, def);
  // A label several settings share ("Amount") gets its section in History.
  const seen = new Map();
  for (const p of ui.panels) for (const s of p.sections) for (const def of s.controls) seen.set(def.label, (seen.get(def.label) ?? 0) + 1);
  for (const p of ui.panels) for (const s of p.sections) for (const def of s.controls) {
    if (seen.get(def.label) > 1 && s.label) ui.historyNames.set(def.name, `${s.label}: ${def.label}`);
  }
  await loadLooks();
  // Task plugins bring their tabs and layouts.
  await loadTaskPlugins();
  applyActiveLooks();
  await loadKeybindings();

  $("btn-open").addEventListener("click", openDialog);
  $("btn-library").addEventListener("click", () => libraryToggle());
  $("btn-open-empty").addEventListener("click", openDialog);
  $("btn-open-folder-empty").addEventListener("click", () => openFolder());
  $("btn-sample-empty").addEventListener("click", () => sampleFlow());
  $("btn-watch-empty").addEventListener("click", () => watchFlow());
  wireHistoryKeys();
  wireDockGrips();
  wireHelpCards();
  wireTerms();
  wireZones();
  wireBoxZoom();
  wireLoupe();
  wirePeek();
  initLearn();
  wireKeyBadges();
  $("btn-save").addEventListener("click", save);
  // Not the click event as an argument: exportFlow(batch) would take it for a batch.
  $("btn-export").addEventListener("click", () => exportFlow());
  $("btn-undo").addEventListener("click", () => run([{ cmd: "undo" }]));
  $("btn-undo-all").addEventListener("click", undoAll);
  $("snapshot-add").addEventListener("click", addSnapshot);
  $("btn-redo").addEventListener("click", () => run([{ cmd: "redo" }]));
  $("btn-before").addEventListener("click", toggleBefore);
  $("btn-zoom").addEventListener("click", () => setZoom(!ui.zoom.on, ui.zoom.cx, ui.zoom.cy));
  $("btn-wb").addEventListener("click", () => setPicking(ui.picking === "white" ? false : "white"));
  $("btn-clip").addEventListener("click", toggleClipping);
  $("proof-space").addEventListener("change", applyProof);
  $("proof-warn").addEventListener("change", applyProof);
  $("btn-feedback").addEventListener("click", feedbackFlow);
  $("btn-collage").addEventListener("click", collageFlow);
  $("btn-hub").addEventListener("click", () => browseFlow().catch((e) => toast(String(e), true)));
  $("btn-plugins").addEventListener("click", () => pluginsFlow().catch((e) => toast(String(e), true)));
  $("btn-looks").addEventListener("click", () => looksFlow().catch((e) => toast(String(e), true)));
  wireEditor();
  wireBorders();
  AmbPicker.enhance($("crop-aspect"), { open: ["Common ratios"] });
  AmbPicker.enhance($("export-form").size_preset, { open: ["Common"] });
  wireInfo();
  wireFilmstrip();
  for (const n of ["resize_mode", "resize_value"]) $("export-form")[n].addEventListener("input", drawBorderPreview);
  wireExportPresets();
  wireFeatures();
  wireTips();
  wireWatermark();
  $("btn-settings").addEventListener("click", () => settingsFlow().catch((e) => toast(String(e), true)));
  wireViewer();
  wireCrop();
  wireMasks();
  wireSpots();
  wireSelect();
  wireSplit();
  wireTouch();
  wireGuided();
  $("btn-add-presets").addEventListener("click", () => loadPresets(true));
  wirePresetLibrary();
  wireFrame();
  wireQuickTips();
  $("fold-left").addEventListener("click", () => toggleColumn("left"));
  const panels = new MutationObserver(() => { applySideCollapse(); if (ui.info) requestPreview(false); });
  document.querySelectorAll(".viewer > .preset-library").forEach((p) => panels.observe(p, { attributes: true, attributeFilter: ["hidden"] }));
  $("fold-right").addEventListener("click", () => toggleColumn("right"));
  $("btn-keys").addEventListener("click", showShortcuts);
  $("btn-search").addEventListener("click", searchFlow);
  $("btn-auto").addEventListener("click", () => autoEditFlow());
  $("btn-select").addEventListener("click", () => toggleSelect());
  wireToolBlocks();
  $("btn-save-preset").addEventListener("click", () => savePresetFlow());
  $("btn-match").addEventListener("click", () => matchFlow().catch((e) => toast(String(e), true)));
  $("btn-assistant").addEventListener("click", () => assistantFlow().catch((e) => toast(String(e), true)));
  $("btn-surprise").addEventListener("click", () => surpriseFlow().catch((e) => toast(String(e), true)));
  // Enter in a dialog's text box means its main button, not the first one (Cancel).
  for (const form of document.querySelectorAll('dialog form[method="dialog"]')) {
    form.addEventListener("keydown", (e) => {
      const ok = form.querySelector("button.primary");
      if (e.key !== "Enter" || !ok || e.target.tagName !== "INPUT" || ["checkbox", "radio"].includes(e.target.type)) return;
      e.preventDefault();
      form.requestSubmit(ok);
    });
  }
  $("preset-filter").addEventListener("input", renderPresets);
  $("preset-amount").addEventListener("input", (e) => ($("preset-amount-out").textContent = e.target.value));
  // The previews show the preset at this amount.
  $("preset-amount").addEventListener("change", () => renderPresets());
  const f = $("export-form");
  f.format.addEventListener("change", updateExportForm);
  f.quality.addEventListener("input", () => (f.quality_out.value = f.quality.value));
  f.size_preset.addEventListener("change", pickExportSize);
  for (const input of [f.exact_w, f.exact_h]) {
    input.addEventListener("input", () => {
      // Typed sizes are a custom size unless they still match the preset.
      const p = ui.sizes.find((x) => `size:${x.id}` === f.size_preset.value);
      if (!p || p.width !== Number(f.exact_w.value) || p.height !== Number(f.exact_h.value)) {
        f.size_preset.value = f.exact_w.value || f.exact_h.value ? "custom" : "";
      }
      updateExportSize();
    });
  }
  $("export-swap").addEventListener("click", () => {
    [f.exact_w.value, f.exact_h.value] = [f.exact_h.value, f.exact_w.value];
    f.size_preset.value = f.exact_w.value || f.exact_h.value ? "custom" : "";
    updateExportSize();
  });

  // Keyboard shortcuts come from one table (keys.js), which the user can change in Settings.
  document.addEventListener("keydown", dispatchKey);

  let resizeTimer;
  window.addEventListener("resize", () => {
    clearTimeout(resizeTimer);
    resizeTimer = setTimeout(() => requestPreview(false), 250);
  });

  wireMenu();
  wireFolderTree();
  wireLibraryMenus();
  wireSearch();
  wirePresetOriginal();
  wireTasks();
  wireInterfaces();
  wireConnect();
  wireDenoise();
  wireMonitor();
  wireTopbar();
  wireStatusBar();
  wireChrome();
  wireSlideshow();
  // Drag and drop a raw file onto the window.
  tauri.event.listen("folder-changed", () => folderChanged());
  tauri.event.listen("sidecar-changed", () => sidecarChanged());
  // A folder (no file extension) opens in the library instead.
  const dropZone = () => $(libraryOn() ? "library-empty" : "viewer");
  tauri.event.listen("tauri://drag-enter", () => dropZone().classList.add("dragover"));
  tauri.event.listen("tauri://drag-leave", () => dropZone().classList.remove("dragover"));
  tauri.event.listen("tauri://drag-drop", async (e) => {
    dropZone().classList.remove("dragover");
    const path = e.payload?.paths?.[0];
    if (!path) return;
    if (/\.[a-z0-9]{2,12}$/i.test(path)) return openAny(path);
    openFolder(path);
  });
  loadPresets();
  await loadSizes();
  // A photo given on the command line.
  const initial = await invoke("take_initial_path");
  if (initial) openAny(initial);
  // `amb slideshow` (slideshow.rs).
  const show = await invoke("take_slideshow");
  if (show?.photos.length) {
    const { photos, ...settings } = show;
    startSlideshow(photos, settings, { remember: false, done: (p) => openFolder(dirOf(p.path)) });
  }
  // `ambrotype --ui-test SCRIPT` (uitest.js).
  const test = await invoke("ui_test");
  if (test) runUiTest(test);
}

init().catch((e) => toast(String(e), true));
