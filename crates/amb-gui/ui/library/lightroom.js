// SPDX-License-Identifier: GPL-3.0-or-later
// Lightroom Classic catalogues opened in place, in the Folders panel
// (amb-library's lightroom, src/commands/catalog.rs): each with its folders
// and collections. Their photos show with their Lightroom edits, ratings,
// labels, flags and keywords, read from the catalogue; nothing is imported,
// and the catalogue is never written. An edit here goes into the photo's
// sidecar as usual; a later change in Lightroom shows again.
"use strict";

const lrin = { list: [], open: new Set(), seen: new Set() };

async function loadLightroom() {
  try {
    lrin.list = await invoke("lightroom_catalogues");
    // A catalogue shows its folders and collections when it first appears.
    for (const c of lrin.list) {
      if (!lrin.seen.has(c.catalogue)) lrin.open.add(c.catalogue);
      lrin.seen.add(c.catalogue);
    }
    renderLightroom();
  } catch (e) {
    console.warn("lightroom catalogues:", e);
  }
}

// A folder by its last two names ("2024 / Italy"), the full path on hover.
function lightroomFolderName(path) {
  return path.split(/[\\/]/).filter(Boolean).slice(-2).join(" / ");
}

function lightroomRow(text, count, title, onclick) {
  const row = Object.assign(document.createElement("div"), { className: "tree-row", title });
  row.append(
    Object.assign(document.createElement("span"), { className: "twist" }),
    Object.assign(document.createElement("span"), { className: "tree-name", textContent: text }),
    Object.assign(document.createElement("span"), { className: "tree-count", textContent: count == null ? "" : count.toLocaleString() }),
  );
  row.addEventListener("click", onclick);
  return row;
}

function lightroomItem(row) {
  const li = document.createElement("li");
  li.append(row);
  return li;
}

function renderLightroom() {
  const list = $("tree-lr-list");
  list.replaceChildren(...lrin.list.map((c) => {
    const li = document.createElement("li");
    const open = lrin.open.has(c.catalogue);
    const about = c.error
      ? `${c.catalogue}\nCan't be read now: ${c.error}`
      : `${c.catalogue}\nOpened in place: ${c.photos.toLocaleString()} photos${c.missing ? `, ${c.missing} not found` : ""}. Read from the catalogue, never written.${c.warnings.length ? `\n${c.warnings.join("\n")}` : ""}`;
    const row = lightroomRow(c.name, c.photos, about, () => toggle());
    row.classList.toggle("offline", !!c.error);
    row.querySelector(".twist").textContent = open ? "▾" : "▸";
    const toggle = () => {
      if (open) lrin.open.delete(c.catalogue);
      else lrin.open.add(c.catalogue);
      renderLightroom();
    };
    const close = Object.assign(document.createElement("button"), { type: "button", className: "small tree-remove", textContent: "×", title: "Close the catalogue (nothing is deleted)" });
    close.addEventListener("click", (e) => {
      e.stopPropagation();
      closeLightroomCatalogue(c);
    });
    row.append(close);
    li.append(row);
    if (open && !c.error) {
      const ul = document.createElement("ul");
      for (const f of c.folders) {
        const r = lightroomRow(lightroomFolderName(f.path), f.photos, f.path, () => showTreeFolder(f.path));
        r.classList.toggle("current", !lib.collection && !lib.smart && !lib.lightroom && lib.dir === f.path);
        ul.append(lightroomItem(r));
      }
      for (const k of c.collections) {
        const label = `▤ ${k.set ? `${k.set} / ` : ""}${k.name}`;
        const r = lightroomRow(label, k.photos, `Lightroom collection: ${k.name}`, () => showLightroomCollection(c, k.name));
        r.classList.toggle("current", lib.lightroom?.catalogue === c.catalogue && lib.lightroom?.name === k.name);
        ul.append(lightroomItem(r));
      }
      li.append(ul);
    }
    return li;
  }));
  $("tree-lr-empty").hidden = lrin.list.length > 0;
}

// Open a catalogue in place (`path`, or one chosen now).
async function openLightroomCatalogue(path = null) {
  path ??= await invoke("pick_lrcat");
  if (!path) return;
  $("busy").hidden = false;
  try {
    const c = await invoke("lightroom_open", { path });
    await loadLightroom();
    toast(`${c.name} is open in place: ${c.photos.toLocaleString()} photos in ${plural(c.folders.length, "folder")}, ${plural(c.collections.length, "collection")}. Nothing was imported or written; edits you make here go into the photos' sidecars.${c.warnings.length ? ` ${c.warnings.join(". ")}.` : ""}`);
    if (c.folders.length) await showTreeFolder(c.folders[0].path);
  } catch (e) {
    showError(e);
  } finally {
    $("busy").hidden = true;
  }
}

async function closeLightroomCatalogue(c) {
  if (!await askYes(`Close “${c.name}”? Its photos stay where they are, and so do sidecars written here.`)) return;
  try {
    await invoke("lightroom_close", { path: c.catalogue });
    if (lib.lightroom?.catalogue === c.catalogue) await showCollection("");
    await loadLightroom();
    $("lib-reload").click();
  } catch (e) {
    showError(e);
  }
}

// A catalogue's collection in the grid (read-only: it's Lightroom's).
async function showLightroomCollection(c, name) {
  try {
    const view = await invoke("lightroom_collection", { catalogue: c.catalogue, name });
    lib.collection = null;
    lib.smart = null;
    lib.lightroom = { catalogue: c.catalogue, name };
    lib.photos = view.photos;
    const keep = new Set(lib.photos.map((p) => p.path));
    lib.selected = new Set([...lib.selected].filter((p) => keep.has(p)));
    $("library-folder").textContent = `Lightroom: ${name}`;
    $("lib-drop-folders").hidden = true;
    if (view.missing.length) toast(`${view.missing.length} of its photos weren't found where the catalogue says: ${fileName(view.missing[0])}${view.missing.length > 1 ? "…" : ""}`, true);
    renderLibrary();
    renderLightroom();
    if (typeof renderCollectionControls === "function") renderCollectionControls();
  } catch (e) {
    showError(e);
  }
}

function wireLightroom() {
  $("tree-lr-add").addEventListener("click", () => openLightroomCatalogue());
}
