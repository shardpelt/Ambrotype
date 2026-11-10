// SPDX-License-Identifier: GPL-3.0-or-later
// Appearance ("looks"): the active layout (toolbar, side columns, panels, the widget each
// setting uses) and theme (colour tokens), and the Appearance dialog.
// The file formats are in docs/ai/ui-layouts.md; src/looks/ validates and
// stores them. Panels still take their settings, ranges and labels from the
// editor schema, so a layout only decides where and how they are shown.
"use strict";

const TOOLBAR_IDS = {
  open: "btn-open", library: "btn-library", save: "btn-save", export: "btn-export", collage: "btn-collage", undo: "btn-undo", redo: "btn-redo", auto: "btn-auto", select: "btn-select", search: "btn-search",
  before: "btn-before", crop: "btn-crop", masks: "btn-masks", spots: "btn-spots", zoom: "btn-zoom", wb: "btn-wb", clip: "btn-clip", frame: "btn-frame", keys: "btn-keys",
  hub: "btn-hub", plugins: "btn-plugins", looks: "btn-looks", edit: "btn-edit-layout", settings: "btn-settings", feedback: "btn-feedback",
};
const BLOCK_IDS = { presets: "block-presets", info: "block-info", history: "block-history", histogram: "block-histogram", tools: "block-tools", quick: "block-quick", retouch: "block-retouch" };
const WHEEL_SECTIONS = ["Shadow", "Midtone", "Highlight", "Global"];
const DEFAULT_WIDTH = { left: 250, right: 330 };

// Used only if the backend can't list looks: everything in the classic places.
function fallbackLayout() {
  return {
    name: "Classic",
    left: { width: 250, items: ["presets", "info", "history"] },
    right: { width: 330, items: ["histogram", ...ui.panels.map((p) => ({ panel: p.id, open: p.id === "basic" }))] },
  };
}

// ---------------------------------------------------------------- theme

function applyTheme(theme) {
  const root = document.documentElement;
  for (const name of [...root.style]) if (name.startsWith("--")) root.style.removeProperty(name);
  for (const [k, v] of Object.entries(theme?.tokens ?? {})) {
    root.style.setProperty(`--${k}`, typeof v === "number" ? `${v}px` : v);
  }
  root.style.colorScheme = theme?.scheme === "light" ? "light" : "dark";
  // Canvases read the tokens when they draw.
  requestAnimationFrame(redrawCanvases);
}

function redrawCanvases() {
  if (ui.histogram) drawHistogram(ui.histogram);
  ui.curve?.refresh();
  ui.wheels?.refresh();
  ui.crop.tool?.draw?.();
  if (typeof drawMasks === "function") drawMasks();
  if (typeof drawSpots === "function") drawSpots();
}

// ---------------------------------------------------------------- layout

function applyLayout(layout) {
  ui.layout = layout;
  const density = ["compact", "large"].includes(layout.density) ? layout.density : "normal";
  document.body.classList.remove("density-compact", "density-normal", "density-large");
  document.body.classList.add(`density-${density}`);
  applyToolbarIcons(layout.toolbar_style === "words" ? "words" : "icons");
  buildToolbar(layout.toolbar);
  if (typeof applyStripVisibility === "function") applyStripVisibility();
  applyChrome();

  // Take everything out, then place what the layout asks for.
  const store = $("block-store");
  for (const id of Object.values(BLOCK_IDS)) store.append($(id));
  store.append($("crop-panel"));
  store.append($("mask-panel"));
  store.append($("spot-panel"));
  store.append($("guided-panel"));
  store.append($("select-panel"));
  ui.curve?.destroy();
  ui.wheels?.destroy();
  ui.curve = ui.wheels = null;
  ui.controls.clear();
  const widths = {};
  for (const side of ["left", "right"]) {
    const col = $(`col-${side}`);
    col.replaceChildren();
    const spec = layout[side] ?? {};
    const items = Array.isArray(spec.items) ? spec.items : [];
    // `{ "tab": "Name" }` starts a tab: what follows shows while it's chosen
    // (one bar of tabs, where the first one is).
    let tab = null;
    let bar = null;
    items.forEach((item, i) => {
      if (item && typeof item === "object" && typeof item.tab === "string") {
        tab = item.tab;
        if (!bar) {
          bar = Object.assign(document.createElement("div"), { className: "column-tabs", role: "tablist" });
          col.append(bar);
        }
        const b = Object.assign(document.createElement("button"), { type: "button", className: "column-tab", textContent: tab, role: "tab" });
        b.dataset.tab = tab;
        b.addEventListener("click", () => showColumnTab(side, b.dataset.tab));
        bar.append(b);
        return;
      }
      let el = null;
      if (typeof item === "string") el = BLOCK_IDS[item] ? $(BLOCK_IDS[item]) : null;
      else if (item && typeof item === "object") el = buildPanel(item);
      if (!el) return;
      // Where it is in the layout file, for editor mode (editor.js).
      el.dataset.item = `${side}:${i}`;
      if (tab) el.dataset.tab = tab;
      else delete el.dataset.tab;
      col.append(el);
    });
    if (bar) showColumnTab(side, columnTab(side) ?? bar.firstChild.dataset.tab);
    const w = Number(spec.width ?? DEFAULT_WIDTH[side]);
    widths[side] = items.length && w > 0 ? w : 0;
  }
  // The layout's widths, then any dragged on this computer.
  ui.layoutWidths = { ...widths };
  const mine = colWidths[ui.looks?.active_layout] ?? {};
  for (const side of ["left", "right"]) if (widths[side] && mine[side]) widths[side] = mine[side];
  ui.sideWidths = widths;
  applySideCollapse();
  for (const side of ["left", "right"]) wireColumnBlocks($(`col-${side}`));

  // The crop panel goes to the top of the column with the most panels.
  const host = ["left", "right"]
    .map((s) => $(`col-${s}`))
    .filter((c) => !c.hidden)
    .sort((a, b) => b.querySelectorAll(".panel").length - a.querySelectorAll(".panel").length)[0];
  if (host) {
    const hist = $("block-histogram");
    if (hist.parentElement === host) hist.after($("crop-panel"));
    else host.prepend($("crop-panel"));
    $("crop-panel").after($("mask-panel"));
    $("mask-panel").after($("spot-panel"));
    $("spot-panel").after($("guided-panel"));
    $("guided-panel").after($("select-panel"));
  }
  if (ui.edit) decorateEditing();
  syncControls();
  requestAnimationFrame(redrawCanvases);
  // The viewer changed size.
  if (ui.info) requestPreview(false);
}

function buildToolbar(order) {
  const bar = document.querySelector(".topbar .actions");
  // Each node remembers its place in the layout's toolbar list (for editor mode).
  const tagged = (el, name, i) => {
    el.dataset.tool = name;
    el.dataset.toolIndex = i;
    return el;
  };
  const sep = () => Object.assign(document.createElement("span"), { className: "sep" });
  const nodes = [];
  const placed = new Set();
  (Array.isArray(order) ? order : Object.keys(TOOLBAR_IDS)).forEach((name, i) => {
    if (name === "|") {
      if (nodes.length && !nodes.at(-1).classList.contains("sep")) nodes.push(tagged(sep(), name, i));
    } else if (TOOLBAR_IDS[name] && !placed.has(name)) {
      placed.add(name);
      nodes.push(tagged($(TOOLBAR_IDS[name]), name, i));
    }
  });
  // Appearance is always there, so a layout can't lock you out of changing it.
  if (!placed.has("looks")) {
    if (nodes.length && !nodes.at(-1).classList.contains("sep")) nodes.push(tagged(sep(), "|", -1));
    placed.add("looks");
    nodes.push(tagged($("btn-looks"), "looks", -1));
  }
  while (nodes.at(-1)?.classList.contains("sep")) nodes.pop();
  // Buttons left out stay in the page (hidden), so their shortcuts still work.
  const hidden = Object.entries(TOOLBAR_IDS).filter(([n]) => !placed.has(n)).map(([n, id]) => tagged($(id), n, -1));
  // A button this list doesn't know stays in the page too (hidden), never dropped.
  const known = new Set(Object.values(TOOLBAR_IDS));
  hidden.push(...[...bar.querySelectorAll(":scope > button")].filter((b) => !known.has(b.id)));
  for (const b of nodes) b.hidden = false;
  for (const b of hidden) b.hidden = true;
  bar.replaceChildren(...nodes, ...hidden);
}

function schemaPanel(id) {
  return ui.panels.find((p) => p.id === id);
}

// A schema panel (or one of its sections) as entries to render. `key` names
// each entry within a built-in panel, for editor mode: s<setting>, l<label>,
// h<HSL section>, c<composite widget>.
function panelEntries(p, section, widgets = {}, hide = []) {
  const whole = section == null;
  const out = [];
  if (whole && p.id === "tone_curve") out.push({ type: "curve", key: "cpoint-curve" });
  if (whole && p.id === "color_grading") out.push({ type: "wheels", key: "ccolor-wheels" });
  for (const s of p.sections) {
    if (whole ? p.id === "color_grading" && WHEEL_SECTIONS.includes(s.label) : s.label !== section) continue;
    if (p.sections.length > 1 || s.layout === "sliders") out.push({ type: "label", text: s.label, key: `l${s.label}` });
    if (s.layout === "hsl") out.push({ type: "hsl", section: s, hide, key: `h${s.label}` });
    else {
      for (const def of s.controls) {
        if (!hide.includes(def.name)) out.push({ type: "control", def, widget: widgets[def.name], key: `s${def.name}` });
      }
    }
  }
  return out;
}

// A setting with the widget the layout asks for (the slider row by default).
function makeControl(def, widget, labelText) {
  if (def.name === "amb:Effects") return makeEffectsControl(def);
  // On a JPEG and the like, Temp and Tint drive Lightroom's relative settings.
  if (ui.info?.rendered && def.rendered) def = { ...def.rendered, label: def.label };
  const label = labelText ?? def.label;
  const allowed = AmbControls.WIDGETS[def.kind] ?? AmbControls.WIDGETS.number;
  const maker = AmbControls.MAKERS[widget];
  if (maker && allowed.includes(widget)) {
    const { el, entry } = maker(def, label);
    ui.controls.set(def.name, entry);
    addHelpMark(entry, def);
    return el;
  }
  return makeRow(label === def.label ? def : { ...def, label });
}

function buildHsl(section, hide) {
  const tabs = document.createElement("div");
  tabs.className = "tabs";
  const holder = document.createElement("div");
  const rows = {};
  for (const def of section.controls) {
    if (hide.includes(def.name)) continue;
    const kind = def.name.match(/^(Hue|Saturation|Luminance)/)[1];
    const band = def.name.replace(/^(Hue|Saturation|Luminance)Adjustment/, "");
    (rows[kind] ||= []).push(makeRow({ ...def, label: band }));
  }
  const kinds = ["Hue", "Saturation", "Luminance"].filter((k) => rows[k]);
  if (!kinds.includes(ui.hslTab)) ui.hslTab = kinds[0];
  for (const kind of kinds) {
    const b = document.createElement("button");
    b.textContent = kind;
    b.addEventListener("click", () => {
      ui.hslTab = kind;
      for (const [k, rs] of Object.entries(rows)) rs.forEach((r) => (r.hidden = k !== kind));
      tabs.querySelectorAll("button").forEach((x) => x.classList.toggle("active", x === b));
    });
    tabs.append(b);
    rows[kind].forEach((r) => { r.hidden = kind !== ui.hslTab; holder.append(r); });
    if (kind === ui.hslTab) b.classList.add("active");
  }
  return [tabs, holder];
}

function buildPanel(item) {
  let title, entries, groups = null;
  if (typeof item.panel === "string") {
    const p = schemaPanel(item.panel);
    if (!p) return null;
    title = item.title ?? p.label;
    entries = panelEntries(p, null, item.widgets ?? {}, item.hide ?? []);
    if (!item.hide?.length) groups = p.groups;
  } else {
    title = item.title ?? "Panel";
    entries = [];
    // In your own panels an entry's key is its index in `items` (j<n>).
    (Array.isArray(item.items) ? item.items : []).forEach((e, j) => {
      const key = `j${j}`;
      if (typeof e.setting === "string") {
        entries.push({ type: "control", name: e.setting, widget: e.widget, label: e.label, key });
      } else if (typeof e.include === "string") {
        const [pid, section] = e.include.split("/");
        const p = schemaPanel(pid);
        if (p) entries.push(...panelEntries(p, section).map((x) => ({ ...x, key })));
      } else if (e.widget === "point-curve") entries.push({ type: "curve", key });
      else if (e.widget === "color-wheels") entries.push({ type: "wheels", key });
      else if (typeof e.label === "string") entries.push({ type: "label", text: e.label, key });
    });
  }

  const details = document.createElement("details");
  details.className = "panel";
  // Everything is open while editing, so all of it can be moved.
  details.open = Boolean(item.open) || Boolean(ui.edit);
  const summary = document.createElement("summary");
  const name = document.createElement("span");
  name.textContent = title;
  const reset = document.createElement("button");
  reset.className = "small reset";
  reset.textContent = "Reset";
  reset.title = `Reset ${title}`;
  summary.append(name, reset);
  const body = document.createElement("div");
  body.className = "panel-body";
  const columns = Math.min(4, Math.max(1, Number(item.columns) || 1));
  if (columns > 1) {
    body.classList.add("grid");
    body.style.setProperty("--cols", columns);
  }

  const names = []; // what Reset clears in a custom panel
  const renderEntry = (e) => {
    if (e.type === "label") {
      const l = document.createElement("div");
      l.className = "section-label";
      l.append(Object.assign(document.createElement("span"), { className: "section-name", textContent: e.text }));
      if (e.text === "Tone") {
        // Lightroom's Auto, beside the tone sliders.
        const auto = document.createElement("button");
        auto.className = "small section-action";
        auto.textContent = "Auto";
        auto.title = "Set Exposure, Contrast, Highlights, Shadows, Whites and Blacks for this photo (one undo step)";
        auto.addEventListener("click", () => autoFlow(true, false));
        l.append(auto);
      }
      if (e.text === "Profile" && item.panel === "lens") {
        const fetch = document.createElement("button");
        fetch.className = "small section-action";
        fetch.textContent = "Lens database…";
        fetch.title = "Download or update lensfun's lens profiles (about 0.5 MB), used when there is no profile of your own";
        fetch.addEventListener("click", () => lensDbFlow());
        l.append(fetch);
      }
      if (e.text === "Upright") {
        const guided = document.createElement("button");
        guided.className = "small section-action";
        guided.textContent = "Guided…";
        guided.title = "Draw lines that should be vertical or horizontal; the Transform sliders straighten them";
        guided.addEventListener("click", () => enterGuided());
        l.append(guided);
      }
      if (e.text === "Conversion") {
        // Keep this frame's film base and levels, so later edits don't move them.
        const freeze = document.createElement("button");
        freeze.className = "small section-action";
        freeze.textContent = "Freeze";
        freeze.title = "Store the film base and levels this photo shows now and turn Auto off (one undo step). For a whole roll: Sync negative roll in the library";
        freeze.addEventListener("click", () => freezeNegative());
        const frame = document.createElement("button");
        frame.className = "small section-action";
        frame.textContent = "Crop to frame";
        frame.title = "Crop away the film rebate and the scanner holder's edges (one undo step)";
        frame.addEventListener("click", () => cropToFrame());
        const base = document.createElement("button");
        base.className = "small section-action pick-base";
        base.textContent = "Pick base";
        base.title = "Click the clear film between frames to set the film base (before Crop to frame, or with the crop off). Alt-click to measure it from the scan again";
        base.addEventListener("click", (ev) => pickBaseFlow(ev.altKey));
        l.append(base, frame, freeze);
      }
      body.append(l);
    } else if (e.type === "control") {
      const def = e.def ?? ui.defs.get(e.name);
      if (!def || ui.controls.has(def.name)) {
        const missing = document.createElement("div");
        missing.className = "row placeholder";
        missing.textContent = def ? `${def.label} is shown elsewhere` : `Unknown setting: ${e.name}`;
        body.append(missing);
        return;
      }
      names.push(def.name);
      body.append(makeControl(def, e.widget, e.label));
    } else if (e.type === "hsl") {
      names.push(...e.section.controls.map((c) => c.name).filter((n) => !e.hide.includes(n)));
      body.append(...buildHsl(e.section, e.hide));
    } else if (e.type === "curve" && !ui.curve) {
      const host = document.createElement("div");
      host.className = "composite";
      body.append(host);
      names.push(...AmbCurve.TAGS);
      ui.curve = AmbCurve.create(host, {
        onInput(tag, points) { ui.overrides[tag] = points; requestPreview(true); },
        onCommit(tag, points) {
          delete ui.overrides[tag];
          const channel = tag.replace("ToneCurvePV2012", "").toLowerCase();
          run([{ cmd: "set_param", name: tag, value: points }], channel ? `tone curve (${channel})` : "tone curve");
        },
      });
    } else if (e.type === "wheels" && !ui.wheels) {
      const host = document.createElement("div");
      host.className = "composite";
      body.append(host);
      names.push(...AmbWheels.TAGS);
      ui.wheels = AmbWheels.create(host, {
        onInput(changes) { Object.assign(ui.overrides, changes); requestPreview(true); },
        onCommit(changes) {
          for (const n of Object.keys(changes)) delete ui.overrides[n];
          run(Object.entries(changes).map(([n, value]) => ({ cmd: "set_param", name: n, value })), "color grading");
        },
      });
    }
  };
  for (const e of entries) {
    const first = body.children.length;
    renderEntry(e);
    for (let k = first; k < body.children.length; k++) body.children[k].dataset.unit = e.key ?? "";
  }
  reset.addEventListener("click", (ev) => {
    ev.preventDefault();
    const cmds = groups
      ? groups.map((g) => ({ cmd: "reset_group", group: g }))
      : names.map((n) => ({ cmd: "reset_param", name: n }));
    run(cmds, `reset ${title}`);
  });
  // Canvases inside a closed panel have no size; redraw when it opens.
  details.addEventListener("toggle", () => {
    if (details.open) { ui.curve?.refresh(); ui.wheels?.refresh(); }
  });
  details.append(summary, body);
  return details;
}

// ---------------------------------------------------------------- loading

function findLook(kind, id) {
  return ui.looks?.[kind === "layout" ? "layouts" : "themes"].find((l) => l.id === id);
}

function applyActiveLooks() {
  const theme = findLook("theme", ui.looks?.active_theme);
  const layout = findLook("layout", ui.looks?.active_layout);
  applyTheme(theme?.usable ? theme.data : null);
  applyLayout(layout?.usable ? layout.data : fallbackLayout());
  if ($("task-bar").children.length) renderTasks();
}

async function loadLooks() {
  try {
    ui.looks = await invoke("looks");
  } catch (e) {
    ui.looks = null;
    toast(`Couldn't load layouts and themes: ${e}`, true);
  }
  applyActiveLooks();
}

