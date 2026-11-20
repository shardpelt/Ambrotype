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
  const row = el("div", { class: "tree-row", title: title });
  row.append(
    el("span", { class: "twist" }),
    el("span", { class: "tree-name", translate: "no" }, text),
    el("span", { class: "tree-count" }, count == null ? "" : count.toLocaleString()),
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
    const opened = c.missing
      ? t("Opened in place: {n} photos, {missing} not found. Read from the catalogue, never written.", { n: c.photos.toLocaleString(), missing: c.missing })
      : t("Opened in place: {n} photos. Read from the catalogue, never written.", { n: c.photos.toLocaleString() });
    const about = c.error
      ? `${c.catalogue}\n${t("Can't be read now: {error}", { error: c.error })}`
      : `${c.catalogue}\n${opened}${c.warnings.length ? `\n${c.warnings.join("\n")}` : ""}`;
    const row = lightroomRow(c.name, c.photos, about, () => toggle());
    row.classList.toggle("offline", !!c.error);
    row.querySelector(".twist").textContent = open ? "▾" : "▸";
    const toggle = () => {
      if (open) lrin.open.delete(c.catalogue);
      else lrin.open.add(c.catalogue);
      renderLightroom();
    };
    const close = el("button", { type: "button", class: "small tree-remove", title: t("Close the catalogue (nothing is deleted)") }, "×");
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
        const r = lightroomRow(label, k.photos, t("Lightroom collection: {name}", { name: k.name }), () => showLightroomCollection(c, k.name));
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
    const opened = t("{name} is open in place: {n} photos in {folders}, {collections}. Nothing was imported or written; edits you make here go into the photos' sidecars.", {
      name: c.name, n: c.photos.toLocaleString(), folders: plural(c.folders.length, "folder"), collections: plural(c.collections.length, "collection"),
    });
    toast(`${opened}${c.warnings.length ? ` ${c.warnings.join(". ")}.` : ""}`);
    if (c.folders.length) await showTreeFolder(c.folders[0].path);
  } catch (e) {
    showError(e);
  } finally {
    $("busy").hidden = true;
  }
}

async function closeLightroomCatalogue(c) {
  if (!await askYes(t("Close “{name}”? Its photos stay where they are, and so do sidecars written here.", { name: c.name }))) return;
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
    $("library-folder").textContent = t("Lightroom: {name}", { name });
    $("lib-drop-folders").hidden = true;
    if (view.missing.length) {
      const vars = { n: view.missing.length, name: fileName(view.missing[0]) };
      toast(view.missing.length > 1
        ? t("{n} of its photos weren't found where the catalogue says: {name}…", vars)
        : t("{n} of its photos weren't found where the catalogue says: {name}", vars), true);
    }
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
