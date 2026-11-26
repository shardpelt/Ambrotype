// SPDX-License-Identifier: GPL-3.0-or-later
// The Crop panel: an aspect choice locks the ratio, Swap turns it, the Angle
// slider straightens, Reset clears the crop, Enter applies and Esc cancels.
// Each applied change is one history step. (crop-curve.js drags the corner.)
// Run: ambrotype --ui-test crates/amb-gui/tests/ui/crop.js photo.dng

await t.step("the photo opens and a preview shows", async () => {
  await t.until("the photo", () => ui.info, 60000);
  await t.until("the preview", () => t.visible("#preview") && $("preview").naturalWidth > 0, 60000);
});

const cropOpen = async () => {
  document.activeElement?.blur();
  t.key("R");
  await t.until("the crop tool", () => ui.crop.on && ui.crop.tool && t.visible("#crop-panel"), 30000);
};
const cropApply = async () => {
  document.activeElement?.blur();
  t.key("Enter");
  await t.until("the crop applied", () => !ui.crop.on);
};
// Width / height of the stored crop, in photo pixels.
const cropRatio = () => {
  const s = (k, d) => t.setting(k) ?? d;
  return ((s("CropRight", 1) - s("CropLeft", 0)) * ui.info.width) / ((s("CropBottom", 1) - s("CropTop", 0)) * ui.info.height);
};
const pickAspect = (value) => {
  const sel = $("crop-aspect");
  t.expect([...sel.options].some((o) => o.value === value), `an aspect ${value}`);
  sel.value = value;
  sel.dispatchEvent(new Event("change", { bubbles: true }));
};
const steps0 = t.historyLength();

await t.step("the Aspect list locks the ratio: 1:1 makes a square crop, as one step", async () => {
  t.expect((t.setting("CropRight") ?? 1) === 1, "no crop to begin with");
  await cropOpen();
  pickAspect("1:1");
  await t.until("the panel shows 1:1", () => $("crop-size").textContent.includes("1:1"));
  await cropApply();
  await t.until("a crop", () => t.setting("CropRight") < 1 || t.setting("CropBottom") < 1, 30000);
  await t.until("a history step", () => t.historyLength() === steps0 + 1);
  t.expect(Math.abs(cropRatio() - 1) < 0.01, `ratio ${cropRatio()}`);
});

await t.step("Swap turns 3:2 into 2:3 and keeps the lock", async () => {
  await cropOpen();
  pickAspect("3:2");
  await cropApply();
  await t.until("a 3:2 crop", () => Math.abs(cropRatio() - 1.5) < 0.01, 30000);
  await cropOpen();
  t.key("X");
  await t.until("the panel shows 2:3", () => $("crop-size").textContent.includes("2:3"));
  await cropApply();
  await t.until("a 2:3 crop", () => Math.abs(cropRatio() - 2 / 3) < 0.01, 30000);
});

await t.step("the Angle slider straightens, as one step", async () => {
  const steps = t.historyLength();
  await cropOpen();
  const angle = $("crop-angle");
  angle.value = "4";
  angle.dispatchEvent(new Event("input", { bubbles: true }));
  angle.dispatchEvent(new Event("change", { bubbles: true }));
  await t.until("the number follows", () => Math.abs(Number($("crop-angle-num").value) - 4) < 0.5);
  await cropApply();
  await t.until("a CropAngle", () => Math.abs(Math.abs(t.setting("CropAngle") ?? 0) - 4) < 0.5, 30000);
  await t.until("a history step", () => t.historyLength() === steps + 1);
  t.key("Ctrl+Z");
  await t.until("undone", () => Math.abs(t.setting("CropAngle") ?? 0) < 0.01 && t.historyLength() === steps);
  // Double-click on the Angle label puts it back to level.
  await cropOpen();
  angle.value = "-3";
  angle.dispatchEvent(new Event("input", { bubbles: true }));
  angle.dispatchEvent(new Event("change", { bubbles: true }));
  await t.until("turned", () => Math.abs(Number($("crop-angle-num").value) + 3) < 0.5);
  angle.previousElementSibling.dispatchEvent(new MouseEvent("dblclick", { bubbles: true }));
  await t.until("level again", () => Math.abs(Number($("crop-angle-num").value)) < 0.01);
  // Straightening shrank the frame, so applying would be a change: cancel.
  document.activeElement?.blur();
  t.key("Escape");
  await t.until("the tool closed", () => !ui.crop.on);
});

await t.step("Esc cancels: nothing changes", async () => {
  const steps = t.historyLength();
  const was = cropKey(ui.settings);
  await cropOpen();
  pickAspect("16:9");
  document.activeElement?.blur();
  t.key("Escape");
  await t.until("the tool closed", () => !ui.crop.on);
  t.expect(t.historyLength() === steps && cropKey(ui.settings) === was, "no change");
});

await t.step("Reset clears the crop and the angle, as one step", async () => {
  const steps = t.historyLength();
  t.expect((t.setting("CropRight") ?? 1) < 1 || (t.setting("CropBottom") ?? 1) < 1, "a crop to clear");
  await cropOpen();
  t.click("#crop-reset");
  await t.until("no crop", () => (t.setting("CropRight") ?? 1) === 1 && (t.setting("CropBottom") ?? 1) === 1 && !t.setting("CropAngle"), 30000);
  await t.until("a history step", () => t.historyLength() === steps + 1);
  t.expect($("crop-aspect").value === "original", `aspect ${$("crop-aspect").value}`);
  await cropApply();
});

await t.step("the history is put back for the next script", async () => {
  while (t.historyLength() > steps0) {
    const len = t.historyLength();
    t.key("Ctrl+Z");
    await t.until("one more undo", () => t.historyLength() < len);
  }
  await t.until("no crop", () => (t.setting("CropRight") ?? 1) === 1 && !t.setting("CropAngle"));
});
