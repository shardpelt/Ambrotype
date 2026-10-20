// SPDX-License-Identifier: GPL-3.0-or-later
// Keyboard shortcuts: one table of actions with their default keys, the
// user's changes (kept in the app's settings by src/keys.rs; only changes are
// stored, so new defaults still arrive), the key dispatcher, and the Settings
// dialog that lists them on a keyboard map and lets you change them.
//
// A key is written like "Ctrl+Shift+Z": Ctrl means Ctrl, or Cmd on a Mac.
"use strict";

const IS_MAC = /Mac|iPhone|iPad/.test(navigator.platform);

// `when`: "any", "main" (crop tool closed), "crop" (crop tool open),
// "masks", "spots", "library" (the library grid shown), or a list of them.
// Actions with overlapping contexts can't share a key. `photo`: needs an open photo.
// `button`: the toolbar button whose tooltip shows the keys.
// Colour labels, written to the sidecar as Lightroom does (`xmp:Label`).
const LABELS = ["Red", "Yellow", "Green", "Blue", "Purple"];
// Muted and even in brightness, so no label shouts; readable on dark and light themes.
const LABEL_COLOURS = { Red: "#cf6a5e", Yellow: "#d2b260", Green: "#73a17c", Blue: "#6189c2", Purple: "#9580c0" };

// Flag keys work everywhere but the crop tool, where X swaps the aspect.
const FLAG_CONTEXTS = ["main", "masks", "spots", "library"];

const KEY_ACTIONS = [
  { id: "open", group: "File", label: "Open a photo", keys: ["Ctrl+O"], when: "any", button: "btn-open", run: () => openDialog() },
  { id: "save", group: "File", label: "Save the edit to the sidecar", keys: ["Ctrl+S"], when: "any", button: "btn-save", run: () => save() },
  { id: "export", group: "File", label: "Export", keys: ["Ctrl+E"], when: "any", button: "btn-export", run: () => exportFlow() },
  { id: "edit-in", group: "File", label: "Edit in another app", hint: "The photo, as edited, goes to your image editor as a TIFF next to it (set the app in Settings → General).", keys: ["Ctrl+Alt+E"], when: "any", photo: true, run: () => editInFlow() },
  { id: "collage", group: "File", label: "Collage maker", hint: "Several photos on one canvas.", keys: ["C"], when: "main", button: "btn-collage", run: () => collageFlow() },
  { id: "undo", group: "Edit", label: "Undo", keys: ["Ctrl+Z"], when: "any", button: "btn-undo", run: () => run([{ cmd: "undo" }]) },
  { id: "redo", group: "Edit", label: "Redo", keys: ["Ctrl+Shift+Z", "Ctrl+Y"], when: "any", button: "btn-redo", run: () => run([{ cmd: "redo" }]) },
  { id: "search", group: "Help", label: "Search everything: commands, settings, panels, presets, layouts", keys: ["Ctrl+K"], when: "any", run: () => searchFlow() },
  { id: "learn", group: "Help", label: "Learn: short guided lessons", keys: ["F1"], when: "any", run: () => learnFlow() },
  { id: "before", group: "View", label: "Before / after", keys: ["\\"], when: "any", button: "btn-before", run: () => toggleBefore() },
  { id: "zoom", group: "View", label: "Fit / 100% view", hint: "Or click the photo; drag to pan at 100%.", keys: ["Z"], when: "main", photo: true, button: "btn-zoom", run: () => setZoom(!ui.zoom.on, ui.zoom.cx, ui.zoom.cy) },
  { id: "proof", group: "View", label: "Soft proofing", hint: "See the photo as it will export to sRGB, Adobe RGB, Display P3 or ProPhoto, and mark the colours that won't fit.", keys: ["S"], when: "main", photo: true, run: () => toggleProof() },
  { id: "frame", group: "View", label: "Frame: border and text around the photo", keys: ["Shift+B"], when: "main", photo: true, button: "btn-frame", run: () => toggleFrame() },
  { id: "shortcuts", group: "Help", label: "Show all keyboard shortcuts", keys: ["Shift+?"], when: "any", button: "btn-keys", run: () => showShortcuts() },
  { id: "clipping", group: "View", label: "Clipping overlay", hint: "Red: clipped highlights, blue: crushed shadows.", keys: ["J"], when: "any", button: "btn-clip", run: () => toggleClipping() },
  { id: "crop", group: "Tools", label: "Crop and straighten", keys: ["R"], when: "any", photo: true, button: "btn-crop", run: () => toggleCrop() },
  { id: "masks", group: "Tools", label: "Masks", hint: "Gradients, brush and range masks with their own sliders.", keys: ["M"], when: "any", photo: true, button: "btn-masks", run: () => toggleMasks() },
  { id: "spots", group: "Tools", label: "Spot removal", hint: "Heal or clone blemishes and distractions.", keys: ["Q"], when: "any", photo: true, button: "btn-spots", run: () => toggleSpots() },
  { id: "wb", group: "Tools", label: "White-balance picker", hint: "Then click something neutral grey or white.", keys: ["W"], when: "main", button: "btn-wb", run: () => setPicking(!ui.picking) },
  { id: "leave", group: "Tools", label: "Leave the picker", keys: ["Escape"], when: "main", run: () => setPicking(false) },
  { id: "crop-draw", group: "Crop tool", label: "Draw a new crop (corner to corner)", keys: ["D"], when: "crop", run: () => ui.crop.tool.setDrawMode(!ui.crop.tool.drawMode()) },
  { id: "crop-swap", group: "Crop tool", label: "Swap portrait and landscape", keys: ["X"], when: "crop", run: () => swapCrop() },
  { id: "crop-apply", group: "Crop tool", label: "Apply the crop", keys: ["Enter"], when: "crop", run: (e) => (e.target.matches("button, select") ? false : exitCrop(true)) },
  { id: "crop-cancel", group: "Crop tool", label: "Cancel the crop", keys: ["Escape"], when: "crop", run: () => exitCrop(false) },
  { id: "mask-linear", group: "Masks", label: "New linear gradient", keys: ["L"], when: "masks", run: () => startTool("linear", true) },
  { id: "mask-radial", group: "Masks", label: "New radial gradient", keys: ["Shift+R"], when: "masks", run: () => startTool("radial", true) },
  { id: "mask-brush", group: "Masks", label: "New brush mask", hint: "Hold Alt to erase.", keys: ["B"], when: "masks", run: () => startTool("brush", true) },
  { id: "mask-overlay", group: "Masks", label: "Show or hide the mask overlay", keys: ["O"], when: "masks", run: () => toggleMaskOverlay() },
  { id: "mask-smaller", group: "Masks", label: "Smaller brush", keys: ["["], when: "masks", run: () => brushSize(-1) },
  { id: "mask-bigger", group: "Masks", label: "Bigger brush", keys: ["]"], when: "masks", run: () => brushSize(1) },
  { id: "mask-delete", group: "Masks", label: "Delete the selected part (or mask)", keys: ["Delete", "Backspace"], when: "masks", run: (e) => (onControl(e) ? false : deleteMaskPart()) },
  { id: "spot-smaller", group: "Spot removal", label: "Smaller spot", keys: ["["], when: "spots", run: () => spotSize(-1) },
  { id: "spot-bigger", group: "Spot removal", label: "Bigger spot", keys: ["]"], when: "spots", run: () => spotSize(1) },
  { id: "spot-delete", group: "Spot removal", label: "Delete the selected spot", keys: ["Delete", "Backspace"], when: "spots", run: (e) => (onControl(e) ? false : deleteSpot()) },
  { id: "spot-visualise", group: "Spot removal", label: "Visualise spots", keys: ["A"], when: "spots", run: () => setVisualise(!sp.visualise) },
  { id: "spot-leave", group: "Spot removal", label: "Leave spot removal", keys: ["Escape"], when: "spots", run: () => exitSpots() },
  { id: "mask-leave", group: "Masks", label: "Stop drawing, then leave the masks", keys: ["Escape"], when: "masks", run: () => maskEscape() },
  // Rating, labels and keywords: the open photo, or in the library the selected photos.
  ...[1, 2, 3, 4, 5].map((n) => ({ id: `rate-${n}`, group: "Rating", label: `Rate ${"★".repeat(n)}`, keys: [String(n)], when: "any", run: () => rateKey(n) })),
  { id: "rate-0", group: "Rating", label: "Clear the rating", keys: ["0"], when: "any", run: () => rateKey(0) },
  ...LABELS.slice(0, 4).map((l, i) => ({ id: `label-${l.toLowerCase()}`, group: "Rating", label: `${l} label (again to remove)`, keys: [String(6 + i)], when: "any", run: () => labelKey(l) })),
  { id: "keywords", group: "Rating", label: "Add keywords (jump to the keyword field)", keys: ["K"], when: "any", run: () => keywordsKey() },
  { id: "flag-pick", group: "Rating", label: "Flag as a pick", keys: ["P"], when: FLAG_CONTEXTS, run: () => flagKey("pick") },
  { id: "flag-reject", group: "Rating", label: "Flag as rejected", keys: ["X"], when: FLAG_CONTEXTS, run: () => flagKey("reject") },
  { id: "flag-none", group: "Rating", label: "Remove the flag", keys: ["U"], when: FLAG_CONTEXTS, run: () => flagKey("") },
  { id: "library", group: "Library", label: "Library grid / editor", hint: "All the photos in the folder, to rate, label, tag and export many at once.", keys: ["G"], when: "any", button: "btn-library", run: () => cullLeave() || toggleLibrary() },
  { id: "import", group: "Library", label: "Import photos from a card or folder", hint: "Copied into dated folders, renamed if you like.", keys: ["Ctrl+Shift+I"], when: "any", run: () => importFlow() },
  { id: "virtual-copy", group: "Library", label: "Make a virtual copy", hint: "A second edit of the photo, kept as its own sidecar.", keys: ["Ctrl+'"], when: ["main", "library"], run: () => virtualCopyKey() },
  { id: "quick-collection", group: "Library", label: "Add to the last collection used", hint: "The selection, or the open photo.", keys: ["B"], when: ["main", "library"], run: () => quickCollectionKey() },
  { id: "lib-loupe", group: "Library", label: "Loupe: one photo large", hint: "← → step through the photos.", keys: ["E"], when: "library", run: () => setLibraryView("loupe") },
  { id: "lib-compare", group: "Library", label: "Compare two photos", hint: "← → change the candidate, ↓ swaps, ↑ makes it the select.", keys: ["C"], when: "library", run: () => setLibraryView("compare") },
  { id: "lib-survey", group: "Library", label: "Survey the selection", hint: "Click to make a photo active; × takes it out.", keys: ["N"], when: "library", run: () => setLibraryView("survey") },
  { id: "lib-open", group: "Library", label: "Open the photo in the editor", keys: ["Enter"], when: "library", run: (e) => (e.target.matches("button, select") ? false : libraryOpenFocused()) },
  { id: "lib-all", group: "Library", label: "Select all shown", keys: ["Ctrl+A"], when: "library", run: () => librarySelectAll() },
  { id: "lib-none", group: "Library", label: "Select none", keys: ["Ctrl+D"], when: "library", run: () => librarySelectNone() },
  { id: "lib-stack", group: "Library", label: "Stack the selection", keys: ["Ctrl+G"], when: "library", run: () => stackSelection() },
  { id: "lib-unstack", group: "Library", label: "Unstack", keys: ["Ctrl+Shift+G"], when: "library", run: () => unstackSelection() },
  { id: "lib-copy", group: "Library", label: "Copy the selected photo's settings", keys: ["Ctrl+Shift+C"], when: "library", run: () => libraryCopySettings() },
  { id: "lib-paste", group: "Library", label: "Paste settings onto the selection", keys: ["Ctrl+Shift+V"], when: "library", run: () => libraryPasteSettings() },
  { id: "lib-leave", group: "Library", label: "Back to the grid, then the editor", keys: ["Escape"], when: "library", run: () => cullLeave() || toggleLibrary(false) },
  { id: "slider-next", group: "Sliders", label: "Select the next slider", hint: "As in Lightroom; the selected one is outlined.", keys: ["."], when: "main", photo: true, run: () => stepSlider(1) },
  { id: "slider-prev", group: "Sliders", label: "Select the previous slider", keys: [","], when: "main", photo: true, run: () => stepSlider(-1) },
  { id: "slider-up", group: "Sliders", label: "Raise the selected slider a step", keys: ["="], when: "main", photo: true, run: () => nudgeSlider(1) },
  { id: "slider-down", group: "Sliders", label: "Lower the selected slider a step", keys: ["-"], when: "main", photo: true, run: () => nudgeSlider(-1) },
  { id: "slider-up-more", group: "Sliders", label: "Raise the selected slider ten steps", keys: ["Shift++"], when: "main", photo: true, run: () => nudgeSlider(10) },
  { id: "slider-down-more", group: "Sliders", label: "Lower the selected slider ten steps", keys: ["Shift+_"], when: "main", photo: true, run: () => nudgeSlider(-10) },
  { id: "prev-photo", group: "Photos", label: "Previous photo in the folder", keys: ["ArrowLeft"], when: "main", photo: true, run: (e) => (onControl(e) ? false : ui.zoom.on ? panZoom(-0.25, 0) : stepPhoto(-1)) },
  { id: "next-photo", group: "Photos", label: "Next photo in the folder", keys: ["ArrowRight"], when: "main", photo: true, run: (e) => (onControl(e) ? false : ui.zoom.on ? panZoom(0.25, 0) : stepPhoto(1)) },
  { id: "pan-up", group: "View", label: "Move up the zoomed-in photo", keys: ["ArrowUp"], when: "main", photo: true, run: (e) => (onControl(e) || !ui.zoom.on ? false : panZoom(0, -0.25)) },
  { id: "pan-down", group: "View", label: "Move down the zoomed-in photo", keys: ["ArrowDown"], when: "main", photo: true, run: (e) => (onControl(e) || !ui.zoom.on ? false : panZoom(0, 0.25)) },
  { id: "fold-left", group: "Window", label: "Hide or show the left column", keys: ["Ctrl+["], when: "any", run: () => toggleColumn("left") },
  { id: "fold-right", group: "Window", label: "Hide or show the right column", keys: ["Ctrl+]"], when: "any", run: () => toggleColumn("right") },
  { id: "filmstrip", group: "Photos", label: "Show or hide the filmstrip", keys: ["F"], when: "any", run: () => toggleFilmstrip() },
  { id: "assistant", group: "Tools", label: "Assistant: your AI edits the photo", keys: ["Ctrl+Shift+A"], when: "any", photo: true, run: () => assistantFlow() },
  { id: "hub", group: "Tools", label: "Browse what others shared", hint: "Layouts, themes, presets, collages and plugins from the hub.", keys: ["Ctrl+Shift+H"], when: "any", button: "btn-hub", run: () => browseFlow() },
  { id: "plugins", group: "Tools", label: "Plugins", hint: "Extra tools that make edits; run, install and share them.", keys: ["Ctrl+Shift+P"], when: "any", button: "btn-plugins", run: () => pluginsFlow() },
  { id: "looks", group: "Window", label: "Appearance: layouts and themes", keys: ["Ctrl+L"], when: "any", button: "btn-looks", run: () => looksFlow() },
  { id: "edit-layout", group: "Window", label: "Edit layout", hint: "Move panels, controls and toolbar buttons.", keys: ["Ctrl+Shift+L"], when: "any", button: "btn-edit-layout", run: () => startEditing() },
  { id: "settings", group: "Window", label: "Settings and keyboard shortcuts", keys: ["Ctrl+,"], when: "any", button: "btn-settings", run: () => settingsFlow() },
  { id: "feedback", group: "Window", label: "Send feedback", hint: "A bug, an idea, or a photo that looks different from Lightroom.", keys: [], when: "any", button: "btn-feedback", run: () => feedbackFlow() },
];

// Arrow keys belong to a focused slider, list or knob.
const onControl = (e) => e.target.matches("input, select, textarea, [tabindex], .bar, svg");
const keysFor = (a) => ui.keybindings?.[a.id] ?? a.keys;
const contextsOf = (a) => [a.when].flat();
const contextsOverlap = (a, b) => a.when === "any" || b.when === "any" || contextsOf(a).some((w) => contextsOf(b).includes(w));

// The key combination of a keydown event, or null for a lone modifier.
function comboOf(e) {
  if (["Control", "Shift", "Alt", "Meta", "AltGraph", "CapsLock", "Dead", "Unidentified"].includes(e.key)) return null;
  let key = e.key === " " ? "Space" : e.key.length === 1 ? e.key.toUpperCase() : e.key;
  // Alt on a Mac types special characters (Alt+R is ®); use the key's place instead.
  if (e.altKey && /^Key[A-Z]$|^Digit\d$/.test(e.code) && !/^[A-Z0-9]$/.test(key)) key = e.code.replace(/^Key|^Digit/, "");
  const parts = [];
  if (e.ctrlKey || e.metaKey) parts.push("Ctrl");
  if (e.altKey) parts.push("Alt");
  if (e.shiftKey) parts.push("Shift");
  return [...parts, key].join("+");
}

const KEY_NAMES = { Escape: "Esc", ArrowUp: "↑", ArrowDown: "↓", ArrowLeft: "←", ArrowRight: "→", Delete: "Del", Backspace: "⌫", Enter: "Enter", Space: "Space" };

// How a key looks on screen: "Ctrl+Shift+Z", or "⌘⇧Z" on a Mac.
function keyLabel(combo) {
  const parts = combo.split("+");
  // "Ctrl++" is Ctrl and the plus key.
  const key = combo.endsWith("++") ? "+" : parts.pop();
  const mods = combo.endsWith("++") ? parts.filter((p) => p) : parts;
  const name = KEY_NAMES[key] ?? key;
  if (IS_MAC) {
    const sym = { Ctrl: "⌘", Alt: "⌥", Shift: "⇧" };
    return mods.map((m) => sym[m] ?? m).join("") + name;
  }
  return [...mods, name].join("+");
}

// Which actions' keys work now (an action's `when`).
const keyContext = () => (libraryOn() ? "library" : ui.crop.on ? "crop" : mk.on ? "masks" : sp.on ? "spots" : "main");

// When each action last ran from its key, so a menu bar that saw the key too
// doesn't run it again (menu.js).
const keyRunAt = {};

function dispatchKey(e) {
  if (e.target.matches("input[type=number], input[type=search], input[type=text], textarea, input.value")) return;
  // Keys belong to an open dialog.
  if (document.querySelector("dialog[open]")) return;
  const combo = comboOf(e);
  if (!combo) return;
  const ctx = keyContext();
  for (const a of KEY_ACTIONS) {
    if (!keysFor(a).includes(combo)) continue;
    if (a.when !== "any" && !contextsOf(a).includes(ctx)) continue;
    if (a.photo && !ui.info) continue;
    if (a.run(e) === false) continue;
    keyRunAt[a.id] = Date.now();
    e.preventDefault();
    return;
  }
}

// Tooltips on the toolbar buttons name their current keys.
function updateKeyHints() {
  for (const a of KEY_ACTIONS) {
    const b = a.button && $(a.button);
    if (!b) continue;
    const keys = keysFor(a).map(keyLabel).join(" or ");
    b.title = `${a.label}${keys ? ` (${keys})` : ""}${a.hint ? `\n${a.hint}` : ""}`;
  }
  buildMenu();
}

async function loadKeybindings() {
  try {
    ui.keybindings = await invoke("keybindings");
  } catch (e) {
    ui.keybindings = {};
    toast(`Couldn't load your keyboard shortcuts: ${e}`, true);
  }
  updateKeyHints();
}

async function saveKeybindings() {
  // Only changes are stored; an action back at its defaults is dropped.
  for (const a of KEY_ACTIONS) {
    const mine = ui.keybindings[a.id];
    if (mine && mine.length === a.keys.length && mine.every((k, i) => k === a.keys[i])) delete ui.keybindings[a.id];
  }
  updateKeyHints();
  try {
    await invoke("set_keybindings", { bindings: ui.keybindings });
  } catch (e) {
    toast(String(e), true);
  }
}

// ---------------------------------------------------------------- the Settings dialog

const KEYBOARD_ROWS = [
  ["Escape", "1", "2", "3", "4", "5", "6", "7", "8", "9", "0", "-", "="],
  ["Q", "W", "E", "R", "T", "Y", "U", "I", "O", "P", "[", "]", "\\"],
  ["A", "S", "D", "F", "G", "H", "J", "K", "L", ";", "'", "Enter"],
  ["Z", "X", "C", "V", "B", "N", "M", ",", ".", "/"],
  ["Space"],
];

async function settingsFlow() {
  const dlg = $("settings-dialog");
  let capturing = null; // the action waiting for a new key
  let pending = null; // a key that clashes, waiting for "use it here"
  let focusKey = null; // a key picked on the keyboard map

  const baseKey = (combo) => (combo.endsWith("++") ? "+" : combo.split("+").pop());
  const message = (text, actions = []) => {
    const box = $("keys-message");
    box.replaceChildren(document.createTextNode(text), ...actions);
  };

  function renderMap() {
    const map = $("keyboard-map");
    map.replaceChildren();
    for (const row of KEYBOARD_ROWS) {
      const r = document.createElement("div");
      r.className = "kb-row";
      for (const key of row) {
        const users = KEY_ACTIONS.flatMap((a) => keysFor(a).filter((k) => baseKey(k) === key).map((k) => [k, a]));
        const cap = document.createElement("button");
        cap.type = "button";
        cap.className = `kb-key kb-${key.length > 1 ? key.toLowerCase() : "char"}`;
        cap.textContent = KEY_NAMES[key] ?? key;
        cap.classList.toggle("bare", users.some(([k]) => k === key));
        cap.classList.toggle("with-mod", users.some(([k]) => k !== key));
        cap.classList.toggle("focus", focusKey === key);
        cap.title = users.length ? users.map(([k, a]) => `${keyLabel(k)}: ${a.label}`).join("\n") : `${KEY_NAMES[key] ?? key}: free`;
        cap.addEventListener("click", () => {
          focusKey = focusKey === key ? null : key;
          renderMap();
          renderList();
        });
        r.append(cap);
      }
      map.append(r);
    }
  }

  function renderList() {
    const filter = $("keys-filter").value.trim().toLowerCase();
    const list = $("keys-list");
    list.replaceChildren();
    let group = null;
    for (const a of KEY_ACTIONS) {
      const keys = keysFor(a);
      if (focusKey && !keys.some((k) => baseKey(k) === focusKey)) continue;
      if (filter && !`${a.label} ${a.group} ${keys.map(keyLabel).join(" ")}`.toLowerCase().includes(filter)) continue;
      if (a.group !== group) {
        group = a.group;
        list.append(Object.assign(document.createElement("h3"), { textContent: group }));
      }
      const row = document.createElement("div");
      row.className = "key-row";
      const name = document.createElement("span");
      name.className = "key-action";
      name.textContent = a.label;
      if (a.when === "crop") name.title = "Only while the crop tool is open";
      if (a.when === "masks") name.title = "Only while the masks are open";
      if (a.when === "spots") name.title = "Only while spot removal is open";
      if (a.when === "library") name.title = "Only in the library";
      const caps = document.createElement("span");
      caps.className = "key-caps";
      for (const k of keys) {
        const kbd = document.createElement("kbd");
        kbd.textContent = keyLabel(k);
        const x = Object.assign(document.createElement("button"), { type: "button", className: "kbd-remove", textContent: "×", title: `Remove ${keyLabel(k)}` });
        x.addEventListener("click", () => {
          ui.keybindings[a.id] = keys.filter((y) => y !== k);
          saveKeybindings();
          render();
        });
        kbd.append(x);
        caps.append(kbd);
      }
      if (capturing === a.id) {
        caps.append(Object.assign(document.createElement("span"), { className: "key-capture", textContent: "Press the new shortcut… (Esc cancels)" }));
      } else if (!keys.length) {
        caps.append(Object.assign(document.createElement("span"), { className: "hint", textContent: "none" }));
      }
      const add = Object.assign(document.createElement("button"), { type: "button", className: "small", textContent: "Add key", disabled: keys.length >= 4 });
      add.addEventListener("click", () => {
        capturing = a.id;
        pending = null;
        message("");
        render();
      });
      const changed = Boolean(ui.keybindings[a.id]);
      const reset = Object.assign(document.createElement("button"), { type: "button", className: "small", textContent: "Reset", disabled: !changed, title: `Back to ${a.keys.map(keyLabel).join(" or ") || "no key"}` });
      reset.addEventListener("click", () => {
        delete ui.keybindings[a.id];
        saveKeybindings();
        render();
      });
      row.classList.toggle("changed", changed);
      row.classList.toggle("capturing", capturing === a.id);
      row.append(name, caps, add, reset);
      list.append(row);
    }
    if (!list.children.length) list.append(Object.assign(document.createElement("p"), { className: "hint", textContent: "No shortcuts match." }));
  }

  function render() {
    renderMap();
    renderList();
  }

  function assign(action, combo) {
    const clashes = KEY_ACTIONS.filter((b) => b !== action && contextsOverlap(action, b) && keysFor(b).includes(combo));
    const add = () => {
      for (const b of clashes) ui.keybindings[b.id] = keysFor(b).filter((k) => k !== combo);
      if (!keysFor(action).includes(combo)) ui.keybindings[action.id] = [...keysFor(action), combo];
      capturing = pending = null;
      message(`${keyLabel(combo)} now does "${action.label}".`);
      saveKeybindings();
      render();
    };
    if (!clashes.length) return add();
    pending = combo;
    capturing = null;
    const use = Object.assign(document.createElement("button"), { type: "button", className: "small primary", textContent: "Use it here instead" });
    use.addEventListener("click", add);
    const keep = Object.assign(document.createElement("button"), { type: "button", className: "small", textContent: "Keep it there" });
    keep.addEventListener("click", () => { pending = null; message(""); render(); });
    message(`${keyLabel(combo)} already does "${clashes.map((b) => b.label).join('", "')}". `, [use, " ", keep]);
    render();
  }

  // While waiting for a new key, every key goes to the dialog.
  const onKey = (e) => {
    if (!capturing) return;
    e.preventDefault();
    e.stopPropagation();
    const combo = comboOf(e);
    if (!combo) return;
    if (combo === "Escape") {
      capturing = null;
      return render();
    }
    if (["Tab", "Shift+Tab"].includes(combo)) return message("Tab moves between controls, so it can't be a shortcut.");
    const action = KEY_ACTIONS.find((a) => a.id === capturing);
    assign(action, combo);
  };
  const onCancel = (e) => { if (capturing) e.preventDefault(); };
  const tabs = [...dlg.querySelectorAll(".settings-tabs button")];
  const showTab = (name) => {
    tabs.forEach((t) => t.classList.toggle("active", t.dataset.tab === name));
    $("settings-keys").hidden = name !== "keys";
    $("settings-general").hidden = name !== "general";
    $("settings-about").hidden = name !== "about";
    $("settings-models").hidden = name !== "models";
    $("keys-reset-all").hidden = name !== "keys";
    if (name === "models") modelsTab().catch((e) => toast(String(e), true));
  };
  const onTab = (e) => showTab(e.currentTarget.dataset.tab);
  const resetAll = () => {
    ui.keybindings = {};
    capturing = pending = null;
    message("All shortcuts are back to their defaults.");
    saveKeybindings();
    render();
  };
  const onFilter = () => renderList();
  const openFolder = () => invoke("open_settings_folder").catch((e) => toast(String(e), true));
  const setHub = async (url) => {
    try {
      $("settings-hub").value = await invoke("set_hub_url", { url });
    } catch (e) {
      toast(String(e), true);
    }
  };
  const onHub = () => setHub($("settings-hub").value);
  // Edit in another app.
  const showEditIn = (e) => {
    $("settings-edit-app").value = e.app ?? "";
    $("settings-edit-space").value = e.color_space;
    $("settings-edit-bits").value = String(e.bits);
  };
  const saveEditIn = async (app) => {
    try {
      showEditIn(await invoke("set_edit_in", {
        settings: {
          app: app === undefined ? $("settings-edit-app").value || null : app,
          color_space: $("settings-edit-space").value,
          bits: Number($("settings-edit-bits").value),
        },
      }));
    } catch (e) {
      toast(String(e), true);
    }
  };
  const pickEditApp = async () => {
    const app = await invoke("pick_edit_app");
    if (app) saveEditIn(app);
  };
  const editDefault = () => saveEditIn(null);
  const onEditFormat = () => saveEditIn();
  const hubDefault = () => setHub(null);
  const openLooks = () => { dlg.close(); setTimeout(() => looksFlow().catch((e) => toast(String(e), true))); };
  const showText = (which) => async () => {
    try {
      const pre = $("about-text");
      pre.textContent = await invoke("about_text", { which });
      pre.hidden = false;
      pre.scrollTop = 0;
    } catch (e) {
      toast(String(e), true);
    }
  };
  const onLicence = showText("licence");
  const onNotice = showText("notice");
  const onSource = () => invoke("open_source_page").catch((e) => toast(String(e), true));

  window.addEventListener("keydown", onKey, true);
  dlg.addEventListener("cancel", onCancel);
  tabs.forEach((t) => t.addEventListener("click", onTab));
  $("keys-reset-all").addEventListener("click", resetAll);
  $("keys-filter").addEventListener("input", onFilter);
  $("settings-folder").addEventListener("click", openFolder);
  $("settings-looks").addEventListener("click", openLooks);
  $("settings-hub").addEventListener("change", onHub);
  $("settings-hub-default").addEventListener("click", hubDefault);
  $("settings-edit-pick").addEventListener("click", pickEditApp);
  $("settings-edit-default").addEventListener("click", editDefault);
  $("settings-edit-space").addEventListener("change", onEditFormat);
  $("settings-edit-bits").addEventListener("change", onEditFormat);
  $("about-licence").addEventListener("click", onLicence);
  $("about-notice").addEventListener("click", onNotice);
  $("about-source").addEventListener("click", onSource);
  $("about-text").hidden = true;
  invoke("hub_url").then((u) => ($("settings-hub").value = u)).catch(() => {});
  invoke("edit_in_settings").then(showEditIn).catch(() => {});

  try {
    const info = await invoke("settings_info");
    $("settings-version").textContent = `Ambrotype ${info.version}`;
    $("about-version").textContent = info.version;
    $("settings-path").textContent = info.folder ?? "";
  } catch { /* the General tab just shows less */ }
  $("keys-filter").value = "";
  message("");
  showTab("keys");
  render();
  dlg.showModal();
  await new Promise((resolve) => dlg.addEventListener("close", resolve, { once: true }));

  window.removeEventListener("keydown", onKey, true);
  dlg.removeEventListener("cancel", onCancel);
  tabs.forEach((t) => t.removeEventListener("click", onTab));
  $("keys-reset-all").removeEventListener("click", resetAll);
  $("keys-filter").removeEventListener("input", onFilter);
  $("settings-folder").removeEventListener("click", openFolder);
  $("settings-looks").removeEventListener("click", openLooks);
  $("settings-hub").removeEventListener("change", onHub);
  $("settings-hub-default").removeEventListener("click", hubDefault);
  $("settings-edit-pick").removeEventListener("click", pickEditApp);
  $("settings-edit-default").removeEventListener("click", editDefault);
  $("settings-edit-space").removeEventListener("change", onEditFormat);
  $("settings-edit-bits").removeEventListener("change", onEditFormat);
  $("about-licence").removeEventListener("click", onLicence);
  $("about-notice").removeEventListener("click", onNotice);
  $("about-source").removeEventListener("click", onSource);
}
