// SPDX-License-Identifier: GPL-3.0-or-later
// Toolbar icons: simple line drawings (our own), drawn in the text colour,
// for the buttons in TOOLBAR_ICONS. Drawings for the others are kept here in
// case a layout wants them later.
// The button keeps its name for screen readers (aria-label) and its tooltip.
"use strict";

const ICON_PATHS = {
  // A folder, opened.
  open: '<path d="M3 18V6.5A1.5 1.5 0 0 1 4.5 5h4l2 2h7A1.5 1.5 0 0 1 19 8.5V10"/><path d="M3 18l2.6-6.6A1.5 1.5 0 0 1 7 10.5h13.4a1 1 0 0 1 .9 1.4L18.8 18a1.5 1.5 0 0 1-1.4 1H4.4A1.4 1.4 0 0 1 3 18Z"/>',
  // A stack of prints: the photos you have.
  library: '<rect x="8" y="3" width="13" height="10" rx="1"/><rect class="cover" x="5.5" y="5.5" width="13" height="10" rx="1"/><rect class="cover" x="3" y="8" width="13" height="10" rx="1"/><path d="m4.8 16.2 3.4-3.4 2.6 2.6 1.6-1.6 2 2"/><circle cx="12.8" cy="11" r="1"/>',
  // A floppy disk.
  save: '<path d="M5 3.5h11.5l4 4V19a1.5 1.5 0 0 1-1.5 1.5H5A1.5 1.5 0 0 1 3.5 19V5A1.5 1.5 0 0 1 5 3.5Z"/><path d="M7.5 3.5v5h8v-5"/><rect x="7" y="13" width="10" height="7.5" rx=".5"/>',
  // Down into a tray: a file comes out of the app.
  export: '<path d="M12 3v11"/><path d="M7.5 9.5 12 14l4.5-4.5"/><path d="M4 14.5v4A1.5 1.5 0 0 0 5.5 20h13a1.5 1.5 0 0 0 1.5-1.5v-4"/>',
  // Two photos laid on top of each other at angles, as on a collage.
  collage: '<g transform="rotate(-10 8.5 8.5)"><rect x="2.5" y="3.5" width="12" height="10" rx=".8"/><path d="m4.5 11.5 3-3 2.5 2.5"/></g><g transform="rotate(9 15 15.5)"><rect class="cover" x="8.8" y="10.5" width="12.4" height="10" rx=".8"/><circle cx="17.8" cy="13.8" r="1.2"/><path d="m10.8 18.5 3-3 3.5 3.5"/></g>',
  undo: '<path d="M9 14 4 9l5-5"/><path d="M4 9h10.5a5.5 5.5 0 0 1 0 11H10"/>',
  redo: '<path d="m15 14 5-5-5-5"/><path d="M20 9H9.5a5.5 5.5 0 0 0 0 11H14"/>',
  // Half before, half after.
  before: '<rect x="3.5" y="4.5" width="17" height="15" rx="1.5"/><path d="M12 2.5v19"/><path d="M12 4.5h7a1.5 1.5 0 0 1 1.5 1.5v12a1.5 1.5 0 0 1-1.5 1.5h-7Z" fill="currentColor" stroke="none" opacity=".45"/>',
  // A selection with handles at the corners.
  crop: '<rect x="5" y="5" width="14" height="14" stroke-dasharray="2.6 2.2"/><rect x="3" y="3" width="4" height="4" fill="currentColor" stroke="none"/><rect x="17" y="3" width="4" height="4" fill="currentColor" stroke="none"/><rect x="3" y="17" width="4" height="4" fill="currentColor" stroke="none"/><rect x="17" y="17" width="4" height="4" fill="currentColor" stroke="none"/>',
  // A masquerade mask.
  masks: '<path d="M2.5 8.5C2.5 6.8 4.5 6 7 6c2.2 0 3.4 1 5 1s2.8-1 5-1c2.5 0 4.5.8 4.5 2.5 0 4.6-2.4 8-5.2 8-2.1 0-2.6-2.2-4.3-2.2s-2.2 2.2-4.3 2.2c-2.8 0-5.2-3.4-5.2-8Z"/><ellipse cx="7.8" cy="10.3" rx="2.1" ry="1.4"/><ellipse cx="16.2" cy="10.3" rx="2.1" ry="1.4"/>',
  // A plaster: healing.
  spots: '<rect x="2.6" y="8.2" width="18.8" height="7.6" rx="3.8" transform="rotate(-45 12 12)"/><rect x="8.6" y="8.6" width="6.8" height="6.8" transform="rotate(-45 12 12)"/><circle cx="11" cy="12" r=".6" fill="currentColor"/><circle cx="13" cy="12" r=".6" fill="currentColor"/><circle cx="12" cy="11" r=".6" fill="currentColor"/><circle cx="12" cy="13" r=".6" fill="currentColor"/>',
  // A magnifier.
  zoom: '<circle cx="10.5" cy="10.5" r="6.5"/><path d="m15.5 15.5 5 5"/><path d="M10.5 7.5v6M7.5 10.5h6"/>',
  // An eyedropper: a long rubber bulb, a crossbar wider than the tube, a
  // tapered glass tube and a drop falling from its tip.
  wb: '<rect x="15" y="1.6" width="4.4" height="8.4" rx="2.2" transform="rotate(45 17.2 5.8)" fill="currentColor" stroke="none"/><path d="m11.4 7.6 4.9 4.9" stroke-width="2"/><path d="M12.6 10.6 6.2 17 5.4 19.4l2.4-.8 6.4-6.4"/><path d="M4.4 22.2a1.1 1.1 0 0 1-2.2 0c0-.6 1.1-1.9 1.1-1.9s1.1 1.3 1.1 1.9Z" fill="currentColor" stroke="none" transform="translate(0 -.5)"/>',
  // A warning triangle.
  clip: '<path d="M12 3.5 21.5 20h-19L12 3.5Z"/><path d="M12 10v4.5"/><circle cx="12" cy="17.3" r=".8" fill="currentColor" stroke="none"/>',
  // A border: a thick frame around an empty middle.
  frame: '<path fill="currentColor" stroke="none" fill-rule="evenodd" d="M3 3h18v18H3Zm4 4v8.5h10V7Z"/>',
  // A keyboard.
  keys: '<rect x="2.5" y="6" width="19" height="12" rx="1.5"/><path d="M6 9.5h.01M9 9.5h.01M12 9.5h.01M15 9.5h.01M18 9.5h.01M6 12h.01M9 12h.01M12 12h.01M15 12h.01M18 12h.01M8 15h8"/>',
  // Building blocks, one coming loose: add-ons (as code editors show them).
  plugins: '<rect x="3.5" y="7.5" width="6" height="6" rx=".8"/><rect x="3.5" y="14.5" width="6" height="6" rx=".8"/><rect x="10.5" y="14.5" width="6" height="6" rx=".8"/><rect x="13.5" y="3" width="6" height="6" rx=".8" transform="rotate(15 16.5 6)"/>',
  // A painter's palette.
  looks: '<path d="M12 3a9 9 0 1 0 0 18c1.3 0 2-.8 2-1.8 0-.5-.2-.9-.5-1.3-.3-.4-.5-.8-.5-1.3 0-1 .8-1.6 1.8-1.6H17a4 4 0 0 0 4-4C21 6.6 17 3 12 3Z"/><circle cx="7.5" cy="11" r="1.2" fill="currentColor"/><circle cx="10" cy="7" r="1.2" fill="currentColor"/><circle cx="14.5" cy="7" r="1.2" fill="currentColor"/><circle cx="17.2" cy="10.5" r="1.2" fill="currentColor"/>',
  // The window's panels, with a move arrow: rearrange the layout.
  edit: '<rect x="2.5" y="4" width="19" height="16" rx="1.5"/><path d="M8 4v16"/><path d="M15 8.5v7M11.5 12h7"/><path d="m13.6 10 1.4-1.5 1.4 1.5M13.6 14l1.4 1.5 1.4-1.5M13.5 10.6 12 12l1.5 1.4M16.5 10.6 18 12l-1.5 1.4"/>',
  // A cog.
  settings: '<circle cx="12" cy="12" r="3"/><path d="M12 2.5v3M12 18.5v3M2.5 12h3M18.5 12h3M5.3 5.3l2.1 2.1M16.6 16.6l2.1 2.1M5.3 18.7l2.1-2.1M16.6 7.4l2.1-2.1"/><circle cx="12" cy="12" r="6.5"/>',
  // A speech bubble.
  feedback: '<path d="M4.5 5h15A1.5 1.5 0 0 1 21 6.5v9a1.5 1.5 0 0 1-1.5 1.5H10l-4.5 3.5V17h-1A1.5 1.5 0 0 1 3 15.5v-9A1.5 1.5 0 0 1 4.5 5Z"/><path d="M7.5 9.5h9M7.5 12.5h6"/>',
};

// Only the buttons whose icon says what it does at a glance get one; the
// rest keep their word, so the bar stays easy to read.
// Spots, Clipping and Appearance have no picture that says it.
const TOOLBAR_ICONS = new Set([
  "open", "library", "save", "export", "collage", "undo", "redo", "before", "crop", "masks", "zoom", "wb",
  "frame", "keys", "plugins", "edit", "settings", "feedback",
]);

function iconSvg(name) {
  const paths = ICON_PATHS[name];
  if (!paths) return null;
  return `<svg class="tool-svg" viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${paths}</svg>`;
}

// Show the toolbar as icons (where TOOLBAR_ICONS has one) or, for the
// layouts that ask ("toolbar_style": "words"), as words only. The name
// stays as the label for screen readers and in the tooltip.
function applyToolbarIcons(style = "icons") {
  for (const [name, id] of Object.entries(TOOLBAR_IDS)) {
    const b = $(id);
    if (!b) continue;
    if (b.dataset.label == null) b.dataset.label = b.textContent.trim();
    const label = b.dataset.label || b.getAttribute("aria-label") || name;
    b.setAttribute("aria-label", label.replace(/…$/, ""));
    if (!b.title) b.title = label;
    const svg = style !== "words" && TOOLBAR_ICONS.has(name) ? iconSvg(name) : null;
    if (svg) b.innerHTML = svg;
    else b.textContent = label;
    b.classList.toggle("tool-icon", Boolean(svg));
  }
}

// The toolbar's tooltips, shown sooner than the system's (which waits about a
// second): the button's title, moved aside while the pointer is on it so
// the system one doesn't show too.
function wireQuickTips() {
  const tip = Object.assign(document.createElement("div"), { className: "quick-tip", role: "tooltip", hidden: true });
  document.body.append(tip);
  let timer = null;
  let over = null;
  const hide = () => {
    clearTimeout(timer);
    tip.hidden = true;
    if (over?.dataset.tip != null) {
      if (!over.title) over.title = over.dataset.tip;
      delete over.dataset.tip;
    }
    over = null;
  };
  const bar = document.querySelector(".topbar .actions");
  bar.addEventListener("mouseover", (e) => {
    const b = e.target.closest("button");
    if (b === over) return;
    hide();
    if (!b?.title) return;
    over = b;
    b.dataset.tip = b.title;
    b.removeAttribute("title");
    timer = setTimeout(() => {
      tip.textContent = b.dataset.tip;
      tip.hidden = false;
      const r = b.getBoundingClientRect();
      const left = Math.min(Math.max(4, r.left + r.width / 2 - tip.offsetWidth / 2), innerWidth - tip.offsetWidth - 4);
      Object.assign(tip.style, { left: `${left}px`, top: `${r.bottom + 6}px` });
    }, 250);
  });
  bar.addEventListener("mouseleave", hide);
  bar.addEventListener("mousedown", hide);
}
