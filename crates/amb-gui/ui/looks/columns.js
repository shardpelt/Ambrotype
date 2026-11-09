// SPDX-License-Identifier: GPL-3.0-or-later
// The side columns: their widths, folding them away, the top bar, the chrome
// and the column tabs. Split out of layout.js.
"use strict";

// A side column can be folded away (the arrow on the photo's edge, or
// Ctrl+[ and Ctrl+]) and back; remembered on this computer.
const folded = (() => {
  return store.get("folded-columns") ?? {};
})();

// A side panel (Preset library, Frame) takes the left column's place while
// it's open, so the photo doesn't shrink.
const sidePanelOpen = () => [...document.querySelectorAll(".viewer > .preset-library")].some((p) => !p.hidden);

// Column widths dragged outside editor mode, per layout, on this computer:
// a layout's own width is only where it starts.
const colWidths = (() => {
  return store.get("amb.colWidths") ?? {};
})();
const COL_MIN = 140, COL_MAX = 800;

function saveColWidths() {
  store.set("amb.colWidths", colWidths);
}

// A handle on each shown column's inner edge: drag to resize, double-click
// for the layout's own width.
function addColumnGrips(shown) {
  const ws = $("workspace");
  ws.querySelectorAll(".col-resizer").forEach((n) => n.remove());
  if (ui.edit) return;
  for (const side of ["left", "right"]) {
    if (!shown[side]) continue;
    const grip = Object.assign(document.createElement("div"), { className: "col-resizer", title: "Drag to resize; double-click for the layout's width" });
    grip.style[side] = `${shown[side] - 4}px`;
    grip.addEventListener("pointerdown", (ev) => dragColumn(ev, side, grip, shown));
    grip.addEventListener("dblclick", () => {
      delete colWidths[ui.looks?.active_layout]?.[side];
      saveColWidths();
      ui.sideWidths[side] = ui.layoutWidths?.[side] ?? ui.sideWidths[side];
      applySideCollapse();
    });
    ws.append(grip);
  }
}

function dragColumn(ev, side, grip, shown) {
  if (ev.button !== 0) return;
  ev.preventDefault();
  const ws = $("workspace");
  const r = ws.getBoundingClientRect();
  let w = shown[side];
  grip.classList.add("dragging");
  const move = (e) => {
    w = Math.round(Math.min(COL_MAX, Math.max(COL_MIN, side === "left" ? e.clientX - r.left : r.right - e.clientX)));
    const cols = { ...shown, [side]: w };
    ws.style.gridTemplateColumns = `${cols.left}px minmax(0, 1fr) ${cols.right}px`;
    grip.style[side] = `${w - 4}px`;
  };
  const up = () => {
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", up);
    grip.classList.remove("dragging");
    const id = ui.looks?.active_layout;
    if (id) {
      colWidths[id] = { ...colWidths[id], [side]: w };
      saveColWidths();
    }
    ui.sideWidths[side] = w;
    applySideCollapse();
  };
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", up);
}

// The fold buttons' mark: a window with that side's column, filled while
// it shows. (Arrows read as previous / next photo.)
function sidebarIcon(side, open) {
  const x = side === "left" ? 3.5 : 14.5;
  return `<svg viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.4" aria-hidden="true"><rect x="2" y="3.5" width="16" height="13" rx="2"/><rect x="${x}" y="5" width="2" height="10" rx=".5" fill="${open ? "currentColor" : "none"}" stroke="${open ? "none" : "currentColor"}"/></svg>`;
}

function applySideCollapse() {
  const widths = ui.sideWidths ?? { left: 0, right: 0 };
  if (chromeHidden()) {
    // Over the photo, out of sight until the pointer reaches that edge.
    for (const side of ["left", "right"]) {
      const col = $(`col-${side}`);
      col.hidden = !widths[side];
      col.style.width = `${widths[side]}px`;
      $(`fold-${side}`).hidden = true;
      $(`edge-${side}`).hidden = !widths[side];
    }
    $("workspace").style.gridTemplateColumns = "0px minmax(0, 1fr) 0px";
    addColumnGrips({ left: 0, right: 0 });
    return;
  }
  for (const side of ["left", "right"]) {
    $(`col-${side}`).style.width = "";
    $(`edge-${side}`).hidden = true;
  }
  const shown = {};
  for (const side of ["left", "right"]) {
    shown[side] = folded[side] || (side === "left" && sidePanelOpen()) ? 0 : widths[side];
    $(`col-${side}`).hidden = !shown[side];
    const t = $(`fold-${side}`);
    t.hidden = !widths[side];
    const open = Boolean(shown[side]);
    t.innerHTML = sidebarIcon(side, open);
    t.title = `${open ? "Hide" : "Show"} the ${side} column (Ctrl+${side === "left" ? "[" : "]"})`;
  }
  $("workspace").style.gridTemplateColumns = `${shown.left}px minmax(0, 1fr) ${shown.right}px`;
  addColumnGrips(shown);
}

// Section heights in a column, set by dragging the line between two
// sections; remembered on this computer. The presets list fills what's left.
const sectionHeights = (() => {
  return store.get("section-heights") ?? {};
})();

function addSectionDividers(col) {
  const sections = [...col.children].filter((el) => el.classList.contains("side-section"));
  for (const s of sections) {
    const h = sectionHeights[s.id];
    s.style.height = h && !s.classList.contains("presets-block") ? `${h}px` : "";
    s.classList.toggle("sized", Boolean(s.style.height));
  }
  for (let i = 1; i < sections.length; i++) {
    const above = sections[i - 1];
    const below = sections[i];
    const grip = Object.assign(document.createElement("div"), { className: "section-resizer", title: "Drag to resize; double-click to reset" });
    below.before(grip);
    // Resize the section that doesn't fill the spare height.
    const target = below.classList.contains("presets-block") ? above : below;
    const sign = target === above ? 1 : -1;
    grip.addEventListener("pointerdown", (e) => {
      if (document.body.classList.contains("editing")) return;
      e.preventDefault();
      grip.setPointerCapture(e.pointerId);
      const y0 = e.clientY;
      const h0 = target.getBoundingClientRect().height;
      const move = (ev) => {
        const h = Math.max(40, Math.round(h0 + sign * (ev.clientY - y0)));
        target.style.height = `${h}px`;
        target.classList.add("sized");
        sectionHeights[target.id] = h;
      };
      const up = () => {
        grip.removeEventListener("pointermove", move);
        store.set("section-heights", sectionHeights);
      };
      grip.addEventListener("pointermove", move);
      grip.addEventListener("pointerup", up, { once: true });
    });
    grip.addEventListener("dblclick", () => {
      delete sectionHeights[target.id];
      target.style.height = "";
      target.classList.remove("sized");
      store.set("section-heights", sectionHeights);
    });
  }
}

function toggleColumn(side) {
  // In the library the same keys fold its folders and panel.
  if (libraryOn()) return toggleLibrarySide(side);
  if (!ui.sideWidths?.[side]) return;
  // With the chrome hidden the keys slide the column in and out.
  if (chromeHidden()) return peekColumn(side, !document.body.classList.contains(`peek-${side}`));
  folded[side] = !folded[side];
  store.set("folded-columns", folded);
  applySideCollapse();
  requestPreview(false);
}

// The top bar can be hidden for more room (Settings, Ctrl+Shift+T); a thin
// strip at the window's top edge then shows it while the pointer is there.
function setTopbar(show) {
  document.body.classList.toggle("topbar-off", !show);
  document.body.classList.remove("topbar-peek");
  $("topbar-edge").hidden = show;
  const pref = $("settings-topbar");
  if (pref) pref.checked = show;
  store.setText("amb.topbar", show ? "1" : "0");
  requestPreview(false);
  return true;
}

function wireTopbar() {
  let shown = true;
  shown = store.text("amb.topbar") !== "0";
  const bar = document.querySelector(".topbar");
  let leave = 0;
  $("topbar-edge").addEventListener("mouseenter", () => {
    clearTimeout(leave);
    document.body.classList.add("topbar-peek");
  });
  bar.addEventListener("mouseenter", () => clearTimeout(leave));
  bar.addEventListener("mouseleave", () => {
    // Not while one of its menus is open.
    if (bar.querySelector("details[open]")) return;
    leave = setTimeout(() => document.body.classList.remove("topbar-peek"), 400);
  });
  $("settings-topbar")?.addEventListener("change", (e) => setTopbar(e.target.checked));
  if (!shown) setTopbar(false);
  else if ($("settings-topbar")) $("settings-topbar").checked = true;
}

// A layout with `"chrome": "hidden"` (Minimalism) shows only the photo: no
// top bar, status bar or filmstrip, and its side columns slide in over the
// photo while the pointer is at the window's left or right edge (or with
// Ctrl+[ and Ctrl+]). Shortcuts all work. Not while editing the layout.
const chromeHidden = () => ui.layout?.chrome === "hidden" && !ui.edit;
const peekTimers = {};

function applyChrome() {
  const on = chromeHidden();
  const was = document.body.classList.contains("chrome-hidden");
  document.body.classList.toggle("chrome-hidden", on);
  if (on && !was) {
    document.body.classList.add("topbar-off");
    $("topbar-edge").hidden = false;
  } else if (!on && was) {
    document.body.classList.remove("peek-left", "peek-right");
    let shown = true;
    shown = store.text("amb.topbar") !== "0";
    document.body.classList.toggle("topbar-off", !shown);
    $("topbar-edge").hidden = shown;
  }
}

function peekColumn(side, show) {
  clearTimeout(peekTimers[side]);
  document.body.classList.toggle(`peek-${side}`, show);
  return true;
}

function wireChrome() {
  for (const side of ["left", "right"]) {
    $(`edge-${side}`).addEventListener("mouseenter", () => peekColumn(side, true));
    const col = $(`col-${side}`);
    col.addEventListener("mouseenter", () => clearTimeout(peekTimers[side]));
    col.addEventListener("mouseleave", () => {
      if (!chromeHidden()) return;
      // Not while a slider is held (dragging past the edge), or once the
      // pointer is back. (Panels are <details> too, so open ones don't count.)
      const later = () => {
        if (ui.active || col.matches(":hover")) {
          peekTimers[side] = setTimeout(later, 400);
        } else {
          document.body.classList.remove(`peek-${side}`);
        }
      };
      peekTimers[side] = setTimeout(later, 500);
    });
  }
}

// The chosen tab of a column, per layout, on this computer.
function columnTab(side) {
  try { return JSON.parse(localStorage.getItem("amb.tabs") ?? "{}")[`${ui.looks?.active_layout}:${side}`] ?? null; } catch { return null; }
}

function showColumnTab(side, name) {
  const col = $(`col-${side}`);
  const tabs = [...col.querySelectorAll(":scope > .column-tabs .column-tab")];
  if (!tabs.length) return;
  if (!tabs.some((b) => b.dataset.tab === name)) name = tabs[0].dataset.tab;
  for (const b of tabs) {
    b.classList.toggle("on", b.dataset.tab === name);
    b.setAttribute("aria-selected", b.dataset.tab === name);
  }
  // Editor mode shows every tab's panels, to move them between tabs.
  for (const el of col.querySelectorAll(":scope > [data-tab]")) el.classList.toggle("tab-off", !ui.edit && el.dataset.tab !== name);
  try {
    const all = JSON.parse(localStorage.getItem("amb.tabs") ?? "{}");
    all[`${ui.looks?.active_layout}:${side}`] = name;
    localStorage.setItem("amb.tabs", JSON.stringify(all));
  } catch { /* not kept */ }
  requestAnimationFrame(redrawCanvases);
}

// Bring the tab holding `el` forward (search, help, a shortcut to a slider).
function revealInTab(el) {
  const holder = el?.closest(".column > [data-tab]");
  if (holder?.classList.contains("tab-off")) showColumnTab(holder.parentElement.id.replace("col-", ""), holder.dataset.tab);
}
