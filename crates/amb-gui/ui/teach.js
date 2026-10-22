// SPDX-License-Identifier: GPL-3.0-or-later
// Teaching by showing (docs/NEXT.md, "teach by showing"): stepping through
// History lights up the controls each step changed and moves them to their
// value; holding Ctrl (⌘ on a Mac) shows every shortcut on the buttons; on a
// photo not yet edited the most useful sliders glow faintly until first used.
"use strict";

const teach = { tweens: new Map(), held: null, badges: [] };

// The row a setting's control sits in, if this layout shows it.
function teachRow(name) {
  const c = ui.controls.get(name);
  const el = c?.label ?? c?.check ?? c?.select ?? c?.input;
  return el?.closest(".row, .knob, .bar, .number-row") ?? el ?? null;
}

// Draw a setting's control at `v` without editing anything.
function drawControl(c, v) {
  if (c.set) c.set(v);
  else if (c.input) {
    c.input.value = toSlider(c.def, v);
    c.number.value = format(c.def, v);
  }
}

// Slide a control from `from` to its value, so the eye follows the change.
function tweenControl(name, from, to) {
  const c = ui.controls.get(name);
  if (!c || !(c.set || c.input) || !Number.isFinite(from) || !Number.isFinite(to) || from === to) return;
  const id = {};
  teach.tweens.set(name, id);
  const t0 = performance.now();
  const frame = (now) => {
    // Stop when another change or the user's own hand takes over.
    if (teach.tweens.get(name) !== id || ui.active === name || Number(currentValue(name)) !== to) return;
    const k = Math.min(1, (now - t0) / 450);
    drawControl(c, from + (to - from) * (1 - (1 - k) ** 3));
    if (k < 1) requestAnimationFrame(frame);
    else teach.tweens.delete(name);
  };
  requestAnimationFrame(frame);
}

function glow(el) {
  el.classList.remove("taught");
  void el.offsetWidth; // restart the animation
  el.classList.add("taught");
  el.addEventListener("animationend", () => el.classList.remove("taught"), { once: true });
}

// What each control shows now, before a change: where a tween starts.
function teachShown() {
  const out = {};
  if (ui.state) for (const name of ui.controls.keys()) out[name] = currentValue(name);
  return out;
}

// After an undo or redo (a History click, Ctrl+Z): show what it changed.
// `open` (a History click) also opens a closed panel to show it.
function teachMove(before, after, shown, open) {
  const names = new Set([...Object.keys(before), ...Object.keys(after)]);
  let first = null;
  for (const name of names) {
    if (JSON.stringify(before[name]) === JSON.stringify(after[name])) continue;
    const row = teachRow(name);
    if (!row) continue;
    const panel = row.closest("details");
    if (panel && !panel.open) {
      if (!open) continue;
      panel.open = true;
    }
    glow(row);
    first ??= row;
    const from = Number(shown[name]);
    tweenControl(name, from, Number(currentValue(name)));
  }
  first?.scrollIntoView({ block: "nearest", behavior: "smooth" });
}

// Called by setState with the state before it; an undo or redo moves steps
// between the two lists without adding any.
function teachStateChanged(prev, shown) {
  const open = teach.fromHistory;
  teach.fromHistory = false;
  if (!prev || !ui.state) return;
  const was = [prev.history?.length ?? 0, prev.redo_history?.length ?? 0];
  const now = [ui.state.history?.length ?? 0, ui.state.redo_history?.length ?? 0];
  if (was[0] !== now[0] && was[0] + was[1] === now[0] + now[1]) teachMove(prev.settings ?? {}, ui.settings, shown, open);
}

// ------------------------------------------------- hold Ctrl to see the keys

// The buttons with a shortcut: the toolbar's (from the key table) and the tasks.
function shortcutTargets() {
  const out = [];
  for (const a of KEY_ACTIONS) {
    const keys = keysFor(a);
    const el = a.button && $(a.button);
    if (keys.length && el) out.push([el, keys[0]]);
  }
  for (const a of KEY_ACTIONS.filter((x) => x.id.startsWith("task-"))) {
    const el = document.querySelector(`#task-bar [data-task="${a.id.slice(5)}"]`);
    const keys = keysFor(a);
    if (el && keys.length) out.push([el, keys[0]]);
  }
  return out.filter(([el]) => el.getClientRects().length && !el.closest("dialog:not([open])"));
}

// A key as short as it can be while Ctrl is held: "Ctrl+Shift+Z" is "⇧Z".
function shortKey(combo) {
  const plus = combo.endsWith("++");
  const parts = (plus ? combo.slice(0, -2) : combo).split("+").filter(Boolean);
  const key = plus ? "+" : parts.pop();
  const mods = parts.filter((m) => m !== "Ctrl").map((m) => ({ Shift: "⇧", Alt: IS_MAC ? "⌥" : "Alt " })[m] ?? `${m}+`);
  return mods.join("") + (KEY_NAMES[key] ?? key);
}

function showKeyBadges() {
  hideKeyBadges();
  const placed = [];
  for (const [el, key] of shortcutTargets()) {
    const r = el.getBoundingClientRect();
    const b = Object.assign(document.createElement("span"), { className: "key-badge", textContent: shortKey(key) });
    b.style.left = `${Math.round(r.left + r.width / 2)}px`;
    let top = Math.round(r.bottom + 3);
    b.style.top = `${top}px`;
    document.body.append(b);
    // Neighbours that would overlap go a row lower.
    for (let tries = 0; tries < 3; tries++) {
      const me = b.getBoundingClientRect();
      if (!placed.some((o) => me.left < o.right && o.left < me.right && me.top < o.bottom && o.top < me.bottom)) break;
      top += me.height + 2;
      b.style.top = `${top}px`;
    }
    placed.push(b.getBoundingClientRect());
    teach.badges.push(b);
  }
}

function hideKeyBadges() {
  for (const b of teach.badges) b.remove();
  teach.badges = [];
}

// Ctrl (or ⌘) held alone for a moment shows the badges; any other key, a
// click or letting go hides them, so Ctrl+Z and friends never flash them.
function wireKeyBadges() {
  const mod = IS_MAC ? "Meta" : "Control";
  window.addEventListener("keydown", (e) => {
    clearTimeout(teach.held);
    hideKeyBadges();
    if (e.key === mod && !e.repeat && !document.querySelector("dialog[open]")) teach.held = setTimeout(showKeyBadges, 450);
  }, true);
  const stop = () => {
    clearTimeout(teach.held);
    hideKeyBadges();
  };
  window.addEventListener("keyup", stop, true);
  window.addEventListener("pointerdown", stop, true);
  window.addEventListener("blur", stop);
}

// ------------------------------------------- what to try first on a new photo

// On a photo with no edit yet, these glow faintly until one is used.
const FIRST_SLIDERS = ["Exposure2012", "Highlights2012", "Shadows2012", "Temperature", "Vibrance"];

function glowUnused() {
  const fresh = ui.info && Object.keys(ui.settings ?? {}).length === 0;
  for (const name of FIRST_SLIDERS) teachRow(name)?.classList.toggle("try-me", Boolean(fresh));
}

// ------------------------------------------------------- the sample photo

// The sample photo with its edit (src/commands/sample.rs): the History steps
// pulse until one is clicked, then each click shows what that slider did.
async function sampleFlow() {
  status("Getting the sample photo (18 MB the first time)…");
  $("busy").hidden = false;
  let path;
  try {
    path = await invoke("sample_photo", { fresh: false });
  } catch (e) {
    toast(`Couldn't get the sample photo: ${e}`, true);
    status("Ready");
    return;
  } finally {
    $("busy").hidden = true;
  }
  if (!await openPath(path)) return;
  status("Click the steps in History to see what each one did");
  const hist = $("history-list");
  hist.classList.add("try-me");
  hist.addEventListener("click", () => hist.classList.remove("try-me"), { once: true });
}
