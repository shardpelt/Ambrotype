// SPDX-License-Identifier: GPL-3.0-or-later
// Collections in the library: named lists of photos from any folders, kept
// as small files in the app's config folder (amb-library's collections).
// The library shows a folder or a collection; the panel adds the selection
// to one, and B adds to the last one used (Lightroom's quick collection).
"use strict";

const coll = { list: [], smart: [], target: null };
const NEW_COLLECTION = "\u0000new";
const NEW_SMART = "\u0000smart";
// A smart collection's value in the Show menu.
const SMART = "smart:";
// Every photo in the catalogue: shown like a smart collection.
const ALL_PHOTOS = "\u0000all";
// "Rated photos": All photos with one star or more, highest first.
const RATED = "\u0000rated";

// Ask for a name in a small dialog; null when cancelled.
// `allowEmpty`: an empty answer is "" rather than a cancel.
async function askName(title, hint = "", value = "", allowEmpty = false) {
  const dlg = $("name-dialog");
  $("name-title").textContent = title;
  $("name-hint").textContent = hint;
  const input = dlg.querySelector("input[name=value]");
  input.value = value;
  input.required = !allowEmpty;
  dlg.showModal();
  input.select();
  const choice = await new Promise((resolve) => dlg.addEventListener("close", () => resolve(dlg.returnValue), { once: true }));
  const name = input.value.trim();
  if (choice !== "ok") return null;
  return name || (allowEmpty ? "" : null);
}

async function loadCollections() {
  try {
    coll.list = await invoke("collections_list");
    coll.smart = await invoke("smart_list");
  } catch (e) {
    coll.list = [];
    coll.smart = [];
    toast(String(e), true);
  }
  if (coll.target && !coll.list.some((c) => c.name === coll.target)) coll.target = null;
  renderCollectionControls();
}

function renderCollectionControls() {
  const option = (value, text) => Object.assign(document.createElement("option"), { value, textContent: text });
  const src = $("lib-source");
  const folder = lib.dir ? `Folder: ${fileName(lib.dir.replace(/[\\/]+$/, "")) || lib.dir}` : "Folder";
  // Collections not in a set first, then one group per collection set.
  const sets = [...new Set(coll.list.map((c) => c.set ?? ""))].sort((a, b) => (b === "") - (a === "") || a.localeCompare(b));
  const inSets = sets.map((set) => {
    const g = Object.assign(document.createElement("optgroup"), { label: set ? `▸ ${set}` : "Collections" });
    g.append(...coll.list.filter((c) => (c.set ?? "") === set).map((c) => option(c.name, `${c.name} (${c.count})`)));
    return g;
  });
  const smart = Object.assign(document.createElement("optgroup"), { label: "Smart collections" });
  smart.append(...coll.smart.map((c) => option(SMART + c.name, `⚙ ${c.name}`)), option(NEW_SMART, "New smart collection…"));
  src.replaceChildren(option("", folder), option(ALL_PHOTOS, "All photos"), option(RATED, "★ Rated photos, best first"), ...inSets, smart);
  const rated = lib.smart === ALL_PHOTOS && $("lib-filter-rating").value !== "0" && $("lib-filter-rating").value !== "unrated" && $("lib-sort").value === "rating";
  src.value = rated ? RATED : lib.smart === ALL_PHOTOS ? ALL_PHOTOS : lib.smart ? SMART + lib.smart : lib.collection ?? "";
  const add = $("lib-add-to");
  add.replaceChildren(...coll.list.map((c) => option(c.name, c.name)), option(NEW_COLLECTION, "New collection…"));
  add.value = coll.target ?? coll.list[0]?.name ?? NEW_COLLECTION;
  const viewing = Boolean(lib.collection) && !lib.smart;
  for (const id of ["lib-remove-from", "lib-coll-rename", "lib-coll-set"]) $(id).hidden = !viewing;
  const ownSmart = lib.smart && lib.smart !== ALL_PHOTOS;
  $("lib-coll-delete").hidden = !viewing && !ownSmart;
  $("lib-smart-edit").hidden = !ownSmart;
  $("lib-subfolders").disabled = Boolean(lib.collection || lib.smart);
  if (viewing) $("lib-remove-from").textContent = `Remove from “${lib.collection}”`;
}

// Show a collection in the library ("" for the folder).
async function showCollection(name) {
  if (name === NEW_SMART) {
    renderCollectionControls();
    return editSmartCollection(null);
  }
  if (name === RATED) {
    // Filter ▾ then narrows it to the stars wanted (★★★ and up, say).
    if (["0", "unrated"].includes($("lib-filter-rating").value)) $("lib-filter-rating").value = "1";
    $("lib-sort").value = "rating";
    $("lib-reverse").checked = false;
    return showSmartCollection(ALL_PHOTOS);
  }
  if (name === ALL_PHOTOS) {
    if (lib.smart === ALL_PHOTOS) $("lib-filter-rating").value = "0";
    return showSmartCollection(ALL_PHOTOS);
  }
  if (name?.startsWith(SMART)) return showSmartCollection(name.slice(SMART.length));
  lib.smart = null;
  if (!name) {
    lib.collection = null;
    if (lib.dir) await loadLibrary(lib.dir);
    else renderLibrary();
    renderCollectionControls();
    return;
  }
  try {
    const view = await invoke("collection_photos", { name });
    lib.collection = view.name;
    lib.photos = view.photos;
    const keep = new Set(lib.photos.map((p) => p.path));
    lib.selected = new Set([...lib.selected].filter((p) => keep.has(p)));
    $("library-folder").textContent = "";
    $("lib-drop-folders").hidden = true;
    if (view.missing.length) toast(`${view.missing.length} photo${view.missing.length === 1 ? " in it was" : "s in it were"} moved or deleted: ${fileName(view.missing[0])}${view.missing.length > 1 ? "…" : ""}`, true);
    renderLibrary();
  } catch (e) {
    toast(String(e), true);
  }
  renderCollectionControls();
}

// Add photos to a collection; NEW_COLLECTION asks for a name.
async function addToCollection(paths, name = $("lib-add-to").value) {
  if (!paths.length) return false;
  if (!name || name === NEW_COLLECTION) {
    name = await askName("New collection", "Collections hold photos from any folders. They are kept in Ambrotype's settings folder, not next to the photos.");
    if (!name) return true;
  }
  try {
    const c = await invoke("collection_add", { name, paths });
    coll.target = c.name;
    status(`Added ${paths.length} photo${paths.length === 1 ? "" : "s"} to “${c.name}” (${c.count} in it)`);
    await loadCollections();
    if (lib.collection === c.name) await showCollection(c.name);
  } catch (e) {
    toast(String(e), true);
  }
  return true;
}

// B: the selection (or the open photo) to the last collection used.
function quickCollectionKey() {
  const paths = lib.on ? libraryTargets().map((p) => p.path) : ui.info ? [ui.info.path] : [];
  if (!paths.length) return false;
  return addToCollection(paths, coll.target ?? NEW_COLLECTION);
}

async function removeFromCollection() {
  const paths = libraryTargets().map((p) => p.path);
  if (!lib.collection || !paths.length) return;
  try {
    await invoke("collection_remove", { name: lib.collection, paths });
    status(`Removed ${paths.length} photo${paths.length === 1 ? "" : "s"} from “${lib.collection}” (the files stay)`);
    lib.selected.clear();
    await loadCollections();
    await showCollection(lib.collection);
  } catch (e) {
    toast(String(e), true);
  }
}

async function renameCollection() {
  if (!lib.collection) return;
  const to = await askName("Rename collection", "", lib.collection);
  if (!to || to === lib.collection) return;
  try {
    const c = await invoke("collection_rename", { from: lib.collection, to });
    if (coll.target === lib.collection) coll.target = c.name;
    lib.collection = c.name;
    await loadCollections();
    $("library-folder").textContent = "";
    $("lib-drop-folders").hidden = true;
  } catch (e) {
    toast(String(e), true);
  }
}

// Lightroom's collection sets: a collection is filed under a set's name.
async function moveCollectionToSet() {
  if (!lib.collection) return;
  const current = coll.list.find((c) => c.name === lib.collection)?.set ?? "";
  const names = [...new Set(coll.list.map((c) => c.set).filter(Boolean))];
  const hint = `Sets group collections in the Show menu. ${names.length ? `Sets so far: ${names.join(", ")}. ` : ""}Leave empty to take it out of its set.`;
  const set = await askName("Collection set", hint, current, true);
  if (set === null || set === undefined || set === current) return;
  try {
    await invoke("collection_set", { name: lib.collection, set });
    await loadCollections();
  } catch (e) {
    toast(String(e), true);
  }
}

async function deleteCollection() {
  if (!lib.collection || !await askYes(`Delete the collection “${lib.collection}”? Its photos stay where they are.`)) return;
  try {
    if (lib.smart) await invoke("smart_delete", { name: lib.smart });
    else await invoke("collection_delete", { name: lib.collection });
    status(`Deleted the collection “${lib.collection}”`);
    await loadCollections();
    await showCollection("");
  } catch (e) {
    toast(String(e), true);
  }
}

function wireCollections() {
  $("lib-source").addEventListener("change", () => showCollection($("lib-source").value));
  $("lib-add").addEventListener("click", () => addToCollection(libraryTargets().map((p) => p.path)));
  $("lib-add-to").addEventListener("change", () => {
    if ($("lib-add-to").value !== NEW_COLLECTION) coll.target = $("lib-add-to").value;
  });
  $("lib-remove-from").addEventListener("click", removeFromCollection);
  $("lib-coll-rename").addEventListener("click", renameCollection);
  $("lib-coll-set").addEventListener("click", moveCollectionToSet);
  $("lib-coll-delete").addEventListener("click", deleteCollection);
  $("lib-smart-edit").addEventListener("click", () => editSmartCollection(lib.smart));
  $("lib-subfolders").addEventListener("change", () => {
    try { localStorage.setItem("amb.lib.subfolders", $("lib-subfolders").checked ? "1" : ""); } catch { /* not kept */ }
    if (lib.dir && !lib.collection) loadLibrary(lib.dir);
  });
  try { $("lib-subfolders").checked = localStorage.getItem("amb.lib.subfolders") === "1"; } catch { /* default off */ }
  loadCollections();
}

// ---------------------------------------------------------------- smart collections

// Worked out when shown: the photos under its folders that meet its rules.
async function showSmartCollection(name) {
  const all = name === ALL_PHOTOS;
  $("library-count").textContent = all ? "Reading the catalogue…" : "Looking through the folders…";
  try {
    const view = all ? await invoke("catalog_photos") : await invoke("smart_photos", { name });
    lib.smart = all ? ALL_PHOTOS : view.name;
    lib.collection = view.name;
    lib.photos = view.photos;
    const keep = new Set(lib.photos.map((p) => p.path));
    lib.selected = new Set([...lib.selected].filter((p) => keep.has(p)));
    $("library-folder").textContent = "";
    $("lib-drop-folders").hidden = true;
    renderLibrary();
  } catch (e) {
    toast(String(e), true);
  }
  renderCollectionControls();
}

// What each field can test, and how its value is entered.
const SMART_FIELDS = {
  rating: { label: "Rating", ops: [[">=", "is at least"], ["<=", "is at most"], ["=", "is"], ["!=", "is not"]],
    values: [0, 1, 2, 3, 4, 5].map((v) => [v, v ? "★".repeat(v) : "unrated"]) },
  label: { label: "Colour label", ops: [["=", "is"], ["!=", "is not"]], values: [["", "none"], ...LABELS.map((l) => [l, l])] },
  flag: { label: "Flag", ops: [["=", "is"], ["!=", "is not"]], values: [["pick", "pick"], ["reject", "reject"], ["", "none"]] },
  keyword: { label: "Keyword", ops: [["contains", "contains"], ["!contains", "doesn't contain"]], text: true },
  name: { label: "File name", ops: [["contains", "contains"], ["!contains", "doesn't contain"]], text: true },
  edited: { label: "Edited", ops: [["=", "is"]], values: [[true, "yes"], [false, "no"]] },
  captured: { label: "Capture date", ops: [["after", "is on or after"], ["before", "is before"]], date: true },
  camera: { label: "Camera", ops: [["contains", "contains"], ["!contains", "doesn't contain"]], text: true },
  lens: { label: "Lens", ops: [["contains", "contains"], ["!contains", "doesn't contain"]], text: true },
  iso: { label: "ISO", ops: [[">=", "is at least"], ["<=", "is at most"]], number: true },
  focal: { label: "Focal length (mm)", ops: [[">=", "is at least"], ["<=", "is at most"]], number: true },
};

function smartRuleRow(rule = { field: "rating", op: ">=", value: 3 }) {
  const row = document.createElement("div");
  row.className = "smart-rule";
  const field = document.createElement("select");
  for (const [k, f] of Object.entries(SMART_FIELDS)) field.append(new Option(f.label, k));
  const op = document.createElement("select");
  const holder = document.createElement("span");
  holder.style.display = "contents";
  const remove = Object.assign(document.createElement("button"), { type: "button", className: "small", textContent: "×", title: "Remove this rule" });
  remove.addEventListener("click", () => row.remove());
  let value = null;
  const build = (r) => {
    const f = SMART_FIELDS[field.value];
    op.replaceChildren(...f.ops.map(([v, t]) => new Option(t, v)));
    if (f.ops.some(([v]) => v === r.op)) op.value = r.op;
    if (f.values) {
      value = document.createElement("select");
      f.values.forEach(([v, t], i) => value.append(new Option(t, String(i))));
      const at = f.values.findIndex(([v]) => v === r.value);
      value.value = String(Math.max(0, at));
      value.read = () => f.values[Number(value.value)][0];
    } else {
      value = document.createElement("input");
      value.type = f.date ? "date" : f.number ? "number" : "text";
      if (f.number) {
        value.min = "0";
        value.value = typeof r.value === "number" ? String(r.value) : "";
        value.read = () => Number(value.value);
      } else {
        value.value = typeof r.value === "string" ? r.value : "";
        value.read = () => value.value.trim();
      }
    }
    holder.replaceChildren(value);
  };
  field.value = rule.field in SMART_FIELDS ? rule.field : "rating";
  build(rule);
  field.addEventListener("change", () => build({}));
  row.read = () => ({ field: field.value, op: op.value, value: value.read() });
  row.append(field, op, holder, remove);
  return row;
}

function smartFolderRow(path) {
  const row = document.createElement("div");
  row.className = "smart-folder";
  row.dataset.path = path;
  const code = Object.assign(document.createElement("code"), { textContent: path, title: path });
  const remove = Object.assign(document.createElement("button"), { type: "button", className: "small", textContent: "×", title: "Don't look here" });
  remove.addEventListener("click", () => row.remove());
  row.append(code, remove);
  return row;
}

// Make (name null) or change a smart collection.
async function editSmartCollection(name) {
  const existing = name ? coll.smart.find((c) => c.name === name) : null;
  const c = existing ?? { name: "", folders: lib.dir ? [lib.dir] : [], match: "all", rules: [{ field: "rating", op: ">=", value: 3 }] };
  const dlg = $("smart-dialog");
  $("smart-title").textContent = existing ? `Edit “${existing.name}”` : "New smart collection";
  $("smart-name").value = c.name;
  $("smart-match").value = c.match ?? "all";
  $("smart-folders").replaceChildren(...c.folders.map(smartFolderRow));
  $("smart-rules").replaceChildren(...c.rules.map(smartRuleRow));
  $("smart-error").textContent = "Photos are found by these rules each time the collection is shown, in the folders above and their subfolders.";
  $("smart-add-folder").onclick = async () => {
    const dir = await invoke("library_pick_folder");
    if (dir) $("smart-folders").append(smartFolderRow(dir));
  };
  $("smart-add-rule").onclick = () => $("smart-rules").append(smartRuleRow());
  const read = () => ({
    format: "ambrotype-smart-collection/1",
    name: $("smart-name").value.trim(),
    folders: [...$("smart-folders").children].map((r) => r.dataset.path),
    match: $("smart-match").value,
    rules: [...$("smart-rules").children].map((r) => r.read()),
  });
  // Save checks first and keeps the dialog open on a problem.
  $("smart-save").onclick = async (e) => {
    e.preventDefault();
    const collection = read();
    try {
      await invoke("smart_save", { collection, previous: existing?.name ?? null });
      dlg.close("ok");
      await loadCollections();
      await showSmartCollection(collection.name);
    } catch (err) {
      $("smart-error").textContent = String(err);
    }
  };
  dlg.showModal();
}
