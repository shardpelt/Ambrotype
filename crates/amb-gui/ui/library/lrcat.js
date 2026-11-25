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

async function runLrcatPlan() {
  const go = $("lrcat-go");
  go.disabled = true;
  $("lrcat-inplace").disabled = true;
  lrc.plan = null;
  $("lrcat-list").replaceChildren();
  const args = lrcatArgs();
  if (!args.catalog) return;
  const seq = ++lrc.seq;
  $("lrcat-summary").textContent = t("Reading the catalogue…");
  try {
    const plan = await invoke("lrcat_plan", args);
    if (seq !== lrc.seq) return;
    lrc.plan = plan;
    const c = plan.counts;
    const parts = [t("{sidecars} to write", { sidecars: plural(c.write, "sidecar") })];
    if (c.keep) parts.push(t("{n} already there (kept)", { n: c.keep }));
    if (c.missing) parts.push(args.from ? t("{photos} not found", { photos: plural(c.missing, "photo") }) : t("{photos} not found (moved? see above)", { photos: plural(c.missing, "photo") }));
    parts.push(plural(plan.collections.length, "collection"));
    let text = `${parts.join(", ")}.`;
    if (plan.smart_collections.length) {
      text += ` ${t("Smart collections aren't moved (make them again here): {names}.", { names: plan.smart_collections.join(", ") })}`;
    }
    for (const w of plan.warnings) text += ` ${w}.`;
    if (plan.photos.length > LRCAT_SHOWN) text += ` ${t("The first {n} photos are listed.", { n: LRCAT_SHOWN })}`;
    $("lrcat-summary").textContent = text;
    const label = { write: "→", keep: tKey("kept"), missing: tKey("not found") };
    $("lrcat-list").replaceChildren(...plan.photos.slice(0, LRCAT_SHOWN).map((p) => {
      const li = el("li");
      li.classList.toggle("skip", p.action !== "write");
      li.title = `${p.photo}\n${p.sidecar}${p.problem ? `\n${p.problem}` : ""}`;
      li.append(
        el("span", { translate: "no" }, fileName(p.photo)),
        el("span", { class: "arrow" }, t(label[p.action])),
        el("span", { translate: "no" }, fileName(p.sidecar) + (p.problem ? " ⚠" : "")),
      );
      return li;
    }));
    $("lrcat-inplace").disabled = false;
    const work = c.write + plan.collections.length;
    go.disabled = work === 0;
    go.textContent = c.write ? t("Write {sidecars}", { sidecars: plural(c.write, "sidecar") }) : t("Move");
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
  if (choice === "inplace") return openLightroomCatalogue(lrcatArgs().catalog);
  if (choice !== "ok" || !lrc.plan) return;
  $("busy").hidden = false;
  try {
    const r = await invoke("lrcat_apply", lrcatArgs());
    const done = [t("Wrote {sidecars}, made {collections}", { sidecars: plural(r.written, "sidecar"), collections: plural(r.collections, "collection") })];
    if (r.kept) done.push(t("kept {n}", { n: r.kept }));
    if (r.missing) done.push(t("{n} not found", { n: r.missing }));
    const text = done.join(", ");
    toast(r.errors.length ? t("{text}; {problems}: {error}", { text, problems: plural(r.errors.length, "problem"), error: r.errors[0] }) : text, r.errors.length > 0);
    await loadCollections();
    $("lib-reload").click();
  } catch (e) {
    showError(e);
  } finally {
    $("busy").hidden = true;
  }
}
