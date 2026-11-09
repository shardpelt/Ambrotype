// SPDX-License-Identifier: GPL-3.0-or-later
// History entries in words, the History panel and snapshots. Split out of
// app.js.
"use strict";

// History entries in words: "set_param Exposure2012" → "Exposure" (the
// slider's label), "add_mask linear to mask 0" → "Mask 1: add linear".
function historyLabel(label) {
  const n = (i) => `Mask ${Number(i) + 1}`;
  const named = (tag) => ui.historyNames.get(tag) ?? ui.defs.get(tag)?.label ?? tag;
  return label
    .replace(/^set_param (\S+)$/, (_, t) => named(t))
    .replace(/^reset_param (\S+)$/, (_, t) => `Reset ${named(t)}`)
    .replace(/^adjust_param (\S+) (\S+)$/, (_, t, d) => `${named(t)} ${d}`)
    .replace(/^apply_preset .*[\\/]/, "preset ")
    .replace(/^add_mask (\w+) to mask (\d+)$/, (_, k, m) => `${n(m)}: add ${k}`)
    .replace(/^add_mask (\w+)$/, (_, k) => `New ${k} mask`)
    .replace(/^set_mask (\d+) part (\d+)$/, (_, m, p) => `${n(m)}: edit part ${Number(p) + 1}`)
    .replace(/^set_mask (\d+)$/, (_, m) => n(m))
    .replace(/^paint_mask new$/, "New brush mask")
    .replace(/^(paint|erase)_mask (\d+)$/, (_, v, m) => `${n(m)}: ${v === "paint" ? "brush" : "erase"}`)
    .replace(/^remove_mask (\d+) part (\d+)$/, (_, m, p) => `${n(m)}: remove part ${Number(p) + 1}`)
    .replace(/^remove_mask (\d+)$/, (_, m) => `Delete ${n(m).toLowerCase()}`)
    .replace(/^duplicate_mask (\d+)$/, (_, m) => `Duplicate ${n(m).toLowerCase()}`)
    .replace(/^move_mask (\d+) to (\d+)$/, (_, m, t) => `Move ${n(m).toLowerCase()} to ${Number(t) + 1}`)
    .replace(/^add_spot (\w+)$/, (_, k) => `${k === "clone" ? "Clone" : "Heal"} spot`)
    .replace(/^add_spot (\w+) \(brushed\)$/, (_, k) => `${k === "clone" ? "Clone" : "Heal"} brush`)
    .replace(/^set_spot (\d+)$/, (_, s) => `Spot ${Number(s) + 1}`)
    .replace(/^remove_spot (\d+)$/, (_, s) => `Delete spot ${Number(s) + 1}`);
}

// A History value in the slider's own format; unset is the default.
function historyValue(tag, v) {
  if (v == null) {
    if (ui.info?.defaults && tag in ui.info.defaults) v = ui.info.defaults[tag];
    else if (tag === "Temperature" || tag === "Tint") return "As Shot";
    else v = ui.defs.get(tag)?.default ?? 0;
  }
  // Plugin effects: their names, not the JSON they're kept as.
  if (tag === "amb:Effects") {
    const names = effectsOf(v).map((e) => fx.plugins.find((p) => p.id === e.plugin)?.manifest?.name ?? e.plugin);
    return names.join(", ") || "none";
  }
  const def = ui.defs.get(tag);
  if (typeof v === "number") return def ? format(def, v) || String(v) : String(Math.round(v * 100) / 100);
  if (typeof v === "boolean") return v ? "on" : "off";
  return String(v) || "none";
}

// "0.00 → +0.70" for a step's changes (Redlamp shows them too).
function historyChange(c) {
  return `${historyValue(c.name, c.before)} → ${historyValue(c.name, c.after)}`;
}

function syncControls() {
  for (const [name, c] of ui.controls) {
    if (name === ui.active) continue;
    const v = currentValue(name);
    const changed = name in ui.settings;
    if (c.label) c.label.classList.toggle("changed", changed);
    if (c.set) c.set(v);
    else if (c.check) c.check.checked = asBool(v);
    else if (c.select && name === "CameraProfile") {
      fillProfileOptions(c.select, v);
      c.select.value = JSON.stringify(v ?? "");
    } else if (c.select && name === "amb:LUT") {
      fillLutOptions(c.select, v);
      c.select.value = JSON.stringify(v ?? "");
    } else if (c.select) c.select.value = JSON.stringify(typeof v === "string" && /^\d+$/.test(v) ? Number(v) : v);
    else if (c.input) {
      const n = Number(v);
      c.input.value = toSlider(c.def, n);
      c.number.value = format(c.def, n);
    }
  }
  $("btn-undo").disabled = !ui.state?.can_undo;
  $("btn-redo").disabled = !ui.state?.can_redo;
  renderSnapshots();
  renderHistory();
  if (ui.curve) for (const tag of AmbCurve.TAGS) ui.curve.set(tag, currentValue(tag));
  if (ui.wheels) ui.wheels.set(Object.fromEntries(AmbWheels.TAGS.map((t) => [t, currentValue(t)])));
  const nr = ui.state?.not_rendered ?? [];
  $("status-right").textContent = nr.length ? `Not rendered yet: ${nr.length} setting(s)` : "";
  $("status-right").title = nr.join(", ");
  syncMenu();
  glowUnused();
}

// Snapshots: named copies of the edit, kept in the photo's sidecar. Click
// one to apply it (one undoable step); hover for update, rename and delete.
// The one matching the current settings is bold.
function renderSnapshots() {
  const list = $("snapshot-list");
  list.innerHTML = "";
  $("snapshot-add").disabled = !ui.state;
  for (const s of ui.state?.snapshots ?? []) {
    const li = document.createElement("li");
    li.classList.toggle("current", s.current);
    const name = document.createElement("span");
    name.className = "snap-name";
    name.textContent = s.name;
    name.title = s.current ? "The edit is this snapshot" : "Apply this snapshot (undoable)";
    name.addEventListener("click", () => {
      if (!s.current) run([{ cmd: "apply_snapshot", name: s.name }], `Snapshot: ${s.name}`);
    });
    const act = (text, title, fn) => {
      const b = document.createElement("button");
      b.type = "button";
      b.className = "snap-act";
      b.textContent = text;
      b.title = title;
      b.addEventListener("click", fn);
      return b;
    };
    li.append(
      name,
      act("↻", "Update with the current settings", () => run([{ cmd: "save_snapshot", name: s.name }])),
      act("✎", "Rename", async () => {
        const to = await askName("Rename snapshot", "", s.name);
        if (to && to !== s.name) run([{ cmd: "rename_snapshot", name: s.name, to }]);
      }),
      act("×", "Delete", async () => {
        if (await askYes(`Delete the snapshot “${s.name}”? The photo's edit stays as it is.`)) {
          run([{ cmd: "delete_snapshot", name: s.name }]);
        }
      }),
    );
    list.append(li);
  }
}

async function addSnapshot() {
  if (!ui.state) return;
  const d = new Date();
  const pad = (n) => String(n).padStart(2, "0");
  const stamp = `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
  const name = await askName("New snapshot", "The edit as it is now, under a name, to come back to. Kept in the photo's .xmp.", stamp);
  if (!name) return;
  const taken = (ui.state.snapshots ?? []).some((s) => s.name === name);
  if (taken && !await askYes(`Replace the snapshot “${name}” with the edit as it is now?`)) return;
  run([{ cmd: "save_snapshot", name }]);
}

// The History panel: Original, the steps done, then the steps undone (greyed)
// until a new edit replaces them. Clicking one goes back or forward to just
// after it, as one batch of undos or redos (so it is undoable too). History
// is kept in the sidecar; a very long one keeps its newest steps, and then
// starts at "Earlier edits".
function renderHistory() {
  const hist = $("history-list");
  hist.innerHTML = "";
  if (!ui.state) return;
  const done = ui.state.history ?? [];
  const undone = ui.state.redo_history ?? [];
  const first = ui.state.history_trimmed ? "Earlier edits" : "Original";
  const steps = [first, ...done.map(historyLabel), ...undone.map(historyLabel)];
  hist.start = 0;
  const changes = ui.state.history_changes ?? [];
  steps.forEach((text, i) => {
    const li = document.createElement("li");
    li.textContent = text;
    li.classList.toggle("current", i === done.length);
    li.classList.toggle("undone", i > done.length);
    // Its values: one change on the line, every change in the tooltip.
    const step = i > 0 ? changes[i - 1] : null;
    const lines = (step?.shown ?? []).map((c) => `${ui.historyNames.get(c.name) ?? ui.defs.get(c.name)?.label ?? c.name}: ${historyChange(c)}`);
    if (step?.more) lines.push(`and ${step.more} more`);
    if (step?.shown.length === 1 && !step.more) {
      const v = document.createElement("span");
      v.className = "history-values";
      v.textContent = historyChange(step.shown[0]);
      li.append(" ", v);
    }
    if (i !== done.length) {
      li.title = i < done.length ? "Go back to this step" : "Go forward to this step";
      li.addEventListener("click", () => {
        hist.focus({ preventScroll: true });
        historyGo(i - done.length);
      });
    }
    if (lines.length) li.title = [li.title, ...lines].filter(Boolean).join("\n");
    hist.append(li);
  });
  // Keep the current step in view inside the list only: scrollIntoView would
  // also scroll the column, and the panels above History with it.
  const cur = hist.querySelector(".current");
  if (cur) {
    const top = cur.offsetTop - hist.offsetTop;
    if (top < hist.scrollTop) hist.scrollTop = top;
    else if (top + cur.offsetHeight > hist.scrollTop + hist.clientHeight) hist.scrollTop = top + cur.offsetHeight - hist.clientHeight;
  }
}

// Move `n` steps through History: back when negative, as one batch.
// A held arrow key waits for each step before the next.
async function historyGo(n) {
  if (!n || ui.historyBusy) return;
  ui.historyBusy = true;
  teach.fromHistory = true;
  try {
    await run(Array.from({ length: Math.abs(n) }, () => ({ cmd: n < 0 ? "undo" : "redo" })));
  } finally {
    ui.historyBusy = false;
  }
}

// With History focused (clicked, or Tab), the arrows step through it one
// step at a time and Home/End go to the first and last; the photo's own
// arrow keys wait until it loses focus.
function wireHistoryKeys() {
  const hist = $("history-list");
  hist.addEventListener("keydown", (e) => {
    if (e.ctrlKey || e.metaKey || e.altKey || !ui.state) return;
    const done = ui.state.history?.length ?? 0;
    const undone = ui.state.redo_history?.length ?? 0;
    const n = { ArrowUp: -1, ArrowLeft: -1, ArrowDown: 1, ArrowRight: 1, Home: -done, End: undone }[e.key];
    if (n === undefined) return;
    e.preventDefault();
    e.stopPropagation();
    historyGo(Math.max(-done, Math.min(undone, n)));
  });
}
