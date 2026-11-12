// SPDX-License-Identifier: GPL-3.0-or-later
// Tasks: the bar at the top (Library · Develop · Film · Collage, then the
// task plugins: Trippy, and any installed) switches between the library,
// layouts made for one job each, and the collage maker, so the right
// controls are on screen for what you're doing. What earns a tab:
// docs/design/tabs.md (Color and Retouch became part of Develop, and their
// layouts stay in Appearance). A task keeps the layout you
// choose while in it (Appearance), on this computer. A task plugin
// (`"kind": "task"`, docs/design/plugin-kinds.md) brings its layout
// (`plugin:<id>`) and the buttons its layout's `tools` block shows.
"use strict";

const TASKS = [
  { id: "library", label: "Library", hint: "Find, rate, sort and tag photos" },
  { id: "develop", label: "Develop", hint: "Edit the photo: light, colour, detail", layout: "builtin:classic" },
  { id: "film", label: "Film", hint: "Scanned film negatives", layout: "builtin:film-lab" },
  // The collage maker itself: a screen of its own, not a layout.
  { id: "collage", label: "Collage", hint: "Several photos on one canvas: the collage maker", open: () => collageFlow() },
];

// Tabs that were, until 2026-11-09: someone last in one goes back to Develop.
const RETIRED_TASKS = { color: "builtin:color-grading", retouch: "builtin:retouch", collage: "builtin:collage-prep" };

// The task bar's own layouts (not Develop's everyday one), kept out of the
// Appearance list; task plugins' join when they load.
const TASK_LAYOUTS = new Set(TASKS.filter((t) => t.layout && t.id !== "develop").map((t) => t.layout));

// Task plugins, from the plugin list: added after the built-in tasks, their
// layouts to the looks.
async function loadTaskPlugins() {
  let list = [];
  try {
    list = await invoke("plugins_list");
  } catch {
    return;
  }
  for (const p of list) {
    const task = p.manifest?.kind === "task" && !p.error ? p.manifest.task : null;
    if (!task || TASKS.some((t) => t.id === p.id)) continue;
    const layout = `plugin:${p.id}`;
    TASKS.push({ id: p.id, label: task.label, hint: task.hint || p.manifest.description, layout, tools: task.tools ?? [], plugin: true });
    TASK_LAYOUTS.add(layout);
    if (ui.looks && !findLook("layout", layout)) {
      ui.looks.layouts.push({ id: layout, name: task.layout.name ?? task.label, data: task.layout, usable: true, builtin: p.builtin, problems: [] });
    }
  }
}

// What a task plugin's tool does, run.
async function runTaskTool(tool) {
  if (tool.command) {
    const a = KEY_ACTIONS.find((x) => x.id === tool.command);
    if (!a) return toast(`No command ${tool.command}.`, true);
    return a.run();
  }
  if (!ui.info) return toast("Open a photo first.", true);
  if (tool.effect) return setEffect(tool.effect, toolEffect(tool), tool.label);
  const r = await invoke("run_plugin", { id: tool.plugin, params: tool.params ?? {} });
  setState(r.state);
  requestPreview(false);
  if (r.message) toast(`${tool.label}: ${r.message}`);
}

// An effect button's options; the seed stays for the tile's life, so the
// hover preview is what a click gives.
function toolEffect(tool) {
  tool.seed ??= tripSeed();
  return { recipe: tool.recipe, params: tool.params, where: tool.where, amount: tool.amount, seed: tool.seed };
}

// The tools block: the task on screen's buttons.
function renderTaskTools() {
  const t = TASKS.find((x) => x.id === currentTask());
  const tools = t?.tools ?? [];
  ($("tools-title").querySelector(".fold-head") ?? $("tools-title")).textContent = t?.label ? `${t.label} tools` : "Tools";
  $("tools-task").replaceChildren(...tools.map((tool, i) => {
    const b = Object.assign(document.createElement("button"), { type: "button", className: `tool-tile${tool.big ? " big" : ""}`, title: tool.hint ?? "" });
    b.dataset.tool = String(i);
    b.append(Object.assign(document.createElement("strong"), { textContent: tool.label }));
    if (tool.hint) b.append(Object.assign(document.createElement("span"), { textContent: tool.hint }));
    b.addEventListener("click", () => {
      hoverPreview(null);
      Promise.resolve(runTaskTool(tool)).then(() => { delete tool.seed; }).catch((e) => toast(String(e), true));
    });
    // An effect button shows the photo with it while hovered, as presets do.
    if (tool.effect) {
      b.addEventListener("mouseenter", () => ui.info && hoverPreview(() => effectCommands(tool.effect, toolEffect(tool))));
      b.addEventListener("mouseleave", () => hoverPreview(null));
    }
    return b;
  }));
}

const taskState = (() => {
  return store.get("amb.tasks") ?? {};
})();
taskState.layouts ??= {};

function saveTasks() {
  store.set("amb.tasks", taskState);
}

function taskLayout(t) {
  const mine = taskState.layouts[t.id];
  return mine && findLook("layout", mine)?.usable ? mine : t.layout;
}

// The task on screen: the library, the collage maker, or the editing task
// whose layout is up.
function currentTask() {
  if (libraryOn()) return "library";
  if ($("collage-dialog")?.open) return "collage";
  const active = ui.looks?.active_layout;
  const mine = TASKS.find((t) => t.id === taskState.current && t.layout && taskLayout(t) === active);
  return (mine ?? TASKS.find((t) => t.layout && taskLayout(t) === active))?.id ?? taskState.current ?? "develop";
}

async function switchTask(id) {
  const t = TASKS.find((x) => x.id === id);
  if (!t) return;
  // The collage maker opens over the task you're in, and closes back to it.
  if (t.open) {
    await t.open();
    renderTasks();
    return;
  }
  if ($("collage-dialog")?.open) closeCollage();
  // One step for Back: the views the library passes through on the way don't count.
  const held = nav.hold;
  nav.hold = true;
  try {
    await enterTask(t, id);
  } finally {
    nav.hold = held;
  }
  renderTasks();
}

async function enterTask(t, id) {
  // The layout chosen in the task being left is that task's from now on.
  const from = TASKS.find((x) => x.id === currentTask());
  if (from?.layout && ui.looks?.active_layout && !libraryOn()) taskState.layouts[from.id] = ui.looks.active_layout;
  taskState.current = id;
  saveTasks();
  // The frame's settings are put away; the frame stays on.
  if (!$("frame-panel").hidden) showFrameSettings(false);
  if (id === "library") {
    await toggleLibrary(true);
  } else {
    if (libraryOn()) await leaveLibrary();
    const layout = taskLayout(t);
    if (ui.looks && layout !== ui.looks.active_layout && findLook("layout", layout)?.usable) switchLook("layout", layout);
  }
}

function renderTasks() {
  const now = currentTask();
  // Each task has its own accent (style.css), so you can see where you are.
  document.body.dataset.task = now;
  renderTaskTools();
  navNote();
  for (const b of $("task-bar").querySelectorAll("[data-task]")) {
    const on = b.dataset.task === now;
    b.classList.toggle("on", on);
    b.setAttribute("aria-selected", String(on));
  }
}

// Last in a tab that's gone (Color, Retouch, the old Collage prep): back to
// Develop, its own layout too, once.
function retireTasks() {
  const was = RETIRED_TASKS[taskState.current];
  for (const id of Object.keys(RETIRED_TASKS)) delete taskState.layouts[id];
  if (!was) return;
  taskState.current = "develop";
  saveTasks();
  const develop = TASKS.find((t) => t.id === "develop");
  const layout = taskLayout(develop);
  if (ui.looks?.active_layout === was && !libraryOn() && findLook("layout", layout)?.usable) switchLook("layout", layout);
}

function wireTasks() {
  retireTasks();
  // The collage maker's tab is lit while it's open.
  for (const ev of ["close", "cancel"]) $("collage-dialog").addEventListener(ev, () => setTimeout(renderTasks));
  const bar = $("task-bar");
  bar.replaceChildren(navButtons(), ...TASKS.map((t, i) => {
    const b = Object.assign(document.createElement("button"), { type: "button", textContent: t.label, role: "tab" });
    b.dataset.task = t.id;
    b.setAttribute("aria-label", i < 9 ? `${t.label}: ${t.hint} (${keyLabel(`Ctrl+Alt+${i + 1}`)})` : `${t.label}: ${t.hint}`);
    b.addEventListener("click", () => switchTask(t.id));
    return b;
  }));
  wireTaskSketches();
  wireNavMouse();
  navWatchScroll();
  renderTasks();
}

// Hovering a task shows a sketch of its screen: the side columns with their
// panels around the photo, so you see where it takes you before you go.
function taskSketch(t) {
  const box = Object.assign(document.createElement("div"), { className: "task-sketch" });
  const key = keyLabel(`Ctrl+Alt+${TASKS.indexOf(t) + 1}`);
  box.append(Object.assign(document.createElement("p"), { className: "task-sketch-hint", textContent: `${t.hint} · ${key}` }));
  const data = t.layout && findLook("layout", taskLayout(t))?.data;
  const screen = Object.assign(document.createElement("div"), { className: "task-sketch-screen" });
  const column = (side) => {
    const col = Object.assign(document.createElement("div"), { className: `task-sketch-col ${side}` });
    for (const item of data?.[side]?.items ?? []) {
      const name = typeof item === "string" ? BLOCK_TITLES[item] ?? item : panelTitle(item);
      col.append(Object.assign(document.createElement("span"), { textContent: name, className: item.open ? "open" : "" }));
    }
    return col;
  };
  if (data) screen.append(column("left"), Object.assign(document.createElement("div"), { className: "task-sketch-photo" }), column("right"));
  else screen.append(Object.assign(document.createElement("div"), { className: "task-sketch-grid" }));
  box.append(screen);
  return box;
}

function wireTaskSketches() {
  let timer = null;
  const hide = () => {
    clearTimeout(timer);
    document.querySelector(".task-sketch")?.remove();
  };
  for (const b of $("task-bar").querySelectorAll("[data-task]")) {
    const t = TASKS.find((x) => x.id === b.dataset.task);
    b.addEventListener("mouseenter", () => {
      hide();
      timer = setTimeout(() => {
        const s = taskSketch(t);
        const r = b.getBoundingClientRect();
        s.style.left = `${Math.round(r.left)}px`;
        s.style.top = `${Math.round(r.bottom + 6)}px`;
        document.body.append(s);
      }, 350);
    });
    b.addEventListener("mouseleave", hide);
    b.addEventListener("click", hide);
  }
}

// Back to where you were before, as in a browser: the task (Library,
// Develop, Film, Collage, …) and, in the library, the folder or collection
// shown (with the grid's scroll), its view (Grid, Insights, Map…) and an
// Insights bar's filter. The arrow at the task
// bar's start, Alt+←, or the mouse's back button. Forward has no button
// (room), only Alt+→ and the mouse's forward button.
const nav = { back: [], fwd: [], at: null, hold: false, chain: Promise.resolve() };

function navButtons() {
  const b = Object.assign(document.createElement("button"), { type: "button", id: "btn-nav-back", className: "task-nav", textContent: "‹", disabled: true });
  b.addEventListener("click", navBack);
  return b;
}

// Where you are now.
function navPlace() {
  const task = currentTask();
  const inLib = task === "library" && typeof lib !== "undefined" && lib.on;
  const place = { task, view: inLib ? cull.view : null, pick: inLib ? lib.pick : null, scroll: 0 };
  if (inLib) place.source = { dir: lib.dir, more: [...lib.more], collection: lib.collection, smart: lib.smart };
  place.key = `${task}|${place.view ?? ""}|${place.pick?.words ?? ""}|${navSourceKey(place.source)}`;
  return place;
}

const navSourceKey = (s) => (s ? `${s.dir}|${s.more.join(",")}|${s.collection}|${s.smart}` : "");

// Called whenever the place on screen may have changed: the task
// (renderTasks), the library's view or its Insights filter.
function navNote() {
  if (nav.hold) return;
  const now = navPlace();
  if (now.key === nav.at?.key) return;
  if (nav.at) {
    nav.back.push(nav.at);
    if (nav.back.length > 50) nav.back.shift();
    nav.fwd = [];
  }
  nav.at = now;
  navSetBack();
}

function navSetBack() {
  const b = $("btn-nav-back");
  if (b) b.disabled = !nav.back.length;
}

// Run `fn` without noting steps (the library loads its folder while it
// opens); the place it ends in is noted once, after.
async function navQuiet(fn) {
  const was = nav.hold;
  nav.hold = true;
  try {
    return await fn();
  } finally {
    nav.hold = was;
    navNote();
  }
}

// The grid's scroll, kept on the place shown so Back can return to it.
function navWatchScroll() {
  $("library-grid").addEventListener("scroll", (e) => {
    if (nav.at && !nav.hold) nav.at.scroll = e.target.scrollTop;
  }, { passive: true });
}

// Show the folder or collection of a place the library is not at.
async function navShowSource(s) {
  if (s.smart) await showSmartCollection(s.smart);
  else if (s.collection) await showCollection(s.collection);
  else if (s.dir) await loadLibrary(s.dir, s.more);
  else return;
  renderFolderTree();
}

// -1 back, 1 forward. One step at a time: a second press waits for the first.
function navGo(dir) {
  nav.chain = nav.chain.then(() => navStep(dir), () => navStep(dir));
  return nav.chain;
}

async function navStep(dir) {
  const to = (dir < 0 ? nav.back : nav.fwd).pop();
  if (!to) return;
  (dir < 0 ? nav.fwd : nav.back).push(nav.at);
  nav.hold = true;
  try {
    if (to.task !== currentTask()) await switchTask(to.task);
    if (to.task === "library" && lib.on) {
      const s = to.source;
      if (s && navSourceKey(s) !== navSourceKey(navPlace().source)) await navShowSource(s);
      if (lib.pick !== to.pick) setInsightFilter(to.pick, false);
      if (to.view && cull.view !== to.view) setLibraryView(to.view);
      if (to.scroll) $("library-grid").scrollTop = to.scroll;
    }
  } finally {
    nav.hold = false;
  }
  // Arrived; anything done meanwhile (a quick click elsewhere) is a step on.
  nav.at = to;
  navNote();
}
const navBack = () => navGo(-1);

function wireNavMouse() {
  // Buttons 3 and 4 are the mouse's back and forward.
  window.addEventListener("mouseup", (e) => {
    if (e.button !== 3 && e.button !== 4) return;
    e.preventDefault();
    navGo(e.button === 3 ? -1 : 1);
  });
}
