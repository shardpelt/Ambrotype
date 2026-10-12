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
      const dir = await invoke("pick_folder", { title: which === "source" ? "Import photos from…" : "Import photos into…" });
      if (dir) {
        f[which].value = dir;
        planImport();
      }
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
    $("import-summary").textContent = "Choose where the photos are and where they go.";
    return;
  }
  const seq = ++imp.seq;
  $("import-summary").textContent = "Looking at the photos…";
  try {
    const plan = await invoke("import_plan", { source: f.source.value, dest: f.dest.value, options: importOptions() });
    if (seq !== imp.seq) return;
    imp.plan = plan;
    const fresh = plan.filter((p) => !p.skip).length;
    const skipped = plan.length - fresh;
    $("import-summary").textContent = !plan.length ? "No photos found there."
      : `${fresh} photo${fresh === 1 ? "" : "s"} to import${skipped ? `, ${skipped} already there (skipped)` : ""}.`
        + (plan.length > IMPORT_SHOWN ? ` The first ${IMPORT_SHOWN} are listed.` : "");
    $("import-list").replaceChildren(...plan.slice(0, IMPORT_SHOWN).map((p) => {
      const li = document.createElement("li");
      li.classList.toggle("skip", Boolean(p.skip));
      const from = fileName(p.source) + (p.extra ? ` +${p.extra}` : "");
      const to = p.skip ? p.skip : p.dest.slice(f.dest.value.length).replace(/^[\\/]/, "");
      li.title = `${p.source}\n${p.captured}${p.skip ? `\n${p.skip}` : `\n→ ${p.dest}`}`;
      li.append(
        Object.assign(document.createElement("span"), { textContent: from }),
        Object.assign(document.createElement("span"), { className: "arrow", textContent: p.skip ? "skip" : "→" }),
        Object.assign(document.createElement("span"), { textContent: to }),
      );
      return li;
    }));
    go.disabled = fresh === 0;
    go.textContent = fresh ? `Import ${fresh}` : "Import";
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
  $("import-tokens").textContent = `In your own templates: ${d.tokens.map(([t, what]) => `${t} ${what}`).join(", ")}. A / makes a subfolder.`;
  const dlg = $("import-dialog");
  dlg.showModal();
  planImport();
  const choice = await new Promise((resolve) => dlg.addEventListener("close", () => resolve(dlg.returnValue), { once: true }));
  if (choice !== "ok" || !imp.plan.some((p) => !p.skip)) return;
  const off = await tauri.event.listen("import-progress", (e) => {
    const p = e.payload;
    status(p.done < p.total ? `Importing ${p.done + 1} of ${p.total}: ${p.name}` : "Imported");
  });
  $("busy").hidden = false;
  try {
    const r = await invoke("import_run", { source: f.source.value, dest: f.dest.value, options: importOptions() });
    const failed = r.errors.length ? `; ${r.errors.length} problem${r.errors.length === 1 ? "" : "s"}: ${r.errors[0]}` : "";
    toast(`Imported ${r.imported} photo${r.imported === 1 ? "" : "s"}${r.skipped ? `, skipped ${r.skipped} already there` : ""}${failed}`, r.errors.length > 0);
    status(r.folders.length ? `Imported into ${r.folders.length} folder${r.folders.length === 1 ? "" : "s"}` : "Nothing imported");
    // Show the new photos.
    if (r.folders.length) {
      await toggleLibrary(true);
      lib.selected.clear();
      await loadLibrary(r.folders[0], r.folders.slice(1));
    }
  } catch (e) {
    toast(String(e), true);
  } finally {
    off();
    $("busy").hidden = true;
  }
}
