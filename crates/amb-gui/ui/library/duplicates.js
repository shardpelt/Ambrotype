// SPDX-License-Identifier: GPL-3.0-or-later
// Duplicates and bursts (library → Organise): copies of the same file,
// alike pictures and bursts among the photos shown (or selected), grouped
// with the best first (amb_library::duplicates). Each group can be stacked
// with the best on top, or the best picked and the rest rejected; the
// photo kept can be changed by clicking another.
"use strict";

const dupes = { groups: [], scope: [] };
const DUPE_KINDS = { same: tKey("Same file"), similar: tKey("Alike"), burst: tKey("Burst") };

function dupesScope() {
  const sel = selectedPhotos();
  return sel.length > 1 ? sel : lib.shown;
}

function openDuplicates() {
  $("lib-organise-menu").open = false;
  dupes.scope = dupesScope().map((p) => p.path);
  const n = dupes.scope.length;
  const all = n === lib.shown.length;
  $("dupes-scope").textContent = selectedPhotos().length > 1
    ? all ? t("in all {n} selected photos", { n }) : t("in the {n} selected photos", { n })
    : all ? t("in all {n} shown photos", { n }) : t("in the {n} shown photos", { n });
  $("dupes-status").textContent = "";
  dupes.groups = [];
  renderDupes();
  $("dupes-dialog").showModal();
}

async function findDuplicates() {
  $("dupes-status").textContent = t("Looking…");
  await withBusy($("dupes-find"), $("dupes-status"), async () => {
    dupes.groups = await invoke("library_find_duplicates", {
      paths: dupes.scope,
      burstSeconds: Number($("dupes-burst").value) || 0,
      maxBits: Number($("dupes-alike").value),
    });
    const n = dupes.groups.reduce((k, g) => k + g.photos.length - 1, 0);
    $("dupes-status").textContent = dupes.groups.length
      ? t("{groups}: {photos} you may not need", { groups: plural(dupes.groups.length, "group"), photos: plural(n, "photo") })
      : t("Nothing alike found.");
  });
  renderDupes();
}

function renderDupes() {
  const list = $("dupes-list");
  list.replaceChildren(...dupes.groups.map((g, gi) => {
    const row = el("div", { class: "dupes-group" });
    const head = el("div", { class: "dupes-head" });
    head.append(
      el("span", { class: `dupes-kind ${g.kind}` }, t(DUPE_KINDS[g.kind])),
      el("span", { class: "hint" }, g.done
        ? t("{n} photos · {done}", { n: g.photos.length, done: t(g.done) })
        : t("{n} photos", { n: g.photos.length })),
      el("span", { class: "grow" }),
    );
    const action = (text, title, fn) => {
      const b = el("button", { type: "button", class: "small", title: title }, text);
      b.addEventListener("click", fn);
      head.append(b);
    };
    action(t("Show"), t("Select these in the library"), () => {
      lib.selected = new Set(g.photos);
      lib.focus = g.photos[0];
      $("dupes-dialog").close();
      renderLibrary();
      revealPhoto(g.photos[0], "center");
    });
    action(t("Stack"), t("Into one stack, the ★ one on top"), () => dupesStack([gi]));
    action(t("Reject the rest"), t("Pick the ★ one, reject the others"), () => dupesReject([gi]));
    const strip = el("div", { class: "dupes-photos" });
    const dupeCell = ({ path }) => {
      const i = g.photos.indexOf(path);
      const p = libPhoto(path);
      const name = p?.name ?? fileName(path);
      const fig = el("figure", { class: "dupes-photo", title: [name, p?.captured, i ? t("Click to keep this one instead") : null].filter(Boolean).join("\n") },
        el("img", { alt: p?.name ?? "" }),
        el("figcaption", { translate: "no" }, `${i === 0 ? "★ " : ""}${name}`));
      fig.classList.toggle("best", i === 0);
      fig.classList.toggle("rejected", p?.flag === "reject");
      fig.addEventListener("dblclick", () => {
        $("dupes-dialog").close();
        libraryOpen(path);
      });
      return fig;
    };
    photoRow(strip, {
      cell: dupeCell,
      thumb: ({ path }, fig) => {
        const p = libPhoto(path);
        if (p) bigImage(p, 256, p.edited).then((u) => { fig.querySelector("img").src = u; }).catch(() => {});
      },
      open: ({ path }) => {
        g.photos.splice(g.photos.indexOf(path), 1);
        g.photos.unshift(path);
        renderDupes();
      },
    }).set(g.photos.map((path) => ({ path })));
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
    g.done = tKey("stacked");
  }
  status(t("Stacked {groups}", { groups: plural(indices.length, "group") }));
  renderLibrary();
  renderDupes();
}

async function dupesReject(indices) {
  const keep = indices.map((gi) => dupes.groups[gi].photos[0]);
  const rest = indices.flatMap((gi) => dupes.groups[gi].photos.slice(1));
  for (const [paths, flag] of [[keep, "pick"], [rest, "reject"]]) {
    try {
      const updates = await setInfo(paths, { flag });
      for (const u of updates) {
        photoChanged(u.path, { flag: u.info.flag ?? null });
      }
    } catch (e) {
      showError(e);
      return;
    }
  }
  for (const gi of indices) dupes.groups[gi].done = tKey("best picked, rest rejected");
  status(t("Picked {picked}, rejected {rejected} (Filter → Rejected shows them; X and U change a flag)", { picked: keep.length, rejected: rest.length }));
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
    if ($("dupes-dialog").open && total) $("dupes-status").textContent = t("Looking at the pictures… {done} of {total}", { done, total });
  });
}
