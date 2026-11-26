// SPDX-License-Identifier: GPL-3.0-or-later
// Darkroom aids: the zone overlay (Shift+Z, View menu), the Film Negative
// panel's range, clipping and the zone and density under the pointer, and the
// test strip and ring-around dialog, placing a spot in a zone and peeking at
// a negative.
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

await t.step("Pick Better: A, then B, then Same, then Done sets the exposure as one undo step", async () => {
  const before = Number(currentValue("Exposure2012")) || 0;
  const steps = t.historyLength();
  const pair = () => document.querySelectorAll("#better-stage .aids-cell");
  const shown = () => t.visible("#better-stage") && pair().length === 2 && !$("better-same").disabled;
  KEY_ACTIONS.find((a) => a.id === "pick-better").run();
  await t.until("the first pair", shown, 120000);
  t.expect(/Round 1 of 5/.test($("better-round").textContent), `round 1 of 5 (${$("better-round").textContent})`);
  t.expect(pair()[0].textContent === "A -2/3" && pair()[1].textContent === "B +2/3", "2/3 stop each way");
  pair()[0].click();
  await t.until("a finer pair", () => shown() && pair()[1].textContent === "B +1/3", 120000);
  pair()[1].click();
  await t.until("the third pair", () => shown() && pair()[1].textContent === "B +0.17", 120000);
  $("better-same").click();
  await t.until("round 2", () => shown() && /Round 2 of 5/.test($("better-round").textContent), 120000);
  t.expect(t.historyLength() === steps, "nothing applied yet");
  $("better-done").click();
  await t.until("the dialog closed", () => !$("better-dialog").open);
  await t.until("the exposure set, in one step", () => Math.abs(Number(currentValue("Exposure2012")) - (before - 1 / 3)) < 1e-6 && t.historyLength() === steps + 1, 30000);
  t.key("Ctrl+Z");
  await t.until("one undo puts it back", () => Math.abs((Number(currentValue("Exposure2012")) || 0) - before) < 1e-6);
});

await t.step("Place a spot in a zone: the exposure moves, one undo step, one undo puts it back", async () => {
  const at = [0.5, 0.5];
  const probe = await invoke("darkroom_place", { at, zone: 5 });
  const zone = Math.max(0, Math.min(10, Math.round(probe.zone_before) + (probe.zone_before < 5 ? 2 : -2)));
  const setting = probe.setting;
  const before = Number(currentValue(setting)) || 0;
  const steps = t.historyLength();
  // The photo's right-click menu offers it where the pointer is.
  const r = $("preview").getBoundingClientRect();
  const items = aidsPlaceItems({ clientX: r.left + r.width / 2, clientY: r.top + r.height / 2 });
  t.expect(items.length === 2 && /^Place this spot/.test(items[0][0]), "the menu item is offered on the photo");
  items[0][1]();
  await t.until("the picker", () => t.visible(".zone-pop") && document.querySelectorAll(".zone-pop .zone-swatch").length === 11);
  document.querySelectorAll(".zone-pop .zone-swatch")[zone].click();
  await t.until("the picker closed", () => !document.querySelector(".zone-pop"));
  await t.until("the exposure moved, in one step", () => Math.abs((Number(currentValue(setting)) || 0) - before) > 0.05 && t.historyLength() === steps + 1, 60000);
  const after = await invoke("darkroom_place", { at, zone });
  t.expect(Math.abs(after.zone_before - zone) < 0.5, `the spot is near zone ${zone} now (${after.zone_before.toFixed(2)})`);
  t.key("Ctrl+Z");
  await t.until("one undo puts it back", () => Math.abs((Number(currentValue(setting)) || 0) - before) < 1e-6);
});

await t.step("Peek at the negative: the preview shows the scan, off restores it, history is untouched", async () => {
  const steps0 = t.historyLength();
  t.expect(!menuItemTick("peek-negative"), "not ticked");
  t.expect(MENU_BAR.find(([name]) => name === "View")[1].includes("peek-negative"), "in the View menu");
  const sum = async () => {
    // Only a finished render counts: one still in flight shows the last picture.
    await t.until("a preview", () => t.visible("#preview") && $("preview").naturalWidth > 0 && $("preview").complete && !ui.preview.inFlight && !ui.preview.dirty, 60000);
    const c = el("canvas");
    c.width = 16; c.height = 16;
    c.getContext("2d").drawImage($("preview"), 0, 0, 16, 16);
    return c.getContext("2d").getImageData(0, 0, 16, 16).data.reduce((a, v) => a + v, 0);
  };
  const plain = await sum();
  await run([{ cmd: "set_param", name: "amb:NegativeConvert", value: true }], "test");
  const steps = steps0 + 1;
  let converted = plain;
  await t.until("the converted print", async () => Math.abs((converted = await sum()) - plain) > 2000, 60000);
  t.key("Shift+N");
  await t.until("peeking", () => aids.peek && menuItemTick("peek-negative"));
  await t.until("the scan shows", async () => { const v = await sum(); return Math.abs(v - plain) < Math.abs(v - converted); }, 60000);
  t.expect(t.historyLength() === steps, "no undo step");
  t.expect(asBool(currentValue("amb:NegativeConvert")), "the edit still says converted");
  t.key("Shift+N");
  await t.until("back", () => !aids.peek && !menuItemTick("peek-negative"));
  await t.until("the print shows again", async () => { const v = await sum(); return Math.abs(v - converted) < Math.abs(v - plain); }, 60000);
  t.expect(t.historyLength() === steps, "still no undo step");
  await run([{ cmd: "set_param", name: "amb:NegativeConvert", value: false }], "test");
});
