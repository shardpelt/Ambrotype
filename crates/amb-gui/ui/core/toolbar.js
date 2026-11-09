// SPDX-License-Identifier: GPL-3.0-or-later
// The toolbar on a narrow window (a 13" laptop): the buttons that don't fit
// are hidden from the end, and ⋯ lists them. The buttons themselves stay in
// the toolbar (layout.js builds it), so shortcuts, tips and editing the
// layout still find them; the menu's entries click them.
"use strict";

const tbar = { scheduled: false };

function toolbarBar() {
  return document.querySelector(".topbar .actions");
}

// Hide from the end what doesn't fit, and show ⋯ when anything is hidden.
function fitToolbar() {
  tbar.scheduled = false;
  const bar = toolbarBar();
  const more = $("btn-toolbar-more");
  if (!bar || !more) return;
  const kids = [...bar.children];
  for (const k of kids) k.classList.remove("overflowed");
  more.hidden = true;
  // Editing the layout needs every button in place.
  if (typeof ui !== "undefined" && ui.edit) return;
  if (bar.scrollWidth <= bar.clientWidth + 1) return;
  more.hidden = false;
  const shown = kids.filter((k) => !k.hidden);
  while (shown.length && bar.scrollWidth > bar.clientWidth + 1) shown.pop().classList.add("overflowed");
  // No separator left at the end.
  while (shown.length && shown.at(-1).classList.contains("sep")) shown.pop().classList.add("overflowed");
}

function scheduleFitToolbar() {
  if (tbar.scheduled) return;
  tbar.scheduled = true;
  requestAnimationFrame(fitToolbar);
}

// The buttons hidden for want of room, in their order.
function overflowedButtons() {
  return [...(toolbarBar()?.children ?? [])].filter((b) => b.classList.contains("overflowed") && b.tagName === "BUTTON" && !b.hidden);
}

function closeToolbarMenu() {
  $("toolbar-menu").hidden = true;
  $("btn-toolbar-more").setAttribute("aria-expanded", "false");
}

function openToolbarMenu() {
  const menu = $("toolbar-menu");
  const entries = overflowedButtons().map((b) => {
    // Its picture (or room for one) and its name, so the list lines up.
    const item = Object.assign(document.createElement("button"), { type: "button", disabled: b.disabled });
    item.setAttribute("role", "menuitem");
    const svg = b.querySelector("svg.tool-svg");
    item.append(svg ? svg.cloneNode(true) : Object.assign(document.createElement("span"), { className: "menu-icon-space" }));
    const name = (b.dataset.label || b.getAttribute("aria-label") || b.textContent || "").trim();
    item.append(document.createTextNode(name));
    const tip = b.title || b.dataset.tip;
    if (tip) item.title = tip;
    item.addEventListener("click", () => {
      closeToolbarMenu();
      b.click();
    });
    return item;
  });
  menu.replaceChildren(...entries);
  const r = $("btn-toolbar-more").getBoundingClientRect();
  menu.hidden = false;
  menu.style.top = `${r.bottom + 4}px`;
  menu.style.left = `${Math.max(8, Math.min(r.left, window.innerWidth - menu.offsetWidth - 8))}px`;
  $("btn-toolbar-more").setAttribute("aria-expanded", "true");
  entries.find((e) => !e.disabled)?.focus();
}

function wireToolbarOverflow() {
  const more = $("btn-toolbar-more");
  more.addEventListener("click", (e) => {
    e.stopPropagation();
    if ($("toolbar-menu").hidden) openToolbarMenu();
    else closeToolbarMenu();
  });
  document.addEventListener("pointerdown", (e) => {
    if (!$("toolbar-menu").hidden && !e.target.closest?.("#toolbar-menu, #btn-toolbar-more")) closeToolbarMenu();
  });
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape" && !$("toolbar-menu").hidden) {
      e.preventDefault();
      closeToolbarMenu();
      more.focus();
    }
  }, true);
  // Again whenever the window, the toolbar's buttons or their labels change.
  const resized = new ResizeObserver(scheduleFitToolbar);
  resized.observe(document.querySelector(".topbar"));
  resized.observe(toolbarBar());
  // Buttons shown or hidden by a task's layout count too (not the class
  // this sets, or it would chase itself).
  new MutationObserver(scheduleFitToolbar).observe(toolbarBar(), { childList: true, subtree: true, characterData: true, attributes: true, attributeFilter: ["hidden"] });
  document.fonts?.ready.then(scheduleFitToolbar);
  scheduleFitToolbar();
}

document.addEventListener("DOMContentLoaded", wireToolbarOverflow);
