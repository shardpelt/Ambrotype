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
  // The grid redraws its rows after a change; add room to scroll after that.
  await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
  const spacer = Object.assign(document.createElement("div"), { style: "height: 4000px; grid-column: 1 / -1" });
  grid.append(spacer);
  const cell = grid.querySelector(".lib-cell");
  const at = cell.getBoundingClientRect();
  const ev = (type, y) => cell.dispatchEvent(new PointerEvent(type, { bubbles: true, clientX: at.left + 20, clientY: y, pointerId: 9, button: 0, pointerType: "mouse" }));
  ev("pointerdown", at.top + 200);
  ev("pointermove", at.top + 180);
  ev("pointermove", at.top - 100);
  ev("pointerup", at.top - 100);
  cell.dispatchEvent(new MouseEvent("click", { bubbles: true }));
  await new Promise((r) => setTimeout(r, 50));
  const moved = grid.scrollTop;
  spacer.remove();
  t.expect(moved > 100, `moved down: ${moved}`);
  t.expect(lib.selected.size === 0, "nothing selected");
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

await t.step("the editor gets its top bar back", async () => {
  await toggleLibrary(false);
  t.expect(document.querySelector(".topbar").getBoundingClientRect().height > 30, "the top bar");
  t.expect(document.querySelector(".topbar").contains($("task-bar")), "the tasks in it");
  t.key("Ctrl+Enter");
  t.expect(!$("slideshow-dialog").open, "no slideshow in the editor");
});
