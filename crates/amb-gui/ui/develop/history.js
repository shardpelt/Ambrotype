// SPDX-License-Identifier: GPL-3.0-or-later
// History entries in words, the History panel and snapshots. Split out of
// app.js.
"use strict";

// History entries in words: "set_param Exposure2012" → "Exposure" (the
// slider's label), "add_mask linear to mask 0" → "Mask 1: add linear".
function historyLabel(label) {
  const n = (i) => t("Mask {n}", { n: Number(i) + 1 });
  return label
    .replace(/^set_param (\S+)$/, (_, tag) => historyName(tag))
    .replace(/^reset_param (\S+)$/, (_, tag) => t("Reset {name}", { name: historyName(tag) }))
    .replace(/^adjust_param (\S+) (\S+)$/, (_, tag, d) => `${historyName(tag)} ${d}`)
    .replace(/^apply_preset .*[\\/](.*)$/, (_, name) => t("preset {name}", { name }))
    .replace(/^add_mask (\w+) to mask (\d+)$/, (_, k, m) => t("{mask}: add {kind}", { mask: n(m), kind: t(k) }))
    .replace(/^add_mask (\w+)$/, (_, k) => t("New {kind} mask", { kind: t(k) }))
    .replace(/^set_mask (\d+) part (\d+)$/, (_, m, p) => t("{mask}: edit part {part}", { mask: n(m), part: Number(p) + 1 }))
    .replace(/^set_mask (\d+)$/, (_, m) => n(m))
    .replace(/^paint_mask new$/, () => t("New brush mask"))
    .replace(/^(paint|erase)_mask (\d+)$/, (_, v, m) => (v === "paint" ? t("{mask}: brush", { mask: n(m) }) : t("{mask}: erase", { mask: n(m) })))
    .replace(/^remove_mask (\d+) part (\d+)$/, (_, m, p) => t("{mask}: remove part {part}", { mask: n(m), part: Number(p) + 1 }))
    .replace(/^remove_mask (\d+)$/, (_, m) => t("Delete mask {n}", { n: Number(m) + 1 }))
    .replace(/^duplicate_mask (\d+)$/, (_, m) => t("Duplicate mask {n}", { n: Number(m) + 1 }))
    .replace(/^move_mask (\d+) to (\d+)$/, (_, m, to) => t("Move mask {n} to {to}", { n: Number(m) + 1, to: Number(to) + 1 }))
    .replace(/^add_spot (\w+)$/, (_, k) => (k === "clone" ? t("Clone spot") : t("Heal spot")))
    .replace(/^add_spot (\w+) \(brushed\)$/, (_, k) => (k === "clone" ? t("Clone brush") : t("Heal brush")))
    .replace(/^set_spot (\d+)$/, (_, s) => t("Spot {n}", { n: Number(s) + 1 }))
    .replace(/^remove_spot (\d+)$/, (_, s) => t("Delete spot {n}", { n: Number(s) + 1 }));
}

// A setting's name in History: its label, with its section where the label alone is ambiguous.
function historyName(tag) {
  const def = ui.defs.get(tag);
  return ui.historyNames.get(tag) ?? (def?.label ? t(def.label) : tag);
}

// A History value in the slider's own format; unset is the default.
function historyValue(tag, v) {
  if (v == null) {
    if (ui.info?.defaults && tag in ui.info.defaults) v = ui.info.defaults[tag];
    else if (tag === "Temperature" || tag === "Tint") return t("As Shot");
    else v = ui.defs.get(tag)?.default ?? 0;
  }
  // Plugin effects: their names, not the JSON they're kept as.
  if (tag === "amb:Effects") {
    const names = effectsOf(v).map((e) => fx.plugins.find((p) => p.id === e.plugin)?.manifest?.name ?? e.plugin);
    return names.join(", ") || t("none");
  }
  const def = ui.defs.get(tag);
  if (typeof v === "number") return def ? format(def, v) || String(v) : String(Math.round(v * 100) / 100);
  if (typeof v === "boolean") return v ? t("on") : t("off");
  return String(v) || t("none");
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
  $("btn-undo-all").disabled = !ui.state?.can_undo;
  renderSnapshots();
  renderHistory();
  if (ui.curve) for (const tag of AmbCurve.TAGS) ui.curve.set(tag, currentValue(tag));
  if (ui.wheels) ui.wheels.set(Object.fromEntries(AmbWheels.TAGS.map((t) => [t, currentValue(t)])));
  const nr = ui.state?.not_rendered ?? [];
  $("status-right").textContent = nr.length ? t("Not rendered yet: {n} setting(s)", { n: nr.length }) : "";
  $("status-right").title = nr.join(", ");
  syncMenu();
  glowUnused();
}

// Snapshots: named copies of the edit, kept in the photo's sidecar. Click
// one to apply it (one undoable step); hover for update, rename and delete.
// The one matching the current settings is bold.
function renderSnapshots() {
  const list = $("snapshot-list");
  list.replaceChildren();
  $("snapshot-add").disabled = !ui.state;
  if (ui.state && !ui.state.snapshots?.length) emptyTip(list, t("No snapshots yet"));
  for (const s of ui.state?.snapshots ?? []) {
    const li = el("li");
    li.classList.toggle("current", s.current);
    const name = el("span", { class: "snap-name" });
    name.textContent = s.name;
    name.setAttribute("translate", "no");
    name.title = s.current ? t("The edit is this snapshot") : t("Apply this snapshot (undoable)");
    name.addEventListener("click", () => {
      if (!s.current) run([{ cmd: "apply_snapshot", name: s.name }], t("Snapshot: {name}", { name: s.name }));
    });
    const act = (text, title, fn) => {
      const b = el("button", { type: "button", class: "snap-act", title: title }, text);
      b.addEventListener("click", fn);
      return b;
    };
    li.append(
      name,
      act("↻", t("Update with the current settings"), () => run([{ cmd: "save_snapshot", name: s.name }])),
      act("✎", t("Rename"), async () => {
        const to = await askName(t("Rename snapshot"), "", s.name);
        if (to && to !== s.name) run([{ cmd: "rename_snapshot", name: s.name, to }]);
      }),
      act("×", t("Delete"), async () => {
        if (await askYes(t("Delete the snapshot “{name}”? The photo's edit stays as it is.", { name: s.name }))) {
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
  const name = await askName(t("New snapshot"), t("The edit as it is now, under a name, to come back to. Kept in the photo's .xmp."), stamp);
  if (!name) return;
  const taken = (ui.state.snapshots ?? []).some((s) => s.name === name);
  if (taken && !await askYes(t("Replace the snapshot “{name}” with the edit as it is now?", { name }))) return;
  run([{ cmd: "save_snapshot", name }]);
}

// The History panel: Original, the steps done, then the steps undone (greyed)
// until a new edit replaces them. Clicking one goes back or forward to just
// after it, as one batch of undos or redos (so it is undoable too). History
// is kept in the sidecar; a very long one keeps its newest steps, and then
// starts at "Earlier edits".
function renderHistory() {
  const hist = $("history-list");
  hist.replaceChildren();
  if (!ui.state) return;
  const done = ui.state.history ?? [];
  const undone = ui.state.redo_history ?? [];
  const first = ui.state.history_trimmed ? t("Earlier edits") : t("Original");
  const steps = [first, ...done.map(historyLabel), ...undone.map(historyLabel)];
  hist.start = 0;
  const changes = ui.state.history_changes ?? [];
  steps.forEach((text, i) => {
    const li = el("li", {}, text);
    li.classList.toggle("current", i === done.length);
    li.classList.toggle("undone", i > done.length);
    // Its values: one change on the line, every change in the tooltip.
    const step = i > 0 ? changes[i - 1] : null;
    const lines = (step?.shown ?? []).map((c) => `${historyName(c.name)}: ${historyChange(c)}`);
    if (step?.more) lines.push(t("and {n} more", { n: step.more }));
    if (step?.shown.length === 1 && !step.more) {
      const v = el("span", { class: "history-values" }, historyChange(step.shown[0]));
      li.append(" ", v);
    }
    if (i !== done.length) {
      li.title = i < done.length ? t("Go back to this step") : t("Go forward to this step");
      li.addEventListener("click", () => {
        hist.focus({ preventScroll: true });
        historyGo(i - done.length);
      });
    }
    if (lines.length) li.title = [li.title, ...lines].filter(Boolean).join("\n");
    hist.append(li);
  });
  if (!done.length && !undone.length) emptyTip(hist, t("No edits yet"));
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

// Back to the photo as it opened, as History's first step: every step
// stays in History, so Redo (or a click there) brings them back.
function undoAll() {
  const n = ui.state?.history?.length ?? 0;
  if (!n) return;
  historyGo(-n).then(() => status(t("Undid all {steps}; Redo brings them back", { steps: plural(n, "step") })));
}

// With History focused (clicked, or Tab), the arrows step through it one
// step at a time and Home/End go to the first and last; the photo's own
// arrow keys wait until it loses focus.
// Drag History's top edge, or the line under the snapshots, to size them;
// kept on this computer. A double-click goes back to the usual size.
function wireDockGrips() {
  const block = $("block-history");
  const sizes = store.get("amb.historyDock") ?? {};
  const apply = () => {
    block.classList.toggle("sized", sizes.history != null);
    block.style.setProperty("--history-h", sizes.history != null ? `${sizes.history}px` : "");
    block.style.setProperty("--snap-h", sizes.snapshots != null ? `${sizes.snapshots}px` : "");
  };
  apply();
  for (const grip of block.querySelectorAll(".dock-grip")) {
    const which = grip.dataset.grip;
    grip.addEventListener("pointerdown", (e) => {
      e.preventDefault();
      grip.setPointerCapture(e.pointerId);
      grip.classList.add("dragging");
      const start = e.clientY;
      const from = which === "history" ? block.getBoundingClientRect().height : $("snapshot-list").getBoundingClientRect().height;
      const move = (m) => {
        // History grows upwards from its top edge; the snapshots downwards.
        const d = which === "history" ? start - m.clientY : m.clientY - start;
        sizes[which] = Math.round(Math.max(which === "history" ? 90 : 24, from + d));
        apply();
      };
      const up = () => {
        grip.classList.remove("dragging");
        grip.removeEventListener("pointermove", move);
        store.set("amb.historyDock", sizes);
      };
      grip.addEventListener("pointermove", move);
      grip.addEventListener("pointerup", up, { once: true });
      grip.addEventListener("pointercancel", up, { once: true });
    });
    grip.addEventListener("dblclick", () => {
      delete sizes[which];
      apply();
      store.set("amb.historyDock", sizes);
    });
  }
}

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
