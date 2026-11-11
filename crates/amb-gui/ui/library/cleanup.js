// SPDX-License-Identifier: GPL-3.0-or-later
// Clean up (library → Organise): the photos shown (or selected) that may
// go, each with why (amb_ops::cleanup: copies and alike pictures, the
// weaker frames of bursts, blurry, eyes closed, nearly black or blown out,
// rejected). One window pages through them, all marked to go; Keep (K)
// or Delete (D) each, then they go to the trash together, or are rejected.
"use strict";

const clean = { scope: [], found: [], at: 0, keep: new Set() };
const CLEAN_TESTS = [
  ["dupes", "Copies and alike pictures"],
  ["bursts", "Weaker frames of bursts"],
  ["blurry", "Blurry"],
  ["eyes", "Eyes closed"],
  ["exposure", "Nearly black or blown out"],
  ["rejected", "Already rejected"],
];
const CLEAN_WHY = {
  same: "a copy of the same file",
  alike: "alike a sharper photo",
  burst: "not the best of its burst",
  blurry: "blurry",
  eyes: "eyes closed",
  dark: "nearly black",
  bright: "blown out",
  rejected: "rejected",
};

function openCleanup() {
  if (!libraryOn()) return;
  $("lib-organise-menu").open = false;
  const sel = selectedPhotos();
  clean.scope = (sel.length > 1 ? sel : lib.shown).map((p) => p.path);
  if (!clean.scope.length) return toast("Open a folder first.", true);
  clean.found = [];
  $("clean-scope").textContent = `Looks through the ${clean.scope.length} ${sel.length > 1 ? "selected" : "shown"} photo${clean.scope.length === 1 ? "" : "s"}. Photos you picked or rated are left out.`;
  $("clean-tests").replaceChildren(...CLEAN_TESTS.map(([id, label]) => {
    const l = document.createElement("label");
    const box = Object.assign(document.createElement("input"), { type: "checkbox", checked: true, value: id });
    l.append(box, ` ${label}`);
    return l;
  }));
  $("clean-status").textContent = "";
  cleanStage("setup");
  $("clean-dialog").showModal();
}

function cleanStage(stage) {
  $("clean-setup").hidden = stage !== "setup";
  $("clean-review").hidden = stage !== "review";
  $("clean-trash").hidden = $("clean-reject").hidden = stage !== "review";
}

async function findCleanup() {
  const skip = [...$("clean-tests").querySelectorAll("input:not(:checked)")].map((b) => b.value);
  $("clean-find").disabled = true;
  $("clean-status").textContent = "Looking…";
  try {
    clean.found = await invoke("library_cleanup", { paths: clean.scope, skip });
  } catch (e) {
    $("clean-status").textContent = String(e);
    return;
  } finally {
    $("clean-find").disabled = false;
  }
  if (!clean.found.length) {
    $("clean-status").textContent = "Nothing to clean up here.";
    return;
  }
  clean.at = 0;
  clean.keep = new Set();
  cleanStage("review");
  renderCleanStrip();
  showCleanPhoto();
}

function cleanWhy(r) {
  return r.kind === "burst" && r.note ? `${CLEAN_WHY.burst} (${r.note})` : CLEAN_WHY[r.kind] ?? r.kind;
}

function cleanImage(img, path, edge) {
  const p = libPhoto(path);
  img.removeAttribute("src");
  img.alt = p?.name ?? fileName(path);
  if (p) bigImage(p, edge, p.edited).then((u) => { img.src = u; }).catch(() => {});
}

function showCleanPhoto() {
  const s = clean.found[clean.at];
  if (!s) return;
  const kept = clean.keep.has(s.path);
  cleanImage($("clean-img"), s.path, 1024);
  $("clean-name").textContent = `${clean.at + 1} of ${clean.found.length} · ${libPhoto(s.path)?.name ?? fileName(s.path)}`;
  $("clean-why").replaceChildren(...s.reasons.map((r) => Object.assign(document.createElement("span"), { className: `clean-chip ${r.kind}`, textContent: cleanWhy(r) })));
  const keep = s.reasons.find((r) => r.keep)?.keep;
  $("clean-instead").hidden = !keep;
  if (keep) {
    cleanImage($("clean-instead-img"), keep, 256);
    $("clean-instead-name").textContent = libPhoto(keep)?.name ?? fileName(keep);
  }
  $("clean-review").classList.toggle("kept", kept);
  $("clean-mark").textContent = kept ? "Kept" : "Goes to the trash";
  $("clean-prev").disabled = clean.at === 0;
  $("clean-next").disabled = clean.at === clean.found.length - 1;
  for (const [i, fig] of [...$("clean-strip").children].entries()) {
    fig.classList.toggle("at", i === clean.at);
    fig.classList.toggle("kept", clean.keep.has(clean.found[i].path));
  }
  $("clean-strip").children[clean.at]?.scrollIntoView({ block: "nearest", inline: "nearest" });
  const going = clean.found.length - clean.keep.size;
  $("clean-trash").textContent = `Move ${going} to the trash`;
  $("clean-trash").disabled = $("clean-reject").disabled = !going;
  $("clean-count").textContent = `${going} of ${clean.found.length} marked to go`;
}

function renderCleanStrip() {
  $("clean-strip").replaceChildren(...clean.found.map((s, i) => {
    const fig = Object.assign(document.createElement("figure"), { className: "clean-thumb", title: s.reasons.map(cleanWhy).join(", ") });
    const img = document.createElement("img");
    cleanImage(img, s.path, 192);
    fig.append(img);
    fig.addEventListener("click", () => { clean.at = i; showCleanPhoto(); });
    return fig;
  }));
}

function cleanMark(keep) {
  const s = clean.found[clean.at];
  if (!s) return;
  if (keep) clean.keep.add(s.path);
  else clean.keep.delete(s.path);
  if (clean.at < clean.found.length - 1) clean.at++;
  showCleanPhoto();
}

function cleanStep(d) {
  clean.at = Math.max(0, Math.min(clean.found.length - 1, clean.at + d));
  showCleanPhoto();
}

function cleanGoing() {
  return clean.found.map((s) => s.path).filter((p) => !clean.keep.has(p));
}

async function cleanTrash() {
  const paths = cleanGoing();
  if (!paths.length) return;
  const n = paths.length;
  if (!(await askYes(`Move ${n} photo${n === 1 ? "" : "s"} to the trash, with ${n === 1 ? "its" : "their"} sidecars, JPEG twins and virtual copies?\n\nThe trash can give them back.`))) return;
  try {
    const [files, closed] = await invoke("files_trash", { paths });
    if (closed) photoGone();
    lib.selected.clear();
    $("clean-dialog").close();
    status(`Moved ${n} photo${n === 1 ? "" : "s"} (${files} files) to the trash`);
    await reloadLibraryView();
  } catch (e) {
    toast(String(e), true);
  }
}

async function cleanReject() {
  const paths = cleanGoing();
  try {
    for (const u of await setInfo(paths, { flag: "reject" })) {
      const p = libPhoto(u.path);
      if (p) p.flag = u.info.flag ?? null;
    }
  } catch (e) {
    return toast(String(e), true);
  }
  $("clean-dialog").close();
  status(`Rejected ${paths.length} photo${paths.length === 1 ? "" : "s"} (Filter → Rejected shows them; Delete… sends them to the trash)`);
  renderLibrary();
}

function cleanKey(e) {
  if ($("clean-review").hidden || e.ctrlKey || e.metaKey || e.altKey) return;
  const k = e.key.toLowerCase();
  const act = {
    arrowleft: () => cleanStep(-1),
    arrowright: () => cleanStep(1),
    k: () => cleanMark(true),
    d: () => cleanMark(false),
    delete: () => cleanMark(false),
    backspace: () => cleanMark(false),
  }[k];
  if (!act) return;
  e.preventDefault();
  e.stopPropagation();
  act();
}

function wireCleanup() {
  $("lib-cleanup").addEventListener("click", openCleanup);
  $("clean-find").addEventListener("click", findCleanup);
  $("clean-close").addEventListener("click", () => $("clean-dialog").close());
  $("clean-keep").addEventListener("click", () => cleanMark(true));
  $("clean-delete").addEventListener("click", () => cleanMark(false));
  $("clean-prev").addEventListener("click", () => cleanStep(-1));
  $("clean-next").addEventListener("click", () => cleanStep(1));
  $("clean-trash").addEventListener("click", cleanTrash);
  $("clean-reject").addEventListener("click", cleanReject);
  $("clean-instead").addEventListener("click", () => {
    const keep = clean.found[clean.at]?.reasons.find((r) => r.keep)?.keep;
    const i = clean.found.findIndex((s) => s.path === keep);
    if (i >= 0) { clean.at = i; showCleanPhoto(); }
  });
  $("clean-dialog").addEventListener("keydown", cleanKey);
  tauri.event.listen("cleanup-progress", (e) => {
    const [done, total] = e.payload;
    if ($("clean-dialog").open && total) $("clean-status").textContent = `Looking at the pictures… ${done} of ${total}`;
  });
}
