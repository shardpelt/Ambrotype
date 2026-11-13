// SPDX-License-Identifier: GPL-3.0-or-later
// The shared sideways row of photos (core/photo-row.js): the filmstrip and the Map's tray scroll
// with the wheel and with a drag, click as before, and the tray still drags a photo out.
// Run: ambrotype --ui-test crates/amb-gui/tests/ui/photo-row.js photo.dng

await t.step("the photo opens and the filmstrip shows it", async () => {
  await t.until("the photo", () => ui.info?.path, 60000);
  await t.until("a filmstrip cell", () => strip.cells.size > 0 && t.visible("#filmstrip"));
});

// A row that is too short for its photos, from the test (the fixture has few).
const narrow = (list, px) => {
  list.style.width = `${px}px`;
  list.style.flex = "none";
  list.scrollLeft = 0;
  t.expect(list.scrollWidth > list.clientWidth, `the row overflows: ${list.scrollWidth} > ${list.clientWidth}`);
};
const wheel = (list, init) => {
  const e = new WheelEvent("wheel", { bubbles: true, cancelable: true, ...init });
  list.dispatchEvent(e);
  return e;
};
const drag = async (list, cell, dx) => {
  const at = cell.getBoundingClientRect();
  const ev = (type, x) => cell.dispatchEvent(new PointerEvent(type, { bubbles: true, clientX: x, clientY: at.top + 10, pointerId: 7, button: 0, pointerType: "mouse" }));
  ev("pointerdown", at.left + 40);
  ev("pointermove", at.left + 40 + dx / 2);
  ev("pointermove", at.left + 40 + dx);
  ev("pointerup", at.left + 40 + dx);
  cell.dispatchEvent(new MouseEvent("click", { bubbles: true }));
  await t.sleep(50);
};

await t.step("filmstrip: the wheel and a drag scroll sideways, a click opens", async () => {
  const list = $("filmstrip-list");
  const was = [strip.photos, openPath];
  const second = { path: `${ui.info.path}#2`, name: "second" };
  strip.photos = [...strip.photos, second];
  renderStrip();
  narrow(list, 60);
  const e = wheel(list, { deltaY: 60 });
  t.expect(e.defaultPrevented, "the wheel is taken while the row can scroll");
  await t.until("scrolled sideways by the wheel", () => list.scrollLeft > 0);
  list.classList.add("dragging"); // no smooth scrolling in the test's reads
  list.scrollLeft = list.scrollWidth;
  await t.sleep(50);
  t.expect(!wheel(list, { deltaY: 60 }).defaultPrevented, "at the end the wheel is left to the page");
  list.scrollLeft = 0;
  await t.sleep(50);
  list.classList.remove("dragging");
  let opened = null;
  openPath = async (path) => { opened = path; return true; };
  const first = strip.cells.get(second.path);
  const x0 = list.scrollLeft;
  await drag(list, first, -40);
  t.expect(list.scrollLeft > x0 && opened === null, `a sideways drag moves along and opens nothing: ${list.scrollLeft}`);
  strip.cells.get(second.path).dispatchEvent(new MouseEvent("click", { bubbles: true }));
  await t.sleep(20);
  t.expect(opened === second.path, `a plain click opens: ${opened}`);
  openPath = was[1];
  list.removeAttribute("style");
  strip.photos = was[0];
  renderStrip();
});

await t.step("Map tray: the wheel and a drag scroll, a click selects, a drag up still drags out", async () => {
  await openFolder(dirOf(ui.info.path));
  await t.until("the grid", () => libraryOn() && lib.photos.length > 0);
  t.click('#lib-views [data-view="map"]');
  await t.until("the tray", () => document.querySelector(".map-tray .map-thumb"));
  const tray = document.querySelector(".map-tray");
  const thumb = tray.querySelector(".map-thumb");
  narrow(tray, 90);
  t.expect(thumb.draggable, "photos can be dragged out");
  const e = wheel(tray, { deltaY: 40 });
  t.expect(e.defaultPrevented, "the wheel is taken");
  await t.until("scrolled by the wheel", () => tray.scrollLeft > 0);
  tray.scrollLeft = 0;
  const before = [...lib.selected].join("|");
  await drag(tray, thumb, -40);
  t.expect(tray.scrollLeft > 0, `a sideways drag scrolls the tray: ${tray.scrollLeft}`);
  t.expect([...lib.selected].join("|") === before, "and selects nothing");
  thumb.click();
  t.expect(lib.selected.has(thumb.dataset.path) && thumb.classList.contains("selected"), "a plain click selects");
  // dragstart after a press: up is left to the browser, sideways scrolls instead.
  const at = thumb.getBoundingClientRect();
  const dragStart = (dx, dy) => {
    thumb.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, clientX: at.left + 20, clientY: at.top + 20, pointerId: 8, button: 0, pointerType: "mouse" }));
    const ev = new Event("dragstart", { bubbles: true, cancelable: true });
    const got = [];
    Object.assign(ev, { clientX: at.left + 20 + dx, clientY: at.top + 20 + dy, dataTransfer: { setData: (k, v) => got.push([k, v]) } });
    thumb.dispatchEvent(ev);
    thumb.dispatchEvent(new PointerEvent("pointercancel", { bubbles: true, pointerId: 8 }));
    return { prevented: ev.defaultPrevented, got };
  };
  const up = dragStart(3, -30);
  t.expect(!up.prevented && up.got[0]?.[0] === "text/x-amb-photo" && up.got[0][1] === thumb.dataset.path, "mostly vertical: the native drag starts and carries the photo");
  const side = dragStart(30, 3);
  t.expect(side.prevented && !side.got.length, "mostly sideways: the native drag is cancelled");
  tray.removeAttribute("style");
});
