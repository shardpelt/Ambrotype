// SPDX-License-Identifier: GPL-3.0-or-later
// The library's other views: Timeline and Map; placing a photo on the map;
// the slideshow.
// Run: ambrotype --ui-test crates/amb-gui/tests/ui/browse.js photo.dng
// (with a folder "more" holding a photo next to it: amb testimage ramp more/other.dng)

await t.step("Open folder shows a folder in the library", async () => {
  await t.until("the photo", () => ui.info, 60000);
  t.expect(t.visible("#btn-open-folder-empty") || !t.visible("#empty"), "the button on the start screen");
  await openFolder(dirOf(ui.info.path));
  await t.until("the grid", () => libraryOn() && lib.photos.length > 0);
});

await t.step("the library has no editor top bar, but keeps the tasks", async () => {
  t.expect(document.querySelector(".topbar").getBoundingClientRect().height === 0, "the top bar gone");
  t.expect($("lib-task-slot").contains($("task-bar")) && t.visible("#task-bar"), "the tasks in the library bar");
});

await t.step("Timeline opens", async () => {
  t.click('#lib-views [data-view="timeline"]');
  await t.until("the timeline", () => document.querySelector(".tl-months, .browse-empty"));
  t.log(document.querySelector(".tl-head b")?.textContent ?? "no dates");
});

await t.step("a photo dropped on the map gets a place", async () => {
  t.click('#lib-views [data-view="map"]');
  await t.until("the map", () => document.querySelector("#library-view .map"));
  const p = libPhoto(ui.info.path);
  await placePhotos([p.path], "48.858400,2.294500");
  await t.until("a pin", () => document.querySelectorAll(".map-pin").length > 0);
  const listed = await t.invoke("library_list", { dir: lib.dir, more: [], subfolders: false });
  t.expect(listed.find((q) => q.path === p.path)?.gps?.[0] === 48.8584, "the place in the sidecar");
});

await t.step("Remove place takes it out again", async () => {
  select(ui.info.path, "only");
  await placePhotos([ui.info.path], "");
  t.expect(!libPhoto(ui.info.path).gps, "no place");
  t.key("Escape");
  await t.until("the grid", () => cull.view === "grid");
});

await t.step("dragging the grid moves through it; a drag doesn't select", async () => {
  const grid = $("library-grid");
  grid.scrollTop = 0;
  librarySelectNone();
  // Folder tiles come first; the photo may be a row further down.
  revealPhoto(lib.shown[0].path);
  // The grid redraws its rows after a change; add room to scroll after that.
  await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
  const spacer = Object.assign(document.createElement("div"), { style: "height: 4000px; grid-column: 1 / -1" });
  grid.append(spacer);
  const start = grid.scrollTop;
  const cell = grid.querySelector(".lib-cell:not(.lib-dir)");
  const at = cell.getBoundingClientRect();
  const ev = (type, y) => cell.dispatchEvent(new PointerEvent(type, { bubbles: true, clientX: at.left + 20, clientY: y, pointerId: 9, button: 0, pointerType: "mouse" }));
  ev("pointerdown", at.top + 200);
  ev("pointermove", at.top + 180);
  ev("pointermove", at.top - 100);
  ev("pointerup", at.top - 100);
  cell.dispatchEvent(new MouseEvent("click", { bubbles: true }));
  await new Promise((r) => setTimeout(r, 50));
  const moved = grid.scrollTop - start;
  spacer.remove();
  t.expect(moved > 100, `moved down: ${moved}`);
  t.expect(lib.selected.size === 0, "nothing selected");
});

await t.step("the grid shows the folders inside this one first, as tiles; a click opens one", async () => {
  const dir = lib.dir;
  const tile = (name) => [...$("library-grid").querySelectorAll(".lib-dir")].find((c) => c.querySelector(".lib-name").textContent === name);
  await t.until("the more tile", () => tile("more"));
  const cells = [...$("library-grid").querySelectorAll(".lib-cell")];
  t.expect(cells.indexOf(tile("more")) < cells.findIndex((c) => !c.classList.contains("lib-dir")), "before the photos");
  t.expect(tile("more").querySelector(".lib-dir-count").textContent === "1 photo", tile("more").textContent);
  t.click(tile("more").querySelector(".lib-frame"));
  await t.until("the folder more", () => lib.dir !== dir && lib.dir.endsWith("more") && lib.photos.length === 1);
  await t.until("its tiles: only ..", () => lib.dirs.length === 1 && tile(".. (up)"));
  t.click(tile(".. (up)").querySelector(".lib-frame"));
  await t.until("back up", () => lib.dir.replace(/[\\/]+$/, "") === dir.replace(/[\\/]+$/, ""));
  await t.until("the more tile again", () => tile("more"));
});

await t.step("double-click a folder in the tree: it closes, and opens again", async () => {
  // A stand-in library folder with one folder inside.
  const saved = { roots: tree.roots, folders: tree.folders, open: new Set(tree.open) };
  tree.roots = [{ path: "/photos", offline: false }];
  tree.folders = [{ folder: "/photos/trip", photos: 3 }];
  tree.open = new Set(["/photos"]);
  renderFolderTree();
  const row = () => [...$("tree-list").querySelectorAll(".tree-row")].find((r) => r.dataset.path === "/photos");
  const dbl = () => row().querySelector(".tree-name").dispatchEvent(new MouseEvent("dblclick", { bubbles: true }));
  t.expect($("tree-list").textContent.includes("trip"), "trip shown");
  dbl();
  t.expect(!$("tree-list").textContent.includes("trip"), "closed");
  dbl();
  t.expect($("tree-list").textContent.includes("trip"), "open again");
  Object.assign(tree, saved);
  renderFolderTree();
});

await t.step("Folders | All photos, and Ctrl-click shows more folders together", async () => {
  const dir = lib.dir;
  await setTreeMode(true);
  t.expect(lib.smart === ALL_PHOTOS && $("tree-mode-all").classList.contains("on"), "All photos");
  t.expect($("lib-sort").value === "captured" && $("lib-reverse").checked, "newest first");
  await setTreeMode(false);
  t.expect(!lib.smart && lib.dir === dir && $("tree-mode-folders").classList.contains("on"), "back to the folder");
  const one = lib.photos.length;
  // CI makes it: a folder "more" next to the test photo, with one photo.
  const parent = `${dir}${dir.includes("\\") ? "\\" : "/"}more`;
  await toggleTreeFolder(parent);
  t.expect(lib.more.includes(parent) && lib.photos.length > one, `two folders: ${lib.photos.length} photos`);
  await toggleTreeFolder(parent);
  t.expect(!lib.more.includes(parent) && lib.photos.length === one, "one folder again");
  $("lib-sort").value = "name";
  $("lib-reverse").checked = false;
  renderLibrary();
});

await t.step("a Lightroom catalogue opens in place: its folder and collection, with its edits, nothing written", async () => {
  const dir = lib.dir;
  const sep = dir.includes("\\") ? "\\" : "/";
  // CI makes it (amb lrcat sample): more/other.dng rated 4, Red, Exposure +1.
  const catalogue = `${dir}${sep}more${sep}Sample.lrcat`;
  t.expect(t.visible("#tree-lr-add"), "the Lightroom + in the Folders panel");
  // Left open by a run that stopped: closed first.
  for (const c of await invoke("lightroom_catalogues")) await invoke("lightroom_close", { path: c.catalogue });
  let opened = null;
  try {
    opened = await invoke("lightroom_open", { path: catalogue });
  } catch (e) {
    t.log(`no sample catalogue (amb lrcat sample ${catalogue} …): ${e}`);
    return;
  }
  t.expect(opened.photos === 1 && opened.collections.length === 1, `${opened.photos} photo, ${opened.collections.length} collection`);
  await loadLightroom();
  const rows = [...$("tree-lr-list").querySelectorAll(".tree-row")];
  t.expect(rows.length === 3, `catalogue, folder and collection: ${rows.map((r) => r.textContent).join(" | ")}`);
  // Its folder: the photo with Lightroom's rating, label and edit.
  rows[1].click();
  await t.until("its folder", () => lib.photos.length === 1 && lib.photos[0].path.endsWith("other.dng"));
  const p = lib.photos[0];
  t.expect(p.rating === 4 && p.label === "Red" && p.edited, `from the catalogue: ${JSON.stringify({ rating: p.rating, label: p.label, edited: p.edited })}`);
  // Its collection, shown like one of ours.
  [...$("tree-lr-list").querySelectorAll(".tree-row")][2].click();
  await t.until("the collection", () => lib.lightroom?.name === "Selects" && lib.photos.length === 1 && lib.photos[0].path.endsWith("other.dng"));
  t.expect($("library-folder").textContent === "Lightroom: Selects", $("library-folder").textContent);
  // Closed, the photo is as its own files say.
  await invoke("lightroom_close", { path: opened.catalogue });
  await loadLightroom();
  t.expect($("tree-lr-list").children.length === 0 && t.visible("#tree-lr-empty"), "closed");
  await loadLibrary(dir, []);
});

await t.step("Sync negative roll asks how: scenes, odd frames, a baseline frame", async () => {
  const parent = `${lib.dir}${lib.dir.includes("\\") ? "\\" : "/"}more`;
  await toggleTreeFolder(parent);
  await t.until("both folders", () => lib.shown.length >= 2);
  lib.selected = new Set(lib.shown.map((p) => p.path));
  lib.focus = ui.info.path;
  const flow = syncNegativeRoll();
  await t.until("the dialog", () => $("roll-dialog").open);
  t.expect($("roll-scenes").checked && $("roll-odd").checked && !$("roll-own-midtone").checked, "scenes and odd frames on");
  t.expect(!$("roll-baseline").disabled && $("roll-baseline-label").textContent.includes(fileName(ui.info.path)), $("roll-baseline-label").textContent);
  // Cancelled: the test photos are not negatives, and later scripts use them.
  $("roll-dialog").close("cancel");
  await flow;
  t.expect(!ui.settings["amb:NegativeConvert"], "nothing changed");
  await toggleTreeFolder(parent);
  lib.selected.clear();
});

await t.step("Best in each burst: the best marked with why, the rest dimmed; Done clears it", async () => {
  const parent = `${lib.dir}${lib.dir.includes("\\") ? "\\" : "/"}more`;
  await toggleTreeFolder(parent);
  await t.until("both folders", () => lib.shown.length >= 2);
  renderLibrary();
  const paths = lib.shown.map((p) => p.path).filter((p) => lib.cells.has(p)).slice(0, 2);
  t.expect(paths.length === 2, `two photos on screen: ${lib.shown.length} shown, ${lib.cells.size} drawn`);
  // The real search: these two weren't taken in one burst.
  const found = await t.invoke("library_best_in_bursts", { paths, burstSeconds: 2 });
  t.expect(Array.isArray(found.bursts) && found.faces === false, JSON.stringify(found));
  // A burst as it would come back.
  bcull.bursts = [[...paths]];
  bcull.notes = new Map([[paths[0], "sharpest"], [paths[1], "soft"]]);
  bcull.faces = false;
  setCull(true);
  const cell = (p) => lib.cells.get(p);
  t.expect(cell(paths[0]).classList.contains("cull-best") && cell(paths[1]).classList.contains("cull-rest"), "best marked, the rest dimmed");
  t.expect(cell(paths[0]).querySelector(".lib-cull").textContent.includes("sharpest"), "says why");
  t.expect(t.visible("#cull-bar") && $("cull-text").textContent.includes("YuNet"), $("cull-text").textContent);
  cell(paths[1]).querySelector(".lib-cull").click();
  t.expect(bcull.best.has(paths[1]) && cell(paths[0]).classList.contains("cull-rest"), "a click keeps that one instead");
  t.click("#cull-done");
  t.expect(!t.visible("#cull-bar") && !cell(paths[0]).classList.contains("cull-rest") && !cell(paths[1]).querySelector(".lib-cull"), "Done clears it");
  t.expect(KEY_ACTIONS.some((a) => a.id === "best-in-bursts"), "a command, for search");
  await toggleTreeFolder(parent);
});

await t.step("Contact sheet: the selection as film strips on a page", async () => {
  lib.selected = new Set(lib.shown.map((p) => p.path));
  const flow = contactSheetFlow();
  await t.until("the dialog", () => $("contact-dialog").open);
  $("contact-title").value = "Test roll";
  $("contact-dialog").close("ok");
  $("toast").hidden = true;
  await flow;
  t.expect($("toast").textContent.startsWith("Saved the contact sheet"), `the note: ${$("toast").textContent}`);
  lib.selected.clear();
});

await t.step("Smart search without a search model says where to get one; the grid stays as it was", async () => {
  const before = lib.shown.length;
  $("lib-search").value = "a ship at sea";
  $("toast").hidden = true;
  await smartSearch();
  t.expect(!lib.found && $("toast").textContent.includes("AI models"), `the note: ${$("toast").textContent}`);
  $("lib-search").value = "";
  renderLibrary();
  t.expect(lib.shown.length === before, "the same photos shown");
});

await t.step("right-click in the library: a preset onto the selected photos, one step each", async () => {
  lib.selected = new Set(lib.shown.map((p) => p.path));
  const paths = [...lib.selected];
  const cell = document.querySelector(".lib-cell:not(.lib-dir)");
  cell.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 60, clientY: 60 }));
  const items = [...document.querySelectorAll(".ctx-menu button")].map((b) => b.textContent);
  document.querySelector(".ctx-menu")?.remove();
  t.expect(items.some((i) => i.startsWith("Apply a preset")) && items.some((i) => i.startsWith("Export")), items.join(", "));
  // A preset made for the test (a fresh setup has none): Exposure +1.
  await run([{ cmd: "set_param", name: "Exposure2012", value: 1 }], "test");
  ui.presets = await t.invoke("save_preset", { name: "Test brighter", group: "Tests" });
  await run([{ cmd: "undo" }]);
  const flow = presetsOnPhotos(paths);
  await t.until("the picker", () => $("preset-pick-dialog").open);
  const first = $("preset-pick-list").querySelector("button");
  t.expect(first, "presets to pick from");
  first.click();
  await flow;
  t.expect($("toast").textContent.includes("applied to"), `the note: ${$("toast").textContent}`);
  // Take it off again: later steps and scripts use the same photos.
  // The open photo with the editor's own undo; the others in their files.
  const others = paths.filter((p) => p !== ui.info.path);
  if (others.length) await t.invoke("apply_to_photos", { paths: others, label: "undo", commands: [{ cmd: "undo" }] });
  if (paths.includes(ui.info.path)) await run([{ cmd: "undo" }]);
  t.expect(!ui.settings.Exposure2012, `the photo as it was: ${ui.settings.Exposure2012}`);
  lib.selected.clear();
});

await t.step("Convert raws: Like … picks which edited photo to match", async () => {
  const parent = `${lib.dir}${lib.dir.includes("\\") ? "\\" : "/"}more`;
  await toggleTreeFolder(parent);
  await t.until("both folders", () => lib.shown.length >= 2);
  const [a, b] = lib.shown.slice(0, 2);
  const was = [a.edited, b.edited];
  a.edited = b.edited = true;
  lib.selected.clear();
  lib.focus = a.path;
  const flow = convertRawsFlow();
  await t.until("the dialog", () => $("convert-dialog").open);
  const picks = [...$("convert-like-pick").querySelectorAll("button")];
  t.expect(picks.length === 2 && picks[0].classList.contains("on"), `${picks.length} to pick from`);
  picks[1].click();
  t.expect($("convert-like-label").textContent.includes(fileName(b.path)) && picks[1].classList.contains("on"), $("convert-like-label").textContent);
  t.expect($("convert-dialog").querySelector("form").convert_look.value === "like", "picking one chooses Like");
  $("convert-dialog").querySelector("form").convert_look.value = "camera";
  $("convert-dialog").close("cancel");
  await flow;
  [a.edited, b.edited] = was;
  await toggleTreeFolder(parent);
});

await t.step("Convert raws: the photos shown to JPEGs, on Auto", async () => {
  lib.selected.clear();
  const flow = convertRawsFlow();
  await t.until("the dialog", () => $("convert-dialog").open);
  t.expect($("convert-like").disabled || lib.focus, "Like needs a photo picked first");
  $("convert-dialog").querySelector("form").convert_look.value = "auto";
  // The test photos' own edits stay as they are, for the later steps and scripts.
  $("convert-edits").checked = false;
  $("convert-dialog").close("ok");
  $("toast").hidden = true;
  await flow;
  t.expect($("toast").textContent.includes("JPEG") && !$("toast").classList.contains("error"), `the note: ${$("toast").textContent}`);
});

await t.step("Rated photos shows every rated photo, best first", async () => {
  select(ui.info.path, "only");
  await libraryChange({ rating: 4 });
  $("lib-source").value = RATED;
  $("lib-source").dispatchEvent(new Event("change"));
  await t.until("all photos", () => lib.smart === ALL_PHOTOS && lib.photos.length > 0, 30000);
  t.expect($("lib-filter-rating").value === "1" && $("lib-sort").value === "rating", "one star and up, by rating");
  t.expect(lib.shown.every((p) => (p.rating ?? 0) >= 1), "only rated photos");
  t.expect($("lib-source").value === RATED, "the list says Rated photos");
  await showCollection("");
  $("lib-filter-rating").value = "0";
  $("lib-sort").value = "name";
  renderLibrary();
  await t.until("the folder", () => libPhoto(ui.info.path));
  select(ui.info.path, "only");
  await libraryChange({ rating: 0 });
});

await t.step("Auto-advance: a rating from the keyboard goes on to the next photo", async () => {
  const parent = `${lib.dir}${lib.dir.includes("\\") ? "\\" : "/"}more`;
  await toggleTreeFolder(parent);
  const order = lib.shown.map((p) => p.path);
  t.expect(order.length >= 2, `two photos: ${order.length}`);
  $("settings-auto-advance").checked = true;
  $("settings-auto-advance").dispatchEvent(new Event("change"));
  select(order[0], "only");
  await rateKey(2);
  await t.until("the next photo selected", () => lib.focus === order[1] && libraryTargets().length === 1);
  t.expect(libPhoto(order[0]).rating === 2, "the first rated");
  $("settings-auto-advance").checked = false;
  $("settings-auto-advance").dispatchEvent(new Event("change"));
  select(order[0], "only");
  await rateKey(0);
  t.expect(lib.focus === order[0], "off: it stays");
  await toggleTreeFolder(parent);
  select(ui.info.path, "only");
});

await t.step("the library runs a slideshow; Esc ends it", async () => {
  t.key("Ctrl+Enter");
  await t.until("the Slideshow dialog", () => $("slideshow-dialog").open);
  const f = $("slideshow-form").elements;
  f.fullscreen.checked = false;
  f.effect.value = "random";
  f.seconds.value = 1;
  t.click("#slideshow-start");
  await t.until("a photo on screen", () => document.querySelector(".slideshow .ss-img.shown")?.src);
  t.key(" ");
  t.expect(slides.paused, "Space pauses");
  t.key("Escape");
  t.expect(!document.querySelector(".slideshow") && !slides.on, "Esc ends it");
});

await t.step("Develop opens the library's photo, from another folder too", async () => {
  const first = ui.info.path;
  const parent = `${lib.dir}${lib.dir.includes("\\") ? "\\" : "/"}more`;
  await toggleTreeFolder(parent);
  const other = lib.shown.find((p) => p.path !== first);
  t.expect(other, "a second photo shown");
  select(other.path, "only");
  await switchTask("develop");
  await t.until("the other photo in the editor", () => !libraryOn() && ui.info?.path === other.path, 60000);
  await switchTask("library");
  await libraryOpen(first);
  await t.until("the first photo again", () => ui.info?.path === first, 60000);
  await switchTask("library");
  if (lib.more.includes(parent)) await toggleTreeFolder(parent);
  t.expect(libraryOn() && !lib.more.length, "back in the library, one folder");
});

await t.step("the editor gets its top bar back", async () => {
  await toggleLibrary(false);
  t.expect(document.querySelector(".topbar").getBoundingClientRect().height > 30, "the top bar");
  t.expect(document.querySelector(".topbar").contains($("task-bar")), "the tasks in it");
  t.key("Ctrl+Enter");
  t.expect(!$("slideshow-dialog").open, "no slideshow in the editor");
});
