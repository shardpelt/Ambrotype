// SPDX-License-Identifier: GPL-3.0-or-later
// The library's quieter chrome: the Import and Filter menus in its bar, and
// folding away the Folders panel and the right panel (the sidebar buttons, or F7 and
// F8 as in the editor), remembered on this computer.
"use strict";

const libFolded = (() => {
  return store.get("amb.lib.folded") ?? {};
})();

function applyLibraryFolds() {
  const body = document.querySelector(".library-body");
  body.classList.toggle("tree-off", Boolean(libFolded.left));
  body.classList.toggle("panel-off", Boolean(libFolded.right));
  $("library-tree").hidden = Boolean(libFolded.left);
  $("library-panel").hidden = Boolean(libFolded.right);
  for (const side of ["left", "right"]) {
    const open = !libFolded[side];
    const b = $(`lib-fold-${side}`);
    b.innerHTML = sidebarIcon(side, open);
    b.title = `${open ? "Hide" : "Show"} the ${side === "left" ? "folders" : "panel"} (Ctrl+${side === "left" ? "[" : "]"})`;
  }
}

function toggleLibrarySide(side) {
  libFolded[side] = !libFolded[side];
  store.set("amb.lib.folded", libFolded);
  applyLibraryFolds();
}

// "Filter" says how many filters are on, so a short grid isn't a surprise.
function updateFilterSummary() {
  const on = [
    $("lib-filter-rating").value !== "0",
    $("lib-filter-label").value !== "all",
    $("lib-filter-flag").value !== "all",
    $("lib-filter-edited").value !== "all",
    $("lib-filter-file").value !== "all",
  ].filter(Boolean).length;
  $("lib-filter-summary").textContent = on ? `Filter · ${on} ▾` : "Filter ▾";
  $("lib-filter-menu").classList.toggle("active", on > 0);
}

function wireLibraryMenus() {
  for (const side of ["left", "right"]) $(`lib-fold-${side}`).addEventListener("click", () => toggleLibrarySide(side));
  applyLibraryFolds();
  const menus = [...document.querySelectorAll(".lib-menu")];
  // A click elsewhere, or Escape, closes them; choosing an import closes its menu.
  document.addEventListener("click", (e) => {
    for (const m of menus) if (m.open && !m.contains(e.target)) m.open = false;
  });
  // Escape closes an open menu, and does nothing else (not leave the library).
  document.addEventListener("keydown", (e) => {
    if (e.key !== "Escape" || !menus.some((m) => m.open)) return;
    for (const m of menus) m.open = false;
    e.preventDefault();
    e.stopPropagation();
  }, true);
  for (const b of $("lib-import-menu").querySelectorAll("button")) b.addEventListener("click", () => ($("lib-import-menu").open = false));
  for (const id of ["lib-filter-rating", "lib-filter-label", "lib-filter-flag", "lib-filter-edited", "lib-filter-file"]) $(id).addEventListener("change", updateFilterSummary);
  updateFilterSummary();
}
