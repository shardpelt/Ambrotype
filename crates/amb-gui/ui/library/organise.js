// SPDX-License-Identifier: GPL-3.0-or-later
// File care (library → Organise): move, rename, sort into dated folders and
// delete photos with their sidecars, JPEG twins and virtual copies
// (amb_library::files, src/commands/files.rs); find photos moved outside
// the app; tidy the keyword list; metadata templates. Moves are shown
// before they happen and can be undone; deleting goes to the system's
// trash. The catalogue and collections follow every move.
"use strict";

const org = { mode: null, plan: [], last: null, timer: 0, templates: [] };

// What the file operations act on: the selection, or for sorting into
// folders every photo shown when none is selected.
function organiseTargets(all = false) {
  const sel = selectedPhotos();
  return sel.length ? sel : all ? lib.shown : [];
}

const NAME_TEMPLATES = [
  ["{yyyy}{mm}{dd}_{seq}", tKey("Date and number: 20260714_0001")],
  ["{yyyy}-{mm}-{dd}_{name}", tKey("Date and name: 2026-07-14_IMG_1234")],
  ["{camera}_{seq}", tKey("Camera and number: Z6_0001")],
  ["{name}", tKey("Keep the names")],
];
const FOLDER_TEMPLATES = [
  ["{yyyy}/{yyyy}-{mm}-{dd}", tKey("Year, then day: 2026/2026-07-14")],
  ["{yyyy}/{mm}", tKey("Year, then month: 2026/07")],
  ["{yyyy}-{mm}-{dd}", tKey("One folder a day: 2026-07-14")],
];

// ---------------------------------------------------------------- move, rename, dated

async function organiseFlow(mode) {
  $("lib-organise-menu").open = false;
  const photos = organiseTargets(mode === "dated");
  if (!photos.length) return toast(t("Select the photos first."), true);
  org.mode = mode;
  org.paths = photos.map((p) => p.path);
  org.plan = [];
  const dlg = $("files-dialog");
  const title = { move: tKey("Move to a folder"), rename: tKey("Rename"), dated: tKey("Sort into dated folders") }[mode];
  $("files-title").textContent = t("{title}: {photos}", { title: t(title), photos: plural(photos.length, "photo") });
  const opts = $("files-options");
  opts.replaceChildren();
  if (mode === "move" || mode === "dated") {
    const row = el("div", { class: "folder-row" });
    const label = el("span", { id: "files-folder", class: "files-folder", translate: "no" }, lib.dir ?? "");
    const pick = el("button", { type: "button", class: "small" }, t("Choose folder…"));
    pick.addEventListener("click", async () => {
      const dir = await invoke("library_pick_folder");
      if (!dir) return;
      label.textContent = dir;
      planFiles();
    });
    row.append(el("span", {}, mode === "move" ? t("Into") : t("Under")), label, pick);
    opts.append(row);
    if (mode === "move") label.textContent = "";
  }
  if (mode === "rename" || mode === "dated") {
    const presets = mode === "rename" ? NAME_TEMPLATES : FOLDER_TEMPLATES;
    const sel = el("select", { id: "files-preset" });
    sel.append(...presets.map(([v, text]) => el("option", { value: v }, t(text))),
      el("option", { value: "" }, t("My own…")));
    const input = el("input", { id: "files-template", type: "text", spellcheck: "false", value: presets[0][0] });
    sel.addEventListener("change", () => {
      if (sel.value) input.value = sel.value;
      input.focus();
      planFiles();
    });
    input.addEventListener("input", () => {
      sel.value = presets.some(([v]) => v === input.value.trim()) ? input.value.trim() : "";
      clearTimeout(org.timer);
      org.timer = setTimeout(planFiles, 250);
    });
    const tokens = el("p", {
      class: "hint",
    }, mode === "dated"
      ? t("{name} the name now · {yyyy} {yy} {mm} {dd} {hh} {min} {ss} the capture time · {seq} a number in capture order · {camera} · / makes a subfolder")
      : t("{name} the name now · {yyyy} {yy} {mm} {dd} {hh} {min} {ss} the capture time · {seq} a number in capture order · {camera}"));
    opts.append(sel, input, tokens);
  }
  $("files-list").replaceChildren();
  $("files-summary").textContent = mode === "move" ? t("Choose the folder they go into.") : "";
  $("files-go").disabled = true;
  $("files-go").textContent = mode === "rename" ? t("Rename") : t("Move");
  dlg.showModal();
  if (mode !== "move") planFiles();
}

function filesTarget() {
  const folder = $("files-folder")?.textContent;
  const template = $("files-template")?.value.trim();
  if (org.mode === "move") return folder ? { kind: "move", folder } : null;
  if (org.mode === "rename") return template ? { kind: "rename", template } : null;
  return folder && template ? { kind: "dated", root: folder, template } : null;
}

async function planFiles() {
  const target = filesTarget();
  $("files-go").disabled = true;
  if (!target) return;
  try {
    org.plan = await invoke("files_plan", { paths: org.paths, target });
  } catch (e) {
    org.plan = [];
    $("files-summary").textContent = String(e);
    $("files-list").replaceChildren();
    return;
  }
  const files = org.plan.reduce((n, p) => n + p.files.length, 0);
  const same = org.paths.length - org.plan.length;
  const counts = { photos: plural(org.plan.length, "photo"), files: plural(files, "file"), same };
  $("files-summary").textContent = org.plan.length
    ? same > 0
      ? t("{photos}, {files} with their sidecars and companions · {same} already there. Nothing is overwritten.", counts)
      : t("{photos}, {files} with their sidecars and companions. Nothing is overwritten.", counts)
    : t("They are already there.");
  const base = (p) => fileName(p);
  const rel = (p) => (org.mode === "rename" ? base(p) : p);
  $("files-list").replaceChildren(...org.plan.slice(0, 300).map((p) => {
    const row = el("div", { class: "files-row" });
    row.append(
      el("span", { translate: "no" }, base(p.photo)),
      el("span", { class: "files-arrow" }, "→"),
      el("span", { translate: "no", title: p.files.map((f) => base(f[1])).join("\n") }, rel(p.to)),
    );
    return row;
  }), ...(org.plan.length > 300 ? [el("p", { class: "hint" }, t("and {n} more", { n: org.plan.length - 300 }))] : []));
  $("files-go").disabled = !org.plan.length;
}

// Photos dragged onto a folder in the Folders panel: moved with their files,
// no dialog (Organise → Undo the last move puts them back).
async function movePhotosTo(paths, folder) {
  try {
    const plan = await invoke("files_plan", { paths, target: { kind: "move", folder } });
    if (!plan.length) return status(t("They are already there"));
    await applyFiles(plan);
    await loadFolderTree();
  } catch (e) {
    showError(e);
  }
}

async function applyFiles(plan, undo = false) {
  try {
    const r = await invoke("files_apply", { plan });
    if (r.errors.length) toast(t("{n} couldn't be moved: {error}", { n: r.errors.length, error: r.errors[0] }), true);
    if (!undo) {
      org.last = r.moves;
      $("lib-undo-move").hidden = !r.moves.length;
    } else {
      org.last = null;
      $("lib-undo-move").hidden = true;
    }
    const vars = { photos: plural(r.photos, "photo"), files: r.files };
    status(undo ? t("Moved back {photos} ({files} files)", vars) : t("Moved {photos} ({files} files) · Organise → Undo puts them back", vars));
    // The selection follows the photos.
    const moved = new Map(r.moves);
    lib.selected = new Set([...lib.selected].map((p) => moved.get(p) ?? p));
    if (r.reopen) await openPath(r.reopen);
    await reloadLibraryView();
  } catch (e) {
    showError(e);
  }
}

// Show what the library showed again, after files changed.
async function reloadLibraryView() {
  if (lib.smart) return showSmartCollection(lib.smart);
  if (lib.collection) return showCollection(lib.collection);
  if (lib.dir) return loadLibrary(lib.dir);
}

function undoMove() {
  $("lib-organise-menu").open = false;
  if (!org.last?.length) return;
  return applyFiles([{ photo: "", to: "", files: org.last.map(([a, b]) => [b, a]).reverse() }], true);
}

// ---------------------------------------------------------------- delete

async function deleteFlow() {
  $("lib-organise-menu").open = false;
  const photos = organiseTargets();
  if (!photos.length) return toast(t("Select the photos to delete first."), true);
  const n = photos.length;
  const ok = await askYes(n === 1
    ? t("Move {photos} to the trash, with its sidecars, JPEG twins and virtual copies?\n\nThe trash can give them back.", { photos: plural(n, "photo") })
    : t("Move {photos} to the trash, with their sidecars, JPEG twins and virtual copies?\n\nThe trash can give them back.", { photos: plural(n, "photo") }));
  if (!ok) return;
  try {
    const [files, closed] = await invoke("files_trash", { paths: photos.map((p) => p.path) });
    if (closed) photoGone();
    lib.selected.clear();
    status(t("Moved {photos} ({files} files) to the trash", { photos: plural(n, "photo"), files }));
    await reloadLibraryView();
  } catch (e) {
    showError(e);
  }
}

// The open photo was deleted: the editor goes back to its empty state.
function photoGone() {
  ui.info = null;
  $("preview").hidden = true;
  $("empty").hidden = false;
  $("photo-title").textContent = "";
  $("exif").textContent = "";
  for (const id of ["btn-save", "btn-export", "btn-before", "btn-crop", "btn-masks", "btn-spots", "btn-zoom", "btn-wb", "btn-clip", "btn-frame", "btn-save-preset", "btn-match", "btn-surprise"]) $(id).disabled = true;
}

// ---------------------------------------------------------------- missing photos

async function missingFlow() {
  $("lib-organise-menu").open = false;
  const dlg = $("missing-dialog");
  $("missing-list").replaceChildren();
  $("missing-status").textContent = t("Looking…");
  dlg.showModal();
  try {
    const missing = await invoke("files_missing");
    $("missing-status").textContent = !missing.length
      ? t("Every photo the library knows is where it was. (Photos on drives that aren't plugged in don't count.)")
      : missing.length === 1
        ? t("{n} photo isn't where the library last saw it (moved or deleted outside Ambrotype). Choose a folder to look in: photos found there by name and size are followed, in collections too.", { n: 1 })
        : t("{n} photos aren't where the library last saw them (moved or deleted outside Ambrotype). Choose a folder to look in: photos found there by name and size are followed, in collections too.", { n: missing.length });
    $("missing-list").replaceChildren(...missing.slice(0, 300).map((p) => el("div", { class: "files-row", translate: "no" }, p)));
    $("missing-look").disabled = !missing.length;
  } catch (e) {
    $("missing-status").textContent = String(e);
  }
}

async function lookForMissing() {
  const folder = await invoke("library_pick_folder");
  if (!folder) return;
  $("missing-status").textContent = t("Looking in {folder}…", { folder });
  try {
    const found = await invoke("files_find_moved", { folder });
    toast(found.length ? t("Found {photos}", { photos: plural(found.length, "photo") }) : t("None of them is in that folder"), !found.length);
    await missingFlow();
    await reloadLibraryView();
  } catch (e) {
    showError(e);
  }
}

// ---------------------------------------------------------------- keywords

async function keywordsFlow() {
  $("lib-organise-menu").open = false;
  $("keywords-dialog").showModal();
  await renderKeywordList();
}

async function renderKeywordList() {
  const list = $("keywords-list");
  list.replaceChildren(el("p", { class: "hint" }, t("Reading the catalogue…")));
  const all = await invoke("keywords_all").catch((e) => (showError(e), []));
  const filter = $("keywords-filter").value.trim().toLowerCase();
  list.replaceChildren(...all.filter(([k]) => !filter || k.toLowerCase().includes(filter)).map(([k, n]) => {
    const row = el("label", { class: "keyword-row" });
    const box = el("input", { type: "checkbox", value: k });
    box.addEventListener("change", keywordButtons);
    row.append(box, el("span", { translate: "no" }, k), el("span", { class: "hint" }, n));
    return row;
  }));
  if (!all.length) list.replaceChildren(el("p", { class: "hint" }, t("No keywords yet.")));
  keywordButtons();
}

const checkedKeywords = () => [...$("keywords-list").querySelectorAll("input:checked")].map((b) => b.value);

function keywordButtons() {
  const n = checkedKeywords().length;
  const sel = selectedPhotos().length;
  $("keywords-add").disabled = !n || !sel;
  $("keywords-add").textContent = sel ? t("Add to {n} selected", { n: sel }) : t("Add to the selection");
  $("keywords-rename").disabled = n === 0;
  $("keywords-rename").textContent = n > 1 ? t("Merge {n} into…", { n }) : t("Rename…");
  $("keywords-remove").disabled = n === 0;
}

async function changeKeywords(remove) {
  const from = checkedKeywords();
  if (!from.length) return;
  let to = "";
  if (!remove) {
    const quoted = from.map((k) => `“${k}”`).join(", ");
    to = await askName(from.length > 1 ? t("Merge keywords") : t("Rename keyword"), t("Every photo with {keywords} gets this keyword instead, in its sidecar.", { keywords: quoted }), from[0]);
    if (!to) return;
  } else if (!(await askYes(from.length === 1
    ? t("Remove {keywords} from every photo that has it?", { keywords: `“${from[0]}”` })
    : t("Remove {keywords} from every photo that has them?", { keywords: from.map((k) => `“${k}”`).join(", ") })))) {
    return;
  }
  try {
    const n = await invoke("keywords_change", { from, to });
    status(remove ? t("Removed on {photos}", { photos: plural(n, "photo") }) : t("Changed on {photos}", { photos: plural(n, "photo") }));
    await renderKeywordList();
    await reloadLibraryView();
    if (ui.info) loadInfo();
  } catch (e) {
    showError(e);
  }
}

// ---------------------------------------------------------------- metadata templates

async function metaTemplatesFlow() {
  $("lib-organise-menu").open = false;
  org.templates = await invoke("metadata_templates").catch(() => []);
  renderMetaTemplates();
  $("meta-dialog").showModal();
}

function renderMetaTemplates() {
  const sel = $("meta-choose");
  const keep = sel.value;
  sel.replaceChildren(el("option", { value: "" }, t("New template")),
    ...org.templates.map((tpl) => el("option", { value: tpl.name, translate: "no" }, tpl.name)));
  sel.value = org.templates.some((tpl) => tpl.name === keep) ? keep : "";
  fillMetaTemplate();
}

function fillMetaTemplate() {
  const tpl = org.templates.find((x) => x.name === $("meta-choose").value) ?? {};
  const f = $("meta-form");
  for (const k of ["name", "title", "caption", "creator", "copyright"]) f.elements[k].value = tpl[k] ?? "";
  f.elements.keywords.value = (tpl.keywords ?? []).join(", ");
  $("meta-delete").hidden = !tpl.name;
  const n = selectedPhotos().length;
  $("meta-apply").disabled = !n || !tpl.name;
  $("meta-apply").textContent = n ? t("Apply to {n} selected", { n }) : t("Apply (select photos first)");
}

function metaTemplateFromForm() {
  const f = $("meta-form").elements;
  const text = (k) => f[k].value.trim() || null;
  return { name: f.name.value.trim(), title: text("title"), caption: text("caption"), creator: text("creator"), copyright: text("copyright"),
    keywords: f.keywords.value.split(",").map((k) => k.trim()).filter(Boolean) };
}

async function saveMetaTemplate(remove = false) {
  const tpl = metaTemplateFromForm();
  if (!tpl.name) return toast(t("Give the template a name."), true);
  try {
    org.templates = await invoke("save_metadata_template", { template: tpl, delete: remove });
    $("meta-choose").value = remove ? "" : tpl.name;
    renderMetaTemplates();
    status(remove ? t("Deleted “{name}”", { name: tpl.name }) : t("Saved “{name}”", { name: tpl.name }));
  } catch (e) {
    showError(e);
  }
}

// The first selected photo's metadata into the form.
function metaTemplateFromPhoto() {
  const p = selectedPhotos()[0];
  if (!p) return toast(t("Select a photo first."), true);
  const f = $("meta-form").elements;
  for (const k of ["title", "caption", "creator", "copyright"]) f[k].value = p[k] ?? "";
  f.keywords.value = (p.keywords ?? []).join(", ");
}

async function applyMetaTemplate() {
  const t = metaTemplateFromForm();
  // Only what the template has: empty fields leave the photos' own.
  const change = { add_keywords: t.keywords };
  for (const k of ["title", "caption", "creator", "copyright"]) if (t[k]) change[k] = t[k];
  $("meta-dialog").close();
  await libraryChange(change);
}

function wireOrganise() {
  $("lib-move").addEventListener("click", () => organiseFlow("move"));
  $("lib-rename").addEventListener("click", () => organiseFlow("rename"));
  $("lib-dated").addEventListener("click", () => organiseFlow("dated"));
  $("lib-delete").addEventListener("click", deleteFlow);
  $("lib-undo-move").addEventListener("click", undoMove);
  $("lib-missing").addEventListener("click", missingFlow);
  $("lib-keyword-list").addEventListener("click", keywordsFlow);
  $("lib-meta-templates").addEventListener("click", metaTemplatesFlow);
  $("files-go").addEventListener("click", () => {
    $("files-dialog").close();
    applyFiles(org.plan);
  });
  $("files-cancel").addEventListener("click", () => $("files-dialog").close());
  $("missing-look").addEventListener("click", lookForMissing);
  $("missing-close").addEventListener("click", () => $("missing-dialog").close());
  $("keywords-filter").addEventListener("input", renderKeywordList);
  $("keywords-rename").addEventListener("click", () => changeKeywords(false));
  $("keywords-add").addEventListener("click", async () => {
    await libraryChange({ add_keywords: checkedKeywords() });
    await renderKeywordList();
  });
  $("keywords-remove").addEventListener("click", () => changeKeywords(true));
  $("keywords-close").addEventListener("click", () => $("keywords-dialog").close());
  $("meta-choose").addEventListener("change", fillMetaTemplate);
  $("meta-save").addEventListener("click", () => saveMetaTemplate(false));
  $("meta-delete").addEventListener("click", () => saveMetaTemplate(true));
  $("meta-from-photo").addEventListener("click", metaTemplateFromPhoto);
  $("meta-apply").addEventListener("click", applyMetaTemplate);
  $("meta-close").addEventListener("click", () => $("meta-dialog").close());
}
