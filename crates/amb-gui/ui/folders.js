// SPDX-License-Identifier: GPL-3.0-or-later
// The Folders panel beside the library grid: the library folders (whole
// drives too) and their subfolders, with photo counts from the catalogue.
// They're indexed in the background at start, and again when an unplugged
// drive comes back (src/commands/catalog.rs). An unplugged drive's folders
// stay, greyed out; showing one lists its photos as they were last read.
"use strict";

const tree = { roots: [], folders: [], open: new Set(), offline: new Set(), loaded: false };

// How often to look for a library drive that was plugged back in.
const DRIVE_CHECK_MS = 30000;

async function loadFolderTree() {
  try {
    const r = await invoke("library_folders");
    const back = r.roots.filter((x) => !x.offline && tree.offline.has(x.path));
    tree.roots = r.roots;
    tree.folders = r.folders;
    tree.offline = new Set(r.roots.filter((x) => x.offline).map((x) => x.path));
    if (!tree.loaded) for (const x of r.roots) tree.open.add(x.path);
    tree.loaded = true;
    renderFolderTree();
    if (back.length && !r.indexing) invoke("index_library");
  } catch (e) {
    console.warn("library folders:", e);
  }
}

// The folders under each library folder as a tree; folders without photos
// of their own are filled in between, and each counts its subfolders' photos.
function folderTreeNodes() {
  const sep = tree.roots.some((r) => r.path.includes("\\")) ? "\\" : "/";
  return tree.roots.map((root) => {
    const top = { path: root.path, name: root.path, own: 0, children: new Map(), offline: root.offline, root: true };
    for (const f of tree.folders) {
      if (f.folder !== root.path && !f.folder.startsWith(root.path.replace(/[\\/]$/, "") + sep)) continue;
      let node = top;
      const rel = f.folder.slice(root.path.length).split(/[\\/]/).filter(Boolean);
      for (const part of rel) {
        if (!node.children.has(part)) {
          node.children.set(part, { path: node.path.replace(/[\\/]$/, "") + sep + part, name: part, own: 0, children: new Map(), offline: root.offline });
        }
        node = node.children.get(part);
      }
      node.own += f.photos;
    }
    return top;
  });
}

function folderTotal(node) {
  return node.own + [...node.children.values()].reduce((n, c) => n + folderTotal(c), 0);
}

function renderFolderTree() {
  const list = $("tree-list");
  list.replaceChildren(...folderTreeNodes().map(folderTreeItem));
  $("tree-empty").hidden = tree.roots.length > 0;
  syncTreeMode();
}

function folderTreeItem(node) {
  const li = document.createElement("li");
  const row = Object.assign(document.createElement("div"), { className: "tree-row", title: node.path });
  row.classList.toggle("offline", node.offline);
  const shown = !lib.collection && !lib.smart && (lib.dir === node.path || lib.more.includes(node.path));
  row.classList.toggle("current", shown);
  const kids = [...node.children.values()].sort((a, b) => a.name.localeCompare(b.name, undefined, { numeric: true }));
  const open = tree.open.has(node.path);
  const twist = Object.assign(document.createElement("span"), { className: "twist", textContent: kids.length ? (open ? "▾" : "▸") : "" });
  twist.addEventListener("click", (e) => {
    e.stopPropagation();
    if (open) tree.open.delete(node.path);
    else tree.open.add(node.path);
    renderFolderTree();
  });
  const name = Object.assign(document.createElement("span"), { className: "tree-name", textContent: node.root ? node.path.split(/[\\/]/).filter(Boolean).pop() ?? node.path : node.name });
  const count = Object.assign(document.createElement("span"), { className: "tree-count", textContent: folderTotal(node).toLocaleString() });
  row.append(twist, name, count);
  if (node.root) {
    const remove = Object.assign(document.createElement("button"), { type: "button", className: "small tree-remove", textContent: "×", title: "Remove from the library folders" });
    remove.addEventListener("click", (e) => {
      e.stopPropagation();
      removeLibraryFolder(node.path);
    });
    row.append(remove);
  }
  // Ctrl (⌘)-click: show this folder too, or not any more.
  row.addEventListener("click", (e) => (e.ctrlKey || e.metaKey ? toggleTreeFolder(node.path) : showTreeFolder(node.path)));
  li.append(row);
  if (open && kids.length) {
    const ul = document.createElement("ul");
    ul.append(...kids.map(folderTreeItem));
    li.append(ul);
  }
  return li;
}

async function showTreeFolder(path) {
  lib.selected.clear();
  await loadLibrary(path, []);
  renderFolderTree();
}

async function toggleTreeFolder(path) {
  if (!lib.dir || lib.collection || lib.smart) return showTreeFolder(path);
  if (path === lib.dir) {
    // The first folder leaves; the next one shown takes its place.
    if (!lib.more.length) return;
    const [next, ...rest] = lib.more;
    await loadLibrary(next, rest);
  } else {
    const more = lib.more.includes(path) ? lib.more.filter((d) => d !== path) : [...lib.more, path];
    await loadLibrary(lib.dir, more);
  }
  renderFolderTree();
}

// Folders | All photos, like a phone's gallery: every photo the library
// knows, in one timeline, or a folder at a time.
async function setTreeMode(all) {
  if (all) {
    await showCollection(ALL_PHOTOS);
    if (cull.view === "grid") $("lib-sort").value = "captured";
    $("lib-reverse").checked = true;
    renderLibrary();
  } else {
    await showCollection("");
  }
  syncTreeMode();
}

function syncTreeMode() {
  const all = lib.smart === ALL_PHOTOS;
  $("tree-mode-all").classList.toggle("on", all);
  $("tree-mode-folders").classList.toggle("on", !all);
  $("tree-mode-all").setAttribute("aria-selected", all);
  $("tree-mode-folders").setAttribute("aria-selected", !all);
}

async function addLibraryRoot() {
  const dir = await invoke("library_pick_folder");
  if (!dir) return;
  try {
    await invoke("add_library_folder", { path: dir });
    tree.open.add(dir);
    await loadFolderTree();
  } catch (e) {
    toast(String(e), true);
  }
}

async function removeLibraryFolder(path) {
  if (!await askYes(`Remove “${path}” from the library folders? Its photos leave the catalogue; the files stay where they are.`)) return;
  try {
    await invoke("remove_library_folder", { path, forget: true });
    await loadFolderTree();
  } catch (e) {
    toast(String(e), true);
  }
}

function wireFolderTree() {
  $("tree-add").addEventListener("click", addLibraryRoot);
  $("tree-mode-folders").addEventListener("click", () => setTreeMode(false));
  $("tree-mode-all").addEventListener("click", () => setTreeMode(true));
  const progress = $("tree-progress");
  tauri.event.listen("index-progress", (e) => {
    const { folder, done, total } = e.payload;
    progress.hidden = false;
    progress.textContent = `Indexing ${folder.split(/[\\/]/).filter(Boolean).pop() ?? folder}: ${done.toLocaleString()} of ${total.toLocaleString()} photos`;
  });
  tauri.event.listen("index-done", () => loadFolderTree());
  tauri.event.listen("index-finished", () => {
    progress.hidden = true;
    loadFolderTree();
    // A folder shown while its drive was away: show it from the drive now.
    if (libraryOn() && lib.dir && !lib.collection && lib.photos.some((p) => p.offline)) loadLibrary(lib.dir);
  });
  loadFolderTree();
  // Indexed once the window is up, so the first photo opens first.
  setTimeout(() => invoke("index_library"), 3000);
  setInterval(loadFolderTree, DRIVE_CHECK_MS);
}
