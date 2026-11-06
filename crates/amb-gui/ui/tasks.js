// SPDX-License-Identifier: GPL-3.0-or-later
// Tasks: the bar at the top (Library · Develop · Color · Retouch · Film ·
// Collage, then the task plugins: Trippy, and any installed) switches
// between the library and layouts made for one job each, so the right
// controls are on screen for what you're doing. A task keeps the layout you
// choose while in it (Appearance), on this computer. A task plugin
// (`"kind": "task"`, docs/design/plugin-kinds.md) brings its layout
// (`plugin:<id>`) and the buttons its layout's `tools` block shows.
"use strict";

const TASKS = [
  { id: "library", label: "Library", hint: "Find, rate, sort and tag photos" },
  { id: "develop", label: "Develop", hint: "Edit the photo: light, colour, detail", layout: "builtin:classic" },
  { id: "color", label: "Color", hint: "Colour grading: wheels, mixer and curves up front", layout: "builtin:color-grading" },
  { id: "retouch", label: "Retouch", hint: "Spots, brushes and masks", layout: "builtin:retouch" },
  { id: "film", label: "Film", hint: "Scanned film negatives", layout: "builtin:film-lab" },
  { id: "collage", label: "Collage", hint: "Prepare photos for a collage", layout: "builtin:collage-prep" },
];

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
  $("tools-title").textContent = t?.label ? `${t.label} tools` : "Tools";
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
  try { return JSON.parse(localStorage.getItem("amb.tasks")) ?? {}; } catch { return {}; }
})();
taskState.layouts ??= {};

function saveTasks() {
  try { localStorage.setItem("amb.tasks", JSON.stringify(taskState)); } catch { /* not kept */ }
}

function taskLayout(t) {
  const mine = taskState.layouts[t.id];
  return mine && findLook("layout", mine)?.usable ? mine : t.layout;
}

// The task on screen: the library, or the editing task whose layout is up.
function currentTask() {
  if (libraryOn()) return "library";
  const active = ui.looks?.active_layout;
  const mine = TASKS.find((t) => t.id === taskState.current && t.layout && taskLayout(t) === active);
  return (mine ?? TASKS.find((t) => t.layout && taskLayout(t) === active))?.id ?? taskState.current ?? "develop";
}

async function switchTask(id) {
  const t = TASKS.find((x) => x.id === id);
  if (!t) return;
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
  for (const b of $("task-bar").children) {
    const on = b.dataset.task === now;
    b.classList.toggle("on", on);
    b.setAttribute("aria-selected", String(on));
  }
}

function wireTasks() {
  const bar = $("task-bar");
  bar.replaceChildren(...TASKS.map((t, i) => {
    const b = Object.assign(document.createElement("button"), { type: "button", textContent: t.label, role: "tab" });
    b.dataset.task = t.id;
    b.setAttribute("aria-label", i < 9 ? `${t.label}: ${t.hint} (${keyLabel(`Ctrl+Alt+${i + 1}`)})` : `${t.label}: ${t.hint}`);
    b.addEventListener("click", () => switchTask(t.id));
    return b;
  }));
  wireTaskSketches();
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
  for (const b of $("task-bar").children) {
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
