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

await t.step("Meter area: a drag stores the box (one step); Alt-click meters the whole crop again", async () => {
  const meter = document.querySelector(".pick-meter");
  t.expect(meter, "the Film Negative panel's Meter area button");
  const steps = t.historyLength();
  t.click(meter);
  await t.until("metering", () => ui.picking === "meter" && meter.classList.contains("active"));
  t.click(meter);
  t.expect(!ui.picking, "the button toggles the tool off");
  t.click(meter);
  await t.until("metering again", () => ui.picking === "meter");
  const r = $("preview").getBoundingClientRect();
  const at = (fx, fy) => ({ clientX: r.left + r.width * fx, clientY: r.top + r.height * fy, button: 0, bubbles: true });
  $("preview").dispatchEvent(new MouseEvent("mousedown", at(0.2, 0.4)));
  window.dispatchEvent(new MouseEvent("mousemove", at(0.5, 0.7)));
  t.expect(t.visible("#meter-box"), "the box shows while dragging");
  window.dispatchEvent(new MouseEvent("mouseup", at(0.8, 0.9)));
  await t.until("a metering box", () => t.setting("amb:NegativeMeterLeft") != null, 30000);
  const box = ["Left", "Top", "Right", "Bottom"].map((n) => t.setting(`amb:NegativeMeter${n}`));
  t.expect(Math.abs(box[0] - 0.2) < 0.02 && Math.abs(box[1] - 0.4) < 0.02 && Math.abs(box[2] - 0.8) < 0.02 && Math.abs(box[3] - 0.9) < 0.02, `the box ${box}`);
  t.expect(t.setting("amb:NegativeConvert") === true, "the conversion on");
  t.expect(!ui.picking && !t.visible("#meter-box"), "drawing over");
  t.expect(t.historyLength() === steps + 1, "one history step");
  meter.dispatchEvent(new MouseEvent("click", { altKey: true, bubbles: true }));
  await t.until("the whole crop again", () => t.setting("amb:NegativeMeterLeft") == null && t.setting("amb:NegativeMeterBottom") == null);
  t.click(meter);
  await t.until("metering", () => ui.picking === "meter");
  $("preview").dispatchEvent(new MouseEvent("mousedown", at(0.5, 0.5)));
  window.dispatchEvent(new MouseEvent("mouseup", at(0.5, 0.5)));
  t.expect(!ui.picking && t.setting("amb:NegativeMeterLeft") == null, "a plain click cancels");
  t.key("Ctrl+Z");
  t.key("Ctrl+Z");
  t.key("Ctrl+Z");
  await t.until("the photo as it was", () => !t.setting("amb:NegativeConvert"));
});
