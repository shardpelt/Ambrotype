// SPDX-License-Identifier: GPL-3.0-or-later
// Retouch: the Spot panel and Remove object.
// Run: ambrotype --ui-test crates/amb-gui/tests/ui/retouch.js photo.dng

await t.step("the photo opens and a preview shows", async () => {
  await t.until("the photo", () => ui.info, 60000);
  await t.until("the preview", () => t.visible("#preview") && $("preview").naturalWidth > 0, 60000);
});

await t.step("Spot panel: Find dust finds none on a clean chart; Shift-drag heals a line as one step", async () => {
  t.key("Q");
  await t.until("the spot tool", () => sp.on && !$("spot-panel").hidden);
  const steps = t.historyLength();
  $("toast").hidden = true;
  t.click("#spot-dust");
  await t.until("its note", () => !$("toast").hidden && $("toast").textContent.includes("dust"), 60000);
  t.expect($("toast").textContent.startsWith("No dust found"), `the note: ${$("toast").textContent}`);
  t.expect(t.historyLength() === steps, "no history step");
  // Shift-drag heals along a straight line, as one step.
  const before = spotList().length;
  const c = $("spot-canvas").getBoundingClientRect();
  const pt = (fx, fy, extra = {}) => ({ clientX: c.left + c.width * fx, clientY: c.top + c.height * fy, button: 0, shiftKey: true, bubbles: true, pointerId: 1, ...extra });
  $("spot-canvas").dispatchEvent(new PointerEvent("pointerdown", pt(0.3, 0.5)));
  $("spot-canvas").dispatchEvent(new PointerEvent("pointermove", pt(0.5, 0.52)));
  $("spot-canvas").dispatchEvent(new PointerEvent("pointerup", pt(0.7, 0.55)));
  await t.until("the scratch healed", () => spotList().length > before, 30000);
  t.expect(t.historyLength() === steps + 1, "one history step");
  t.key("Ctrl+Z");
  await t.until("undone", () => spotList().length === before);
  // Z and the 100% button zoom with the tool open, and the tool stays.
  t.expect(!$("btn-zoom").disabled, "the 100% button works in spot removal");
  t.key("z");
  await t.until("Z zooms in", () => ui.zoom.on);
  t.click("#btn-zoom");
  await t.until("and out", () => !ui.zoom.on);
  t.expect(sp.on, "still in spot removal");
  // Zoomed in, opening the tool keeps the zoom.
  exitSpots();
  setZoom(true);
  await t.until("zoomed", () => ui.zoom.on);
  t.key("Q");
  await t.until("the spot tool", () => sp.on);
  t.expect(ui.zoom.on, "the zoom kept");
  setZoom(false);
  await t.until("fit", () => !ui.zoom.on);
  // Ctrl + scroll zooms in at the pointer (not the spot size); arrows and a
  // right-drag move the zoomed-in photo with the tool open; Ctrl + scroll
  // back out.
  const size = sp.size;
  const wheelAt = (deltaY) => $("spot-canvas").dispatchEvent(new WheelEvent("wheel", {
    deltaY, ctrlKey: true, clientX: c.left + c.width / 2, clientY: c.top + c.height / 2, bubbles: true, cancelable: true }));
  wheelAt(-100);
  await t.until("zoomed in", () => ui.zoom.on && ui.zoom.meta);
  t.expect(sp.size === size, `the spot size stays (${sp.size})`);
  const cx = ui.zoom.cx;
  t.key("ArrowRight");
  await t.until("moved right", () => ui.zoom.cx > cx);
  const cy = ui.zoom.cy;
  const mouse = (type, x, y) => $("spot-canvas").dispatchEvent(new MouseEvent(type, { button: 2, clientX: x, clientY: y, bubbles: true }));
  mouse("mousedown", c.left + 300, c.top + 300);
  window.dispatchEvent(new MouseEvent("mousemove", { clientX: c.left + 300, clientY: c.top + 200 }));
  window.dispatchEvent(new MouseEvent("mouseup", { button: 2, clientX: c.left + 300, clientY: c.top + 200 }));
  await t.until("a right-drag moved it", () => ui.zoom.cy > cy);
  t.expect(sp.on, "still in spot removal");
  wheelAt(100);
  await t.until("the whole photo", () => !ui.zoom.on);
  // The Size slider sets the next spot's size as it moves, up to 25 %.
  const was = sp.size;
  $("spot-size").value = "20";
  $("spot-size").dispatchEvent(new Event("input"));
  t.expect(sp.size === 20 && $("spot-size").max === "25", `size ${sp.size}, max ${$("spot-size").max}`);
  $("spot-size").value = String(was);
  $("spot-size").dispatchEvent(new Event("input"));
  // Remove: a click makes a content-aware spot with no source.
  t.click('#spot-kind [data-kind="remove"]');
  t.expect(document.querySelector('#spot-kind [data-kind="remove"]').classList.contains("active"), "Remove chosen");
  const click = (extra) => pt(0.4, 0.4, { shiftKey: false, ...extra });
  $("spot-canvas").dispatchEvent(new PointerEvent("pointerdown", click()));
  $("spot-canvas").dispatchEvent(new PointerEvent("pointerup", click()));
  await t.until("the Remove spot", () => spotList().length > before, 30000);
  const r = spotList()[spotList().length - 1];
  t.expect(r.kind === "content-aware" && !r.source, `a Remove spot: ${JSON.stringify(r)}`);
  t.key("Ctrl+Z");
  await t.until("undone", () => spotList().length === before);
  t.click('#spot-kind [data-kind="heal"]');
  t.click("#spot-done");
  await t.until("the spot tool closed", () => !sp.on);
});

await t.step("a brushed spot is drawn along its stroke and moves by dragging it", async () => {
  t.key("Q");
  await t.until("the spot tool", () => sp.on);
  const before = spotList().length;
  const c = $("spot-canvas").getBoundingClientRect();
  const pt = (fx, fy) => ({ clientX: c.left + c.width * fx, clientY: c.top + c.height * fy, button: 0, bubbles: true, pointerId: 1 });
  const drag = (path) => {
    $("spot-canvas").dispatchEvent(new PointerEvent("pointerdown", pt(...path[0])));
    for (const p of path.slice(1)) $("spot-canvas").dispatchEvent(new PointerEvent("pointermove", pt(...p)));
    $("spot-canvas").dispatchEvent(new PointerEvent("pointerup", pt(...path.at(-1))));
  };
  drag([[0.3, 0.3], [0.35, 0.3], [0.4, 0.32], [0.45, 0.33]]);
  await t.until("the brushed spot", () => spotList().length > before, 30000);
  const s = spotList().at(-1);
  t.expect(s.shape === "brush" && s.at && s.strokes?.[0]?.length > 1, `listed with its stroke: ${JSON.stringify(s).slice(0, 200)}`);
  const steps = t.historyLength();
  // Grab it on its stroke and move it down by a tenth of the photo.
  drag([[0.35, 0.3], [0.35, 0.35], [0.35, 0.4]]);
  await t.until("moved", () => t.historyLength() === steps + 1, 30000);
  const m = spotList().find((x) => x.index === s.index);
  t.expect(spotList().length === before + 1, "no new spot from the drag");
  t.expect(m.at[1] > s.at[1] + 0.03 && Math.abs(m.at[0] - s.at[0]) < 0.01, `moved from ${s.at} to ${m.at}`);
  t.key("Ctrl+Z");
  t.key("Ctrl+Z");
  await t.until("undone", () => spotList().length === before);
  t.click("#spot-done");
  await t.until("the spot tool closed", () => !sp.on);
});

await t.step("Remove object: Shift+Q opens the tool; a box sends its corners, a click its point; Esc cancels", async () => {
  t.key("Shift+Q");
  await t.until("box mode", () => sp.on && sp.object && $("spot-object").classList.contains("active"));
  t.expect($("spot-hint").textContent.includes("box"), $("spot-hint").textContent);
  t.expect($("spot-panel").firstElementChild.nextElementSibling === $("spot-object"), "the panel's first button");
  // A drag draws a box and asks for that box (the model isn't run here).
  const spots = spotList().length;
  const asked = [];
  const real = window.removeObject;
  window.removeObject = async (target) => { asked.push(target); setObjectMode(false); };
  const c = $("spot-canvas").getBoundingClientRect();
  const pt = (fx, fy) => ({ clientX: c.left + c.width * fx, clientY: c.top + c.height * fy, button: 0, bubbles: true, pointerId: 1 });
  $("spot-canvas").dispatchEvent(new PointerEvent("pointerdown", pt(0.6, 0.6)));
  $("spot-canvas").dispatchEvent(new PointerEvent("pointermove", pt(0.4, 0.5)));
  $("spot-canvas").dispatchEvent(new PointerEvent("pointerup", pt(0.3, 0.3)));
  window.removeObject = real;
  t.expect(asked.length === 1 && asked[0].rect, `asked ${JSON.stringify(asked)}`);
  const [x0, y0, x1, y1] = asked[0].rect;
  t.expect(x0 < x1 && y0 < y1, `corners in order: ${asked[0].rect}`);
  // A click asks for the object under it.
  setObjectMode(true);
  window.removeObject = async (target) => { asked.push(target); setObjectMode(false); };
  $("spot-canvas").dispatchEvent(new PointerEvent("pointerdown", pt(0.5, 0.5)));
  $("spot-canvas").dispatchEvent(new PointerEvent("pointerup", pt(0.5, 0.5)));
  window.removeObject = real;
  t.expect(asked.length === 2 && asked[1].point?.length === 2, `a click: ${JSON.stringify(asked[1])}`);
  t.expect(!sp.object && spotList().length === spots, "one box, then back to spots; no spot drawn");
  // Esc leaves box mode first, then the tool.
  t.click("#spot-object");
  t.expect(sp.object, "the button turns it on");
  t.key("Escape");
  t.expect(!sp.object && sp.on, "Esc ends box mode, the tool stays");
  t.key("Escape");
  await t.until("the spot tool closed", () => !sp.on);
});
