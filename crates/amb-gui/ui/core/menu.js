// SPDX-License-Identifier: GPL-3.0-or-later
// The menu bar: the keyboard actions (keys.js) by name, in menus as
// Lightroom has them, with their current keys. On macOS it is the system's
// menu bar at the top of the screen; Windows and Linux show it in the window
// when Settings → General says so (src/commands/menu.rs). An item runs its action
// as its key does, and is greyed out where the key wouldn't work.
"use strict";

// Each menu lists action ids, "|" for a line, [title, [...]] for a submenu,
// the system's own items as "#copy" and so on, and "~" before what only
// Windows and Linux list there (macOS has it in the app's own menu).
const MENU_BAR = [
  ["Ambrotype", ["#about", "|", "settings", "|", "#services", "|", "#hide", "#hide_others", "#show_all", "|", "#quit"], "mac"],
  [tKey("File"), ["open", "open-folder", "import", "|", "save", "export", "convert-raws", "batch-export", "share-compare", "replay", "edit-in", "|", "collage", "watermark-maker", "|", "~settings", "|", "#close_window", "~#quit"]],
  [tKey("Edit"), ["undo", "undo-all", "redo", "|", "#cut", "#copy", "#paste", "lib-all", "lib-none", "|", "lib-copy", "lib-paste"]],
  [tKey("Photo"), [
    "crop", "masks", "spots", "wb", "|",
    [tKey("Rating"), ["rate-1", "rate-2", "rate-3", "rate-4", "rate-5", "|", "rate-0"]],
    [tKey("Colour Label"), ["label-red", "label-yellow", "label-green", "label-blue"]],
    [tKey("Flag"), ["flag-pick", "flag-reject", "flag-none"]],
    "keywords", "|", "rotate-left", "rotate-right", "flip-horizontal", "|", "save-preset", "styles", "style-new", "virtual-copy", "quick-collection", "|", "assistant", "plugins",
  ]],
  [tKey("Library"), ["library", "|", "lib-loupe", "lib-compare", "|", "slideshow", "|", "lib-stack", "lib-unstack", "|", "prev-photo", "next-photo"]],
  [tKey("View"), ["before", "split", "side-by-side", "side-by-side-fill", "above-below", "touch", "zoom", "loupe", "proof", "clipping", "zone-overlay", "peek-negative", "scope-toggle", "frame", "|", "filmstrip", "fold-left", "fold-right", "|", "#fullscreen"]],
  [tKey("Window"), ["#minimize", "#maximize", "|", [tKey("Task"), ["task-library", "task-develop", "task-film", "task-collage"]], "|", "looks", "edit-layout", "blank-layout", "|", "hub", "ai-free-toggle", "|", "#bring_all_to_front"], "window"],
  [tKey("Help"), ["search", "|", "connect-ai", "|", "features", "learn", "basics", "camsim", "sample", "watch", "shortcuts", "|", "feedback", "source"], "help"],
];
// Only on macOS: the rest have them in the window's title bar, or no Edit menu.
const MENU_MAC_ONLY = new Set(["#services", "#hide", "#hide_others", "#show_all", "#cut", "#copy", "#paste", "#fullscreen", "#minimize", "#maximize", "#bring_all_to_front"]);

// Menu titles, where the action's own label (keys.js) is a sentence.
const MENU_TITLES = {
  open: tKey("Open…"), "open-folder": tKey("Open Folder…"), slideshow: tKey("Slideshow…"), import: tKey("Import Photos…"), save: tKey("Save"), export: tKey("Export…"), "edit-in": tKey("Edit in Another App"), "share-compare": tKey("Share Before and After…"), touch: tKey("Touch: Drag on the Photo"),
  collage: tKey("Collage Maker…"), "watermark-maker": tKey("Watermark Maker…"), "lib-all": tKey("Select All"), "lib-none": tKey("Select None"), "lib-copy": tKey("Copy Settings"),
  "lib-paste": tKey("Paste Settings"), "save-preset": tKey("New Preset from This Photo…"), "convert-raws": tKey("Convert Raws to JPEGs…"), styles: tKey("Styles…"), "style-new": tKey("New Style from Photos…"), "batch-export": tKey("Batch Export…"), replay: tKey("Export Edit Replay…"), crop: tKey("Crop and Straighten"), spots: tKey("Spot Removal"), wb: tKey("White Balance Picker"),
  "rate-0": tKey("No Rating"), "label-red": tKey("Red"), "label-yellow": tKey("Yellow"), "label-green": tKey("Green"), "label-blue": tKey("Blue"),
  "flag-pick": tKey("Pick"), "flag-reject": tKey("Reject"), "flag-none": tKey("No Flag"), keywords: tKey("Add Keywords"),
  "virtual-copy": tKey("Make a Virtual Copy"), "quick-collection": tKey("Add to the Last Collection"), assistant: tKey("Assistant…"),
  plugins: tKey("Plugins…"), library: tKey("Library Grid"), "lib-loupe": tKey("One photo large"), "lib-compare": tKey("Compare"),
  "lib-stack": tKey("Stack"), "lib-unstack": tKey("Unstack"), "prev-photo": tKey("Previous Photo"), "next-photo": tKey("Next Photo"),
  zoom: tKey("Fit / 100%"), proof: tKey("Soft Proofing"), clipping: tKey("Clipping Overlay"), "zone-overlay": tKey("Zone Overlay"), "peek-negative": tKey("Peek at Negative"), frame: tKey("Frame"), filmstrip: tKey("Filmstrip"),
  "fold-left": tKey("Left Column"), "fold-right": tKey("Right Column"), looks: tKey("Appearance…"), "edit-layout": tKey("Edit Layout"), "blank-layout": tKey("New Layout from Scratch"),
  hub: tKey("Browse the Hub…"), "ai-free-toggle": tKey("AI-free Mode"), settings: tKey("Settings…"), learn: tKey("Learn the Basics"), features: tKey("What Ambrotype Can Do…"), shortcuts: tKey("Keyboard Shortcuts"),
  feedback: tKey("Send Feedback…"), source: tKey("Ambrotype on GitHub"), search: tKey("Search Everything…"), "connect-ai": tKey("Connect an AI App…"),
  "task-library": tKey("Library"), "task-develop": tKey("Develop"), "task-film": tKey("Film"), "task-collage": tKey("Collage"),
};

// Items that aren't keyboard actions.
const MENU_EXTRA = { source: () => invoke("open_source_page").catch(showError) };

const menuState = { shown: false, enabled: {}, checked: {} };

// A key the menu can take itself: one with Ctrl (Cmd) or Alt, or F1 to F12.
// Lone letters, digits and Shift+letter stay with the page, or typing in a
// field would run them; their key shows in the item's title instead.
function menuAccelerator(combo) {
  if (!/(^|\+)(Ctrl|Alt)\+/.test(combo) && !/^F\d+$/.test(combo)) return null;
  return combo.replace(/^Ctrl\+/, "CmdOrCtrl+");
}

function menuEntry(name, mac) {
  if (name === "|") return { kind: "separator" };
  if (Array.isArray(name)) return { kind: "submenu", text: t(name[0]), items: menuEntries(name[1], mac) };
  if (name.startsWith("#")) return { kind: "native", name: name.slice(1) };
  const a = KEY_ACTIONS.find((x) => x.id === name);
  let text = MENU_TITLES[name] ? t(MENU_TITLES[name]) : a?.vars?.stars ?? (a ? actionLabel(a) : name);
  const combo = a ? keysFor(a)[0] : null;
  const keys = combo ? menuAccelerator(combo) : null;
  if (combo && !keys) text += `  (${keyLabel(combo)})`;
  const entry = { kind: "item", id: name, text, keys, enabled: menuItemOn(name) };
  if (a?.isOn) entry.checked = menuItemTick(name);
  return entry;
}

function menuEntries(names, mac) {
  return names
    .filter((n) => (mac ? !String(n).startsWith("~") : !MENU_MAC_ONLY.has(n)))
    .map((n) => menuEntry(typeof n === "string" ? n.replace(/^~/, "") : n, mac))
    // No line at either end, or two in a row, once items are left out.
    .filter((e, i, all) => e.kind !== "separator" || (i > 0 && i < all.length - 1 && all[i - 1].kind !== "separator"));
}

// Sends the menus to the app; again whenever the keys change.
async function buildMenu() {
  const menus = MENU_BAR.filter(([, , role]) => IS_MAC || role !== "mac").map(([text, names, role]) => ({
    kind: "submenu",
    text: t(text),
    role: role === "mac" ? null : role ?? null,
    items: menuEntries(names, IS_MAC),
  }));
  try {
    menuState.shown = await invoke("set_menu", { menus, wanted: menuWanted() });
    menuState.enabled = {};
    menuState.checked = {};
    syncMenu();
  } catch (e) {
    console.warn("menu bar:", e);
  }
}

function menuItemOn(id) {
  if (MENU_EXTRA[id] || ["undo", "redo", "lib-all"].includes(id)) return true;
  const a = KEY_ACTIONS.find((x) => x.id === id);
  return Boolean(a) && (a.when === "any" || contextsOf(a).includes(keyContext())) && (!needsPhoto(a) || Boolean(ui.info));
}

// Whether an on/off action is on, for its tick; a state that can't be read yet is off.
function menuItemTick(id) {
  try {
    return Boolean(KEY_ACTIONS.find((x) => x.id === id)?.isOn?.());
  } catch {
    return false;
  }
}

// Greys out what can't run now and ticks what is on; only changes go to the app.
function syncMenu() {
  if (!menuState.shown) return;
  const changes = {};
  const ticks = {};
  const walk = (names) => {
    for (const n of names) {
      if (Array.isArray(n)) walk(n[1]);
      else if (n !== "|" && !n.startsWith("#")) {
        const on = menuItemOn(n);
        if (menuState.enabled[n] !== on) changes[n] = menuState.enabled[n] = on;
        if (KEY_ACTIONS.find((x) => x.id === n)?.isOn) {
          const tick = menuItemTick(n);
          if (menuState.checked[n] !== tick) ticks[n] = menuState.checked[n] = tick;
        }
      }
    }
  };
  for (const [, names] of MENU_BAR) walk(names);
  if (Object.keys(changes).length) invoke("menu_enable", { enabled: changes }).catch(() => {});
  if (Object.keys(ticks).length) invoke("menu_check", { checked: ticks }).catch(() => {});
}

// A chosen item. Undo, Redo and Select All work on a text field while one
// is being typed in, as the system's would.
function runMenuItem(id) {
  // The page ran it already for this key press (a menu that saw the key too).
  if (Date.now() - (keyRunAt[id] ?? 0) < 300) return;
  const field = document.activeElement?.matches?.("input:not([type=range]):not([type=checkbox]), textarea, [contenteditable]");
  if (field && ["undo", "redo", "lib-all"].includes(id)) {
    document.execCommand(id === "lib-all" ? "selectAll" : id);
    return;
  }
  if (MENU_EXTRA[id]) return MENU_EXTRA[id]();
  if (id === "lib-all" && !libraryOn()) return;
  const a = KEY_ACTIONS.find((x) => x.id === id);
  if (a && menuItemOn(id)) a.run({ target: document.body, preventDefault() {} });
  requestAnimationFrame(syncMenu);
}

// Windows and Linux: the Settings switch, kept in this computer's browser storage.
function menuWanted() {
  return store.text("amb.menuBar") === "1";
}

function wireMenu() {
  // macOS always has it, at the top of the screen.
  $("settings-menu-row").hidden = IS_MAC;
  $("settings-menu-bar").checked = menuWanted();
  $("settings-menu-bar").addEventListener("change", () => {
    store.setText("amb.menuBar", $("settings-menu-bar").checked ? "1" : "");
    buildMenu();
  });
  tauri.event.listen("menu", (e) => runMenuItem(e.payload));
  // Most changes of tool or photo come from a click or a key.
  for (const type of ["click", "keyup"]) document.addEventListener(type, () => requestAnimationFrame(syncMenu), true);
}
