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
  ["File", ["open", "import", "|", "save", "export", "edit-in", "|", "collage", "|", "~settings", "|", "#close_window", "~#quit"]],
  ["Edit", ["undo", "redo", "|", "#cut", "#copy", "#paste", "lib-all", "lib-none", "|", "lib-copy", "lib-paste"]],
  ["Photo", [
    "crop", "masks", "spots", "wb", "|",
    ["Rating", ["rate-1", "rate-2", "rate-3", "rate-4", "rate-5", "|", "rate-0"]],
    ["Colour Label", ["label-red", "label-yellow", "label-green", "label-blue"]],
    ["Flag", ["flag-pick", "flag-reject", "flag-none"]],
    "keywords", "|", "virtual-copy", "quick-collection", "|", "assistant", "plugins",
  ]],
  ["Library", ["library", "|", "lib-loupe", "lib-compare", "lib-survey", "|", "lib-stack", "lib-unstack", "|", "prev-photo", "next-photo"]],
  ["View", ["before", "zoom", "loupe", "proof", "clipping", "frame", "|", "filmstrip", "fold-left", "fold-right", "|", "#fullscreen"]],
  ["Window", ["#minimize", "#maximize", "|", ["Task", ["task-library", "task-develop", "task-color", "task-retouch", "task-film", "task-collage"]], "|", "looks", "edit-layout", "|", "hub", "|", "#bring_all_to_front"], "window"],
  ["Help", ["search", "|", "connect-ai", "|", "learn", "basics", "sample", "watch", "shortcuts", "|", "feedback", "source"], "help"],
];
// Only on macOS: the rest have them in the window's title bar, or no Edit menu.
const MENU_MAC_ONLY = new Set(["#services", "#hide", "#hide_others", "#show_all", "#cut", "#copy", "#paste", "#fullscreen", "#minimize", "#maximize", "#bring_all_to_front"]);

// Menu titles, where the action's own label (keys.js) is a sentence.
const MENU_TITLES = {
  open: "Open…", import: "Import Photos…", save: "Save", export: "Export…", "edit-in": "Edit in Another App",
  collage: "Collage Maker…", "lib-all": "Select All", "lib-none": "Select None", "lib-copy": "Copy Settings",
  "lib-paste": "Paste Settings", crop: "Crop and Straighten", spots: "Spot Removal", wb: "White Balance Picker",
  "rate-0": "No Rating", "label-red": "Red", "label-yellow": "Yellow", "label-green": "Green", "label-blue": "Blue",
  "flag-pick": "Pick", "flag-reject": "Reject", "flag-none": "No Flag", keywords: "Add Keywords",
  "virtual-copy": "Make a Virtual Copy", "quick-collection": "Add to the Last Collection", assistant: "Assistant…",
  plugins: "Plugins…", library: "Library Grid", "lib-loupe": "Loupe", "lib-compare": "Compare", "lib-survey": "Survey",
  "lib-stack": "Stack", "lib-unstack": "Unstack", "prev-photo": "Previous Photo", "next-photo": "Next Photo",
  zoom: "Fit / 100%", proof: "Soft Proofing", clipping: "Clipping Overlay", frame: "Frame", filmstrip: "Filmstrip",
  "fold-left": "Left Column", "fold-right": "Right Column", looks: "Appearance…", "edit-layout": "Edit Layout",
  hub: "Browse the Hub…", settings: "Settings…", learn: "Learn the Basics", shortcuts: "Keyboard Shortcuts",
  feedback: "Send Feedback…", source: "Ambrotype on GitHub", search: "Search Everything…", "connect-ai": "Connect an AI App…",
  "task-library": "Library", "task-develop": "Develop", "task-color": "Color", "task-retouch": "Retouch", "task-film": "Film", "task-collage": "Collage",
};

// Items that aren't keyboard actions.
const MENU_EXTRA = { source: () => invoke("open_source_page").catch((e) => toast(String(e), true)) };

const menuState = { shown: false, enabled: {} };

// A key the menu can take itself: one with Ctrl (Cmd) or Alt, or F1 to F12.
// Lone letters, digits and Shift+letter stay with the page, or typing in a
// field would run them; their key shows in the item's title instead.
function menuAccelerator(combo) {
  if (!/(^|\+)(Ctrl|Alt)\+/.test(combo) && !/^F\d+$/.test(combo)) return null;
  return combo.replace(/^Ctrl\+/, "CmdOrCtrl+");
}

function menuEntry(name, mac) {
  if (name === "|") return { kind: "separator" };
  if (Array.isArray(name)) return { kind: "submenu", text: name[0], items: menuEntries(name[1], mac) };
  if (name.startsWith("#")) return { kind: "native", name: name.slice(1) };
  const a = KEY_ACTIONS.find((x) => x.id === name);
  let text = MENU_TITLES[name] ?? a?.label.replace(/^Rate /, "") ?? name;
  const combo = a ? keysFor(a)[0] : null;
  const keys = combo ? menuAccelerator(combo) : null;
  if (combo && !keys) text += `  (${keyLabel(combo)})`;
  return { kind: "item", id: name, text, keys, enabled: menuItemOn(name) };
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
    text,
    role: role === "mac" ? null : role ?? null,
    items: menuEntries(names, IS_MAC),
  }));
  try {
    menuState.shown = await invoke("set_menu", { menus, wanted: menuWanted() });
    menuState.enabled = {};
    syncMenu();
  } catch (e) {
    console.warn("menu bar:", e);
  }
}

function menuItemOn(id) {
  if (MENU_EXTRA[id] || ["undo", "redo", "lib-all"].includes(id)) return true;
  const a = KEY_ACTIONS.find((x) => x.id === id);
  return Boolean(a) && (a.when === "any" || contextsOf(a).includes(keyContext())) && (!a.photo || Boolean(ui.info));
}

// Greys out what can't run now; only changes go to the app.
function syncMenu() {
  if (!menuState.shown) return;
  const changes = {};
  const walk = (names) => {
    for (const n of names) {
      if (Array.isArray(n)) walk(n[1]);
      else if (n !== "|" && !n.startsWith("#")) {
        const on = menuItemOn(n);
        if (menuState.enabled[n] !== on) changes[n] = menuState.enabled[n] = on;
      }
    }
  };
  for (const [, names] of MENU_BAR) walk(names);
  if (Object.keys(changes).length) invoke("menu_enable", { enabled: changes }).catch(() => {});
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
  try { return localStorage.getItem("amb.menuBar") === "1"; } catch { return false; }
}

function wireMenu() {
  // macOS always has it, at the top of the screen.
  $("settings-menu-row").hidden = IS_MAC;
  $("settings-menu-bar").checked = menuWanted();
  $("settings-menu-bar").addEventListener("change", () => {
    try { localStorage.setItem("amb.menuBar", $("settings-menu-bar").checked ? "1" : ""); } catch { /* not kept */ }
    buildMenu();
  });
  tauri.event.listen("menu", (e) => runMenuItem(e.payload));
  // Most changes of tool or photo come from a click or a key.
  for (const type of ["click", "keyup"]) document.addEventListener(type, () => requestAnimationFrame(syncMenu), true);
}
