// SPDX-License-Identifier: GPL-3.0-or-later
// A sideways row of photos, shared by every place that shows one: the
// filmstrip, the Map's tray, the dialogs' strips. The row owns what they
// all need: a vertical wheel scrolls it sideways, press-and-drag scrolls it
// with a glide (`dragToScroll`, also the library grid's), cells load their
// thumbnails when they come near the view, and a click, Ctrl/Shift-click
// or right-click goes to the callbacks. State is per row.
"use strict";

// Press and drag a list to move along it, as on a touch screen; let go
// while moving and it glides to a stop. A click that didn't drag still
// works. `axis` is "x" (a photo row) or "y" (the library grid); `when`
// says whether a press may start a drag (Shift- and Ctrl-clicks select);
// `native(e)` says whether a `dragstart` is left to the browser (a cell
// that can be dragged out) instead of being cancelled so the list scrolls.
// `handOff(e)`, asked at a press's first move past 5 px, ends the press
// there when it returns true: no scroll (a cell pulled out of the row).
function dragToScroll(list, axis = "x", when = () => true, native = () => false, handOff = () => false) {
  const along = axis === "x" ? "clientX" : "clientY";
  const scroll = axis === "x" ? "scrollLeft" : "scrollTop";
  let drag = null;
  let glide = 0;
  list.addEventListener("dragstart", (e) => {
    if (!native(e)) e.preventDefault();
  });
  list.addEventListener("pointerdown", (e) => {
    if (e.button !== 0 || e.pointerType === "touch" || e.shiftKey || e.ctrlKey || e.metaKey || !when(e)) return;
    cancelAnimationFrame(glide);
    list.classList.remove("dragging");
    drag = { at: e[along], start: list[scroll], moved: false, id: e.pointerId, v: 0, t: performance.now(), last: e[along] };
  });
  list.addEventListener("pointermove", (e) => {
    if (!drag || e.pointerId !== drag.id) return;
    const d = e[along] - drag.at;
    if (!drag.moved && Math.abs(d) < 5) return;
    if (!drag.moved) {
      if (handOff(e)) { drag = null; return; }
      drag.moved = true;
      try { list.setPointerCapture(e.pointerId); } catch { /* a pointer the page made up (tests) */ }
      list.classList.add("dragging");
    }
    list[scroll] = drag.start - d;
    const now = performance.now();
    if (now > drag.t) drag.v = 0.8 * drag.v + 0.2 * ((drag.last - e[along]) / (now - drag.t));
    drag.t = now;
    drag.last = e[along];
  });
  const end = (e) => {
    if (!drag || e.pointerId !== drag.id) return;
    const d = drag;
    drag = null;
    if (!d.moved) return;
    // The click this release makes isn't a choice of photo.
    const eat = (c) => { c.stopPropagation(); c.preventDefault(); };
    list.addEventListener("click", eat, { capture: true, once: true });
    setTimeout(() => list.removeEventListener("click", eat, { capture: true }), 0);
    // ("dragging" also turns smooth scrolling off for the glide.)
    let v = performance.now() - d.t > 80 ? 0 : d.v; // pixels per ms
    let prev = performance.now();
    const step = (now) => {
      const dt = now - prev;
      prev = now;
      list[scroll] += v * dt;
      v *= Math.pow(0.995, dt);
      if (Math.abs(v) > 0.02) glide = requestAnimationFrame(step);
      else list.classList.remove("dragging");
    };
    if (Math.abs(v) > 0.1) glide = requestAnimationFrame(step);
    else list.classList.remove("dragging");
  };
  list.addEventListener("pointerup", end);
  list.addEventListener("pointercancel", end);
}

// photoRow(list, options) -> { set(photos), cells, mark(), reveal(path), destroy() }
// `list` is the element that scrolls sideways. Options, all optional:
//   cell(p) -> Element       builds one cell (the row sets data-path on it)
//   thumb(p, cell)           once, when the cell first comes near the view
//   open(p, e)               a plain click
//   select(p, how)           "toggle" (Ctrl/⌘-click) or "range" (Shift-click)
//   isSelected(p), isCurrent(p)   the "selected" and "current" classes
//   menu(e, p)               right-click
//   dragOut(p, e)            cells can be dragged out of the row; fills
//                            e.dataTransfer. A mostly sideways drag still
//                            scrolls, a mostly vertical one drags out.
//   pullOut(p, e)            cells can be pulled out by the pointer, for a
//                            drag the caller runs itself (the collage's
//                            tray). A press that first moves mostly up or
//                            down calls it once with that move; the row
//                            doesn't scroll for that press and ignores the
//                            click that ends it. For touch the caller's
//                            cells need `touch-action: pan-x`.
//   before(p, prev) -> Element | null   an element put before p's cell, not
//                            a cell (a group head); prev is the photo
//                            before p, or null.
function photoRow(list, o = {}) {
  list.classList.add("photo-row");
  const cells = new Map(); // path -> cell
  const byPath = new Map(); // path -> photo
  const extras = []; // what before(p, prev) put between the cells
  let observer = null;
  let pressed = null; // where the pointer went down, for dragstart
  const photoOf = (e) => {
    const cell = e.target.closest?.("[data-path]");
    return cell && list.contains(cell) ? byPath.get(cell.dataset.path) : null;
  };

  list.addEventListener("wheel", (e) => {
    if (Math.abs(e.deltaY) <= Math.abs(e.deltaX)) return;
    const dy = e.deltaMode === 1 ? e.deltaY * 16 : e.deltaY;
    const room = dy < 0 ? list.scrollLeft : list.scrollWidth - list.clientWidth - list.scrollLeft;
    if (room < 1) return; // at the end (or nothing to scroll): the page may use it
    e.preventDefault();
    list.scrollLeft += dy;
  }, { passive: false });

  list.addEventListener("pointerdown", (e) => { pressed = { x: e.clientX, y: e.clientY, p: photoOf(e), done: false, pulled: false }; });
  const pressEnds = () => { if (pressed) pressed.done = true; };
  list.addEventListener("pointerup", pressEnds);
  list.addEventListener("pointercancel", pressEnds);
  const verticalDrag = (e) => !pressed || Math.abs(e.clientX - pressed.x) < Math.abs(e.clientY - pressed.y);
  // Before dragToScroll's own listener, so a pull is known when it asks.
  if (o.pullOut) {
    list.addEventListener("pointermove", (e) => {
      if (!pressed?.p || pressed.done) return;
      if (e.pointerType !== "touch" && !(e.buttons & 1)) return;
      if (Math.hypot(e.clientX - pressed.x, e.clientY - pressed.y) < 5) return;
      pressed.done = true;
      if (!verticalDrag(e)) return;
      pressed.pulled = true;
      o.pullOut(pressed.p, e);
    });
  }
  dragToScroll(list, "x", () => true, (e) => Boolean(o.dragOut) && verticalDrag(e), () => Boolean(pressed?.pulled));
  list.addEventListener("dragstart", (e) => {
    const p = o.dragOut && !e.defaultPrevented && photoOf(e);
    if (p) o.dragOut(p, e);
  });

  list.addEventListener("click", (e) => {
    const p = photoOf(e);
    if (!p) return;
    if (pressed?.pulled) return;
    if ((e.ctrlKey || e.metaKey || e.shiftKey) && o.select) o.select(p, e.shiftKey ? "range" : "toggle");
    else o.open?.(p, e);
  });
  list.addEventListener("contextmenu", (e) => {
    const p = photoOf(e);
    if (p && o.menu) o.menu(e, p);
  });

  const row = {
    cells,
    set(photos) {
      observer?.stop();
      observer = null;
      for (const cell of cells.values()) cell.remove();
      for (const x of extras.splice(0)) x.remove();
      cells.clear();
      byPath.clear();
      if (o.thumb) {
        observer = nearView(list, "300px", (cell) => {
          const p = byPath.get(cell.dataset.path);
          if (p) o.thumb(p, cell);
        });
      }
      let prev = null;
      for (const p of photos) {
        const head = o.before?.(p, prev);
        if (head) {
          extras.push(head);
          list.append(head);
        }
        prev = p;
        const cell = o.cell ? o.cell(p) : el("button", { type: "button", title: p.name });
        cell.dataset.path = p.path;
        if (o.dragOut) cell.draggable = true;
        cells.set(p.path, cell);
        byPath.set(p.path, p);
        list.append(cell);
        observer?.watch(cell);
      }
      row.mark();
    },
    mark() {
      for (const [path, cell] of cells) {
        const p = byPath.get(path);
        cell.classList.toggle("selected", Boolean(o.isSelected?.(p)));
        if (!o.isCurrent) continue;
        const on = Boolean(o.isCurrent(p));
        cell.classList.toggle("current", on);
        cell.setAttribute("aria-current", on ? "true" : "false");
      }
    },
    reveal(path) {
      cells.get(path)?.scrollIntoView({ block: "nearest", inline: "nearest" });
    },
    destroy() {
      observer?.stop();
      observer = null;
      cells.clear();
      byPath.clear();
      list.classList.remove("photo-row");
    },
  };
  return row;
}
