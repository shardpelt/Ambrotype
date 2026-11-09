// SPDX-License-Identifier: GPL-3.0-or-later
// Best in each burst (library → Organise): the bursts among the
// photos shown (or selected), each ranked by focus and open eyes
// (amb_ops::burst; eyes when YuNet is installed in Settings → AI
// models). In the grid the best of each burst is marked with why, and the
// rest are dimmed with what's wrong with them; a bar over the grid picks
// the best and rejects the rest, or stacks each burst with the best on
// top. Click a dimmed photo's badge to make it the best instead.
"use strict";

const bcull = { on: false, bursts: [], notes: new Map(), best: new Set(), faces: false };

async function bestInBursts() {
  if (!libraryOn()) return;
  $("lib-organise-menu").open = false;
  const scope = (selectedPhotos().length > 1 ? selectedPhotos() : lib.shown).map((p) => p.path);
  if (scope.length < 2) return toast("Show (or select) a few photos first.", true);
  $("busy").hidden = false;
  status("Looking at the bursts…");
  try {
    const r = await invoke("library_best_in_bursts", { paths: scope, burstSeconds: 2 });
    if (!r.bursts.length) {
      toast("No bursts here: no photos taken within 2 seconds of each other.");
      return;
    }
    bcull.bursts = r.bursts.map((b) => b.map((x) => x.path));
    bcull.faces = r.faces;
    bcull.notes = new Map(r.bursts.flat().map((x) => [x.path, x.note]));
    setCull(true);
  } catch (e) {
    toast(String(e), true);
  } finally {
    $("busy").hidden = true;
    status("Ready");
  }
}

function setCull(on) {
  bcull.on = on;
  bcull.best = new Set(on ? bcull.bursts.map((b) => b[0]) : []);
  if (!on) {
    bcull.bursts = [];
    bcull.notes.clear();
  }
  renderCullBar();
  for (const p of lib.shown) updateCell(p);
}

function renderCullBar() {
  const bar = $("cull-bar");
  bar.hidden = !bcull.on;
  if (!bcull.on) return;
  const n = bcull.bursts.length;
  const rest = bcull.bursts.reduce((k, b) => k + b.length - 1, 0);
  $("cull-text").textContent = `${n} burst${n === 1 ? "" : "s"}: the best of each is marked, the other ${rest} dimmed.${bcull.faces ? "" : " Focus only: install YuNet (Settings → AI models) to check eyes too."}`;
}

// A cell's look while culling: the best marked with why, the rest dimmed.
function cullCell(cell, p) {
  const note = bcull.on ? bcull.notes.get(p.path) : undefined;
  const best = bcull.best.has(p.path);
  cell.classList.toggle("cull-best", note !== undefined && best);
  cell.classList.toggle("cull-rest", note !== undefined && !best);
  let badge = cell.querySelector(".lib-cull");
  if (note === undefined) {
    badge?.remove();
    return;
  }
  if (!badge) {
    badge = Object.assign(document.createElement("button"), { type: "button", className: "lib-cull" });
    badge.addEventListener("click", (e) => {
      e.stopPropagation();
      cullChoose(cell.dataset.path);
    });
    cell.querySelector(".lib-frame").append(badge);
  }
  badge.textContent = best ? `★ Best${note ? `: ${note}` : ""}` : note || "not the best";
  badge.title = best ? "The best of its burst" : "Click to keep this one instead";
}

// Keep `path` instead of its burst's best.
function cullChoose(path) {
  const b = bcull.bursts.find((x) => x.includes(path));
  if (!b || bcull.best.has(path)) return;
  b.splice(b.indexOf(path), 1);
  b.unshift(path);
  bcull.best = new Set(bcull.bursts.map((x) => x[0]));
  for (const q of b) {
    const p = libPhoto(q);
    if (p) updateCell(p);
  }
}

async function cullReject() {
  const keep = bcull.bursts.map((b) => b[0]);
  const rest = bcull.bursts.flatMap((b) => b.slice(1));
  for (const [paths, flag] of [[keep, "pick"], [rest, "reject"]]) {
    try {
      const updates = await setInfo(paths, { flag });
      for (const u of updates) {
        const p = libPhoto(u.path);
        if (p) p.flag = u.info.flag ?? null;
      }
    } catch (e) {
      return toast(String(e), true);
    }
  }
  status(`Picked ${keep.length}, rejected ${rest.length} (Filter → Rejected shows them; X and U change a flag)`);
  setCull(false);
  renderLibrary();
}

async function cullStack() {
  for (const [i, b] of bcull.bursts.entries()) {
    const id = libPhoto(b[0])?.stack?.id ?? `s${Date.now().toString(36)}b${i}`;
    await setStacks([[b[0]], b.slice(1)], id);
  }
  status(`Stacked ${bcull.bursts.length} burst${bcull.bursts.length === 1 ? "" : "s"}, the best on top`);
  setCull(false);
  renderLibrary();
}

function wireBursts() {
  $("lib-bursts").addEventListener("click", bestInBursts);
  $("cull-reject").addEventListener("click", cullReject);
  $("cull-stack").addEventListener("click", cullStack);
  $("cull-done").addEventListener("click", () => setCull(false));
  tauri.event.listen("bursts-progress", (e) => {
    const [done, total] = e.payload;
    if (total) status(`Looking at the bursts… ${done} of ${total}`);
  });
}
