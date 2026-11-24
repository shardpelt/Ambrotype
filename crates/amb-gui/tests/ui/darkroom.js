// SPDX-License-Identifier: GPL-3.0-or-later
// Darkroom aids: the zone overlay (Shift+Z, View menu), the Film Negative
// panel's range, clipping and the zone and density under the pointer, and the
// test strip and ring-around dialog.
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

await t.step("Test strip: seven cells, one click sets the exposure as one undo step", async () => {
  await run([{ cmd: "set_param", name: "amb:NegativeConvert", value: true }], "test");
  const before = Number(currentValue("amb:NegativeExposure")) || 0;
  const steps = t.historyLength();
  aidsOpen("strip");
  await t.until("the strip", () => t.visible("#aids-stage") && document.querySelectorAll("#aids-stage .aids-cell").length === 7, 120000);
  const cell = document.querySelectorAll("#aids-stage .aids-cell")[6];
  t.expect(cell.textContent === "+1", "the last band is a stop up");
  cell.click();
  await t.until("the dialog closed", () => !$("aids-dialog").open);
  await t.until("the exposure set, in one step", () => Math.abs(Number(currentValue("amb:NegativeExposure")) - (before + 1)) < 1e-6 && t.historyLength() === steps + 1);
  t.key("Ctrl+Z");
  await t.until("one undo puts it back", () => Math.abs((Number(currentValue("amb:NegativeExposure")) || 0) - before) < 1e-6);
});

await t.step("Ring-around: nine cells on a converted negative; Esc closes", async () => {
  aidsOpen("ring");
  await t.until("the ring", () => t.visible("#aids-stage") && document.querySelectorAll("#aids-stage .aids-cell").length === 9, 120000);
  t.expect([...document.querySelectorAll("#aids-stage .aids-cell")].some((c) => c.textContent === "More red"), "a More red cell");
  $("aids-dialog").close();
  t.expect(!$("aids-dialog").open, "closed");
  await run([{ cmd: "set_param", name: "amb:NegativeConvert", value: false }], "test");
});
