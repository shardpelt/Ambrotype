// SPDX-License-Identifier: GPL-3.0-or-later
// Negatives: the Auto switches and picking the film base.
// Run: ambrotype --ui-test crates/amb-gui/tests/ui/negative.js photo.dng

await t.step("the photo opens and a preview shows", async () => {
  await t.until("the photo", () => ui.info, 60000);
  await t.until("the preview", () => t.visible("#preview") && $("preview").naturalWidth > 0, 60000);
});

await t.step("Negative: Auto Density and Auto Grade are switches beside Auto", async () => {
  revealSetting("amb:NegativeAutoDensity");
  t.expect(t.visible(t.row("amb:NegativeAutoDensity")) && t.visible(t.row("amb:NegativeAutoGrade")), "both shown");
  revealSetting("Exposure2012");
});

await t.step("Pick base: a click on the photo stores the film base; Alt-click forgets it", async () => {
  const pick = document.querySelector(".pick-base");
  t.expect(pick, "the Film Negative panel's Pick base button");
  const steps = t.historyLength();
  t.click(pick);
  await t.until("picking the base", () => ui.picking === "base" && $("viewer").classList.contains("picking"));
  const r = $("preview").getBoundingClientRect();
  const at = { clientX: r.left + r.width * 0.5, clientY: r.top + r.height * 0.5, button: 0, bubbles: true };
  $("preview").dispatchEvent(new MouseEvent("mousedown", at));
  window.dispatchEvent(new MouseEvent("mouseup", at));
  await t.until("a film base", () => t.setting("amb:FilmBaseRed") != null, 30000);
  t.expect(t.setting("amb:NegativeConvert") === true, "the conversion on");
  t.expect(!ui.picking && !pick.classList.contains("active"), "picking over");
  t.expect(t.historyLength() === steps + 1, "one history step");
  pick.dispatchEvent(new MouseEvent("click", { altKey: true, bubbles: true }));
  await t.until("the base measured again", () => t.setting("amb:FilmBaseRed") == null);
  t.key("Ctrl+Z");
  t.key("Ctrl+Z");
  await t.until("the photo as it was", () => !t.setting("amb:NegativeConvert"));
});
