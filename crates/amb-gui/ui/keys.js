// Keyboard shortcuts: one table of actions with their default keys, the
// user's changes (kept in the app's settings by src/keys.rs; only changes are
// stored, so new defaults still arrive), the key dispatcher, and the Settings
// dialog that lists them on a keyboard map and lets you change them.
//
// A key is written like "Ctrl+Shift+Z": Ctrl means Ctrl, or Cmd on a Mac.
"use strict";

const IS_MAC = /Mac|iPhone|iPad/.test(navigator.platform);

// `when`: "any", "main" (crop tool closed) or "crop" (crop tool open). Actions
// with overlapping contexts can't share a key. `photo`: needs an open photo.
// `button`: the toolbar button whose tooltip shows the keys.
const KEY_ACTIONS = [
  { id: "open", group: "File", label: "Open a photo", keys: ["Ctrl+O"], when: "any", button: "btn-open", run: () => openDialog() },
  { id: "save", group: "File", label: "Save the edit to the sidecar", keys: ["Ctrl+S"], when: "any", button: "btn-save", run: () => save() },
  { id: "export", group: "File", label: "Export", keys: ["Ctrl+E"], when: "any", button: "btn-export", run: () => exportFlow() },
  { id: "collage", group: "File", label: "Collage maker", hint: "Several photos on one canvas.", keys: ["C"], when: "main", button: "btn-collage", run: () => collageFlow() },
  { id: "undo", group: "Edit", label: "Undo", keys: ["Ctrl+Z"], when: "any", button: "btn-undo", run: () => run([{ cmd: "undo" }]) },
  { id: "redo", group: "Edit", label: "Redo", keys: ["Ctrl+Shift+Z", "Ctrl+Y"], when: "any", button: "btn-redo", run: () => run([{ cmd: "redo" }]) },
  { id: "before", group: "View", label: "Before / after", keys: ["\\"], when: "any", button: "btn-before", run: () => toggleBefore() },
  { id: "zoom", group: "View", label: "Fit / 100% view", hint: "Or click the photo; drag to pan at 100%.", keys: ["Z"], when: "main", photo: true, button: "btn-zoom", run: () => setZoom(!ui.zoom.on, ui.zoom.cx, ui.zoom.cy) },
  { id: "clipping", group: "View", label: "Clipping overlay", hint: "Red: clipped highlights, blue: crushed shadows.", keys: ["J"], when: "any", button: "btn-clip", run: () => toggleClipping() },
  { id: "crop", group: "Tools", label: "Crop and straighten", keys: ["R"], when: "any", photo: true, button: "btn-crop", run: () => toggleCrop() },
  { id: "wb", group: "Tools", label: "White-balance picker", hint: "Then click something neutral grey or white.", keys: ["W"], when: "main", button: "btn-wb", run: () => setPicking(!ui.picking) },
  { id: "leave", group: "Tools", label: "Leave the picker", keys: ["Escape"], when: "main", run: () => setPicking(false) },
  { id: "crop-draw", group: "Crop tool", label: "Draw a new crop (corner to corner)", keys: ["D"], when: "crop", run: () => ui.crop.tool.setDrawMode(!ui.crop.tool.drawMode()) },
  { id: "crop-swap", group: "Crop tool", label: "Swap portrait and landscape", keys: ["X"], when: "crop", run: () => swapCrop() },
  { id: "crop-apply", group: "Crop tool", label: "Apply the crop", keys: ["Enter"], when: "crop", run: (e) => (e.target.matches("button, select") ? false : exitCrop(true)) },
  { id: "crop-cancel", group: "Crop tool", label: "Cancel the crop", keys: ["Escape"], when: "crop", run: () => exitCrop(false) },
  ...[1, 2, 3, 4, 5].map((n) => ({ id: `rate-${n}`, group: "Rating", label: `Rate ${"★".repeat(n)}`, keys: [String(n)], when: "any", photo: true, run: () => setRating(n) })),
  { id: "rate-0", group: "Rating", label: "Clear the rating", keys: ["0"], when: "any", photo: true, run: () => setRating(0) },
  { id: "keywords", group: "Rating", label: "Add keywords (jump to the keyword field)", keys: ["K"], when: "any", photo: true, run: () => $("tag-input").focus() },
  { id: "prev-photo", group: "Photos", label: "Previous photo in the folder", keys: ["ArrowLeft"], when: "main", photo: true, run: (e) => (onControl(e) ? false : stepPhoto(-1)) },
  { id: "next-photo", group: "Photos", label: "Next photo in the folder", keys: ["ArrowRight"], when: "main", photo: true, run: (e) => (onControl(e) ? false : stepPhoto(1)) },
  { id: "filmstrip", group: "Photos", label: "Show or hide the filmstrip", keys: ["F"], when: "any", run: () => toggleFilmstrip() },
  { id: "plugins", group: "Tools", label: "Plugins", hint: "Extra tools that make edits; run, install and share them.", keys: ["Ctrl+Shift+P"], when: "any", button: "btn-plugins", run: () => pluginsFlow() },
  { id: "looks", group: "Window", label: "Looks: layouts and themes", keys: ["Ctrl+L"], when: "any", button: "btn-looks", run: () => looksFlow() },
  { id: "edit-layout", group: "Window", label: "Edit layout", hint: "Move panels, controls and toolbar buttons.", keys: ["Ctrl+Shift+L"], when: "any", button: "btn-edit-layout", run: () => startEditing() },
  { id: "settings", group: "Window", label: "Settings and keyboard shortcuts", keys: ["Ctrl+,"], when: "any", button: "btn-settings", run: () => settingsFlow() },
  { id: "feedback", group: "Window", label: "Send feedback", hint: "A bug, an idea, or a photo that looks different from Lightroom.", keys: [], when: "any", button: "btn-feedback", run: () => feedbackFlow() },
];

// Arrow keys belong to a focused slider, list or knob.
const onControl = (e) => e.target.matches("input, select, textarea, [tabindex], .bar, svg");
const keysFor = (a) => ui.keybindings?.[a.id] ?? a.keys;
const contextsOverlap = (a, b) => a.when === "any" || b.when === "any" || a.when === b.when;

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

function dispatchKey(e) {
  if (e.target.matches("input[type=number], input[type=search], input[type=text], textarea, input.value")) return;
  // Keys belong to an open dialog.
  if (document.querySelector("dialog[open]")) return;
  const combo = comboOf(e);
  if (!combo) return;
  const ctx = ui.crop.on ? "crop" : "main";
  for (const a of KEY_ACTIONS) {
    if (!keysFor(a).includes(combo)) continue;
    if (a.when !== "any" && a.when !== ctx) continue;
    if (a.photo && !ui.info) continue;
    if (a.run(e) === false) continue;
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
  const hubDefault = () => setHub(null);
  const openLooks = () => { dlg.close(); setTimeout(() => looksFlow().catch((e) => toast(String(e), true))); };

  window.addEventListener("keydown", onKey, true);
  dlg.addEventListener("cancel", onCancel);
  tabs.forEach((t) => t.addEventListener("click", onTab));
  $("keys-reset-all").addEventListener("click", resetAll);
  $("keys-filter").addEventListener("input", onFilter);
  $("settings-folder").addEventListener("click", openFolder);
  $("settings-looks").addEventListener("click", openLooks);
  $("settings-hub").addEventListener("change", onHub);
  $("settings-hub-default").addEventListener("click", hubDefault);
  invoke("hub_url").then((u) => ($("settings-hub").value = u)).catch(() => {});

  try {
    const info = await invoke("settings_info");
    $("settings-version").textContent = `Ambrotype ${info.version}`;
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
}
