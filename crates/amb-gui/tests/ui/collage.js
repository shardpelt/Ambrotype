// SPDX-License-Identifier: GPL-3.0-or-later
// The collage maker's free arrangement: snapping a dragged frame.
// Alt is held only while moving: pressed on the frame it moves the photo instead.
// Run: ambrotype --ui-test crates/amb-gui/tests/ui/collage.js photo.dng

await t.step("a frame dragged near another's line snaps to it and shows a guide; Alt turns it off", async () => {
  await t.until("the photo", () => ui.info, 60000);
  await collageFlow();
  await t.until("the collage maker", () => $("collage-dialog").open && collage.templates.length);
  toggleFree();
  // Two frames: the second well away from any line of the first or of the canvas.
  collage.t.cells = [[0.1, 0.1, 0.3, 0.3], [0.62, 0.5, 0.3, 0.2]];
  collage.cells = [null, null];
  collage.selected = -1;
  renderStage();
  const stage = $("col-stage");
  const scale = Number(stage.dataset.scale);
  const rects = () => cellRects(collage.t);
  const drag = async (altKey) => {
    collage.t.cells[1] = [0.62, 0.5, 0.3, 0.2];
    renderStage();
    const r = stage.querySelector('.col-cell[data-index="1"]').getBoundingClientRect();
    const [x, y] = [r.left + r.width / 2, r.top + r.height / 2];
    const ev = (type, dy) => new PointerEvent(type, { clientX: x, clientY: y + dy, button: 0, pointerId: 9, altKey: altKey && type === "pointermove", bubbles: true });
    // The top line to 3 screen pixels below the first frame's top.
    const dy = (0.1 - 0.5) * collage.t.height * scale + 3;
    stage.querySelector('.col-cell[data-index="1"]').dispatchEvent(ev("pointerdown", 0));
    stage.dispatchEvent(ev("pointermove", dy));
    return { up: () => stage.dispatchEvent(ev("pointerup", dy)), dy };
  };
  let d = await drag(false);
  t.expect(Math.abs(rects()[1].y - rects()[0].y) <= 1, `snapped to the other frame's top: ${rects()[1].y} / ${rects()[0].y}`);
  t.expect(stage.querySelector(".col-guide.h"), "a guide line shows");
  d.up();
  t.expect(!stage.querySelector(".col-guide"), "the guide goes with the drag");
  d = await drag(true);
  t.expect(rects()[1].y - rects()[0].y >= 2, `Alt: no snapping (${rects()[1].y} / ${rects()[0].y})`);
  t.expect(!stage.querySelector(".col-guide"), "no guide with Alt");
  d.up();
  collage.dirty = false;
  closeCollage();
});
