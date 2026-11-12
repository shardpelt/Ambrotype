// SPDX-License-Identifier: GPL-3.0-or-later
// The presets block: a row of groups to pick from (All, Favourites, then
// each folder's group) over a grid of tiles, each a small preview of the open
// photo with that preset on top of its edit, and the preset's name. Hover
// shows it on the photo, a click applies it, right-click has the rest.
// Previews are made as tiles come into sight, a few at a time, and kept per
// photo and amount.

const presetGrid = { urls: new Map(), queue: [], busy: false, seen: null, photo: null };
const ALL_PRESETS = "";
const FAVOURITES = "★";

function presetGroupChosen() {
  return store.text("amb.presetGroup") ?? ALL_PRESETS;
}

function renderPresets() {
  const list = $("preset-list");
  const filter = $("preset-filter").value.trim().toLowerCase();
  const favs = ui.presetFavourites ?? new Set();
  const all = presetsShown(presetHide.showing);
  const groups = [...new Set(all.map(presetGroupOf))];
  let chosen = presetGroupChosen();
  if (chosen !== ALL_PRESETS && chosen !== FAVOURITES && !groups.includes(chosen)) chosen = ALL_PRESETS;
  if (chosen === FAVOURITES && !all.some((p) => favs.has(p.path))) chosen = ALL_PRESETS;
  renderPresetGroups(all, groups, chosen, favs);
  const shown = all.filter((p) => (!filter || (p.name + " " + p.group).toLowerCase().includes(filter))
    && (filter || chosen === ALL_PRESETS || (chosen === FAVOURITES ? favs.has(p.path) : (p.group || "Presets") === chosen)));
  // Favourites first.
  shown.sort((a, b) => Number(favs.has(b.path)) - Number(favs.has(a.path)));
  presetGrid.seen?.disconnect();
  presetGrid.queue = [];
  list.replaceChildren();
  if (!shown.length) {
    list.innerHTML = `<p class="hint">${ui.presets.length ? "No match." : "No presets yet. Lightroom's preset folder is used automatically when it exists; or add a folder."}</p>`;
    list.append(presetHiddenFoot() ?? "");
    return;
  }
  presetGrid.photo = ui.info?.path ?? null;
  presetGrid.seen = new IntersectionObserver((entries) => {
    for (const e of entries) {
      if (!e.isIntersecting) continue;
      presetGrid.seen.unobserve(e.target);
      presetGrid.queue.push(e.target);
    }
    pumpPresetThumbs();
  });
  const grid = Object.assign(document.createElement("div"), { className: "preset-grid" });
  for (const p of shown) grid.append(presetTile(p, favs));
  list.append(grid, presetHiddenFoot() ?? "");
}

function renderPresetGroups(all, groups, chosen, favs) {
  const box = $("preset-groups");
  if (!box) return;
  const chips = [[ALL_PRESETS, `All ${all.length}`]];
  if (all.some((p) => favs.has(p.path))) chips.push([FAVOURITES, "★ Favourites"]);
  for (const g of groups) chips.push([g, g]);
  box.hidden = chips.length < 3;
  box.replaceChildren(...chips.map(([value, text]) => {
    const b = Object.assign(document.createElement("button"), { type: "button", className: "preset-chip", textContent: text });
    b.classList.toggle("on", value === chosen);
    b.classList.toggle("preset-dim", presetHide.groups.has(value));
    if (value !== ALL_PRESETS && value !== FAVOURITES) b.addEventListener("contextmenu", (e) => presetGroupMenu(e, value));
    b.addEventListener("click", () => {
      store.setText("amb.presetGroup", value);
      renderPresets();
    });
    return b;
  }));
}

function presetTile(p, favs) {
  const tile = Object.assign(document.createElement("div"), { className: "preset-tile" });
  tile.dataset.path = p.path;
  tile.classList.toggle("preset-dim", presetIsHidden(p));
  const b = Object.assign(document.createElement("button"), { type: "button", className: "preset", title: p.group ? `${p.name} (${p.group})` : p.name });
  const img = Object.assign(document.createElement("img"), { alt: "", draggable: false });
  const url = presetGrid.urls.get(presetThumbKey(p.path));
  if (url) img.src = url;
  b.append(img, Object.assign(document.createElement("span"), { className: "preset-name", textContent: p.name }));
  b.addEventListener("click", () => {
    presetHover(null);
    applyPreset(p);
  });
  b.addEventListener("contextmenu", (e) => {
    presetHover(null);
    const n = folderPhotos().length;
    contextMenu(e, [
      ["Apply", () => applyPreset(p)],
      ["Apply some of its settings…", () => applyPresetPartly(p)],
      ui.info && n > 1 ? [`Apply to all ${n} photos in the folder`, () => applyPresetToFolder(p)] : null,
      ["Show the file", () => invoke("reveal_file", { path: p.path }).catch((err) => toast(String(err), true))],
      ["Share to the hub…", () => sharePresetToHub(p)],
      ...presetHideItems(p),
    ].filter(Boolean));
  });
  b.addEventListener("mouseenter", () => presetHover(p));
  b.addEventListener("mouseleave", () => presetHover(null));
  const on = favs.has(p.path);
  const star = Object.assign(document.createElement("button"), { type: "button", className: `preset-star${on ? " on" : ""}`, textContent: on ? "★" : "☆", title: on ? "Remove from favourites" : "Add to favourites" });
  star.addEventListener("click", () => togglePresetFavourite(p));
  tile.append(b, star);
  if (!url && ui.info) presetGrid.seen.observe(tile);
  return tile;
}

const presetThumbKey = (path) => `${ui.info?.path}|${$("preset-amount").value}|${path}`;

// A few previews at a time, for the tiles in sight.
async function pumpPresetThumbs() {
  if (presetGrid.busy || !presetGrid.queue.length || !ui.info) return;
  presetGrid.busy = true;
  const tiles = presetGrid.queue.splice(0, 6).filter((t) => t.isConnected);
  try {
    const paths = tiles.map((t) => t.dataset.path);
    const urls = paths.length ? await invoke("preset_thumbnails", { paths, amount: Number($("preset-amount").value) }) : [];
    tiles.forEach((t, i) => {
      if (!urls[i]) return;
      presetGrid.urls.set(presetThumbKey(paths[i]), urls[i]);
      t.querySelector("img").src = urls[i];
    });
  } catch {
    // No photo open any more, or a preset that can't be read: the tile keeps its name.
  }
  presetGrid.busy = false;
  pumpPresetThumbs();
}

// Another photo: the previews are made again for it.
function presetsPhotoChanged() {
  if ((ui.info?.path ?? null) === presetGrid.photo) return;
  if (presetGrid.urls.size > 600) presetGrid.urls.clear();
  renderPresets();
}
