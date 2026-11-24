// SPDX-License-Identifier: GPL-3.0-or-later
// Darkroom aids: the zone overlay (Shift+Z, View menu), and the Film Negative
// panel's range, clipping and the zone and density under the pointer.
// Run: ambrotype --ui-test crates/amb-gui/tests/ui/darkroom.js photo.dng

await t.step("the photo opens and a preview shows", async () => {
  await t.until("the photo", () => ui.info, 60000);
  await t.until("the preview", () => t.visible("#preview") && $("preview").naturalWidth > 0, 60000);
});

await t.step("Zone overlay: Shift+Z paints the photo by zone, with a legend, and ticks the menu item", async () => {
  t.expect(!t.visible("#darkroom-zones") && !t.visible("#darkroom-legend"), "off on start");
  t.expect(!menuItemTick("zone-overlay"), "not ticked");
  t.key("Shift+Z");
  await t.until("the overlay", () => t.visible("#darkroom-zones") && t.visible("#darkroom-legend"));
  t.expect(document.querySelectorAll("#darkroom-legend .darkroom-cell").length === 11, "eleven zones in the legend");
  t.expect(menuItemTick("zone-overlay"), "ticked");
  t.expect(MENU_BAR.find(([name]) => name === "View")[1].includes("zone-overlay"), "in the View menu");
  const r = $("preview").getBoundingClientRect();
  $("viewer").dispatchEvent(new PointerEvent("pointermove", { clientX: r.left + r.width / 2, clientY: r.top + r.height / 2, bubbles: true }));
  t.expect(document.querySelectorAll("#darkroom-legend .darkroom-cell.on").length === 1, "the zone under the pointer is outlined");
  t.key("Shift+Z");
  await t.until("the overlay gone", () => !t.visible("#darkroom-zones") && !t.visible("#darkroom-legend"));
  t.expect(!menuItemTick("zone-overlay"), "not ticked again");
});

await t.step("Film Negative: with the conversion on, the range line has text", async () => {
  revealSetting("amb:NegativeConvert");
  t.expect(document.querySelector("#darkroom-readout"), "the readout in the panel");
  await run([{ cmd: "set_param", name: "amb:NegativeConvert", value: true }], "test");
  await t.until("the range", () => /Range/.test($("darkroom-range")?.textContent ?? "") && !$("darkroom-readout").hidden, 60000);
  t.expect(/Clipped at scan/.test($("darkroom-range").textContent), "clipping shown");
  // On the Film tab, a pointer resting on the photo reads the zone and density there.
  await switchTask("film");
  await t.until("the film tab", () => currentTask() === "film");
  await t.until("the photo again", () => t.visible("#preview") && $("preview").naturalWidth > 0, 60000);
  const r = $("preview").getBoundingClientRect();
  $("viewer").dispatchEvent(new PointerEvent("pointermove", { clientX: r.left + r.width / 2, clientY: r.top + r.height / 2, bubbles: true }));
  await t.until("the zone and density", () => /Zone [0IVX]+ · D -?\d/.test($("darkroom-probe")?.textContent ?? ""), 30000);
  $("viewer").dispatchEvent(new PointerEvent("pointerleave", { bubbles: true }));
  t.expect($("darkroom-probe").textContent === "", "cleared when the pointer leaves");
  await switchTask("develop");
  await run([{ cmd: "set_param", name: "amb:NegativeConvert", value: false }], "test");
  await t.until("the range gone", () => $("darkroom-readout").hidden, 60000);
  t.key("Ctrl+Z");
  t.key("Ctrl+Z");
});
