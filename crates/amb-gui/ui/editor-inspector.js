// SPDX-License-Identifier: GPL-3.0-or-later
// Editor mode's Add list, the inspector for the selected item, and the edit
// bar. Split out of editor.js.
"use strict";

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
