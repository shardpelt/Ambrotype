// SPDX-License-Identifier: GPL-3.0-or-later
// The library bar's sort (with file date and size, and a direction) and the
// thumbnail size by Ctrl + wheel, Ctrl + / − / 0 and the slider.
// Run: ambrotype --ui-test crates/amb-gui/tests/ui/library-zoom.js photo.dng

await t.step("Open folder shows the photo in the library", async () => {
  await t.until("the photo", () => ui.info, 60000);
  await openFolder(dirOf(ui.info.path));
  await t.until("the grid", () => libraryOn() && lib.photos.length > 0 && lib.cells.size > 0);
});

await t.step("the sort is in the bar with six options and a direction", async () => {
  const sort = $("lib-sort");
  t.expect(!sort.closest("details"), "not folded away");
  t.expect(t.visible(sort) && t.visible($("lib-reverse").closest("label")), "both shown");
  t.expect([...sort.options].map((o) => o.value).join() === "captured,modified,name,size,rating,edited", "the six options in order");
  t.expect($("lib-reverse").type === "checkbox" && $("lib-reverse").closest(".lib-sort-dir"), "the direction toggle");
});

await t.step("libSortCompare orders by each field, unknown last", async () => {
  const byName = (a, b) => a.name.localeCompare(b.name);
  const order = (sort, list) => [...list].sort(libSortCompare(sort, byName)).map((p) => p.name).join();
  const a = { name: "a", captured: "2026-02-01 10:00:00", modified: 300, size: 30 };
  const b = { name: "b", captured: "2026-01-01 10:00:00", modified: 100, size: 10 };
  const c = { name: "c", captured: null, modified: null, size: null };
  const d = { name: "d", captured: "2026-01-01 10:00:00", modified: 100, size: 10 };
  t.expect(order("name", [c, a, b]) === "a,b,c", "name");
  t.expect(order("captured", [c, a, b, d]) === "b,d,a,c", "captured: oldest first, ties by name, unknown last");
  t.expect(order("modified", [c, a, d, b]) === "b,d,a,c", "modified: oldest first, unknown last");
  t.expect(order("size", [c, a, d, b]) === "b,d,a,c", "size: smallest first, unknown last");
});

await t.step("the listing's photos carry size and modified time", async () => {
  const p = lib.photos[0];
  t.expect(p.size > 0 && p.modified > 1e12, `size ${p.size}, modified ${p.modified}`);
});

const wheel = (deltaY, over = document.querySelector("#library-grid .lib-cell"), ctrlKey = true) =>
  over.dispatchEvent(new WheelEvent("wheel", { deltaY, ctrlKey, bubbles: true, cancelable: true }));
const size = () => Number($("lib-size").value);
const thumbVar = () => $("library-grid").style.getPropertyValue("--thumb");

await t.step("Ctrl + wheel sizes the thumbnails, within the limits", async () => {
  t.key("Ctrl+0");
  t.expect(size() === 160, "starts at 160");
  wheel(100);
  t.expect(size() < 160 && thumbVar() === `${size()}px`, `smaller (${size()}), --thumb follows (${thumbVar()})`);
  for (let i = 0; i < 40; i++) wheel(100);
  t.expect(size() === 56 && thumbVar() === "56px", `stops at the smallest (${size()})`);
  wheel(-100);
  t.expect(size() > 56, `bigger again (${size()})`);
  for (let i = 0; i < 80; i++) wheel(-100);
  t.expect(size() === Number($("lib-size").max) && size() > 480 - 1, `stops at the largest (${size()})`);
});

await t.step("a plain wheel leaves the size alone", async () => {
  const before = size();
  wheel(100, undefined, false);
  t.expect(size() === before, "same size");
});

await t.step("Ctrl + = grows, Ctrl + − shrinks, Ctrl + 0 is 160, and it is remembered", async () => {
  t.key("Ctrl+0");
  t.expect(size() === 160, "Ctrl+0 is 160");
  t.key("Ctrl+=");
  t.expect(size() === 200, `grows by a step (${size()})`);
  t.key("Ctrl+-");
  t.key("Ctrl+-");
  t.expect(size() === 128, `shrinks (${size()})`);
  t.expect(store.text("amb.libSize") === "128", "remembered");
  $("lib-size").value = 220;
  $("lib-size").dispatchEvent(new Event("input", { bubbles: true }));
  t.expect(thumbVar() === "220px" && store.text("amb.libSize") === "220", "the slider goes through the same path");
  t.key("Ctrl+0");
});

await t.step("File size and the direction re-sort and are remembered", async () => {
  $("lib-sort").value = "size";
  $("lib-sort").dispatchEvent(new Event("change", { bubbles: true }));
  t.expect(store.text("amb.libSort") === "size", "sort remembered");
  t.expect($("lib-sort").value === "size" && lib.shown.length > 0, "grid redrawn");
  $("lib-reverse").click();
  t.expect($("lib-reverse").checked && store.get("amb.libReverse") === true, "direction remembered");
  t.expect(getComputedStyle($("lib-reverse").nextElementSibling, "::before").content.includes("↓"), "shows ↓");
  const byKey = KEY_ACTIONS.find((a) => a.id === "lib-sort-reverse");
  t.expect(byKey, "a key action for it");
  byKey.run();
  t.expect(!$("lib-reverse").checked && store.get("amb.libReverse") === false, "the key action flips it back");
  $("lib-sort").value = "name";
  $("lib-sort").dispatchEvent(new Event("change", { bubbles: true }));
});

await t.step("the largest size is one photo as big as the grid, and E toggles it", async () => {
  t.key("Ctrl+0");
  const grid = $("library-grid");
  const max = Math.floor(grid.clientWidth - 24);
  t.expect(Number($("lib-size").max) === max, `slider max ${$("lib-size").max} follows the grid width ${max}`);
  libSetSize(max);
  await t.until("one photo per row", () => grid.classList.contains("one-photo"));
  const cell = grid.querySelector(".lib-cell").getBoundingClientRect();
  t.expect(cell.height <= grid.clientHeight, `a tile ${Math.round(cell.height)}px fits the grid ${grid.clientHeight}px`);
  libSetSize(160);
  KEY_ACTIONS.find((a) => a.id === "lib-loupe").run();
  t.expect(size() === Number($("lib-size").max), `E goes to one photo (${size()})`);
  KEY_ACTIONS.find((a) => a.id === "lib-loupe").run();
  t.expect(size() === 160, `E again goes back (${size()})`);
});
