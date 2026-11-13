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
function dragToScroll(list, axis = "x", when = () => true, native = () => false) {
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
function photoRow(list, o = {}) {
  list.classList.add("photo-row");
  const cells = new Map(); // path -> cell
  const byPath = new Map(); // path -> photo
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

  list.addEventListener("pointerdown", (e) => { pressed = { x: e.clientX, y: e.clientY }; });
  const verticalDrag = (e) => !pressed || Math.abs(e.clientX - pressed.x) < Math.abs(e.clientY - pressed.y);
  dragToScroll(list, "x", () => true, (e) => Boolean(o.dragOut) && verticalDrag(e));
  list.addEventListener("dragstart", (e) => {
    const p = o.dragOut && !e.defaultPrevented && photoOf(e);
    if (p) o.dragOut(p, e);
  });

  list.addEventListener("click", (e) => {
    const p = photoOf(e);
    if (!p) return;
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
      observer?.disconnect();
      observer = null;
      for (const cell of cells.values()) cell.remove();
      cells.clear();
      byPath.clear();
      if (o.thumb) {
        observer = new IntersectionObserver((entries) => {
          for (const e of entries) {
            if (!e.isIntersecting) continue;
            observer.unobserve(e.target);
            const p = byPath.get(e.target.dataset.path);
            if (p) o.thumb(p, e.target);
          }
        }, { root: list, rootMargin: "300px" });
      }
      for (const p of photos) {
        const cell = o.cell ? o.cell(p) : el("button", { type: "button", title: p.name });
        cell.dataset.path = p.path;
        if (o.dragOut) cell.draggable = true;
        cells.set(p.path, cell);
        byPath.set(p.path, p);
        list.append(cell);
        observer?.observe(cell);
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
      observer?.disconnect();
      observer = null;
      cells.clear();
      byPath.clear();
      list.classList.remove("photo-row");
    },
  };
  return row;
}
