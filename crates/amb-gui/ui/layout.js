// Looks: the active layout (toolbar, side columns, panels, the widget each
// setting uses) and theme (colour tokens), and the Looks dialog.
// The file formats are in docs/ai/ui-layouts.md; src/looks.rs validates and
// stores them. Panels still take their settings, ranges and labels from the
// editor schema, so a layout only decides where and how they are shown.
"use strict";

const TOOLBAR_IDS = {
  open: "btn-open", save: "btn-save", export: "btn-export", undo: "btn-undo", redo: "btn-redo",
  before: "btn-before", crop: "btn-crop", zoom: "btn-zoom", wb: "btn-wb", clip: "btn-clip",
  looks: "btn-looks", edit: "btn-edit-layout", settings: "btn-settings", feedback: "btn-feedback",
};
const BLOCK_IDS = { presets: "block-presets", history: "block-history", histogram: "block-histogram" };
const WHEEL_SECTIONS = ["Shadow", "Midtone", "Highlight", "Global"];
const DEFAULT_WIDTH = { left: 250, right: 330 };

// Used only if the backend can't list looks: everything in the classic places.
function fallbackLayout() {
  return {
    name: "Classic",
    left: { width: 250, items: ["presets", "history"] },
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
}

// ---------------------------------------------------------------- layout

function applyLayout(layout) {
  ui.layout = layout;
  const density = ["compact", "large"].includes(layout.density) ? layout.density : "normal";
  document.body.classList.remove("density-compact", "density-normal", "density-large");
  document.body.classList.add(`density-${density}`);
  buildToolbar(layout.toolbar);

  // Take everything out, then place what the layout asks for.
  const store = $("block-store");
  for (const id of Object.values(BLOCK_IDS)) store.append($(id));
  store.append($("crop-panel"));
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
    items.forEach((item, i) => {
      let el = null;
      if (typeof item === "string") el = BLOCK_IDS[item] ? $(BLOCK_IDS[item]) : null;
      else if (item && typeof item === "object") el = buildPanel(item);
      if (!el) return;
      // Where it is in the layout file, for editor mode (editor.js).
      el.dataset.item = `${side}:${i}`;
      col.append(el);
    });
    const w = Number(spec.width ?? DEFAULT_WIDTH[side]);
    widths[side] = items.length && w > 0 ? w : 0;
    col.hidden = !widths[side];
  }
  $("workspace").style.gridTemplateColumns = `${widths.left}px minmax(0, 1fr) ${widths.right}px`;

  // The crop panel goes to the top of the column with the most panels.
  const host = ["left", "right"]
    .map((s) => $(`col-${s}`))
    .filter((c) => !c.hidden)
    .sort((a, b) => b.querySelectorAll(".panel").length - a.querySelectorAll(".panel").length)[0];
  if (host) {
    const hist = $("block-histogram");
    if (hist.parentElement === host) hist.after($("crop-panel"));
    else host.prepend($("crop-panel"));
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
  // Looks is always there, so a layout can't lock you out of changing it.
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
  const label = labelText ?? def.label;
  const allowed = AmbControls.WIDGETS[def.kind] ?? AmbControls.WIDGETS.number;
  const maker = AmbControls.MAKERS[widget];
  if (maker && allowed.includes(widget)) {
    const { el, entry } = maker(def, label);
    ui.controls.set(def.name, entry);
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
      l.textContent = e.text;
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

// ---------------------------------------------------------------- Looks dialog

async function looksFlow() {
  const dlg = $("looks-dialog");
  if (!ui.looks) return toast("Layouts and themes aren't available", true);
  const before = { layout: ui.looks.active_layout, theme: ui.looks.active_theme };
  const sel = { ...before };
  let view = { kind: "layout", id: sel.layout }; // what the name/clone/share/delete row acts on
  let armedDelete = null;

  const viewed = () => findLook(view.kind, view.id);
  const apply = () => {
    const l = findLook("layout", sel.layout), t = findLook("theme", sel.theme);
    applyTheme(t?.usable ? t.data : null);
    if (l?.usable) applyLayout(l.data);
  };
  const choose = (kind, look) => {
    view = { kind, id: look.id };
    if (look.usable) {
      sel[kind] = look.id;
      // A layout may name the theme it was made with.
      const t = kind === "layout" && look.data.theme && findLook("theme", look.data.theme);
      if (t?.usable) sel.theme = t.id;
      apply();
    }
    render();
  };
  const saved = (r) => {
    ui.looks = r.looks;
    const look = findLook(r.kind, r.id);
    if (look) choose(r.kind, look);
  };

  function render() {
    for (const kind of ["layout", "theme"]) {
      const list = $(kind === "layout" ? "looks-layouts" : "looks-themes");
      list.replaceChildren(...ui.looks[`${kind}s`].map((look) => {
        const b = document.createElement("button");
        b.type = "button";
        b.className = "look";
        b.setAttribute("role", "option");
        b.setAttribute("aria-selected", String(sel[kind] === look.id));
        b.classList.toggle("selected", sel[kind] === look.id);
        b.classList.toggle("viewed", view.kind === kind && view.id === look.id);
        b.classList.toggle("broken", !look.usable);
        const name = document.createElement("span");
        name.textContent = look.name;
        const marks = document.createElement("span");
        marks.className = "marks";
        marks.textContent = (look.problems.length ? "⚠ " : "") + (look.builtin ? "🔒" : "");
        b.title = look.builtin ? "Built in: clone it to make changes" : (look.usable ? "" : "Can't be used: see the problems below");
        b.append(name, marks);
        b.addEventListener("click", () => choose(kind, look));
        return b;
      }));
    }
    const look = viewed();
    $("looks-name").value = look?.name ?? "";
    $("looks-name").disabled = !look || look.builtin || !look.usable;
    $("looks-clone").disabled = !look?.usable;
    $("looks-share").disabled = !look?.usable;
    $("looks-delete").disabled = !look || look.builtin;
    $("looks-submit").disabled = !look || look.builtin || !look.usable;
    $("looks-delete").textContent = armedDelete === look?.id ? "Really delete?" : "Delete";
    const about = [];
    if (look?.data?.description) about.push(look.data.description);
    const base = look?.based_on && findLook(view.kind, look.based_on);
    if (look?.based_on) about.push(`Based on ${base?.name ?? look.based_on}.`);
    if (look && !look.builtin) about.push(`Your ${view.kind}: a file in the looks folder.`);
    $("looks-about").textContent = about.join(" ");
    $("looks-problems").replaceChildren(...(look?.problems ?? []).map((p) => {
      const li = document.createElement("li");
      li.className = p.error ? "error" : "warning";
      li.textContent = `${p.error ? "Error" : "Warning"}: ${p.message}`;
      return li;
    }));
    renderThemeEditor(view.kind === "theme" && look && !look.builtin && look.usable ? look : null);
  }

  // Colour pickers for one of the user's themes; changes show at once and save on release.
  function renderThemeEditor(look) {
    const box = $("theme-editor");
    box.hidden = !look;
    if (!look) return box.replaceChildren();
    const data = structuredClone(look.data);
    data.tokens ??= {};
    const fallback = findLook("theme", "builtin:dark")?.data.tokens ?? {};
    const save = async () => {
      try {
        saved(await invoke("save_look", { kind: "theme", id: look.id, data }));
      } catch (e) { toast(String(e), true); }
    };
    const rows = Object.keys(fallback).map((token) => {
      const label = document.createElement("label");
      label.className = "token";
      const value = data.tokens[token] ?? fallback[token];
      let input;
      if (typeof fallback[token] === "number") {
        input = Object.assign(document.createElement("input"), { type: "range", min: 0, max: 16, step: 1, value });
      } else {
        input = Object.assign(document.createElement("input"), { type: "color", value: toHex6(value) });
      }
      input.addEventListener("input", () => {
        data.tokens[token] = input.type === "range" ? Number(input.value) : input.value;
        applyTheme(data);
      });
      input.addEventListener("change", save);
      label.append(input, document.createTextNode(token));
      return label;
    });
    box.replaceChildren(...rows);
  }

  const handlers = {
    "looks-clone": async () => {
      const look = viewed();
      const data = structuredClone(look.data);
      const names = new Set(ui.looks[`${view.kind}s`].map((l) => l.name));
      let name = `${look.name} copy`;
      for (let n = 2; names.has(name); n++) name = `${look.name} copy ${n}`;
      data.name = name;
      data.based_on = look.id;
      saved(await invoke("save_look", { kind: view.kind, id: null, data }));
      $("looks-name").focus();
      $("looks-name").select();
    },
    "looks-share": async () => {
      const look = viewed();
      const path = await invoke("export_look", { kind: view.kind, id: look.id, name: look.name });
      if (path) toast(`Saved ${path}`);
    },
    "looks-delete": async () => {
      const look = viewed();
      if (armedDelete !== look.id) {
        armedDelete = look.id;
        setTimeout(() => { if (armedDelete === look.id) { armedDelete = null; render(); } }, 3000);
        return render();
      }
      armedDelete = null;
      ui.looks = await invoke("delete_look", { kind: view.kind, id: look.id });
      const fallback = view.kind === "layout" ? "builtin:classic" : "builtin:dark";
      if (sel[view.kind] === look.id) sel[view.kind] = fallback;
      if (before[view.kind] === look.id) before[view.kind] = fallback;
      view = { kind: view.kind, id: sel[view.kind] };
      apply();
      render();
    },
    "looks-import": async () => {
      const r = await invoke("import_look");
      if (r) { saved(r); toast(`Imported "${findLook(r.kind, r.id)?.name}"`); }
    },
    "looks-folder": async () => toast(`Your looks: ${await invoke("open_looks_folder")}`),
    "looks-hub": async () => {
      await hubFlow();
      // Something may have been installed (or switched to with "Use now").
      ui.looks = await invoke("looks");
      sel.layout = ui.looks.active_layout;
      sel.theme = ui.looks.active_theme;
      view = { kind: view.kind, id: sel[view.kind] };
      render();
    },
    "looks-submit": async () => {
      const look = viewed();
      await invoke("hub_submit", { kind: view.kind, id: look.id });
      toast("GitHub is open with your look filled in: press Propose changes, then Create pull request.");
    },
    "looks-edit": async () => {
      // Keep the chosen look, then edit it.
      dlg.close("ok");
      setTimeout(startEditing);
    },
    "looks-reload": async () => {
      ui.looks = await invoke("looks");
      for (const kind of ["layout", "theme"]) {
        if (!findLook(kind, sel[kind])?.usable) sel[kind] = kind === "layout" ? "builtin:classic" : "builtin:dark";
      }
      if (!viewed()) view = { kind: "layout", id: sel.layout };
      apply();
      render();
    },
  };
  const wired = Object.entries(handlers).map(([id, fn]) => {
    const h = () => fn().catch((e) => toast(String(e), true));
    $(id).addEventListener("click", h);
    return [id, h];
  });
  const rename = async () => {
    const look = viewed();
    const name = $("looks-name").value.trim();
    if (!look || look.builtin || !name || name === look.name) return render();
    try {
      saved(await invoke("save_look", { kind: view.kind, id: look.id, data: { ...look.data, name } }));
    } catch (e) { toast(String(e), true); }
  };
  $("looks-name").addEventListener("change", rename);
  // Enter would submit the form (as Cancel); make it confirm the name instead.
  const enter = (e) => { if (e.key === "Enter") { e.preventDefault(); $("looks-name").blur(); } };
  $("looks-name").addEventListener("keydown", enter);

  render();
  dlg.returnValue = "";
  dlg.showModal();
  await new Promise((resolve) => dlg.addEventListener("close", resolve, { once: true }));
  for (const [id, h] of wired) $(id).removeEventListener("click", h);
  $("looks-name").removeEventListener("change", rename);
  $("looks-name").removeEventListener("keydown", enter);

  if (dlg.returnValue === "ok") {
    ui.looks.active_layout = sel.layout;
    ui.looks.active_theme = sel.theme;
    invoke("set_active_look", { layout: sel.layout, theme: sel.theme }).catch((e) => toast(String(e), true));
  } else {
    ui.looks.active_layout = before.layout;
    ui.looks.active_theme = before.theme;
    applyActiveLooks();
  }
}

// "#abc" / "#aabbccdd" → "#aabbcc" for <input type=color>.
function toHex6(c) {
  if (typeof c !== "string" || !c.startsWith("#")) return "#000000";
  const h = c.slice(1);
  if (h.length === 3 || h.length === 4) return `#${[...h.slice(0, 3)].map((x) => x + x).join("")}`;
  return `#${h.slice(0, 6)}`;
}
