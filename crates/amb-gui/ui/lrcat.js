// SPDX-License-Identifier: GPL-3.0-or-later
// Move from Lightroom Classic (amb-library's lrcat, also `amb lrcat import`):
// read a catalogue, show what would be written, then write each photo's
// sidecar and make its collections ours. The catalogue is never changed.
"use strict";

const lrc = { wired: false, plan: null, timer: null, seq: 0 };

const LRCAT_SHOWN = 300;

function lrcatArgs() {
  const f = $("lrcat-form").elements;
  return { catalog: f.catalog.value, from: f.from.value, to: f.to.value };
}

function wireLrcat() {
  if (lrc.wired) return;
  lrc.wired = true;
  const f = $("lrcat-form").elements;
  $("lrcat-pick").addEventListener("click", async () => {
    const path = await invoke("pick_lrcat");
    if (path) {
      f.catalog.value = path;
      planLrcat();
    }
  });
  for (const name of ["from", "to"]) f[name].addEventListener("input", () => planLrcat(400));
}

function planLrcat(delay = 0) {
  clearTimeout(lrc.timer);
  lrc.timer = setTimeout(runLrcatPlan, delay);
}

const plural = (n, one, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;

async function runLrcatPlan() {
  const go = $("lrcat-go");
  go.disabled = true;
  lrc.plan = null;
  $("lrcat-list").replaceChildren();
  const args = lrcatArgs();
  if (!args.catalog) return;
  const seq = ++lrc.seq;
  $("lrcat-summary").textContent = "Reading the catalogue…";
  try {
    const plan = await invoke("lrcat_plan", args);
    if (seq !== lrc.seq) return;
    lrc.plan = plan;
    const c = plan.counts;
    const parts = [`${plural(c.write, "sidecar")} to write`];
    if (c.keep) parts.push(`${c.keep} already there (kept)`);
    if (c.missing) parts.push(`${plural(c.missing, "photo")} not found${args.from ? "" : " (moved? see above)"}`);
    parts.push(plural(plan.collections.length, "collection"));
    let text = `${parts.join(", ")}.`;
    if (plan.smart_collections.length) {
      text += ` Smart collections aren't moved (make them again here): ${plan.smart_collections.join(", ")}.`;
    }
    for (const w of plan.warnings) text += ` ${w}.`;
    if (plan.photos.length > LRCAT_SHOWN) text += ` The first ${LRCAT_SHOWN} photos are listed.`;
    $("lrcat-summary").textContent = text;
    const label = { write: "→", keep: "kept", missing: "not found" };
    $("lrcat-list").replaceChildren(...plan.photos.slice(0, LRCAT_SHOWN).map((p) => {
      const li = document.createElement("li");
      li.classList.toggle("skip", p.action !== "write");
      li.title = `${p.photo}\n${p.sidecar}${p.problem ? `\n${p.problem}` : ""}`;
      li.append(
        Object.assign(document.createElement("span"), { textContent: fileName(p.photo) }),
        Object.assign(document.createElement("span"), { className: "arrow", textContent: label[p.action] }),
        Object.assign(document.createElement("span"), { textContent: fileName(p.sidecar) + (p.problem ? " ⚠" : "") }),
      );
      return li;
    }));
    const work = c.write + plan.collections.length;
    go.disabled = work === 0;
    go.textContent = c.write ? `Write ${plural(c.write, "sidecar")}` : "Move";
  } catch (e) {
    if (seq === lrc.seq) $("lrcat-summary").textContent = String(e);
  }
}

async function lrcatFlow() {
  wireLrcat();
  const dlg = $("lrcat-dialog");
  dlg.showModal();
  if (lrcatArgs().catalog) planLrcat();
  const choice = await new Promise((resolve) => dlg.addEventListener("close", () => resolve(dlg.returnValue), { once: true }));
  if (choice !== "ok" || !lrc.plan) return;
  $("busy").hidden = false;
  try {
    const r = await invoke("lrcat_apply", lrcatArgs());
    const failed = r.errors.length ? `; ${plural(r.errors.length, "problem")}: ${r.errors[0]}` : "";
    toast(`Wrote ${plural(r.written, "sidecar")}, made ${plural(r.collections, "collection")}${r.kept ? `, kept ${r.kept}` : ""}${r.missing ? `, ${r.missing} not found` : ""}${failed}`, r.errors.length > 0);
    await loadCollections();
    $("lib-reload").click();
  } catch (e) {
    toast(String(e), true);
  } finally {
    $("busy").hidden = true;
  }
}
