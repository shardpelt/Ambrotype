// SPDX-License-Identifier: GPL-3.0-or-later
// Duplicates and bursts (library → Organise): copies of the same file,
// alike pictures and bursts among the photos shown (or selected), grouped
// with the best first (amb_library::duplicates). Each group can be stacked
// with the best on top, or the best picked and the rest rejected; the
// photo kept can be changed by clicking another.
"use strict";

const dupes = { groups: [], scope: [] };
const DUPE_KINDS = { same: "Same file", similar: "Alike", burst: "Burst" };

function dupesScope() {
  const sel = selectedPhotos();
  return sel.length > 1 ? sel : lib.shown;
}

function openDuplicates() {
  $("lib-organise-menu").open = false;
  dupes.scope = dupesScope().map((p) => p.path);
  $("dupes-scope").textContent = `in ${dupes.scope.length === lib.shown.length ? "all" : "the"} ${dupes.scope.length} ${selectedPhotos().length > 1 ? "selected" : "shown"} photos`;
  $("dupes-status").textContent = "";
  dupes.groups = [];
  renderDupes();
  $("dupes-dialog").showModal();
}

async function findDuplicates() {
  const find = $("dupes-find");
  find.disabled = true;
  $("dupes-status").textContent = "Looking…";
  try {
    dupes.groups = await invoke("library_find_duplicates", {
      paths: dupes.scope,
      burstSeconds: Number($("dupes-burst").value) || 0,
      maxBits: Number($("dupes-alike").value),
    });
    const n = dupes.groups.reduce((k, g) => k + g.photos.length - 1, 0);
    $("dupes-status").textContent = dupes.groups.length
      ? `${dupes.groups.length} group${dupes.groups.length === 1 ? "" : "s"}: ${n} photo${n === 1 ? "" : "s"} you may not need`
      : "Nothing alike found.";
  } catch (e) {
    $("dupes-status").textContent = String(e);
  } finally {
    find.disabled = false;
  }
  renderDupes();
}

function renderDupes() {
  const list = $("dupes-list");
  list.replaceChildren(...dupes.groups.map((g, gi) => {
    const row = Object.assign(document.createElement("div"), { className: "dupes-group" });
    const head = Object.assign(document.createElement("div"), { className: "dupes-head" });
    head.append(
      Object.assign(document.createElement("span"), { className: `dupes-kind ${g.kind}`, textContent: DUPE_KINDS[g.kind] }),
      Object.assign(document.createElement("span"), { className: "hint", textContent: `${g.photos.length} photos${g.done ? ` · ${g.done}` : ""}` }),
      Object.assign(document.createElement("span"), { className: "grow" }),
    );
    const action = (text, title, fn) => {
      const b = Object.assign(document.createElement("button"), { type: "button", className: "small", textContent: text, title });
      b.addEventListener("click", fn);
      head.append(b);
    };
    action("Show", "Select these in the library", () => {
      lib.selected = new Set(g.photos);
      lib.focus = g.photos[0];
      $("dupes-dialog").close();
      renderLibrary();
      revealPhoto(g.photos[0], "center");
    });
    action("Stack", "Into one stack, the ★ one on top", () => dupesStack([gi]));
    action("Reject the rest", "Pick the ★ one, reject the others", () => dupesReject([gi]));
    const strip = Object.assign(document.createElement("div"), { className: "dupes-photos" });
    g.photos.forEach((path, i) => {
      const p = libPhoto(path);
      const fig = Object.assign(document.createElement("figure"), { className: "dupes-photo", title: `${p?.name ?? fileName(path)}${p?.captured ? `\n${p.captured}` : ""}${i ? "\nClick to keep this one instead" : ""}` });
      fig.classList.toggle("best", i === 0);
      fig.classList.toggle("rejected", p?.flag === "reject");
      const img = Object.assign(document.createElement("img"), { alt: p?.name ?? "" });
      if (p) bigImage(p, 256, p.edited).then((u) => { img.src = u; }).catch(() => {});
      fig.append(img, Object.assign(document.createElement("figcaption"), { textContent: `${i === 0 ? "★ " : ""}${p?.name ?? fileName(path)}` }));
      fig.addEventListener("click", () => {
        g.photos.splice(i, 1);
        g.photos.unshift(path);
        renderDupes();
      });
      fig.addEventListener("dblclick", () => {
        $("dupes-dialog").close();
        libraryOpen(path);
      });
      strip.append(fig);
    });
    row.append(head, strip);
    return row;
  }));
  $("dupes-stack-all").disabled = $("dupes-reject-all").disabled = !dupes.groups.length;
}

async function dupesStack(indices) {
  for (const gi of indices) {
    const g = dupes.groups[gi];
    // A group already one stack keeps its id.
    const id = libPhoto(g.photos[0])?.stack?.id ?? `s${Date.now().toString(36)}${gi}`;
    await setStacks([[g.photos[0]], g.photos.slice(1)], id);
    g.done = "stacked";
  }
  status(`Stacked ${indices.length} group${indices.length === 1 ? "" : "s"}`);
  renderLibrary();
  renderDupes();
}

async function dupesReject(indices) {
  const keep = indices.map((gi) => dupes.groups[gi].photos[0]);
  const rest = indices.flatMap((gi) => dupes.groups[gi].photos.slice(1));
  for (const [paths, flag] of [[keep, "pick"], [rest, "reject"]]) {
    try {
      const updates = await invoke("library_set_info", { paths, change: { flag } });
      for (const u of updates) {
        const p = libPhoto(u.path);
        if (p) p.flag = u.info.flag ?? null;
      }
    } catch (e) {
      toast(String(e), true);
      return;
    }
  }
  for (const gi of indices) dupes.groups[gi].done = "best picked, rest rejected";
  status(`Picked ${keep.length}, rejected ${rest.length} (Filter → Rejected shows them; X and U change a flag)`);
  renderLibrary();
  renderDupes();
}

function wireDuplicates() {
  $("lib-duplicates").addEventListener("click", openDuplicates);
  $("dupes-find").addEventListener("click", findDuplicates);
  $("dupes-close").addEventListener("click", () => $("dupes-dialog").close());
  $("dupes-stack-all").addEventListener("click", () => dupesStack(dupes.groups.map((_, i) => i)));
  $("dupes-reject-all").addEventListener("click", () => dupesReject(dupes.groups.map((_, i) => i)));
  tauri.event.listen("duplicates-progress", (e) => {
    const [done, total] = e.payload;
    if ($("dupes-dialog").open && total) $("dupes-status").textContent = `Looking at the pictures… ${done} of ${total}`;
  });
}
