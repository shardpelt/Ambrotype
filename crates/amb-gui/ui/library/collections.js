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
    showError(e);
  }
  if (coll.target && !coll.list.some((c) => c.name === coll.target)) coll.target = null;
  renderCollectionControls();
}

function renderCollectionControls() {
  const option = (value, text, own = false) => el("option", { value: value, translate: own ? "no" : null }, text);
  const src = $("lib-source");
  const folder = lib.dir ? t("Folder: {name}", { name: fileName(lib.dir.replace(/[\\/]+$/, "")) || lib.dir }) : t("Folder");
  // Collections not in a set first, then one group per collection set.
  const sets = [...new Set(coll.list.map((c) => c.set ?? ""))].sort((a, b) => (b === "") - (a === "") || a.localeCompare(b));
  const inSets = sets.map((set) => {
    const g = el("optgroup", { label: set ? `▸ ${set}` : t("Collections"), translate: set ? "no" : null });
    g.append(...coll.list.filter((c) => (c.set ?? "") === set).map((c) => option(c.name, `${c.name} (${c.count})`, true)));
    return g;
  });
  const smart = el("optgroup", { label: t("Smart collections") });
  smart.append(...coll.smart.map((c) => option(SMART + c.name, `⚙ ${c.name}`, true)), option(NEW_SMART, t("New smart collection…")));
  src.replaceChildren(option("", folder), option(ALL_PHOTOS, t("All photos")), option(RATED, t("★ Rated photos, best first")), ...inSets, smart);
  const rated = lib.smart === ALL_PHOTOS && $("lib-filter-rating").value !== "0" && $("lib-filter-rating").value !== "unrated" && $("lib-sort").value === "rating";
  src.value = rated ? RATED : lib.smart === ALL_PHOTOS ? ALL_PHOTOS : lib.smart ? SMART + lib.smart : lib.collection ?? "";
  const add = $("lib-add-to");
  add.replaceChildren(...coll.list.map((c) => option(c.name, c.name, true)), option(NEW_COLLECTION, t("New collection…")));
  add.value = coll.target ?? coll.list[0]?.name ?? NEW_COLLECTION;
  const viewing = Boolean(lib.collection) && !lib.smart;
  for (const id of ["lib-remove-from", "lib-coll-rename", "lib-coll-set"]) $(id).hidden = !viewing;
  const ownSmart = lib.smart && lib.smart !== ALL_PHOTOS;
  $("lib-coll-delete").hidden = !viewing && !ownSmart;
  $("lib-smart-edit").hidden = !ownSmart;
  $("lib-subfolders").disabled = Boolean(lib.collection || lib.smart);
  if (viewing) $("lib-remove-from").textContent = t("Remove from “{name}”", { name: lib.collection });
}

// Show a collection in the library ("" for the folder).
async function showCollection(name) {
  lib.lightroom = null;
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
    if (view.missing.length) {
      const name = fileName(view.missing[0]);
      toast(view.missing.length === 1
        ? t("{n} photo in it was moved or deleted: {name}", { n: 1, name })
        : t("{n} photos in it were moved or deleted: {name}…", { n: view.missing.length, name }), true);
    }
    renderLibrary();
  } catch (e) {
    showError(e);
  }
  renderCollectionControls();
}

// Add photos to a collection; NEW_COLLECTION asks for a name.
async function addToCollection(paths, name = $("lib-add-to").value) {
  if (!paths.length) return false;
  if (!name || name === NEW_COLLECTION) {
    name = await askName(t("New collection"), t("Collections hold photos from any folders. They are kept in Ambrotype's settings folder, not next to the photos."));
    if (!name) return true;
  }
  try {
    const c = await invoke("collection_add", { name, paths });
    coll.target = c.name;
    status(t("Added {photos} to “{name}” ({count} in it)", { photos: plural(paths.length, "photo"), name: c.name, count: c.count }));
    await loadCollections();
    if (lib.collection === c.name) await showCollection(c.name);
  } catch (e) {
    showError(e);
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
    status(t("Removed {photos} from “{name}” (the files stay)", { photos: plural(paths.length, "photo"), name: lib.collection }));
    lib.selected.clear();
    await loadCollections();
    await showCollection(lib.collection);
  } catch (e) {
    showError(e);
  }
}

async function renameCollection() {
  if (!lib.collection) return;
  const to = await askName(t("Rename collection"), "", lib.collection);
  if (!to || to === lib.collection) return;
  try {
    const c = await invoke("collection_rename", { from: lib.collection, to });
    if (coll.target === lib.collection) coll.target = c.name;
    lib.collection = c.name;
    await loadCollections();
    $("library-folder").textContent = "";
    $("lib-drop-folders").hidden = true;
  } catch (e) {
    showError(e);
  }
}

// Lightroom's collection sets: a collection is filed under a set's name.
async function moveCollectionToSet() {
  if (!lib.collection) return;
  const current = coll.list.find((c) => c.name === lib.collection)?.set ?? "";
  const names = [...new Set(coll.list.map((c) => c.set).filter(Boolean))];
  const hint = names.length
    ? t("Sets group collections in the Show menu. Sets so far: {names}. Leave empty to take it out of its set.", { names: names.join(", ") })
    : t("Sets group collections in the Show menu. Leave empty to take it out of its set.");
  const set = await askName(t("Collection set"), hint, current, true);
  if (set === null || set === undefined || set === current) return;
  try {
    await invoke("collection_set", { name: lib.collection, set });
    await loadCollections();
  } catch (e) {
    showError(e);
  }
}

async function deleteCollection() {
  if (!lib.collection || !await askYes(t("Delete the collection “{name}”? Its photos stay where they are.", { name: lib.collection }))) return;
  try {
    if (lib.smart) await invoke("smart_delete", { name: lib.smart });
    else await invoke("collection_delete", { name: lib.collection });
    status(t("Deleted the collection “{name}”", { name: lib.collection }));
    await loadCollections();
    await showCollection("");
  } catch (e) {
    showError(e);
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
    store.setText("amb.lib.subfolders", $("lib-subfolders").checked ? "1" : "");
    if (lib.dir && !lib.collection) loadLibrary(lib.dir);
  });
  $("lib-subfolders").checked = store.text("amb.lib.subfolders") === "1";
  loadCollections();
}

// ---------------------------------------------------------------- smart collections

// Worked out when shown: the photos under its folders that meet its rules.
async function showSmartCollection(name) {
  const all = name === ALL_PHOTOS;
  $("library-count").textContent = all ? t("Reading the catalogue…") : t("Looking through the folders…");
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
    showError(e);
  }
  renderCollectionControls();
}

// What each field can test, and how its value is entered.
const SMART_IS = [["=", tKey("is")], ["!=", tKey("is not")]];
const SMART_HAS = [["contains", tKey("contains")], ["!contains", tKey("doesn't contain")]];
const SMART_RANGE = [[">=", tKey("is at least")], ["<=", tKey("is at most")]];
const SMART_FIELDS = {
  rating: { label: tKey("Rating"), ops: [[">=", tKey("is at least")], ["<=", tKey("is at most")], ...SMART_IS],
    values: [0, 1, 2, 3, 4, 5].map((v) => [v, v ? "★".repeat(v) : tKey("unrated")]) },
  label: { label: tKey("Colour label"), ops: SMART_IS, values: [["", tKey("none")], ...LABELS.map((l) => [l, l])] },
  flag: { label: tKey("Flag"), ops: SMART_IS, values: [["pick", tKey("pick")], ["reject", tKey("reject")], ["", tKey("none")]] },
  keyword: { label: tKey("Keyword"), ops: SMART_HAS, text: true },
  name: { label: tKey("File name"), ops: SMART_HAS, text: true },
  edited: { label: tKey("Edited"), ops: [["=", tKey("is")]], values: [[true, tKey("yes")], [false, tKey("no")]] },
  captured: { label: tKey("Capture date"), ops: [["after", tKey("is on or after")], ["before", tKey("is before")]], date: true },
  camera: { label: tKey("Camera"), ops: SMART_HAS, text: true },
  lens: { label: tKey("Lens"), ops: SMART_HAS, text: true },
  iso: { label: tKey("ISO"), ops: SMART_RANGE, number: true },
  focal: { label: tKey("Focal length (mm)"), ops: SMART_RANGE, number: true },
};

function smartRuleRow(rule = { field: "rating", op: ">=", value: 3 }) {
  const row = el("div", { class: "smart-rule" });
  const field = el("select");
  for (const [k, f] of Object.entries(SMART_FIELDS)) field.append(new Option(t(f.label), k));
  const op = el("select");
  const holder = el("span");
  holder.style.display = "contents";
  const remove = el("button", { type: "button", class: "small", title: t("Remove this rule") }, "×");
  remove.addEventListener("click", () => row.remove());
  let value = null;
  const build = (r) => {
    const f = SMART_FIELDS[field.value];
    op.replaceChildren(...f.ops.map(([v, text]) => new Option(t(text), v)));
    if (f.ops.some(([v]) => v === r.op)) op.value = r.op;
    if (f.values) {
      value = el("select");
      f.values.forEach(([v, text], i) => value.append(new Option(t(text), String(i))));
      const at = f.values.findIndex(([v]) => v === r.value);
      value.value = String(Math.max(0, at));
      value.read = () => f.values[Number(value.value)][0];
    } else {
      value = el("input");
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
  const row = el("div", { class: "smart-folder" });
  row.dataset.path = path;
  const code = el("code", { title: path, translate: "no" }, path);
  const remove = el("button", { type: "button", class: "small", title: t("Don't look here") }, "×");
  remove.addEventListener("click", () => row.remove());
  row.append(code, remove);
  return row;
}

// Make (name null) or change a smart collection.
async function editSmartCollection(name) {
  const existing = name ? coll.smart.find((c) => c.name === name) : null;
  const c = existing ?? { name: "", folders: lib.dir ? [lib.dir] : [], match: "all", rules: [{ field: "rating", op: ">=", value: 3 }] };
  const dlg = $("smart-dialog");
  $("smart-title").textContent = existing ? t("Edit “{name}”", { name: existing.name }) : t("New smart collection");
  $("smart-name").value = c.name;
  $("smart-match").value = c.match ?? "all";
  $("smart-folders").replaceChildren(...c.folders.map(smartFolderRow));
  $("smart-rules").replaceChildren(...c.rules.map(smartRuleRow));
  $("smart-error").textContent = t("Photos are found by these rules each time the collection is shown, in the folders above and their subfolders.");
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
