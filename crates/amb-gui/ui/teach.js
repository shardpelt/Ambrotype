// SPDX-License-Identifier: GPL-3.0-or-later
// Teaching by showing (docs/NEXT.md, "teach by showing"): stepping through
// History lights up the controls each step changed and moves them to their
// value; holding Ctrl (⌘ on a Mac) shows every shortcut on the buttons; on a
// photo not yet edited the most useful sliders glow faintly until first used;
// watch mode replays the sample's edit with a ghost cursor.
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
  if (!prev || !ui.state || teach.quiet) return;
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

// ------------------------------------------------------------- watch mode

// A ghost cursor replays the sample's edit, one History step at a time,
// about 20 seconds in all. Any key or click stops it where it is; the rest
// of the steps stay in History to click through.
async function watchFlow() {
  if (teach.watching) return;
  // The button's own tooltip would stay up while the pointer rests on it.
  const button = $("btn-watch-empty");
  const title = button.title;
  button.removeAttribute("title");
  try {
    await watchSample();
  } finally {
    button.title = title;
  }
}

async function watchSample() {
  status("Getting the sample photo (18 MB the first time)…");
  $("busy").hidden = false;
  let path;
  try {
    path = await invoke("sample_photo", { fresh: true });
  } catch (e) {
    toast(`Couldn't get the sample photo: ${e}`, true);
    status("Ready");
    return;
  } finally {
    $("busy").hidden = true;
  }
  if (!await openPath(path)) return;
  // Back to Original without lighting anything up.
  teach.quiet = true;
  try {
    await run(Array.from({ length: ui.state?.history?.length ?? 0 }, () => ({ cmd: "undo" })));
  } finally {
    teach.quiet = false;
  }

  const w = { stopped: false, timer: null, wake: null };
  teach.watching = w;
  const ghost = document.createElement("div");
  ghost.className = "ghost-cursor";
  ghost.innerHTML = '<svg viewBox="0 0 16 22" width="18" height="24"><path d="M1 1v17l4.5-4.2L8.6 21l3-1.3-3.1-7H14z"/></svg><span></span>';
  const caption = ghost.querySelector("span");
  document.body.append(ghost);
  const stop = (e) => {
    if (!e.isTrusted || w.stopped) return;
    w.stopped = true;
    clearTimeout(w.timer);
    w.wake?.();
  };
  const events = ["keydown", "pointerdown", "wheel"];
  for (const t of events) window.addEventListener(t, stop, true);
  const wait = (ms) => new Promise((done) => {
    w.wake = done;
    w.timer = setTimeout(done, ms);
  });
  const at = { x: innerWidth / 2, y: innerHeight / 2 };
  const glide = (x, y, ms) => {
    ghost.style.transition = `transform ${ms}ms cubic-bezier(0.4, 0, 0.2, 1)`;
    ghost.style.transform = `translate(${x}px, ${y}px)`;
    // The caption goes left of the cursor near the right edge.
    ghost.classList.toggle("flip", x + 24 + caption.offsetWidth > innerWidth);
    Object.assign(at, { x, y });
    return wait(ms);
  };
  ghost.style.transform = `translate(${at.x}px, ${at.y}px)`;
  status("Watching an edit: any key or click stops it");

  try {
    await wait(400);
    while (!w.stopped && ui.state?.redo_history?.length) {
      const label = ui.state.redo_history[0];
      const name = /^set_param (\S+)$/.exec(label)?.[1];
      const row = name && teachRow(name);
      const panel = row?.closest("details");
      if (panel && !panel.open) panel.open = true;
      row?.scrollIntoView({ block: "nearest" });
      caption.textContent = historyLabel(label);
      const target = row ? watchPoint(name, row, currentValue(name)) : centreOf($("history-list"));
      await glide(target.x, target.y, 700);
      if (w.stopped) break;
      ghost.classList.add("pressed");
      teach.fromHistory = true;
      await run([{ cmd: "redo" }]);
      if (row) {
        const end = watchPoint(name, row, currentValue(name));
        await glide(end.x, end.y, 450);
      }
      ghost.classList.remove("pressed");
      await wait(900);
    }
  } finally {
    for (const t of events) window.removeEventListener(t, stop, true);
    ghost.remove();
    teach.watching = null;
  }
  status(w.stopped ? "Stopped. The rest of the edit is in History: click a step to go on" : "That's the edit. Click a History step to see it again");
}

// Where a hand would grab a control: a slider's thumb, else its middle.
function watchPoint(name, row, value) {
  const c = ui.controls.get(name);
  const input = c?.input;
  if (input?.type === "range" && Number.isFinite(Number(value))) {
    const r = input.getBoundingClientRect();
    const min = Number(input.min), max = Number(input.max);
    const k = Math.min(1, Math.max(0, (toSlider(c.def, Number(value)) - min) / (max - min || 1)));
    return { x: r.left + 7 + k * (r.width - 14), y: r.top + r.height / 2 };
  }
  return centreOf(row);
}

function centreOf(el) {
  const r = el.getBoundingClientRect();
  if (!r.width) return { x: innerWidth / 2, y: innerHeight / 2 };
  return { x: r.left + r.width / 2, y: r.top + r.height / 2 };
}
