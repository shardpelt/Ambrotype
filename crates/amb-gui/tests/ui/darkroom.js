// SPDX-License-Identifier: GPL-3.0-or-later
// Darkroom aids: the Film Negative panel's range and clipping readout.
// Run: ambrotype --ui-test crates/amb-gui/tests/ui/darkroom.js photo.dng

await t.step("the photo opens and a preview shows", async () => {
  await t.until("the photo", () => ui.info, 60000);
  await t.until("the preview", () => t.visible("#preview") && $("preview").naturalWidth > 0, 60000);
});

await t.step("Film Negative: with the conversion on, the range line has text", async () => {
  revealSetting("amb:NegativeConvert");
  t.expect(document.querySelector("#darkroom-readout"), "the readout in the panel");
  await run([{ cmd: "set_param", name: "amb:NegativeConvert", value: true }], "test");
  await t.until("the range", () => /Range/.test($("darkroom-range")?.textContent ?? "") && !$("darkroom-readout").hidden, 60000);
  t.expect(/Clipped at scan/.test($("darkroom-range").textContent), "clipping shown");
  await run([{ cmd: "set_param", name: "amb:NegativeConvert", value: false }], "test");
  await t.until("the range gone", () => $("darkroom-readout").hidden, 60000);
  t.key("Ctrl+Z");
  t.key("Ctrl+Z");
});
