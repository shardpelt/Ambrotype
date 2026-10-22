// SPDX-License-Identifier: GPL-3.0-or-later
// A "?" by each setting: hovering it says in a line what the setting does;
// clicking it opens a card with how it shows in a photo and when to raise or
// lower it. The text comes from the settings' descriptions
// (crates/amb-protocol/data/descriptions.json, "coach" for the longer help).
"use strict";

const help = { tip: null, card: null, timer: null, open: null };

// Called for each setting's row as it is built.
function addHelpMark(entry, def) {
  const label = entry?.label;
  if (!label || !def?.description || label.querySelector(".help-mark")) return;
  const mark = Object.assign(document.createElement("span"), { className: "help-mark", textContent: "?", tabIndex: 0 });
  mark.setAttribute("role", "button");
  mark.setAttribute("aria-label", `About ${def.label}`);
  mark.dataset.setting = def.name;
  // The label is a drag and double-click target; the mark is only for help.
  for (const t of ["pointerdown", "mousedown", "dblclick"]) mark.addEventListener(t, (e) => e.stopPropagation());
  mark.addEventListener("click", (e) => {
    e.preventDefault();
    e.stopPropagation();
    toggleHelpCard(mark, def);
  });
  mark.addEventListener("keydown", (e) => {
    if (e.key !== "Enter" && e.key !== " ") return;
    e.preventDefault();
    e.stopPropagation();
    toggleHelpCard(mark, def);
  });
  mark.addEventListener("mouseenter", () => {
    // The label's own tooltip says the same; hold it back while here.
    if (label.title) {
      label.dataset.title = label.title;
      label.removeAttribute("title");
    }
    clearTimeout(help.timer);
    if (help.open === mark) return;
    help.timer = setTimeout(() => showHelpTip(mark, def), 200);
  });
  mark.addEventListener("mouseleave", () => {
    hideHelpTip();
    if (label.dataset.title) {
      label.title = label.dataset.title;
      delete label.dataset.title;
    }
  });
  label.append(mark);
}

// Beside the mark's column, so the other settings stay in view; inside the
// window.
function placeBeside(el, mark) {
  const r = mark.getBoundingClientRect();
  const col = (mark.closest(".column, aside, .panel") ?? mark).getBoundingClientRect();
  const w = el.offsetWidth, h = el.offsetHeight;
  let left = col.right + 8;
  if (left + w > innerWidth - 8) left = col.left - w - 8;
  if (left < 8) left = r.left - w - 8;
  const top = Math.min(Math.max(8, r.top - 6), innerHeight - h - 8);
  Object.assign(el.style, { left: `${Math.max(8, left)}px`, top: `${top}px` });
}

function showHelpTip(mark, def) {
  help.tip ??= Object.assign(document.createElement("div"), { className: "help-tip", role: "tooltip" });
  help.tip.textContent = def.description;
  const lit = ZONE_OF[def.name] && !$("preview").hidden ? "Lit up on your photo. " : "";
  help.tip.append(Object.assign(document.createElement("small"), { textContent: `${lit}Click for more` }));
  document.body.append(help.tip);
  placeBeside(help.tip, mark);
}

function hideHelpTip() {
  clearTimeout(help.timer);
  help.tip?.remove();
}

function toggleHelpCard(mark, def) {
  hideHelpTip();
  const again = help.open === mark;
  closeHelpCard();
  if (again) return;
  const card = Object.assign(document.createElement("div"), { className: "help-card", role: "dialog" });
  card.setAttribute("aria-label", def.label);
  const add = (tag, text, className = "") => card.appendChild(Object.assign(document.createElement(tag), { textContent: text, className }));
  // Paragraphs with their photography terms linked (basics.js).
  const para = (text) => card.appendChild(document.createElement("p")).append(linkTerms(text, def.label));
  add("h4", def.label);
  para(def.description);
  const c = def.coach;
  if (c) {
    add("h5", "In a photo");
    para(c.looks);
    if (c.more) {
      add("h5", def.kind === "number" ? "Raise it when" : "Use it when");
      para(c.more);
    }
    if (c.less) {
      add("h5", "Lower it when");
      para(c.less);
    }
  }
  const facts = [];
  if (def.kind === "number" && def.min != null && def.max != null) facts.push(`${format(def, def.min)} to ${format(def, def.max)}`);
  if (def.default != null) facts.push(`starts at ${def.kind === "number" ? format(def, def.default) : def.default}`);
  if (def.units) facts.push(def.units);
  if (facts.length) add("p", facts.join(" · "), "help-facts");
  if (def.kind === "number") add("p", "Double-click the label to reset it.", "help-facts");
  const more = add("p", "", "help-facts");
  const link = Object.assign(document.createElement("span"), { className: "term", textContent: "Photography basics", tabIndex: 0 });
  link.dataset.term = "workflow";
  more.append("New to editing? ", link);
  document.body.append(card);
  placeBeside(card, mark);
  help.card = card;
  help.open = mark;
  mark.classList.add("open");
}

function closeHelpCard() {
  help.card?.remove();
  help.open?.classList.remove("open");
  help.card = help.open = null;
}

// A click elsewhere, Esc, scrolling a panel or a new layout closes the card.
function wireHelpCards() {
  window.addEventListener("pointerdown", (e) => {
    if (help.card && !help.card.contains(e.target) && !e.target.closest(".help-mark")) closeHelpCard();
  }, true);
  window.addEventListener("keydown", (e) => {
    if (e.key === "Escape" && help.card) {
      e.stopPropagation();
      closeHelpCard();
    }
  }, true);
  window.addEventListener("scroll", () => { closeHelpCard(); hideHelpTip(); }, true);
}
