// SPDX-License-Identifier: GPL-3.0-or-later
// The Map view: dragging a pin moves a photo's place; place names.
// Run: ambrotype --ui-test crates/amb-gui/tests/ui/map-place.js photo.dng

await t.step("a pin dragged on the Map moves the photo's place and keeps it", async () => {
  await t.until("the photo", () => ui.info, 60000);
  await openFolder(dirOf(ui.info.path));
  await t.until("the grid", () => libraryOn() && lib.photos.length > 0);
  t.click('#lib-views [data-view="map"]');
  await t.until("the map", () => document.querySelector("#library-view .map"));
  const path = ui.info.path;
  await placePhotos([path], "48.858400,2.294500");
  await t.until("a one-photo pin", () => document.querySelector(".map-pin.one"));
  const pin = document.querySelector(".map-pin.one");
  const r = pin.getBoundingClientRect();
  const at = (dx, dy) => ({ clientX: r.left + r.width / 2 + dx, clientY: r.top + r.height / 2 + dy, button: 0, pointerId: 9, bubbles: true });
  // Under MAP_DRAG_PX: a click, no move.
  pin.dispatchEvent(new PointerEvent("pointerdown", at(0, 0)));
  pin.dispatchEvent(new PointerEvent("pointerup", at(2, 1)));
  t.expect(Math.abs(libPhoto(path).gps[0] - 48.8584) < 1e-6, "a short press keeps the place");
  pin.dispatchEvent(new PointerEvent("pointerdown", at(0, 0)));
  pin.dispatchEvent(new PointerEvent("pointermove", at(60, 40)));
  pin.dispatchEvent(new PointerEvent("pointerup", at(60, 40)));
  await t.until("the place saved", () => Math.abs(libPhoto(path).gps[1] - 2.2945) > 1e-4);
  const [lat, lon] = libPhoto(path).gps;
  t.expect(lon > 2.2945 && lat < 48.8584, `moved right and down: ${lat}, ${lon}`);
  const listed = await t.invoke("library_list", { dir: lib.dir, more: [], subfolders: false });
  const saved = listed.find((q) => q.path === path)?.gps;
  t.expect(Math.abs(saved[0] - lat) < 1e-5 && Math.abs(saved[1] - lon) < 1e-5, "the sidecar has it");
  await placePhotos([path], "");
});

await t.step("a pin of several photos drags too, and every photo gets the new place", async () => {
  const box = $("lib-subfolders");
  box.checked = true;
  box.dispatchEvent(new Event("change"));
  await t.until("both folders' photos", () => lib.photos.length >= 2);
  const paths = lib.photos.slice(0, 2).map((p) => p.path);
  await placePhotos(paths, "48.858400,2.294500");
  await t.until("a group pin", () => document.querySelector(".map-pin:not(.one)"));
  const pin = document.querySelector(".map-pin:not(.one)");
  const r = pin.getBoundingClientRect();
  const at = (dx, dy) => ({ clientX: r.left + r.width / 2 + dx, clientY: r.top + r.height / 2 + dy, button: 0, pointerId: 9, bubbles: true });
  pin.dispatchEvent(new PointerEvent("pointerdown", at(0, 0)));
  pin.dispatchEvent(new PointerEvent("pointermove", at(60, 40)));
  pin.dispatchEvent(new PointerEvent("pointerup", at(60, 40)));
  await t.until("the places saved", () => paths.every((q) => Math.abs(libPhoto(q).gps[1] - 2.2945) > 1e-4));
  const [a, b] = paths.map((q) => libPhoto(q).gps);
  t.expect(Math.abs(a[0] - b[0]) < 1e-6 && Math.abs(a[1] - b[1]) < 1e-6 && a[1] > 2.2945, "both at the new place, right of the old");
  const before = lib.selected.size;
  // The pin is drawn again at its new place: the click lands on that one.
  await t.until("the pin redrawn", () => document.querySelector(".map-pin:not(.one)"));
  document.querySelector(".map-pin:not(.one)").dispatchEvent(new MouseEvent("click", { bubbles: true }));
  t.expect(lib.selected.size === before, "the click that ends a drag is not a selection");
  await placePhotos(paths, "");
  box.checked = false;
  box.dispatchEvent(new Event("change"));
  await t.until("one folder again", () => lib.photos.length === 1 || !lib.photos.some((p) => p.path.includes("more")));
});

await t.step("Add place names is a key action and a button on the Map; names show and are searchable", async () => {
  t.expect(KEY_ACTIONS.some((a) => a.id === "place-names" && a.when === "library"), "the key action");
  t.expect(document.querySelector(".map-bar [data-place-names]"), "the button on the Map's bar");
  // No network: the names are put in as the lookup would leave them.
  const p = libPhoto(ui.info.path);
  photoChanged(p.path, { city: "Paris", state: null, country: "France" });
  t.expect(placeNamesText(libPhoto(p.path)) === "Paris, France", "City, Country");
  t.expect(libPhoto(p.path).city === "Paris", "the library copy has the city, which the grid's search reads");
});
