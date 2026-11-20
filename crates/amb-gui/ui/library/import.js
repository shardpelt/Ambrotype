// SPDX-License-Identifier: GPL-3.0-or-later
// The Import dialog: copy photos from a card or folder into dated folders,
// renamed from a template, with keywords for all (amb-library's import, also
// `amb import`). It shows what would happen before anything is copied, then
// opens the library on the new photos.
"use strict";

const imp = { wired: false, plan: [], timer: null, seq: 0 };

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
  };
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
    const fresh = plan.filter((p) => !p.skip).length;
    const skipped = plan.length - fresh;
    const base = skipped
      ? t("{photos} to import, {skipped} already there (skipped).", { photos: plural(fresh, "photo"), skipped })
      : t("{photos} to import.", { photos: plural(fresh, "photo") });
    $("import-summary").textContent = !plan.length ? t("No photos found there.")
      : plan.length > IMPORT_SHOWN ? t("{text} The first {n} are listed.", { text: base, n: IMPORT_SHOWN }) : base;
    $("import-list").replaceChildren(...plan.slice(0, IMPORT_SHOWN).map((p) => {
      const li = document.createElement("li");
      li.classList.toggle("skip", Boolean(p.skip));
      const from = fileName(p.source) + (p.extra ? ` +${p.extra}` : "");
      const to = p.skip ? p.skip : p.dest.slice(f.dest.value.length).replace(/^[\\/]/, "");
      li.title = `${p.source}\n${p.captured}${p.skip ? `\n${p.skip}` : `\n→ ${p.dest}`}`;
      li.append(
        el("span", { translate: "no" }, from),
        el("span", { class: "arrow" }, p.skip ? t("skip") : "→"),
        el("span", { translate: p.skip ? null : "no" }, to),
      );
      return li;
    }));
    go.disabled = fresh === 0;
    go.textContent = fresh ? t("Import {n}", { n: fresh }) : t("Import");
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
  $("import-tokens").textContent = t("In your own templates: {tokens}. A / makes a subfolder.", { tokens: d.tokens.map(([token, what]) => `${token} ${what}`).join(", ") });
  const dlg = $("import-dialog");
  dlg.showModal();
  planImport();
  const choice = await new Promise((resolve) => dlg.addEventListener("close", () => resolve(dlg.returnValue), { once: true }));
  if (choice !== "ok" || !imp.plan.some((p) => !p.skip)) return;
  const off = await tauri.event.listen("import-progress", (e) => {
    const p = e.payload;
    status(p.done < p.total ? t("Importing {n} of {total}: {name}", { n: p.done + 1, total: p.total, name: p.name }) : t("Imported"));
  });
  $("busy").hidden = false;
  try {
    const r = await invoke("import_run", { source: f.source.value, dest: f.dest.value, options: importOptions() });
    const done = r.skipped
      ? t("Imported {photos}, skipped {skipped} already there", { photos: plural(r.imported, "photo"), skipped: r.skipped })
      : t("Imported {photos}", { photos: plural(r.imported, "photo") });
    toast(r.errors.length ? t("{text}; {problems}: {error}", { text: done, problems: plural(r.errors.length, "problem"), error: r.errors[0] }) : done, r.errors.length > 0);
    status(r.folders.length ? t("Imported into {folders}", { folders: plural(r.folders.length, "folder") }) : t("Nothing imported"));
    // Show the new photos.
    if (r.folders.length) {
      await toggleLibrary(true);
      lib.selected.clear();
      await loadLibrary(r.folders[0], r.folders.slice(1));
    }
  } catch (e) {
    showError(e);
  } finally {
    off();
    $("busy").hidden = true;
  }
}
