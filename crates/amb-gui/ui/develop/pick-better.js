// SPDX-License-Identifier: GPL-3.0-or-later
// Pick Better (amb_ops::darkroom::pick_pair): two prints side by side, one
// setting a step down (A) and a step up (B); click the better one. Each pick
// halves the step; after three, or Same, the next setting. Done applies every
// setting moved as one undo step; Cancel or Esc applies nothing.
"use strict";

// Picks per setting: the step halves after each, then the next setting.
const PICK_HALVINGS = 3;

const pick = {
  rounds: [],   // [{ name, step }]: the settings in order, and each one's first step
  round: 0,     // the one on screen
  value: null,  // its value so far (null: as it is now)
  step: 0,      // how far A and B are from it
  halvings: 0,
  moved: {},    // setting → the value picked, not applied yet
  seq: 0,       // the newest pair asked for; older answers are dropped
};

// A setting's name as the panels show it (from the registry).
function pickLabel(name) {
  return t(ui.controls.get(name)?.def?.label ?? name);
}

async function pickOpen() {
  if (!ui.info) return toast(t("Open a photo first."), true);
  try {
    pick.rounds = await invoke("darkroom_pick_rounds");
  } catch (e) {
    return showError(e);
  }
  pick.moved = {};
  if (!$("better-dialog").open) $("better-dialog").showModal();
  pickStart(0);
}

// Round `i`: its setting as it is now, at its first step.
function pickStart(i) {
  if (i >= pick.rounds.length) return pickDone().catch(showError);
  Object.assign(pick, { round: i, value: null, step: pick.rounds[i].step, halvings: 0 });
  pickRender();
}

async function pickRender() {
  const { name } = pick.rounds[pick.round];
  const seq = ++pick.seq;
  $("better-round").textContent = t("Round {n} of {m}: {setting}", { n: pick.round + 1, m: pick.rounds.length, setting: pickLabel(name) });
  $("better-stage").hidden = true;
  $("better-status").textContent = t("Printing…");
  await withBusy($("better-same"), $("better-status"), async () => {
    const sheet = await invoke("darkroom_aid", { kind: "pick", setting: name, value: pick.value, step: pick.step, with: pick.moved });
    if (seq !== pick.seq || !$("better-dialog").open) return;
    pickShow(sheet);
  });
}

// The pair, with a button over each print.
function pickShow({ image, aid }) {
  $("better-img").src = image;
  const boxes = aid.cells.map((c) => {
    const box = el("button", { type: "button", class: "aids-cell", title: t("This one is better") }, el("span", {}, c.label));
    box.style.setProperty("--x", `${(c.x / aid.width) * 100}%`);
    box.style.setProperty("--y", `${(c.y / aid.height) * 100}%`);
    box.style.setProperty("--w", `${(c.w / aid.width) * 100}%`);
    box.style.setProperty("--h", `${(c.h / aid.height) * 100}%`);
    box.addEventListener("click", () => pickChoose(c));
    return box;
  });
  $("better-stage").replaceChildren($("better-img"), ...boxes);
  $("better-stage").hidden = false;
  $("better-status").textContent = "";
}

// The better print: its value, a finer step, or the next setting.
function pickChoose(cell) {
  const { name } = pick.rounds[pick.round];
  pick.value = cell.changes[name];
  pick.moved[name] = pick.value;
  pick.halvings += 1;
  if (pick.halvings >= PICK_HALVINGS) return pickStart(pick.round + 1);
  pick.step /= 2;
  pickRender();
}

// Every setting picked, as one undo step.
async function pickDone() {
  const commands = Object.entries(pick.moved).map(([name, value]) => ({ cmd: "set_param", name, value }));
  pick.moved = {};
  $("better-dialog").close();
  if (commands.length) await run(commands, t("Pick Better"));
}

function wirePickBetter() {
  $("better-same").addEventListener("click", () => pickStart(pick.round + 1));
  $("better-done").addEventListener("click", () => pickDone().catch(showError));
  $("better-cancel").addEventListener("click", () => $("better-dialog").close());
  $("better-dialog").addEventListener("close", () => { pick.seq++; pick.moved = {}; });
}
