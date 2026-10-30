// SPDX-License-Identifier: GPL-3.0-or-later
// Editor mode: change the active layout in place.
//
// Drag panels, blocks, settings and toolbar buttons to move them (drop on the
// Add list to remove). Without a mouse: Tab to one, Alt+arrows move it
// (Alt+Left/Right moves a panel to the other column), Enter opens its
// options, Delete removes it. Click one to change its control type, label, title or
// columns, to split a panel or merge it with the next. "Add" lists what isn't
// shown yet. Drag a column's inner edge to resize it.
//
// Every change edits a draft copy of the layout file and re-renders it
// (layout.js), so what you see is exactly what gets saved, and undo is a
// stack of drafts. Save writes your own layout (a built-in one is saved as
// your copy); Cancel puts the old one back. Built-in panels stay built-in
// (they gain new settings when Ambrotype does) as long as you only hide
// settings or change their control type; reordering their contents turns
// them into your own panel.
"use strict";

const EDIT_ZONES = ".column, .topbar .actions";
const COMPOSITE_TITLES = { "point-curve": "Point curve", "color-wheels": "Colour wheels" };
const BLOCK_TITLES = { presets: "Presets", info: "Rating & keywords", history: "History", histogram: "Histogram" };
const TOOL_TITLES = {
  open: "Open", library: "Library", save: "Save", export: "Export", collage: "Collage", undo: "Undo", redo: "Redo", before: "Before / After",
  crop: "Crop", masks: "Masks", spots: "Spots", zoom: "100%", wb: "WB picker", clip: "Clipping", frame: "Frame", keys: "Shortcuts", hub: "Browse", plugins: "Plugins", looks: "Appearance", edit: "Edit layout", settings: "Settings",
  feedback: "Feedback", "|": "Separator",
};

// ---------------------------------------------------------------- start, stop, undo

// A blank canvas: no panels, no toolbar buttons, empty columns to drop
// things into from Add.
const BLANK_LAYOUT = {
  format: "ambrotype-layout/1",
  name: "New layout",
  toolbar: [],
  left: { width: 250, items: [] },
  right: { width: 330, items: [] },
};

// `blank`: start a new layout from nothing instead of changing this one.
function startEditing(blank = false) {
  if (ui.edit) return;
  if (ui.crop.on) exitCrop(false);
  exitMasks();
  exitSpots();
  const look = blank ? null : findLook("layout", ui.looks?.active_layout);
  const draft = structuredClone(blank ? BLANK_LAYOUT : look?.usable ? look.data : ui.layout);
  ui.edit = {
    look,
    original: ui.layout,
    theme: ui.looks?.active_theme,
    draft,
    past: [],
    future: [],
    target: null, // the panel "Add" puts settings into (the last one clicked)
    palette: false,
  };
  document.body.classList.add("editing");
  $("edit-bar").hidden = false;
  $("edit-name").value = blank ? "" : look && !look.builtin ? look.name : `${draft.name ?? "Layout"} (mine)`;
  $("edit-name").placeholder = blank ? "New layout" : "";
  $("edit-density").value = ["compact", "large"].includes(draft.density) ? draft.density : "normal";
  $("edit-theme").replaceChildren(...(ui.looks?.themes ?? []).filter((t) => t.usable).map((t) =>
    Object.assign(document.createElement("option"), { value: t.id, textContent: t.name, selected: t.id === ui.edit.theme })));
  updateEditBar();
  applyLayout(draft);
  if (blank) {
    togglePalette(true);
    status("A blank layout: drag panels, blocks and buttons from Add into the columns and the toolbar");
    return;
  }
  status("Editing the layout: drag things to move them, click one to change it");
}

async function finishEditing(save) {
  const e = ui.edit;
  if (!e) return;
  if (save) {
    const name = $("edit-name").value.trim() || e.draft.name || "My layout";
    const mine = e.look && !e.look.builtin;
    const data = { ...e.draft, name };
    if (!mine && e.look) data.based_on = e.look.id;
    try {
      const r = await invoke("save_look", { kind: "layout", id: mine ? e.look.id : null, data });
      ui.looks = r.looks;
      ui.looks.active_layout = r.id;
      ui.looks.active_theme = e.theme;
      await invoke("set_active_look", { layout: r.id, theme: e.theme });
      toast(`Saved layout "${name}"`);
    } catch (err) {
      // Stay in editor mode so nothing is lost.
      toast(`Not saved: ${err}`, true);
      return;
    }
  }
  ui.edit = null;
  closeInspector();
  $("edit-palette").hidden = true;
  $("edit-bar").hidden = true;
  document.body.classList.remove("editing");
  $("workspace").querySelectorAll(".col-resizer").forEach((n) => n.remove());
  if (save) applyActiveLooks();
  else {
    applyTheme(findLook("theme", ui.looks?.active_theme)?.data ?? null);
    applyLayout(e.original);
  }
  status("Ready");
}

// One undoable change to the draft.
function edit(change) {
  const e = ui.edit;
  const before = JSON.stringify(e.draft);
  change(e.draft);
  if (JSON.stringify(e.draft) === before) return;
  e.past.push(before);
  e.future = [];
  closeInspector();
  applyLayout(e.draft);
  updateEditBar();
}

function editUndo(redo = false) {
  const e = ui.edit;
  const [from, to] = redo ? [e.future, e.past] : [e.past, e.future];
  if (!from.length) return;
  to.push(JSON.stringify(e.draft));
  e.draft = JSON.parse(from.pop());
  closeInspector();
  $("edit-density").value = ["compact", "large"].includes(e.draft.density) ? e.draft.density : "normal";
  applyLayout(e.draft);
  updateEditBar();
}

function updateEditBar() {
  $("edit-undo").disabled = !ui.edit.past.length;
  $("edit-redo").disabled = !ui.edit.future.length;
  $("edit-add").classList.toggle("active", ui.edit.palette);
}

// Called by applyLayout while editing: empty columns become drop strips, and
// visible ones get a resize handle on their inner edge.
function decorateEditing() {
  const d = ui.edit.draft;
  const ws = $("workspace");
  ws.querySelectorAll(".col-resizer").forEach((n) => n.remove());
  const widths = {};
  for (const side of ["left", "right"]) {
    const c = $(`col-${side}`);
    const items = d[side]?.items ?? [];
    if (!items.length) {
      c.hidden = false;
      const hint = document.createElement("div");
      hint.className = "drop-hint";
      hint.textContent = "Drop here";
      c.replaceChildren(hint);
      widths[side] = 64;
      continue;
    }
    // A column with items but no width is shown while editing, so it can be changed.
    const w = Number(d[side]?.width ?? DEFAULT_WIDTH[side]);
    widths[side] = w > 0 ? w : DEFAULT_WIDTH[side];
    c.hidden = false;
    const grip = document.createElement("div");
    grip.className = "col-resizer";
    grip.title = "Drag to resize this column";
    grip.style[side] = `${widths[side] - 4}px`;
    grip.addEventListener("pointerdown", (ev) => resizeColumn(ev, side, grip));
    ws.append(grip);
  }
  ws.style.gridTemplateColumns = `${widths.left}px minmax(0, 1fr) ${widths.right}px`;
  // Everything that can be moved can be reached with Tab.
  for (const n of ws.querySelectorAll(".column [data-item], .column .panel-body [data-unit]")) {
    if (n.dataset.unit !== "") n.tabIndex = 0;
  }
  if (ui.edit.refocus) {
    const sel = ui.edit.refocus;
    ui.edit.refocus = null;
    requestAnimationFrame(() => document.querySelector(sel)?.focus());
  }
  // The edit bar sits over the middle of the photo area.
  requestAnimationFrame(() => {
    const v = $("viewer").getBoundingClientRect();
    Object.assign($("edit-bar").style, { left: `${v.left + v.width / 2}px`, maxWidth: `${v.width - 16}px` });
    if (ui.edit?.palette) renderPalette();
  });
}

function resizeColumn(ev, side, grip) {
  ev.preventDefault();
  ev.stopPropagation();
  const ws = $("workspace");
  const r = ws.getBoundingClientRect();
  const other = side === "left" ? $("col-right") : $("col-left");
  const otherW = other.hidden ? 0 : other.getBoundingClientRect().width;
  let w = 0;
  grip.classList.add("dragging");
  const move = (e) => {
    w = Math.round(Math.min(800, Math.max(160, side === "left" ? e.clientX - r.left : r.right - e.clientX)));
    const [l, rt] = side === "left" ? [w, otherW] : [otherW, w];
    ws.style.gridTemplateColumns = `${l}px minmax(0, 1fr) ${rt}px`;
    grip.style[side] = `${w - 4}px`;
  };
  const up = () => {
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", up);
    grip.classList.remove("dragging");
    if (w) {
      // The layout's own width now; a width dragged outside editor mode would hide it.
      delete colWidths[ui.looks?.active_layout]?.[side];
      saveColWidths();
      edit((d) => { col(d, side).width = w; });
    }
  };
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", up);
}

// ---------------------------------------------------------------- the draft

function col(d, side) {
  d[side] ??= { width: DEFAULT_WIDTH[side], items: [] };
  d[side].items ??= [];
  return d[side];
}

// Give a column a width when something first goes into it.
function ensureWidth(d, side) {
  if (!(Number(d[side].width) > 0)) d[side].width = DEFAULT_WIDTH[side];
}

function toolbarList(d) {
  if (!Array.isArray(d.toolbar)) d.toolbar = Object.keys(TOOLBAR_IDS);
  return d.toolbar;
}

// A rendered entry as a layout item.
function entryToItem(e, panelId) {
  if (e.type === "label") return { label: e.text };
  if (e.type === "control") return e.widget ? { setting: e.def.name, widget: e.widget } : { setting: e.def.name };
  if (e.type === "curve") return { widget: "point-curve" };
  if (e.type === "wheels") return { widget: "color-wheels" };
  return { include: `${panelId}/${e.section.label}` };
}

// A panel as your own list of items, with the key of each (see panelEntries).
// A built-in panel is converted in place; `cache` keeps one conversion per change.
function resolvePanel(d, side, i, cache) {
  const k = `${side}:${i}`;
  if (cache.has(k)) return cache.get(k);
  const item = d[side].items[i];
  let out;
  if (typeof item.panel === "string") {
    const p = schemaPanel(item.panel);
    const entries = panelEntries(p, null, item.widgets ?? {}, item.hide ?? []);
    const panel = { title: item.title ?? p.label, ...(item.open ? { open: true } : {}), items: entries.map((e) => entryToItem(e, p.id)) };
    d[side].items[i] = panel;
    out = { panel, keys: entries.map((e) => e.key) };
  } else {
    item.items ??= [];
    out = { panel: item, keys: item.items.map((_, j) => `j${j}`) };
  }
  cache.set(k, out);
  return out;
}

const entryAt = (r, key) => r.panel.items[r.keys.indexOf(key)];

function removeItem(d, obj) {
  for (const side of ["left", "right"]) {
    const items = d[side]?.items ?? [];
    const at = items.indexOf(obj);
    if (at >= 0) return items.splice(at, 1);
  }
}

function itemTitle(item) {
  if (item.setting) return item.label ?? ui.defs.get(item.setting)?.label ?? item.setting;
  if (item.include) {
    const [pid, section] = item.include.split("/");
    return section ?? schemaPanel(pid)?.label ?? item.include;
  }
  if (item.widget) return COMPOSITE_TITLES[item.widget] ?? item.widget;
  return item.label ?? "Panel";
}

// Put a layout item (a setting, widget, include or label) where a drop says.
function placeEntry(d, entry, to, cache, source = null) {
  if (to.type === "panel") {
    const t = resolvePanel(d, to.side, to.i, cache);
    const before = to.beforeKey == null ? null : entryAt(t, to.beforeKey);
    if (before === entry) return false;
    if (source) source.items.splice(source.items.indexOf(entry), 1);
    const at = before ? t.panel.items.indexOf(before) : t.panel.items.length;
    t.panel.items.splice(at, 0, entry);
    ui.edit.target = t.panel;
  } else {
    const items = col(d, to.side).items;
    const before = items[to.index];
    if (source) source.items.splice(source.items.indexOf(entry), 1);
    const at = before === undefined ? -1 : items.indexOf(before);
    const panel = { title: itemTitle(entry), open: true, items: [entry] };
    items.splice(at < 0 ? items.length : at, 0, panel);
    ensureWidth(d, to.side);
    ui.edit.target = panel;
  }
  return true;
}

function moveUnit(d, from, to) {
  const cache = new Map();
  const raw = d[from.side].items[from.i];
  const samePanel = to.type === "panel" && to.side === from.side && to.i === from.i;
  let entry, source = null;
  if (typeof raw.panel === "string" && from.key.startsWith("s") && !samePanel) {
    // A setting leaving a built-in panel: hide it there, so the panel stays built-in.
    const name = from.key.slice(1);
    const w = raw.widgets?.[name];
    entry = w ? { setting: name, widget: w } : { setting: name };
    (raw.hide ??= []).push(name);
  } else {
    const r = resolvePanel(d, from.side, from.i, cache);
    entry = entryAt(r, from.key);
    source = r.panel;
  }
  if (!entry || !placeEntry(d, entry, to, cache, source)) return;
  if (source && !source.items.length) removeItem(d, source);
}

function removeUnit(d, from) {
  const raw = d[from.side].items[from.i];
  if (typeof raw.panel === "string" && from.key.startsWith("s")) {
    (raw.hide ??= []).push(from.key.slice(1));
    return;
  }
  const r = resolvePanel(d, from.side, from.i, new Map());
  const entry = entryAt(r, from.key);
  if (entry) r.panel.items.splice(r.panel.items.indexOf(entry), 1);
}

function moveItem(d, from, to) {
  const src = col(d, from.side).items;
  const item = src[from.i];
  const dst = col(d, to.side).items;
  const before = dst[to.index];
  if (before === item) return;
  src.splice(from.i, 1);
  const at = before === undefined ? -1 : dst.indexOf(before);
  dst.splice(at < 0 ? dst.length : at, 0, item);
  ensureWidth(d, to.side);
}

function moveTool(d, from, to) {
  const list = toolbarList(d);
  let name = from.name;
  if (from.index >= 0) {
    if (to.index === from.index || to.index === from.index + 1) return;
    list.splice(from.index, 1);
    const at = to.index > from.index ? to.index - 1 : to.index;
    list.splice(Math.min(at, list.length), 0, name);
  } else {
    if (name !== "|" && list.includes(name)) return;
    list.splice(Math.min(to.index, list.length), 0, name);
  }
}

// Where "Add" puts a setting when it's clicked rather than dragged.
function defaultEntryTarget(d) {
  for (const side of ["right", "left"]) {
    const items = d[side]?.items ?? [];
    const i = items.indexOf(ui.edit.target);
    if (i >= 0) return { type: "panel", side, i, beforeKey: null };
  }
  return { type: "column", side: busiestSide(d), index: Infinity };
}

function busiestSide(d) {
  const count = (s) => (d[s]?.items ?? []).filter((x) => typeof x === "object").length;
  return count("left") > count("right") ? "left" : "right";
}

// What isn't on screen, for the Add list.
function shownInDraft(d) {
  const settings = new Set(), widgets = new Set(), blocks = new Set(), panels = new Set();
  for (const side of ["left", "right"]) {
    for (const item of d[side]?.items ?? []) {
      if (typeof item === "string") { blocks.add(item); continue; }
      let entries = [];
      if (typeof item.panel === "string") {
        const p = schemaPanel(item.panel);
        if (!p) continue;
        panels.add(p.id);
        entries = panelEntries(p, null, item.widgets ?? {}, item.hide ?? []);
      } else {
        for (const e of item.items ?? []) {
          if (typeof e.setting === "string") settings.add(e.setting);
          else if (typeof e.include === "string") {
            const [pid, section] = e.include.split("/");
            const p = schemaPanel(pid);
            if (p) entries.push(...panelEntries(p, section ?? null));
          } else if (typeof e.widget === "string") widgets.add(e.widget);
        }
      }
      for (const e of entries) {
        if (e.type === "control") settings.add(e.def.name);
        else if (e.type === "hsl") e.section.controls.forEach((c) => { if (!e.hide.includes(c.name)) settings.add(c.name); });
        else if (e.type === "curve") widgets.add("point-curve");
        else if (e.type === "wheels") widgets.add("color-wheels");
      }
    }
  }
  if (widgets.has("color-wheels")) AmbWheels.TAGS.forEach((t) => settings.add(t));
  return { settings, widgets, blocks, panels };
}

// ---------------------------------------------------------------- pointer handling

function editTarget(el) {
  const tool = el.closest(".topbar .actions [data-tool]");
  if (tool) {
    return { kind: "tool", name: tool.dataset.tool, index: Number(tool.dataset.toolIndex), el: tool, label: TOOL_TITLES[tool.dataset.tool] };
  }
  const unit = el.closest(".panel-body [data-unit]");
  const panel = el.closest(".column [data-item]");
  if (!panel) return null;
  const [side, i] = panel.dataset.item.split(":");
  if (unit && unit.dataset.unit && panel.contains(unit)) {
    return { kind: "unit", side, i: Number(i), key: unit.dataset.unit, el: unit, label: unitLabel(side, Number(i), unit.dataset.unit) };
  }
  const item = ui.edit.draft[side]?.items?.[Number(i)];
  return { kind: "item", side, i: Number(i), el: panel, label: typeof item === "string" ? BLOCK_TITLES[item] : panelTitle(item) };
}

function panelTitle(item) {
  return item?.title ?? schemaPanel(item?.panel)?.label ?? "Panel";
}

// The layout item a unit stands for (without converting anything).
function unitItem(side, i, key) {
  const raw = ui.edit.draft[side].items[i];
  if (typeof raw.panel !== "string") return raw.items?.[Number(key.slice(1))];
  const p = schemaPanel(raw.panel);
  const e = panelEntries(p, null, raw.widgets ?? {}, raw.hide ?? []).find((x) => x.key === key);
  return e ? entryToItem(e, p.id) : null;
}

function unitLabel(side, i, key) {
  const item = unitItem(side, i, key);
  return item ? itemTitle(item) : "";
}

window.addEventListener("pointerdown", (e) => {
  if (!ui.edit || e.button !== 0) return;
  if (!e.target.closest("#edit-inspector")) closeInspector();
  if (!e.target.closest(EDIT_ZONES) || e.target.closest(".col-resizer, .drop-hint")) return;
  e.preventDefault();
  e.stopPropagation();
  const what = editTarget(e.target);
  if (!what) return;
  // So the arrow keys can carry on from here.
  if (what.el?.tabIndex >= 0) what.el.focus({ preventScroll: true });
  beginDrag(e, what);
}, true);

// While editing, the controls in the columns and the toolbar don't act.
for (const type of ["click", "dblclick", "contextmenu"]) {
  window.addEventListener(type, (e) => {
    if (ui.edit && e.target.closest(EDIT_ZONES)) {
      e.preventDefault();
      e.stopPropagation();
    }
  }, true);
}

window.addEventListener("keydown", (e) => {
  if (!ui.edit) return;
  const typing = e.target.matches("input, textarea, select");
  const mod = e.ctrlKey || e.metaKey;
  const k = e.key.toLowerCase();
  if (e.key === "Escape") {
    if (!$("edit-inspector").hidden) closeInspector();
    else if (ui.edit.palette) togglePalette(false);
    e.preventDefault();
    e.stopPropagation();
  } else if (!typing && mod && (k === "z" || k === "y")) {
    e.preventDefault();
    e.stopPropagation();
    editUndo(k === "y" || e.shiftKey);
  } else if (!typing && keyboardEdit(e)) {
    e.preventDefault();
    e.stopPropagation();
  } else if (!typing) {
    // The photo's shortcuts (crop, zoom, picker...) wait until editing is done.
    e.stopPropagation();
  }
}, true);

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

// ---------------------------------------------------------------- the Add list

function togglePalette(on = !ui.edit.palette) {
  ui.edit.palette = on;
  $("edit-palette").hidden = !on;
  if (on) renderPalette();
  updateEditBar();
}

function renderPalette() {
  const box = $("edit-palette");
  const v = $("viewer").getBoundingClientRect();
  Object.assign(box.style, { left: `${v.left + 8}px`, top: `${v.top + 56}px`, maxHeight: `${v.height - 64}px` });
  const filter = $("edit-search").value.trim().toLowerCase();
  const d = ui.edit.draft;
  const shown = shownInDraft(d);
  const groups = [];
  const item = (label, payload, hint = "") => ({ label, payload, hint });
  const matches = (...texts) => !filter || texts.some((t) => String(t).toLowerCase().includes(filter));

  groups.push(["New", [item("Empty panel", { add: "newpanel" }), item("Heading", { add: "label" }), item("Toolbar separator", { add: "tool", name: "|" })]]);
  const blocks = Object.keys(BLOCK_TITLES).filter((b) => !shown.blocks.has(b)).map((b) => item(BLOCK_TITLES[b], { add: "block", name: b }));
  const tools = Object.keys(TOOLBAR_IDS).filter((t) => !(Array.isArray(d.toolbar) ? d.toolbar : Object.keys(TOOLBAR_IDS)).includes(t) && t !== "looks")
    .map((t) => item(TOOL_TITLES[t], { add: "tool", name: t }, "toolbar"));
  const composites = Object.keys(COMPOSITE_TITLES).filter((c) => !shown.widgets.has(c)).map((c) => item(COMPOSITE_TITLES[c], { add: "composite", name: c }));
  // Whole panels and sections only when none of their settings are on screen (each shows once).
  const panels = [], sections = [], settings = [];
  for (const p of ui.panels) {
    const all = p.sections.flatMap((s) => s.controls.map((c) => c.name));
    const composite = p.id === "tone_curve" ? "point-curve" : p.id === "color_grading" ? "color-wheels" : null;
    if (!all.some((n) => shown.settings.has(n)) && !(composite && shown.widgets.has(composite)) && matches(p.label)) {
      panels.push(item(p.label, { add: "panel", id: p.id }, "built-in panel"));
    }
    for (const s of p.sections) {
      const names = s.controls.map((c) => c.name);
      if (p.sections.length > 1 && !names.some((n) => shown.settings.has(n)) && matches(p.label, s.label)) {
        sections.push(item(`${p.label} › ${s.label}`, { add: "include", include: `${p.id}/${s.label}` }, "group"));
      }
      if (s.layout === "hsl" && !names.some((n) => shown.settings.has(n)) && matches("hsl", s.label)) {
        sections.push(item("HSL tabs", { add: "include", include: `${p.id}/${s.label}` }, "group"));
      }
      for (const c of s.controls) {
        if (!shown.settings.has(c.name) && matches(c.label, c.name, s.label, p.label)) {
          settings.push(item(c.label, { add: "setting", name: c.name }, `${p.label} › ${s.label}`));
        }
      }
    }
  }
  groups.push(["Panels", panels], ["Blocks", blocks], ["Widgets", composites], ["Groups", sections], ["Settings", settings], ["Toolbar buttons", tools]);

  const list = $("edit-palette-list");
  list.replaceChildren();
  for (const [title, items] of groups) {
    const shownItems = title === "New" ? items : items.filter((x) => matches(x.label, x.hint));
    if (!shownItems.length) continue;
    const h = document.createElement("h3");
    h.textContent = title;
    list.append(h);
    for (const x of shownItems) {
      const row = document.createElement("div");
      row.className = "palette-item";
      row.title = "Drag into place, or click to add";
      const name = document.createElement("span");
      name.textContent = x.label;
      row.append(name);
      if (x.hint && title === "Settings") {
        const hint = document.createElement("span");
        hint.className = "hint";
        hint.textContent = x.hint;
        row.append(hint);
      }
      row.addEventListener("pointerdown", (e) => {
        if (e.button !== 0) return;
        e.preventDefault();
        beginDrag(e, { kind: "palette", payload: x.payload, label: x.label });
      });
      list.append(row);
    }
  }
}

function paletteEntry(payload) {
  if (payload.add === "setting") return { setting: payload.name };
  if (payload.add === "composite") return { widget: payload.name };
  if (payload.add === "include") return { include: payload.include };
  if (payload.add === "label") return { label: "Heading" };
  return null;
}

function insertFromPalette(d, payload, drop) {
  const entry = paletteEntry(payload);
  if (entry) {
    if (drop.type === "panel" || drop.type === "column") placeEntry(d, entry, drop, new Map());
    return;
  }
  if (payload.add === "tool") {
    if (drop.type === "toolbar") moveTool(d, { name: payload.name, index: -1 }, drop);
    return;
  }
  if (drop.type !== "column") return;
  const items = col(d, drop.side).items;
  const thing = payload.add === "block" ? payload.name
    : payload.add === "panel" ? { panel: payload.id, open: true }
    : { title: "New panel", open: true, items: [] };
  items.splice(Math.min(drop.index, items.length), 0, thing);
  ensureWidth(d, drop.side);
  if (typeof thing === "object") ui.edit.target = thing;
}

function addFromPalette(payload) {
  edit((d) => {
    if (payload.add === "tool") return moveTool(d, { name: payload.name, index: -1 }, { index: toolbarList(d).length });
    const entry = paletteEntry(payload);
    if (entry) return placeEntry(d, entry, defaultEntryTarget(d), new Map());
    insertFromPalette(d, payload, { type: "column", side: busiestSide(d), index: Infinity });
  });
}

// ---------------------------------------------------------------- the inspector

function closeInspector() {
  $("edit-inspector").hidden = true;
}

function openInspector(what, x, y) {
  const box = $("edit-inspector");
  const d = ui.edit.draft;
  const parts = [];
  const heading = (text, sub = "") => {
    const h = document.createElement("h3");
    h.textContent = text;
    parts.push(h);
    if (sub) parts.push(Object.assign(document.createElement("p"), { className: "hint", textContent: sub }));
  };
  const field = (label, input) => {
    const l = document.createElement("label");
    l.append(document.createTextNode(label), input);
    parts.push(l);
    return input;
  };
  const text = (value, onChange) => {
    const t = Object.assign(document.createElement("input"), { type: "text", value: value ?? "", spellcheck: false });
    t.addEventListener("change", () => onChange(t.value.trim()));
    t.addEventListener("keydown", (e) => { if (e.key === "Enter") t.blur(); });
    return t;
  };
  const select = (options, value, onChange) => {
    const s = document.createElement("select");
    for (const [v, label] of options) s.append(Object.assign(document.createElement("option"), { value: v, textContent: label, selected: v === value }));
    s.addEventListener("change", () => onChange(s.value));
    return s;
  };
  const buttons = [];
  const button = (label, onClick, disabled = false) => {
    const b = Object.assign(document.createElement("button"), { type: "button", textContent: label, disabled });
    b.addEventListener("click", onClick);
    buttons.push(b);
  };

  if (what.kind === "tool") {
    heading(TOOL_TITLES[what.name] ?? what.name, "Toolbar");
    button("Remove from toolbar", () => removeThing(what), what.name === "looks" || what.index < 0);
  } else if (what.kind === "item") {
    const item = d[what.side].items[what.i];
    if (typeof item === "string") {
      heading(BLOCK_TITLES[item] ?? item, "Block");
      button("Remove", () => removeThing(what));
    } else {
      ui.edit.target = item;
      const builtin = typeof item.panel === "string";
      heading(panelTitle(item), builtin ? "Built-in panel: it gains new settings as Ambrotype adds them. Reordering its contents makes it your own panel." : "Your panel");
      field("Title", text(panelTitle(item), (v) => edit((dd) => {
        const it = dd[what.side].items[what.i];
        // Built-in panels fall back to their own name; yours always need a title.
        if (!builtin) it.title = v || it.title;
        else if (v && v !== schemaPanel(it.panel)?.label) it.title = v;
        else delete it.title;
      })));
      if (!builtin) {
        field("Columns", select([["1", "1 (rows)"], ["2", "2"], ["3", "3"], ["4", "4"]], String(item.columns ?? 1), (v) => edit((dd) => {
          const it = dd[what.side].items[what.i];
          if (v === "1") delete it.columns; else it.columns = Number(v);
        })));
      }
      const open = Object.assign(document.createElement("input"), { type: "checkbox", checked: Boolean(item.open) });
      open.addEventListener("change", () => edit((dd) => {
        const it = dd[what.side].items[what.i];
        if (open.checked) it.open = true; else delete it.open;
      }));
      field("Open when the app starts", open).parentElement.classList.add("check");
      const next = d[what.side].items[what.i + 1];
      button("Merge with the next panel", () => edit((dd) => {
        const cache = new Map();
        const a = resolvePanel(dd, what.side, what.i, cache).panel;
        const b = resolvePanel(dd, what.side, what.i + 1, cache).panel;
        a.items.push({ label: b.title ?? "Panel" }, ...b.items);
        removeItem(dd, b);
      }), !(next && typeof next === "object"));
      button("Remove panel", () => removeThing(what));
    }
  } else if (what.kind === "unit") {
    const item = unitItem(what.side, what.i, what.key);
    if (!item) return;
    const raw = d[what.side].items[what.i];
    const builtin = typeof raw.panel === "string";
    const split = () => edit((dd) => {
      const r = resolvePanel(dd, what.side, what.i, new Map());
      const at = r.keys.indexOf(what.key);
      if (at <= 0) return;
      const rest = r.panel.items.splice(at);
      const panel = { title: `${r.panel.title} (2)`, ...(r.panel.open ? { open: true } : {}), ...(r.panel.columns ? { columns: r.panel.columns } : {}), items: rest };
      const items = dd[what.side].items;
      items.splice(items.indexOf(r.panel) + 1, 0, panel);
    });
    if (item.setting) {
      const def = ui.defs.get(item.setting);
      heading(itemTitle(item), def ? `${item.setting} · ${def.kind === "number" ? "number" : def.kind === "bool" ? "on/off" : "choice"}` : item.setting);
      if (def) {
        const allowed = AmbControls.WIDGETS[def.kind] ?? AmbControls.WIDGETS.number;
        const names = { slider: "Slider", knob: "Knob", bar: "Bar", number: "Number box", checkbox: "Checkbox", toggle: "Switch", select: "Drop-down", segmented: "Buttons" };
        field("Control", select(allowed.map((w) => [w, names[w] ?? w]), item.widget ?? allowed[0], (w) => edit((dd) => {
          const it = dd[what.side].items[what.i];
          const isDefault = w === allowed[0];
          if (typeof it.panel === "string") {
            it.widgets ??= {};
            if (isDefault) delete it.widgets[item.setting]; else it.widgets[item.setting] = w;
            if (!Object.keys(it.widgets).length) delete it.widgets;
          } else {
            const e = it.items[Number(what.key.slice(1))];
            if (isDefault) delete e.widget; else e.widget = w;
          }
        })));
        field("Label", text(item.label ?? def.label, (v) => edit((dd) => {
          const r = resolvePanel(dd, what.side, what.i, new Map());
          const e = entryAt(r, what.key);
          if (!e) return;
          if (v && v !== def.label) e.label = v; else delete e.label;
        })));
        if (builtin) parts.push(Object.assign(document.createElement("p"), { className: "hint", textContent: "Changing the label makes this panel your own." }));
      }
    } else if (item.include) {
      heading(itemTitle(item), `Group: ${item.include}`);
      button("Unpack into separate settings", () => edit((dd) => {
        const r = resolvePanel(dd, what.side, what.i, new Map());
        const e = entryAt(r, what.key);
        const [pid, section] = e.include.split("/");
        const p = schemaPanel(pid);
        const items = panelEntries(p, section ?? null).map((x) => entryToItem(x, p.id));
        r.panel.items.splice(r.panel.items.indexOf(e), 1, ...items);
      }), builtin || item.include.endsWith("/HSL"));
    } else if (item.widget) {
      heading(itemTitle(item), "Widget");
    } else {
      heading("Heading");
      field("Text", text(item.label, (v) => edit((dd) => {
        const r = resolvePanel(dd, what.side, what.i, new Map());
        const e = entryAt(r, what.key);
        if (e && v) e.label = v;
      })));
    }
    button("Split the panel here", split);
    button("Remove", () => removeThing(what));
  }

  box.replaceChildren(...parts, Object.assign(document.createElement("div"), { className: "inspector-buttons" }));
  box.lastChild.append(...buttons);
  box.hidden = false;
  const r = box.getBoundingClientRect();
  box.style.left = `${Math.max(8, Math.min(x + 8, window.innerWidth - r.width - 8))}px`;
  box.style.top = `${Math.max(8, Math.min(y + 8, window.innerHeight - r.height - 8))}px`;
}

// ---------------------------------------------------------------- the edit bar

function wireEditor() {
  $("btn-edit-layout").addEventListener("click", () => (ui.edit ? null : startEditing()));
  $("edit-save").addEventListener("click", () => finishEditing(true));
  $("edit-cancel").addEventListener("click", () => finishEditing(false));
  $("edit-undo").addEventListener("click", () => editUndo(false));
  $("edit-redo").addEventListener("click", () => editUndo(true));
  $("edit-add").addEventListener("click", () => togglePalette());
  $("edit-palette-close").addEventListener("click", () => togglePalette(false));
  $("edit-search").addEventListener("input", renderPalette);
  $("edit-density").addEventListener("change", (e) => edit((d) => {
    if (e.target.value === "normal") delete d.density; else d.density = e.target.value;
  }));
  $("edit-theme").addEventListener("change", (e) => {
    ui.edit.theme = e.target.value;
    applyTheme(findLook("theme", ui.edit.theme)?.data ?? null);
  });
  window.addEventListener("resize", () => { if (ui.edit?.palette) renderPalette(); });
}
