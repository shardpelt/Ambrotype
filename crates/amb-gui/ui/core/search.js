// SPDX-License-Identifier: GPL-3.0-or-later
// Search everything (Ctrl/⌘+K): one box that finds any command, setting,
// panel, preset, layout or theme by name and takes you to it, so nothing is
// lost behind a layout that doesn't show it. A setting's name and a number
// ("exposure 0.7", "temp 5200") sets it. What people want ("whatsapp",
// "raw by mistake") finds the feature in docs/FEATURES.md ("How to").
"use strict";

const searchState = { items: [], shown: [], at: 0 };

// What can be found, gathered fresh each time the box opens.
function searchItems() {
  const items = [];
  for (const a of KEY_ACTIONS) {
    if (a.id === "search") continue;
    const combo = keysFor(a)[0];
    items.push({
      kind: "Command", title: MENU_TITLES[a.id]?.replace(/…$/, "") ?? a.label, also: `${a.label} ${a.group}`,
      keys: combo ? keyLabel(combo) : "", enabled: menuItemOn(a.id), run: () => runMenuItem(a.id),
    });
  }
  // Settings by name, with the panel they're in; one a layout doesn't show
  // is listed only when no setting on screen has its name.
  const settings = [...ui.controls].filter(([, c]) => c.def?.label);
  const onScreen = new Set(settings.filter(([, c]) => c.label?.isConnected).map(([, c]) => c.def.label));
  for (const [name, c] of settings) {
    const shown = Boolean(c.label?.isConnected);
    if (!shown && onScreen.has(c.def.label)) continue;
    const panel = c.label?.closest?.("details.panel")?.querySelector("summary > span")?.textContent;
    items.push({
      kind: "Setting", title: c.def.label, also: `${name} ${panel ?? ""}`, enabled: shown && Boolean(ui.info),
      note: shown ? (panel ? `Setting · ${panel}` : "") : "not in this layout", run: () => revealSetting(name),
      number: c.input && typeof c.def.min === "number" && typeof c.def.max === "number" ? name : null,
    });
  }
  for (const s of document.querySelectorAll("#workspace details.panel > summary > span:first-child")) {
    items.push({ kind: "Panel", title: s.textContent, also: "", enabled: true, run: () => revealPanel(s.closest("details")) });
  }
  for (const p of ui.presets ?? []) {
    items.push({
      kind: "Preset", title: p.name, also: p.group ?? "", enabled: Boolean(ui.info),
      run: () => applyPreset(p),
    });
  }
  for (const f of ui.features ?? []) {
    items.push({ kind: "How to", title: f.want, also: `${f.how} ${f.section}`, note: `How to · ${f.section}`, enabled: true, run: () => showHow(f) });
  }
  for (const t of TASKS) items.push({ kind: "Task", title: t.label, also: t.hint, enabled: true, run: () => switchTask(t.id) });
  for (const kind of ["layout", "theme"]) {
    for (const l of ui.looks?.[`${kind}s`] ?? []) {
      if (!l.usable || TASK_LAYOUTS.has(l.id)) continue;
      items.push({ kind: kind === "layout" ? "Layout" : "Theme", title: l.name ?? l.data?.name ?? l.id, also: l.id, enabled: true, run: () => switchLook(kind, l.id) });
    }
  }
  return items;
}

// Every word typed must be found; a title that starts with it ranks first.
function searchScore(item, words) {
  const title = item.title.toLowerCase();
  const hay = `${title} ${item.also.toLowerCase()} ${item.kind.toLowerCase()}`;
  let score = 0;
  for (const w of words) {
    const at = hay.indexOf(w);
    if (at < 0) return -1;
    score += title.startsWith(w) ? 30 : title.includes(` ${w}`) ? 20 : title.includes(w) ? 10 : 1;
  }
  // A command or setting of that name comes before a "how to".
  return score + (item.enabled ? 5 : 0) - (item.kind === "How to" ? 3 : 0) - title.length / 100;
}

// "exposure 0.7": the numeric settings those words find, set to the number
// (within the slider's range), best first.
function searchValues(words) {
  if (words.length < 2 || !ui.info) return [];
  const value = Number(words.at(-1).replace(",", "."));
  if (!/^[+-]?(\d+\.?\d*|\.\d+)$/.test(words.at(-1).replace(",", ".")) || !Number.isFinite(value)) return [];
  const name = words.slice(0, -1);
  return searchState.items
    .filter((it) => it.kind === "Setting" && it.enabled && it.number)
    .map((it) => [searchScore(it, name), it])
    .filter(([s]) => s >= 0)
    .sort((a, b) => b[0] - a[0])
    .slice(0, 3)
    .map(([, it]) => {
      const def = ui.controls.get(it.number).def;
      const v = Math.min(def.max, Math.max(def.min, value));
      return {
        kind: "Set", title: `Set ${it.title} to ${format(def, v)}`, also: "", enabled: true,
        note: v === value ? "Set" : `Set · ${def.min} to ${def.max}`,
        run: () => run([{ cmd: "set_param", name: it.number, value: v }]),
      };
    });
}

function renderSearch() {
  const words = $("search-input").value.toLowerCase().split(/\s+/).filter(Boolean);
  searchState.shown = words.length
    ? [
      ...searchValues(words),
      ...searchState.items.map((it) => [searchScore(it, words), it]).filter(([s]) => s >= 0).sort((a, b) => b[0] - a[0]).slice(0, 60).map(([, it]) => it),
    ]
    : searchState.items.filter((it) => it.kind === "Command" && it.enabled).slice(0, 12);
  searchState.at = Math.min(searchState.at, Math.max(0, searchState.shown.length - 1));
  const list = $("search-results");
  list.replaceChildren(...searchState.shown.map((it, i) => {
    const li = Object.assign(document.createElement("li"), { className: `search-row${i === searchState.at ? " on" : ""}${it.enabled ? "" : " off"}` });
    li.append(
      Object.assign(document.createElement("span"), { className: "search-title", textContent: it.title }),
      Object.assign(document.createElement("span"), { className: "search-kind", textContent: it.note || it.kind }),
    );
    if (it.keys) li.append(Object.assign(document.createElement("kbd"), { textContent: it.keys }));
    li.addEventListener("mousemove", () => {
      if (searchState.at === i) return;
      searchState.at = i;
      for (const [j, row] of [...list.children].entries()) row.classList.toggle("on", j === i);
    });
    li.addEventListener("click", () => chooseSearch(i));
    return li;
  }));
  $("search-none").hidden = searchState.shown.length > 0;
  list.children[searchState.at]?.scrollIntoView({ block: "nearest" });
}

function chooseSearch(i) {
  const it = searchState.shown[i];
  if (!it || !it.enabled) return;
  $("search-dialog").close();
  // After the box is gone, so the action's own dialogs and focus work.
  requestAnimationFrame(() => it.run());
}

// The features list, read once from the app (commands/features.rs).
let featuresLoading = null;
function loadFeatures() {
  if (ui.features?.length) return Promise.resolve(ui.features);
  featuresLoading ??= invoke("features").then((list) => {
    ui.features = list;
    if ($("search-dialog").open) {
      searchState.items = searchItems();
      renderSearch();
    }
    return list;
  }).catch(() => {
    featuresLoading = null;
    return [];
  });
  return featuresLoading;
}

// Help → What Ambrotype Can Do: the whole list by section, with a filter.
async function featuresFlow() {
  const dlg = $("features-dialog");
  if (dlg.open) return;
  await loadFeatures();
  $("features-filter").value = "";
  renderFeatures();
  dlg.showModal();
  $("features-filter").focus();
}

function renderFeatures() {
  const words = $("features-filter").value.toLowerCase().split(/\s+/).filter(Boolean);
  const hits = (ui.features ?? []).filter((f) => {
    const hay = `${f.want} ${f.how} ${f.section}`.toLowerCase();
    return words.every((w) => hay.includes(w));
  });
  const list = $("features-list");
  const out = [];
  let section = null;
  for (const f of hits) {
    if (f.section !== section) {
      section = f.section;
      out.push(Object.assign(document.createElement("h3"), { textContent: section }));
    }
    const row = Object.assign(document.createElement("button"), { type: "button", className: "feature-row" });
    row.append(
      Object.assign(document.createElement("span"), { className: "feature-want", textContent: f.want }),
      Object.assign(document.createElement("span"), { className: "feature-how", textContent: f.how }),
    );
    row.addEventListener("click", () => showHow(f));
    out.push(row);
  }
  list.replaceChildren(...out);
  $("features-none").hidden = hits.length > 0;
}

// Words too common to tie two features together.
const HOW_COMMON = new Set("with from that this your what when them they into like more than have make does each also only just then there their photo photos click right open drag show shows button settings".split(" "));

// Features close to `f`, best first: ones naming the same things in bold,
// sharing words, or in the same section; so one answer leads to the next.
function relatedFeatures(f, count = 3) {
  const words = (x) => new Set((`${x.want} ${x.how}`.toLowerCase().match(/[a-z]{4,}/g) ?? []).filter((w) => !HOW_COMMON.has(w)));
  const names = (x) => new Set(x.names.map((n) => n.toLowerCase().replace(/[…:]+$/, "")));
  const [mine, my] = [words(f), names(f)];
  const list = ui.features ?? [];
  const at = list.indexOf(f);
  const out = list
    .filter((o) => o !== f && o.want !== f.want)
    .map((o) => {
      let score = o.section === f.section ? 1.5 : 0;
      for (const n of names(o)) if (my.has(n)) score += 3;
      for (const w of words(o)) if (mine.has(w)) score += 1;
      return [score, o];
    })
    .filter(([score]) => score >= 2.5)
    .sort((a, b) => b[0] - a[0])
    .slice(0, count)
    .map(([, o]) => o);
  // Too few: its neighbours in the list, which are about the same thing.
  for (const d of [1, -1, 2, -2]) {
    const o = list[at + d];
    if (out.length >= 2 || at < 0) break;
    if (o && o.section === f.section && !out.includes(o)) out.push(o);
  }
  return out;
}

// A feature: where it is, with a button for each thing it names in bold
// that can be used from here: a command, a task, a panel, a setting, or a
// button on screen; and two or three related features to follow.
function showHow(f) {
  $("how-section").textContent = f.section;
  $("how-want").textContent = f.want;
  $("how-text").replaceChildren(...f.parts.map(([code, t]) => (code
    ? Object.assign(document.createElement("code"), { textContent: t })
    : document.createTextNode(t))));
  const buttons = [];
  for (const name of f.names) {
    const target = howTarget(name);
    if (!target || buttons.some((b) => b.dataset.key === target.key)) continue;
    const b = Object.assign(document.createElement("button"), { type: "button", textContent: name.replace(/…$/, "") });
    b.dataset.key = target.key;
    b.addEventListener("click", () => {
      $("how-dialog").close();
      requestAnimationFrame(() => target.run());
    });
    buttons.push(b);
    if (buttons.length === 4) break;
  }
  // Library work the editor's screen doesn't show: the library is a click away.
  const library = KEY_ACTIONS.find((x) => x.id === "library");
  if (!buttons.length && /library/i.test(f.how) && library && menuItemOn("library") && !$("library")?.offsetParent) {
    const b = Object.assign(document.createElement("button"), { type: "button", textContent: "Open the library" });
    b.addEventListener("click", () => {
      $("how-dialog").close();
      requestAnimationFrame(() => runMenuItem("library"));
    });
    buttons.push(b);
  }
  $("how-actions").replaceChildren(...buttons);
  const also = relatedFeatures(f);
  $("how-also").replaceChildren(
    Object.assign(document.createElement("span"), { className: "hint", textContent: "Also:" }),
    ...also.map((o) => {
      const b = Object.assign(document.createElement("button"), { type: "button", textContent: o.want });
      b.addEventListener("click", () => showHow(o));
      return b;
    }),
  );
  $("how-also").hidden = !also.length;
  $("how-all").hidden = $("features-dialog").open;
  if (!$("how-dialog").open) $("how-dialog").showModal();
}

// What a bold name in the features list stands for, if it can be used now.
function howTarget(name) {
  const norm = (s) => s.toLowerCase().replace(/[…:]+$/, "").replace(/^\+\s*/, "").trim();
  const n = norm(name);
  const prefix = (t) => t === n || t.startsWith(`${n}:`) || t.startsWith(`${n} `);
  const usable = KEY_ACTIONS.filter((x) => x.id !== "search" && menuItemOn(x.id));
  const titles = (x) => [MENU_TITLES[x.id], x.label].filter(Boolean).map(norm);
  // A command by its exact name, else one whose name starts with it.
  const a = usable.find((x) => titles(x).includes(n)) ?? usable.find((x) => titles(x).some(prefix));
  if (a) return { key: `a:${a.id}`, run: () => runMenuItem(a.id) };
  const task = TASKS.find((x) => norm(x.label) === n);
  if (task) return { key: `t:${task.id}`, run: () => switchTask(task.id) };
  for (const it of searchItems()) {
    if ((it.kind === "Panel" || it.kind === "Setting") && it.enabled && norm(it.title) === n) {
      return { key: `${it.kind}:${it.title}`, run: it.run };
    }
  }
  // A button on screen with that name.
  const button = [...document.querySelectorAll("button")]
    .find((b) => b.offsetParent && !b.disabled && !b.closest("dialog") && norm(b.textContent) === n);
  if (button) return { key: `b:${n}`, run: () => button.click() };
  return null;
}

function wireFeatures() {
  $("features-filter").addEventListener("input", renderFeatures);
  $("search-all").addEventListener("click", () => {
    const words = $("search-input").value;
    $("search-dialog").close();
    featuresFlow().then(() => {
      $("features-filter").value = words;
      renderFeatures();
    });
  });
  $("how-all").addEventListener("click", () => {
    $("how-dialog").close();
    if (!$("features-dialog").open) featuresFlow();
  });
}

function searchFlow() {
  const dlg = $("search-dialog");
  if (dlg.open) return;
  loadFeatures();
  searchState.items = searchItems();
  searchState.at = 0;
  $("search-input").value = "";
  boxTip($("search-input"), "Search commands, settings, presets… or what you want to do");
  renderSearch();
  dlg.showModal();
  $("search-input").focus();
}

// A setting: open its panel, select it (= and - then move it) and show it.
function revealSetting(name) {
  const c = ui.controls.get(name);
  if (!c?.label?.isConnected) return;
  const panel = c.label.closest("details.panel");
  revealInTab(panel);
  if (panel) panel.open = true;
  ui.selectedSlider = name;
  showSelectedSlider();
  (c.input ?? c.select ?? c.check)?.focus();
}

function revealPanel(details) {
  revealInTab(details);
  details.open = true;
  details.scrollIntoView({ block: "start", behavior: "smooth" });
}

function switchLook(kind, id) {
  ui.looks[kind === "layout" ? "active_layout" : "active_theme"] = id;
  // A layout that names a theme brings it, as in the Appearance dialog.
  const theme = kind === "layout" && findLook("layout", id)?.data?.theme;
  if (theme && findLook("theme", theme)?.usable) ui.looks.active_theme = theme;
  applyActiveLooks();
  invoke("set_active_look", { layout: ui.looks.active_layout, theme: ui.looks.active_theme }).catch((e) => toast(String(e), true));
}

function wireSearch() {
  const input = $("search-input");
  input.addEventListener("input", () => {
    searchState.at = 0;
    renderSearch();
  });
  input.addEventListener("keydown", (e) => {
    const n = searchState.shown.length;
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      if (n) searchState.at = (searchState.at + (e.key === "ArrowDown" ? 1 : -1) + n) % n;
      renderSearch();
    } else if (e.key === "Enter") {
      e.preventDefault();
      chooseSearch(searchState.at);
    }
  });
}
