// SPDX-License-Identifier: GPL-3.0-or-later
// Clean up (library → Organise): the photos shown (or selected) that may
// go, each with why (amb_ops::cleanup: copies and alike pictures, the
// weaker frames of bursts, blurry, eyes closed, nearly black or blown out,
// rejected). One window pages through them, all marked to go; Keep (K)
// or Delete (D) each, then they go to the trash together, or are rejected.
"use strict";

const clean = { scope: [], found: [], at: 0, keep: new Set(), row: null }; // row: the photoRow under the photo, made by wireCleanup
const CLEAN_TESTS = [
  ["dupes", tKey("Copies and alike pictures")],
  ["bursts", tKey("Weaker frames of bursts")],
  ["blurry", tKey("Blurry")],
  ["eyes", tKey("Eyes closed")],
  ["exposure", tKey("Nearly black or blown out")],
  ["rejected", tKey("Already rejected")],
];
const CLEAN_WHY = {
  same: tKey("a copy of the same file"),
  alike: tKey("alike a sharper photo"),
  burst: tKey("not the best of its burst"),
  blurry: tKey("blurry"),
  eyes: tKey("eyes closed"),
  dark: tKey("nearly black"),
  bright: tKey("blown out"),
  rejected: tKey("rejected"),
};

function openCleanup() {
  if (!libraryOn()) return;
  $("lib-organise-menu").open = false;
  const sel = selectedPhotos();
  clean.scope = (sel.length > 1 ? sel : lib.shown).map((p) => p.path);
  if (!clean.scope.length) return toast(t("Open a folder first."), true);
  clean.found = [];
  const n = clean.scope.length;
  $("clean-scope").textContent = sel.length > 1
    ? t("Looks through the {n} selected photos. Photos you picked or rated are left out.", { n })
    : n === 1
      ? t("Looks through the {n} shown photo. Photos you picked or rated are left out.", { n })
      : t("Looks through the {n} shown photos. Photos you picked or rated are left out.", { n });
  $("clean-tests").replaceChildren(...CLEAN_TESTS.map(([id, label]) => {
    const l = el("label");
    const box = el("input", { type: "checkbox", checked: true, value: id });
    l.append(box, ` ${t(label)}`);
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
  $("clean-status").textContent = t("Looking…");
  const ok = await withBusy($("clean-find"), $("clean-status"), async () => {
    clean.found = await invoke("library_cleanup", { paths: clean.scope, skip });
  });
  if (!ok) return;
  if (!clean.found.length) {
    $("clean-status").textContent = t("Nothing to clean up here.");
    return;
  }
  clean.at = 0;
  clean.keep = new Set();
  cleanStage("review");
  renderCleanStrip();
  showCleanPhoto();
}

function cleanWhy(r) {
  if (r.kind === "burst" && r.note) return t("not the best of its burst ({note})", { note: r.note });
  return CLEAN_WHY[r.kind] ? t(CLEAN_WHY[r.kind]) : r.kind;
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
  $("clean-name").textContent = t("{n} of {total} · {name}", { n: clean.at + 1, total: clean.found.length, name: libPhoto(s.path)?.name ?? fileName(s.path) });
  $("clean-why").replaceChildren(...s.reasons.map((r) => el("span", { class: `clean-chip ${r.kind}` }, cleanWhy(r))));
  const keep = s.reasons.find((r) => r.keep)?.keep;
  $("clean-instead").hidden = !keep;
  if (keep) {
    cleanImage($("clean-instead-img"), keep, 256);
    $("clean-instead-name").textContent = libPhoto(keep)?.name ?? fileName(keep);
  }
  $("clean-review").classList.toggle("kept", kept);
  $("clean-mark").textContent = kept ? t("Kept") : t("Goes to the trash");
  $("clean-prev").disabled = clean.at === 0;
  $("clean-next").disabled = clean.at === clean.found.length - 1;
  for (const [path, fig] of clean.row.cells) fig.classList.toggle("kept", clean.keep.has(path));
  clean.row.mark();
  clean.row.reveal(clean.found[clean.at]?.path);
  const going = clean.found.length - clean.keep.size;
  $("clean-trash").textContent = t("Move {n} to the trash", { n: going });
  $("clean-trash").disabled = $("clean-reject").disabled = !going;
  $("clean-count").textContent = t("{n} of {total} marked to go", { n: going, total: clean.found.length });
}

function renderCleanStrip() {
  clean.row.set(clean.found);
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
  const ask = n === 1
    ? t("Move {photos} to the trash, with its sidecars, JPEG twins and virtual copies?\n\nThe trash can give them back.", { photos: plural(n, "photo") })
    : t("Move {photos} to the trash, with their sidecars, JPEG twins and virtual copies?\n\nThe trash can give them back.", { photos: plural(n, "photo") });
  if (!(await askYes(ask))) return;
  try {
    const [files, closed] = await invoke("files_trash", { paths });
    if (closed) photoGone();
    lib.selected.clear();
    $("clean-dialog").close();
    status(t("Moved {photos} ({files} files) to the trash", { photos: plural(n, "photo"), files }));
    await reloadLibraryView();
  } catch (e) {
    showError(e);
  }
}

async function cleanReject() {
  const paths = cleanGoing();
  try {
    for (const u of await setInfo(paths, { flag: "reject" })) {
      photoChanged(u.path, { flag: u.info.flag ?? null });
    }
  } catch (e) {
    return showError(e);
  }
  $("clean-dialog").close();
  status(t("Rejected {photos} (Filter → Rejected shows them; Delete… sends them to the trash)", { photos: plural(paths.length, "photo") }));
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
  clean.row = photoRow($("clean-strip"), {
    cell: (s) => el("figure", { class: "clean-thumb", title: s.reasons.map(cleanWhy).join(", ") }, el("img")),
    thumb: (s, fig) => cleanImage(fig.firstChild, s.path, 192),
    open: (s) => {
      clean.at = clean.found.indexOf(s);
      showCleanPhoto();
    },
    isCurrent: (s) => s === clean.found[clean.at],
  });
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
    if ($("clean-dialog").open && total) $("clean-status").textContent = t("Looking at the pictures… {done} of {total}", { done, total });
  });
}
