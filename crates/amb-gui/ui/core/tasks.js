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
  if (tool.effect) return setEffect(tool.effect, { recipe: tool.recipe, params: tool.params, where: tool.where, amount: tool.amount }, tool.label);
  const r = await invoke("run_plugin", { id: tool.plugin, params: tool.params ?? {} });
  setState(r.state);
  requestPreview(false);
  if (r.message) toast(`${tool.label}: ${r.message}`);
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
    b.addEventListener("click", () => Promise.resolve(runTaskTool(tool)).catch((e) => toast(String(e), true)));
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
  renderTasks();
}

function renderTasks() {
  const now = currentTask();
  // Each task has its own accent (style.css), so you can see where you are.
  document.body.dataset.task = now;
  renderTaskTools();
  navNoteTask(now);
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

// Back to the tasks you were in before (Library, Develop, Film, Collage,
// …), as in a browser: the arrow at the task bar's start, Alt+←, or the
// mouse's back button.
const nav = { back: [], at: null, going: null };

function navButtons() {
  const b = Object.assign(document.createElement("button"), { type: "button", id: "btn-nav-back", className: "task-nav", textContent: "‹", disabled: true });
  b.addEventListener("click", navBack);
  return b;
}

// Called whenever the task on screen may have changed (renderTasks).
function navNoteTask(now) {
  if (now === nav.at) return;
  // Arriving where Back was going isn't a step to come back from.
  if (now === nav.going) nav.going = null;
  else if (nav.at) {
    nav.back.push(nav.at);
    if (nav.back.length > 50) nav.back.shift();
  }
  nav.at = now;
  const b = $("btn-nav-back");
  if (b) b.disabled = !nav.back.length;
}

async function navBack() {
  const id = nav.back.pop();
  if (!id) return;
  nav.going = id;
  await switchTask(id);
}

function wireNavMouse() {
  // Button 3 is the mouse's back button.
  window.addEventListener("mouseup", (e) => {
    if (e.button !== 3) return;
    e.preventDefault();
    navBack();
  });
}
