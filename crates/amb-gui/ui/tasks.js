// SPDX-License-Identifier: GPL-3.0-or-later
// Tasks: the bar at the top (Library · Develop · Color · Retouch · Film ·
// Collage) switches between the library and layouts made for one job each,
// so the right controls are on screen for what you're doing. A task keeps
// the layout you choose while in it (Appearance), on this computer.
"use strict";

const TASKS = [
  { id: "library", label: "Library", hint: "Find, rate, sort and tag photos" },
  { id: "develop", label: "Develop", hint: "Edit the photo: light, colour, detail", layout: "builtin:classic" },
  { id: "color", label: "Color", hint: "Colour grading: wheels, mixer and curves up front", layout: "builtin:color-grading" },
  { id: "retouch", label: "Retouch", hint: "Spots, brushes and masks", layout: "builtin:retouch" },
  { id: "film", label: "Film", hint: "Scanned film negatives", layout: "builtin:film-lab" },
  { id: "collage", label: "Collage", hint: "Prepare photos for a collage", layout: "builtin:collage-prep" },
];

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
  if (id === "library") {
    await toggleLibrary(true);
  } else {
    if (libraryOn()) await toggleLibrary(false);
    const layout = taskLayout(t);
    if (ui.looks && layout !== ui.looks.active_layout && findLook("layout", layout)?.usable) switchLook("layout", layout);
  }
  renderTasks();
}

function renderTasks() {
  const now = currentTask();
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
    b.title = `${t.hint} (${keyLabel(`Ctrl+Alt+${i + 1}`)})`;
    b.addEventListener("click", () => switchTask(t.id));
    return b;
  }));
  renderTasks();
}
