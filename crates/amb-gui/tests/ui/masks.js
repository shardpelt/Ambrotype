// SPDX-License-Identifier: GPL-3.0-or-later
// The Masking panel: open it by its key, draw a linear, a radial and a brush
// mask with the pointer, set a mask's slider, rename, hide, invert and delete
// a mask. Each change is one history step that Ctrl+Z undoes. No AI masks.
// Run: ambrotype --ui-test crates/amb-gui/tests/ui/masks.js photo.dng

await t.step("the photo opens and a preview shows", async () => {
  await t.until("the photo", () => ui.info, 60000);
  await t.until("the preview", () => t.visible("#preview") && $("preview").naturalWidth > 0, 60000);
});

// Pointer events on the mask canvas from (fx0, fy0) to (fx1, fy1), as fractions of its size.
const maskDrag = async ([fx0, fy0], [fx1, fy1]) => {
  const canvas = $("mask-canvas");
  canvas.setPointerCapture = () => {}; // the pointer is made up here
  const c = canvas.getBoundingClientRect();
  const at = (fx, fy) => [c.left + c.width * fx, c.top + c.height * fy];
  const ev = (type, [x, y], buttons) => canvas.dispatchEvent(new PointerEvent(type, { bubbles: true, cancelable: true, clientX: x, clientY: y, pointerId: 7, button: 0, buttons, pointerType: "mouse" }));
  ev("pointerdown", at(fx0, fy0), 1);
  for (const f of [0.25, 0.5, 0.75, 1]) {
    ev("pointermove", at(fx0 + (fx1 - fx0) * f, fy0 + (fy1 - fy0) * f), 1);
    await t.sleep(20);
  }
  ev("pointerup", at(fx1, fy1), 0);
};

let steps0 = 0;
let masks0 = 0;

// Draw a mask with the key `key`, and wait for it to be the newest one.
const drawMask = async (key, from, to, kind) => {
  const steps = t.historyLength();
  const n = maskList().length;
  document.activeElement?.blur();
  t.key(key);
  await t.until(`the ${kind} tool`, () => mk.tool === kind);
  await maskDrag(from, to);
  await t.until(`a ${kind} mask`, () => maskList().length === n + 1 && mk.selected === n);
  await t.until("a history step", () => t.historyLength() === steps + 1);
  t.expect(maskList()[n].parts[0].kind === kind, `the part is ${maskList()[n].parts[0].kind}`);
  t.expect(t.setting(MASK_TAG).length === n + 1, "the edit holds the mask");
};

await t.step("M opens the Masking panel", async () => {
  document.activeElement?.blur();
  t.key("M");
  await t.until("the mask tool", () => mk.on && mk.frame && t.visible("#mask-panel") && t.visible("#mask-canvas"), 30000);
  steps0 = t.historyLength();
  masks0 = maskList().length;
  t.expect($("mask-new").querySelectorAll("button").length >= 3, "the kinds of new mask");
});

await t.step("a dragged linear gradient is one history step; Ctrl+Z undoes it", async () => {
  await drawMask("L", [0.3, 0.5], [0.7, 0.5], "linear");
  t.expect(!mk.tool, "the tool stops after one gradient");
  t.key("Ctrl+Z");
  await t.until("undone", () => maskList().length === masks0 && t.historyLength() === steps0);
});

await t.step("a dragged radial gradient is one history step; Ctrl+Z undoes it", async () => {
  await drawMask("Shift+R", [0.5, 0.5], [0.7, 0.7], "radial");
  t.key("Ctrl+Z");
  await t.until("undone", () => maskList().length === masks0 && t.historyLength() === steps0);
});

await t.step("a brush stroke makes a brush mask as one step; Ctrl+Z undoes it", async () => {
  await drawMask("B", [0.3, 0.3], [0.6, 0.6], "brush");
  t.expect(maskList()[masks0].parts[0].strokes >= 1, "a stroke");
  // The brush stays on to paint more; Esc stops it, and the panel stays.
  t.key("Escape");
  await t.until("the brush stopped", () => !mk.tool);
  t.expect(mk.on, "the Masking panel is still open");
  t.key("Ctrl+Z");
  await t.until("undone", () => maskList().length === masks0 && t.historyLength() === steps0);
});

await t.step("a mask's slider sets its correction, as one step", async () => {
  await drawMask("L", [0.3, 0.5], [0.7, 0.5], "linear");
  const steps = t.historyLength();
  const tag = "LocalExposure2012";
  const input = $("mask-sliders").querySelector(`label[title="${tag}"]`)?.closest(".row")?.querySelector('input[type="range"]');
  t.expect(input, "an Exposure slider on the mask");
  t.expect(!selectedMask().sliders[tag], "no exposure to begin with");
  const v = Number(input.max) / 2;
  input.value = String(v);
  input.dispatchEvent(new Event("input", { bubbles: true }));
  input.dispatchEvent(new Event("change", { bubbles: true }));
  await t.until("the mask's exposure", () => selectedMask()?.sliders[tag] === v, 30000);
  t.expect(t.setting(MASK_TAG)[mk.selected].LocalExposure2012 !== undefined, "the edit holds the correction");
  await t.until("a history step", () => t.historyLength() === steps + 1);
  t.key("Ctrl+Z");
  await t.until("undone", () => !selectedMask()?.sliders[tag] && t.historyLength() === steps);
});

await t.step("rename, hide and invert a mask, each one step", async () => {
  const i = masks0;
  const row = () => $("mask-list").querySelectorAll(".mask-row")[i];
  // Rename: double-click the name, type, Enter.
  let steps = t.historyLength();
  row().querySelector(".mask-name").dispatchEvent(new MouseEvent("dblclick", { bubbles: true }));
  const box = row().querySelector('input[type="text"]');
  t.expect(box, "a name box");
  box.value = "Sky grad";
  box.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  await t.until("renamed", () => maskList()[i].name === "Sky grad" && t.historyLength() === steps + 1);
  // Hide: the eye box.
  steps = t.historyLength();
  const eye = row().querySelector('input[type="checkbox"]');
  eye.checked = false;
  eye.dispatchEvent(new Event("change", { bubbles: true }));
  await t.until("hidden", () => maskList()[i].active === false && t.historyLength() === steps + 1);
  // Invert the part.
  steps = t.historyLength();
  const inv = $("mask-parts").querySelector('input[type="checkbox"]');
  inv.checked = true;
  inv.dispatchEvent(new Event("change", { bubbles: true }));
  await t.until("inverted", () => maskList()[i].parts[0].invert === true && t.historyLength() === steps + 1);
  for (let k = 0; k < 3; k++) t.key("Ctrl+Z");
  await t.until("all undone", () => maskList()[i].name !== "Sky grad" && maskList()[i].active && !maskList()[i].parts[0].invert);
});

await t.step("the × deletes the mask as one step; Ctrl+Z brings it back", async () => {
  const steps = t.historyLength();
  const n = maskList().length;
  t.click($("mask-list").querySelectorAll(".mask-row .icon-x")[n - 1]);
  await t.until("deleted", () => maskList().length === n - 1 && t.historyLength() === steps + 1);
  t.key("Ctrl+Z");
  await t.until("back", () => maskList().length === n);
  // Leave nothing behind for the scripts that follow.
  while (t.historyLength() > steps0) {
    const len = t.historyLength();
    t.key("Ctrl+Z");
    await t.until("one more undo", () => t.historyLength() < len);
  }
  t.expect(maskList().length === masks0, "the masks as they were");
});

await t.step("Esc leaves the Masking panel", async () => {
  t.key("Escape");
  await t.until("closed", () => !mk.on && !t.visible("#mask-panel"));
});
