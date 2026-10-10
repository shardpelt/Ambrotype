// SPDX-License-Identifier: GPL-3.0-or-later
// Collections in the library: named lists of photos from any folders, kept
// as small files in the app's config folder (amb-library's collections).
// The library shows a folder or a collection; the panel adds the selection
// to one, and B adds to the last one used (Lightroom's quick collection).
"use strict";

const coll = { list: [], target: null };
const NEW_COLLECTION = "\u0000new";

// Ask for a name in a small dialog; null when cancelled.
async function askName(title, hint = "", value = "") {
  const dlg = $("name-dialog");
  $("name-title").textContent = title;
  $("name-hint").textContent = hint;
  const input = dlg.querySelector("input[name=value]");
  input.value = value;
  dlg.showModal();
  input.select();
  const choice = await new Promise((resolve) => dlg.addEventListener("close", () => resolve(dlg.returnValue), { once: true }));
  const name = input.value.trim();
  return choice === "ok" && name ? name : null;
}

async function loadCollections() {
  try {
    coll.list = await invoke("collections_list");
  } catch (e) {
    coll.list = [];
    toast(String(e), true);
  }
  if (coll.target && !coll.list.some((c) => c.name === coll.target)) coll.target = null;
  renderCollectionControls();
}

function renderCollectionControls() {
  const option = (value, text) => Object.assign(document.createElement("option"), { value, textContent: text });
  const src = $("lib-source");
  const folder = lib.dir ? `Folder: ${fileName(lib.dir.replace(/[\\/]+$/, "")) || lib.dir}` : "Folder";
  const group = Object.assign(document.createElement("optgroup"), { label: "Collections" });
  group.append(...coll.list.map((c) => option(c.name, `${c.name} (${c.count})`)));
  src.replaceChildren(option("", folder), ...(coll.list.length ? [group] : []));
  src.value = lib.collection ?? "";
  const add = $("lib-add-to");
  add.replaceChildren(...coll.list.map((c) => option(c.name, c.name)), option(NEW_COLLECTION, "New collection…"));
  add.value = coll.target ?? coll.list[0]?.name ?? NEW_COLLECTION;
  const viewing = Boolean(lib.collection);
  for (const id of ["lib-remove-from", "lib-coll-rename", "lib-coll-delete"]) $(id).hidden = !viewing;
  if (viewing) $("lib-remove-from").textContent = `Remove from “${lib.collection}”`;
}

// Show a collection in the library ("" for the folder).
async function showCollection(name) {
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
  } catch (e) {
    toast(String(e), true);
  }
}

async function deleteCollection() {
  if (!lib.collection || !confirm(`Delete the collection “${lib.collection}”? Its photos stay where they are.`)) return;
  try {
    await invoke("collection_delete", { name: lib.collection });
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
  $("lib-coll-delete").addEventListener("click", deleteCollection);
  loadCollections();
}
