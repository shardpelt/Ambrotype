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
  ["{yyyy}{mm}{dd}_{seq}", "Date and number: 20260714_0001"],
  ["{yyyy}-{mm}-{dd}_{name}", "Date and name: 2026-07-14_IMG_1234"],
  ["{camera}_{seq}", "Camera and number: Z6_0001"],
  ["{name}", "Keep the names"],
];
const FOLDER_TEMPLATES = [
  ["{yyyy}/{yyyy}-{mm}-{dd}", "Year, then day: 2026/2026-07-14"],
  ["{yyyy}/{mm}", "Year, then month: 2026/07"],
  ["{yyyy}-{mm}-{dd}", "One folder a day: 2026-07-14"],
];

// ---------------------------------------------------------------- move, rename, dated

async function organiseFlow(mode) {
  $("lib-organise-menu").open = false;
  const photos = organiseTargets(mode === "dated");
  if (!photos.length) return toast("Select the photos first.", true);
  org.mode = mode;
  org.paths = photos.map((p) => p.path);
  org.plan = [];
  const dlg = $("files-dialog");
  const title = { move: "Move to a folder", rename: "Rename", dated: "Sort into dated folders" }[mode];
  $("files-title").textContent = `${title}: ${photos.length} photo${photos.length === 1 ? "" : "s"}`;
  const opts = $("files-options");
  opts.replaceChildren();
  if (mode === "move" || mode === "dated") {
    const row = Object.assign(document.createElement("div"), { className: "folder-row" });
    const label = Object.assign(document.createElement("span"), { id: "files-folder", className: "files-folder", textContent: lib.dir ?? "" });
    const pick = Object.assign(document.createElement("button"), { type: "button", className: "small", textContent: "Choose folder…" });
    pick.addEventListener("click", async () => {
      const dir = await invoke("library_pick_folder");
      if (!dir) return;
      label.textContent = dir;
      planFiles();
    });
    row.append(Object.assign(document.createElement("span"), { textContent: mode === "move" ? "Into" : "Under" }), label, pick);
    opts.append(row);
    if (mode === "move") label.textContent = "";
  }
  if (mode === "rename" || mode === "dated") {
    const presets = mode === "rename" ? NAME_TEMPLATES : FOLDER_TEMPLATES;
    const sel = Object.assign(document.createElement("select"), { id: "files-preset" });
    sel.append(...presets.map(([v, t]) => Object.assign(document.createElement("option"), { value: v, textContent: t })),
      Object.assign(document.createElement("option"), { value: "", textContent: "My own…" }));
    const input = Object.assign(document.createElement("input"), { id: "files-template", type: "text", spellcheck: false, value: presets[0][0] });
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
    const tokens = Object.assign(document.createElement("p"), { className: "hint",
      textContent: "{name} the name now · {yyyy} {yy} {mm} {dd} {hh} {min} {ss} the capture time · {seq} a number in capture order · {camera}" + (mode === "dated" ? " · / makes a subfolder" : "") });
    opts.append(sel, input, tokens);
  }
  $("files-list").replaceChildren();
  $("files-summary").textContent = mode === "move" ? "Choose the folder they go into." : "";
  $("files-go").disabled = true;
  $("files-go").textContent = mode === "rename" ? "Rename" : "Move";
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
  $("files-summary").textContent = org.plan.length
    ? `${org.plan.length} photo${org.plan.length === 1 ? "" : "s"}, ${files} file${files === 1 ? "" : "s"} with their sidecars and companions${same > 0 ? ` · ${same} already there` : ""}. Nothing is overwritten.`
    : "They are already there.";
  const base = (p) => p.replace(/^.*[\\/]/, "");
  const rel = (p) => (org.mode === "rename" ? base(p) : p);
  $("files-list").replaceChildren(...org.plan.slice(0, 300).map((p) => {
    const row = Object.assign(document.createElement("div"), { className: "files-row" });
    row.append(
      Object.assign(document.createElement("span"), { textContent: base(p.photo) }),
      Object.assign(document.createElement("span"), { className: "files-arrow", textContent: "→" }),
      Object.assign(document.createElement("span"), { textContent: rel(p.to), title: p.files.map((f) => base(f[1])).join("\n") }),
    );
    return row;
  }), ...(org.plan.length > 300 ? [Object.assign(document.createElement("p"), { className: "hint", textContent: `and ${org.plan.length - 300} more` })] : []));
  $("files-go").disabled = !org.plan.length;
}

async function applyFiles(plan, undo = false) {
  try {
    const r = await invoke("files_apply", { plan });
    if (r.errors.length) toast(`${r.errors.length} couldn't be moved: ${r.errors[0]}`, true);
    if (!undo) {
      org.last = r.moves;
      $("lib-undo-move").hidden = !r.moves.length;
    } else {
      org.last = null;
      $("lib-undo-move").hidden = true;
    }
    status(`${undo ? "Moved back" : "Moved"} ${r.photos} photo${r.photos === 1 ? "" : "s"} (${r.files} files)${undo ? "" : " · Organise → Undo puts them back"}`);
    // The selection follows the photos.
    const moved = new Map(r.moves);
    lib.selected = new Set([...lib.selected].map((p) => moved.get(p) ?? p));
    if (r.reopen) await openPath(r.reopen);
    await reloadLibraryView();
  } catch (e) {
    toast(String(e), true);
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
  if (!photos.length) return toast("Select the photos to delete first.", true);
  const n = photos.length;
  const ok = await askYes(`Move ${n} photo${n === 1 ? "" : "s"} to the trash, with ${n === 1 ? "its" : "their"} sidecars, JPEG twins and virtual copies?\n\nThe trash can give them back.`);
  if (!ok) return;
  try {
    const [files, closed] = await invoke("files_trash", { paths: photos.map((p) => p.path) });
    if (closed) photoGone();
    lib.selected.clear();
    status(`Moved ${n} photo${n === 1 ? "" : "s"} (${files} files) to the trash`);
    await reloadLibraryView();
  } catch (e) {
    toast(String(e), true);
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
  $("missing-status").textContent = "Looking…";
  dlg.showModal();
  try {
    const missing = await invoke("files_missing");
    $("missing-status").textContent = missing.length
      ? `${missing.length} photo${missing.length === 1 ? " isn't" : "s aren't"} where the library last saw ${missing.length === 1 ? "it" : "them"} (moved or deleted outside Ambrotype). Choose a folder to look in: photos found there by name and size are followed, in collections too.`
      : "Every photo the library knows is where it was. (Photos on drives that aren't plugged in don't count.)";
    $("missing-list").replaceChildren(...missing.slice(0, 300).map((p) => Object.assign(document.createElement("div"), { className: "files-row", textContent: p })));
    $("missing-look").disabled = !missing.length;
  } catch (e) {
    $("missing-status").textContent = String(e);
  }
}

async function lookForMissing() {
  const folder = await invoke("library_pick_folder");
  if (!folder) return;
  $("missing-status").textContent = `Looking in ${folder}…`;
  try {
    const found = await invoke("files_find_moved", { folder });
    toast(found.length ? `Found ${found.length} photo${found.length === 1 ? "" : "s"}` : "None of them is in that folder", !found.length);
    await missingFlow();
    await reloadLibraryView();
  } catch (e) {
    toast(String(e), true);
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
  list.replaceChildren(Object.assign(document.createElement("p"), { className: "hint", textContent: "Reading the catalogue…" }));
  const all = await invoke("keywords_all").catch((e) => (toast(String(e), true), []));
  const filter = $("keywords-filter").value.trim().toLowerCase();
  list.replaceChildren(...all.filter(([k]) => !filter || k.toLowerCase().includes(filter)).map(([k, n]) => {
    const row = Object.assign(document.createElement("label"), { className: "keyword-row" });
    const box = Object.assign(document.createElement("input"), { type: "checkbox", value: k });
    box.addEventListener("change", keywordButtons);
    row.append(box, Object.assign(document.createElement("span"), { textContent: k }), Object.assign(document.createElement("span"), { className: "hint", textContent: n }));
    return row;
  }));
  if (!all.length) list.replaceChildren(Object.assign(document.createElement("p"), { className: "hint", textContent: "No keywords yet." }));
  keywordButtons();
}

const checkedKeywords = () => [...$("keywords-list").querySelectorAll("input:checked")].map((b) => b.value);

function keywordButtons() {
  const n = checkedKeywords().length;
  const sel = selectedPhotos().length;
  $("keywords-add").disabled = !n || !sel;
  $("keywords-add").textContent = sel ? `Add to ${sel} selected` : "Add to the selection";
  $("keywords-rename").disabled = n === 0;
  $("keywords-rename").textContent = n > 1 ? `Merge ${n} into…` : "Rename…";
  $("keywords-remove").disabled = n === 0;
}

async function changeKeywords(remove) {
  const from = checkedKeywords();
  if (!from.length) return;
  let to = "";
  if (!remove) {
    to = await askName(from.length > 1 ? "Merge keywords" : "Rename keyword", `Every photo with ${from.map((k) => `“${k}”`).join(", ")} gets this keyword instead, in its sidecar.`, from[0]);
    if (!to) return;
  } else if (!(await askYes(`Remove ${from.map((k) => `“${k}”`).join(", ")} from every photo that has ${from.length === 1 ? "it" : "them"}?`))) {
    return;
  }
  try {
    const n = await invoke("keywords_change", { from, to });
    status(`${remove ? "Removed" : "Changed"} on ${n} photo${n === 1 ? "" : "s"}`);
    await renderKeywordList();
    await reloadLibraryView();
    if (ui.info) loadInfo();
  } catch (e) {
    toast(String(e), true);
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
  sel.replaceChildren(Object.assign(document.createElement("option"), { value: "", textContent: "New template" }),
    ...org.templates.map((t) => Object.assign(document.createElement("option"), { value: t.name, textContent: t.name })));
  sel.value = org.templates.some((t) => t.name === keep) ? keep : "";
  fillMetaTemplate();
}

function fillMetaTemplate() {
  const t = org.templates.find((x) => x.name === $("meta-choose").value) ?? {};
  const f = $("meta-form");
  for (const k of ["name", "title", "caption", "creator", "copyright"]) f.elements[k].value = t[k] ?? "";
  f.elements.keywords.value = (t.keywords ?? []).join(", ");
  $("meta-delete").hidden = !t.name;
  const n = selectedPhotos().length;
  $("meta-apply").disabled = !n || !t.name;
  $("meta-apply").textContent = n ? `Apply to ${n} selected` : "Apply (select photos first)";
}

function metaTemplateFromForm() {
  const f = $("meta-form").elements;
  const text = (k) => f[k].value.trim() || null;
  return { name: f.name.value.trim(), title: text("title"), caption: text("caption"), creator: text("creator"), copyright: text("copyright"),
    keywords: f.keywords.value.split(",").map((k) => k.trim()).filter(Boolean) };
}

async function saveMetaTemplate(remove = false) {
  const t = metaTemplateFromForm();
  if (!t.name) return toast("Give the template a name.", true);
  try {
    org.templates = await invoke("save_metadata_template", { template: t, delete: remove });
    $("meta-choose").value = remove ? "" : t.name;
    renderMetaTemplates();
    status(remove ? `Deleted “${t.name}”` : `Saved “${t.name}”`);
  } catch (e) {
    toast(String(e), true);
  }
}

// The first selected photo's metadata into the form.
function metaTemplateFromPhoto() {
  const p = selectedPhotos()[0];
  if (!p) return toast("Select a photo first.", true);
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
