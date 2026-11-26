// SPDX-License-Identifier: GPL-3.0-or-later
// The Import dialog: copy photos from a card or folder into dated folders,
// renamed from a template, with keywords for all (amb-library's import, also
// `amb import`). It shows what would happen before anything is copied, then
// opens the library on the new photos. Move instead of copy, picking single
// photos (ticks in the list), Stop (import again to carry on) and ejecting
// the card are here too.
"use strict";

// `off`: the photos in the list that are unticked (by source path).
const imp = { wired: false, plan: [], timer: null, seq: 0, off: new Set(), running: false, result: null };

const IMPORT_SHOWN = 300;

function importForm() {
  return $("import-form").elements;
}

// The chosen template: a preset from the list, or the user's own.
function importTemplate(kind) {
  const f = importForm();
  const choice = f[`${kind}_choice`].value;
  return choice === "custom" ? f[kind].value.trim() : choice;
}

function importOptions() {
  const f = importForm();
  return {
    folder: importTemplate("folder"),
    name: importTemplate("name") || "{name}",
    first_number: Math.max(0, Math.round(Number(f.first_number.value) || 0)),
    subfolders: f.subfolders.checked,
    keywords: f.keywords.value.split(",").map((k) => k.trim()).filter(Boolean),
    preset: f.preset.value || null,
    backup: f.backup.value || null,
    mode: f.mode.value,
    only: [],
  };
}

// The photos ticked, for the run: empty (all) while none is unticked.
function importPicked() {
  if (!imp.off.size) return [];
  return imp.plan.filter((p) => !p.skip && !imp.off.has(p.source)).map((p) => p.source);
}

// The photos that would come in: listed, new and ticked.
function importTicked() {
  return imp.plan.filter((p) => !p.skip && !imp.off.has(p.source)).length;
}

// Show a template in its select, or as "your own".
function setTemplate(kind, value) {
  const f = importForm();
  const select = f[`${kind}_choice`];
  const preset = [...select.options].some((o) => o.value === value && o.value !== "custom");
  select.value = preset ? value : "custom";
  f[kind].value = value;
  f[kind].hidden = preset;
}

function wireImport() {
  if (imp.wired) return;
  imp.wired = true;
  const f = importForm();
  for (const b of $("import-form").querySelectorAll("[data-pick]")) {
    b.addEventListener("click", async () => {
      const which = b.dataset.pick;
      const titles = { source: tKey("Import photos from…"), dest: tKey("Import photos into…"), backup: tKey("Keep a second copy in…") };
      const dir = await invoke("pick_folder", { title: t(titles[which]) });
      if (dir) {
        f[which].value = dir;
        if (which === "source") imp.off.clear();
        planImport();
      }
    });
  }
  for (const b of $("import-form").querySelectorAll("[data-clear]")) {
    b.addEventListener("click", () => {
      f[b.dataset.clear].value = "";
    });
  }
  for (const kind of ["folder", "name"]) {
    f[`${kind}_choice`].addEventListener("change", () => {
      const custom = f[`${kind}_choice`].value === "custom";
      f[kind].hidden = !custom;
      if (custom) f[kind].focus();
      planImport();
    });
    f[kind].addEventListener("input", () => planImport(400));
  }
  for (const name of ["first_number", "subfolders"]) f[name].addEventListener("change", () => planImport());
  f.eject.addEventListener("change", () => store.set("amb.importEject", f.eject.checked));
  $("import-all").addEventListener("click", () => importTick(false));
  $("import-none").addEventListener("click", () => importTick(true));
  $("import-go").addEventListener("click", (e) => {
    e.preventDefault();
    importStart();
  });
  $("import-stop").addEventListener("click", () => {
    $("import-summary").textContent = t("Stopping after the photo under way…");
    invoke("import_stop").catch(() => {});
  });
  // Esc doesn't close the dialog in the middle of an import.
  $("import-dialog").addEventListener("cancel", (e) => {
    if (imp.running) e.preventDefault();
  });
}

// All or none of the new photos ticked (also the ones past the listed).
function importTick(none) {
  imp.off = new Set(none ? imp.plan.filter((p) => !p.skip).map((p) => p.source) : []);
  for (const box of $("import-list").querySelectorAll("input")) box.checked = !none;
  importSummary();
}

// The line above the list, and the Import button, from what is ticked.
function importSummary() {
  const go = $("import-go");
  const fresh = importTicked();
  const skipped = imp.plan.filter((p) => p.skip).length;
  const base = skipped
    ? t("{photos} to import, {skipped} already there (skipped).", { photos: plural(fresh, "photo"), skipped })
    : t("{photos} to import.", { photos: plural(fresh, "photo") });
  $("import-summary").textContent = !imp.plan.length ? t("No photos found there.")
    : imp.plan.length > IMPORT_SHOWN ? t("{text} The first {n} are listed.", { text: base, n: IMPORT_SHOWN }) : base;
  go.disabled = fresh === 0 || imp.running;
  go.textContent = fresh ? t("Import {n}", { n: fresh }) : t("Import");
}

// One row of the list: a tick for a new photo, then from, an arrow and to.
function importRow(p, destRoot) {
  const from = fileName(p.source) + (p.extra ? ` +${p.extra}` : "");
  const to = p.skip ? p.skip : p.dest.slice(destRoot.length).replace(/^[\\/]/, "");
  const title = `${p.source}\n${p.captured}${p.skip ? `\n${p.skip}` : `\n→ ${p.dest}`}`;
  const li = el("li", { class: p.skip ? "skip" : null, title });
  const tick = p.skip ? el("span") : el("input", { type: "checkbox", title: t("Import this photo"), checked: !imp.off.has(p.source) });
  if (!p.skip) {
    tick.addEventListener("change", () => {
      if (tick.checked) imp.off.delete(p.source);
      else imp.off.add(p.source);
      importSummary();
    });
  }
  li.append(
    tick,
    el("span", { translate: "no" }, from),
    el("span", { class: "arrow" }, p.skip ? t("skip") : "→"),
    el("span", { translate: p.skip ? null : "no" }, to),
  );
  return li;
}

// Plan again shortly after a change (the latest request wins).
function planImport(delay = 0) {
  clearTimeout(imp.timer);
  imp.timer = setTimeout(runPlan, delay);
}

async function runPlan() {
  const f = importForm();
  const go = $("import-go");
  go.disabled = true;
  imp.plan = [];
  $("import-list").replaceChildren();
  $("import-pick").hidden = true;
  if (!f.source.value || !f.dest.value) {
    $("import-summary").textContent = t("Choose where the photos are and where they go.");
    return;
  }
  const seq = ++imp.seq;
  $("import-summary").textContent = t("Looking at the photos…");
  try {
    const plan = await invoke("import_plan", { source: f.source.value, dest: f.dest.value, options: importOptions() });
    if (seq !== imp.seq) return;
    imp.plan = plan;
    $("import-list").replaceChildren(...plan.slice(0, IMPORT_SHOWN).map((p) => importRow(p, f.dest.value)));
    $("import-pick").hidden = !plan.some((p) => !p.skip);
    importSummary();
  } catch (e) {
    if (seq === imp.seq) $("import-summary").textContent = String(e);
  }
}

async function importFlow() {
  wireImport();
  const f = importForm();
  const d = await invoke("import_defaults");
  if (d.dest && !f.dest.value) f.dest.value = d.dest;
  setTemplate("folder", d.options.folder);
  setTemplate("name", d.options.name);
  f.first_number.value = d.options.first_number;
  f.subfolders.checked = d.options.subfolders;
  // The develop presets, by group; the last one chosen if it's still there.
  if (!ui.presets?.length) ui.presets = await invoke("presets").catch(() => []);
  const groups = new Map();
  for (const p of presetsShown()) groups.set(p.group || "Presets", [...(groups.get(p.group || "Presets") ?? []), p]);
  f.preset.replaceChildren(el("option", { value: "" }, t("None")),
    ...[...groups].map(([group, list]) => {
      const g = el("optgroup", { label: group === "Presets" ? t("Presets") : group, translate: group === "Presets" ? null : "no" });
      g.append(...list.map((p) => el("option", { value: p.path, translate: "no" }, p.name)));
      return g;
    }));
  f.preset.value = presetsShown().some((p) => p.path === d.options.preset) ? d.options.preset : "";
  f.backup.value = d.options.backup ?? "";
  f.mode.value = d.options.mode === "move" ? "move" : "copy";
  f.eject.checked = store.get("amb.importEject", false) === true;
  imp.result = null;
  imp.off.clear();
  $("import-tokens").textContent = t("In your own templates: {tokens}. A / makes a subfolder.", { tokens: d.tokens.map(([token, what]) => `${token} ${what}`).join(", ") });
  const dlg = $("import-dialog");
  dlg.showModal();
  planImport();
  await new Promise((resolve) => dlg.addEventListener("close", resolve, { once: true }));
  // Show the new photos, however the dialog was closed.
  const r = imp.result;
  if (r?.folders.length) {
    await toggleLibrary(true);
    lib.selected.clear();
    await loadLibrary(r.folders[0], r.folders.slice(1));
  }
}

// Import, inside the dialog so Stop is at hand. The dialog closes when all
// went well; after a Stop or a problem it stays, saying what happened.
async function importStart() {
  const f = importForm();
  const dlg = $("import-dialog");
  const source = f.source.value;
  const off = await tauri.event.listen("import-progress", (e) => {
    const p = e.payload;
    const line = p.done < p.total ? t("Importing {n} of {total}: {name}", { n: p.done + 1, total: p.total, name: p.name }) : t("Imported");
    status(line);
    if (p.done < p.total) $("import-summary").textContent = line;
  });
  importRunning(true);
  let again = "";
  try {
    const r = await invoke("import_run", { source, dest: f.dest.value, options: { ...importOptions(), only: importPicked() } });
    imp.result = imp.result ? { ...r, folders: [...new Set([...imp.result.folders, ...r.folders])] } : r;
    const done = r.skipped
      ? t("Imported {photos}, skipped {skipped} already there", { photos: plural(r.imported, "photo"), skipped: r.skipped })
      : t("Imported {photos}", { photos: plural(r.imported, "photo") });
    toast(r.errors.length ? t("{text}; {problems}: {error}", { text: done, problems: plural(r.errors.length, "problem"), error: r.errors[0] }) : done, r.errors.length > 0);
    status(r.folders.length ? t("Imported into {folders}", { folders: plural(r.folders.length, "folder") }) : t("Nothing imported"));
    if (r.cleared) toast(t("Cleared {n} half-copied from an earlier import", { n: r.cleared }));
    if (r.stopped) again = t("Stopped. Import again to carry on: photos already in place are skipped.");
    else if (r.errors.length) again = r.errors[0];
    else if (f.eject.checked) {
      try {
        await invoke("import_eject", { source });
        toast(t("The card can be taken out"));
      } catch (e) {
        again = t(String(e));
      }
    }
  } catch (e) {
    again = t(String(e));
    showError(e);
  } finally {
    off();
    importRunning(false);
  }
  if (!again) {
    dlg.close("ok");
    return;
  }
  await runPlan();
  $("import-summary").textContent = `${again} ${$("import-summary").textContent}`;
}

// The dialog while an import runs: the choices and the list are still, Stop
// shows, and the dialog can't be cancelled.
function importRunning(on) {
  imp.running = on;
  for (const part of [...$("import-form").querySelectorAll(".grid, label.check"), $("import-list"), $("import-pick")]) part.inert = on;
  $("import-stop").hidden = !on;
  $("import-cancel").disabled = on;
  $("import-go").disabled = on || importTicked() === 0;
}
