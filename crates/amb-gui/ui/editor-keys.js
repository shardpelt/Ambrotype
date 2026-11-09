// SPDX-License-Identifier: GPL-3.0-or-later
// Editor mode's keyboard: the keys while the interface is being edited. Split
// out of editor.js.
"use strict";

// ---------------------------------------------------------------- keyboard

// Alt+arrows move the focused panel, setting or toolbar button one place;
// Enter opens its options and Delete removes it.
function keyboardEdit(e) {
  const what = editTarget(document.activeElement ?? document.body);
  if (!what) return false;
  if (e.key === "Enter" && !e.altKey) {
    const r = what.el.getBoundingClientRect();
    openInspector(what, r.left + 12, r.top + r.height / 2);
    return true;
  }
  if (e.key === "Delete") {
    removeThing(what);
    return true;
  }
  if (!e.altKey || !e.key.startsWith("Arrow")) return false;
  const dir = { ArrowUp: -1, ArrowLeft: -1, ArrowDown: 1, ArrowRight: 1 }[e.key];
  const across = e.key === "ArrowLeft" || e.key === "ArrowRight";
  if (what.kind === "tool") {
    if (!across || what.index < 0) return true;
    const to = what.index + (dir < 0 ? -1 : 2);
    if (to < 0) return true;
    edit((d) => moveTool(d, what, { index: to }));
    ui.edit.refocus = `.topbar .actions [data-tool-index="${what.index + dir}"]`;
  } else if (what.kind === "item") {
    const d = ui.edit.draft;
    if (across) {
      const side = what.side === "left" ? "right" : "left";
      const index = Math.min(what.i, d[side]?.items?.length ?? 0);
      ui.edit.refocus = `.column [data-item="${side}:${index}"]`;
      edit((dr) => moveItem(dr, what, { side, index }));
    } else {
      const len = d[what.side]?.items?.length ?? 0;
      const at = what.i + dir;
      if (at < 0 || at >= len) return true;
      ui.edit.refocus = `.column [data-item="${what.side}:${at}"]`;
      edit((dr) => moveItem(dr, what, { side: what.side, index: dir < 0 ? at : at + 1 }));
    }
  } else if (what.kind === "unit" && !across) {
    const panel = what.el.closest(".panel[data-item]");
    const keys = [...new Set([...panel.querySelectorAll(".panel-body > [data-unit]")].map((n) => n.dataset.unit).filter(Boolean))];
    const at = keys.indexOf(what.key) + dir;
    if (at < 0 || at >= keys.length) return true;
    const beforeKey = dir < 0 ? keys[at] : keys[at + 1] ?? null;
    // The panel becomes a list of its own: its entries are j0, j1, ...
    ui.edit.refocus = `.column [data-item="${what.side}:${what.i}"] [data-unit="j${at}"]`;
    edit((d) => moveUnit(d, what, { type: "panel", side: what.side, i: what.i, beforeKey }));
  }
  status(`Moved ${what.label || "it"}`);
  return true;
}

function beginDrag(e, what) {
  const x0 = e.clientX, y0 = e.clientY;
  let ghost = null, drop = null;
  const move = (ev) => {
    if (!ghost && Math.hypot(ev.clientX - x0, ev.clientY - y0) < 5) return;
    if (!ghost) {
      ghost = document.createElement("div");
      ghost.className = "edit-ghost";
      ghost.textContent = what.label || "Item";
      document.body.append(ghost);
      document.body.classList.add("edit-dragging");
      what.el?.classList.add("edit-lifted");
    }
    ghost.style.transform = `translate(${ev.clientX + 14}px, ${ev.clientY + 10}px)`;
    drop = findDrop(what, ev.clientX, ev.clientY);
    showIndicator(drop);
  };
  const up = (ev) => {
    window.removeEventListener("pointermove", move, true);
    window.removeEventListener("pointerup", up, true);
    showIndicator(null);
    document.body.classList.remove("edit-dragging");
    what.el?.classList.remove("edit-lifted");
    if (!ghost) {
      if (what.kind === "palette") addFromPalette(what.payload);
      else openInspector(what, ev.clientX, ev.clientY);
      return;
    }
    ghost.remove();
    if (drop) performDrop(what, drop);
  };
  window.addEventListener("pointermove", move, true);
  window.addEventListener("pointerup", up, true);
}

const isEntryDrag = (w) => w.kind === "unit" || (w.kind === "palette" && ["setting", "composite", "include", "label"].includes(w.payload.add));
const isToolDrag = (w) => w.kind === "tool" || (w.kind === "palette" && w.payload.add === "tool");

function findDrop(what, x, y) {
  const el = document.elementFromPoint(x, y);
  if (!el) return null;
  if (what.kind !== "palette" && el.closest("#edit-palette")) {
    const r = $("edit-palette").getBoundingClientRect();
    return { remove: true, box: r };
  }
  if (isToolDrag(what)) return el.closest(".topbar") ? toolbarDrop(x) : null;
  const column = el.closest(".column");
  if (!column) return null;
  const side = column.id === "col-left" ? "left" : "right";
  if (isEntryDrag(what)) {
    const panel = el.closest(".panel[data-item]");
    if (panel) return panelDrop(panel, x, y);
  }
  return columnDrop(column, side, y);
}

function columnDrop(column, side, y) {
  const kids = [...column.children].filter((n) => n.dataset.item);
  const cr = column.getBoundingClientRect();
  for (const k of kids) {
    const r = k.getBoundingClientRect();
    if (y < r.top + r.height / 2) {
      return { type: "column", side, index: Number(k.dataset.item.split(":")[1]), line: [cr.left, r.top, cr.width, 0] };
    }
  }
  const last = kids.at(-1)?.getBoundingClientRect();
  return { type: "column", side, index: ui.edit.draft[side]?.items?.length ?? 0, line: [cr.left, last ? last.bottom : cr.top + 4, cr.width, 0] };
}

function panelDrop(panel, x, y) {
  const [side, i] = panel.dataset.item.split(":");
  const body = panel.querySelector(".panel-body");
  const br = body.getBoundingClientRect();
  // Group the nodes of each entry (an include renders several).
  const units = [];
  for (const n of body.querySelectorAll(":scope > [data-unit]")) {
    if (!n.dataset.unit) continue;
    const r = n.getBoundingClientRect();
    const last = units.at(-1);
    if (last && last.key === n.dataset.unit) {
      last.rect = { left: Math.min(last.rect.left, r.left), top: Math.min(last.rect.top, r.top), right: Math.max(last.rect.right, r.right), bottom: Math.max(last.rect.bottom, r.bottom) };
    } else units.push({ key: n.dataset.unit, rect: { left: r.left, top: r.top, right: r.right, bottom: r.bottom } });
  }
  for (const u of units) {
    const r = u.rect;
    const narrow = r.right - r.left < br.width * 0.7; // a knob in a grid
    if (narrow) {
      if (y < r.top || (y <= r.bottom && x < (r.left + r.right) / 2)) {
        return { type: "panel", side, i: Number(i), beforeKey: u.key, line: [r.left - 2, r.top, 0, r.bottom - r.top] };
      }
    } else if (y < (r.top + r.bottom) / 2) {
      return { type: "panel", side, i: Number(i), beforeKey: u.key, line: [br.left, r.top - 1, br.width, 0] };
    }
  }
  const lastR = units.at(-1)?.rect;
  return { type: "panel", side, i: Number(i), beforeKey: null, line: [br.left, lastR ? lastR.bottom + 1 : br.top + 4, br.width, 0] };
}

function toolbarDrop(x) {
  const nodes = [...document.querySelectorAll(".topbar .actions > [data-tool]")].filter((n) => !n.hidden);
  const list = toolbarList(structuredClone(ui.edit.draft));
  for (const n of nodes) {
    const r = n.getBoundingClientRect();
    if (x < (r.left + r.right) / 2) {
      const index = Number(n.dataset.toolIndex);
      return { type: "toolbar", index: index >= 0 ? index : list.length, line: [r.left - 3, r.top, 0, r.height] };
    }
  }
  const r = nodes.at(-1)?.getBoundingClientRect();
  return { type: "toolbar", index: list.length, line: r ? [r.right + 2, r.top, 0, r.height] : null };
}

function showIndicator(drop) {
  const ind = $("edit-indicator");
  const pal = $("edit-palette");
  pal.classList.toggle("drop-remove", Boolean(drop?.remove));
  if (!drop?.line) { ind.hidden = true; return; }
  const [x, y, w, h] = drop.line;
  Object.assign(ind.style, { left: `${x}px`, top: `${y - (h ? 0 : 1)}px`, width: `${w || 3}px`, height: `${h || 3}px` });
  ind.hidden = false;
}

function performDrop(what, drop) {
  if (drop.remove) return removeThing(what);
  edit((d) => {
    if (what.kind === "item" && drop.type === "column") moveItem(d, what, drop);
    else if (what.kind === "unit" && drop.type !== "toolbar") moveUnit(d, what, drop);
    else if (what.kind === "tool" && drop.type === "toolbar") moveTool(d, what, drop);
    else if (what.kind === "palette") insertFromPalette(d, what.payload, drop);
  });
}

function removeThing(what) {
  if (what.kind === "tool") {
    if (what.name === "looks") return toast("Appearance always stays, so you can get back to your layouts");
    if (what.index < 0) return;
    edit((d) => { toolbarList(d).splice(what.index, 1); });
  } else if (what.kind === "item") {
    edit((d) => { col(d, what.side).items.splice(what.i, 1); });
  } else if (what.kind === "unit") {
    edit((d) => removeUnit(d, what));
  }
}
