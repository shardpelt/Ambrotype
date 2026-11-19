// SPDX-License-Identifier: GPL-3.0-or-later
// Dragging with the pointer: a crop's corner and a tone curve's point, each as one history step.
// Run: ambrotype --ui-test crates/amb-gui/tests/ui/crop-curve.js photo.dng

await t.step("the photo opens and a preview shows", async () => {
  await t.until("the photo", () => ui.info, 60000);
  await t.until("the preview", () => t.visible("#preview") && $("preview").naturalWidth > 0, 60000);
});

// Pointer events on `el` from (x0, y0) to (x1, y1), in client pixels, in a few moves.
const dragPointer = async (el, [x0, y0], [x1, y1]) => {
  const ev = (type, x, y, buttons) => el.dispatchEvent(new PointerEvent(type, { bubbles: true, cancelable: true, clientX: x, clientY: y, pointerId: 5, button: 0, buttons, pointerType: "mouse" }));
  ev("pointerdown", x0, y0, 1);
  for (const f of [0.25, 0.5, 0.75, 1]) {
    ev("pointermove", x0 + (x1 - x0) * f, y0 + (y1 - y0) * f, 1);
    await t.sleep(20);
  }
  ev("pointerup", x1, y1, 0);
  await t.sleep(50);
};

await t.step("dragging the crop's corner makes the crop smaller, as one step", async () => {
  const steps = t.historyLength();
  t.expect((t.setting("CropRight") ?? 1) === 1, "no crop to begin with");
  t.key("R");
  await t.until("the crop tool", () => ui.crop.on && ui.crop.tool);
  // The tool starts in draw mode when there is no crop; a corner is for resizing.
  if (ui.crop.tool.drawMode()) t.key("D");
  t.expect(!ui.crop.tool.drawMode(), "draw mode is off");
  // The photo is fitted in the canvas with a 28 px margin; its bottom-right corner is the handle.
  const canvas = $("crop-canvas");
  const c = canvas.getBoundingClientRect();
  const pad = 28;
  const s = Math.min((c.width - 2 * pad) / ui.info.width, (c.height - 2 * pad) / ui.info.height);
  const corner = [c.left + c.width / 2 + (s * ui.info.width) / 2 - 1, c.top + c.height / 2 + (s * ui.info.height) / 2 - 1];
  await dragPointer(canvas, corner, [corner[0] - 100, corner[1] - 100]);
  t.key("Enter");
  await t.until("the crop applied", () => !ui.crop.on);
  await t.until("a smaller crop", () => t.setting("CropRight") < 1, 30000);
  await t.until("a history step", () => t.historyLength() === steps + 1);
  t.key("Ctrl+Z");
  await t.until("undone", () => (t.setting("CropRight") ?? 1) === 1 && t.historyLength() === steps);
});

await t.step("dragging a curve point changes the curve, as one step", async () => {
  const tag = "ToneCurvePV2012";
  const canvas = ui.curve.element.querySelector("canvas");
  // Another tab of the column may be in front (an earlier script chose it), and
  // the panel's section may be closed: show both so the canvas has a size.
  revealInTab(canvas);
  canvas.closest("details")?.setAttribute("open", "");
  ui.curve.refresh();
  await t.until("the curve has a size", () => canvas.getBoundingClientRect().width > 50);
  canvas.setPointerCapture = () => {}; // the pointer is made up here
  const was = JSON.stringify(t.setting(tag) ?? null);
  const steps = t.historyLength();
  const c = canvas.getBoundingClientRect();
  const mid = [c.left + c.width / 2, c.top + c.height / 2];
  await dragPointer(canvas, mid, [mid[0], mid[1] - 30]);
  await t.until("the curve changed", () => JSON.stringify(t.setting(tag) ?? null) !== was, 30000);
  await t.until("a history step", () => t.historyLength() === steps + 1);
  t.key("Ctrl+Z");
  await t.until("undone", () => JSON.stringify(t.setting(tag) ?? null) === was && t.historyLength() === steps);
});
