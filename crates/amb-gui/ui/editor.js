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
const BLOCK_TITLES = { presets: "Presets", info: "Rating & keywords", history: "History", histogram: "Histogram", tools: "Task tools", quick: "Quick fixes", retouch: "Retouch tools" };
const TOOL_TITLES = {
  open: "Open", library: "Library", save: "Save", export: "Export", collage: "Collage", undo: "Undo", redo: "Redo", before: "Before / After",
  crop: "Crop", masks: "Masks", spots: "Spots", zoom: "100%", wb: "WB picker", clip: "Clipping", frame: "Frame", auto: "Auto edit", select: "Select object", search: "Search", keys: "Shortcuts", hub: "Browse", plugins: "Plugins", looks: "Appearance", edit: "Edit layout", settings: "Settings",
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
  if (typeof scheduleFitToolbar === "function") scheduleFitToolbar();
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
  if (typeof scheduleFitToolbar === "function") scheduleFitToolbar();
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

