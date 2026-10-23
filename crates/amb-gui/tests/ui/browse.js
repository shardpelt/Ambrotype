// SPDX-License-Identifier: GPL-3.0-or-later
// The library's other views: Timeline and Map; placing a photo on the map;
// the slideshow.
// Run: ambrotype --ui-test crates/amb-gui/tests/ui/browse.js photo.dng

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
